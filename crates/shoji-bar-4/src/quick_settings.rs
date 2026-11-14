//! Quick settings: who is here and the power buttons, toggles as tiles, the volume and the
//! brightness, the battery, what is playing, and the notifications. Some tiles lead to a page of
//! their own (`qs_pages`), which takes the place of all this in the panel; the gear leads to the
//! settings of the shell (`qs_settings`).

use crate::bar::wifi_icon;
use crate::power_dialog::Ask;
use crate::settings::{ThemeMode, settings};
use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::BatteryState;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{SliderStyle, sized_icon, styled_slider};
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;

const WIDTH: f32 = 396.0;

/// What the panel shows: everything, or the page of one thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    Wifi,
    Bluetooth,
    Sound,
    Settings,
}

thread_local! {
    /// The page of the panel that is open, if one is.
    static PAGE: Cell<Option<Signal<Page>>> = const { Cell::new(None) };
    /// The page the panel opens at the next time.
    static START: Cell<Page> = const { Cell::new(Page::Main) };
}

/// Opens the panel at a page, on the output where the window with the keyboard is.
pub fn open_at(page: Page) {
    START.with(|s| s.set(page));
    if let Some(output) = crate::ipc::focused_output() {
        crate::panels::open(&output, crate::panels::Panel::QuickSettings);
    }
}

fn page_signal() -> Option<Signal<Page>> {
    PAGE.with(Cell::get).filter(|p| p.is_alive())
}

/// Shows a page of the panel that is open.
pub fn go(page: Page) {
    if let Some(p) = page_signal() {
        p.set(page);
    }
}

/// Goes back to the first page, for Escape; false when it is already there.
pub fn back() -> bool {
    match page_signal() {
        Some(p) if p.get_untracked() != Page::Main => {
            p.set(Page::Main);
            true
        }
        _ => false,
    }
}

/// The style of the panel's sliders.
pub fn slider_style() -> SliderStyle {
    SliderStyle::new().wavy().no_stops()
}

pub fn panel(output: Output) -> Div {
    let _ = output;
    let page = signal(START.with(|s| s.replace(Page::Main)));
    PAGE.with(|p| p.set(Some(page)));
    // The pages lie over each other, so the panel takes the height of the one shown, going to that
    // of another on a spring as it comes in.
    let height = animated(0.0f32);
    let sized = Cell::new(false);
    let fit = std::rc::Rc::new(move |which: Page, h: f32| {
        if untrack(|| page.get()) != which {
            return;
        }
        if sized.replace(true) {
            height.spring_to(h, fast_spatial());
        } else {
            height.set(h);
        }
    });
    let page_view = |which: Page, build: fn() -> Div, from: f32| {
        let fit = fit.clone();
        show(
            move || page.get() == which,
            move || {
                let fit = fit.clone();
                build()
                    .absolute()
                    .top(0.0)
                    .left(0.0)
                    .right(0.0)
                    .on_resized(move |_, h| fit(which, h))
            },
        )
        .enter(move |s| s.opacity(0.0_f32).translate_x(from))
        .exit(move |s| s.opacity(0.0_f32).translate_x(from))
        .transition(emphasized(380))
    };
    div()
        .w(WIDTH)
        .p(16.0_f32)
        .rounded(30.0_f32)
        // Pages going out to the side go out of sight at its edge.
        .overflow_hidden()
        .shadows(elevation(3))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container).text_color(c.on_surface)
        })
        .child(
            div()
                .relative()
                .dyn_style(move |s| s.h(height.get()))
                .child(page_view(Page::Main, main_page, -56.0))
                .child(page_view(Page::Wifi, crate::qs_pages::wifi_page, 56.0))
                .child(page_view(
                    Page::Bluetooth,
                    crate::qs_pages::bluetooth_page,
                    56.0,
                ))
                .child(page_view(Page::Sound, crate::qs_pages::sound_page, 56.0))
                .child(page_view(
                    Page::Settings,
                    crate::qs_settings::settings_page,
                    56.0,
                )),
        )
}

/// What the panel shows first.
fn main_page() -> Div {
    div()
        .flex()
        .flex_col()
        .gap(12.0_f32)
        .child(settle_in(0, header()))
        .child(tiles())
        .child(settle_in(4, sliders()))
        .child(show(
            || battery().available(),
            || settle_in(5, crate::battery_card::card()),
        ))
        .child(crate::notify::history(6))
}

/// A section of the panel on a card of its own.
pub fn section() -> Div {
    div()
        .flex()
        .flex_col()
        .gap(10.0_f32)
        .p(14.0_f32)
        .rounded(22.0_f32)
        .dyn_style(|s| s.bg(colors().surface_container_high))
}

fn greeting() -> &'static str {
    match local_time(Every::Minute).hour {
        5..=10 => "Good morning",
        11..=16 => "Good afternoon",
        17..=21 => "Good evening",
        _ => "Good night",
    }
}

/// The user, the battery, and the power buttons, which open into a row of what they can do.
fn header() -> Div {
    let user = crate::user::name();
    let power_open = signal(false);
    // The row of actions opens to its height with an easing, moving what is below it smoothly;
    // its height is measured once it is laid out.
    let opened = animated(0.0f32);
    effect(move || opened.to(if power_open.get() { 1.0 } else { 0.0 }, emphasized(350)));
    let row_height = signal(64.0f32);
    let b = battery();
    let name = user.clone();
    div()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(12.0_f32)
                .px(4.0_f32)
                .pt(2.0_f32)
                .child(
                    // To the settings, where it can be changed.
                    crate::user::avatar(38.0)
                        .cursor(CursorIcon::Pointer)
                        .on_click(|_| go(Page::Settings)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(0.0)
                        .child(txt(TextStyle::TITLE_SMALL, move || {
                            format!("{}, {name}", greeting())
                        }))
                        .child(
                            txt(TextStyle::BODY_SMALL, move || {
                                let t = local_time(Every::Minute);
                                let mut s = t.format("%A, %-d %B");
                                if b.available() {
                                    s.push_str(&format!(" · {:.0}%", b.percentage()));
                                    if b.state() == BatteryState::Charging {
                                        s.push_str(" charging");
                                    }
                                }
                                s
                            })
                            .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                        ),
                )
                .child(
                    icon_btn(icons::SETTINGS, 34.0, |c| c.on_surface_variant)
                        .on_click(|_| go(Page::Settings)),
                )
                .child(power_button(power_open)),
        )
        .child(
            div()
                .overflow_hidden()
                .flex_none()
                .dyn_style(move |s| s.h(opened.get() * row_height.get()))
                .child(
                    show(
                        move || power_open.get(),
                        move || {
                            let s = session();
                            let actions: [(&'static Symbol, &str, Power); 5] = [
                                (icons::LOCK, "Lock", Power::Lock),
                                (icons::BEDTIME, "Sleep", Power::Sleep),
                                (icons::LOGOUT, "Log out", Power::Ask(Ask::LogOut)),
                                (icons::RESTART_ALT, "Restart", Power::Ask(Ask::Restart)),
                                (
                                    icons::POWER_SETTINGS_NEW,
                                    "Shut down",
                                    Power::Ask(Ask::PowerOff),
                                ),
                            ];
                            let mut row = div()
                                .flex()
                                .flex_row()
                                .gap(6.0_f32)
                                .pt(10.0_f32)
                                .on_resized(move |_, h| row_height.set(h));
                            for (i, (symbol, label, action)) in actions.into_iter().enumerate() {
                                let appear = animated(0.0f32);
                                set_timeout(
                                    std::time::Duration::from_millis(40 * i as u64),
                                    move || appear.spring_to(1.0, fast_spatial()),
                                );
                                row = row.child(pressable(
                                    div()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .gap(3.0_f32)
                                        .py(8.0_f32)
                                        .rounded(16.0_f32)
                                        .dyn_style(move |st| {
                                            let c = colors();
                                            let a = appear.get();
                                            let danger = matches!(action, Power::Ask(_));
                                            st.bg(if danger {
                                                c.error_container
                                            } else {
                                                c.secondary_container
                                            })
                                            .text_color(if danger {
                                                c.on_error_container
                                            } else {
                                                c.on_secondary_container
                                            })
                                            .opacity(a.clamp(0.0, 1.0))
                                            .scale(0.6 + 0.4 * a)
                                        })
                                        .on_click(move |_| match action {
                                            Power::Lock => {
                                                crate::panels::close();
                                                crate::lock::lock();
                                            }
                                            Power::Sleep => {
                                                crate::panels::close();
                                                drop(s.suspend());
                                            }
                                            // Asked about first, in a dialog in place of this
                                            // panel.
                                            Power::Ask(what) => crate::power_dialog::ask(what),
                                        })
                                        .child(sized_icon(symbol, 18.0))
                                        .child(static_txt(TextStyle::LABEL_MEDIUM, label)),
                                    |c| c.on_secondary_container,
                                ));
                            }
                            row
                        },
                    )
                    .enter(|s| s.opacity(0.0_f32))
                    .exit(|s| s.opacity(0.0_f32))
                    .transition(emphasized(300)),
                ),
        )
}

#[derive(Clone, Copy)]
enum Power {
    Lock,
    Sleep,
    /// Log out, restart or shut down, which ask first.
    Ask(Ask),
}

/// The power button: it turns into a close button while the actions are shown.
fn power_button(open: Signal<bool>) -> Div {
    let turn = follow(move || if open.get() { 1.0 } else { 0.0 }, fast_spatial());
    pressable(
        div()
            .size(34.0)
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .on_click(move |_| open.update(|o| *o = !*o))
            .dyn_style(move |s| {
                let c = colors();
                let t = turn.get();
                s.rounded(17.0 - 7.0 * t)
                    .bg(c.error_container.with_alpha(t.clamp(0.0, 1.0)))
                    .rotate(90.0 * t)
            })
            .child(dyn_sym(
                move || {
                    if open.get() {
                        icons::CLOSE
                    } else {
                        icons::POWER_SETTINGS_NEW
                    }
                },
                18.0,
                |c| c.on_surface_variant,
            )),
        |c| c.on_surface,
    )
}

/// A toggle as a tile: it grows rounder and takes the primary color when on.
fn tile(
    icon: impl Fn() -> &'static Symbol + 'static,
    title: &'static str,
    subtitle: impl Fn() -> String + 'static,
    on: impl Fn() -> bool + Copy + 'static,
    available: impl Fn() -> bool + Copy + 'static,
    click: impl Fn() + 'static,
    details: Option<Page>,
) -> Div {
    let round = follow(move || if on() { 1.0 } else { 0.0 }, fast_spatial());
    let pressed = signal(false);
    let squash = follow(
        move || if pressed.get() { 0.95 } else { 1.0 },
        fast_spatial(),
    );
    let content =
        move |c: &anti_webui_md3e::ColorScheme| if on() { c.on_primary } else { c.on_surface };
    let click = std::rc::Rc::new(click);
    let bubble = div()
        .size(36.0)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .transition_of([Prop::Background], Transition::ms(180).ease_out())
        .dyn_style(move |s| {
            let c = colors();
            let r = round.get();
            s.rounded(11.0 + 7.0 * r).bg(if on() {
                c.on_primary.with_alpha(0.16)
            } else {
                c.secondary_container
            })
        })
        .child(
            div()
                .transition(Transition::ms(200).ease_out())
                .dyn_style(move |s| s.font_axis(b"FILL", if on() { 1.0 } else { 0.0 }))
                .child(dyn_view(move || sized_icon(icon(), 18.0))),
        );
    // With a page of its own, the tile opens it, and its icon is what turns it on and off.
    let bubble = if details.is_some() {
        let click = click.clone();
        pressable(
            bubble.on_click(move |e| {
                e.stop_propagation();
                if available() {
                    click();
                }
            }),
            content,
        )
    } else {
        bubble
    };
    pressable(
        div()
            .flex_1()
            .min_w(0.0)
            .h(56.0)
            .flex()
            .flex_row()
            .items_center()
            .gap(10.0_f32)
            .px(10.0_f32)
            .on_active(move |a| pressed.set(a))
            .on_click({
                let click = click.clone();
                move |_| match details {
                    Some(page) => go(page),
                    None if available() => click(),
                    None => {}
                }
            })
            .transition_of(
                [Prop::Background, Prop::TextColor],
                Transition::ms(180).ease_out(),
            )
            .dyn_style(move |s| {
                let c = colors();
                let r = round.get();
                s.rounded(16.0 + 12.0 * r)
                    .scale(squash.get())
                    .bg(if on() {
                        c.primary
                    } else {
                        c.surface_container_highest
                    })
                    .text_color(content(&c))
                    .opacity(if available() { 1.0 } else { 0.45_f32 })
            })
            .child(bubble)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(0.0)
                    .overflow_hidden()
                    .child(static_txt(TextStyle::LABEL_LARGE, title))
                    .child(
                        txt(TextStyle::LABEL_SMALL, subtitle)
                            .overflow_hidden()
                            .dyn_style(move |s| s.opacity(0.8_f32)),
                    ),
            )
            .children(details.map(|_| {
                sized_icon(icons::CHEVRON_RIGHT, 18.0)
                    .flex_none()
                    .dyn_style(|s| s.opacity(0.7_f32))
            })),
        content,
    )
}

fn tiles() -> Div {
    let net = network();
    let bt = bluetooth();
    let a = audio();
    let p = power_profiles();
    let server = crate::notify::server();
    let has_server = server.is_some();
    let dnd = memo({
        let server = server.clone();
        move || server.as_ref().is_some_and(|s| s.do_not_disturb())
    });
    let profile_label = |name: &str| match name {
        "power-saver" => "Power saver".to_owned(),
        "performance" => "Performance".to_owned(),
        "balanced" => "Balanced".to_owned(),
        other => other.to_owned(),
    };
    let row = || div().flex().flex_row().gap(8.0_f32);
    let dark = move || crate::theme::is_dark();
    div()
        .flex()
        .flex_col()
        .gap(8.0_f32)
        .child(
            settle_in(1, row())
                .child(tile(
                    move || match net.wifi() {
                        Some(ap) => wifi_icon(ap.strength),
                        None if net.wired().is_some_and(|w| w.connected) => icons::LAN,
                        None => icons::WIFI_OFF,
                    },
                    "Internet",
                    move || match net.wifi() {
                        Some(ap) => ap.ssid,
                        None if net.wired().is_some_and(|w| w.connected) => "Ethernet".into(),
                        None if net.wifi_enabled() => "Not connected".into(),
                        None => "Wi-Fi off".into(),
                    },
                    move || net.wifi_enabled(),
                    move || net.available(),
                    move || drop(net.set_wifi_enabled(!net.wifi_enabled())),
                    Some(Page::Wifi),
                ))
                .child(tile(
                    move || {
                        if bt.devices().iter().any(|d| d.connected) {
                            icons::BLUETOOTH_CONNECTED
                        } else if bt.adapter().is_some_and(|a| a.powered) {
                            icons::BLUETOOTH
                        } else {
                            icons::BLUETOOTH_DISABLED
                        }
                    },
                    "Bluetooth",
                    move || {
                        let connected: Vec<String> = bt
                            .devices()
                            .into_iter()
                            .filter(|d| d.connected)
                            .map(|d| d.name)
                            .collect();
                        match connected.len() {
                            0 if bt.adapter().is_some_and(|a| a.powered) => "On".into(),
                            0 => "Off".into(),
                            1 => connected[0].clone(),
                            n => format!("{n} devices"),
                        }
                    },
                    move || bt.adapter().is_some_and(|a| a.powered),
                    move || bt.available(),
                    move || {
                        let on = bt.adapter().is_some_and(|a| a.powered);
                        drop(bt.set_powered(!on));
                    },
                    Some(Page::Bluetooth),
                )),
        )
        .child(
            settle_in(2, row())
                .child(tile(
                    move || {
                        if dark() {
                            icons::DARK_MODE
                        } else {
                            icons::LIGHT_MODE
                        }
                    },
                    "Dark theme",
                    move || match settings().with(|s| s.mode) {
                        ThemeMode::Auto => "By time of day".into(),
                        _ if dark() => "On".into(),
                        _ => "Off".into(),
                    },
                    dark,
                    || true,
                    move || {
                        settings().update(|s| {
                            s.mode = if dark() {
                                ThemeMode::Light
                            } else {
                                ThemeMode::Dark
                            }
                        })
                    },
                    None,
                ))
                .child(tile(
                    move || {
                        if dnd.get() {
                            icons::NOTIFICATIONS_OFF
                        } else {
                            icons::NOTIFICATIONS
                        }
                    },
                    "Do not disturb",
                    move || if dnd.get() { "On".into() } else { "Off".into() },
                    move || dnd.get(),
                    move || has_server,
                    move || {
                        if let Some(s) = &server {
                            s.set_do_not_disturb(!s.do_not_disturb());
                        }
                    },
                    None,
                )),
        )
        .child(
            settle_in(3, row())
                .child(tile(
                    move || match p.active().as_str() {
                        "power-saver" => icons::ENERGY_SAVINGS_LEAF,
                        "performance" => icons::SPEED,
                        _ => icons::BOLT,
                    },
                    "Power mode",
                    move || profile_label(&p.active()),
                    move || p.active() != "balanced",
                    move || p.available(),
                    move || {
                        let all = p.profiles();
                        let at = all.iter().position(|x| *x == p.active()).unwrap_or(0);
                        if let Some(next) = all.get((at + 1) % all.len().max(1)) {
                            drop(p.set_active(next));
                        }
                    },
                    None,
                ))
                .child(tile(
                    move || {
                        if a.default_source().is_some_and(|s| s.muted) {
                            icons::MIC_OFF
                        } else {
                            icons::MIC
                        }
                    },
                    "Microphone",
                    move || match a.default_source() {
                        Some(s) if s.muted => "Muted".into(),
                        Some(s) => s.description,
                        None => "None".into(),
                    },
                    move || a.default_source().is_some_and(|s| !s.muted),
                    move || a.default_source().is_some(),
                    move || {
                        if let Some(s) = a.default_source() {
                            drop(s.set_muted(!s.muted));
                        }
                    },
                    Some(Page::Sound),
                )),
        )
}

/// The volume and the brightness.
fn sliders() -> Div {
    let a = audio();
    let s = session();
    let style = slider_style;
    let row = |icon: Div, slider: Div| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(10.0_f32)
            // The slider takes the pointer 48 high, but needs no more room than its handle.
            .child(icon)
            .child(slider.flex_1().min_w(0.0).my(-8.0))
    };
    let volume = Binding::new(
        move || a.default_sink().map_or(0.0, |s| s.volume as f32),
        move |v| {
            if let Some(sink) = untrack(|| a.default_sink()) {
                drop(sink.set_volume(v as f64));
            }
        },
    );
    let brightness = Binding::new(
        move || s.brightness().unwrap_or(0.0) as f32,
        move |v| drop(s.set_brightness(v.max(0.02) as f64)),
    );
    section()
        .py(8.0_f32)
        .gap(2.0_f32)
        .child(
            row(
                pressable(
                    div()
                        .size(34.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_none()
                        .rounded(17.0_f32)
                        .on_click(move |_| {
                            if let Some(sink) = a.default_sink() {
                                drop(sink.set_muted(!sink.muted));
                            }
                        })
                        .child(dyn_sym(
                            move || match a.default_sink() {
                                Some(s) if s.muted || s.volume <= 0.001 => icons::VOLUME_OFF,
                                Some(s) if s.volume < 0.5 => icons::VOLUME_DOWN,
                                _ => icons::VOLUME_UP,
                            },
                            18.0,
                            |c| c.on_surface_variant,
                        )),
                    |c| c.on_surface,
                ),
                styled_slider(volume, style()),
            )
            .child(
                icon_btn(icons::CHEVRON_RIGHT, 32.0, |c| c.on_surface_variant)
                    .on_click(|_| go(Page::Sound)),
            ),
        )
        .child(show(
            move || s.brightness().is_some(),
            move || {
                row(
                    div()
                        .size(34.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_none()
                        .child(sym(icons::BRIGHTNESS_6, 18.0, |c| c.on_surface_variant)),
                    styled_slider(brightness, style()),
                )
                // As wide as the way to the sound page above it, for the sliders to line up.
                .child(div().size(32.0).flex_none())
            },
        ))
}
