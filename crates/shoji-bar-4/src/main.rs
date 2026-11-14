//! shoji-bar-4: a Material 3 Expressive desktop shell for ShojiWM.

use shoji_bar_4_core::{dock, ipc, lock, osd, preview, shell_layer, theme, wallpaper, wm};
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
            shell_layer::wayland::open_popups();
            osd::install();
            lock::install();
            for_each_output(|output| {
                shell_layer::wayland::open_bar(output.clone());
                dock::open(output.clone());
                wallpaper::open(output.clone());
                shell_layer::wayland::install_panels(output);
            });
        });
    if let Err(e) = result {
        eprintln!("shoji-bar-4: {e}");
        std::process::exit(1);
    }
}
