//! Changes to the stock interface. The pieces live next to this file, one module each.

mod agents;
mod animation;
mod colour;
mod menu;
mod omarchy_menu;
mod image_picker;
mod meter;
#[path = "music-player.rs"]
mod music_player;
mod tray;

use shoji_default::Ui;

pub fn patch(ui: &mut Ui) {
    // The tray as a `⌄` drop-down with a scrollable 3x3 grid (see tray.rs).
    tray::install(ui);
    // A menu of palettes in the quick settings panel (see menu.rs, colour.rs and palettes/).
    menu::install(ui);
    // Omarchy's menu and its select/input prompts (see omarchy_menu.rs).
    omarchy_menu::install(ui);
    image_picker::install(ui);
    // How colours change, and the pane behind the bar (see animation.rs).
    animation::install(ui);
    // The music panel with the blurred cover behind it (see music-player.rs).
    ui.replace_panel(music_player::panel());
}
