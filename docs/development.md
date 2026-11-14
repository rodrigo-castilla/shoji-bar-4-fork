# Development

## Layout of the repository

The project is a cargo workspace of two crates:

- `crates/shoji-bar-4-core`: a library with everything the shell is made of. The components (the bar, the dock, the panels, the lock screen, the notifications, the OSD) and the machinery under them (settings, theme, the ShojiWM link, the shell layers). `run(root)` starts the shell: it handles the `msg` and `preview` subcommands and otherwise connects to the compositor and runs `root` on the UI thread.
- `crates/shoji-bar-4`: the binary. Its `main.rs` is the root: it says what is opened, and for each output. How the pieces are arranged is decided here, not in the core.

The binary is still `target/release/shoji-bar-4`, at the root of the workspace.

## AntiWebUI

AntiWebUI comes from GitHub's `main`, and `Cargo.lock` pins the commit used. To move to a newer AntiWebUI:

```sh
cargo update -p anti-webui -p anti-webui-md3e
```

To work on a local AntiWebUI checkout at the same time, patch it in from an uncommitted `.cargo/config.toml` (it is in `.gitignore`):

```toml
[patch."https://github.com/bea4dev/AntiWebUI.git"]
anti-webui = { path = "/path/to/AntiWebUI/src/anti-webui" }
anti-webui-md3e = { path = "/path/to/AntiWebUI/src/anti-webui-md3e" }
```

The `dynamic_linking` feature links AntiWebUI dynamically, for faster rebuilds while developing.

## Offscreen previews

Any part of the shell can be drawn to a PNG without a compositor surface, with a simulated pointer and clicks:

```sh
shoji-bar-4 preview dock out.png --pointer 470,110
shoji-bar-4 preview quick-settings out.png --width 460 --height 900 --scale 1 --light
shoji-bar-4 preview workspaces out.png --ms 0 --frames 40 --every 30   # out-0.png … out-40.png
shoji-bar-4 preview quick-settings out.png --click 1548,80 --click-frames 1 --ms 0 --frames 8 --every 40
shoji-bar-4 preview bar left.png --bar-position left
shoji-bar-4 preview quick-settings right.png --bar-position right --scale 1
```

- **Parts**: `bar`, `dock`, `workspaces`, `title`, `quick-settings`, `calendar`, `launcher`, `clipboard`, `media`, `power`, `lock` and `notifications`.
  - In `lock`, the password is `shoji`, and PAM is not used.
  - `title` is the window island with a made-up window whose title goes between a long one and a short one every `--switch` ms (80 by default), from 1.2 s for a second and a half, and then stays on the one `--end` names (`long` or `short`). With `--windows`, the two titles belong to two windows that take the keyboard in turn. With `--spinner`, the title is a terminal's with a spinner in front, as Claude Code's is, which at the end changes to another title.
- **Settings**: `preview` doesn't save them. `--bar-position top|left|right` selects the bar edge (top by default), including for panel and component previews.
- **Real services**: the previews act on them.
  - A click on a network, Bluetooth or audio row really changes it.
  - Enter and Delete in `clipboard` act on the real cliphist and clipboard.
- **`--click X,Y`**: a click.
- **`--right-click X,Y`**: a right click.
- **`--drag X1,Y1:X2,Y2`**: after the clicks, presses at the first point, moves to the second and lets go.
- **`--key K`**: after the clicks, presses a key (`ArrowDown`, `ArrowUp`, `Enter`, `Delete`, `Escape`), or types K if it is a character. It can be given more than once.
- **`--click-frames N`**: how many frames to run after a click (20 by default). Make it small to catch an animation a click starts from its beginning.

## Layer lifecycle tests

`crates/shoji-bar-4-core/src/shell_layer.rs` owns shell-surface creation, output binding, placement-driven reconfiguration and closing. Production uses its Wayland adapter; unit tests use the recording adapter through the same `open_bar` and `install_panels` interface. This covers multi-output assignment, unrelated-setting isolation, in-place placement updates and panel input changes without a compositor.

A bar reserves its thickness from its first commit, and moves to another edge with one `set_geometry`, which changes the edge, both sizes and the reserved zone in one commit. AntiWebUI's own layer protocol tests cover what that sends.

```sh
cargo test --locked -p shoji-bar-4-core shell_layer::tests
```

## CPU reports

When the shell keeps using CPU:

```sh
scripts/cpu-report.sh 10               # look for 10 seconds now and write a report
scripts/cpu-report.sh watch 8 30       # stay running; write a report when CPU is over 8% twice in a row
scripts/cpu-report.sh --sudo watch     # also record perf's time per function (only perf runs under sudo)
```

Reports are written to `~/.local/state/shoji-bar-4/reports/`. They cover:

- **CPU and wakeups per thread**: a UI thread waking 60 to 120 times a second is drawing frames all the time.
- **`msg status`**: whether a clock keeps ticking (`ticking`), and whether the UI thread answers quickly.
- **perf**: the functions taking the time. Release builds keep function names for this.
- **D-Bus**: whether a flood of signals is coming in.

The report ends with what these suggest.

## What was added to AntiWebUI for this shell

All of it went in as general features, and is in AntiWebUI's `main`.

- **Layer background**: `WindowOptions::background`; layers are transparent by default.
- **md3e dynamic color**, ported from material-color-utilities:
  - `ColorScheme::from_seed` / `from_seed_with`, with 9 variants.
  - `seed_colors`, which takes colors from an image.
  - `ColorScheme::lerp` and `set_color_scheme_animated`.
- **md3e shapes**: `MaterialShape` (34 shapes), `shape` / `styled_shape` (which morph on a spring), and a public `Ink` (state layers and ripples).
- **md3e components**:
  - `DatePickerStyle::no_header()`.
  - `wavy_progress_eased`: a wavy progress bar whose line moves to a new value at a speed you choose, for seek bars.
  - Menu submenus: `MenuItem::submenu`.
- **style**: `overflow_x_scroll()`.
- **Image memory**:
  - Large photos are decoded one at a time and scaled down while still RGB.
  - What stays in glibc's heap is returned to the OS.
  - `images::decode_bitmap` takes the pixels without showing the image.
- **shell**: `Battery`'s `energy()`, `energy_full()`, `health()` and `charge_cycles()`.
