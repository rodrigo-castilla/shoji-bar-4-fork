# Usage

## Building and running

```sh
cargo build --release
./target/release/shoji-bar-4
```

Run it inside a ShojiWM session. Normally ShojiWM starts it (see below).

## Using it with ShojiWM

ShojiWM's default config already does everything here. For a config of your own, start the shell and bind its panels to keys in `~/.config/shojiwm/src/index.tsx`:

```ts
const SHOJI_BAR_4 = "~/.config/shoji-bar-4/target/release/shoji-bar-4";
COMPOSITOR.process.once("shell", {
  command: `exec ${SHOJI_BAR_4}`,
  runPolicy: "once-per-session",
});

function toggleLauncher() {
  const monitor = HYBRID_WINDOW_MANAGER.getCurrentMonitorName();
  COMPOSITOR.process.spawn({ command: `${SHOJI_BAR_4} msg toggle launcher "${monitor}"` });
}
COMPOSITOR.key.bind("launcher", "Super+A", toggleLauncher);
```

The window frames can follow the shell's colors:

- The shell stays connected to ShojiWM's IPC socket (`$XDG_RUNTIME_DIR/shojiwm-$WAYLAND_DISPLAY.sock`).
- It sends its color scheme there as `theme.set` (`src/wm.rs`): on start, on reconnecting (including after a config reload), and whenever the scheme changes.
- A config that doesn't handle `theme.set` ignores it.

The layer namespaces are `shoji-bar-4-bar`, `-dock`, `-quick-settings`, `-calendar`, `-launcher`, `-clipboard`, `-media`, `-notifications`, `-osd`, `-power` and `-wallpaper`. Use them to give layers ShojiWM effects such as blur.

## Commands

The running shell takes commands over its socket (`$XDG_RUNTIME_DIR/shoji-bar-4.$WAYLAND_DISPLAY.sock`):

```sh
shoji-bar-4 msg toggle launcher [OUTPUT]       # without OUTPUT: the output of the focused window
shoji-bar-4 msg toggle quick-settings [OUTPUT]
shoji-bar-4 msg toggle calendar [OUTPUT]
shoji-bar-4 msg toggle clipboard [OUTPUT]
shoji-bar-4 msg toggle media [OUTPUT]
shoji-bar-4 msg close
shoji-bar-4 msg lock                           # only the password unlocks
shoji-bar-4 msg theme dark|light|auto|toggle
shoji-bar-4 msg status                         # open panels, running clocks and so on
shoji-bar-4 msg layout                         # the bar's islands and the panels, with their ids (for plugin development)
shoji-bar-4 msg quit
```

## Settings

Settings are kept in `$XDG_STATE_HOME/shoji-bar-4/settings.json` (usually `~/.local/state/…`). Changes made in the quick settings are saved there, and you can also edit the file by hand.

| Key | Meaning |
|---|---|
| `mode` | `light` / `dark` / `auto` (light from 7:00 to 19:00) |
| `seed` | The seed color (`#rrggbb`) |
| `seed_from_wallpaper` | Take the seed color from the wallpaper |
| `palette` | `tonal-spot` / `vibrant` / `expressive` / `fidelity` / `neutral` / `monochrome` |
| `wallpaper` | The wallpaper's path (`null` draws none) |
| `pinned` | Desktop IDs of the apps pinned to the dock |
| `clock_24h` / `clock_seconds` | How the clock is shown |
| `ui_scale` | Size of the whole interface: `1.0` normal, `0.85` smaller (`0.5` to `2.0`). Read at start-up; needs the patched AntiWebUI, see [UI zoom](development.md#ui-zoom) |

Wallpapers to choose from are read from `~/Pictures/wallpapers`.
