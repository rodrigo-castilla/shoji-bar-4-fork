//! shoji-bar-4: a Material 3 Expressive desktop shell for ShojiWM.
//!
//! The components are in `shoji-bar-4-core`; this is where they are arranged: what the root of the
//! shell opens, and for each output.

use anti_webui::shell::prelude::*;
use shoji_bar_4_core::{dock, ipc, lock, osd, shell_layer, theme, wallpaper, wm};

fn main() {
    shoji_bar_4_core::run(|| {
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
}
