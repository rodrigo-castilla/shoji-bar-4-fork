//! What the user chose, kept in `$XDG_STATE_HOME/shoji-bar-4/settings.json` and written back
//! whenever it changes.

use anti_webui::prelude::*;
use serde_json::{Value, json};
use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

/// Light, dark, or following the time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    /// Light from 7:00 to 19:00.
    Auto,
}

impl ThemeMode {
    fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Auto => "auto",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "auto" => Self::Auto,
            _ => return None,
        })
    }
}

/// How colorful the scheme made from the seed is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    TonalSpot,
    Vibrant,
    Expressive,
    Fidelity,
    Neutral,
    Monochrome,
}

impl Palette {
    pub const ALL: [Palette; 6] = [
        Self::TonalSpot,
        Self::Vibrant,
        Self::Expressive,
        Self::Fidelity,
        Self::Neutral,
        Self::Monochrome,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::TonalSpot => "tonal-spot",
            Self::Vibrant => "vibrant",
            Self::Expressive => "expressive",
            Self::Fidelity => "fidelity",
            Self::Neutral => "neutral",
            Self::Monochrome => "monochrome",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TonalSpot => "Tonal",
            Self::Vibrant => "Vibrant",
            Self::Expressive => "Expressive",
            Self::Fidelity => "Fidelity",
            Self::Neutral => "Neutral",
            Self::Monochrome => "Mono",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

/// Where the bar sits on each output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarPosition {
    Top,
    Left,
    Right,
}

impl BarPosition {
    pub const ALL: [BarPosition; 3] = [Self::Top, Self::Left, Self::Right];

    pub fn is_vertical(self) -> bool {
        self != Self::Top
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Left => "left",
            Self::Right => "right",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub mode: ThemeMode,
    /// The color the scheme is made from, as `0xRRGGBB`.
    pub seed: u32,
    /// Takes the seed from the wallpaper instead.
    pub seed_from_wallpaper: bool,
    pub palette: Palette,
    pub wallpaper: Option<PathBuf>,
    /// The desktop entry ids of the apps the dock always shows, in order.
    pub pinned: Vec<String>,
    pub bar_position: BarPosition,
    pub clock_24h: bool,
    pub clock_seconds: bool,
    /// The user's picture, a copy kept next to this file; `None` for the desktop's.
    pub avatar: Option<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            seed: 0x6750a4,
            seed_from_wallpaper: false,
            palette: Palette::TonalSpot,
            wallpaper: None,
            pinned: [
                "org.kde.dolphin",
                "google-chrome",
                "kitty",
                "com.microsoft.VSCode",
                "discord",
                "org.kde.kate",
            ]
            .map(String::from)
            .to_vec(),
            bar_position: BarPosition::Top,
            clock_24h: true,
            clock_seconds: false,
            avatar: None,
        }
    }
}

impl Settings {
    fn to_json(&self) -> Value {
        json!({
            "mode": self.mode.name(),
            "seed": format!("#{:06x}", self.seed),
            "seed_from_wallpaper": self.seed_from_wallpaper,
            "palette": self.palette.name(),
            "wallpaper": self.wallpaper.as_ref().map(|p| p.to_string_lossy().into_owned()),
            "pinned": self.pinned,
            "bar_position": self.bar_position.name(),
            "clock_24h": self.clock_24h,
            "clock_seconds": self.clock_seconds,
            "avatar": self.avatar.as_ref().map(|p| p.to_string_lossy().into_owned()),
        })
    }

    /// What is in the file over the defaults: a key that is missing or wrong keeps its default.
    fn from_json(v: &Value) -> Self {
        let mut s = Self::default();
        let str_of = |k: &str| v.get(k).and_then(Value::as_str);
        let bool_of = |k: &str| v.get(k).and_then(Value::as_bool);
        if let Some(m) = str_of("mode").and_then(ThemeMode::from_name) {
            s.mode = m;
        }
        if let Some(seed) =
            str_of("seed").and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok())
        {
            s.seed = seed & 0xffffff;
        }
        if let Some(b) = bool_of("seed_from_wallpaper") {
            s.seed_from_wallpaper = b;
        }
        if let Some(p) = str_of("palette").and_then(Palette::from_name) {
            s.palette = p;
        }
        s.wallpaper = str_of("wallpaper").map(PathBuf::from);
        s.avatar = str_of("avatar").map(PathBuf::from);
        if let Some(list) = v.get("pinned").and_then(Value::as_array) {
            s.pinned = list
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
        if let Some(p) = str_of("bar_position").and_then(BarPosition::from_name) {
            s.bar_position = p;
        }
        if let Some(b) = bool_of("clock_24h") {
            s.clock_24h = b;
        }
        if let Some(b) = bool_of("clock_seconds") {
            s.clock_seconds = b;
        }
        s
    }
}

fn path() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    state.join("shoji-bar-4").join("settings.json")
}

fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|v| Settings::from_json(&v))
        .unwrap_or_default()
}

fn save(settings: &Settings) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string_pretty(&settings.to_json()).unwrap_or_default();
    // Written next to it and moved over it, so that a crash never leaves half a file.
    let partial = path.with_extension("json.partial");
    if std::fs::write(&partial, text).is_ok() {
        let _ = std::fs::rename(&partial, &path);
    }
}

thread_local! {
    static SETTINGS: Cell<Option<Signal<Settings>>> = const { Cell::new(None) };
    static SAVING: Cell<bool> = const { Cell::new(true) };
}

/// Keeps changes from being written, for pictures of the shell.
pub fn never_save() {
    SAVING.with(|s| s.set(false));
}

/// The settings of the shell. Reading them tracks them; writing them saves them a moment later.
pub fn settings() -> Signal<Settings> {
    SETTINGS.with(|cell| {
        cell.get().unwrap_or_else(|| {
            let (s, _) = create_root(|| {
                let s = signal(load());
                // Saves once things have settled, not on every step of a slider.
                let pending: Cell<Option<Timeout>> = Cell::new(None);
                let first = Cell::new(true);
                effect(move || {
                    let now = s.get();
                    if first.replace(false) || !SAVING.with(Cell::get) {
                        return;
                    }
                    if let Some(t) = pending.take() {
                        t.cancel();
                    }
                    pending.set(Some(set_timeout(Duration::from_millis(400), move || {
                        save(&now)
                    })));
                });
                s
            });
            cell.set(Some(s));
            s
        })
    })
}
