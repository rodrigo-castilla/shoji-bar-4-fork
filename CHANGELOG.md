# Changelog

All notable changes to this fork of [shoji-bar-4](https://github.com/bea4dev/shoji-bar-4) are
documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (below 1.0 a minor bump may break things).

## Building

The fork builds against AntiWebUI with its own patches (`patches/`). Once per clone:

```sh
scripts/apply-antiwebui-patch.sh   # copies the pinned AntiWebUI, patches it, writes .cargo/config.toml
cargo build --release              # Cargo.lock is committed as this build resolves it: it stays clean
```

Some features call external tools: `grim` (translucent ink), `wl-paste`, `setpriv`, `jq` and `perl`
(clipboard history, through Omarchy's own `capture.sh`).

## [Unreleased]

### Added
- `docs/architecture.md`: a reading guide to the code (entry point, startup, rendering, panels, theme,
  outside channels).

## [0.4.0] - 2026-11-11 (pre-release)

Everything added on top of 0.3.0: the shell can now be the only bar of an Omarchy session, answering
what Omarchy asks of Quickshell.

### Added
- **Omarchy integration**
  - The shell answers Omarchy's `omarchy-shell` socket in place of Quickshell (media, OSD,
    notifications, menus; what it does not answer is forwarded to Quickshell when that is running).
  - The volume, brightness, media, notification and menu keys that Omarchy binds as global shortcuts
    are rebound to their command forms while the shell runs, and bound again after Hyprland reloads
    its configuration. They are put back when the shell exits (also on `SIGTERM`, `SIGINT`,
    `SIGHUP` or a panic).
  - Omarchy's menu and its select/input prompts as a panel (`omarchy-menu`, `omarchy-menu-select`,
    `omarchy-menu-input`, keybindings, ...): default menu plus the user's extensions, providers,
    conditions, search, keyboard and pointer.
  - Image picker for themes and backgrounds (`omarchy-shell image-selector`,
    `shell summon omarchy.image-picker`).
  - Clipboard panel on Omarchy's own history (`clipboard-history.json`); when Quickshell is not
    running the shell runs the `wl-paste --watch capture.sh` watchers that keep it, and writes the
    history under a lock.
- **Notifications**
  - `notifications.*` calls (`act`, `invokeLast`, `invokeAction`, `dismiss*`, do-not-disturb,
    `count`, `showHistory`, `expand`) and the `Super+,` keys.
  - Keyboard mode (`notifications keyboard`): arrows or J/K/Tab, number keys for buttons, Enter,
    Delete, Escape.
  - Clicking a card does what Omarchy's does (its `omarchy-exec-argv` command, else `default`, else
    the first button, else brings the app forward); `omarchy-actions` buttons such as a
    screenshot's Open and Edit.
- **Themes**
  - Opacity slider for any palette.
  - Translucent: the ink turns dark over a bright backdrop (sampled off the UI thread, on
    Hyprland's events, with hysteresis; with a panel open it follows what is behind the panel).
- **Quick settings:** tile that cycles the GPU mode (integrated, hybrid, dedicated) with
  `supergfxctl`.
- **Panels** fit their content: the Omarchy menu card, the clipboard and the power dialog are as
  wide as what is in them needs (from their old width up to 94 % of the screen).
- **Performance tooling:** frame meter with per-frame CPU time (`plugins/meter.rs`),
  `scripts/fps-test.sh`, and `scripts/perf-ab.sh` / `perf-report.py` / `blur-ab.sh` for repeated,
  interleaved A/B runs (frame-time percentiles, 1 % / 0.1 % lows, jitter, dropped frames, iGPU busy,
  Mann-Whitney U).
- `patches/antiwebui-notification-exec.patch` (notification hints) next to the UI zoom patch;
  `scripts/apply-antiwebui-patch.sh` applies every patch of `patches/` at the revision pinned in
  `patches/antiwebui.rev`.

### Changed
- Blocking `hyprctl` calls and disk reads moved off the UI thread (frame drops down from 2.7 % to
  0.7 % in the meter); fat LTO release profile.
- `Cargo.lock` is committed as the patched build resolves it, so `cargo build --release` leaves it
  untouched.

### Fixed
- The shell no longer panics when the notification popups close while a layout report of them is
  still coming.

[Unreleased]: https://github.com/rodrigo-castilla/shoji-bar-4-fork/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/rodrigo-castilla/shoji-bar-4-fork/compare/v0.3.0...v0.4.0
