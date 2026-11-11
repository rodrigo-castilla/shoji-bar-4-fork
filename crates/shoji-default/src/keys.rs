//! The keys Omarchy binds as Hyprland *global shortcuts* (`hl.dsp.global("omarchy:audio.raise")`):
//! the compositor hands those to whoever registered them, which is Quickshell, so with it gone (or
//! while it still holds them) nothing here would hear them. While this shell runs they are bound to
//! the command Omarchy itself falls back to when no shell registers a shortcut, and the commands
//! reach this shell the usual way: `omarchy-audio-output-volume` and `omarchy-brightness-display`
//! change the level and call `omarchy-osd`, and `omarchy-shell media ...` is answered by
//! [`omarchy`](crate::omarchy). On exit the global shortcuts are put back.
//!
//! Done through `hyprctl eval` at run time: nothing is written to the user's configuration.

use std::process::Command;

/// (keys, global shortcut it is by default (empty: a plain command), command, repeats while held, works when locked)
const KEYS: &[(&str, &str, &str, bool, bool)] = &[
    ("XF86AudioRaiseVolume", "audio.raise", "omarchy-audio-output-volume raise", true, true),
    ("XF86AudioLowerVolume", "audio.lower", "omarchy-audio-output-volume lower", true, true),
    ("XF86AudioMute", "audio.mute-toggle", "omarchy-audio-output-volume mute-toggle", false, true),
    ("XF86MonBrightnessUp", "brightness.raise", "omarchy-brightness-display +5%", true, true),
    ("XF86MonBrightnessDown", "brightness.lower", "omarchy-brightness-display 5%-", true, true),
    ("XF86AudioNext", "ipc.media.next", "omarchy-shell media next", false, true),
    ("ALT + XF86AudioPlay", "ipc.media.next", "omarchy-shell media next", false, true),
    ("XF86AudioPrev", "ipc.media.previous", "omarchy-shell media previous", false, true),
    ("ALT + SHIFT + XF86AudioPlay", "ipc.media.previous", "omarchy-shell media previous", false, true),
    ("XF86AudioPause", "ipc.media.playPause", "omarchy-shell media playPause", false, true),
    ("XF86AudioPlay", "ipc.media.playPause", "omarchy-shell media playPause", false, true),
    ("SUPER + comma", "ipc.notifications.act", "omarchy-shell notifications act", false, false),
    ("SUPER + ALT + comma", "ipc.notifications.dismissOne", "omarchy-shell notifications dismissOne", false, false),
    ("SUPER + SHIFT + comma", "ipc.notifications.dismissAll", "omarchy-shell notifications dismissAll", false, false),
    ("SUPER + SHIFT + ALT + comma", "ipc.notifications.showHistory", "omarchy-shell notifications showHistory", false, false),
    ("SUPER + SPACE", "menu.root", "omarchy-menu toggle root", false, false),
    ("SUPER + ALT + SPACE", "", "omarchy-shell omarchy.dock apps", false, false),
    ("SUPER + ESCAPE", "menu.system", "omarchy-menu toggle system", false, false),
    ("XF86PowerOff", "menu.system", "omarchy-menu toggle system", false, true),
    ("SUPER + CTRL + C", "menu.capture", "omarchy-menu toggle capture", false, false),
    ("SUPER + CTRL + O", "menu.toggle", "omarchy-menu toggle toggle", false, false),
    ("SUPER + CTRL + H", "menu.hardware", "omarchy-menu toggle hardware", false, false),
    ("SUPER + CTRL + SPACE", "menu.background", "omarchy-menu toggle background", false, false),
    ("SUPER + CTRL + W", "menu.background", "omarchy-menu toggle background", false, false),
    ("SUPER + SHIFT + CTRL + SPACE", "menu.theme", "omarchy-menu toggle theme", false, false),
    ("SUPER + CTRL + T", "menu.theme", "omarchy-menu toggle theme", false, false),
    ("SUPER + CTRL + S", "menu.share", "omarchy-menu toggle share", false, false),
    ("SUPER + CTRL + R", "menu.reminder-set", "omarchy-menu toggle reminder-set", false, false),
    ("SUPER + CTRL + V", "panel.omarchy.clipboard", "omarchy-shell shell toggle omarchy.clipboard", false, false),
];

fn eval(code: &str) {
    let _ = Command::new("hyprctl").args(["eval", code]).output();
}

fn lua(to_command: bool) -> String {
    let mut code = String::new();
    for (keys, shortcut, command, repeats, locked) in KEYS {
        let dispatcher = if to_command || shortcut.is_empty() {
            format!("hl.dsp.exec_cmd('{command}')")
        } else {
            format!("hl.dsp.global('omarchy:{shortcut}')")
        };
        code.push_str(&format!(
            "hl.unbind('{keys}'); hl.bind('{keys}', {dispatcher}, \
             {{ locked = {locked}, repeating = {repeats} }}); "
        ));
    }
    code
}

/// Binds the keys to the commands. Off the UI thread: `hyprctl` is slow.
///
/// Hyprland forgets runtime binds whenever the configuration is reloaded (a theme change, a
/// `hyprctl reload`...), which would hand the keys back to the global shortcuts, so the same thread
/// listens to its event socket and binds them again after every `configreloaded`.
pub fn take() {
    let _ = std::thread::Builder::new().name("shoji-bar-4 keys".into()).spawn(|| {
        eval(&lua(true));
        watch_reloads();
    });
}

fn watch_reloads() {
    use std::io::{BufRead, BufReader};
    let (Ok(runtime), Ok(sig)) = (std::env::var("XDG_RUNTIME_DIR"), std::env::var("HYPRLAND_INSTANCE_SIGNATURE"))
    else {
        return;
    };
    let Ok(stream) = std::os::unix::net::UnixStream::connect(format!("{runtime}/hypr/{sig}/.socket2.sock")) else {
        return;
    };
    for line in BufReader::new(stream).lines().map_while(Result::ok) {
        if line.starts_with("configreloaded") {
            // The reload finishes binding a moment after the event.
            std::thread::sleep(std::time::Duration::from_millis(300));
            eval(&lua(true));
        }
    }
}

/// Puts the global shortcuts back.
pub fn release() {
    eval(&lua(false));
}
