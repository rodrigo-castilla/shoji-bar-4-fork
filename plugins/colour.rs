//! Colours: where the palettes are read from, which one is in use, and whether the bar has its
//! pane. Nothing here draws anything: `menu.rs` shows the choice in the quick settings panel and
//! `animation.rs` draws the pane and sets how a change of colours moves.
//!
//! A palette is a text file, `plugins/palettes/<name>.palette`, with one `key = value` per line
//! (see `red.palette` for what the keys are). The files are read when the panel opens and when the
//! shell starts, so adding or editing one needs no rebuild: open the panel again.
//!
//! `dark` and `light` are the stock themes; any other file with a seed or colour roles in it is put
//! in place of the scheme the settings make. The choices are kept in `~/.local/state/shoji-bar-4/`.
//!
//! For scripts (and tests) there is a command file: writing `palette <name>`, `bar on` or
//! `bar off` to `~/.local/state/shoji-bar-4/command` does that, within a fraction of a second.

use anti_webui::prelude::*;
use anti_webui_md3e::ColorScheme;
use shoji_core::settings::{Palette, ThemeMode, settings};
use shoji_core::theme::{current_seed, scheme_for, set_scheme_override};
use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

thread_local! {
    static CURRENT: Cell<Option<Signal<String>>> = const { Cell::new(None) };
    static LABEL: Cell<Option<Signal<String>>> = const { Cell::new(None) };
    static BAR_ON: Cell<Option<Signal<bool>>> = const { Cell::new(None) };
    static GLASS: Cell<Option<Signal<bool>>> = const { Cell::new(None) };
    static OPACITY: Cell<Option<Signal<f32>>> = const { Cell::new(None) };
}

fn sig<T: Clone + 'static>(slot: &'static std::thread::LocalKey<Cell<Option<Signal<T>>>>, init: T) -> Signal<T> {
    slot.with(|s| match s.get() {
        Some(x) => x,
        None => {
            let x = signal(init);
            s.set(Some(x));
            x
        }
    })
}

/// The name of the palette in use.
pub fn current() -> Signal<String> {
    sig(&CURRENT, String::new())
}

/// The label of the palette in use, for the tile.
pub fn label() -> Signal<String> {
    sig(&LABEL, String::new())
}

/// Whether the bar has its pane behind the islands.
pub fn bar_on() -> Signal<bool> {
    sig(&BAR_ON, true)
}

/// Whether the palette in use is see-through glass (one with a `blur`).
pub fn is_glass() -> Signal<bool> {
    sig(&GLASS, false)
}

/// How opaque the surfaces of the palette in use are, from `MIN_OPACITY` to 1 (the palette as it is).
pub fn opacity() -> Signal<f32> {
    sig(&OPACITY, 1.0)
}

/// The least opacity the slider gives: below this the text would have nothing to be read on.
pub const MIN_OPACITY: f32 = 0.1;

/// Brings back what was chosen last time, and starts listening for the command file.
pub fn restore() {
    bar_on().set(bar_saved());
    opacity().set(opacity_saved());
    current().set(saved().unwrap_or_else(|| "dark".to_owned()));
    // The one saved, or `dark` if it is gone (a file taken away).
    let mut all = list();
    let wanted = current().get_untracked();
    let at = all
        .iter()
        .position(|p| p.name == wanted)
        .or_else(|| all.iter().position(|p| p.name == "dark"));
    if let Some(p) = at.map(|i| all.swap_remove(i)) {
        current().set(p.name.clone());
        apply(&p);
    }
    poll_command();
}

fn poll_command() {
    if let Some(file) = state_file().map(|f| f.with_file_name("command")) {
        if let Ok(text) = fs::read_to_string(&file) {
            let _ = fs::remove_file(&file);
            let mut words = text.split_whitespace();
            match (words.next(), words.next()) {
                (Some("palette"), Some(name)) => {
                    if let Some(p) = list().into_iter().find(|p| p.name == name) {
                        choose(&p);
                    }
                }
                (Some("bar"), Some(v)) => set_bar(v == "on"),
                _ => {}
            }
        }
    }
    drop(set_timeout(Duration::from_millis(150), poll_command));
}

pub struct Entry {
    pub name: String,
    pub label: String,
    pub dark: bool,
    pub seed: Option<u32>,
    pub variant: Option<Palette>,
    pub roles: Vec<(String, Color)>,
    /// Frosted glass: the radius of the blur Hyprland puts behind the shell, in pixels (0: none).
    pub blur: f32,
}

impl Entry {
    /// Only says dark or light: the stock theme.
    pub fn is_stock(&self) -> bool {
        self.seed.is_none() && self.variant.is_none() && self.roles.is_empty() && self.blur == 0.0
    }
}

fn palettes_dir() -> PathBuf {
    std::env::var_os("SHOJI_PLUGINS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../plugins"))
        })
        .join("palettes")
}

pub fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("shoji-bar-4/palette"))
}

fn saved() -> Option<String> {
    Some(fs::read_to_string(state_file()?).ok()?.trim().to_owned()).filter(|n| !n.is_empty())
}

fn bar_file() -> Option<PathBuf> {
    Some(state_file()?.with_file_name("bar-pane"))
}

/// Whether the bar had its pane when the shell last ran: on unless it was switched off.
fn bar_saved() -> bool {
    bar_file()
        .and_then(|f| fs::read_to_string(f).ok())
        .is_none_or(|t| t.trim() != "off")
}

fn opacity_file() -> Option<PathBuf> {
    Some(state_file()?.with_file_name("opacity"))
}

fn opacity_saved() -> f32 {
    opacity_file()
        .and_then(|f| fs::read_to_string(f).ok())
        .and_then(|t| t.trim().parse::<f32>().ok())
        .map_or(1.0, |v| v.clamp(MIN_OPACITY, 1.0))
}

/// Changes the opacity of whatever palette is in use, with the fade, and remembers it.
pub fn set_opacity(value: f32) {
    let value = value.clamp(MIN_OPACITY, 1.0);
    if (opacity().get_untracked() - value).abs() < 0.005 {
        return;
    }
    opacity().set(value);
    if let Some(file) = opacity_file() {
        let _ = fs::create_dir_all(file.parent().unwrap());
        let _ = fs::write(file, value.to_string());
    }
    let wanted = current().get_untracked();
    if let Some(p) = list().into_iter().find(|p| p.name == wanted) {
        apply(&p);
    }
}

fn remember(name: &str) {
    if let Some(file) = state_file() {
        let _ = fs::create_dir_all(file.parent().unwrap());
        let _ = fs::write(file, name);
    }
}

/// `#rrggbb` or `#rrggbbaa`.
fn color(text: &str) -> Option<Color> {
    let hex = text.trim().strip_prefix('#')?;
    match hex.len() {
        6 => Some(rgb(u32::from_str_radix(hex, 16).ok()?)),
        8 => {
            let v = u32::from_str_radix(hex, 16).ok()?;
            Some(rgb(v >> 8).with_alpha((v & 0xff) as f32 / 255.0))
        }
        _ => None,
    }
}

fn parse(name: &str, text: &str) -> Entry {
    let mut entry = Entry {
        name: name.to_owned(),
        label: name.to_owned(),
        dark: true,
        seed: None,
        variant: None,
        roles: Vec::new(),
        blur: 0.0,
    };
    for raw in text.lines() {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        // A trailing comment is ` #` followed by a space; a colour's `#` is not preceded by one
        // and followed by a space.
        let raw = raw.split(" # ").next().unwrap_or(raw);
        let Some((key, value)) = raw.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "label" => entry.label = value.to_owned(),
            "blur" => entry.blur = value.parse().unwrap_or(0.0),
            "base" => entry.dark = value != "light",
            "seed" => entry.seed = color(value).map(hex_of),
            "variant" => entry.variant = Palette::ALL.into_iter().find(|p| p.name() == value),
            role => {
                if let Some(c) = color(value) {
                    entry.roles.push((role.to_owned(), c));
                }
            }
        }
    }
    entry
}

fn hex_of(c: Color) -> u32 {
    shoji_core::theme::hex_of(c)
}

/// The palettes there are: `dark` and `light` first, then the others by name.
pub fn list() -> Vec<Entry> {
    let mut all: Vec<Entry> = fs::read_dir(palettes_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "palette"))
        .filter_map(|e| {
            let name = e.path().file_stem()?.to_str()?.to_owned();
            Some(parse(&name, &fs::read_to_string(e.path()).ok()?))
        })
        .collect();
    all.sort_by_key(|e| (!matches!(e.name.as_str(), "dark" | "light"), e.name != "dark", e.name.clone()));
    all
}

macro_rules! set_role {
    ($scheme:ident, $key:expr, $value:expr; $($role:ident),* $(,)?) => {
        match $key {
            $(stringify!($role) => $scheme.$role = $value,)*
            _ => {}
        }
    };
}

fn scheme_of(entry: &Entry) -> ColorScheme {
    let s = settings().get_untracked();
    let seed = entry.seed.unwrap_or_else(|| current_seed(&s));
    let mut scheme = scheme_for(seed, entry.variant.unwrap_or(s.palette), entry.dark);
    for (key, c) in &entry.roles {
        set_role!(scheme, key.as_str(), *c;
            primary, on_primary, primary_container, on_primary_container,
            secondary, on_secondary, secondary_container, on_secondary_container,
            tertiary, on_tertiary, tertiary_container, on_tertiary_container,
            error, on_error, error_container, on_error_container,
            surface, surface_dim, surface_bright, surface_variant, on_surface, on_surface_variant,
            inverse_surface, inverse_on_surface, inverse_primary, scrim,
            surface_container_lowest, surface_container_low, surface_container,
            surface_container_high, surface_container_highest, outline, outline_variant);
    }
    let k = opacity().get_untracked();
    if k < 1.0 {
        let fade = |c: &mut Color| *c = c.with_alpha(c.a * k);
        for c in [
            &mut scheme.surface, &mut scheme.surface_dim, &mut scheme.surface_bright,
            &mut scheme.surface_variant, &mut scheme.surface_container_lowest,
            &mut scheme.surface_container_low, &mut scheme.surface_container,
            &mut scheme.surface_container_high, &mut scheme.surface_container_highest,
        ] {
            fade(c);
        }
    }
    scheme
}

/// Where whether Hyprland's blur was on before a palette turned it on is kept, so that it can be
/// put back even after the shell has restarted in between.
fn blur_before_file() -> Option<PathBuf> {
    Some(state_file()?.with_file_name("blur-before"))
}

/// A surface of the shell can only blur what is in it, not the windows and the wallpaper behind
/// it: that is the compositor's job. So a palette with `blur` asks Hyprland to blur behind the
/// shell's layers (what is see-through in them), and turns it off again for the other palettes if
/// it was off to begin with.
fn compositor_glass(radius: f32) {
    let hypr = |code: String| {
        let _ = std::process::Command::new("hyprctl").args(["eval", &code]).output();
    };
    if radius > 0.0 {
        if let Some(file) = blur_before_file().filter(|f| !f.exists()) {
            let on = std::process::Command::new("hyprctl")
                .args(["getoption", "decoration:blur:enabled"])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().is_some_and(|l| l.contains("true")))
                .unwrap_or(false);
            let _ = fs::create_dir_all(file.parent().unwrap());
            let _ = fs::write(file, on.to_string());
        }
        hypr(format!(
            "hl.layer_rule({{ name = 'shoji-glass', match = {{ namespace = '^shoji-bar-4-.*$' }}, \
             blur = true, ignore_alpha = 0.03 }}); \
             hl.config({{ decoration = {{ blur = {{ enabled = true, size = {radius}, passes = 3, \
             new_optimizations = true, xray = false, noise = 0.0, contrast = 1.0, \
             brightness = 1.0, vibrancy = 0.2 }} }} }})"
        ));
    } else if let Some(file) = blur_before_file().filter(|f| f.exists()) {
        let before = fs::read_to_string(&file).is_ok_and(|t| t.trim() == "true");
        let _ = fs::remove_file(file);
        // Back to Hyprland's own values for what the glass changed.
        hypr(format!(
            "hl.config({{ decoration = {{ blur = {{ enabled = {before}, size = 8, passes = 1, \
             noise = 0.0117, contrast = 0.8916, brightness = 1.0, vibrancy = 0.1696 }} }} }})"
        ));
    }
}


/// Puts the pane behind the islands of the bar, or takes it away.
pub fn toggle_bar() {
    set_bar(!bar_on().get_untracked());
}

pub fn set_bar(now: bool) {
    if bar_on().get_untracked() == now {
        return;
    }
    bar_on().set(now);
    if let Some(file) = bar_file() {
        let _ = fs::create_dir_all(file.parent().unwrap());
        let _ = fs::write(file, if now { "on" } else { "off" });
    }
}

/// Puts the palette in place, with the fade.
pub fn apply(entry: &Entry) {
    compositor_glass(entry.blur);
    label().set(entry.label.clone());
    is_glass().set(entry.blur > 0.0);
    if entry.is_stock() && opacity().get_untracked() >= 1.0 {
        set_scheme_override(None);
        settings().update(|s| s.mode = if entry.dark { ThemeMode::Dark } else { ThemeMode::Light });
    } else {
        set_scheme_override(Some(scheme_of(entry)));
    }
}

/// Picks a palette and remembers it.
pub fn choose(entry: &Entry) {
    apply(entry);
    remember(&entry.name);
    current().set(entry.name.clone());
}
