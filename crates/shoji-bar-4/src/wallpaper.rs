//! The wallpaper behind everything, when one is chosen. A new one fades in over the old one and
//! settles from a slight zoom; the colors of the shell can come from it.

use crate::settings::settings;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::path::PathBuf;

/// Where the pictures offered in quick settings are.
pub fn directory() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Pictures/wallpapers")
}

/// The pictures in [`directory`], by name.
pub fn pictures() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(directory())
        .map(|dir| {
            dir.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                        matches!(e.to_lowercase().as_str(), "jpg" | "jpeg" | "png" | "webp")
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

pub fn open(output: Output) {
    // Only there while there is one, so that whatever else draws the desktop can.
    let shown = memo(|| settings().with(|s| s.wallpaper.is_some()));
    effect(move || {
        if !shown.get() {
            return;
        }
        open_layer(
            LayerOptions::background()
                .output(&output)
                .namespace("shoji-bar-4-wallpaper"),
            view,
        );
    });
}

fn view() -> Div {
    let current = memo(|| settings().with(|s| s.wallpaper.clone()));
    div()
        .size_full()
        .relative()
        .overflow_hidden()
        .dyn_style(|s| s.bg(crate::theme::colors().surface_dim))
        .child(
            for_each(
                move || current.get().into_iter().collect::<Vec<_>>(),
                |p| p.clone(),
                |path| {
                    img(path)
                        .absolute()
                        .inset(0.0)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                },
            )
            .appear()
            .enter(|s| s.opacity(0.0_f32).scale(1.06_f32))
            .exit(|s| s.opacity(0.0_f32))
            .transition(Transition::ms(1100).cubic_bezier(0.2, 0.0, 0.0, 1.0)),
        )
}
