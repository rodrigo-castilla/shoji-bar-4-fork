//! How the colours move, and the pane behind the bar.
//!
//! - A change of palette is one fade of about 0.6 s, from the colours on screen to the new ones.
//! - The pane behind the islands spreads from the middle island out to the sides when it is switched
//!   on and goes back in when it is switched off. With a glass palette it spreads out in the
//!   colours of the light theme and then fades into the glass, and does the reverse on the way out.
//!
//! Every animation here is driven by signals (the state in `colour.rs`), and the effects that start
//! them read nothing else: reading the colours in them would start them again on every frame of a
//! fade.

use super::colour;
use anti_webui::prelude::*;
use shoji_core::theme::{colors, current_seed, scheme_for, set_scheme_fade};
use shoji_core::settings::settings;
use shoji_default::Ui;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// How long a fade between two sets of colours takes (a critically damped spring: ~0.6 s).
fn fade() -> Spring {
    Spring::from_ratio(110.0, 1.0)
}

const SPREAD: u64 = 600;
/// Going to the light colour before the pane is drawn in, and from it to the glass once it is out.
const WHITEN: u64 = 180;
const SETTLE: u64 = 300;
/// How far the pane reaches past each side of the bar: its rounded ends are there, out of sight
/// while it covers the bar, and show along the way as it spreads or is drawn in.
const ROUND: f32 = 19.0;

pub fn install(ui: &mut Ui) {
    set_scheme_fade(fade());
    if let Some(bar) = ui.bar.as_mut() {
        bar.background = Some(Rc::new(|_output| pane()));
    }
}

/// Premultiplied mix of two colours.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let (a, b) = (a.to_components(), b.to_components());
    Color::from_components(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t))
}

fn pane() -> Div {
    let on = colour::bar_on();
    let glass = colour::is_glass();
    // How far it has spread from the middle island to the sides, and how much of the light theme's
    // colour it has on the way in and out of a glass palette.
    let spread = animated(if on.get_untracked() { 1.0_f32 } else { 0.0 });
    let white = animated(0.0_f32);
    let turn = Rc::new(Cell::new(0u32));

    let later = {
        let turn = turn.clone();
        move |id: u32, ms: u64, f: Box<dyn FnOnce()>| {
            let turn = turn.clone();
            drop(set_timeout(Duration::from_millis(ms), move || {
                if turn.get() == id {
                    f()
                }
            }));
        }
    };

    // The switch: what it does depends on the palette at that moment (read untracked).
    let first = Cell::new(true);
    effect({
        let turn = turn.clone();
        move || {
            let now = on.get();
            if first.replace(false) {
                return;
            }
            let id = turn.get() + 1;
            turn.set(id);
            untrack(|| {
                let glassy = glass.get_untracked();
                if now {
                    white.set(if glassy { 1.0 } else { 0.0 });
                    spread.to(1.0, Transition::ms(SPREAD));
                    if glassy {
                        later(id, SPREAD, Box::new(move || white.to(0.0, Transition::ms(SETTLE))));
                    }
                } else if glassy {
                    white.to(1.0, Transition::ms(WHITEN));
                    later(id, WHITEN, Box::new(move || spread.to(0.0, Transition::ms(SPREAD))));
                } else {
                    white.to(0.0, Transition::ms(1));
                    spread.to(0.0, Transition::ms(SPREAD));
                }
            });
        }
    });
    // A change of palette stops any of the light colour that is left.
    let first = Cell::new(true);
    effect({
        let turn = turn.clone();
        move || {
            glass.get();
            if first.replace(false) {
                return;
            }
            untrack(|| {
                if white.get_untracked() != 0.0 {
                    // Not while it spreads: the new palette takes over the end of that.
                    turn.set(turn.get() + 1);
                    white.to(0.0, Transition::ms(SETTLE));
                }
            });
        }
    });

    // The faint pane of the scheme in use: a third of the surface's opacity, and where the scheme is
    // see-through its glass. The one goes into the other as the scheme's opacity does, so that a
    // change of palette is a plain fade.
    let faint = |c: Color| c.with_alpha(c.a * 0.34);
    let light = {
        let s = settings().get_untracked();
        faint(scheme_for(current_seed(&s), s.palette, false).surface)
    };
    let layer = || div().absolute().inset(0.0).rounded(ROUND).pointer_events_none();
    // 1 for a see-through scheme, 0 for an opaque one.
    let see_through = || ((1.0 - colors().surface.a) / 0.97).clamp(0.0, 1.0);
    div()
        .absolute()
        .z_index(-1)
        .top(0.0)
        .bottom(0.0)
        .left(-ROUND)
        .right(-ROUND)
        .pointer_events_none()
        .dyn_style(move |s| {
            let k = spread.get();
            s.scale_x(k).opacity((k * 4.0).min(1.0))
        })
        .child(layer().dyn_style(move |s| {
            let c = colors();
            let glass = c.surface_container;
            let glass = glass.with_alpha(glass.a.max(0.04));
            let t = see_through();
            s.bg(mix(faint(c.surface), glass, t)).opacity(1.0 - white.get() * t)
        }))
        .child(layer().dyn_style(move |s| s.bg(light).opacity(white.get() * see_through())))
}
