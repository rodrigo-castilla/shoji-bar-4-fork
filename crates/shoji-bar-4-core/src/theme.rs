//! The color scheme, made from the settings and moved to smoothly when they change, and the few
//! tokens of Material 3 that the components of `anti_webui_md3e` don't carry.

use crate::settings::{Palette, Settings, ThemeMode, settings};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::dynamic::seed_colors;
use anti_webui_md3e::motion::default_effects;
use anti_webui_md3e::{
    ColorScheme, SchemeOptions, SchemeVariant, set_color_scheme, set_color_scheme_animated,
};
use std::cell::Cell;
use std::path::Path;

pub use anti_webui_md3e::color_scheme as colors;
pub use anti_webui_md3e::typography::TextStyle;

/// The seeds offered in quick settings, with what they are called.
pub const SEEDS: [(u32, &str); 8] = [
    (0x6750a4, "Lilac"),
    (0x0061a4, "Ocean"),
    (0x006a6a, "Lagoon"),
    (0x3a6a1f, "Forest"),
    (0x7c5800, "Amber"),
    (0xa23f00, "Ember"),
    (0xb0255c, "Rose"),
    (0x5d5f73, "Slate"),
];

fn variant(p: Palette) -> SchemeVariant {
    match p {
        Palette::TonalSpot => SchemeVariant::TonalSpot,
        Palette::Vibrant => SchemeVariant::Vibrant,
        Palette::Expressive => SchemeVariant::Expressive,
        Palette::Fidelity => SchemeVariant::Fidelity,
        Palette::Neutral => SchemeVariant::Neutral,
        Palette::Monochrome => SchemeVariant::Monochrome,
    }
}

/// The scheme made from a seed.
pub fn scheme_for(seed: u32, palette: Palette, dark: bool) -> ColorScheme {
    ColorScheme::from_seed_with(
        rgb(seed),
        SchemeOptions {
            variant: variant(palette),
            dark,
            ..Default::default()
        },
    )
}

thread_local! {
    static DARK: Cell<Option<Memo<bool>>> = const { Cell::new(None) };
    static WALLPAPER_SEED: Cell<Option<Signal<Option<u32>>>> = const { Cell::new(None) };
}

/// The scheme the colors are going to, without the steps on the way there, for what can't follow
/// them as they change (a picture made in a color).
pub fn target_scheme() -> ColorScheme {
    let s = settings().get();
    scheme_for(current_seed(&s), s.palette, is_dark())
}

/// Whether the scheme is dark now.
pub fn is_dark() -> bool {
    DARK.with(|d| d.get()).is_some_and(|m| m.get())
}

/// The seed taken from the wallpaper, once it has been read.
pub fn wallpaper_seed() -> Option<u32> {
    WALLPAPER_SEED.with(|w| w.get()).and_then(|s| s.get())
}

/// The seed in use: the one chosen, or the wallpaper's.
pub fn current_seed(s: &Settings) -> u32 {
    if s.seed_from_wallpaper {
        wallpaper_seed().unwrap_or(s.seed)
    } else {
        s.seed
    }
}

/// Follows the settings, the time of day and the wallpaper. Call once, from the root.
pub fn install() {
    let dark = memo(move || match settings().with(|s| s.mode) {
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
        ThemeMode::Auto => {
            let t = local_time(Every::Minute);
            !(7..19).contains(&t.hour)
        }
    });
    DARK.with(|d| d.set(Some(dark)));
    let wallpaper_seed = signal(None::<u32>);
    WALLPAPER_SEED.with(|w| w.set(Some(wallpaper_seed)));

    // Reads the wallpaper's colors off the UI thread whenever the wallpaper changes.
    effect(move || {
        let Some(path) = settings().with(|s| s.wallpaper.clone()) else {
            wallpaper_seed.set(None);
            return;
        };
        task(move || seed_of_image(&path)).then(move |seed| wallpaper_seed.set(seed));
    });

    let first = Cell::new(true);
    effect(move || {
        let s = settings().get();
        let scheme = scheme_for(current_seed(&s), s.palette, dark.get());
        if first.replace(false) {
            set_color_scheme(scheme);
        } else {
            set_color_scheme_animated(scheme, default_effects());
        }
    });
}

/// The best seed in a picture, read from a small copy of it.
fn seed_of_image(path: &Path) -> Option<u32> {
    let small = anti_webui::core::images::decode_bitmap(path, (128, 128)).ok()?;
    let mut pixels = Vec::new();
    small.with_levels(|i, _, level| {
        if i == 0 {
            pixels = level.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect();
        }
    });
    let seeds = seed_colors(pixels, 1);
    seeds.first().map(|c| hex_of(*c))
}

/// The shadows of an elevation level of Material 3 (0 to 5): a key light and an ambient one.
pub fn elevation(level: u8) -> Vec<BoxShadow> {
    let (key, ambient) = match level {
        0 => return Vec::new(),
        1 => ((1.0, 2.0, 0.0), (1.0, 3.0, 1.0)),
        2 => ((1.0, 2.0, 0.0), (2.0, 6.0, 2.0)),
        3 => ((1.0, 3.0, 0.0), (4.0, 8.0, 3.0)),
        4 => ((2.0, 3.0, 0.0), (6.0, 10.0, 4.0)),
        _ => ((4.0, 4.0, 0.0), (8.0, 12.0, 6.0)),
    };
    let shadow = |(y, blur, spread): (f32, f32, f32), alpha: f32| BoxShadow {
        offset_x: 0.0,
        offset_y: y,
        blur,
        spread,
        color: rgba(0x00000000).with_alpha(alpha),
        inset: false,
    };
    vec![shadow(key, 0.30), shadow(ambient, 0.15)]
}
/// A color as `0xRRGGBB`.
pub fn hex_of(c: Color) -> u32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (byte(c.r) << 16) | (byte(c.g) << 8) | byte(c.b)
}
