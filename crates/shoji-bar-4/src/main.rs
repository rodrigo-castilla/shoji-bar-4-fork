//! shoji-bar-4: a Material 3 Expressive desktop shell for ShojiWM.

mod appinfo;
mod bar;
mod battery_card;
mod calendar;
mod clipboard;
mod dock;
mod ipc;
mod launcher;
mod lock;
mod media_panel;
mod notify;
mod osd;
mod pam;
mod panels;
mod placement;
mod power_dialog;
mod preview;
mod qs_pages;
mod qs_settings;
mod quick_settings;
mod settings;
mod shell_layer;
mod theme;
mod ui;
mod user;
mod wallpaper;
mod wm;

use anti_webui::shell::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("msg") {
        if let Err(e) = ipc::send(&args[1..]) {
            eprintln!("shoji-bar-4: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.first().map(String::as_str) == Some("preview") {
        if let Err(e) = preview::run(&args[1..]) {
            eprintln!("shoji-bar-4: {e}");
            std::process::exit(1);
        }
        return;
    }
    let result = Shell::new()
        .namespace("shoji-bar-4")
        .app_id("shoji-bar-4")
        .run(|| {
            ipc::serve();
            theme::install();
            wm::share_theme();
            shell_layer::open_popups(shell_layer::Wayland);
            osd::install();
            lock::install();
            for_each_output(|output| {
                shell_layer::open_bar(output.clone(), shell_layer::Wayland);
                dock::open(output.clone());
                wallpaper::open(output.clone());
                shell_layer::install_panels(output, shell_layer::Wayland);
            });
        });
    if let Err(e) = result {
        eprintln!("shoji-bar-4: {e}");
        std::process::exit(1);
    }
}
