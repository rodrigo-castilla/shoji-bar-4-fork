//! The calendar under the clock: the time large, the month, and how busy the machine is.
//!
//! The month is its name between the ways to the months around it, the days of the week from
//! Monday, and six weeks of days, those of the months around it faint. Today stands on a cookie.

use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{
    Date, MaterialShape, ShapeStyle, circular_wavy_progress, styled_shape,
};
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;
use std::time::{Duration, Instant};

pub fn panel() -> Div {
    div()
        .flex()
        .flex_col()
        .gap(14.0_f32)
        .p(16.0_f32)
        .rounded(30.0_f32)
        .shadows(elevation(3))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container).text_color(c.on_surface)
        })
        .child(settle_in(0, header()))
        .child(settle_in(1, month_view()))
        // After the days, which come in over about a quarter of a second.
        .child(settle_in(6, gauges()))
}

/// The time with its seconds rolling, and the day.
fn header() -> Div {
    let big = TextStyle {
        size: 44.0,
        line_height: 52.0,
        letter_spacing: -0.5,
        weight: 400,
    };
    let small = TextStyle {
        size: 18.0,
        line_height: 24.0,
        letter_spacing: 0.0,
        weight: 500,
    };
    let width = |size: f32| {
        move |c: char| match c {
            '0'..='9' => size * 0.6,
            ':' => size * 0.3,
            _ => size * 0.5,
        }
    };
    let clock = move |f: &'static str| move || local_time(Every::Second).format(f);
    div()
        .flex()
        .flex_col()
        .px(14.0_f32)
        .pt(6.0_f32)
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .gap(6.0_f32)
                .child(
                    rolling_text(clock("%H:%M"), big, width(52.0))
                        .dyn_style(|s| s.text_color(colors().primary)),
                )
                .child(
                    rolling_text(clock("%S"), small, width(22.0))
                        .mb(10.0)
                        .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                ),
        )
        .child(
            txt(TextStyle::TITLE_MEDIUM, || {
                local_time(Every::Minute).format("%A, %-d %B %Y")
            })
            .dyn_style(|s| s.text_color(colors().on_surface_variant)),
        )
}

/// How wide and how tall a day is.
const CELL_W: f32 = 48.0;
const CELL_H: f32 = 36.0;
/// How big today's cookie is.
const TODAY: f32 = 36.0;
const GRID_W: f32 = CELL_W * 7.0;

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// A month as one number, to count in.
fn month_index(year: i32, month: u32) -> i32 {
    year * 12 + month as i32 - 1
}

fn from_month_index(i: i32) -> (i32, u32) {
    (i.div_euclid(12), i.rem_euclid(12) as u32 + 1)
}

fn days_in(year: i32, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The 42 days a month shows, and whether each is of it. Its first week is the one before the
/// one its first day is in when that day is a Monday, so that there is always some of the month
/// before it.
fn days_shown(month: i32) -> Vec<(Date, bool)> {
    let (year, m) = from_month_index(month);
    let (py, pm) = from_month_index(month - 1);
    let (ny, nm) = from_month_index(month + 1);
    let (days, before) = (days_in(year, m) as i32, days_in(py, pm) as i32);
    let from_monday = (Date::new(year, m, 1).weekday() as i32 + 6) % 7;
    let lead = if from_monday == 0 { 7 } else { from_monday };
    (0..42)
        .map(|n| {
            let d = n - lead + 1;
            if d < 1 {
                (Date::new(py, pm, (before + d) as u32), false)
            } else if d > days {
                (Date::new(ny, nm, (d - days) as u32), false)
            } else {
                (Date::new(year, m, d as u32), true)
            }
        })
        .collect()
}

/// `node` growing as it fades in, `order` steps after it is first laid out, for the days that
/// come in one after another along the diagonals.
fn pop_in(order: u32, node: Div) -> Div {
    let go = signal(false);
    let shown = follow(move || if go.get() { 1.0f32 } else { 0.0 }, fast_spatial());
    let started = Cell::new(false);
    node.on_resized(move |_, _| {
        if !started.replace(true) {
            set_timeout(Duration::from_millis(22) * order, move || {
                if go.is_alive() {
                    go.set(true);
                }
            });
        }
    })
    .dyn_style(move |s| {
        let t = shown.get();
        s.opacity(t.clamp(0.0, 1.0)).scale(0.6 + 0.4 * t)
    })
}

/// The month: its name between the arrows to the ones around it, the days of the week, and its
/// weeks, which slide over when it changes. The wheel goes through the months too, and the name
/// goes back to this one.
fn month_view() -> Div {
    let today = memo(|| {
        let t = local_time(Every::Minute);
        Date::new(t.year, t.month as u32, t.day as u32)
    });
    let this_month = memo(move || {
        let t = today.get();
        month_index(t.year, t.month)
    });
    let shown = signal(untrack(|| this_month.get()));
    // Where the strip of months is, in months from the first: small numbers stay precise.
    let base = shown.get_untracked();
    let position = follow(
        move || (shown.get() - base) as f32,
        Spring::from_ratio(1500.0, 1.0),
    );

    let title = TextStyle {
        weight: 600,
        ..TextStyle::TITLE_MEDIUM
    };
    let name = pressable(
        div()
            .relative()
            .flex_1()
            .h(32.0)
            .rounded(16.0_f32)
            .on_click(move |_| shown.set(untrack(|| this_month.get())))
            .child(
                for_each(
                    move || vec![shown.get()],
                    |m| *m,
                    move |m| {
                        let (year, month) = from_month_index(m);
                        title.apply(
                            div()
                                .absolute()
                                .inset(0.0)
                                .flex()
                                .items_center()
                                .justify_center()
                                .dyn_style(|s| s.text_color(colors().primary))
                                .child(format!("{} {year}", MONTHS[month as usize - 1])),
                        )
                    },
                )
                .enter(|s| s.opacity(0.0_f32).scale(0.9_f32))
                .exit(|s| s.opacity(0.0_f32).scale(0.9_f32))
                .transition(emphasized(250)),
            ),
        |c| c.primary,
    );
    let nav = div()
        .flex()
        .flex_row()
        .items_center()
        .w(GRID_W)
        .child(
            icon_btn(icons::CHEVRON_LEFT, 32.0, |c| c.on_surface_variant)
                .on_click(move |_| shown.update(|m| *m -= 1)),
        )
        .child(name)
        .child(
            icon_btn(icons::CHEVRON_RIGHT, 32.0, |c| c.on_surface_variant)
                .on_click(move |_| shown.update(|m| *m += 1)),
        );

    let weekday = TextStyle {
        weight: 700,
        ..TextStyle::LABEL_LARGE
    };
    let week = div().flex().flex_row().children(
        WEEKDAYS
            .iter()
            .enumerate()
            .map(|(column, name)| {
                pop_in(
                    column as u32,
                    weekday.apply(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .w(CELL_W)
                            .h(CELL_H)
                            .flex_none()
                            .dyn_style(|s| s.text_color(colors().on_surface))
                            .child(*name),
                    ),
                )
            })
            .collect::<Vec<_>>(),
    );

    // The months on the screen: the one shown, and the one it is sliding from.
    let visible = move || {
        let p = position.get();
        let (low, high) = (base + p.floor() as i32, base + p.ceil() as i32);
        let mut months = vec![low];
        if high != low {
            months.push(high);
        }
        let wanted = shown.get();
        if !months.contains(&wanted) {
            months.push(wanted);
        }
        months.sort_unstable();
        months
    };
    // Only the month it shows first comes in one day after another.
    let first = Cell::new(true);
    let strip = for_each(visible, |m| *m, move |m| {
        weeks(m, today, first.replace(false))
            .absolute()
            .top(0.0)
            .w(GRID_W)
            .dyn_style(move |s| s.left(((m - base) as f32 - position.get()) * GRID_W))
    });
    // A wheel's notch is one month; a touchpad's many small ones are too, not one each.
    let last_turn = Cell::new(None::<Instant>);
    let days = div()
        .relative()
        .w(GRID_W)
        .h(CELL_H * 6.0)
        .flex_none()
        .overflow_hidden()
        .on_scroll(move |e| {
            let step = if e.delta_y > 0.0 {
                1
            } else if e.delta_y < 0.0 {
                -1
            } else {
                return;
            };
            let now = Instant::now();
            if last_turn
                .get()
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(250))
            {
                return;
            }
            last_turn.set(Some(now));
            shown.update(|m| *m += step);
        })
        .child(strip);

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(4.0_f32)
        .px(12.0_f32)
        .pt(10.0_f32)
        .pb(12.0_f32)
        .rounded(20.0_f32)
        .dyn_style(|s| s.bg(colors().surface_container_high))
        .child(nav)
        .child(week)
        .child(days)
}

/// The six weeks of a month. With `cascade`, the days come in one after another.
fn weeks(month: i32, today: Memo<Date>, cascade: bool) -> Div {
    let all = days_shown(month);
    div().flex().flex_col().children(
        all.chunks(7)
            .enumerate()
            .map(|(row, week)| {
                div().flex().flex_row().children(
                    week.iter()
                        .enumerate()
                        .map(|(column, &(date, of_month))| {
                            let cell = day(date, of_month, today);
                            if cascade {
                                // After the days of the week.
                                pop_in((row + 1 + column) as u32, cell)
                            } else {
                                cell
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
}

/// A day: faint when it is of the month before or after, on a cookie when it is today, which
/// turns a little under the pointer.
fn day(date: Date, of_month: bool, today: Memo<Date>) -> Div {
    let is_today = memo(move || today.get() == date);
    let hovered = signal(false);
    let number = TextStyle {
        weight: 500,
        ..TextStyle::BODY_MEDIUM
    };
    let bold = TextStyle {
        weight: 700,
        ..TextStyle::BODY_MEDIUM
    };
    div()
        .relative()
        .flex()
        .items_center()
        .justify_center()
        .w(CELL_W)
        .h(CELL_H)
        .flex_none()
        // Its own stacking context, for the number to go over the cookie.
        .z_index(0)
        .on_hover(move |h| hovered.set(h))
        .child(
            show(
                move || is_today.get(),
                move || {
                    styled_shape(
                        || MaterialShape::Cookie9Sided,
                        ShapeStyle::new()
                            .color(|c| c.primary)
                            .spring(fast_spatial())
                            .rotation(move || if hovered.get() { 60.0 } else { 0.0 }),
                    )
                    .absolute()
                    .size(TODAY)
                    .left((CELL_W - TODAY) / 2.0)
                    .top((CELL_H - TODAY) / 2.0)
                },
            )
            .enter(|s| s.opacity(0.0_f32).scale(0.3_f32))
            .exit(|s| s.opacity(0.0_f32).scale(0.3_f32))
            .transition(overshoot(400)),
        )
        // Over the cookie.
        .child(div().relative().z_index(1).child(dyn_view(move || {
            let style = if is_today.get() { bold } else { number };
            style.apply(div().child(date.day.to_string()))
        })))
        .dyn_style(move |s| {
            let c = colors();
            s.text_color(if is_today.get() {
                c.on_primary
            } else if of_month {
                c.on_surface
            } else {
                c.on_surface.with_alpha(0.38)
            })
        })
}

/// CPU, memory and temperature as wavy rings.
fn gauges() -> Div {
    let sys = std::rc::Rc::new(system_every(Duration::from_secs(2)));
    let cpu = memo({
        let sys = sys.clone();
        move || sys.cpu()
    });
    let memory = memo({
        let sys = sys.clone();
        move || {
            let m = sys.memory();
            (m.used, m.total)
        }
    });
    let heat = memo(move || sys.cpu_temperature());
    let gauge = |symbol: &'static Symbol,
                 label: &'static str,
                 value: Box<dyn Fn() -> f32>,
                 shown: Box<dyn Fn() -> String>| {
        let value: std::rc::Rc<dyn Fn() -> f32> = value.into();
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .gap(8.0_f32)
            .px(8.0_f32)
            .py(14.0_f32)
            .rounded(20.0_f32)
            .dyn_style(|s| s.bg(colors().surface_container_high))
            .child(
                // The ring is as big as it draws itself; the icon sits in its middle.
                div()
                    .relative()
                    .flex_none()
                    .child(circular_wavy_progress(move || value()))
                    .child(
                        div()
                            .absolute()
                            .inset(0.0)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(sym(symbol, 18.0, |c| c.primary)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(txt(TextStyle::TITLE_SMALL, shown))
                    .child(
                        static_txt(TextStyle::LABEL_SMALL, label)
                            .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                    ),
            )
    };
    let gib = |b: u64| b as f64 / (1u64 << 30) as f64;
    div()
        .flex()
        .flex_row()
        .gap(10.0_f32)
        .child(gauge(
            icons::SPEED,
            "CPU",
            Box::new(move || cpu.get() as f32),
            Box::new(move || format!("{:.0}%", cpu.get() * 100.0)),
        ))
        .child(gauge(
            icons::MEMORY,
            "Memory",
            Box::new(move || {
                let (used, total) = memory.get();
                (used as f64 / total.max(1) as f64) as f32
            }),
            Box::new(move || format!("{:.1} GiB", gib(memory.get().0))),
        ))
        .child(gauge(
            icons::THERMOSTAT,
            "Heat",
            Box::new(move || ((heat.get().unwrap_or(0.0) - 30.0) / 70.0).clamp(0.0, 1.0) as f32),
            Box::new(move || {
                heat.get()
                    .map(|t| format!("{t:.0}°C"))
                    .unwrap_or_else(|| "—".into())
            }),
        ))
}
