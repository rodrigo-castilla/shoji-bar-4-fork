# Changelog

All notable changes to this fork of [shoji-bar-4](https://github.com/bea4dev/shoji-bar-4) are
documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the
project uses [Semantic Versioning](https://semver.org/) (below 1.0 a minor bump may break things).

Building needs the patched AntiWebUI: run `scripts/apply-antiwebui-patch.sh` first (it applies every
file in `patches/`). Some features call external tools: `grim` (translucent ink), `wl-paste`,
`setpriv`, `jq` and `perl` (clipboard history, via Omarchy's own `capture.sh`).

## [Unreleased]

## [0.14.0] - 2026-11-11 (pre-release)

### Added
- Shutdown handling (`shutdown.rs`): on `SIGTERM`, `SIGINT`, `SIGHUP` or a panic the shell gives
  back what it took (the rebound keys, Omarchy's socket, its own socket) before it exits.
- The clipboard history is written under a lock (a mutex in the process plus `flock` on a lock
  file), so two writers cannot drop each other's changes.
- `CHANGELOG.md`; `.gitignore` for build output, local cargo config, measurements and leftovers.

### Fixed
- `Cargo.lock` had been committed pointing at the machine-local AntiWebUI copy instead of the
  upstream revision; it is the upstream lock again, with the new dependencies.

## [0.13.0] - 2026-11-11

### Changed
- The clipboard panel reads Omarchy's own history (`~/.local/state/omarchy/clipboard-history.json`)
  instead of cliphist, with pictures from Omarchy's image store. Copying goes through
  `omarchy-clipboard-paste-text --copy-only`.

### Added
- When Quickshell is not running, the shell runs the `wl-paste --watch capture.sh` watchers its
  clipboard plugin used to run, so copies are kept.

## [0.12.1] - 2026-11-11

### Fixed
- The clipboard and the power dialog are as wide as their content needs (their old width up to 94 %
  of the screen) instead of cutting it.

## [0.12.0] - 2026-11-11

### Added
- Clicking a notification does what Omarchy's does: runs its `omarchy-exec-argv` command, else its
  `default` action, else its first button, else brings the app forward.
- Buttons from the `omarchy-actions` hint (a screenshot's Open and Edit).
- Omarchy's menu card (keybindings, selects) fits its content up to 94 % of the screen.
- `patches/antiwebui-notification-exec.patch`: two hint fields on AntiWebUI's `Notification`.

## [0.11.0] - 2026-11-11

### Added
- `omarchy-shell notifications keyboard`: keyboard mode over the popups (arrows or J/K/Tab, number
  keys for buttons, Enter to click, Delete to dismiss, Escape or ten idle seconds to leave).

## [0.10.2] - 2026-11-11

### Fixed
- Translucent: with a panel open, the ink follows what is behind the panel, not the strip of the bar.

## [0.10.1] - 2026-11-11

### Added
- Translucent: the ink turns dark over a bright backdrop. The backdrop is sampled off the UI thread,
  on Hyprland's events, with hysteresis.

## [0.10.0] - 2026-11-11

### Added
- Image picker (themes and backgrounds) answering `omarchy-shell image-selector` and
  `shell summon omarchy.image-picker`.
- `notifications.expand` opens the list in quick settings.

## [0.9.1] - 2026-11-11

### Fixed
- The volume, brightness, media and menu keys are bound again after Hyprland reloads its
  configuration, which used to hand them back to Quickshell's global shortcuts.

### Added
- Performance tooling: per-frame CPU time in the frame meter and `scripts/perf-ab.sh` /
  `perf-report.py` / `blur-ab.sh` for repeated, interleaved A/B runs (frame-time percentiles,
  1 % / 0.1 % lows, jitter, dropped frames, iGPU busy, Mann-Whitney U).

## [0.9.0] - 2026-11-11

### Added
- Omarchy's menu and its select/input prompts as a panel (`omarchy-menu`, `omarchy-menu-select`,
  `omarchy-menu-input`), with the default menu plus the user's extensions, providers, conditions and
  search; the keys for it are rebound while the shell runs.

## [0.8.0] - 2026-11-11

### Added
- `notifications.*` answered by the shell (`act`, `invokeLast`, `invokeAction`, `dismiss*`, DND,
  `count`, `showHistory`), and the `Super+,` keys.

## [0.7.0] - 2026-11-11

### Added
- The shell answers Omarchy's `omarchy-shell` socket in place of Quickshell: media, OSD and
  notification calls locally, the rest forwarded. Volume, brightness and media keys rebound to their
  command forms.

## [0.5.0] - 2026-11-11

### Added
- Quick settings tile that cycles the GPU mode (integrated, hybrid, dedicated) through `supergfxctl`.

## [0.4.3] - 2026-11-11

### Changed
- Performance: blocking `hyprctl` and disk reads moved off the UI thread, fat LTO release profile.

### Added
- Frame meter (`plugins/meter.rs`) and `scripts/fps-test.sh`.

## [0.4.0] - 2026-11-11

### Added
- Opacity slider for any theme.

[Unreleased]: https://github.com/rodrigo-castilla/shoji-bar-4-fork/compare/v0.14.0...HEAD
[0.14.0]: https://github.com/rodrigo-castilla/shoji-bar-4-fork/compare/v0.3.0...v0.14.0
