//! The launcher: a search field and the apps as a grid that rearranges itself as you type. Arrows
//! move the choice, Enter starts it, Escape closes.

use crate::appinfo::app_icon;
use crate::panels::close;
use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::icons;
use anti_webui_md3e::motion::fast_spatial;
use std::time::Duration;

const COLUMNS: usize = 6;
const CELL_W: f32 = 100.0;
const CELL_H: f32 = 100.0;
/// How many cells come in one after another when it opens; the rest come with the last.
const STAGGERED: usize = COLUMNS * 4;

pub fn panel(output: Output) -> Div {
    let _ = output;
    let query = signal(String::new());
    let picked = signal(0usize);
    let all = apps();
    let results = memo(move || {
        all.search(&query.get())
            .into_iter()
            .map(|a| a.id)
            .collect::<Vec<_>>()
    });
    effect(move || {
        query.with(|_| ());
        picked.set(0);
    });
    let launch = move |id: &str| {
        if let Some(app) = all.get(id) {
            app.launch();
        }
        close();
    };
    let input = node_ref();
    request_frame(move |_| input.focus());
    // Room for the cells, the padding and the scroll bar.
    let width = COLUMNS as f32 * CELL_W + 40.0 + 12.0;
    let step = move |by: isize| {
        let n = results.with(Vec::len) as isize;
        if n > 0 {
            picked.update(|p| *p = (*p as isize + by).clamp(0, n - 1) as usize);
        }
    };
    div()
        .w(width)
        .flex()
        .flex_col()
        .gap(16.0_f32)
        .p(20.0_f32)
        .rounded(32.0_f32)
        .shadows(elevation(4))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container).text_color(c.on_surface)
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(10.0_f32)
                .h(46.0)
                .px(16.0_f32)
                .rounded(23.0_f32)
                .dyn_style(|s| s.bg(colors().surface_container_highest))
                .child(sym(icons::SEARCH, 20.0, |c| c.on_surface_variant))
                .child(
                    TextStyle::BODY_MEDIUM
                        .apply(text_input())
                        .value(query)
                        .placeholder("Search apps")
                        .select_text()
                        .node_ref(input)
                        .flex_1()
                        .on_submit(move |_| {
                            if let Some(id) =
                                results.with(|r| r.get(untrack(|| picked.get())).cloned())
                            {
                                launch(&id);
                            }
                        })
                        .on_key_down(move |e| match e.key {
                            Key::Named(NamedKey::Escape) => close(),
                            Key::Named(NamedKey::ArrowRight) => step(1),
                            Key::Named(NamedKey::ArrowLeft) => step(-1),
                            Key::Named(NamedKey::ArrowDown) => step(COLUMNS as isize),
                            Key::Named(NamedKey::ArrowUp) => step(-(COLUMNS as isize)),
                            _ => {}
                        }),
                )
                .child(show(
                    move || !query.with(String::is_empty),
                    move || {
                        icon_btn(icons::CLOSE, 30.0, |c| c.on_surface_variant)
                            .on_click(move |_| query.set(String::new()))
                    },
                )),
        )
        .child(
            div()
                .relative()
                .h(CELL_H * 4.0)
                .overflow_y_scroll()
                .flex()
                .flex_row()
                .flex_wrap()
                .content_start()
                .child(
                    for_each(
                        move || results.get(),
                        |id| id.clone(),
                        move |id| {
                            let index = memo({
                                let id = id.clone();
                                move || results.with(|r| r.iter().position(|x| *x == id))
                            });
                            cell(id, index, picked, launch)
                        },
                    )
                    .enter(|s| s.opacity(0.0_f32).scale(0.5_f32))
                    .exit(|s| s.opacity(0.0_f32).scale(0.5_f32))
                    .moves(emphasized(380))
                    .transition(overshoot(360)),
                )
                .child(show(
                    move || all.loaded() && results.with(Vec::is_empty),
                    move || {
                        div()
                            .absolute()
                            .inset(0.0)
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(8.0_f32)
                            .child(sym(icons::SEARCH, 48.0, |c| c.outline))
                            .child(
                                txt(TextStyle::TITLE_MEDIUM, move || {
                                    format!("Nothing called “{}”", query.get())
                                })
                                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                            )
                    },
                )),
        )
}

/// An app: its icon over its name. The chosen one sits on a round tile; the icon hops when the
/// pointer comes.
fn cell(
    id: String,
    index: Memo<Option<usize>>,
    picked: Signal<usize>,
    launch: impl Fn(&str) + Copy + 'static,
) -> Div {
    let app = apps().get(&id);
    let name = app.as_ref().map(|a| a.name.clone()).unwrap_or_default();
    let icon = app.as_ref().and_then(|a| app_icon(a, 128));
    let chosen = memo(move || index.get() == Some(picked.get()));
    let hovered = signal(false);
    let pressed = signal(false);
    let hop = follow(
        move || if hovered.get() { 1.0 } else { 0.0 },
        fast_spatial(),
    );
    let squash = follow(
        move || if pressed.get() { 0.9 } else { 1.0 },
        fast_spatial(),
    );
    let round = follow(move || if chosen.get() { 1.0 } else { 0.0 }, fast_spatial());
    // Comes in a little after the one before it, the first time the grid shows.
    let shown = animated(0.0f32);
    let delay = untrack(|| index.get()).unwrap_or(0).min(STAGGERED) as u64 * 14;
    set_timeout(Duration::from_millis(delay), move || {
        shown.spring_to(1.0, fast_spatial())
    });
    let launch_id = id.clone();
    div()
        .w(CELL_W)
        .h(CELL_H)
        .flex_none()
        .p(4.0_f32)
        .on_hover(move |h| {
            hovered.set(h);
            if h && let Some(i) = index.get() {
                picked.set(i);
            }
        })
        .on_active(move |a| pressed.set(a))
        .on_click(move |_| launch(&launch_id))
        .child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(6.0_f32)
                .transition_of([Prop::Background], Transition::ms(160).ease_out())
                .dyn_style(move |s| {
                    let c = colors();
                    let t = shown.get();
                    s.rounded(18.0 + 28.0 * round.get())
                        .bg(if chosen.get() {
                            c.secondary_container
                        } else {
                            c.secondary_container.with_alpha(0.0)
                        })
                        .opacity(t.clamp(0.0, 1.0))
                        .scale((0.6 + 0.4 * t) * squash.get())
                        .translate_y(14.0 * (1.0 - t))
                })
                .child(
                    div()
                        .size(46.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .dyn_style(move |s| {
                            s.translate_y(-5.0 * hop.get())
                                .scale(1.0 + 0.08 * hop.get())
                        })
                        .child(match icon {
                            Some(path) => img(path)
                                .size(46.0)
                                .object_fit(ObjectFit::Contain)
                                .into_view(),
                            None => TextStyle::TITLE_LARGE
                                .apply(div())
                                .size(46.0)
                                .rounded(15.0_f32)
                                .flex()
                                .items_center()
                                .justify_center()
                                .dyn_style(|s| {
                                    let c = colors();
                                    s.bg(c.tertiary_container)
                                        .text_color(c.on_tertiary_container)
                                })
                                .child(
                                    name.chars()
                                        .next()
                                        .map(|c| c.to_uppercase().to_string())
                                        .unwrap_or_default(),
                                )
                                .into_view(),
                        }),
                )
                .child(
                    static_txt(TextStyle::LABEL_MEDIUM, name.clone())
                        .max_w(CELL_W - 16.0)
                        .overflow_hidden()
                        .dyn_style(move |s| {
                            let c = colors();
                            s.text_color(if chosen.get() {
                                c.on_secondary_container
                            } else {
                                c.on_surface
                            })
                        }),
                ),
        )
}
