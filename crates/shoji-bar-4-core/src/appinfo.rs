//! Which installed application a window belongs to, and the icons of both.

use anti_webui::prelude::ImageSource;
use anti_webui::shell::TrayIcon;
use anti_webui::shell::icon_path;
use anti_webui::shell::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The application a window's `app_id` stands for: its desktop entry by id, or by the last part
/// of a reverse-DNS id, or by name.
pub fn app_for(app_id: &str) -> Option<Application> {
    if app_id.is_empty() {
        return None;
    }
    let all = apps().all();
    let lower = app_id.to_lowercase();
    let last = |id: &str| id.rsplit('.').next().unwrap_or(id).to_lowercase();
    all.iter()
        .find(|a| bare(&a.id) == app_id)
        .or_else(|| all.iter().find(|a| bare(&a.id).to_lowercase() == lower))
        .or_else(|| all.iter().find(|a| last(bare(&a.id)) == last(&lower)))
        .or_else(|| all.iter().find(|a| a.name.to_lowercase() == lower))
        .cloned()
}

/// A desktop entry id without `.desktop`.
pub fn bare(id: &str) -> &str {
    id.strip_suffix(".desktop").unwrap_or(id)
}

/// An installed application by its desktop entry id, with or without `.desktop`.
pub fn app_by_id(id: &str) -> Option<Application> {
    let all = apps();
    all.get(id)
        .or_else(|| all.get(&format!("{}.desktop", bare(id))))
}

thread_local! {
    static ICONS: RefCell<HashMap<(String, u32), Option<PathBuf>>> = RefCell::default();
}

/// The file of an icon by its name, looked up in the icon theme once.
pub fn icon_file(name: &str, size: u32) -> Option<PathBuf> {
    ICONS.with(|icons| {
        icons
            .borrow_mut()
            .entry((name.to_owned(), size))
            .or_insert_with(|| icon_path(name, size))
            .clone()
    })
}

/// A tray item's named icon: an absolute file, an application-supplied directory,
/// or the desktop icon theme. Per-item directories must not share the name-only cache.
/// Never an XPM file, which images aren't decoded from: the pixmap or the initial goes in its place.
pub fn tray_icon_file(icon: &TrayIcon, size: u32) -> Option<PathBuf> {
    let name = icon.name.as_deref().filter(|name| !name.is_empty())?;
    let path = Path::new(name);
    if path.is_absolute() {
        return (path.is_file() && !is_xpm(path)).then(|| path.to_owned());
    }
    if let Some(directory) = icon.theme_path.as_deref().filter(|path| !path.is_empty()) {
        let directory = Path::new(directory);
        let path = directory.join(name);
        if path.is_file() && !is_xpm(&path) {
            return Some(path);
        }
        for extension in ["png", "svg"] {
            let path = directory.join(format!("{name}.{extension}"));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    icon_file(name, size)
}

fn is_xpm(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xpm"))
}

thread_local! {
    static SYMBOLIC: RefCell<HashMap<(PathBuf, u32), Option<ImageSource>>> = RefCell::default();
}

/// A tray item's picture from its named icon. A symbolic icon comes in `color` (`0xRRGGBB`), as
/// GTK draws them in the text's, rather than in the dark gray it is drawn in for light themes.
pub fn tray_icon_image(icon: &TrayIcon, size: u32, color: u32) -> Option<ImageSource> {
    let path = tray_icon_file(icon, size)?;
    let symbolic = path.extension().is_some_and(|e| e == "svg")
        && path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.ends_with("-symbolic"));
    if !symbolic {
        return Some(path.into());
    }
    let recolored = SYMBOLIC.with(|cache| {
        cache
            .borrow_mut()
            .entry((path.clone(), color))
            .or_insert_with(|| {
                let svg = std::fs::read_to_string(&path).ok()?;
                Some(in_color(&svg, &format!("#{color:06x}")).into_bytes().into())
            })
            .clone()
    });
    Some(recolored.unwrap_or_else(|| path.into()))
}

/// `svg` with every color it paints with, as an attribute or in a style, made `color`, and that
/// also for what it paints without saying how. `none` and gradients stay as they are. `currentColor`
/// is made `color` too: in a file it would be black, not the text's.
fn in_color(svg: &str, color: &str) -> String {
    let mut out = String::with_capacity(svg.len() + 24);
    let mut i = 0;
    while i < svg.len() {
        let rest = &svg[i..];
        let starts_word = svg[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '-'));
        let key = ["fill", "stroke"].into_iter().find(|k| rest.starts_with(k));
        if let (Some(key), true) = (key, starts_word) {
            let after = &rest[key.len()..];
            let (open, close): (usize, &[char]) = if after.starts_with("=\"") {
                (2, &['"'])
            } else if after.starts_with("='") {
                (2, &['\''])
            } else if after.starts_with(':') {
                (1, &[';', '"', '\'', '}'])
            } else {
                (0, &[])
            };
            if open > 0 {
                let start = key.len() + open;
                let value = &rest[start..];
                let len = value.find(close).unwrap_or(value.len());
                let v = value[..len].trim();
                let kept = v.is_empty() || v == "none" || v.starts_with("url(") || v == "inherit";
                out.push_str(&rest[..start]);
                out.push_str(if kept { &value[..len] } else { color });
                i += start + len;
                continue;
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        i += c.len_utf8();
    }
    // What says nothing takes the root's.
    if let Some(at) = out.find("<svg") {
        let end = out[at..].find('>').map_or(out.len(), |e| at + e);
        if !out[at..end].contains(" fill=") {
            out.insert_str(at + 4, &format!(" fill=\"{color}\""));
        }
    }
    out
}

/// The icon of an application.
pub fn app_icon(app: &Application, size: u32) -> Option<PathBuf> {
    icon_file(app.icon.as_deref()?, size)
}

/// The icon of a window with this `app_id`: its application's, else one named like the `app_id`.
pub fn app_id_icon(app_id: &str, size: u32) -> Option<PathBuf> {
    app_for(app_id)
        .and_then(|a| app_icon(&a, size))
        .or_else(|| icon_file(app_id, size))
        .or_else(|| icon_file(&app_id.to_lowercase(), size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn tray_names_resolve_absolute_files_and_private_directory_icons() {
        let directory = tempfile::tempdir().unwrap();
        let png = directory.path().join("tray.png");
        let svg = directory.path().join("tray-symbolic.svg");
        fs::write(&png, []).unwrap();
        fs::write(&svg, []).unwrap();
        let mut icon = TrayIcon {
            name: Some(png.to_str().unwrap().to_owned()),
            ..Default::default()
        };
        assert_eq!(tray_icon_file(&icon, 48), Some(png.clone()));
        icon.theme_path = Some(directory.path().to_str().unwrap().to_owned());
        icon.name = Some("tray".to_owned());
        assert_eq!(tray_icon_file(&icon, 48), Some(png));
        icon.name = Some("tray-symbolic.svg".to_owned());
        assert_eq!(tray_icon_file(&icon, 48), Some(svg));
    }

    #[test]
    fn tray_icons_are_never_xpm() {
        let directory = tempfile::tempdir().unwrap();
        let xpm = directory.path().join("old.xpm");
        fs::write(&xpm, []).unwrap();
        let mut icon = TrayIcon {
            name: Some(xpm.to_str().unwrap().to_owned()),
            ..Default::default()
        };
        assert_eq!(tray_icon_file(&icon, 48), None);
        icon.theme_path = Some(directory.path().to_str().unwrap().to_owned());
        icon.name = Some("old".to_owned());
        assert_eq!(tray_icon_file(&icon, 48), None);
        icon.name = Some("old.xpm".to_owned());
        assert_eq!(tray_icon_file(&icon, 48), None);
    }

    #[test]
    fn symbolic_icons_take_the_color_given() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="#2e3436" d="M0 0"/><rect style="fill:currentColor;fill-opacity:0.5" stroke='red'/><circle fill="none" stroke="url(#g)"/><path d="M1 1"/></svg>"##;
        assert_eq!(
            in_color(svg, "#e3e2e6"),
            r##"<svg fill="#e3e2e6" xmlns="http://www.w3.org/2000/svg" width="16" height="16"><path fill="#e3e2e6" d="M0 0"/><rect style="fill:#e3e2e6;fill-opacity:0.5" stroke='#e3e2e6'/><circle fill="none" stroke="url(#g)"/><path d="M1 1"/></svg>"##
        );
        // A root that says its own isn't given another.
        assert_eq!(
            in_color(r##"<svg fill="#000"><path/></svg>"##, "#fff"),
            r##"<svg fill="#fff"><path/></svg>"##
        );
    }

    #[test]
    fn tray_directory_changes_and_new_files_are_not_hidden_by_the_cache() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        // Use a unique icon name so the test cannot pick an installed theme icon.
        let name = format!(
            "shoji-tray-{}",
            first.path().file_name().unwrap().to_str().unwrap()
        );
        let first_icon = first.path().join(format!("{name}.png"));
        let second_icon = second.path().join(format!("{name}.png"));
        fs::write(&first_icon, []).unwrap();
        let mut icon = TrayIcon {
            name: Some(name),
            theme_path: Some(first.path().to_str().unwrap().to_owned()),
            ..Default::default()
        };
        assert_eq!(tray_icon_file(&icon, 48), Some(first_icon));
        icon.theme_path = Some(second.path().to_str().unwrap().to_owned());
        assert_eq!(tray_icon_file(&icon, 48), None);
        fs::write(&second_icon, []).unwrap();
        assert_eq!(tray_icon_file(&icon, 48), Some(second_icon));
    }
}
