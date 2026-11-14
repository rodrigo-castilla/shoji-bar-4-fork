//! Commands from outside, such as the compositor's key bindings, over a socket in
//! `$XDG_RUNTIME_DIR`:
//!
//! ```text
//! shoji-bar-4 msg toggle launcher [OUTPUT]
//! shoji-bar-4 msg toggle quick-settings [OUTPUT]
//! shoji-bar-4 msg toggle calendar [OUTPUT]
//! shoji-bar-4 msg toggle clipboard [OUTPUT]
//! shoji-bar-4 msg toggle media [OUTPUT]
//! shoji-bar-4 msg close
//! shoji-bar-4 msg lock
//! shoji-bar-4 msg theme dark|light|auto|toggle
//! shoji-bar-4 msg quit
//! shoji-bar-4 msg status
//! ```
//!
//! Without an output, a panel opens where the window with the keyboard is.

use crate::panels::{self, Panel};
use crate::settings::{ThemeMode, settings};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

fn socket() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    dir.join(format!("shoji-bar-4.{display}.sock"))
}

/// Sends a command to the running shell and prints what it says.
pub fn send(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = UnixStream::connect(socket())?;
    writeln!(stream, "{}", args.join(" "))?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer)?;
    let answer = answer.trim();
    match answer.strip_prefix("ok") {
        Some(said) if said.is_empty() || said.starts_with(' ') => {
            if !said.is_empty() {
                println!("{}", said.trim_start());
            }
            Ok(())
        }
        _ => Err(answer.to_owned().into()),
    }
}

/// Listens for commands. Call once, from the root.
pub fn serve() {
    let path = socket();
    // Someone else listening means another shell is running; a file with no one behind it is left
    // over from one that died.
    if UnixStream::connect(&path).is_ok() {
        eprintln!("shoji-bar-4: another one is running at {}", path.display());
        std::process::exit(1);
    }
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("shoji-bar-4: no commands from outside: {e}");
            return;
        }
    };
    let to_ui = ui_channel(
        move |(line, answer): (String, std::sync::mpsc::Sender<String>)| {
            let _ = answer.send(match run(&line) {
                Ok(said) if said.is_empty() => "ok".into(),
                Ok(said) => format!("ok {said}"),
                Err(e) => e,
            });
        },
    );
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 ipc".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let (tx, rx) = std::sync::mpsc::channel();
                to_ui.send((line.trim().to_owned(), tx));
                let answer = rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap_or_else(|_| "no answer".into());
                let _ = writeln!(&stream, "{answer}");
            }
        });
}

/// What could keep the shell busy, on one line, for `scripts/cpu-report.sh`.
fn status() -> String {
    let panel = panels::current().map_or("none".into(), |(output, panel)| {
        format!("{panel:?}@{output}")
    });
    let playing = media()
        .current()
        .map_or("none".into(), |p| format!("{:?}", p.status));
    let notifications = crate::notify::server().map_or("off".into(), |s| {
        format!("{}/{}", s.popups().len(), s.notifications().len())
    });
    format!(
        "panel={panel} ticking={} media={playing} popups/notifications={notifications} \
         windows={} workspaces={} tray={} dark={}",
        crate::ui::ticking(),
        toplevels().all().len(),
        workspaces().all().len(),
        tray().items().len(),
        crate::theme::is_dark(),
    )
}

/// The output where the window with the keyboard is, else the first.
pub fn focused_output() -> Option<String> {
    toplevels()
        .active()
        .and_then(|t| t.outputs.first().cloned())
        .or_else(|| outputs().first().map(|o| o.name.clone()))
}

fn run(line: &str) -> Result<String, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        ["toggle" | "open", name, rest @ ..] => {
            let panel = Panel::from_name(name).ok_or_else(|| format!("no panel called {name}"))?;
            let output = rest
                .first()
                .map(|s| s.to_string())
                .or_else(focused_output)
                .ok_or("no output")?;
            let open_now = panels::current().is_some_and(|(o, p)| o == output && p == panel);
            if words[0] == "toggle" && open_now {
                panels::close();
            } else {
                panels::open(&output, panel);
            }
            Ok(String::new())
        }
        // There is no unlock: only the password does that.
        ["lock"] => {
            crate::lock::lock();
            Ok(String::new())
        }
        ["close"] => {
            panels::close();
            Ok(String::new())
        }
        ["theme", mode] => {
            let mode = match *mode {
                "dark" => ThemeMode::Dark,
                "light" => ThemeMode::Light,
                "auto" => ThemeMode::Auto,
                "toggle" if crate::theme::is_dark() => ThemeMode::Light,
                "toggle" => ThemeMode::Dark,
                other => return Err(format!("no theme called {other}")),
            };
            settings().update(|s| s.mode = mode);
            Ok(String::new())
        }
        ["status"] => Ok(status()),
        ["quit"] => {
            let _ = std::fs::remove_file(socket());
            quit();
            Ok(String::new())
        }
        _ => Err(format!("unknown command: {line}")),
    }
}
