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
    /// The track that plays and what was just done to it.
    Media,
    /// A message, with a bar if it has a level.
    Notice,
}

/// What can be done to the player from outside (`shoji-bar-4 msg media ...`).
#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    Next,
    Previous,
    PlayPause,
    Play,
    Pause,
}

impl Action {
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "next" => Self::Next,
            "previous" | "prev" => Self::Previous,
            "play-pause" | "playpause" | "toggle" => Self::PlayPause,
            "play" => Self::Play,
            "pause" => Self::Pause,
            _ => return None,
        })
    }
}

thread_local! {
    /// Shows the pill for an `omarchy-osd` call: icon, text, level from 0 to 1.
    static NOTICE_SHOWN: std::cell::RefCell<Option<std::rc::Rc<dyn Fn(&'static Symbol, String, Option<f32>)>>> =
        const { std::cell::RefCell::new(None) };
    /// Shows the pill for a media action, once `install` has run.
    static MEDIA_SHOWN: std::cell::RefCell<Option<std::rc::Rc<dyn Fn(&'static Symbol)>>> =
        const { std::cell::RefCell::new(None) };
}

/// What `omarchy-osd` sends: `{icon, message, value, progressText, max, duration}`, as JSON. The
/// volume and the brightness are shown from their own state, so they are left out here.
pub fn notice(payload: &str) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else { return };
    let field = |k: &str| json.get(k).and_then(|v| v.as_str()).unwrap_or("").to_owned();
    let icon = field("icon").to_lowercase();
    if icon.starts_with("volume") || matches!(icon.as_str(), "brightness" | "display") {
        return;
    }
    let symbol: &'static Symbol = match icon.as_str() {
        "microphone" | "mic" => icons::MIC,
        "microphone-muted" | "microphone-off" | "mic-muted" | "mic-off" => icons::MIC_OFF,
        "keyboard" => icons::KEYBOARD,
        "touchpad" => icons::TOUCHPAD_MOUSE,
        "touch" | "touchscreen" => icons::TOUCH_APP,
        "reboot" | "restart" => icons::RESTART_ALT,
        "shutdown" | "power" | "poweroff" => icons::POWER_SETTINGS_NEW,
        "logout" | "sign-out" | "leave" => icons::LOGOUT,
        "media-next" | "player-next" => icons::SKIP_NEXT,
        "media-previous" | "player-previous" => icons::SKIP_PREVIOUS,
        "media-play" | "player-play" => icons::PLAY_ARROW,
        "media-pause" | "player-pause" => icons::PAUSE,
        "media" | "player" | "media-source" | "player-source" => icons::MUSIC_NOTE,
        _ => icons::INFO,
    };
    let number = |k: &str| field(k).trim().parse::<f32>().ok();
    let level = number("value").map(|v| (v / number("max").unwrap_or(100.0).max(1.0)).clamp(0.0, 1.0));
    let message = field("message");
    let text = if message.is_empty() { field("progressText") } else { message };
    if let Some(f) = NOTICE_SHOWN.with(|m| m.borrow().clone()) {
        f(symbol, text, level);
    }
}

/// Does `action` to the player that is current, and says so in the pill.
pub fn media_action(action: Action) -> Result<(), String> {
    let player = media().current().ok_or("nothing is playing")?;
    let playing = player.status == PlaybackStatus::Playing;
    let (icon, _task) = match action {
        Action::Next => (icons::SKIP_NEXT, player.next()),
        Action::Previous => (icons::SKIP_PREVIOUS, player.previous()),
        Action::PlayPause if playing => (icons::PAUSE, player.play_pause()),
        Action::PlayPause => (icons::PLAY_ARROW, player.play_pause()),
        Action::Play => (icons::PLAY_ARROW, player.play()),
        Action::Pause => (icons::PAUSE, player.pause()),
    };
    if let Some(f) = MEDIA_SHOWN.with(|m| m.borrow().clone()) {
        f(icon);
    }
    Ok(())
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
    // The pill of the player: the icon of what was done, and the track as it is now.
    let media_icon = signal(icons::MUSIC_NOTE);
    let track = memo(move || {
        media()
            .current()
            .map(|p| (p.title.clone(), p.artists.join(", ")))
            .unwrap_or_default()
    });
    {
        let show_for = show_for.clone();
        MEDIA_SHOWN.with(|m| {
            *m.borrow_mut() = Some(std::rc::Rc::new(move |icon| {
                media_icon.set(icon);
                show_for(Shown::Media);
            }));
        });
    }
    // What `omarchy-osd` asks for that is not the volume or the brightness.
    let notice_icon = signal(icons::INFO);
    let notice_text = signal(String::new());
    let notice_level = signal(None::<f32>);
    {
        let show_for = show_for.clone();
        NOTICE_SHOWN.with(|m| {
            *m.borrow_mut() = Some(std::rc::Rc::new(move |icon, text, level| {
                notice_icon.set(icon);
                notice_text.set(text);
                notice_level.set(level);
                show_for(Shown::Notice);
            }));
        });
    }
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
                    Shown::Media => 0.0,
                    Shown::Notice => notice_level.get().unwrap_or(0.0),
                };
                let symbol = move || -> &'static Symbol {
                    match kind.get() {
                        Shown::Brightness => icons::BRIGHTNESS_6,
                        Shown::Media => icons::MUSIC_NOTE,
                        Shown::Notice => notice_icon.get(),
                        Shown::Volume => match volume.get() {
                            Some(v) if v.1 || v.0 == 0 => icons::VOLUME_OFF,
                            Some(v) if v.0 < 50 => icons::VOLUME_DOWN,
                            _ => icons::VOLUME_UP,
                        },
                    }
                };
                div().p(16.0_f32).select_none().child(
                    show(move || visible.get(), move || {
                        // What it holds follows what was asked for, while the pill is there.
                        div().child(dyn_view(move || match kind.get() {
                            Shown::Media => {
                                media_pill(media_icon, move || track.get()).into_view()
                            }
                            Shown::Notice if notice_level.get().is_none() => {
                                media_pill(notice_icon, move || (notice_text.get(), String::new()))
                                    .into_view()
                            }
                            _ => pill(level, symbol).into_view(),
                        }))
                    })
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

/// The pill for the player: the icon of the action and the title with who it is by.
fn media_pill(
    icon: Signal<&'static Symbol>,
    track: impl Fn() -> (String, String) + Copy + 'static,
) -> Div {
    div()
        .w(320.0)
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
                .child(dyn_sym(move || icon.get(), 17.0, |c| c.on_primary_container)),
        )
        .child(
            div()
                .flex_1()
                .min_w(0.0)
                .overflow_hidden()
                .flex()
                .justify_center()
                .child(txt(TextStyle::LABEL_LARGE, move || {
                    let (title, by) = track();
                    if by.is_empty() { title } else { format!("{title} · {by}") }
                })),
        )
}
