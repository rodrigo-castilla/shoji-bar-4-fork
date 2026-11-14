//! Small pieces the bar, the dock and the panels share.

use crate::theme::{TextStyle, colors, elevation};
use anti_webui::prelude::*;
use anti_webui_md3e::ColorScheme;
use anti_webui_md3e::components::{Ink, MaterialShape, ShapeStyle, sized_icon, styled_shape};
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// A color of the scheme, chosen by a function of it.
pub trait Role: Fn(&ColorScheme) -> Color + Copy + 'static {}
impl<F: Fn(&ColorScheme) -> Color + Copy + 'static> Role for F {}

/// Material's emphasized easing, for what moves in or out of view.
pub fn emphasized(ms: u64) -> Transition {
    Transition::ms(ms).cubic_bezier(0.2, 0.0, 0.0, 1.0)
}

/// Slow out of the start and quick at the end, for things that leave.
pub fn emphasized_accelerate(ms: u64) -> Transition {
    Transition::ms(ms).cubic_bezier(0.3, 0.0, 0.8, 0.15)
}

/// An easing that goes a little past the end and comes back, for things that pop in.
pub fn overshoot(ms: u64) -> Transition {
    Transition::ms(ms).cubic_bezier(0.34, 1.45, 0.64, 1.0)
}

/// An icon of a size, in a color of the scheme.
pub fn sym(symbol: &'static Symbol, size: f32, color: impl Role) -> Div {
    sized_icon(symbol, size).dyn_style(move |s| s.text_color(color(&colors())))
}

/// An icon that changes with what `which` says.
pub fn dyn_sym(which: impl Fn() -> &'static Symbol + 'static, size: f32, color: impl Role) -> Div {
    let which = memo(move || SameSymbol(which()));
    div()
        .size(size)
        .flex_none()
        .child(dyn_view(move || sym(which.get().0, size, color)))
}

/// A symbol compared by which one it is.
#[derive(Clone, Copy)]
struct SameSymbol(&'static Symbol);

impl PartialEq for SameSymbol {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

/// Text in a style of the type scale, which follows what `f` says.
pub fn txt(style: TextStyle, f: impl Fn() -> String + 'static) -> Div {
    style.apply(div().whitespace_nowrap().child(text(f)))
}

/// Text that stays as it is.
pub fn static_txt(style: TextStyle, s: impl Into<String>) -> Div {
    style.apply(div().whitespace_nowrap().child(s.into()))
}

/// How long after the one before it each part of a panel starts to come in.
const SETTLE_STEP: u64 = 45;

/// `node` as a part of a panel that comes in `order` steps after the panel does: it falls into
/// place, growing as it fades in, as the apps of the launcher do. The steps count from when it is
/// first laid out: a panel's layer is only laid out once the compositor has said how big it is,
/// and timers set when it is made could be over by then.
pub fn settle_in(order: usize, node: Div) -> Div {
    let go = signal(false);
    let shown = follow(move || if go.get() { 1.0f32 } else { 0.0 }, fast_spatial());
    let started = Cell::new(false);
    node.on_resized(move |_, _| {
        if !started.replace(true) {
            set_timeout(
                Duration::from_millis(order as u64 * SETTLE_STEP),
                move || {
                    if go.is_alive() {
                        go.set(true);
                    }
                },
            );
        }
    })
    .dyn_style(move |s| {
        let t = shown.get();
        s.opacity(t.clamp(0.0, 1.0))
            .scale(0.85 + 0.15 * t)
            .translate_y(-24.0 * (1.0 - t))
    })
}

/// A floating container of the bar: a pill on `surface_container`.
/// How tall an island of the bar is.
pub const ISLAND: f32 = 32.0;

pub fn island() -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .h(ISLAND)
        .rounded(ISLAND / 2.0)
        .shadows(elevation(1))
        .dyn_style(|s| s.bg(colors().surface_container))
}

/// Gives `node` Material's state layer and ripples, in its own shape, in `color`.
pub fn pressable(node: Div, color: impl Role) -> Div {
    let ink = Ink::new(move |c: &ColorScheme| color(c), || false);
    ink.paint(ink.listen(node))
}

/// A round button of an icon, `size` across, with its state layer.
pub fn icon_btn(symbol: &'static Symbol, size: f32, color: impl Role) -> Div {
    pressable(
        div()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .rounded(size / 2.0)
            .child(sym(symbol, (size * 0.55).round(), color)),
        color,
    )
}

/// Text whose characters roll into place when they change: the old one goes up and out, the new
/// one comes up from below. `width` says how wide each character's cell is.
pub fn rolling_text(
    f: impl Fn() -> String + 'static,
    style: TextStyle,
    width: impl Fn(char) -> f32 + Copy + 'static,
) -> Div {
    let chars = memo(move || f().chars().collect::<Vec<char>>());
    let height = style.line_height;
    div().flex().flex_row().flex_none().child(for_each(
        move || (0..chars.with(|c| c.len())).collect::<Vec<_>>(),
        |i| *i,
        move |i| {
            let ch = memo(move || chars.with(|c| c.get(i).copied().unwrap_or(' ')));
            div()
                .relative()
                .h(height)
                .flex_none()
                .overflow_hidden()
                .transition_of([Prop::Width], emphasized(400))
                .dyn_style(move |s| s.w(width(ch.get())))
                .child(
                    for_each(
                        move || vec![ch.get()],
                        |c| *c,
                        move |c| {
                            style.apply(
                                div()
                                    .absolute()
                                    .inset(0.0)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(c.to_string()),
                            )
                        },
                    )
                    .enter(move |s| s.translate_y(height * 0.9).opacity(0.0_f32))
                    .exit(move |s| s.translate_y(-height * 0.9).opacity(0.0_f32))
                    .transition(emphasized(550)),
                )
        },
    ))
}

/// Seconds since `running` last became true, going on frame by frame only while it stays so: a
/// [`clock`] that asks for no frames the rest of the time.
pub fn clock_while(running: impl Fn() -> bool + 'static) -> Signal<f32> {
    ticker(running, None)
}

/// [`clock_while`] that goes on only `per_second` times a second, for what moves without having
/// to be smooth: it costs a fraction of the frames.
pub fn clock_at(per_second: u32, running: impl Fn() -> bool + 'static) -> Signal<f32> {
    ticker(
        running,
        Some(Duration::from_secs_f32(1.0 / per_second.max(1) as f32)),
    )
}

thread_local! {
    static TICKING: Cell<usize> = const { Cell::new(0) };
}

/// How many clocks are going on now. Each asks for frames, so one that never stops is what keeps
/// the shell busy: `shoji-bar-4 msg status` tells.
pub fn ticking() -> usize {
    TICKING.with(Cell::get)
}

/// Counted while a clock goes on, however it ends.
struct Ticking;

impl Ticking {
    fn new() -> Self {
        TICKING.with(|t| t.set(t.get() + 1));
        Self
    }
}

impl Drop for Ticking {
    fn drop(&mut self) {
        TICKING.with(|t| t.set(t.get().saturating_sub(1)));
    }
}

fn ticker(running: impl Fn() -> bool + 'static, every: Option<Duration>) -> Signal<f32> {
    struct Run {
        time: Signal<f32>,
        start: Instant,
        running: Memo<bool>,
        every: Option<Duration>,
        /// Which run is the latest: one that went off and on again before its next tick is over.
        latest: Rc<Cell<u64>>,
        me: u64,
        _counted: Ticking,
    }
    fn next(run: Run) {
        let every = run.every;
        let step = move |now: Instant| {
            if run.time.is_alive()
                && run.latest.get() == run.me
                && untrack(|| run.running.get())
            {
                run.time
                    .set(now.saturating_duration_since(run.start).as_secs_f32());
                next(run);
            }
        };
        match every {
            None => request_frame(step),
            Some(every) => drop(set_timeout(every, move || step(Instant::now()))),
        }
    }
    let running = memo(running);
    let time = signal(0.0f32);
    let latest = Rc::new(Cell::new(0u64));
    effect(move || {
        if running.get() {
            let me = latest.get() + 1;
            latest.set(me);
            next(Run {
                time,
                start: Instant::now(),
                running,
                every,
                latest: latest.clone(),
                me,
                _counted: Ticking::new(),
            });
        }
    });
    time
}

/// The mark of the launcher: a four-sided cookie that blooms into a nine-sided one under the
/// pointer and rounds into a circle with a cross while the launcher is open. It fills its box.
pub fn launcher_glyph(
    open: Memo<bool>,
    hovered: Signal<bool>,
    icon_size: impl Fn() -> f32 + 'static,
) -> Div {
    let turn = follow(move || if open.get() { 90.0 } else { 0.0 }, fast_spatial());
    styled_shape(
        move || {
            if open.get() {
                MaterialShape::Circle
            } else if hovered.get() {
                MaterialShape::Cookie9Sided
            } else {
                MaterialShape::Cookie4Sided
            }
        },
        ShapeStyle::new()
            .color(move |c| {
                if open.get() {
                    c.primary
                } else {
                    c.primary_container
                }
            })
            .spring(fast_spatial())
            .rotation(move || {
                if hovered.get() && !open.get() {
                    45.0
                } else {
                    0.0
                }
            }),
    )
    .flex()
    .items_center()
    .justify_center()
    .child(
        div()
            .dyn_style(move |s| {
                let c = colors();
                s.rotate(turn.get()).text_color(if open.get() {
                    c.on_primary
                } else {
                    c.on_primary_container
                })
            })
            .child(dyn_view(move || {
                let size = icon_size();
                sized_icon(
                    if open.get() {
                        icons::CLOSE
                    } else {
                        icons::APPS
                    },
                    size,
                )
            })),
    )
}
