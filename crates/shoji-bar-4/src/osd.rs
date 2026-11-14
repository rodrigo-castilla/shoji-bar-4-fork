//! A pill above the dock that shows the volume or the brightness for a moment after either
//! changes, however it was changed.

use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Shown {
    Volume,
    Brightness,
}

/// Follows the volume and the brightness. Call once, from the root.
pub fn install() {
    let a = audio();
    let s = session();
    let volume = memo(move || {
        a.default_sink()
            .map(|d| ((d.volume * 100.0).round() as i32, d.muted, d.id))
    });
    let brightness = memo(move || s.brightness().map(|b| (b * 100.0).round() as i32));
    let shown = signal(None::<Shown>);
    let hide: Cell<Option<Timeout>> = Cell::new(None);
    let show_for = move |what: Shown| {
        shown.set(Some(what));
        if let Some(t) = hide.take() {
            t.cancel();
        }
        hide.set(Some(set_timeout(Duration::from_millis(1600), move || {
            shown.set(None)
        })));
    };
    let show_for = std::rc::Rc::new(show_for);
    // Not for what is there at the start, nor for another device becoming the default.
    let last_volume = Cell::new(None::<(i32, bool, u32)>);
    let on_volume = show_for.clone();
    effect(move || {
        let now = volume.get();
        let before = last_volume.replace(now);
        if let (Some(b), Some(n)) = (before, now)
            && b.2 == n.2
            && (b.0, b.1) != (n.0, n.1)
        {
            untrack(|| on_volume(Shown::Volume));
        }
    });
    let last_brightness = Cell::new(None::<i32>);
    effect(move || {
        let now = brightness.get();
        if last_brightness.replace(now).is_some() && now.is_some() {
            untrack(|| show_for(Shown::Brightness));
        }
    });

    let visible = memo(move || shown.get().is_some());
    let mounted = signal(false);
    let pending: Cell<Option<Timeout>> = Cell::new(None);
    effect(move || {
        if let Some(t) = pending.take() {
            t.cancel();
        }
        if visible.get() {
            mounted.set(true);
        } else {
            pending.set(Some(set_timeout(Duration::from_millis(400), move || {
                mounted.set(false)
            })));
        }
    });
    // What the pill shows stays while it leaves.
    let kind = signal(Shown::Volume);
    effect(move || {
        if let Some(k) = shown.get() {
            kind.set(k);
        }
    });
    effect(move || {
        if !mounted.get() {
            return;
        }
        open_layer(
            LayerOptions::new()
                .layer(Layer::Overlay)
                .anchor(Anchor::BOTTOM)
                .margin(Margin {
                    bottom: 16,
                    ..Default::default()
                })
                .fit_content()
                .click_through()
                .namespace("shoji-bar-4-osd"),
            move || {
                let level = move || match kind.get() {
                    Shown::Volume => volume
                        .get()
                        .map_or(0.0, |v| if v.1 { 0.0 } else { v.0 as f32 / 100.0 }),
                    Shown::Brightness => brightness.get().unwrap_or(0) as f32 / 100.0,
                };
                let symbol = move || -> &'static Symbol {
                    match kind.get() {
                        Shown::Brightness => icons::BRIGHTNESS_6,
                        Shown::Volume => match volume.get() {
                            Some(v) if v.1 || v.0 == 0 => icons::VOLUME_OFF,
                            Some(v) if v.0 < 50 => icons::VOLUME_DOWN,
                            _ => icons::VOLUME_UP,
                        },
                    }
                };
                div().p(16.0_f32).select_none().child(
                    show(move || visible.get(), move || pill(level, symbol))
                        .appear()
                        .enter(|s| s.opacity(0.0_f32).translate_y(40.0_f32).scale(0.8_f32))
                        .exit(|s| s.opacity(0.0_f32).translate_y(24.0_f32).scale(0.9_f32))
                        .transition(overshoot(420)),
                )
            },
        );
    });
}

fn pill(
    level: impl Fn() -> f32 + Copy + 'static,
    symbol: impl Fn() -> &'static Symbol + 'static,
) -> Div {
    let fill = follow(level, fast_spatial());
    div()
        .w(280.0)
        .h(48.0)
        .flex()
        .flex_row()
        .items_center()
        .gap(12.0_f32)
        .px(12.0_f32)
        .rounded(24.0_f32)
        .shadows(elevation(3))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container_high).text_color(c.on_surface)
        })
        .child(
            div()
                .size(30.0)
                .rounded(15.0_f32)
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .dyn_style(|s| s.bg(colors().primary_container))
                .child(dyn_sym(symbol, 17.0, |c| c.on_primary_container)),
        )
        .child(
            div()
                .flex_1()
                .h(10.0)
                .rounded(5.0_f32)
                .relative()
                .overflow_hidden()
                .dyn_style(|s| s.bg(colors().secondary_container))
                .child(
                    div()
                        .absolute()
                        .top(0.0)
                        .bottom(0.0)
                        .left(0.0)
                        .rounded(5.0_f32)
                        .dyn_style(move |s| {
                            s.w(pct(fill.get().clamp(0.0, 1.0) * 100.0))
                                .bg(colors().primary)
                        }),
                ),
        )
        .child(
            rolling_text(
                move || format!("{:>3}", (level() * 100.0).round() as i32),
                TextStyle::LABEL_LARGE,
                |c| if c == ' ' { 0.0 } else { 9.0 },
            )
            .w(32.0)
            .justify_end(),
        )
}
