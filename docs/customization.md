# Customization

The interface of shoji-bar-4 is Rust code, and you can change it with Rust code of your own: no
settings language in between. You put a file in a plugins directory, build, and the shell you start
is the one you described.

This is not hot reloading. The code is compiled into the binary, so a change needs a build and a
restart of the shell. With the `fast` profile that build takes about half a second (see
[Building](#building)).

## The three ways to run it

What the binary shows is decided when it is built, by what the plugins directory holds:

| In the plugins directory | The interface |
|---|---|
| nothing | the stock interface, as shipped |
| `init-default.rs` | the stock interface, changed by your `patch` function |
| `init.rs` | only what your `init` function puts on the screen |

If both files exist, `init.rs` wins and the build prints a warning. A file only counts if it defines its function (`init` or `patch`) outside of comments, so commenting a plugin out entirely brings back the stock interface (with a warning).

The plugins directory is `plugins/` at the root of the project, next to the top-level `Cargo.toml`
(or `$SHOJI_PLUGINS_DIR`, if you set it). The build creates it if it does not exist. It is yours.
In the fork's repository it holds the author's own plugins, as a real example (see
[Example plugins](#example-plugins)); in a project of your own, leave it out of version control.

Whatever the interface, `shoji-bar-4 msg ...` and `shoji-bar-4 preview ...` still work. `preview`
always draws the stock parts.

## Building

```sh
cargo build --release          # what ShojiWM's default config starts: target/release/shoji-bar-4
cargo build --profile fast     # for working on your interface:      target/fast/shoji-bar-4
```

The `fast` profile skips LTO. Measured on one machine: the first `fast` build from scratch took about
80 seconds, and after that, editing a file in the plugins directory rebuilt in about half a second
(the same edit takes about 30 seconds with `--release`). Only the small `shoji-bar-4` binary crate
is compiled again; `shoji-core` and `shoji-default` are not.

Adding, removing or editing any file in the plugins directory makes cargo rebuild. The files are
compiled as modules of the binary, so you can split your code: `mod helpers;` in `init.rs` loads
`helpers.rs` from the same directory.

Your code can use `shoji_core`, `shoji_default`, `anti_webui`, `anti_webui_md3e` and `serde_json`.
For another crate, add it to `crates/shoji-bar-4/Cargo.toml`.

Examples to start from are in `crates/shoji-bar-4/plugin-examples/`; `init-default-commented.rs` changes nothing until you uncomment one of its blocks, one change at a time.

## `init-default.rs`: change the stock interface

Define `pub fn patch(ui: &mut shoji_default::Ui)`. `Ui::stock()` describes what the stock shell
shows, and `patch` edits it before it is put on the screen.

```rust
use anti_webui::prelude::*;
use shoji_default::{Island, Ui};

pub fn patch(ui: &mut Ui) {
    if let Some(bar) = ui.bar.as_mut() {
        bar.height = 34;
        bar.remove("tray");
        bar.insert_after("clock", Island::new("greeting", |_output| div().child(text("hello"))));
    }
    ui.dock = false;
}
```

What `Ui` holds:

| Field or method | What it does |
|---|---|
| `bar: Option<BarLayout>` | the bar; `None` for no bar |
| `dock`, `wallpaper` | whether those layers are opened |
| `services` | `notifications`, `osd` and `lock`: turn one off to draw it yourself, or to go without |
| `panels` | the stock panels, as `PanelDef` values |
| `replace_panel(def)`, `remove_panel(id)` | add or replace a panel by id, or take one away |
| `on_start(f)` | runs once the services are up, before any output is set up |
| `on_output(f)` | runs for every output, after the bar, dock and wallpaper: the place to open layers of your own |

`BarLayout` has a `height` and three groups of islands (`left`, `center`, `right`), and
`remove`, `replace`, `insert_before` and `insert_after` that find an island by id. The stock island
ids are `launcher`, `workspaces`, `window`, `clock`, `media`, `tray` and `status`. The stock islands
are drawn for a height of 38, so go gently with `height`.

The stock panel ids are `quick-settings`, `calendar`, `launcher`, `clipboard`, `media` and `power`.

## `init.rs`: write the interface yourself

Define `pub fn init()`. It runs as the root of the shell, and nothing is on the screen unless it
puts it there. Start from `plugin-examples/init.rs`.

Two things are worth calling, unless you have a reason not to:

- `shoji_default::ipc::serve()` listens for `shoji-bar-4 msg ...`.
- `shoji_core::theme::install()` makes the Material colors (`shoji_core::theme::colors()`) follow the
  settings. Without it there is nothing to read.

Then open layers with AntiWebUI's `open_layer`, for every output with `for_each_output`. The parts
of the stock interface are public modules of `shoji_default` (`bar`, `dock`, `quick_settings`...), so
you can reuse them, or `shoji_default::Ui::stock()` and its pieces. They are the stock interface's
own parts, not a stable API: expect them to change with it.

From here on, panels, colors and animations are yours, and so is keeping them working.

## Panels

A panel is a `PanelDef` registered with `shoji_core::panels::register`, before
`shoji_core::panels::install(output)` runs for the outputs:

```rust
use shoji_core::panels::{self, PanelDef};

panels::register(
    PanelDef::new("my-panel", |_output| div().p(24.0_f32).child(text("hello")))
        .place(|root| root.items_center().justify_center())   // where it goes on the output
        .veiled(),                                            // blur the desktop behind it
);

panels::toggle(&output, "my-panel");   // from a button; or: shoji-bar-4 msg toggle my-panel
```

| Method | What it does |
|---|---|
| `PanelDef::new(id, content)` | `content` builds the panel for the output it opens on |
| `.place(f)` | aligns the panel in a column that covers the whole output, the bar included |
| `.enter(f)` | the style it comes in from; see `panels::from_above` and `panels::rise_in` |
| `.with_field()` | the panel has a search field that takes the keyboard focus |
| `.veiled()` | veils and blurs the desktop around it |
| `.on_escape(f)` | return `true` when Escape was used inside the panel, `false` to close it |

At most one panel is open on an output. A click outside the panel closes it, and so does Escape.
Registering an id again replaces the earlier definition.

## Hooks for plugins

Things the stock interface leaves open for a plugin to fill:

| Hook | What it is for |
|---|---|
| `BarLayout.background: Option<Rc<dyn Fn(Output) -> Div>>` | something drawn behind all the islands, across the bar (a pane, a glass) |
| `shoji_core::theme::set_scheme_override(Some(scheme))` | puts a colour scheme of your own in place of the one the settings make; `None` takes it off |
| `shoji_core::theme::set_scheme_fade(spring)` | the motion a change of colours goes through |
| `shoji_default::quick_settings::set_palettes_page(f)` | the page the "Dark theme" tile's chevron opens |
| `quick_settings::set_dark_tile_text(f)`, `set_dark_tile_click(f)` | what the "Dark theme" tile says (title, second line, on/off) and what a click on it does |
| `shoji_default::qs_pages::{header, item, scrolling, subheading}` | the pieces of the quick settings pages, to build a page like the stock ones |

## Example plugins

The `plugins/` directory of the fork is a working set, one module per concern, loaded by
`init-default.rs`:

- `colour.rs`: reads the palettes in `plugins/palettes/*.palette` (`key = value` lines: a seed, a
  variant, colour roles, a `blur` radius for glass palettes), keeps which one is in use and whether
  the bar has its pane, and applies the compositor glass. A command file
  (`~/.local/state/shoji-bar-4/command`: `palette <name>`, `bar on|off`) lets scripts drive it.
- `animation.rs`: how colours move (one fade of about 0.6 s between any two palettes) and the pane
  behind the bar, which spreads from the middle island to the sides.
- `menu.rs`: the theme page in quick settings, with the palette in use ticked.
- `tray.rs`, `agents.rs`, `music-player.rs`: a tray drop-down, an agents panel and a music panel.

## Version

`shoji-bar-4 --version` (also `-V` and `version`) prints `shoji-bar-4 <version>`, read from the
workspace manifest.

## When your code does not work

- **It does not build**: `cargo build` fails and leaves the binary you had.
- **It builds but does not start, or draws nonsense**: run `shoji-bar-4 --default`, or set
  `SHOJI_SAFE=1`, to ignore the plugins and get the stock interface from the same binary. Nothing
  starts it for you if it crashes.
- **To go back for good**: delete `init.rs` and `init-default.rs` and build again.

## Size of the interface

`ui_scale` in `~/.local/state/shoji-bar-4/settings.json` (1.0 normal, 0.85 smaller) sizes the whole
interface, whichever one it is. It is read as the shell starts, and it needs AntiWebUI with a small
patch: see [UI zoom](development.md#ui-zoom).

## Performance

Release builds use fat LTO, one codegen unit and `panic = "abort"`. Frame drops during animations
usually come from the integrated GPU and CPU running at a low clock (the shell, the compositor and
a video wallpaper share one iGPU): `scripts/performance.sh on` switches the power profile to
`performance` and holds the iGPU clock at its maximum; `off` undoes it.

### Measuring frames

`plugins/meter.rs` is a frame meter that is off until asked for. `scripts/fps-test.sh [SECONDS]`
turns it on, switches the bar and the palettes, sweeps the opacity, leaves SECONDS for you to open
and close panels, and writes `fps.txt` (a summary for each step), `fps.csv` (every frame) and
`system.csv` (CPU and iGPU clocks) to `logs/<date>/`. While it measures, the shell is asked for a
frame all the time, so the numbers are the most it can draw, not what it draws when idle.
