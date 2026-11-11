//! The music panel with the cover of the song behind it, blurred, so the colours of the cover mix
//! into the background (the stock panel only shows covers that are local files; Spotifast's are
//! web addresses). The covers are downloaded once into `~/.cache/shoji-bar-4/covers`.
//!
//! Everything here is the stock `media_panel` but for `cover`, `art_of` and `backdrop`.

use anti_webui::core::task::task;

/// The panel, to register in place of the stock one.
pub fn panel() -> PanelDef {
    PanelDef::new(shoji_default::panels::Panel::Media, content)
        .place(|d| d.items_start().justify_start().pt(46.0_f32))
        .enter(from_above)
}

/// The file with the cover of a song: the file itself, or a download of its web address, which is
/// kept for the next time. `None` until the download is there.
fn cache_file(url: &str) -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("shoji-bar-4/covers");
    // FNV-1a: a name for the file that is the same for the same address.
    let hash = url
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    Some(dir.join(format!("{hash:016x}")))
}

fn fetch(url: String, file: PathBuf) -> bool {
    let _ = std::fs::create_dir_all(file.parent().unwrap());
    let part = file.with_extension("part");
    let ok = std::process::Command::new("curl")
        .args(["-sfL", "--max-time", "15", "-o"])
        .arg(&part)
        .arg(&url)
        .status()
        .is_ok_and(|s| s.success());
    ok && std::fs::rename(&part, &file).is_ok()
}

use shoji_default::panels::{PanelDef, close, from_above};
use shoji_core::theme::{TextStyle, colors, elevation};
use shoji_core::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{sized_icon, wavy_progress_eased};
use anti_webui_md3e::icons;
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

/// The panel's, with the card of what is playing inside it.
const WIDTH: f32 = 392.0;
/// How far the panel keeps from the sides of the output.
const EDGE: f32 = 14.0;
/// How tall the place that takes the pointer around the seek bar is.
const SEEK_H: f32 = 28.0;

/// `1:05`, `1:02:03`.
fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn url_now(current: Memo<Option<Player>>) -> Option<String> {
    current.with(|p| p.as_ref().and_then(|p| p.art_url.clone()))
}

fn content(output: Output) -> Div {
    let m = media();
    let current = memo(move || m.current());
    // When it stops being there, so does this.
    effect(move || {
        if current.with(Option::is_none) {
            close();
        }
    });
    // The address of the cover, and the file it is in once there is one.
    let url = memo(move || current.with(|p| p.as_ref().and_then(|p| p.art_url.clone())));
    let local: Signal<Option<String>> = signal(None);
    effect(move || {
        let Some(url) = url.get() else {
            local.set(None);
            return;
        };
        if let Some(file) = url.strip_prefix("file://") {
            local.set(Some(file.to_owned()));
            return;
        }
        let Some(file) = cache_file(&url) else { return };
        if file.is_file() {
            local.set(Some(file.display().to_string()));
            return;
        }
        local.set(None);
        let wanted = url.clone();
        task(move || fetch(url, file.clone()).then(|| file.display().to_string())).then(
            move |done| {
                // Only if it is still the song that plays.
                if untrack(|| url_now(current)) == Some(wanted) {
                    local.set(done);
                }
            },
        );
    });
    let art = memo(move || local.get());
    let playing = memo(move || {
        current.with(|p| {
            p.as_ref()
                .is_some_and(|p| p.status == PlaybackStatus::Playing)
        })
    });
    let has_art = move || art.with(Option::is_some);
    let fg = move |c: &anti_webui_md3e::ColorScheme| {
        if has_art() {
            rgb(0xffffff)
        } else {
            c.on_secondary_container
        }
    };
    let width = output.logical_size.width as f32;
    let center = shoji_default::media_panel::anchor_center().unwrap_or(width - EDGE - WIDTH / 2.0);
    let left = (center - WIDTH / 2.0).clamp(EDGE, (width - WIDTH - EDGE).max(EDGE));

    // The panel, like the others: the theme's color around a card, which has the cover behind it.
    div()
        .ml(left)
        .w(WIDTH)
        .p(16.0)
        .rounded(30.0)
        .shadows(elevation(3))
        .dyn_style(|s| s.bg(colors().surface_container))
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .gap(14.0)
                .p(16.0)
                .rounded(22.0)
                .overflow_hidden()
                // Its own stacking context, so that the cover and its veil stay behind the rest.
                .z_index(0)
                .dyn_style(move |s| {
                    let c = colors();
                    s.bg(c.secondary_container).text_color(fg(&c))
                })
                .child(backdrop(art))
                .child(about(current, art, fg))
                .child(seek_bar(current, playing, fg))
                .child(buttons(current, playing, fg)),
        )
}

/// The cover over all of the panel, darkened so that what is on it can be read; a new one fades in
/// over the old one.
fn backdrop(art: Memo<Option<String>>) -> Div {
    div()
        .absolute()
        .inset(0.0)
        .z_index(-1)
        .pointer_events_none()
        .child(
            for_each(
                move || art.get().into_iter().collect::<Vec<_>>(),
                Clone::clone,
                |path| {
                    div()
                        .absolute()
                        .inset(0.0)
                        // The blurred layer is bigger than the card: it is cut to the card's
                        // rounded corners here.
                        .rounded(22.0)
                        .overflow_hidden()
                        .child(
                            // Blurred until it is only its colours, scaled up so that the blur
                            // does not fade at the edges of the card.
                            div()
                                .absolute()
                                .inset(0.0)
                                .bg_image(path)
                                .bg_size(BackgroundSize::Cover)
                                .blur(38.0)
                                .scale(1.3_f32),
                        )
                        .child(
                            div()
                                .absolute()
                                .inset(0.0)
                                .bg(rgba(0x000000).with_alpha(0.32)),
                        )
                },
            )
            .appear()
            .enter(|s| s.opacity(0.0).scale(1.1))
            .exit(|s| s.opacity(0.0))
            .transition(emphasized(600)),
        )
}

/// The cover, the title, who it is by, and what plays it. A new title slides in over the old one.
fn about(
    current: Memo<Option<Player>>,
    art: Memo<Option<String>>,
    fg: impl shoji_core::ui::Role,
) -> Div {
    let title = memo(move || current.with(|p| p.as_ref().map(|p| p.title.clone())));
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(14.0)
        .child(
            div()
                .size(72.0)
                .flex_none()
                .relative()
                .rounded(18.0)
                .overflow_hidden()
                .dyn_style(|s| s.bg(colors().primary_container))
                .child(
                    div()
                        .absolute()
                        .inset(0.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(sym(icons::MUSIC_NOTE, 32.0, |c| c.on_primary_container)),
                )
                .child(
                    for_each(
                        move || art.get().into_iter().collect::<Vec<_>>(),
                        Clone::clone,
                        |path| img(path).absolute().inset(0.0).object_fit(ObjectFit::Cover),
                    )
                    .appear()
                    .enter(|s| s.opacity(0.0).scale(0.8))
                    .exit(|s| s.opacity(0.0))
                    .transition(overshoot(500)),
                ),
        )
        .child(
            div()
                .relative()
                .flex_1()
                .min_w(0.0)
                .h(64.0)
                .overflow_hidden()
                .child(
                    for_each(
                        move || vec![title.get()],
                        Clone::clone,
                        move |_| {
                            let p = untrack(|| current.get());
                            let title = p.as_ref().map(|p| p.title.clone()).unwrap_or_default();
                            let by = p.as_ref().map(|p| p.artists.join(", ")).unwrap_or_default();
                            let player = p.map(|p| p.identity).unwrap_or_default();
                            let mut column = div()
                                .absolute()
                                .inset(0.0)
                                .flex()
                                .flex_col()
                                .justify_center()
                                .child(
                                    static_txt(TextStyle::TITLE_MEDIUM, title).overflow_hidden(),
                                );
                            if !by.is_empty() {
                                column = column.child(
                                    static_txt(TextStyle::BODY_MEDIUM, by)
                                        .overflow_hidden()
                                        .opacity(0.85),
                                );
                            }
                            column.child(
                                static_txt(TextStyle::LABEL_SMALL, player)
                                    .overflow_hidden()
                                    .opacity(0.65),
                            )
                        },
                    )
                    .enter(|s| s.opacity(0.0).translate_x(40.0))
                    .exit(|s| s.opacity(0.0).translate_x(-40.0))
                    .transition(emphasized(450)),
                )
                .dyn_style(move |s| s.text_color(fg(&colors()))),
        )
}

/// The wavy line of how far it has played, with a handle that comes under the pointer. A click goes
/// there; dragging shows where it would go and goes there when let go.
fn seek_bar(current: Memo<Option<Player>>, playing: Memo<bool>, fg: impl shoji_core::ui::Role) -> Div {
    let can_seek = memo(move || {
        current.with(|p| {
            p.as_ref()
                .is_some_and(|p| p.can_seek && p.length.is_some_and(|l| !l.is_zero()))
        })
    });
    // Where it is dragged to, and for a moment after it is let go, until the player has gone there.
    let held: Signal<Option<f32>> = signal(None);
    let pending: Cell<Option<Timeout>> = Cell::new(None);
    // Smooth while it plays, a little under the frames the wave asks for anyway.
    let tick = clock_at(30, move || playing.get());
    let played = move || {
        tick.get();
        local_time(Every::Second);
        current.with(|p| match p.as_ref() {
            Some(p) => match p.length {
                Some(len) if !len.is_zero() => {
                    (p.position().as_secs_f32() / len.as_secs_f32()).clamp(0.0, 1.0)
                }
                _ => 0.0,
            },
            None => 0.0,
        })
    };
    let progress = move || held.get().unwrap_or_else(played);
    let length = move || {
        current
            .with(|p| p.as_ref().and_then(|p| p.length))
            .unwrap_or_default()
    };
    let go = std::rc::Rc::new(move |ratio: f32| {
        held.set(Some(ratio));
        if let Some(p) = untrack(|| current.get())
            && let Some(len) = p.length
        {
            drop(p.set_position(len.mul_f32(ratio)));
        }
        if let Some(t) = pending.take() {
            t.cancel();
        }
        pending.set(Some(set_timeout(Duration::from_millis(700), move || {
            held.set(None)
        })));
    });
    let go_too = go.clone();
    let ratio_of = |local_x: f32, width: i32| (local_x / width.max(1) as f32).clamp(0.0, 1.0);
    let hovered = signal(false);
    let dragging = signal(false);
    let handle = follow(
        move || {
            if dragging.get() {
                1.0f32
            } else if hovered.get() && can_seek.get() {
                0.75
            } else {
                0.0
            }
        },
        fast_spatial(),
    );
    let width = signal(0.0f32);

    let bar = div()
        .relative()
        .h(SEEK_H)
        .flex()
        .items_center()
        .on_resized(move |w, _| width.set(w))
        .child(wavy_progress_eased(progress, Transition::ms(120).ease_out()).w_full())
        // The handle, a short upright bar at the end of the line.
        .child(
            div()
                .absolute()
                .top(0.0)
                .w(6.0)
                .rounded(3.0)
                .pointer_events_none()
                .dyn_style(move |s| {
                    let t = handle.get().max(0.0);
                    let h = 8.0 + 16.0 * t;
                    s.left(progress() * width.get() - 3.0)
                        .top((SEEK_H - h) / 2.0)
                        .h(h)
                        .bg(colors().primary)
                        .opacity(t.min(1.0))
                }),
        )
        .on_hover(move |h| hovered.set(h))
        .on_click(move |e| {
            if untrack(|| can_seek.get()) {
                go(ratio_of(e.local.x, e.rect.width));
            }
        })
        .on_grab(move |e| {
            if !untrack(|| can_seek.get()) {
                return;
            }
            let ratio = ratio_of(e.local.x, e.rect.width);
            match e.phase {
                GrabPhase::Start | GrabPhase::Move => {
                    dragging.set(true);
                    held.set(Some(ratio));
                }
                GrabPhase::End => {
                    dragging.set(false);
                    go_too(ratio);
                }
                GrabPhase::Cancel => {
                    dragging.set(false);
                    held.set(None);
                }
            }
        })
        .dyn_style(move |s| {
            if can_seek.get() {
                s.cursor(CursorIcon::Pointer)
            } else {
                s
            }
        });

    let times = div()
        .flex()
        .flex_row()
        .justify_between()
        .px(2.0)
        .child(txt(TextStyle::LABEL_SMALL, move || {
            clock(length().mul_f32(progress()))
        }))
        .child(txt(TextStyle::LABEL_SMALL, move || clock(length())))
        .opacity(0.8)
        .dyn_style(move |s| s.text_color(fg(&colors())));

    div().flex().flex_col().gap(2.0).child(bar).child(times)
}

/// Back, play or pause, and forward. The middle one is a big rounded square that goes round while
/// it plays.
fn buttons(current: Memo<Option<Player>>, playing: Memo<bool>, fg: impl shoji_core::ui::Role) -> Div {
    let morph = follow(
        move || if playing.get() { 1.0 } else { 0.0 },
        fast_spatial(),
    );
    let pressed = signal(false);
    let squash = follow(
        move || if pressed.get() { 0.9 } else { 1.0 },
        fast_spatial(),
    );
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(20.0)
        .child(icon_btn(icons::SKIP_PREVIOUS, 44.0, fg).on_click(move |_| {
            if let Some(p) = current.get() {
                drop(p.previous());
            }
        }))
        .child(pressable(
            div()
                .w(72.0)
                .h(56.0)
                .flex()
                .items_center()
                .justify_center()
                .flex_none()
                .on_active(move |a| pressed.set(a))
                .on_click(move |_| {
                    if let Some(p) = current.get() {
                        drop(p.play_pause());
                    }
                })
                .dyn_style(move |s| {
                    let c = colors();
                    let t = morph.get();
                    s.rounded(16.0 + 12.0 * t)
                        .bg(c.primary)
                        .text_color(c.on_primary)
                        .scale(squash.get())
                })
                .child(dyn_view(move || {
                    sized_icon(
                        if playing.get() {
                            icons::PAUSE
                        } else {
                            icons::PLAY_ARROW
                        },
                        28.0,
                    )
                })),
            |c| c.on_primary,
        ))
        .child(icon_btn(icons::SKIP_NEXT, 44.0, fg).on_click(move |_| {
            if let Some(p) = current.get() {
                drop(p.next());
            }
        }))
}
