//! Being the notification daemon, when no one else is: notifications pop up at the top right as
//! cards that slide in, and stay in a list in quick settings until they are cleared.

use crate::appinfo::{app_by_id, app_icon, icon_file};
use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::Urgency;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{ButtonSize, ButtonStyle, styled_button};
use anti_webui_md3e::icons;
use std::cell::{Cell, OnceCell};

thread_local! {
    static SERVER: OnceCell<Option<NotificationServer>> = const { OnceCell::new() };
}

/// The notification server, if this got the name on the bus.
pub fn server() -> Option<NotificationServer> {
    SERVER.with(|s| {
        s.get_or_init(|| match notification_server() {
            Ok(server) => Some(server),
            Err(e) => {
                eprintln!("shoji-bar-4: not showing notifications: {e}");
                None
            }
        })
        .clone()
    })
}

thread_local! {
    /// Keyboard mode (`omarchy-shell notifications keyboard`): the popups take the keyboard, one card
    /// is selected. Arrows (or J/K, Tab) move, a number presses that button, Enter clicks the card,
    /// Delete dismisses it, Escape (or ten idle seconds) gives the keyboard back.
    static KB: Cell<Option<(Signal<bool>, Signal<u32>)>> = const { Cell::new(None) };
    static KB_IDLE: std::cell::RefCell<Option<Timeout>> = const { std::cell::RefCell::new(None) };
    static KB_FOCUS: Cell<Option<NodeRef>> = const { Cell::new(None) };
}

/// The signals of keyboard mode, made on first use (on the UI thread).
fn kb() -> (Signal<bool>, Signal<u32>) {
    KB.with(|k| match k.get() {
        Some(pair) => pair,
        None => {
            let pair = (signal(false), signal(0u32));
            k.set(Some(pair));
            pair
        }
    })
}

/// Starts keyboard mode on the newest card. `false` when there is no card on the screen.
pub fn keyboard_start() -> bool {
    let Some(server) = server() else { return false };
    let Some(first) = server.popups().first().map(|n| n.id) else { return false };
    let (on, sel) = kb();
    sel.set(first);
    on.set(true);
    kb_idle();
    if let Some(node) = KB_FOCUS.with(|f| f.get()) {
        request_frame(move |_| node.focus());
    }
    true
}

pub fn keyboard_stop() {
    let (on, _) = kb();
    on.set(false);
    KB_IDLE.with(|t| {
        if let Some(t) = t.borrow_mut().take() {
            t.cancel();
        }
    });
}

fn kb_idle() {
    KB_IDLE.with(|t| {
        if let Some(old) = t.borrow_mut().take() {
            old.cancel();
        }
        *t.borrow_mut() = Some(set_timeout(std::time::Duration::from_secs(10), keyboard_stop));
    });
}

/// What a key does in keyboard mode.
fn kb_key(server: &NotificationServer, key: &Key) {
    let (_, sel) = kb();
    kb_idle();
    let cards: Vec<Notification> = server.popups().into_iter().take(MAX_POPUPS).collect();
    let at = cards.iter().position(|n| n.id == sel.get_untracked());
    let Some(at) = at else {
        keyboard_stop();
        return;
    };
    let me = cards[at].clone();
    match key {
        Key::Named(NamedKey::Escape) => keyboard_stop(),
        Key::Named(NamedKey::ArrowDown | NamedKey::Tab) => {
            if let Some(n) = cards.get(at + 1) { sel.set(n.id) }
        }
        Key::Character(c) if c == "j" => {
            if let Some(n) = cards.get(at + 1) { sel.set(n.id) }
        }
        Key::Named(NamedKey::ArrowUp) => {
            if at > 0 { sel.set(cards[at - 1].id) }
        }
        Key::Character(c) if c == "k" => {
            if at > 0 { sel.set(cards[at - 1].id) }
        }
        Key::Named(NamedKey::Enter) => {
            keyboard_stop();
            activate(&me);
        }
        Key::Named(NamedKey::Delete | NamedKey::Backspace) => {
            // The next one down (or the one above) takes the selection.
            if let Some(n) = cards.get(at + 1).or_else(|| at.checked_sub(1).and_then(|i| cards.get(i))) {
                sel.set(n.id);
            } else {
                keyboard_stop();
            }
            me.dismiss();
        }
        Key::Character(c) => {
            if let Some(n) = c.parse::<usize>().ok().filter(|n| *n >= 1) {
                if let Some(deed) = deeds(&me).get(n - 1) {
                    keyboard_stop();
                    press(&me, deed);
                }
            }
        }
        _ => {}
    }
}

/// A button of a card: an action the program offers (answered over D-Bus), or a command Omarchy's
/// notifications carry in the `omarchy-actions` hint (at most three), run by the shell.
#[derive(Clone)]
pub enum Deed {
    Action(String, String),
    Exec(String, Vec<String>),
}

impl Deed {
    pub fn label(&self) -> &str {
        match self {
            Deed::Action(_, l) | Deed::Exec(l, _) => l,
        }
    }
}

fn argv_of(json: &serde_json::Value) -> Option<Vec<String>> {
    let argv: Vec<String> = json.as_array()?.iter().filter_map(|a| a.as_str().map(str::to_owned)).collect();
    (!argv.is_empty()).then_some(argv)
}

/// What clicking the card runs by itself: the `omarchy-exec-argv` hint.
fn click_argv(n: &Notification) -> Option<Vec<String>> {
    argv_of(&serde_json::from_str(&n.exec_argv).ok()?)
}

/// The buttons of a card, in the order they are drawn and numbered.
pub fn deeds(n: &Notification) -> Vec<Deed> {
    let mut out: Vec<Deed> =
        n.actions.iter().filter(|a| a.0 != "default").map(|a| Deed::Action(a.0.clone(), a.1.clone())).collect();
    if let Ok(serde_json::Value::Array(list)) = serde_json::from_str::<serde_json::Value>(&n.exec_actions) {
        for item in list.iter().take(3) {
            if let (Some(label), Some(argv)) = (item["label"].as_str(), argv_of(&item["argv"])) {
                out.push(Deed::Exec(label.to_owned(), argv));
            }
        }
    }
    out
}

/// Starts a command apart from the shell, which it may outlive.
fn run(argv: &[String]) {
    let _ = std::process::Command::new("setsid")
        .arg("-f")
        .args(argv)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut c| std::thread::spawn(move || c.wait()));
}

/// Presses a button.
pub fn press(n: &Notification, deed: &Deed) {
    match deed {
        Deed::Action(key, _) => n.invoke(key),
        Deed::Exec(_, argv) => {
            run(argv);
            if !n.resident {
                n.dismiss();
            }
        }
    }
}

/// What clicking a card does, as Omarchy's shell does: the command it carries, else the program's
/// `default` action, else bringing the program's window forward; and if it offers only buttons
/// (a screenshot's Open and Edit), the first one. The card goes either way.
pub fn activate(n: &Notification) {
    if let Some(argv) = click_argv(n) {
        run(&argv);
        n.dismiss();
    } else if n.actions.iter().any(|a| a.0 == "default") {
        n.invoke("default");
    } else if let Some(first) = deeds(n).first() {
        press(n, first);
    } else {
        if !n.app_name.is_empty() && n.app_name != "notify-send" {
            run(&["omarchy-hyprland-focus-app".to_owned(), n.app_name.clone()]);
        }
        n.dismiss();
    }
}

const WIDTH: f32 = 352.0;
/// At most this many pop up at once, the newest; the rest are in quick settings.
const MAX_POPUPS: usize = 3;
/// The layer is this tall whatever is in it, so a card coming in is never cut off while the
/// surface would be growing; clicks go through all of it but the cards.
const LAYER_HEIGHT: i32 = 560;

/// The popups, on the first output. Call once, from the root.
pub fn open_popups() {
    let Some(server) = server() else { return };
    // Made here, in the root, so that they outlive any card that reads them.
    kb();
    let any = memo({
        let server = server.clone();
        move || !server.popups().is_empty()
    });
    // The layer is there while there is something to show, and a moment longer for it to leave.
    let mounted = signal(false);
    let pending: std::cell::Cell<Option<Timeout>> = std::cell::Cell::new(None);
    effect(move || {
        if let Some(t) = pending.take() {
            t.cancel();
        }
        if any.get() {
            mounted.set(true);
        } else {
            pending.set(Some(set_timeout(
                std::time::Duration::from_millis(700),
                move || mounted.set(false),
            )));
        }
    });
    // Made out here, not in the effect: a layout report of a layer that has just gone can still
    // come, and setting a signal that went with it panics.
    let region = signal(InputRegion::Empty);
    effect(move || {
        if !mounted.get() {
            return;
        }
        let server = server.clone();
        region.set(InputRegion::Empty);
        let layer = open_layer(
            LayerOptions::new()
                .layer(Layer::Overlay)
                .anchor(Anchor::TOP | Anchor::RIGHT)
                .margin(Margin {
                    top: 6,
                    right: 10,
                    ..Default::default()
                })
                .width(WIDTH as i32 + 28)
                .height(LAYER_HEIGHT)
                .input_region(InputRegion::Empty)
                .namespace("shoji-bar-4-notifications"),
            move || popups(server.clone(), region),
        );
        let (on, _) = kb();
        effect(move || {
            layer.set_input_region(region.get());
            layer.set_keyboard(if on.get() { Keyboard::Exclusive } else { Keyboard::None });
        });
    });
}

/// The cards, newest on top. Those below make room for a new one and close up after one that
/// goes, sliding to their places; `region` follows where the cards are.
pub fn popups(server: NotificationServer, region: Signal<InputRegion>) -> Div {
    let node = node_ref();
    KB_FOCUS.with(|f| f.set(Some(node)));
    let key_server = server.clone();
    div()
        .node_ref(node)
        .focusable()
        .on_key_down(move |e| {
            if kb().0.get_untracked() {
                kb_key(&key_server, &e.key);
            }
        })
        .flex()
        .flex_col()
        .p(14.0_f32)
        .select_none()
        .on_resized(move |w, h| {
            region.set(if h > 28.5 {
                InputRegion::Rects(vec![RectI::new(0, 0, w.ceil() as i32, h.ceil() as i32)])
            } else {
                InputRegion::Empty
            })
        })
        .child(
            for_each(
                move || server.popups().into_iter().take(MAX_POPUPS).collect(),
                |n| n.id,
                |n| card(n, true).mb(10.0),
            )
            .appear()
            .enter(|s| s.opacity(0.0_f32).translate_x(WIDTH * 0.6).scale(0.9_f32))
            .exit(|s| s.opacity(0.0_f32).translate_x(WIDTH * 0.6).scale(0.9_f32))
            .moves(emphasized(380))
            .transition(overshoot(480)),
        )
}

/// The icon of a notification: its picture, its app's icon, or a bell.
fn icon_of(n: &Notification) -> Div {
    // `image-path` may be a file, a `file://` URL, or the name of an icon.
    let named = |name: &str| -> Option<std::path::PathBuf> {
        let name = name.strip_prefix("file://").unwrap_or(name);
        if name.is_empty() {
            None
        } else if name.starts_with('/') {
            Some(name.into())
        } else {
            icon_file(name, 64)
        }
    };
    let file = n
        .image_path
        .as_deref()
        .and_then(named)
        .or_else(|| named(&n.app_icon))
        .or_else(|| {
            n.desktop_entry
                .as_deref()
                .and_then(app_by_id)
                .and_then(|a| app_icon(&a, 64))
        });
    let frame = div().size(36.0).flex_none().rounded(12.0_f32).overflow_hidden();
    if let Some(picture) = n.image.clone() {
        frame.child(
            bitmap_view(move || Some(picture.clone()))
                .size_full()
                .object_fit(ObjectFit::Cover),
        )
    } else if let Some(path) = file {
        frame.child(img(path).size_full().object_fit(ObjectFit::Contain))
    } else {
        frame
            .flex()
            .items_center()
            .justify_center()
            .dyn_style(|s| s.bg(colors().secondary_container))
            .child(sym(icons::NOTIFICATIONS, 18.0, |c| {
                c.on_secondary_container
            }))
    }
}

/// A notification as a card: its icon, who sent it, what it says, its actions and a close button.
pub fn card(n: Notification, popup: bool) -> Div {
    let urgent = n.urgency == Urgency::Critical;
    let when = n.received();
    let app = n.app_name.clone();
    let gone = n.clone();
    let default = n.clone();
    let mut actions = div().flex().flex_row().gap(8.0_f32).pt(4.0_f32);
    let mut any_action = false;
    let id = n.id;
    let selected = memo(move || popup && kb().0.get() && kb().1.get() == id);
    let mut number = 0;
    for deed in deeds(&n) {
        any_action = true;
        number += 1;
        let numbered = format!("{number}  {}", deed.label());
        let (plain, (n1, d1), (n2, d2)) = (deed.label().to_owned(), (n.clone(), deed.clone()), (n.clone(), deed));
        actions = actions
            .child(show(
                move || selected.get(),
                move || {
                    let (n, d) = (n1.clone(), d1.clone());
                    styled_button(numbered.clone(), ButtonStyle::tonal().size(ButtonSize::ExtraSmall))
                        .on_click(move |_| press(&n, &d))
                },
            ))
            .child(show(
                move || !selected.get(),
                move || {
                    let (n, d) = (n2.clone(), d2.clone());
                    styled_button(plain.clone(), ButtonStyle::tonal().size(ButtonSize::ExtraSmall))
                        .on_click(move |_| press(&n, &d))
                },
            ));
    }
    let hovered = signal(false);
    let mut body = div()
        .w(WIDTH)
        .flex()
        .flex_col()
        .gap(6.0_f32)
        .p(14.0_f32)
        .rounded(20.0_f32)
        .overflow_hidden()
        .shadows(if popup { elevation(3) } else { Vec::new() })
        .on_hover(move |h| hovered.set(h))
        .on_click(move |_| activate(&default))
        .dyn_style(move |s| {
            let c = colors();
            if selected.get() {
                s.bg(c.primary_container).text_color(c.on_primary_container)
            } else if urgent {
                s.bg(c.error_container).text_color(c.on_error_container)
            } else if popup {
                s.bg(c.surface_container_high).text_color(c.on_surface)
            } else {
                s.bg(c.surface_container_highest).text_color(c.on_surface)
            }
        })
        .child(
            div()
                .flex()
                .flex_row()
                .gap(12.0_f32)
                .child(icon_of(&n))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(0.0)
                        .gap(2.0_f32)
                        .child(
                            txt(TextStyle::LABEL_SMALL, move || {
                                let ago = when.elapsed().as_secs();
                                let ago = match ago {
                                    0..60 => "now".to_owned(),
                                    60..3600 => format!("{} min", ago / 60),
                                    _ => format!("{} h", ago / 3600),
                                };
                                local_time(Every::Minute);
                                if app.is_empty() {
                                    ago
                                } else {
                                    format!("{app} · {ago}")
                                }
                            })
                            .dyn_style(move |s| s.opacity(0.75_f32)),
                        )
                        .child(
                            static_txt(TextStyle::TITLE_SMALL, n.summary.clone()).overflow_hidden(),
                        )
                        .child(
                            TextStyle::BODY_SMALL
                                .apply(div())
                                .max_h(48.0)
                                .overflow_hidden()
                                .dyn_style(move |s| s.opacity(0.85_f32))
                                .child(n.body_text()),
                        ),
                )
                .child(
                    icon_btn(icons::CLOSE, 28.0, |c| c.on_surface_variant)
                        .on_click(move |e| {
                            e.stop_propagation();
                            gone.dismiss();
                        })
                        .dyn_style(move |s| {
                            s.opacity(if hovered.get() || !popup { 1.0 } else { 0.0_f32 })
                        })
                        .transition_of([Prop::Opacity], Transition::ms(150).ease_out()),
                ),
        );
    if any_action {
        body = body.child(actions);
    }
    pressable(body, |c| c.on_surface)
}

/// The notifications that are kept, for quick settings, with a button that clears them; with none,
/// it says so. They come in as the `order`th part of the panel.
pub fn history(order: usize) -> Div {
    let server = server();
    let any = {
        let server = server.clone();
        memo(move || {
            server
                .as_ref()
                .is_some_and(|s| !s.notifications().is_empty())
        })
    };
    let clear = server.clone();
    let list = server.clone();
    let elsewhere = server.is_none();
    settle_in(order, div())
        .flex()
        .flex_col()
        .gap(8.0_f32)
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(8.0_f32)
                .h(28.0)
                .px(6.0_f32)
                .child(sym(icons::NOTIFICATIONS, 18.0, |c| c.primary))
                .child(static_txt(TextStyle::TITLE_SMALL, "Notifications").flex_1())
                .child(
                    show(
                        move || any.get(),
                        move || {
                            let clear = clear.clone();
                            styled_button(
                                "Clear all",
                                ButtonStyle::text().size(ButtonSize::ExtraSmall),
                            )
                            .on_click(move |_| {
                                if let Some(s) = &clear {
                                    s.clear();
                                }
                            })
                        },
                    )
                    .enter(|s| s.opacity(0.0_f32).scale(0.8_f32))
                    .exit(|s| s.opacity(0.0_f32).scale(0.8_f32))
                    .transition(overshoot(320)),
                ),
        )
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                // About three cards, with a little of the next one showing that there is more to
                // scroll to.
                .max_h(268.0)
                .overflow_y_scroll()
                .child(
                    for_each(
                        move || list.as_ref().map(|s| s.notifications()).unwrap_or_default(),
                        |n| n.id,
                        |n| card(n, false).w_full().mb(8.0),
                    )
                    .enter(|s| s.opacity(0.0_f32).scale(0.9_f32))
                    .exit(|s| s.opacity(0.0_f32).translate_x(120.0_f32))
                    .moves(emphasized(320))
                    .transition(emphasized(300)),
                ),
        )
        .child(
            show(
                move || !any.get(),
                move || {
                    let (symbol, said) = if elsewhere {
                        (
                            icons::NOTIFICATIONS_PAUSED,
                            "Another app shows notifications",
                        )
                    } else {
                        (icons::NOTIFICATIONS_OFF, "No notifications")
                    };
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(8.0_f32)
                        .h(64.0)
                        .rounded(22.0_f32)
                        .dyn_style(|s| s.bg(colors().surface_container_high))
                        .child(sym(symbol, 20.0, |c| c.outline))
                        .child(
                            static_txt(TextStyle::BODY_MEDIUM, said)
                                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                        )
                },
            )
            .enter(|s| s.opacity(0.0_f32).scale(0.95_f32))
            .transition(emphasized(300)),
        )
}
