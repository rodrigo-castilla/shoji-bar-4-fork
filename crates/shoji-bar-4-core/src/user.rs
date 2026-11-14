//! Who is here: the user's name and picture. The picture is the one chosen in the settings page
//! of quick settings, else the desktop's (`~/.face.icon`, `~/.face`, or AccountsService's).

use crate::settings::settings;
use crate::theme::{TextStyle, colors};
use anti_webui::prelude::*;
use anti_webui_md3e::components::{MaterialShape, ShapeStyle, styled_shape};
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;
use std::path::PathBuf;

pub fn name() -> String {
    std::env::var("USER").unwrap_or_else(|_| "you".into())
}

/// The picture the desktop already has for the user, if any.
fn desktop_picture() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [
        home.as_ref().map(|h| h.join(".face.icon")),
        home.as_ref().map(|h| h.join(".face")),
        Some(PathBuf::from("/var/lib/AccountsService/icons").join(name())),
    ]
    .into_iter()
    .flatten()
    .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0))
}

/// The user's picture, following the settings.
pub fn picture() -> Option<PathBuf> {
    settings()
        .with(|s| s.avatar.clone())
        .filter(|p| p.is_file())
        .or_else(desktop_picture)
}

/// Where chosen pictures are kept, so that moving the original doesn't lose it.
fn kept_dir() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(std::env::temp_dir)
        .join("shoji-bar-4")
}

thread_local! {
    /// Takes what the file chooser said to the UI thread; made once, outside any panel, which is
    /// closed while the chooser is open.
    static CHOSEN: Cell<Option<UiSender<Option<PathBuf>>>> = const { Cell::new(None) };
}

/// Asks for a picture with a file chooser, keeps a copy of it and makes it the user's. `after`
/// runs on the UI thread once it is chosen.
pub fn choose_picture(after: fn()) {
    let sender = CHOSEN.with(|c| {
        let s = c.take().unwrap_or_else(|| {
            create_root(|| {
                ui_channel(move |chosen: Option<PathBuf>| {
                    if let Some(path) = chosen {
                        settings().update(|s| s.avatar = Some(path));
                    }
                    if let Some(f) = AFTER.with(Cell::take) {
                        f();
                    }
                })
            })
            .0
        });
        c.set(Some(s.clone()));
        s
    });
    AFTER.with(|a| a.set(Some(after)));
    let _ = std::thread::Builder::new()
        .name("shoji-bar-4 picture".into())
        .spawn(move || sender.send(pick_and_keep()));
}

thread_local! {
    static AFTER: Cell<Option<fn()>> = const { Cell::new(None) };
}

fn pick_and_keep() -> Option<PathBuf> {
    let out = std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--title=Choose a picture",
            "--file-filter=Pictures | *.png *.jpg *.jpeg *.webp *.gif *.bmp *.svg",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let from = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    let dir = kept_dir();
    std::fs::create_dir_all(&dir).ok()?;
    // A new name each time: pictures are cached by where they are.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let ext = from
        .extension()
        .map_or("png".into(), |e| e.to_string_lossy().to_lowercase());
    let to = dir.join(format!("avatar-{stamp}.{ext}"));
    std::fs::copy(&from, &to).ok()?;
    // The ones chosen before.
    if let Ok(list) = std::fs::read_dir(&dir) {
        for entry in list.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("avatar-") && path != to {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    Some(to)
}

/// Goes back to the desktop's picture, or the initial.
pub fn forget_picture() {
    if let Some(path) = settings().with_untracked(|s| s.avatar.clone()) {
        let _ = std::fs::remove_file(path);
    }
    settings().update(|s| s.avatar = None);
}

/// The user's picture, or first letter, on a sunny shape that turns when the pointer is on it. A
/// new picture comes in over the old one.
pub fn avatar(size: f32) -> Div {
    let hovered = signal(false);
    let hover = follow(
        move || if hovered.get() { 1.0f32 } else { 0.0 },
        fast_spatial(),
    );
    let initial = name()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    let letter = if size >= 56.0 {
        TextStyle::HEADLINE_MEDIUM
    } else {
        TextStyle::TITLE_MEDIUM
    };
    let shown = memo(picture);
    div()
        .size(size)
        .relative()
        .flex_none()
        .on_hover(move |h| hovered.set(h))
        .child(
            styled_shape(
                move || {
                    if hovered.get() {
                        MaterialShape::SoftBurst
                    } else {
                        MaterialShape::Cookie12Sided
                    }
                },
                ShapeStyle::new()
                    .color(|c| c.tertiary_container)
                    .rotation(move || if hovered.get() { 60.0 } else { 0.0 }),
            )
            .absolute()
            .inset(0.0)
            .size_full(),
        )
        .child(
            for_each(
                move || vec![shown.get()],
                Clone::clone,
                move |picture| {
                    let face = div()
                        .absolute()
                        .inset(0.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .pointer_events_none();
                    match picture {
                        // Inside the points of the shape, growing a little under the pointer.
                        Some(path) => {
                            let inner = (size * 0.84).round();
                            face.child(
                                img(path)
                                    .size(inner)
                                    .rounded(inner / 2.0)
                                    .object_fit(ObjectFit::Cover)
                                    .dyn_style(move |s| s.scale(1.0 + 0.06 * hover.get())),
                            )
                        }
                        None => letter
                            .apply(face)
                            .dyn_style(|s| s.text_color(colors().on_tertiary_container))
                            .child(initial.clone()),
                    }
                },
            )
            .enter(|s| s.opacity(0.0_f32).scale(0.6_f32))
            .exit(|s| s.opacity(0.0_f32).scale(1.2_f32))
            .transition(crate::ui::overshoot(480)),
        )
}
