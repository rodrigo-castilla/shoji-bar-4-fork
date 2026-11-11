# TODO

## Omarchy port actions

Analysis of what Omarchy does through Quickshell (its bindings are in
`/usr/share/omarchy/default/hypr/bindings/*.lua`, the shell in `/usr/share/omarchy/shell`) and what
`shoji-bar-4` has to do to answer the same actions when it is the bar in use. Nothing here is
implemented yet by this branch (`feat/omarchy-actions`): this is the analysis.

### The way it works now (0.7.0): shoji answers in place of Quickshell

`omarchy-shell <target> <method>` (used by the `omarchy-*` commands, by `omarchy-osd`, and by any
binding written as a command, e.g. `omarchy-shell media next`) writes the call to
`$XDG_RUNTIME_DIR/omarchy-shell-<md5 of "$OMARCHY_PATH/shell\n$WAYLAND_DISPLAY">.sock` and, on
`SKIP`, falls back to `qs ipc`. `crates/shoji-default/src/omarchy.rs` moves Quickshell's socket
aside (`.quickshell`), binds its own, does what it has an answer for (`media.next|previous|
playPause|play|pause`, `osd.show`) and forwards everything else to Quickshell's socket, relaying its
reply. On `msg quit` the socket goes back; after a crash the next start repairs it.

Done in 0.7.3 (`keys.rs`): while shoji runs, the keys Omarchy binds as global shortcuts for volume,
brightness and media are rebound (with `hyprctl eval`, nothing written to the user's files) to the
command Omarchy falls back to (`omarchy-audio-output-volume raise`, `omarchy-brightness-display
+5%`, `omarchy-shell media next`, ...), and put back on `msg quit`. `osd.show` now draws the volume
and brightness pills from what the call says (the keys move the physical sink, which is not always
the default one the pill used to follow).

Done in 0.8.0: `notifications.*` answered in `omarchy.rs` (`act`, `invokeLast`, `invokeAction`,
`dismissOne`, `dismissAll`, `dismiss`, `showHistory` (opens quick settings, where the list is),
`count`, `isDnd`, `toggleDnd`, `setDnd`), and the four `Super+,` keys rebound in `keys.rs`.
`notifications.keyboard` and `expand` have no equivalent yet (they answer `none`). Fixed on the way:
a panic when the popups layer closed while a layout report of it was still coming.

Done in 0.9.0: the Omarchy menu and its prompts (`plugins/omarchy_menu.rs`, with the hook
`omarchy::on_shell_call` in core). `omarchy-menu toggle|summon|close <route>` and every
`omarchy-shell shell toggle|summon|hide omarchy.menu '{...}'` open a panel with Omarchy's own menu
(default `omarchy-menu.jsonc` plus the user's extensions: submenus, links, actions, `when`/`checked`/
`disabled` conditions, the fonts/kernels/languages/power-profiles providers, search across the whole
menu, keyboard and pointer). `omarchy-menu-select` and `omarchy-menu-input` (single and multiple
selection, text input) answer through the same panel and the files they wait on. Keys rebound in
`keys.rs`: `Super+Space`, `Super+Alt+Space` (apps = the launcher), `Super+Esc`, power key, and
`Super+Ctrl+` C, O, H, Space, W, T, S, R, V (clipboard), `Super+Shift+Ctrl+Space`. Also
`omarchy.dock apps`, and `shell toggle` of `omarchy.clipboard`, `omarchy.clock` and the
audio/network/bluetooth/monitor/power panels (mapped to the clipboard, calendar and quick settings).

Done in 0.10.0: the image picker (`plugins/image_picker.rs`): `omarchy-shell image-selector open ...`
(what `omarchy-menu-images` and the Background row use, so `Super+Ctrl+Space`) and
`shell summon omarchy.image-picker '{"source":"themes"}'` (the Theme row, applied with
`omarchy-theme-set`). Arrows, Enter, Escape, search, click. `notifications.expand` opens the list in quick settings, as `showHistory` does.
`notifications.keyboard` (0.11.0) is the real keyboard mode: the popups take the keyboard, the newest
card is selected; Down/J/Tab and Up/K move, a number presses that button (the buttons of the selected
card are numbered), Enter clicks the card, Delete/Backspace dismisses it, Escape or ten idle seconds
give the keyboard back.

Not yet: emojis (`omarchy.emojis`), reminders (`omarchy-reminder`), translations of the menu
(`~/.config/omarchy/language`), `--on-change` live selects, and the rest of the plugin panels
(weather, tailscale, elsewhen, ...). Next.

Not reached this way (before `keys.rs`): the bindings Omarchy writes as Hyprland **global shortcuts**
(`hl.dsp.global("omarchy:...")`, for the kinds listed in `/usr/share/omarchy/default/omarchy/shortcuts`:
menus, panels, `audio.*`, `brightness.*`, `ipc.media.*`, `ipc.notifications.*`), which go to whoever
registered them, Quickshell. To catch them, `alter-bar` has to rebind those keys to the command form
(`hl.unbind` + `hl.bind(... "omarchy-shell media next")`, through `hyprctl eval`, nothing written
to the user's files) while shoji is the bar, and put them back after. Planned for `alter-bar`.

Next on the list to answer here: `notifications.*`, `shell toggle/summon` for the menus and panels,
`nightlight.*`, `idle.*`, `lock.*`.

### How Omarchy does it

- Key bindings call one of three things: `{ ipc = "target.method" }` (a call into the running
  Quickshell, the same as `omarchy-shell <target> <method>`), `{ menu = ... }` / `{ panel = ... }`
  (the same through `omarchy-shell shell toggle <plugin>`), or a plain `omarchy-*` command.
- Quickshell IPC targets (`qs -n -p /usr/share/omarchy/shell ipc show`): `media`
  (`next previous playPause play pause stop source sourceNext sourcePrevious status`),
  `notifications` (`act dismissOne dismissAll dismiss invokeAction invokeLast keyboard showHistory
  expand toggleDnd setDnd isDnd count`), `osd` (`show close state`), `omarchy.dock` (`apps`),
  `nightlight`, `idle`, `lock`, `shell` (`toggle summon call cycleInput toggleBarTransparency ...`),
  `omarchy.audio|bluetooth|network|power|monitor|clock|elsewhen|...` (`toggle open close`).
- Quickshell is never killed, so those bindings keep reaching it while shoji-bar-4 is the visible
  bar. The gaps are what Quickshell would **show** (OSD, toasts, panels) and what it **owns**:
  `org.freedesktop.Notifications` is held by Quickshell (pid of `quickshell`, checked with
  `busctl --user list`), so the notification code of shoji (`notify.rs`) is idle while that lasts.
- Quickshell's own panels (`omarchy.audio`, `.network`, ...) open in Quickshell's layer, hidden or
  not depending on the bar. The port is: a command in shoji's socket (`shoji-bar-4 msg ...`, see
  `crates/shoji-default/src/ipc.rs`) for each action, and the binding pointing at it while shoji is
  active. `alter-bar` is the natural place to swap the bindings (or a wrapper that forwards to the
  bar in use).

### Already there

| Action | Where |
|---|---|
| Volume up/down/mute, brightness up/down, with the pill | `osd.rs` (reacts to the change, however it was made) |
| Apps (`Super+Alt+Space`, `omarchy.dock apps`) | `shoji-bar-4 msg toggle launcher` |
| Lock | `shoji-bar-4 msg lock` |
| Theme dark/light | `shoji-bar-4 msg theme ...`, palettes in `plugins/colour.rs` |
| Panels: quick settings, calendar, clipboard history, media | `shoji-bar-4 msg toggle <panel>` |
| Power dialog (shutdown, reboot, logout) | `power_dialog.rs` |
| GPU mode (not an Omarchy action) | `plugins/gpu.rs` |
| Media: next, previous, play/pause, with the pill, and the pill for a new track | `shoji-bar-4 msg media next\|previous\|play-pause\|play\|pause`, `osd.rs` |

### To port, by how often they are used

1. **Media: play/pause, next, previous** (DONE in 0.7.0: `omarchy-shell media ...` and `msg media ...` with the pill; a track that ends by itself shows nothing. Left: the global-shortcut bindings, see above) (`XF86AudioPlay/Pause/Next/Prev`, `Super+XF86AudioRaise/Lower`
   for next/previous, `Alt+XF86AudioPlay` next, `Alt+Shift+XF86AudioPlay` previous). Quickshell does
   it through MPRIS; shoji has the MPRIS client (`media()`), so a `msg media next|previous|play-pause`
   is a few lines. Missing: the **OSD for them** (Omarchy's `osd.show` with `media-next`,
   `media-pause`, ... icons) and a toast when the **track changes** (the thing asked for first).
2. **Notifications** (DONE in 0.8.0 except keyboard control and the toasts replay; `Super+,` act on the last one, `Super+Alt+,` dismiss last, `Super+Shift+,`
   dismiss all, `Super+Ctrl+,` keyboard control, `Super+Shift+Alt+,` history, `Super+Ctrl+Alt+,`
   silence). Needs `notify.rs` to be the daemon (Quickshell has the bus name) or to forward to
   `notifications.*`; then `msg notifications act|dismiss-one|dismiss-all|history|keyboard|dnd`.
   Also `omarchy-notification-send|dismiss|wait` and the informative ones
   (`omarchy-notification-time|battery|weather`, `Super+Ctrl+Alt+T|B`) that only need a toast.
3. **Audio switching**: mute microphone (`XF86AudioMicMute`, `omarchy-audio-input-mute`), switch
   output (`Shift+XF86AudioMute`, `omarchy-audio-output-switch`), switch media source
   (`Shift+XF86AudioPlay|Pause`, `media.sourceNext`), precise volume `Alt+XF86Audio*` (±1 %).
   All have an OSD in Quickshell (microphone icon, output name).
4. **Brightness extras**: precise (`Alt+`), min/max (`Shift+`), keyboard backlight
   (`XF86KbdBrightnessUp|Down`, cycle), each with the OSD pill (keyboard icon).
5. **Omarchy menu** (`Super+Space`, `Super+Esc`, and `Super+Ctrl+` C capture, O toggle, H hardware,
   T/W theme, S share, R reminder): `omarchy-shell shell toggle omarchy.menu {"menu":...}`. Either
   forward to Quickshell's menu (works already) or port the menu as a shoji panel; it is a big
   piece (`omarchy-menu`, `~/.config/omarchy/extensions/omarchy-menu.jsonc`).
6. **Toggles** (`Super+Ctrl+O` menu, binds `bind_toggle`): top bar (`Super+Shift+Space`;
   `alter-bar toggle` already switches bars), notification silencing, idle lock (`idle.toggle`),
   night light (`nightlight.toggle`), touchpad, screensaver, animations, suspend, hybrid GPU
   (`omarchy-toggle-hybrid-gpu`), theme sync, full-screen desktop. Each is a tile or a switch in
   quick settings plus a `msg` command.
7. **Capture**: screenshot (`PRINT`), screen recording (`Alt+PRINT`, with a recording indicator and
   stop), colour picker, OCR (`omarchy-capture-text`), QR, webcam overlay. The commands exist; what
   is missing is the **recording indicator** in the bar and the menu entries.
8. **Clipboard / emojis / reminders**: `omarchy.clipboard` (`Super+Ctrl+V`, shoji has its own
   history panel), `omarchy.emojis` (`Super+Ctrl+E`, no equivalent), reminders (`Super+Ctrl+R`,
   `omarchy-reminder`, no equivalent), dictation toggle (`omarchy-dictation`, an indicator while it
   listens).
9. **Window and workspace helpers** (these are Hyprland, not shell, and need nothing from the
   bar): minimize to the dock (`omarchy-hyprland-window-minimize`, needs the dock to show the
   minimized windows), pop out, transparency, gaps, layout cycle, monitor scaling. Only the dock's
   view of minimized windows and the workspace layout name in the bar are shell-side.
10. **Indicators and the rest of the bar**: idle lock on/off, night light, recording, system update
    (`omarchy.system-update`), weather, Tailscale, battery low hook
    (`~/.config/omarchy/hooks/battery-low.d`), Bluetooth/Wi-Fi QR (`omarchy.network showQr`),
    keybindings viewer (`omarchy-menu-keybindings`), input-method cycle (`shell cycleInput`).

### Suggested order of work

1. `msg media ...` + OSD for media actions + track-change toast (small, high use).
2. `msg notifications ...` and the decision on who owns the bus name.
3. Audio/brightness extras with their OSD.
4. Toggles as tiles (idle, night light, silencing, touchpad), reusing `add_tile_row`/`toggle_tile`.
5. Capture indicator, then menu, emojis and reminders.

Open question for each: forward to Quickshell's IPC (cheap, but needs Quickshell to be running,
which it always is here) or reimplement natively in shoji (own look, no dependency). The default
proposed is native for what shoji already has the data for (media, notifications, audio,
brightness) and forward for the rest.
