//! shoji-bar-4 core: the components of the shell (the bar, the dock, the panels, the lock screen, the
//! notifications, the OSD) and the machinery under them (settings, theme, the ShojiWM link, the shell
//! layers). How they are arranged is the business of the binary, `shoji-bar-4`.

pub mod appinfo;
pub mod bar;
pub mod battery_card;
pub mod calendar;
pub mod clipboard;
pub mod dock;
pub mod ipc;
pub mod launcher;
pub mod lock;
pub mod media_panel;
pub mod notify;
pub mod osd;
pub mod pam;
pub mod panels;
pub mod placement;
pub mod power_dialog;
pub mod preview;
pub mod qs_pages;
pub mod qs_settings;
pub mod quick_settings;
pub mod run;
pub mod settings;
pub mod shell_layer;
pub mod theme;
pub mod ui;
pub mod user;
pub mod wallpaper;
pub mod wm;

pub use run::run;
