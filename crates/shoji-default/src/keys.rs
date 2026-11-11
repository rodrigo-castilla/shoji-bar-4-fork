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

/// (keys, global shortcut it is by default, command, repeats while held)
const KEYS: &[(&str, &str, &str, bool)] = &[
    ("XF86AudioRaiseVolume", "audio.raise", "omarchy-audio-output-volume raise", true),
    ("XF86AudioLowerVolume", "audio.lower", "omarchy-audio-output-volume lower", true),
    ("XF86AudioMute", "audio.mute-toggle", "omarchy-audio-output-volume mute-toggle", false),
    ("XF86MonBrightnessUp", "brightness.raise", "omarchy-brightness-display +5%", true),
    ("XF86MonBrightnessDown", "brightness.lower", "omarchy-brightness-display 5%-", true),
    ("XF86AudioNext", "ipc.media.next", "omarchy-shell media next", false),
    ("ALT + XF86AudioPlay", "ipc.media.next", "omarchy-shell media next", false),
    ("XF86AudioPrev", "ipc.media.previous", "omarchy-shell media previous", false),
    ("ALT + SHIFT + XF86AudioPlay", "ipc.media.previous", "omarchy-shell media previous", false),
    ("XF86AudioPause", "ipc.media.playPause", "omarchy-shell media playPause", false),
    ("XF86AudioPlay", "ipc.media.playPause", "omarchy-shell media playPause", false),
];

fn eval(code: &str) {
    let _ = Command::new("hyprctl").args(["eval", code]).output();
}

fn lua(to_command: bool) -> String {
    let mut code = String::new();
    for (keys, shortcut, command, repeats) in KEYS {
        let dispatcher = if to_command {
            format!("hl.dsp.exec_cmd('{command}')")
        } else {
            format!("hl.dsp.global('omarchy:{shortcut}')")
        };
        code.push_str(&format!(
            "hl.unbind('{keys}'); hl.bind('{keys}', {dispatcher}, \
             {{ locked = true, repeating = {repeats} }}); "
        ));
    }
    code
}

/// Binds the keys to the commands. Off the UI thread: `hyprctl` is slow.
pub fn take() {
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 keys".into())
        .spawn(|| eval(&lua(true)));
}

/// Puts the global shortcuts back.
pub fn release() {
    eval(&lua(false));
}
