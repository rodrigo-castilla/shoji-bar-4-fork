# How shoji-bar-4 works: a reading guide

A path through the code, in the order things happen, for understanding the whole project. Paths are
relative to the repository root. Line counts are approximate and drift; the file names are what
matters.

## 0. The big picture

```
Hyprland (compositor)  <--- Wayland layer-shell --->  shoji-bar-4 (one process)
                                                          |
   Omarchy scripts ---- omarchy-shell socket ------------>|  answers media / OSD / notifications / menus
   hyprctl, keys ---- Hyprland IPC and events ----------->|
                                                          v
                          AntiWebUI toolkit: signals + a tree of divs -> layout -> wgpu -> a surface per layer
```

- One process draws everything: the **bar**, the **dock**, the **panels**, the **notification
  popups**, the **OSD**, the **lock screen** and the **wallpaper**. Each is a Wayland *layer* (a
  layer-shell surface), not a window.
- The UI is written in Rust with **AntiWebUI**, an external toolkit (a git dependency, patched by
  this fork, see `patches/`). It is "web-like": you build a tree of `div()`s with style methods
  (`.flex()`, `.p(12.0)`, `.rounded(20.0)`), and **signals** drive updates. There is no HTML, and no
  retained widget classes: a view is a function that returns a `Div`.
- The toolkit lays the tree out (flexbox), paints it with **wgpu** (GPU) and presents each layer's
  surface to the compositor.

## 1. The three crates, and `plugins/`

| Where | What it is | Read it for |
|---|---|---|
| `crates/shoji-bar-4` | The **binary**. Tiny. | The entry point and how `plugins/` is wired in |
| `crates/shoji-core` | The **foundation**: settings, theme, panel manager, UI helpers, ShojiWM link, PAM | The vocabulary every screen is built from |
| `crates/shoji-default` | The **stock interface**: bar, dock, panels, notifications, OSD, lock, Omarchy glue | Where almost every feature lives |
| `plugins/` | **This fork's own changes**, compiled into the binary (palettes, Omarchy menu, image picker, tray, music panel...) | How the interface is patched without touching the stock crates |

Dependency direction: `shoji-bar-4` → `shoji-default` → `shoji-core` → AntiWebUI.

## 2. Entry point: from `main` to the first pixel

Read these in order. Each step is short.

1. **`crates/shoji-bar-4/src/main.rs`**: `main()` handles `--version`, then calls `plugin::run()`.
   `--default` (or `SHOJI_SAFE=1`) skips the plugins and runs the stock interface instead.
2. **`crates/shoji-bar-4/build.rs`**: runs at compile time and *generates* `plugin_entry.rs`. If
   `plugins/init-default.rs` exists, it includes it as a module and generates
   `run() { shoji_default::run_with(init_default::patch) }`. If `plugins/init.rs` exists, it replaces
   the whole interface instead (`init()`). With neither, the stock UI. Editing `plugins/` only
   recompiles this small crate.
3. **`crates/shoji-default/src/lib.rs`**: `run_with(patch)`:
   - `subcommand()` first: `shoji-bar-4 msg ...` and `preview` are *client* modes that talk to the
     running shell and exit; they never start a UI.
   - `Ui::stock()` builds the stock interface **as data** (see §3), `patch(&mut ui)` lets
     `plugins/init-default.rs` change it, then `shoji_core::runtime::start(|| ui.install())`.
4. **`crates/shoji-core/src/runtime.rs`**: `start()` does process-wide setup while there is still one
   thread (reading `ui_scale` into the environment), then `Shell::new().namespace("shoji-bar-4")...run(root)`.
   That is AntiWebUI's Wayland event loop; `root` runs once, **on the UI thread**, inside it.
5. **`crates/shoji-default/src/compose.rs`**: `Ui::install()` is the real startup sequence:
   `ipc::serve()` → `theme::install()` → `wm::share_theme()` → notifications popups → OSD, Omarchy
   socket (`omarchy::serve`), keys (`keys::take`), clipboard watchers, shutdown handlers → lock
   screen → register panels → run `on_start` hooks → **for every output**: open the bar, the dock,
   the wallpaper, run `on_output` hooks, install the panels' layers.

If you only read one file to understand startup, read `compose.rs`.

## 3. The stock UI is data: `Ui` and `BarLayout`

`Ui` (`compose.rs`) holds *what* is shown: `services` (notifications, OSD, lock on/off), a
`BarLayout`, a `dock` flag, a `wallpaper` flag, a `Vec<PanelDef>`, and hooks. Because it is data, a
plugin can change it before `install()`:

```rust
// plugins/init-default.rs
pub fn patch(ui: &mut Ui) {
    tray::install(ui);                       // replaces the tray island
    ui.replace_panel(music_player::panel()); // swaps a stock panel for a different one
}
```

`BarLayout` (`bar.rs`) is the same idea for the bar: three lists of `Island`s (left, center, right)
with `remove`, `replace`, `insert_after`...

## 4. How a screen is drawn (the rendering model)

Take the bar. `bar.rs`:

- `open_with(output, layout)` calls `open_layer(LayerOptions::bar(Anchor::TOP, height)
  .namespace("shoji-bar-4-bar"), move || view_with(output, layout))`. That creates **one layer
  surface** anchored to the top, whose content is the closure's return value.
- `view_with` returns a `Div`: a flex row containing each island's view. An island is a function
  `Fn(Output) -> impl IntoView` (`Island::new("clock", clock_island)`).
- Look at any island (e.g. `clock_island`) to see the idioms:
  - **Static content**: `static_txt(TextStyle::BODY_MEDIUM, "text")`.
  - **Reactive content**: `txt(style, || format!(...))` re-runs when the signals it reads change.
  - **Reactive style**: `.dyn_style(|s| s.bg(colors().surface))`: re-evaluated when `colors()`
    (the theme) changes, which is how a palette change repaints everything.
  - **Conditional / lists**: `show(cond, build)`, `for_each(items, key, build)` with `.enter/.exit`
    animations.
  - **State**: `signal(v)`, `memo(|| ...)`, `effect(|| ...)`. Reading a signal inside a memo,
    effect or `dyn_style` subscribes to it.

In outline (the toolkit's own crates have the details), a frame is: run pending effects → relayout dirty subtrees (flexbox) → paint to the
layer's wgpu surface → hand it to Wayland. Animations (`animated`, `Spring`, `transition`) request
frames until they settle, so an idle shell does no work.

Rules that matter (they explain many code comments):
- **Everything UI runs on one thread.** Anything slow (`hyprctl`, file reads, D-Bus waits) must run on
  another thread and come back through `ui_channel(callback)` or `task(...).then(...)`. Blocking the
  UI thread drops frames; this is why so much code spawns a thread and sends the result back.
- **Signals belong to the scope that created them.** One created inside a card or an effect is
  disposed with it; using it later panics. Long-lived state is created in the root (see
  `notify.rs::kb()`), or in a `thread_local!`.

## 5. Panels: the system most features live in

A panel is a window-like thing that opens over the desktop: quick settings, calendar, launcher,
clipboard, media, power dialog, and in this fork the Omarchy menu and the image picker.

- **`crates/shoji-core/src/panels.rs`**: the manager. A panel is a `PanelDef::new(id, content_fn)`
  with builder options: `.place(...)` (where it sits), `.enter(...)` (animation), `.veiled()` (dim
  the desktop), `.with_field()` (a text field gets keyboard focus), `.on_escape(...)`.
  - State is one signal: `Option<(output_name, PanelId)>`. `open()`, `close()`, `toggle()` set it.
  - `install(output)` runs per output: for each registered panel an effect watches "is it open
    here?" and **mounts a full-screen layer** while it opens, is open, and closes (so the exit
    animation can play). A click outside, or Escape, closes it.
- **`crates/shoji-default/src/panels.rs`**: registers the six stock panels (ids `quick-settings`,
  `calendar`, `launcher`, `clipboard`, `media`, `power`). `shoji-bar-4 msg toggle <id>` opens any
  registered panel from outside.
- Each panel's content is its own file: `quick_settings.rs` (+ `qs_pages.rs`, `qs_settings.rs`),
  `calendar.rs`, `launcher.rs`, `clipboard.rs`, `media_panel.rs`, `power_dialog.rs`.

## 6. Theme and settings

- **`crates/shoji-core/src/settings.rs`**: persisted user choices (`settings.json` in
  `$XDG_STATE_HOME/shoji-bar-4/`), exposed as one `Signal<Settings>`.
- **`crates/shoji-core/src/theme.rs`**: builds a Material 3 `ColorScheme` from a seed colour
  (optionally the wallpaper's), tracks dark/light, and publishes it with `set_color_scheme_animated`.
  `colors()` returns the current scheme; every `dyn_style` that reads it repaints when it changes.
  `set_scheme_override(Some(scheme))` replaces the generated scheme: that is how **palettes** work.
- **`plugins/colour.rs` + `plugins/palettes/*.palette`**: this fork's palette files (dark, light,
  red, translucent). Applying one builds a scheme and calls the override. For *translucent* it also
  asks Hyprland for blur behind the layers and flips the ink dark over bright backdrops.
- **`plugins/animation.rs`**: how colours animate and the pane behind the bar.
- **`crates/shoji-core/src/wm.rs`**: sends the scheme to ShojiWM so window frames match (not used on
  Hyprland).

## 7. Talking to the outside

| Channel | Where | Direction |
|---|---|---|
| **Shell socket** `$XDG_RUNTIME_DIR/shoji-bar-4.<display>.sock` | `crates/shoji-default/src/ipc.rs` | `shoji-bar-4 msg ...` (toggle panels, theme, lock, quit, status) |
| **Omarchy's socket** | `omarchy.rs` | Omarchy's `omarchy-shell <target> <method>` calls arrive here instead of at Quickshell: media, `osd.show`, `notifications.*`, `shell toggle/summon`, `image-selector`... what is not answered is forwarded to Quickshell if it runs |
| **Hyprland keybinds** | `keys.rs` | At startup rebinds Omarchy's *global shortcut* keys to command forms (`hyprctl eval`) and rebinds after a config reload; put back on exit |
| **Hyprland events** | `plugins/colour.rs`, `keys.rs` (socket2) | Layer/window/workspace events drive the translucent ink and rebinding |
| **D-Bus** | AntiWebUI's `anti-webui-wayland-shell` crate | Notifications daemon (`notify.rs`), battery, network, bluetooth, tray, audio, media players |
| **Files** | `clipboard.rs`, `~/.local/state/shoji-bar-4/command` | Omarchy's clipboard history; a test command file polled by `colour.rs` |

`crates/shoji-default/src/shutdown.rs` makes sure the keys and sockets are given back on `SIGTERM`
or a panic.

## 8. Other features, each in one place

- **Notifications**: `notify.rs`: `open_popups()` is a top-right layer; `popups()` the card list;
  `card()` one card (buttons, click -> `activate`); keyboard mode (`keyboard_start`, `kb_key`).
- **OSD** (volume / brightness / media / notices pill): `osd.rs`.
- **Dock**: `dock.rs`. **Lock screen**: `lock.rs` + `pam.rs` (core). **Wallpaper**: `wallpaper.rs`.
- **Omarchy menu and prompts**: `plugins/omarchy_menu.rs` (one `PanelDef`, three request kinds).
- **Image picker**: `plugins/image_picker.rs`. **Clipboard**: `clipboard.rs`.
- **Frame meter** (performance): `plugins/meter.rs` + `scripts/perf-ab.sh`.

## 9. A reading workflow (about two hours)

1. **Run it** so you can poke at it: `scripts/apply-antiwebui-patch.sh && cargo build --release`,
   then `target/release/shoji-bar-4`. Try `shoji-bar-4 msg toggle quick-settings` and `msg status`.
2. **The spine** (20 min): `main.rs` → `build.rs` (skim the doc comment) → `lib.rs` (`run_with`) →
   `runtime.rs` → `compose.rs::install`. You now know the startup order.
3. **One island end to end** (20 min): `bar.rs::open_with` → `view_with` → `clock_island`. Make a
   small change (a different text) and rebuild with `cargo build --profile fast` (about 1 s for
   plugin changes). Watch it update. This is the whole rendering idiom.
4. **One panel end to end** (25 min): `shoji-core/src/panels.rs` (`PanelDef`, `install`, the layer
   effect) → `shoji-default/src/panels.rs::stock` → `power_dialog.rs` (a small panel).
5. **Reactivity** (15 min): `shoji-core/src/ui.rs` (helpers: `txt`, `pressable`, `settle_in`) and
   `theme.rs::install` (signals, memo, effect, `task(...).then(...)` all in 70 lines).
6. **Patching** (15 min): `plugins/init-default.rs` → `plugins/menu.rs` (smallest plugin) →
   `docs/customization.md`.
7. **Outside world** (25 min): `ipc.rs` → `omarchy.rs::handle` (the dispatcher) → `keys.rs`.
8. **A real feature** (your pick): trace the notification of a screenshot: `omasnap` sends it over
   D-Bus → AntiWebUI's notification server → `notify.rs::popups` → `card` → click → `activate`.

Debugging tips: `SHOJI_SAFE=1 shoji-bar-4` ignores plugins; `shoji-bar-4 preview ...` renders a part
of the UI to a picture; `plugins/meter.rs` plus `scripts/fps-test.sh` measure frames.

## 10. Where the toolkit is

AntiWebUI is a separate repository (`~/.local/share/shoji-bar-4/AntiWebUI` is the patched copy this
machine builds from; the upstream is `github.com/bea4dev/AntiWebUI`). Useful crates in it:
`anti-webui-core` (tree, layout, signals, layers), `anti-webui-style` (the style methods),
`anti-webui-md3e` (Material 3 colours, motion, components), `anti-webui-wgpu` (the renderer),
`anti-webui-wayland` and `anti-webui-wayland-shell` (surfaces; D-Bus services).
