//! The settings of the shell, a page of quick settings behind its gear: who is here and their
//! picture, the colors, and the clock.

use crate::qs_pages::header;
use crate::quick_settings::{Page, open_at, section};
use crate::settings::{BarPosition, Palette, ThemeMode, settings};
use crate::theme::{SEEDS, TextStyle, colors, scheme_for, wallpaper_seed};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui_md3e::components::{
    ButtonSize, ButtonStyle, MaterialShape, ShapeStyle, connected_button_group, filter_chip,
    sized_icon, styled_button, styled_shape, switch,
};
use anti_webui_md3e::icons;
use anti_webui_md3e::motion::{default_effects, fast_spatial};

pub fn settings_page() -> Div {
    div()
        .flex()
        .flex_col()
        .gap(12.0_f32)
        .child(header("Settings", div()))
        .child(settle_in(1, account_section()))
        .child(settle_in(2, style_section()))
        .child(settle_in(3, bar_section()))
        .child(settle_in(4, clock_section()))
}

/// The heading of a section: an icon and its name.
fn heading(symbol: &'static icons::Symbol, title: &'static str) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(8.0_f32)
        .child(sym(symbol, 18.0, |c| c.primary))
        .child(static_txt(TextStyle::TITLE_SMALL, title))
}

/// The user's picture, and the ways to change it.
fn account_section() -> Div {
    let chosen = memo(|| settings().with(|s| s.avatar.is_some()));
    let said = move || {
        if chosen.get() {
            "Your picture"
        } else if crate::user::picture().is_some() {
            "The desktop's picture"
        } else {
            "No picture yet"
        }
        .to_owned()
    };
    section().child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(14.0_f32)
            .child(crate::user::avatar(64.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(0.0)
                    .gap(8.0_f32)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(static_txt(TextStyle::TITLE_MEDIUM, crate::user::name()))
                            .child(
                                txt(TextStyle::BODY_SMALL, said)
                                    .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(6.0_f32)
                            .child(
                                styled_button(
                                    "Change picture",
                                    ButtonStyle::tonal()
                                        .size(ButtonSize::ExtraSmall)
                                        .icon(icons::ADD_PHOTO_ALTERNATE),
                                )
                                // The chooser is a window, which the panel would cover; the
                                // panel comes back here once it is closed.
                                .on_click(|_| {
                                    crate::panels::close();
                                    crate::user::choose_picture(|| open_at(Page::Settings));
                                }),
                            )
                            .child(
                                show(
                                    move || chosen.get(),
                                    || {
                                        styled_button(
                                            "Remove",
                                            ButtonStyle::text().size(ButtonSize::ExtraSmall),
                                        )
                                        .on_click(|_| crate::user::forget_picture())
                                    },
                                )
                                .enter(|s| s.opacity(0.0_f32).scale(0.8_f32))
                                .exit(|s| s.opacity(0.0_f32).scale(0.8_f32))
                                .transition(overshoot(320)),
                            ),
                    ),
            ),
    )
}

/// Where the bar is shown.
fn bar_section() -> Div {
    let positions = BarPosition::ALL;
    section()
        .child(heading(icons::DESKTOP_WINDOWS, "Bar position"))
        .child(connected_button_group(
            positions.map(|p| p.label()),
            ButtonStyle::tonal(),
            move |i| settings().with(|s| s.bar_position == positions[i]),
            move |i| settings().update(|s| s.bar_position = positions[i]),
        ))
}

/// How the clock in the bar reads.
fn clock_section() -> Div {
    let row = |label: &'static str, on: Binding<bool>| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .px(4.0_f32)
            .child(static_txt(TextStyle::BODY_MEDIUM, label).flex_1())
            .child(switch(on))
    };
    section()
        .child(heading(icons::SCHEDULE, "Clock"))
        .child(row(
            "24-hour time",
            Binding::new(
                || settings().with(|s| s.clock_24h),
                |on| settings().update(|s| s.clock_24h = on),
            ),
        ))
        .child(row(
            "Show seconds",
            Binding::new(
                || settings().with(|s| s.clock_seconds),
                |on| settings().update(|s| s.clock_seconds = on),
            ),
        ))
}

/// The colors: seeds to choose from, how colorful a scheme is made from them, and light or dark.
fn style_section() -> Div {
    let mut swatches = div().flex().flex_row().justify_between();
    for (seed, name) in SEEDS {
        swatches = swatches.child(swatch(Some(seed), name));
    }
    let from_wallpaper = show(|| wallpaper_seed().is_some(), || swatch(None, "Wallpaper"));
    let chip = |p: Palette| {
        filter_chip(
            p.label(),
            Binding::new(
                move || settings().with(|s| s.palette == p),
                move |on| {
                    if on {
                        settings().update(|s| s.palette = p)
                    }
                },
            ),
        )
    };
    let mut chips_a = div().flex().flex_row().gap(6.0_f32);
    let mut chips_b = div().flex().flex_row().gap(6.0_f32);
    for (i, p) in Palette::ALL.into_iter().enumerate() {
        if i < 3 {
            chips_a = chips_a.child(chip(p));
        } else {
            chips_b = chips_b.child(chip(p));
        }
    }
    let modes = [ThemeMode::Light, ThemeMode::Auto, ThemeMode::Dark];
    section()
        .child(heading(icons::PALETTE, "Style & color"))
        .child(wallpapers())
        .child(swatches.child(from_wallpaper))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(6.0_f32)
                .child(chips_a)
                .child(chips_b),
        )
        .child(connected_button_group(
            ["Light", "Auto", "Dark"],
            ButtonStyle::tonal(),
            move |i| settings().with(|s| s.mode == modes[i]),
            move |i| settings().update(|s| s.mode = modes[i]),
        ))
}

/// The pictures to choose a wallpaper from. Choosing one takes the colors from it too.
fn wallpapers() -> Div {
    let pictures = crate::wallpaper::pictures();
    let mut row = div().flex().flex_row().gap(8.0_f32).pb(4.0_f32).overflow_x_scroll();
    for path in pictures {
        let chosen = memo({
            let path = path.clone();
            move || settings().with(|s| s.wallpaper.as_ref() == Some(&path))
        });
        let round = follow(move || if chosen.get() { 1.0 } else { 0.0 }, fast_spatial());
        let hovered = signal(false);
        let lift = follow(
            move || if hovered.get() { 1.0 } else { 0.0 },
            fast_spatial(),
        );
        let picked = path.clone();
        row = row.child(
            div()
                .w(80.0)
                .h(50.0)
                .flex_none()
                .relative()
                .on_hover(move |h| hovered.set(h))
                .on_click(move |_| {
                    let path = picked.clone();
                    settings().update(move |s| {
                        s.wallpaper = Some(path);
                        s.seed_from_wallpaper = true;
                    })
                })
                .dyn_style(move |s| s.scale(1.0 + 0.05 * lift.get()))
                .child(
                    div()
                        .size_full()
                        .overflow_hidden()
                        .dyn_style(move |s| s.rounded(10.0 + 18.0 * round.get()))
                        .child(img(path).size_full().object_fit(ObjectFit::Cover)),
                )
                .child(
                    div()
                        .absolute()
                        .inset(0.0)
                        .pointer_events_none()
                        .border(3.0_f32)
                        .dyn_style(move |s| {
                            let t = round.get();
                            s.rounded(10.0 + 18.0 * t)
                                .border_color(colors().primary.with_alpha(t.clamp(0.0, 1.0)))
                        }),
                ),
        );
    }
    row
}

/// A seed as a circle of its primary color with its two other accents in the middle; the chosen
/// one blooms into a cookie and shows a check. `None` is the wallpaper's.
fn swatch(seed: Option<u32>, name: &'static str) -> Div {
    let chosen = memo(move || {
        settings().with(|s| match seed {
            Some(seed) => !s.seed_from_wallpaper && s.seed == seed,
            None => s.seed_from_wallpaper,
        })
    });
    let hovered = signal(false);
    let lift = follow(
        move || if hovered.get() { 1.0 } else { 0.0 },
        fast_spatial(),
    );
    let scheme = memo(move || {
        let s = settings().get();
        let seed = seed.or_else(wallpaper_seed).unwrap_or(s.seed);
        scheme_for(seed, s.palette, crate::theme::is_dark())
    });
    let check = follow(
        move || if chosen.get() { 1.0 } else { 0.0 },
        default_effects(),
    );
    div()
        .size(30.0)
        .relative()
        .flex_none()
        .on_hover(move |h| hovered.set(h))
        .on_click(move |_| {
            settings().update(|s| match seed {
                Some(seed) => {
                    s.seed = seed;
                    s.seed_from_wallpaper = false;
                }
                None => s.seed_from_wallpaper = true,
            })
        })
        .access(move |a| a.label(name))
        .dyn_style(move |s| s.scale(1.0 + 0.1 * lift.get()))
        .child(
            styled_shape(
                move || {
                    if chosen.get() {
                        MaterialShape::Cookie9Sided
                    } else {
                        MaterialShape::Circle
                    }
                },
                ShapeStyle::new()
                    .color(move |_| scheme.get().primary)
                    .spring(fast_spatial())
                    .rotation(move || if chosen.get() { 40.0 } else { 0.0 }),
            )
            .absolute()
            .inset(0.0)
            .size_full(),
        )
        .child(
            div()
                .absolute()
                .inset(0.0)
                .flex()
                .items_center()
                .justify_center()
                .pointer_events_none()
                .child(
                    div()
                        .size(14.0)
                        .rounded(7.0_f32)
                        .overflow_hidden()
                        .flex()
                        .flex_row()
                        .dyn_style(move |s| {
                            s.opacity(1.0 - check.get()).scale(1.0 - 0.5 * check.get())
                        })
                        .child(
                            div()
                                .w(7.0)
                                .h_full()
                                .dyn_style(move |s| s.bg(scheme.get().secondary_container)),
                        )
                        .child(
                            div()
                                .w(7.0)
                                .h_full()
                                .dyn_style(move |s| s.bg(scheme.get().tertiary)),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .inset(0.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .dyn_style(move |s| {
                            let t = check.get();
                            s.text_color(scheme.get().on_primary)
                                .opacity(t)
                                .scale(0.4 + 0.6 * t)
                        })
                        .child(sized_icon(
                            if seed.is_some() {
                                icons::CHECK
                            } else {
                                icons::WALLPAPER
                            },
                            16.0,
                        )),
                ),
        )
}
