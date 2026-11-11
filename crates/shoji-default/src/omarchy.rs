//! Answers what Omarchy sends to its shell, in its place.
//!
//! Omarchy talks to its shell (Quickshell) with `omarchy-shell <target> <method> [args]`, which
//! writes the call to a unix socket, `$XDG_RUNTIME_DIR/omarchy-shell-<hash>.sock`, and reads the
//! answer: `OK<US>output<RS>` when the call ran, `SKIP<RS>` when it ran nothing (then
//! `omarchy-shell` asks Quickshell itself over `qs ipc`). Fields are split by the unit separator
//! (0x1f) and the request ends with a record separator (0x1e). Every `omarchy-*` command that shows
//! something (`omarchy-osd`, the media keys written as `omarchy-shell media next`, ...) ends there.
//!
//! This module takes that socket over: Quickshell's is moved aside (`<socket>.quickshell`, its open
//! listener keeps working), ours is bound in its place, and
//!
//! - what the shell has its own answer for (`media.*`, `osd.show`) is done here, with our pill, and
//!   Quickshell never hears of it, so nothing shows twice;
//! - anything else is passed on to Quickshell's socket as it came, and its answer is passed back,
//!   so the rest of Omarchy keeps working.
//!
//! When the shell quits the socket is put back. Quickshell is not touched otherwise.
//!
//! What does not come this way: the bindings Omarchy writes as a Hyprland *global shortcut*
//! (`omarchy:audio.raise`, `omarchy:ipc.media.next`, ... for the keys it binds by default) go to
//! whoever registered that shortcut, which is Quickshell. Those need the binding to run the command
//! form instead (`omarchy-shell media next`).

use crate::osd;
use anti_webui::prelude::*;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const US: char = '\u{1f}';
const RS: u8 = 0x1e;

/// Where Omarchy's shell listens: the same hash `omarchy-shell` makes.
fn socket() -> Option<PathBuf> {
    let omarchy = std::env::var("OMARCHY_PATH").unwrap_or_else(|_| "/usr/share/omarchy".into());
    let display = std::env::var("WAYLAND_DISPLAY").ok()?;
    let mut md5 = Command::new("md5sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    md5.stdin.take()?.write_all(format!("{omarchy}/shell\n{display}").as_bytes()).ok()?;
    let out = md5.wait_with_output().ok()?;
    let hash = String::from_utf8_lossy(&out.stdout);
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)?;
    Some(dir.join(format!("omarchy-shell-{}.sock", hash.get(..16)?)))
}

fn aside(path: &PathBuf) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".quickshell");
    path.with_file_name(name)
}

/// Gives the socket back to Quickshell, or removes ours if there was none to give back to.
pub fn release() {
    let Some(path) = socket() else { return };
    let kept = aside(&path);
    if kept.exists() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(&kept, &path);
    } else if ALONE.load(std::sync::atomic::Ordering::Relaxed) {
        // Only a socket that is ours: one that appeared since is Quickshell's.
        let _ = std::fs::remove_file(&path);
    }
}

/// Whether the socket was bound with no Quickshell to give it back to.
static ALONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Takes the socket over. Call once, from the root, after what answers has been installed. With
/// Quickshell running its socket is moved aside and calls we do not answer are passed on to it;
/// without it (the shell that Omarchy talks to is this one) the socket is ours alone.
pub fn serve() {
    let Some(path) = socket() else { return };
    let kept = aside(&path);
    // What an earlier shell left (it died): ours is stale, Quickshell's is at `kept`.
    if kept.exists() && UnixStream::connect(&path).is_err() {
        release();
    }
    let quickshell = path.exists() && UnixStream::connect(&path).is_ok();
    if quickshell {
        if kept.exists() || std::fs::rename(&path, &kept).is_err() {
            return;
        }
    } else {
        // A socket nobody listens on, or none.
        let _ = std::fs::remove_file(&path);
        ALONE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(_) => {
            release();
            return;
        }
    };
    let to_ui = ui_channel(
        move |(fields, answer): (Vec<String>, std::sync::mpsc::Sender<Option<String>>)| {
            let _ = answer.send(handle(&fields));
        },
    );
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 omarchy".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let to_ui = to_ui.clone();
                let kept = kept.clone();
                std::thread::spawn(move || serve_one(stream, &kept, &to_ui));
            }
        });
}

fn serve_one(
    mut stream: UnixStream,
    kept: &PathBuf,
    to_ui: &UiSender<(Vec<String>, std::sync::mpsc::Sender<Option<String>>)>,
) {
    // The request, up to its record separator.
    let mut request = Vec::new();
    let mut byte = [0u8; 1];
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    while stream.read(&mut byte).is_ok_and(|n| n == 1) {
        if byte[0] == RS {
            break;
        }
        request.push(byte[0]);
    }
    let text = String::from_utf8_lossy(&request).into_owned();
    let fields: Vec<String> = text.split(US).map(str::to_owned).collect();
    let (tx, rx) = std::sync::mpsc::channel();
    to_ui.send((fields, tx));
    match rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Some(output)) => {
            let _ = stream.write_all(format!("OK{US}{output}").as_bytes());
            let _ = stream.write_all(&[RS]);
        }
        // Not ours: Quickshell answers, whatever it says.
        _ => {
            let reply = UnixStream::connect(kept).ok().and_then(|mut q| {
                q.write_all(&request).ok()?;
                q.write_all(&[RS]).ok()?;
                let _ = q.set_read_timeout(Some(Duration::from_secs(86400)));
                let mut reply = Vec::new();
                let mut one = [0u8; 1];
                while q.read(&mut one).is_ok_and(|n| n == 1) {
                    reply.push(one[0]);
                    if one[0] == RS {
                        break;
                    }
                }
                Some(reply)
            });
            let _ = stream.write_all(&reply.unwrap_or_else(|| vec![b'S', b'K', b'I', b'P', RS]));
        }
    }
}

/// Runs on the UI thread. `Some(output)` if the call was ours.
fn handle(fields: &[String]) -> Option<String> {
    let [target, method, args @ ..] = fields else { return None };
    match (target.as_str(), method.as_str()) {
        ("media", m @ ("next" | "previous" | "playPause" | "play" | "pause")) => {
            let action = osd::Action::parse(match m {
                "playPause" => "play-pause",
                other => other,
            })?;
            Some(match osd::media_action(action) {
                Ok(()) => "ok".into(),
                Err(e) => e,
            })
        }
        ("osd", "show") => {
            osd::notice(args.first()?);
            Some("ok".into())
        }
        ("notifications", method) => notifications(method, args),
        _ => None,
    }
}

/// `notifications.*`, on what is on the screen (the cards), as Omarchy's shell does.
fn notifications(method: &str, args: &[String]) -> Option<String> {
    let server = crate::notify::server()?;
    let on_screen = server.popups();
    let newest = on_screen.first();
    let done = |yes: bool| Some(if yes { "ok" } else { "none" }.to_owned());
    // What clicking the card does: its default action, else its only one, else its first.
    let click = |n: &anti_webui::shell::Notification| {
        let key = n
            .actions
            .iter()
            .find(|a| a.0 == "default")
            .or_else(|| n.actions.first())
            .map(|a| a.0.clone());
        match key {
            Some(key) => n.invoke(&key),
            None => n.dismiss(),
        }
    };
    match method {
        // Dismiss, press the one button on, or click the newest card.
        "act" | "invokeLast" => newest.map(|n| click(n)).map_or_else(|| done(false), |()| done(true)),
        "dismissOne" => newest.map(|n| n.dismiss()).map_or_else(|| done(false), |()| done(true)),
        "dismissAll" => {
            server.clear();
            done(true)
        }
        "invokeAction" => {
            let n: usize = args.first()?.trim().parse().ok()?;
            let action = newest?.actions.get(n.checked_sub(1)?)?.0.clone();
            newest?.invoke(&action);
            done(true)
        }
        "dismiss" => {
            let needle = args.first()?.as_str();
            let hit: Vec<_> = on_screen.iter().filter(|n| n.summary.contains(needle)).collect();
            hit.iter().for_each(|n| n.dismiss());
            done(!needle.is_empty() && !hit.is_empty())
        }
        // The history is the list in quick settings.
        "showHistory" => {
            let output = crate::ipc::focused_output()?;
            let panel = crate::panels::lookup("quick-settings")?;
            crate::panels::open(&output, panel);
            done(true)
        }
        "count" => Some(on_screen.len().to_string()),
        "isDnd" => Some(if server.do_not_disturb() { "on" } else { "off" }.to_owned()),
        "toggleDnd" => {
            server.set_do_not_disturb(!server.do_not_disturb());
            Some(if server.do_not_disturb() { "on" } else { "off" }.to_owned())
        }
        "setDnd" => {
            let v = args.first()?.to_lowercase();
            server.set_do_not_disturb(matches!(v.as_str(), "true" | "1" | "on" | "yes"));
            Some(if server.do_not_disturb() { "on" } else { "off" }.to_owned())
        }
        // Keyboard control of the cards and expanding the stack have no equivalent here.
        "keyboard" | "expand" => done(false),
        "ping" | "probe" => Some("ok".to_owned()),
        _ => None,
    }
}
