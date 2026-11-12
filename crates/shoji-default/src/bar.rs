//! The bar along the top of every output: the launcher and the workspaces on the left, the window
//! with the keyboard, the clock in the middle, and what is playing, the tray and the state of the
//! system on the right. Each group floats in an island of its own, on a pane of frosted glass that
//! is all of the bar.

use crate::appinfo::app_id_icon;
use crate::panels::{Panel, is_open, toggle};
use crate::settings::settings;
use crate::theme::{TextStyle, colors};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui::shell::{BatteryState, Connectivity, MenuToggle, TrayMenuItem};
use anti_webui_md3e::components::{MenuItem, MenuStyle, menu_item, styled_menu};
use anti_webui_md3e::icons;
use anti_webui_md3e::motion::{fast_spatial, slow_spatial};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// How tall the layer of the stock bar is; the islands are [`ISLAND`] of it, with room above and
/// below.
pub const HEIGHT: i32 = 38;

thread_local! {
    static BAR_HEIGHT: Cell<i32> = const { Cell::new(HEIGHT) };
}

/// How tall the bar that is open is. What hangs under the bar (the panels at the top) keeps clear of
/// this much.
pub fn height() -> i32 {
    BAR_HEIGHT.with(Cell::get)
}

/// One thing that floats on the bar, in an island of its own.
#[derive(Clone)]
pub struct Island {
    /// What the island is called, for finding it again in a [`BarLayout`].
    pub id: &'static str,
    build: Rc<dyn Fn(Output) -> View>,
}

impl Island {
    /// An island called `id` that shows what `build` makes for the output the bar is on.
    pub fn new<V: IntoView>(id: &'static str, build: impl Fn(Output) -> V + 'static) -> Self {
        Self {
            id,
            build: Rc::new(move |output| build(output).into_view()),
        }
    }
}

/// What is on the bar and how tall it is: three groups of islands, left to right.
///
/// The left group and the right one share the room the middle one leaves; the middle group stays
/// in the middle of the bar.
#[derive(Clone)]
pub struct BarLayout {
    /// How tall the layer is. The stock islands are drawn for [`HEIGHT`].
    pub height: i32,
    /// Something to draw behind all the islands, across the whole bar (a pane, a glass...).
    pub background: Option<Rc<dyn Fn(Output) -> Div>>,
    pub left: Vec<Island>,
    pub center: Vec<Island>,
    pub right: Vec<Island>,
}

impl BarLayout {
    /// The stock bar: the launcher, the workspaces and the window with the keyboard; the clock;
    /// what is playing, the tray and the state of the system.
    pub fn stock() -> Self {
        Self {
            height: HEIGHT,
            background: None,
            left: vec![
                Island::new("launcher", launcher_island),
                Island::new("workspaces", workspaces_stock),
                Island::new("window", window_island),
            ],
            center: vec![Island::new("clock", clock_island)],
            right: vec![
                Island::new("media", media_island),
                Island::new("tray", |_| tray_island()),
                Island::new("status", status_island),
            ],
        }
    }

    /// The layout as text, for a developer who needs to know what each island is called and where
    /// it is (`shoji-bar-4 msg layout`). Marks what comes with the stock bar, and lists the stock
    /// islands that are gone.
    pub fn outline(&self) -> String {
        use std::fmt::Write;
        let stock = Self::stock();
        let stock_ids: Vec<&str> = [&stock.left, &stock.center, &stock.right]
            .into_iter()
            .flatten()
            .map(|i| i.id)
            .collect();
        let mut out = String::new();
        let _ = writeln!(
            out,
            "bar  height={}  background={}",
            self.height,
            if self.background.is_some() { "custom" } else { "none" }
        );
        for (zone, islands) in [("left", &self.left), ("center", &self.center), ("right", &self.right)] {
            let _ = writeln!(out, "  {zone} ({})", islands.len());
            for (at, island) in islands.iter().enumerate() {
                let origin = if stock_ids.contains(&island.id) { "stock" } else { "custom" };
                let _ = writeln!(out, "    [{at}] {:<14} {origin}", island.id);
            }
        }
        let present: Vec<&str> = [&self.left, &self.center, &self.right]
            .into_iter()
            .flatten()
            .map(|i| i.id)
            .collect();
        let gone: Vec<&str> = stock_ids.iter().copied().filter(|id| !present.contains(id)).collect();
        if !gone.is_empty() {
            let _ = writeln!(out, "  stock islands removed: {}", gone.join(", "));
        }
        let _ = writeln!(
            out,
            "  edit it in a plugin's `patch(ui)`, through `ui.bar`, naming an id above:\n    \
             bar.insert_after(\"<id>\", Island::new(\"mine\", builder))   bar.insert_before(..)\n    \
             bar.replace(\"<id>\", Island::new(..))   bar.remove(\"<id>\")\n    \
             (an id that is not on the bar is not an error: the call returns false and does nothing)"
        );
        out
    }

    fn groups_mut(&mut self) -> [&mut Vec<Island>; 3] {
        [&mut self.left, &mut self.center, &mut self.right]
    }

    /// Takes the island called `id` off the bar, wherever it is. Whether there was one.
    pub fn remove(&mut self, id: &str) -> bool {
        let mut found = false;
        for group in self.groups_mut() {
            let before = group.len();
            group.retain(|i| i.id != id);
            found |= group.len() != before;
        }
        found
    }

    /// Puts `island` in the place of the one called `id`. Whether there was one.
    pub fn replace(&mut self, id: &str, island: Island) -> bool {
        for group in self.groups_mut() {
            if let Some(slot) = group.iter_mut().find(|i| i.id == id) {
                *slot = island;
                return true;
            }
        }
        false
    }

    /// Puts `island` right after the one called `id`. Whether there was one.
    pub fn insert_after(&mut self, id: &str, island: Island) -> bool {
        for group in self.groups_mut() {
            if let Some(at) = group.iter().position(|i| i.id == id) {
                group.insert(at + 1, island);
                return true;
            }
        }
        false
    }

    /// Puts `island` right before the one called `id`. Whether there was one.
    pub fn insert_before(&mut self, id: &str, island: Island) -> bool {
        for group in self.groups_mut() {
            if let Some(at) = group.iter().position(|i| i.id == id) {
                group.insert(at, island);
                return true;
            }
        }
        false
    }
}

/// Opens the stock bar on `output`.
pub fn open(output: Output) {
    open_with(output, &BarLayout::stock());
}

/// Opens a bar laid out as `layout` on `output`.
pub fn open_with(output: Output, layout: &BarLayout) {
    BAR_HEIGHT.with(|h| h.set(layout.height));
    let layout = layout.clone();
    open_layer(
        LayerOptions::bar(Anchor::TOP, layout.height).namespace("shoji-bar-4-bar"),
        move || view_with(output, &layout),
    );
}

/// The stock bar, as it is drawn.
pub fn view(output: Output) -> Div {
    view_with(output, &BarLayout::stock())
}

/// The workspaces island of the stock bar: the workspaces of the output, and a click on one
/// activates it.
fn workspaces_stock(output: Output) -> Div {
    workspaces_island(
        workspaces_of(output.clone()),
        Rc::new({
            let output = output.clone();
            move |id: &str| {
                if let Some(w) = workspaces().on(&output).into_iter().find(|w| w.id() == id) {
                    w.activate();
                }
            }
        }),
    )
}

/// A bar laid out as `layout`.
pub fn view_with(output: Output, layout: &BarLayout) -> Div {
    let group = |islands: &[Island]| {
        islands
            .iter()
            .map(|i| (i.build)(output.clone()))
            .collect::<Vec<_>>()
    };
    div()
        .select_none()
        .size_full()
        .relative()
        // Its own stacking context, for the glass to go behind the islands but not behind this.
        .z_index(0)
        .flex()
        .flex_row()
        .items_center()
        .px(14.0_f32)
        .gap(10.0_f32)
        .dyn_style(|s| s.text_color(colors().on_surface))
        // In a box of its own that takes no room in the row: a child of the row, even an empty
        // one, would add a gap of its own and push the islands over.
        .child(
            div()
                .absolute()
                .inset(0.0)
                .z_index(-1)
                .pointer_events_none()
                .children(layout.background.iter().map(|b| b(output.clone()))),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(10.0_f32)
                .flex_1()
                .min_w(0.0)
                .children(group(&layout.left)),
        )
        .children(group(&layout.center))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_end()
                .gap(10.0_f32)
                .flex_1()
                .min_w(0.0)
                .children(group(&layout.right)),
        )
}

/// The button that opens the launcher: an icon on a shape that turns and blooms on hover.
fn launcher_island(output: Output) -> Div {
    let hovered = signal(false);
    let open = memo({
        let output = output.clone();
        move || is_open(&output, Panel::Launcher)
    });
    island()
        .w(ISLAND)
        .justify_center()
        .on_hover(move |h| hovered.set(h))
        .on_click(move |_| toggle(&output, Panel::Launcher))
        .child(launcher_glyph(open, hovered, || 15.0).size(26.0))
}

/// A workspace as the bar shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Ws {
    pub id: String,
    pub name: String,
    pub active: bool,
    /// Has no windows.
    pub empty: bool,
    pub urgent: bool,
}

/// The workspaces of an output, in order.
fn workspaces_of(output: Output) -> Memo<Vec<Ws>> {
    memo(move || {
        // A special workspace (the scratchpad kind) is not one of the row: it shows, as an `S`,
        // only while it is open, and goes when it is put away.
        let mut all: Vec<_> = workspaces()
            .on(&output)
            .into_iter()
            .filter(|w| !w.is_hidden() || w.is_active())
            .collect();
        all.sort_by(|a, b| {
            (a.is_hidden(), a.coordinates()).cmp(&(b.is_hidden(), b.coordinates()))
        });
        let mut row: Vec<Ws> = all
            .iter()
            .map(|w| Ws {
                id: w.id().to_owned(),
                name: if w.is_hidden() {
                    "S".to_owned()
                } else {
                    w.name().to_owned()
                },
                active: w.is_active(),
                empty: w.is_hidden(),
                urgent: w.is_urgent(),
            })
            .collect();
        // An open special workspace is drawn over the one under it: it alone has the pill.
        if all.iter().any(|w| w.is_hidden() && w.is_active()) {
            for (ws, w) in row.iter_mut().zip(&all) {
                ws.active = w.is_hidden();
            }
        }
        row
    })
}

const SLOT: f32 = 18.0;
const ACTIVE: f32 = 32.0;
/// How tall the pill under the active workspace is.
const PILL: f32 = 22.0;

/// The workspaces of the output as dots, with a pill under the active one that stretches as it
/// goes from one to another: its front edge leads on a quick spring, its back follows on a slow
/// one.
pub fn workspaces_island(list: Memo<Vec<Ws>>, activate: Rc<dyn Fn(&str)>) -> Div {
    // Between one workspace going and the next coming there may be none: the pill stays.
    let last = Cell::new(0);
    let active = memo(move || {
        let at = list.with(|l| l.iter().position(|w| w.active)).unwrap_or(last.get());
        last.set(at);
        at
    });
    let scrolled = activate.clone();
    let left = animated(0.0f32);
    let right = animated(ACTIVE);
    let first = Cell::new(true);
    effect(move || {
        let at = active.get() as f32 * SLOT;
        let (to_left, to_right) = (at, at + ACTIVE);
        untrack(|| {
            if first.replace(false) {
                left.set(to_left);
                right.set(to_right);
            } else if to_left >= left.get_untracked() {
                right.spring_to(to_right, Spring::from_ratio(900.0, 0.7));
                left.spring_to(to_left, Spring::from_ratio(260.0, 0.85));
            } else {
                left.spring_to(to_left, Spring::from_ratio(900.0, 0.7));
                right.spring_to(to_right, Spring::from_ratio(260.0, 0.85));
            }
        })
    });
    // Under the slots, also while one of them comes or goes: fading or scaling, a slot is
    // painted as one image with what is in it, and would otherwise go under the pill.
    let pill = div()
        .absolute()
        .z_index(-1)
        .top(0.0)
        .h(PILL)
        .rounded(PILL / 2.0)
        .dyn_style(move |s| {
            let (l, r) = (left.get(), right.get());
            s.left(l).w((r - l).max(PILL)).bg(colors().primary)
        });
    let slots =
        for_each(
            move || list.get(),
            |w| w.id.clone(),
            move |w| {
                let id = w.id.clone();
                // Once gone from the list it leaves as it was, without turning back into a dot.
                let seen = std::cell::RefCell::new(w.clone());
                let me = memo(move || {
                    if let Some(now) = list.with(|l| l.iter().find(|x| x.id == id).cloned()) {
                        *seen.borrow_mut() = now;
                    }
                    seen.borrow().clone()
                });
                let on = memo(move || me.with(|w| w.active));
                let empty = memo(move || me.with(|w| w.empty));
                let urgent = memo(move || me.with(|w| w.urgent));
                let name = w.name.clone();
                let activate = activate.clone();
                div()
                    .relative()
                    .h(PILL)
                    .flex_none()
                    .transition_of([Prop::Width], emphasized(350))
                    .dyn_style(move |s| s.w(if on.get() { ACTIVE } else { SLOT }))
                    .on_click(move |_| activate(&w.id))
                    // The dot, which gives way to the number.
                    .child(
                        div()
                            .absolute()
                            .inset(0.0)
                            .flex()
                            .items_center()
                            .justify_center()
                            .pointer_events_none()
                            .child(div().rounded(3.0_f32).transition(overshoot(350)).dyn_style(
                                move |s| {
                                    let c = colors();
                                    let size = if empty.get() { 5.0 } else { 6.0 };
                                    let color = if urgent.get() {
                                        c.error
                                    } else if empty.get() {
                                        c.outline_variant
                                    } else {
                                        c.on_surface_variant
                                    };
                                    s.size(size)
                                        .bg(color)
                                        .scale(if on.get() { 0.0 } else { 1.0_f32 })
                                },
                            )),
                    )
                    // As wide as the slot is when active, whatever it is on the way, so the
                    // number stays on the pill while the slot opens and closes.
                    .child(
                        TextStyle::LABEL_MEDIUM
                            .apply(div())
                            .absolute()
                            .top(0.0)
                            .bottom(0.0)
                            .left(0.0)
                            .w(ACTIVE)
                            .flex()
                            .items_center()
                            .justify_center()
                            .pointer_events_none()
                            .transition(overshoot(400))
                            .dyn_style(move |s| {
                                s.text_color(colors().on_primary)
                                    .opacity(if on.get() { 1.0 } else { 0.0_f32 })
                                    .scale(if on.get() { 1.0 } else { 0.4_f32 })
                            })
                            .child(name),
                    )
            },
        )
        // They open and close their place, so the others and the island move over with them.
        .enter(|s| s.w(0.0).opacity(0.0_f32).scale(0.3_f32))
        .exit(|s| s.w(0.0).opacity(0.0_f32).scale(0.3_f32))
        .transition(emphasized(350));
    island()
        .px(5.0_f32)
        .on_scroll(move |e| {
            let step = if e.delta_y > 0.0 {
                1
            } else if e.delta_y < 0.0 {
                -1
            } else {
                0
            };
            let all = untrack(|| list.get());
            let next = untrack(|| active.get()) as i32 + step;
            if step != 0
                && let Some(w) = usize::try_from(next).ok().and_then(|i| all.get(i))
            {
                scrolled(&w.id);
            }
        })
        .child(
            div()
                .relative()
                .z_index(0)
                .flex()
                .flex_row()
                .items_center()
                .h(PILL)
                .child(pill)
                .child(slots),
        )
}

/// The window with the keyboard on this output.
fn window_island(output: Output) -> Show {
    focused_island(memo(move || {
        toplevels()
            .active()
            .filter(|t| t.outputs.is_empty() || t.outputs.contains(&output.name))
            .map(|t| Focused {
                id: t.id(),
                title: t.title.chars().take(60).collect(),
                app_id: t.app_id,
            })
    }))
}

/// What the bar shows of the window with the keyboard.
#[derive(Clone, PartialEq, Debug)]
pub struct Focused {
    pub id: u64,
    pub title: String,
    pub app_id: String,
}

/// The icon and title of `active`. When another window takes the keyboard, its row comes up from
/// below as the last one goes up; when the title changes, the new one comes up the same way. The
/// island goes to the width of what comes on a spring, which keeps its speed when that changes
/// again on the way, so it follows however fast they change. What goes stays until it has gone
/// up, filling what the island has yet to give up, and a title wider than the island fades out at
/// its edge rather than being cut off.
pub fn focused_island(active: Memo<Option<Focused>>) -> Show {
    show(
        move || active.with(Option::is_some),
        move || {
            let active_id = memo(move || active.with(|t| t.as_ref().map(|t| t.id)));
            // Each turn a window has with the keyboard gets a row of its own, even when the
            // window had one a moment ago: rows only come up and go up.
            let turn = signal(0u64);
            let rows = signal(vec![0u64]);
            let seen = Cell::new(untrack(|| active_id.get()));
            effect(move || {
                let id = active_id.get();
                if id.is_none() || id == seen.get() {
                    return;
                }
                seen.set(id);
                let gone = untrack(|| turn.get());
                rows.update(|rows| rows.push(gone + 1));
                turn.set(gone + 1);
                after_going(move || {
                    if rows.is_alive() {
                        rows.update(|rows| rows.retain(|r| *r != gone));
                    }
                });
            });
            let width = animated(0.0f32);
            let sized = Rc::new(Cell::new(false));
            island()
                .relative()
                .px(5.0_f32)
                .min_w(0.0)
                .flex_shrink(1.0_f32)
                .overflow_hidden()
                .child(
                    div()
                        .relative()
                        .h_full()
                        .min_w(0.0)
                        .overflow_hidden()
                        .dyn_style(move |s| s.w(width.get()))
                        .child(for_each(
                            move || rows.get(),
                            |row| *row,
                            move |row| {
                                let id = untrack(|| active_id.get());
                                // The window as it was in its turn, kept while it goes.
                                let shown = signal(untrack(|| active.get()));
                                effect(move || {
                                    if turn.get() == row
                                        && let Some(t) = active.get().filter(|t| Some(t.id) == id)
                                    {
                                        shown.set(Some(t));
                                    }
                                });
                                let place = place_of(row == 0, move || turn.get() == row);
                                let sized = sized.clone();
                                window_title(shown)
                                    .absolute()
                                    .top(0.0)
                                    .bottom(0.0)
                                    .left(0.0)
                                    .dyn_style(move |s| {
                                        let p = place.get();
                                        s.opacity((1.0 - p.abs()).max(0.0)).translate_y(-16.0 * p)
                                    })
                                    .on_resized(move |w, _| {
                                        if untrack(|| turn.get()) != row {
                                            return;
                                        }
                                        if sized.replace(true) {
                                            width.spring_to(w, flow());
                                        } else {
                                            width.set(w);
                                        }
                                    })
                            },
                        )),
                )
        },
    )
    .enter(|s| s.opacity(0.0_f32).scale(0.8_f32))
    .exit(|s| s.opacity(0.0_f32).scale(0.8_f32))
    .transition(overshoot(350))
}

/// How the window island and its titles move: quick, without going past.
fn flow() -> Spring {
    Spring::from_ratio(500.0, 1.0)
}
/// How long what goes takes to be gone, with room to spare.
const GOING: Duration = Duration::from_millis(450);
/// How wide the fade at the end of the window island is: as wide as the room after a title, so
/// that it is over nothing until a title is cut off.
const EDGE: f32 = 14.0;

/// Runs `f` once what started going now has gone.
fn after_going(f: impl FnOnce() + 'static) {
    set_timeout(GOING, f);
}

/// Where a row or a title is, from -1 below, where it comes from, through 0 in its place, to 1
/// above, where it goes. It is in its place from the start if `first`, and goes up once `here`
/// is false.
fn place_of(first: bool, here: impl Fn() -> bool + 'static) -> Animated<f32> {
    let place = animated(if first { 0.0f32 } else { -1.0 });
    effect(move || {
        let to = if here() { 0.0 } else { 1.0 };
        if untrack(|| place.get()) != to {
            place.spring_to(to, flow());
        }
    });
    place
}

/// A window's icon and title, on one line.
fn window_title(shown: Signal<Option<Focused>>) -> Div {
    let app_id = memo(move || shown.with(|t| t.as_ref().map(|t| t.app_id.clone())));
    let icon = memo(move || app_id.with(|a| a.as_deref().and_then(|a| app_id_icon(a, 64))));
    let title =
        memo(move || shown.with(|t| t.as_ref().map(|t| t.title.clone()).unwrap_or_default()));
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(8.0_f32)
        .pr(9.0_f32)
        .child(dyn_view(move || match icon.get() {
            Some(path) => img(path).size(22.0).flex_none().into_view(),
            None => div()
                .size(22.0)
                .rounded(11.0_f32)
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .dyn_style(|s| s.bg(colors().secondary_container))
                .child(sym(icons::DESKTOP_WINDOWS, 13.0, |c| c.on_secondary_container))
                .into_view(),
        }))
        .child(changing_title(title))
}

/// A title that, when it changes, comes up from below as the last one goes up. It is as wide as
/// the title it has now; one that goes lies over what is beside it until it has gone. A change of
/// a few characters in one place (a spinner, a counter, a clock) is made where it is, without
/// coming up.
fn changing_title(title: Memo<String>) -> Div {
    let turn = signal(0u64);
    let lines = signal(vec![(0u64, untrack(|| title.get()))]);
    let seen = RefCell::new(untrack(|| title.get()));
    effect(move || {
        let text = title.get();
        let small = {
            let seen = seen.borrow();
            if *seen == text {
                return;
            }
            small_change(&seen, &text)
        };
        seen.replace(text.clone());
        if small {
            lines.update(|lines| {
                if let Some(last) = lines.last_mut() {
                    last.1 = text;
                }
            });
            return;
        }
        let gone = untrack(|| turn.get());
        lines.update(|lines| lines.push((gone + 1, text)));
        turn.set(gone + 1);
        after_going(move || {
            if lines.is_alive() {
                lines.update(|lines| lines.retain(|(t, _)| *t != gone));
            }
        });
    });
    div()
        .relative()
        .flex_none()
        // Unseen, it gives this the width of the title it has now.
        .child(
            txt(BAR_TEXT, move || title.get())
                .flex_none()
                .max_w(320.0)
                .overflow_hidden()
                .opacity(0.0_f32),
        )
        .child(for_each(
            move || lines.with(|lines| lines.iter().map(|(t, _)| *t).collect::<Vec<_>>()),
            |t| *t,
            move |t| {
                let place = place_of(t == 0, move || turn.get() == t);
                txt(BAR_TEXT, move || {
                    lines.with(|lines| {
                        lines
                            .iter()
                            .find(|(l, _)| *l == t)
                            .map(|(_, text)| text.clone())
                            .unwrap_or_default()
                    })
                })
                .flex_none()
                .max_w(320.0)
                .overflow_hidden()
                .absolute()
                .top(0.0)
                .left(0.0)
                .dyn_style(move |s| {
                    let p = place.get();
                    s.opacity((1.0 - p.abs()).max(0.0)).translate_y(-16.0 * p)
                })
            },
        ))
}

/// Whether `new` is `old` with a few characters changed in one place, and at least twice as many
/// kept: what a spinner, a counter or a clock in a title does, not a new title.
fn small_change(old: &str, new: &str) -> bool {
    let (old, new): (Vec<char>, Vec<char>) = (old.chars().collect(), new.chars().collect());
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let kept = head + tail;
    let changed = old.len().max(new.len()) - kept;
    changed <= 4 && kept >= 3.max(2 * changed)
}

/// The time, its digits rolling over as they change, and the date. Hovering makes the time
/// heavier; a click opens the calendar.
fn clock_island(output: Output) -> Div {
    let hovered = signal(false);
    let weight = follow(
        move || if hovered.get() { 760.0 } else { 520.0 },
        fast_spatial(),
    );
    let open = memo({
        let output = output.clone();
        move || is_open(&output, Panel::Calendar)
    });
    let time = move || {
        let s = settings().get();
        let t = local_time(if s.clock_seconds {
            Every::Second
        } else {
            Every::Minute
        });
        let hm = if s.clock_24h { "%H:%M" } else { "%I:%M" };
        t.format(&if s.clock_seconds {
            format!("{hm}:%S")
        } else {
            hm.to_owned()
        })
    };
    let size = 15.0;
    let style = TextStyle {
        size,
        line_height: 20.0,
        letter_spacing: 0.0,
        weight: 500,
    };
    pressable(
        island()
            .px(14.0_f32)
            .gap(8.0_f32)
            .on_hover(move |h| hovered.set(h))
            .on_click(move |_| toggle(&output, Panel::Calendar))
            .dyn_style(move |s| {
                let c = colors();
                if open.get() {
                    s.bg(c.primary_container).text_color(c.on_primary_container)
                } else {
                    s
                }
            })
            .child(
                rolling_text(time, style, move |c| match c {
                    '0'..='9' => size * 0.6,
                    ':' => size * 0.32,
                    _ => size * 0.5,
                })
                .dyn_style(move |s| s.font_axis(b"wght", weight.get())),
            )
            .child(
                txt(TextStyle::LABEL_SMALL, || {
                    local_time(Every::Minute).format("%a %-d %b")
                })
                .dyn_style(move |s| {
                    let c = colors();
                    s.text_color(if open.get() {
                        c.on_primary_container
                    } else {
                        c.on_surface_variant
                    })
                }),
            ),
        |c| c.on_surface,
    )
}

/// What is playing, while something plays: a little equalizer and the title. A click opens the
/// player under it; the middle button pauses it or plays it again.
fn media_island(output: Output) -> Show {
    let m = media();
    let current = memo(move || m.current());
    let open = memo({
        let output = output.clone();
        move || is_open(&output, Panel::Media)
    });
    show(
        move || current.with(|p| p.as_ref().is_some_and(|p| !p.title.is_empty())),
        move || {
            let output = output.clone();
            let playing = memo(move || {
                current.with(|p| {
                    p.as_ref()
                        .is_some_and(|p| p.status == PlaybackStatus::Playing)
                })
            });
            let here = node_ref();
            pressable(
                island()
                    .node_ref(here)
                    .px(12.0_f32)
                    .gap(8.0_f32)
                    .max_w(260.0)
                    .on_click(move |_| {
                        if let Some(r) = here.logical_rect() {
                            crate::media_panel::anchor(r.x + r.width / 2.0);
                        }
                        toggle(&output, Panel::Media);
                    })
                    .on_aux_click(move |e| {
                        if e.button == Some(PointerButton::Middle)
                            && let Some(p) = untrack(|| current.get())
                        {
                            p.play_pause();
                        }
                    })
                    .dyn_style(move |s| {
                        let c = colors();
                        if open.get() {
                            s.bg(c.primary_container).text_color(c.on_primary_container)
                        } else {
                            s
                        }
                    })
                    .child(equalizer(playing))
                    .child(
                        txt(BAR_TEXT, move || {
                            current.with(|p| {
                                p.as_ref()
                                    .map(|p| p.title.chars().take(40).collect())
                                    .unwrap_or_default()
                            })
                        })
                        .min_w(0.0)
                        .overflow_hidden(),
                    ),
                |c| c.on_surface,
            )
        },
    )
    .enter(|s| s.opacity(0.0_f32).scale(0.6_f32))
    .exit(|s| s.opacity(0.0_f32).scale(0.6_f32))
    .transition(overshoot(400))
}

/// Four bars that dance while `playing` and settle into dots when paused.
fn equalizer(playing: Memo<bool>) -> Div {
    let calming = follow(
        move || if playing.get() { 1.0 } else { 0.0 },
        slow_spatial(),
    );
    // Thirty times a second is plenty for bars that dance, and a quarter of the frames.
    let t = clock_at(30, move || calming.get() > 0.001);
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(2.0_f32)
        .h(13.0)
        .flex_none();
    for i in 0..4 {
        row = row.child(div().w(2.5).rounded(1.25_f32).dyn_style(move |s| {
            let level = calming.get();
            // Only asks for frames while it moves.
            let wave = if level > 0.001 {
                let x = t.get() * (5.0 + i as f32 * 1.3) + i as f32 * 1.7;
                (x.sin() * 0.5 + 0.5) * 0.7 + 0.3
            } else {
                0.0
            };
            s.h(2.5 + 10.5 * wave * level).bg(colors().primary)
        }));
    }
    row
}

/// The icons of the tray.
fn tray_island() -> Show {
    show(
        || !tray().items().is_empty(),
        || {
            island().px(5.0_f32).gap(2.0_f32).child(
                for_each(|| tray().items(), |item| item.key(), tray_icon)
                    .enter(|s| s.opacity(0.0_f32).scale(0.4_f32))
                    .exit(|s| s.opacity(0.0_f32).scale(0.4_f32))
                    .transition(overshoot(300)),
            )
        },
    )
    .enter(|s| s.opacity(0.0_f32).scale(0.8_f32))
    .exit(|s| s.opacity(0.0_f32).scale(0.8_f32))
    .transition(overshoot(300))
}

fn tray_icon(item: TrayItem) -> Div {
    let open = signal(false);
    let key = item.key();
    let now = move || tray().items().into_iter().find(|i| i.key() == key);
    let shown = now.clone();
    let clicked = item.clone();
    pressable(
        div()
            .size(24.0)
            .rounded(12.0_f32)
            .flex()
            .items_center()
            .justify_center()
            .child(dyn_view(move || {
                match shown().and_then(|i| i.icon.best(48)) {
                    Some(picture) => bitmap_view(move || Some(picture.clone()))
                        .size(16.0)
                        .into_view(),
                    None => txt(TextStyle::LABEL_MEDIUM, {
                        let shown = shown.clone();
                        move || {
                            shown()
                                .and_then(|i| i.title.chars().next())
                                .map(String::from)
                                .unwrap_or_default()
                        }
                    })
                    .into_view(),
                }
            }))
            .on_pointer_down(move |e| match e.button {
                Some(PointerButton::Secondary) if clicked.has_menu => open.set(true),
                Some(PointerButton::Primary) if clicked.item_is_menu => open.set(true),
                Some(PointerButton::Primary) => {
                    clicked.activate(0, 0);
                }
                _ => {}
            })
            .child(tray_menu(&item, open)),
        |c| c.on_surface,
    )
}

/// The item's menu, in the theme's menus: the separators part it into groups. It follows the item's
/// menu as that changes.
fn tray_menu(item: &TrayItem, open: Signal<bool>) -> View {
    let item = item.clone();
    effect({
        let item = item.clone();
        move || {
            if open.get() {
                // Programs fill some menus only when told they are about to be shown.
                let _ = item.menu_about_to_show(0);
            }
        }
    });
    dyn_view(move || {
        styled_menu(
            move || open.get(),
            move || open.set(false),
            tray_groups(&item, &item.menu()),
            MenuStyle::new().placement(Placement::BelowCenter),
        )
    })
}

fn tray_groups(item: &TrayItem, list: &[TrayMenuItem]) -> Vec<Vec<MenuItem>> {
    let mut groups = vec![Vec::new()];
    for m in list.iter().filter(|m| m.visible) {
        if m.separator {
            groups.push(Vec::new());
            continue;
        }
        let mut row = menu_item(m.label.clone());
        if !m.children.is_empty() {
            row = row.submenu(tray_groups(item, &m.children));
        } else {
            if let Some(MenuToggle::Check(on) | MenuToggle::Radio(on)) = m.toggle {
                row = row.selected_when(move || on);
            }
            let (item, id) = (item.clone(), m.id);
            row = row.on_click(move || {
                let _ = item.menu_clicked(id);
            });
        }
        if !m.enabled {
            row = row.disabled_when(|| true);
        }
        if let Some(keys) = &m.shortcut {
            row = row.trailing(keys.clone());
        }
        groups.last_mut().expect("never empty").push(row);
    }
    groups.retain(|g| !g.is_empty());
    groups
}

/// The size of the words in the bar: smaller than Material's titles, as a bar is.
const BAR_TEXT: TextStyle = TextStyle {
    size: 13.0,
    line_height: 18.0,
    letter_spacing: 0.1,
    weight: 500,
};

/// Network, Bluetooth, sound and battery at a glance; a click opens quick settings.
fn status_island(output: Output) -> Div {
    let net = network();
    let bt = bluetooth();
    let a = audio();
    let b = battery();
    let open = memo({
        let output = output.clone();
        move || is_open(&output, Panel::QuickSettings)
    });
    let color = move |c: &anti_webui_md3e::ColorScheme| c.on_surface_variant;
    pressable(
        island()
            .px(12.0_f32)
            .gap(9.0_f32)
            .on_click(move |_| toggle(&output, Panel::QuickSettings))
            .dyn_style(move |s| {
                if open.get() {
                    s.bg(colors().primary_container)
                } else {
                    s
                }
            })
            .child(dyn_sym(
                move || match net.wifi() {
                    Some(ap) => wifi_icon(ap.strength),
                    None if net.wired().is_some_and(|w| w.connected) => icons::LAN,
                    None if net.connectivity() == Connectivity::None => icons::SIGNAL_WIFI_OFF,
                    None => icons::WIFI_OFF,
                },
                16.0,
                color,
            ))
            .child(show(
                move || bt.available() && bt.adapter().is_some_and(|a| a.powered),
                move || {
                    dyn_sym(
                        move || {
                            if bt.devices().iter().any(|d| d.connected) {
                                icons::BLUETOOTH_CONNECTED
                            } else {
                                icons::BLUETOOTH
                            }
                        },
                        16.0,
                        color,
                    )
                },
            ))
            .child(dyn_sym(
                move || match a.default_sink() {
                    Some(s) if s.muted || s.volume <= 0.001 => icons::VOLUME_OFF,
                    Some(s) if s.volume < 0.5 => icons::VOLUME_DOWN,
                    _ => icons::VOLUME_UP,
                },
                16.0,
                color,
            ))
            .child(show(
                move || b.available(),
                move || {
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(2.0_f32)
                        .child(dyn_sym(
                            move || battery_icon(b.percentage(), b.state()),
                            16.0,
                            color,
                        ))
                        .child(txt(TextStyle::LABEL_MEDIUM, move || {
                            format!("{:.0}%", b.percentage())
                        }))
                },
            ))
            .dyn_style(move |s| {
                if open.get() {
                    s.text_color(colors().on_primary_container)
                } else {
                    s
                }
            }),
        |c| c.on_surface,
    )
}

pub fn wifi_icon(strength: u8) -> &'static anti_webui_md3e::icons::Symbol {
    match strength {
        75.. => icons::SIGNAL_WIFI_4_BAR,
        50.. => icons::NETWORK_WIFI_3_BAR,
        25.. => icons::NETWORK_WIFI_2_BAR,
        5.. => icons::NETWORK_WIFI_1_BAR,
        _ => icons::SIGNAL_WIFI_0_BAR,
    }
}

pub fn battery_icon(
    percentage: f64,
    state: BatteryState,
) -> &'static anti_webui_md3e::icons::Symbol {
    let charging = matches!(
        state,
        BatteryState::Charging | BatteryState::Full | BatteryState::PendingCharge
    );
    if charging {
        return match percentage as u32 {
            95.. => icons::BATTERY_CHARGING_FULL,
            65.. => icons::BATTERY_CHARGING_80,
            _ => icons::BATTERY_CHARGING_50,
        };
    }
    match percentage as u32 {
        95.. => icons::BATTERY_FULL,
        80.. => icons::BATTERY_6_BAR,
        65.. => icons::BATTERY_5_BAR,
        50.. => icons::BATTERY_4_BAR,
        35.. => icons::BATTERY_3_BAR,
        20.. => icons::BATTERY_2_BAR,
        8.. => icons::BATTERY_1_BAR,
        _ => icons::BATTERY_0_BAR,
    }
}

#[cfg(test)]
mod tests {
    use super::small_change;

    #[test]
    fn a_spinner_or_a_counter_is_a_small_change() {
        assert!(small_change("✳ Claude Code", "✶ Claude Code"));
        assert!(small_change("Copying 3/10", "Copying 4/10"));
        assert!(small_change("Downloading 9%", "Downloading 10%"));
        assert!(small_change("12:59 - clock", "13:00 - clock"));
    }

    #[test]
    fn another_title_is_not_a_small_change() {
        assert!(!small_change("New Tab", "New Tab - Google Chrome"));
        assert!(!small_change("✳ Claude Code", "✳ Fixing the window title"));
        assert!(!small_change("", "abc"));
        assert!(!small_change("ab", "ac"));
    }
}
