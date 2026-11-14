//! The battery in quick settings: how full it is, as a battery that fills and a number that rolls,
//! how long until it is empty or full and about when that is, and how it is doing: the power going
//! in or out, its health and how many times it has been charged through.

use crate::quick_settings::section;
use crate::settings::settings;
use crate::theme::{TextStyle, colors};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui::shell::{Battery, BatteryState};
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::{default_spatial, fast_spatial};
use std::time::Duration;

/// At or under this, running down, it shows in the error color.
const LOW: f64 = 20.0;

/// What it is doing, in a word and an icon.
fn state_of(b: Battery) -> (&'static Symbol, &'static str) {
    match b.state() {
        BatteryState::Charging => (icons::BOLT, "Charging"),
        BatteryState::Full => (icons::CHECK_CIRCLE, "Full"),
        // Plugged in, kept from charging, like at a charge limit.
        BatteryState::PendingCharge => (icons::POWER, "Not charging"),
        _ if !b.on_battery() => (icons::POWER, "Plugged in"),
        _ => (icons::BATTERY_FULL, "On battery"),
    }
}

/// `1 h 20 min`, `45 min`.
fn span(d: Duration) -> String {
    let minutes = (d.as_secs() + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, 0) => "less than a minute".into(),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// The time of day `d` from now, as the clock in the bar reads, within the day after.
fn at(d: Duration) -> Option<String> {
    let now = local_time(Every::Minute);
    let minutes = d.as_secs() / 60;
    if minutes >= 24 * 60 {
        return None;
    }
    let then = (now.hour as u64 * 60 + now.minute as u64 + minutes) % (24 * 60);
    let (h, m) = (then / 60, then % 60);
    Some(if settings().with(|s| s.clock_24h) {
        format!("{h:02}:{m:02}")
    } else {
        let half = if h < 12 { "AM" } else { "PM" };
        format!("{}:{m:02} {half}", (h + 11) % 12 + 1)
    })
}

/// What is ahead: how long until empty or full, and about when.
fn estimate(b: Battery) -> String {
    let with_time = |d: Duration, what: &str, when: &str| match at(d) {
        Some(t) => format!("{} {what} · {when} {t}", span(d)),
        None => format!("{} {what}", span(d)),
    };
    match b.state() {
        BatteryState::Full => "Fully charged".into(),
        BatteryState::PendingCharge => format!("Plugged in, held at {:.0}%", b.percentage()),
        BatteryState::Charging => match b.time_to_full() {
            Some(d) => with_time(d, "until full", "full at"),
            None => "Working out how long until full".into(),
        },
        _ if !b.on_battery() => "Plugged in".into(),
        _ => match b.time_to_empty() {
            Some(d) => with_time(d, "left", "until about"),
            None => "Working out how long it lasts".into(),
        },
    }
}

pub fn card() -> Div {
    let b = battery();
    let level = follow(move || b.percentage() as f32 / 100.0, default_spatial());
    let charging = memo(move || b.state() == BatteryState::Charging);
    let low = memo(move || !b.charging() && b.percentage() <= LOW);
    let state = memo(move || state_of(b).1);
    section()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(14.0_f32)
                .child(glyph(level, charging, low))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_end()
                        .flex_1()
                        .child(rolling_text(
                            move || format!("{:.0}", b.percentage()),
                            TextStyle::HEADLINE_MEDIUM,
                            |_| 16.0,
                        ))
                        .child(
                            static_txt(TextStyle::TITLE_MEDIUM, "%")
                                .pb(4.0_f32)
                                .pl(2.0_f32)
                                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                        ),
                )
                .child(state_pill(b, state, low)),
        )
        .child(
            txt(TextStyle::BODY_MEDIUM, move || estimate(b))
                .px(2.0_f32)
                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
        )
        .child(stats(b))
}

/// A battery on its side that fills to the level, in the error color when low; while charging,
/// light runs along what is in it.
fn glyph(level: Animated<f32>, charging: Memo<bool>, low: Memo<bool>) -> Div {
    const W: f32 = 52.0;
    const H: f32 = 30.0;
    const INNER: f32 = W - 8.0;
    let shine = clock_while(move || charging.get());
    let lit = follow(
        move || if charging.get() { 1.0f32 } else { 0.0 },
        fast_spatial(),
    );
    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .child(
            div()
                .w(W)
                .h(H)
                .p(2.0_f32)
                .rounded(9.0_f32)
                .border(2.0_f32)
                .dyn_style(|s| s.border_color(colors().outline))
                .child(
                    div()
                        .h_full()
                        .relative()
                        .overflow_hidden()
                        .rounded(5.0_f32)
                        .transition_of([Prop::Background], Transition::ms(300).ease_out())
                        .dyn_style(move |s| {
                            let c = colors();
                            s.w((INNER * level.get().clamp(0.0, 1.0)).max(4.0))
                                .bg(if low.get() { c.error } else { c.primary })
                        })
                        .child(
                            div()
                                .absolute()
                                .top(0.0)
                                .bottom(0.0)
                                .w(14.0)
                                .pointer_events_none()
                                .dyn_style(move |s| {
                                    // Across and off the end every 1.6 s.
                                    let t = (shine.get() % 1.6) / 1.6;
                                    s.left(-14.0 + (INNER + 14.0) * t)
                                        .bg(colors().on_primary.with_alpha(0.35 * lit.get()))
                                }),
                        ),
                ),
        )
        .child(
            div()
                .w(3.0)
                .h(10.0)
                .ml(1.0)
                .rounded(1.5_f32)
                .dyn_style(|s| s.bg(colors().outline)),
        )
}

/// What it is doing, on a pill that changes its word in place.
fn state_pill(b: Battery, state: Memo<&'static str>, low: Memo<bool>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(4.0_f32)
        .h(28.0)
        .px(10.0_f32)
        .rounded(14.0_f32)
        .flex_none()
        .transition_of(
            [Prop::Background, Prop::TextColor],
            Transition::ms(250).ease_out(),
        )
        .dyn_style(move |s| {
            let c = colors();
            let (bg, fg) = if low.get() {
                (c.error_container, c.on_error_container)
            } else if b.charging() {
                (c.primary_container, c.on_primary_container)
            } else {
                (c.secondary_container, c.on_secondary_container)
            };
            s.bg(bg).text_color(fg)
        })
        .child(
            for_each(
                move || vec![state.get()],
                |s| *s,
                move |_| {
                    let (symbol, word) = untrack(|| state_of(b));
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(4.0_f32)
                        .child(anti_webui_md3e::components::sized_icon(symbol, 16.0))
                        .child(static_txt(TextStyle::LABEL_MEDIUM, word))
                },
            )
            .enter(|s| s.opacity(0.0_f32).translate_y(10.0_f32))
            .exit(|s| s.opacity(0.0_f32).translate_y(-10.0_f32))
            .transition(emphasized(350)),
        )
}

/// The power going in or out, the health and the cycles, as far as they are known.
fn stats(b: Battery) -> Div {
    let tile =
        |symbol: &'static Symbol, label: &'static str, value: Box<dyn Fn() -> Option<String>>| {
            let value = memo(move || value());
            show(
                move || value.with(Option::is_some),
                move || {
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(2.0_f32)
                        .px(12.0_f32)
                        .py(10.0_f32)
                        .rounded(16.0_f32)
                        .dyn_style(|s| s.bg(colors().surface_container_highest))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(4.0_f32)
                                .child(sym(symbol, 14.0, |c| c.primary))
                                .child(
                                    static_txt(TextStyle::LABEL_SMALL, label)
                                        .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                                ),
                        )
                        .child(txt(TextStyle::TITLE_SMALL, move || {
                            value.get().unwrap_or_default()
                        }))
                },
            )
            .appear()
            .enter(|s| s.opacity(0.0_f32).scale(0.9_f32))
            .exit(|s| s.opacity(0.0_f32).scale(0.9_f32))
            .transition(overshoot(360))
        };
    div()
        .flex()
        .flex_row()
        .gap(8.0_f32)
        .child(tile(
            icons::ELECTRIC_BOLT,
            "Power",
            Box::new(move || {
                let w = b.energy_rate();
                (w > 0.05).then(|| {
                    let sign = if b.state() == BatteryState::Charging {
                        "+"
                    } else {
                        "−"
                    };
                    format!("{sign}{w:.1} W")
                })
            }),
        ))
        .child(tile(
            icons::HEALTH_AND_SAFETY,
            "Health",
            Box::new(move || b.health().map(|h| format!("{h:.0}%"))),
        ))
        .child(tile(
            icons::AUTORENEW,
            "Cycles",
            Box::new(move || b.charge_cycles().map(|c| c.to_string())),
        ))
}
