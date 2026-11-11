//! The default shoji-bar-4 user interface: the bar, the dock, the panels, the lock screen, the
//! notifications and the OSD, built on the foundation in [`shoji_core`].

// The stock components are public so that an interface of your own can reuse them. They are the
// stock interface's own parts rather than a stable API: expect them to change with it.

// The foundation lives in `shoji_core`; these keep `crate::theme::...` style paths working.
pub use shoji_core::{appinfo, pam, settings, theme, ui, wm};

pub mod bar;
pub mod battery_card;
pub mod calendar;
pub mod clipboard;
mod compose;
pub mod dock;
pub mod ipc;
pub mod launcher;
pub mod lock;
pub mod media_panel;
pub mod notify;
pub mod omarchy;
pub mod osd;
pub mod power_dialog;
pub mod panels;
mod preview;
pub mod qs_pages;
pub mod qs_settings;
pub mod quick_settings;
pub mod user;
pub mod wallpaper;

pub use bar::{BarLayout, Island};
pub use compose::{Services, Ui};

/// Handles the subcommands that do not start a shell (`msg` talks to the one that is running,
/// `preview` draws a picture of a part of it). Whether there was one.
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

/// Runs the shell the way the stock `shoji-bar-4` binary does: handles the `msg` and `preview`
/// subcommands, otherwise starts the shell with the stock interface.
pub fn run() {
    run_with(|_| {});
}

/// Like [`run`], but lets `patch` change the stock interface before it is put on the screen.
pub fn run_with(patch: impl FnOnce(&mut Ui)) {
    if subcommand() {
        return;
    }
    let mut ui = Ui::stock();
    patch(&mut ui);
    shoji_core::runtime::start(move || ui.install());
}

/// Like [`run`], but with an interface of your own: `root` runs as the root of the shell and puts
/// whatever it wants on the screen. The pieces of the stock interface are there to use
/// ([`Ui`], and the modules of this crate), but nothing is installed unless `root` does it.
pub fn run_custom(root: impl FnOnce() + 'static) {
    if subcommand() {
        return;
    }
    shoji_core::runtime::start(root);
}
