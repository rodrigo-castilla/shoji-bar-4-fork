//! Starting the shell: the subcommands that do not start one, and the shell itself.

use crate::{ipc, preview};
use anti_webui::shell::prelude::*;

/// Handles the subcommands that do not start a shell: `msg` talks to the one that is running and
/// `preview` draws a picture of a part of the interface. Whether there was one.
fn subcommand() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("msg") => {
            if let Err(e) = ipc::send(&args[1..]) {
                eprintln!("shoji-bar-4: {e}");
                std::process::exit(1);
            }
            true
        }
        Some("preview") => {
            if let Err(e) = preview::run(&args[1..]) {
                eprintln!("shoji-bar-4: {e}");
                std::process::exit(1);
            }
            true
        }
        _ => false,
    }
}

/// Runs the shell: handles the `msg` and `preview` subcommands, otherwise connects to the compositor
/// and runs `root` once, as the root of the interface, on the UI thread. What `root` opens (the bar,
/// the dock, the panels...) is what the shell shows.
pub fn run(root: impl FnOnce() + 'static) {
    if subcommand() {
        return;
    }
    let result = Shell::new()
        .namespace("shoji-bar-4")
        .app_id("shoji-bar-4")
        .run(root);
    if let Err(e) = result {
        eprintln!("shoji-bar-4: {e}");
        std::process::exit(1);
    }
}
