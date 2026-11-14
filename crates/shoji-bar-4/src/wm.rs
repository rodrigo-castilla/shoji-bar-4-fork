//! Tells ShojiWM the color scheme, so that the window frames of a config that wants it
//! (`~/.config/shojiwm`) are drawn in the same colors as the shell.
//!
//! It goes over the config's IPC socket (`$XDG_RUNTIME_DIR/shojiwm-$WAYLAND_DISPLAY.sock`) as
//! `{"method":"theme.set","params":{...}}`, the roles named as in Material 3 (`primary`,
//! `surfaceContainerHigh`, ...) in `#rrggbb`. The scheme is sent again whenever it changes and
//! whenever the connection is made again: the socket is made anew when the config is reloaded.
//! A config that doesn't know `theme.set` leaves it unanswered.

use crate::settings::settings;
use crate::theme::{current_seed, hex_of, is_dark, scheme_for};
use anti_webui::prelude::*;
use anti_webui_md3e::ColorScheme;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, Once};
use std::time::Duration;

fn socket() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    dir.join(format!("shojiwm-{display}.sock"))
}

/// What there is to send, and which connections have gone away.
struct State {
    /// The message for the latest scheme.
    latest: Option<String>,
    /// Counts the connections that have ended, so the one waiting knows when it is its turn.
    ended: u64,
}

static STATE: Mutex<State> = Mutex::new(State { latest: None, ended: 0 });
static CHANGED: Condvar = Condvar::new();

/// Sends the scheme whenever it changes, for as long as the shell runs. Call once, from the root,
/// after [`crate::theme::install`].
pub fn share_theme() {
    effect(move || {
        let s = settings().get();
        let scheme = scheme_for(current_seed(&s), s.palette, is_dark());
        let message = theme_message(&scheme, is_dark());
        STATE.lock().unwrap().latest = Some(message);
        CHANGED.notify_all();
    });
    static START: Once = Once::new();
    START.call_once(|| {
        std::thread::Builder::new()
            .name("shojiwm-theme".into())
            .spawn(keep_connected)
            .ok();
    });
}

fn theme_message(c: &ColorScheme, dark: bool) -> String {
    let roles = [
        ("primary", c.primary),
        ("onPrimary", c.on_primary),
        ("primaryContainer", c.primary_container),
        ("onPrimaryContainer", c.on_primary_container),
        ("secondary", c.secondary),
        ("onSecondary", c.on_secondary),
        ("secondaryContainer", c.secondary_container),
        ("onSecondaryContainer", c.on_secondary_container),
        ("tertiary", c.tertiary),
        ("tertiaryContainer", c.tertiary_container),
        ("onTertiaryContainer", c.on_tertiary_container),
        ("error", c.error),
        ("onError", c.on_error),
        ("errorContainer", c.error_container),
        ("onErrorContainer", c.on_error_container),
        ("surface", c.surface),
        ("surfaceDim", c.surface_dim),
        ("surfaceBright", c.surface_bright),
        ("surfaceContainerLowest", c.surface_container_lowest),
        ("surfaceContainerLow", c.surface_container_low),
        ("surfaceContainer", c.surface_container),
        ("surfaceContainerHigh", c.surface_container_high),
        ("surfaceContainerHighest", c.surface_container_highest),
        ("onSurface", c.on_surface),
        ("onSurfaceVariant", c.on_surface_variant),
        ("inverseSurface", c.inverse_surface),
        ("inverseOnSurface", c.inverse_on_surface),
        ("outline", c.outline),
        ("outlineVariant", c.outline_variant),
        ("scrim", c.scrim),
    ];
    let mut params = serde_json::Map::new();
    params.insert("dark".into(), dark.into());
    for (name, color) in roles {
        params.insert(name.into(), format!("#{:06x}", hex_of(color)).into());
    }
    serde_json::json!({ "method": "theme.set", "params": params }).to_string()
}

/// Connects, and again whenever the connection ends: soon after a reload, less and less often
/// while there is no ShojiWM to connect to.
fn keep_connected() {
    let mut wait = Duration::from_millis(500);
    loop {
        if let Ok(stream) = UnixStream::connect(socket()) {
            wait = Duration::from_millis(500);
            serve(stream);
        }
        std::thread::sleep(wait);
        wait = (wait * 2).min(Duration::from_secs(30));
    }
}

/// Sends the scheme now and each time it changes, until the connection ends.
fn serve(stream: UnixStream) {
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    let mine = STATE.lock().unwrap().ended;
    // What the compositor sends (its broadcasts) is read and dropped; the end of it is the end
    // of the connection.
    std::thread::spawn(move || {
        let mut lines = BufReader::new(reader);
        let mut line = String::new();
        while matches!(lines.read_line(&mut line), Ok(n) if n > 0) {
            line.clear();
        }
        STATE.lock().unwrap().ended += 1;
        CHANGED.notify_all();
    });
    let mut stream = stream;
    let mut sent: Option<String> = None;
    let mut state = STATE.lock().unwrap();
    loop {
        if state.ended != mine {
            return;
        }
        match &state.latest {
            Some(latest) if sent.as_ref() != Some(latest) => {
                let message = latest.clone();
                drop(state);
                if writeln!(stream, "{message}").is_err() {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    // The reader ends too, and counts it.
                    let mut state = STATE.lock().unwrap();
                    while state.ended == mine {
                        state = CHANGED.wait(state).unwrap();
                    }
                    return;
                }
                sent = Some(message);
                state = STATE.lock().unwrap();
            }
            _ => state = CHANGED.wait(state).unwrap(),
        }
    }
}
