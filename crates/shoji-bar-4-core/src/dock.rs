//! The dock along the bottom: a button for the launcher, the pinned apps and the other apps with
//! windows open. The icon under the pointer rises a little, an app that is starting bounces until its window
//! shows, and a dot under an app counts its windows.
//!
//! It waits below the edge of the output and comes up when the pointer goes to the bottom edge,
//! going down again a moment after the pointer leaves it. Windows have all of the output.

use crate::appinfo::{app_by_id, app_for, app_icon, bare, icon_file};
use crate::panels::{Panel, is_open, toggle};
use crate::settings::settings;
use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::core::geometry::RectI;
use anti_webui::prelude::*;
use anti_webui::shell::Toplevel;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{MenuItem, MenuStyle, menu_item, styled_menu};
use anti_webui_md3e::icons;
use anti_webui_md3e::motion::{default_spatial, fast_spatial};
use std::time::{Duration, Instant};

/// The square an icon has, unmagnified.
const CELL: f32 = 46.0;
const ICON: f32 = 36.0;
/// How far the icon under the pointer rises.
const RISE: f32 = 6.0;
const PAD: f32 = 7.0;
/// How far an icon at rest stands above the bottom of its cell: in its middle.
const LIFT: f32 = (CELL - ICON) / 2.0;
const MARGIN: f32 = 12.0;
/// The body of the dock; what grows reaches above it, into the rest of the layer.
const BODY: f32 = CELL + 2.0 * PAD;
const LAYER: i32 = 136;
/// How tall the strip along the bottom edge is that brings the dock up.
const EDGE: f32 = 2.0;
/// How far down the dock goes to be out of sight, its shadow too.
const DROP: f32 = BODY + MARGIN + 28.0;
/// How long the dock stays after the pointer has left it.
const STAYS: Duration = Duration::from_millis(600);

pub fn open(output: Output) {
    let region = signal(InputRegion::Empty);
    let layer = open_layer(
        LayerOptions::new()
            .anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
            .height(LAYER)
            .output(&output)
            .namespace("shoji-bar-4-dock")
            .input_region(InputRegion::Empty),
        move || view(output, region),
    );
    effect(move || layer.set_input_region(region.get()));
}

/// Something the dock shows: an installed app, or windows of something that isn't one.
#[derive(Clone)]
struct Entry {
    key: String,
    app: Option<Application>,
    /// The `app_id` of its windows, when it isn't an installed app.
    app_id: String,
    pinned: bool,
}

impl Entry {
    fn owns(&self, window: &Toplevel) -> bool {
        match &self.app {
            Some(app) => app_for(&window.app_id).is_some_and(|a| a.id == app.id),
            None => window.app_id == self.app_id,
        }
    }

    fn name(&self) -> String {
        self.app
            .as_ref()
            .map_or_else(|| self.app_id.clone(), |a| a.name.clone())
    }
}

fn entries() -> Vec<Entry> {
    let pinned = settings().with(|s| s.pinned.clone());
    let mut list: Vec<Entry> = pinned
        .iter()
        .filter_map(|id| app_by_id(id))
        .map(|app| Entry {
            key: app.id.clone(),
            app_id: String::new(),
            app: Some(app),
            pinned: true,
        })
        .collect();
    // A window without an `app_id` is one on its way (a tab being dragged out, a dialog that
    // hasn't said whose it is yet): there is nothing to show for it.
    for window in toplevels().all().into_iter().filter(|w| !w.app_id.is_empty()) {
        let entry = match app_for(&window.app_id) {
            Some(app) => Entry {
                key: app.id.clone(),
                app_id: String::new(),
                app: Some(app),
                pinned: false,
            },
            None => Entry {
                key: format!("window:{}", window.app_id),
                app_id: window.app_id.clone(),
                app: None,
                pinned: false,
            },
        };
        if !list.iter().any(|e| e.key == entry.key) {
            list.push(entry);
        }
    }
    list
}

pub fn view(output: Output, region: Signal<InputRegion>) -> Div {
    let list = memo(move || {
        entries()
            .into_iter()
            .map(|e| (e.key.clone(), e.pinned))
            .collect::<Vec<_>>()
    });
    let menu_open = signal(None::<String>);
    let layer_size = signal((0.0f32, 0.0f32));
    let body_size = signal((0.0f32, 0.0f32));
    // Whether the pointer is over what of the layer takes it: the edge, or the dock while it is up.
    let inside = signal(false);
    let up = signal(false);
    // Each change of mind counts, so that only the last leaving takes the dock down.
    let changes = std::rc::Rc::new(std::cell::Cell::new(0u32));
    effect(move || {
        let wanted = inside.get() || menu_open.get().is_some();
        let n = changes.get() + 1;
        changes.set(n);
        if wanted {
            up.set(true);
        } else if untrack(|| up.get()) {
            let changes = changes.clone();
            set_timeout(STAYS, move || {
                if changes.get() == n {
                    up.set(false);
                }
            });
        }
    });
    let down = animated(1.0f32);
    effect(move || {
        if up.get() {
            down.spring_to(0.0, default_spatial());
        } else {
            down.to(1.0, emphasized_accelerate(250));
        }
    });
    // Clicks go through all of the layer but the edge and, while it is up, the dock and what is
    // below it, unless it is busy.
    effect(move || {
        let (lw, lh) = layer_size.get();
        let (bw, bh) = body_size.get();
        let busy = menu_open.get().is_some();
        let edge = RectI::new(0, (lh - EDGE).floor() as i32, lw.ceil() as i32, EDGE as i32 + 1);
        region.set(if busy {
            InputRegion::Full
        } else if lw <= 0.0 {
            InputRegion::Empty
        } else if up.get() && bw > 0.0 {
            let top = (lh - MARGIN - bh).floor() as i32;
            InputRegion::Rects(vec![
                RectI::new(
                    ((lw - bw) / 2.0).floor() as i32,
                    top,
                    bw.ceil() as i32 + 1,
                    lh.ceil() as i32 - top,
                ),
                edge,
            ])
        } else {
            InputRegion::Rects(vec![edge])
        });
    });
    let launcher_output = output.clone();
    // The pinned apps, or the others.
    let items = move |pinned: bool| {
        for_each(
            move || {
                list.get()
                    .into_iter()
                    .filter(|(_, p)| *p == pinned)
                    .collect::<Vec<_>>()
            },
            |(key, _)| key.clone(),
            move |(key, _)| item(key, menu_open),
        )
        .enter(|s| s.opacity(0.0_f32).scale(0.2_f32))
        .exit(|s| s.opacity(0.0_f32).scale(0.2_f32))
        .transition(overshoot(420))
    };

    let divider_at = memo(move || {
        list.with(|l| {
            let pinned = l.iter().filter(|(_, p)| *p).count();
            (pinned > 0 && pinned < l.len()).then_some(pinned)
        })
    });

    div()
        .size_full()
        .relative()
        .select_none()
        .flex()
        .flex_col()
        .justify_end()
        .items_center()
        .pb(MARGIN)
        .on_resized(move |w, h| layer_size.set((w, h)))
        .on_hover(move |on| inside.set(on))
        .child(
            div()
                .flex()
                .flex_row()
                .items_end()
                .h(BODY)
                .px(PAD)
                .py(PAD)
                .rounded(BODY / 2.0)
                .shadows(elevation(3))
                .dyn_style(move |s| {
                    s.bg(colors().surface_container)
                        .translate_y(down.get() * DROP)
                })
                .on_resized(move |w, h| body_size.set((w, h)))
                .child(launcher_button(launcher_output))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_end()
                        .child(items(true))
                        .child(show(
                            move || divider_at.get().is_some(),
                            || {
                                div()
                                    .w(1.0)
                                    .h(26.0)
                                    .mx(7.0)
                                    .mb(10.0)
                                    .dyn_style(|s| s.bg(colors().outline_variant))
                            },
                        ))
                        .child(items(false)),
                ),
        )
}

/// The cell of an app: its icon, which rises while hovered with its name above it, and
/// its windows as dots below.
fn item(
    key: String,
    menu_open: Signal<Option<String>>,
) -> Div {
    let lookup = {
        let key = key.clone();
        move || entries().into_iter().find(|e| e.key == key)
    };
    let windows = memo({
        let lookup = lookup.clone();
        move || {
            let Some(e) = lookup() else { return Vec::new() };
            toplevels()
                .all()
                .into_iter()
                .filter(|t| e.owns(t))
                .collect::<Vec<_>>()
        }
    });
    // A minimized window can stay `activated` while nothing else takes the keyboard.
    let focused = memo(move || windows.with(|w| w.iter().any(|t| t.activated && !t.minimized)));
    let hovered = signal(false);
    let rise = follow(move || if hovered.get() { RISE } else { 0.0 }, fast_spatial());
    let pressed = signal(false);
    let squash = follow(
        move || if pressed.get() { 0.86 } else { 1.0 },
        fast_spatial(),
    );
    // Bounces from the click that starts it until a window of it shows, for at most 4 seconds.
    let launching = signal(None::<Instant>);
    let count = memo(move || windows.with(Vec::len));
    let before = std::cell::Cell::new(0usize);
    effect(move || {
        let n = count.get();
        if n > before.replace(n) {
            launching.set(None);
        }
    });
    let since_launch = clock_while(move || launching.get().is_some());
    let icon_path = memo({
        let lookup = lookup.clone();
        move || {
            let e = lookup()?;
            match &e.app {
                Some(app) => app_icon(app, 128),
                None => {
                    icon_file(&e.app_id, 128).or_else(|| icon_file(&e.app_id.to_lowercase(), 128))
                }
            }
        }
    });
    let name = memo({
        let lookup = lookup.clone();
        move || lookup().map(|e| e.name()).unwrap_or_default()
    });

    let click = {
        let lookup = lookup.clone();
        move |_: &PointerEvent| {
            let Some(e) = lookup() else { return };
            let open = untrack(|| windows.get());
            if open.is_empty() {
                if let Some(app) = &e.app {
                    launching.set(Some(Instant::now()));
                    app.launch();
                    set_timeout(Duration::from_secs(4), move || launching.set(None));
                }
            } else if let Some(active) = open.iter().find(|t| t.activated) {
                if active.minimized {
                    // After a minimize that left nothing else to take the
                    // keyboard, the WM can keep this window `activated`; a
                    // click on a minimized entry then means restore, not
                    // minimize again, and the one restored is the one that
                    // was minimized even when the app has others.
                    active.activate();
                } else if open.len() > 1 {
                    // Goes round its windows.
                    let at = open.iter().position(|t| t.id() == active.id()).unwrap_or(0);
                    open[(at + 1) % open.len()].activate();
                } else {
                    active.set_minimized(true);
                }
            } else if let Some(last) = open.last() {
                last.activate();
            }
        }
    };

    let menu_key = key.clone();
    let shown_key = key.clone();
    let groups = {
        let lookup = lookup.clone();
        move || menu_groups(lookup(), untrack(|| windows.get()), launching)
    };
    div()
        .relative()
        .flex_none()
        .w(CELL)
        .h(CELL)
        .on_hover(move |h| hovered.set(h))
        .on_active(move |a| pressed.set(a))
        .on_click(click)
        .on_pointer_down(move |e| {
            if e.button == Some(PointerButton::Secondary) {
                menu_open.set(Some(menu_key.clone()));
            }
        })
        // The name, above it.
        .child(
            show(
                move || hovered.get() && menu_open.get().is_none(),
                move || {
                    div()
                        .absolute()
                        .left(-60.0)
                        .right(-60.0)
                        .flex()
                        .justify_center()
                        .pointer_events_none()
                        .dyn_style(move |s| s.bottom(LIFT + ICON + rise.get() + 10.0))
                        .child(
                            txt(TextStyle::LABEL_MEDIUM, move || name.get())
                                .px(10.0_f32)
                                .py(5.0_f32)
                                .rounded(8.0_f32)
                                .shadows(elevation(2))
                                .dyn_style(|s| {
                                    let c = colors();
                                    s.bg(c.inverse_surface).text_color(c.inverse_on_surface)
                                }),
                        )
                },
            )
            .enter(|s| s.opacity(0.0_f32).translate_y(8.0_f32).scale(0.8_f32))
            .exit(|s| s.opacity(0.0_f32).translate_y(4.0_f32).scale(0.9_f32))
            .transition(overshoot(260)),
        )
        // The icon, standing on the bottom of the cell.
        .child(
            div()
                .absolute()
                .bottom(LIFT)
                .left(0.0)
                .right(0.0)
                .flex()
                .justify_center()
                .pointer_events_none()
                .transform_origin(0.5, 1.0)
                .h(ICON)
                .dyn_style(move |s| {
                    let jump = if launching.get().is_some() {
                        -((since_launch.get() * std::f32::consts::PI * 2.2)
                            .sin()
                            .abs())
                            * 13.0
                    } else {
                        0.0
                    };
                    s.translate_y(jump - rise.get()).scale(squash.get())
                })
                .child(dyn_view(move || match icon_path.get() {
                    Some(path) => img(path)
                        .object_fit(ObjectFit::Contain)
                        .size(ICON)
                        .into_view(),
                    None => div()
                        .rounded(11.0_f32)
                        .flex()
                        .items_center()
                        .justify_center()
                        .dyn_style(move |s| {
                            let c = colors();
                            s.size(ICON)
                                .bg(c.tertiary_container)
                                .text_color(c.on_tertiary_container)
                        })
                        .child(
                            TextStyle::TITLE_MEDIUM.apply(div()).child(
                                name.get()
                                    .chars()
                                    .next()
                                    .map(|c| c.to_uppercase().to_string())
                                    .unwrap_or_default(),
                            ),
                        )
                        .into_view(),
                })),
        )
        // Its windows, under it.
        .child(
            div()
                .absolute()
                .bottom(-3.0)
                .left(0.0)
                .right(0.0)
                .flex()
                .justify_center()
                .gap(3.0_f32)
                .pointer_events_none()
                .child(
                    div()
                        .h(3.0)
                        .rounded(1.5_f32)
                        .transition_of(
                            [Prop::Width, Prop::Background, Prop::Opacity],
                            overshoot(380),
                        )
                        .dyn_style(move |s| {
                            let c = colors();
                            let n = count.get();
                            let w = match (n, focused.get()) {
                                (0, _) => 3.0,
                                (_, true) => 14.0,
                                (1, _) => 5.0,
                                _ => 10.0,
                            };
                            s.w(w)
                                .opacity(if n == 0 { 0.0 } else { 1.0_f32 })
                                .bg(if focused.get() {
                                    c.primary
                                } else {
                                    c.on_surface_variant
                                })
                        }),
                ),
        )
        .child(styled_menu(
            move || menu_open.get().as_deref() == Some(shown_key.as_str()),
            move || menu_open.set(None),
            groups(),
            MenuStyle::new().placement(Placement::AboveCenter),
        ))
}

fn menu_groups(
    entry: Option<Entry>,
    windows: Vec<Toplevel>,
    launching: Signal<Option<Instant>>,
) -> Vec<Vec<MenuItem>> {
    let Some(entry) = entry else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    if let Some(app) = entry.app.clone() {
        let mut first = vec![menu_item("New window").icon(icons::ADD).on_click({
            let app = app.clone();
            move || {
                launching.set(Some(Instant::now()));
                app.launch();
                set_timeout(Duration::from_secs(4), move || launching.set(None));
            }
        })];
        for action in app.actions.clone() {
            first.push(menu_item(action.name.clone()).on_click(move || {
                action.launch();
            }));
        }
        groups.push(first);
        let id = bare(&app.id).to_owned();
        groups.push(vec![if entry.pinned {
            menu_item("Unpin").icon(icons::PUSH_PIN).on_click(move || {
                settings().update(|s| s.pinned.retain(|p| bare(p) != id));
            })
        } else {
            menu_item("Pin to dock")
                .icon(icons::PUSH_PIN)
                .on_click(move || {
                    settings().update(|s| s.pinned.push(id.clone()));
                })
        }]);
    }
    if !windows.is_empty() {
        let label = if windows.len() == 1 {
            "Close".to_owned()
        } else {
            format!("Close {} windows", windows.len())
        };
        groups.push(vec![menu_item(label).icon(icons::CLOSE).on_click(
            move || {
                for w in &windows {
                    w.close();
                }
            },
        )]);
    }
    groups
}

/// The button that opens the launcher: a square that rounds into a circle and turns its icon when
/// the pointer is near, and stays a circle while the launcher is open.
fn launcher_button(output: Output) -> Div {
    let hovered = signal(false);
    let rise = follow(move || if hovered.get() { RISE } else { 0.0 }, fast_spatial());
    let open = memo({
        let output = output.clone();
        move || is_open(&output, Panel::Launcher)
    });
    div()
        .relative()
        .flex_none()
        .w(CELL)
        .h(CELL)
        .on_hover(move |h| hovered.set(h))
        .on_click(move |_| toggle(&output, Panel::Launcher))
        .child(
            div()
                .absolute()
                .bottom(LIFT)
                .left(0.0)
                .right(0.0)
                .flex()
                .justify_center()
                .items_end()
                .pointer_events_none()
                .child(
                    launcher_glyph(open, hovered, || 19.0)
                        .size(ICON)
                        .dyn_style(move |s| s.translate_y(-rise.get())),
                ),
        )
}
