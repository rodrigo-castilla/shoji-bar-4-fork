//! The pages quick settings opens from its tiles in place of what it shows first: Wi-Fi,
//! Bluetooth and sound. Each has a header with the way back.

use crate::appinfo::icon_file;
use crate::quick_settings::{Page, go, section, slider_style};
use crate::theme::{TextStyle, colors};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui::shell::{AccessPoint, BluetoothDevice, NetError, Security};
use anti_webui_md3e::components::{
    ButtonStyle, TextFieldStyle, indeterminate_circular_progress, indeterminate_linear_progress,
    sized_icon, styled_button, styled_slider, switch, text_field,
};
use anti_webui_md3e::icons::{self, Symbol};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// How tall a list of a page gets before it scrolls.
const LIST: f32 = 320.0;

/// The top of a page: the way back, its name, and what goes at the end, like its switch.
pub fn header(title: &'static str, end: Div) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(6.0_f32)
        .child(icon_btn(icons::ARROW_BACK, 38.0, |c| c.on_surface).on_click(|_| go(Page::Main)))
        .child(static_txt(TextStyle::TITLE_LARGE, title).flex_1())
        .child(end)
}

/// A thin line that runs while `busy`, under a header.
fn busy_line(busy: impl Fn() -> bool + 'static) -> Div {
    div().h(4.0).flex().flex_col().child(
        show(busy, || indeterminate_linear_progress().w_full())
            .enter(|s| s.opacity(0.0_f32))
            .exit(|s| s.opacity(0.0_f32))
            .transition(Transition::ms(200).ease_out()),
    )
}

/// What a list is of, above it.
pub fn subheading(text: &'static str) -> Div {
    static_txt(TextStyle::LABEL_LARGE, text)
        .px(8.0_f32)
        .dyn_style(|s| s.text_color(colors().primary))
}

/// A small turning circle, for what is under way.
fn spinner() -> Div {
    div()
        .size(24.0)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .child(indeterminate_circular_progress().flex_none().scale(0.55_f32))
}

/// A line of a list on a page: `icon` in a round of its own, a name, how it is, and `end`. While
/// `on`, the round takes the primary colors, for what is in use.
fn item(
    icon: impl Fn() -> &'static Symbol + 'static,
    name: impl Fn() -> String + 'static,
    status: impl Fn() -> String + 'static,
    on: impl Fn() -> bool + Copy + 'static,
    end: Div,
) -> Div {
    pressable(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(12.0_f32)
            .min_h(52.0)
            .px(8.0_f32)
            .py(6.0_f32)
            .rounded(18.0_f32)
            .child(
                div()
                    .size(36.0)
                    .rounded(18.0_f32)
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_none()
                    .transition_of(
                        [Prop::Background, Prop::TextColor],
                        Transition::ms(180).ease_out(),
                    )
                    .dyn_style(move |s| {
                        let c = colors();
                        if on() {
                            s.bg(c.primary).text_color(c.on_primary)
                        } else {
                            s.bg(c.secondary_container)
                                .text_color(c.on_secondary_container)
                        }
                    })
                    .child(dyn_view(move || sized_icon(icon(), 18.0))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(0.0)
                    .overflow_hidden()
                    .child(txt(TextStyle::TITLE_SMALL, name).overflow_hidden())
                    .child(
                        txt(TextStyle::BODY_SMALL, status)
                            .overflow_hidden()
                            .dyn_style(|s| s.text_color(colors().on_surface_variant)),
                    ),
            )
            .child(end),
        |c| c.on_surface,
    )
}

/// A list that scrolls once it is taller than [`LIST`].
fn scrolling(list: impl IntoView) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(2.0_f32)
        .max_h(LIST)
        .overflow_y_scroll()
        .child(list)
}

/// A page's word for when there is nothing to show, with an icon above it.
fn nothing(symbol: &'static Symbol, text: impl Fn() -> String + 'static) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(8.0_f32)
        .py(18.0_f32)
        .child(sym(symbol, 32.0, |c| c.on_surface_variant))
        .child(
            txt(TextStyle::BODY_MEDIUM, text)
                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
        )
}

// Wi-Fi.

/// What the Wi-Fi page keeps while it is open.
#[derive(Clone, Copy)]
struct Wifi {
    net: Network,
    /// The network whose password is being asked for.
    asking: Signal<Option<String>>,
    password: Signal<String>,
    /// The network being joined.
    joining: Signal<Option<String>>,
    /// The last network that couldn't be joined, and why.
    failed: Signal<Option<(String, String)>>,
    scanning: Signal<bool>,
}

impl Wifi {
    fn scan(self) {
        if untrack(|| self.scanning.get()) {
            return;
        }
        self.scanning.set(true);
        drop(self.net.scan());
        // NetworkManager takes a few seconds, and what it finds comes in as it does.
        let scanning = self.scanning;
        set_timeout(Duration::from_secs(4), move || {
            if scanning.is_alive() {
                scanning.set(false);
            }
        });
    }

    /// Joins `ap`, saying how it went. If it can't, it goes back to the network it was on, which
    /// NetworkManager has left by then and doesn't always go back to by itself.
    fn join(self, ap: AccessPoint, password: Option<String>) {
        let ssid = ap.ssid.clone();
        let before = untrack(|| self.net.wifi()).filter(|a| a.ssid != ssid);
        self.joining.set(Some(ssid.clone()));
        self.failed.set(None);
        let net = self.net;
        self.net.connect(&ap, password).then(move |done| {
            if done.is_err()
                && let Some(before) = before
            {
                drop(net.connect(&before, None));
            }
            if !self.joining.is_alive() {
                return;
            }
            self.joining.set(None);
            match done {
                Ok(()) => {
                    self.asking.set(None);
                    self.password.set(String::new());
                }
                Err(NetError::NeedsSecret) => {
                    self.asking.set(Some(ssid.clone()));
                    self.failed
                        .set(Some((ssid, "It needs its password".into())));
                }
                Err(e) => self.failed.set(Some((ssid, e.to_string()))),
            }
        });
    }

    /// What a click on a network does: joins it if it can without a password, or else asks for it.
    fn pick(self, ap: AccessPoint) {
        if ap.known || ap.security == Security::Open {
            self.join(ap, None);
        } else if untrack(|| self.asking.get()).as_ref() == Some(&ap.ssid) {
            self.asking.set(None);
        } else {
            self.password.set(String::new());
            self.failed.set(None);
            self.asking.set(Some(ap.ssid));
        }
    }

    fn submit(self, ssid: &str) {
        let ap = untrack(|| self.net.access_points())
            .into_iter()
            .find(|a| a.ssid == ssid);
        let password = untrack(|| self.password.get());
        if let Some(ap) = ap
            && !password.is_empty()
        {
            self.join(ap, Some(password));
        }
    }
}

fn band(frequency: u32) -> &'static str {
    match frequency {
        5925.. => "6 GHz",
        4900.. => "5 GHz",
        _ => "2.4 GHz",
    }
}

fn wifi_symbol(ap: &AccessPoint) -> &'static Symbol {
    if ap.security == Security::Open {
        return crate::bar::wifi_icon(ap.strength);
    }
    match ap.strength {
        75.. => icons::SIGNAL_WIFI_4_BAR_LOCK,
        50.. => icons::NETWORK_WIFI_3_BAR_LOCKED,
        25.. => icons::NETWORK_WIFI_2_BAR_LOCKED,
        _ => icons::NETWORK_WIFI_1_BAR_LOCKED,
    }
}

/// Wi-Fi: the switch, the network it is on, and the others around, which ask for their password
/// in place.
pub fn wifi_page() -> Div {
    let net = network();
    let w = Wifi {
        net,
        asking: signal(None),
        password: signal(String::new()),
        joining: signal(None),
        failed: signal(None),
        scanning: signal(false),
    };
    // Looks again as the page opens, and when Wi-Fi comes on.
    let on = memo(move || net.wifi_enabled());
    effect(move || {
        if on.get() {
            w.scan();
        }
    });
    let enabled = Binding::new(
        move || net.wifi_enabled(),
        move |v| drop(net.set_wifi_enabled(v)),
    );
    let others = memo(move || {
        net.access_points()
            .into_iter()
            .filter(|a| !a.active && !a.ssid.is_empty())
            .map(|a| a.ssid)
            .collect::<Vec<_>>()
    });
    div()
        .flex()
        .flex_col()
        .gap(10.0_f32)
        .child(header(
            "Wi-Fi",
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(4.0_f32)
                .child(show(
                    move || on.get(),
                    move || {
                        icon_btn(icons::REFRESH, 36.0, |c| c.on_surface_variant)
                            .on_click(move |_| w.scan())
                    },
                ))
                .child(switch(enabled).mr(4.0)),
        ))
        .child(busy_line(move || w.scanning.get() && on.get()))
        .child(show(
            move || net.wired().is_some_and(|w| w.connected),
            move || {
                section().child(item(
                    || icons::LAN,
                    || "Ethernet".into(),
                    move || match net.wired() {
                        Some(w) if w.speed > 0 => format!("Connected · {} Mb/s", w.speed),
                        _ => "Connected".into(),
                    },
                    || true,
                    div(),
                ))
            },
        ))
        .child(show(
            move || !on.get(),
            || section().child(nothing(icons::WIFI_OFF, || "Wi-Fi is off".into())),
        ))
        .child(show(
            move || on.get() && net.wifi().is_some(),
            move || {
                section().child(subheading("Connected")).child(item(
                    move || net.wifi().map_or(icons::WIFI, |a| wifi_symbol(&a)),
                    move || net.wifi().map(|a| a.ssid).unwrap_or_default(),
                    move || {
                        let reach = match net.connectivity() {
                            Connectivity::Full => "Connected",
                            Connectivity::Limited => "No internet",
                            Connectivity::Portal => "Sign in to the network",
                            _ => "Connected",
                        };
                        match net.wifi() {
                            Some(a) => format!("{reach} · {}", band(a.frequency)),
                            None => reach.into(),
                        }
                    },
                    || true,
                    icon_btn(icons::LINK_OFF, 36.0, |c| c.on_surface_variant)
                        .on_click(move |_| drop(net.disconnect())),
                ))
            },
        ))
        .child(show(
            move || on.get(),
            move || {
                section()
                    .child(subheading("Networks"))
                    .child(show(
                        move || others.with(Vec::is_empty),
                        move || {
                            nothing(icons::WIFI_FIND, move || {
                                if w.scanning.get() {
                                    "Looking for networks…".into()
                                } else {
                                    "No networks found".into()
                                }
                            })
                        },
                    ))
                    .child(show(
                        move || !others.with(Vec::is_empty),
                        move || {
                            scrolling(
                                for_each(
                                    move || others.get(),
                                    |s| s.clone(),
                                    move |ssid| network_row(w, ssid),
                                )
                                .enter(|s| s.opacity(0.0_f32).translate_y(-8.0_f32))
                                .exit(|s| s.opacity(0.0_f32))
                                .moves(emphasized(300))
                                .transition(emphasized(300)),
                            )
                        },
                    ))
            },
        ))
}

/// A network around, and the password it asks for when it is picked.
fn network_row(w: Wifi, ssid: String) -> Div {
    let ap = memo({
        let ssid = ssid.clone();
        move || w.net.access_points().into_iter().find(|a| a.ssid == ssid)
    });
    let mine = |s: Signal<Option<String>>, ssid: &String| {
        let ssid = ssid.clone();
        memo(move || s.with(|s| s.as_ref() == Some(&ssid)))
    };
    let busy = mine(w.joining, &ssid);
    let asking = mine(w.asking, &ssid);
    let error = memo({
        let ssid = ssid.clone();
        move || {
            w.failed.with(|f| {
                f.as_ref()
                    .filter(|(s, _)| *s == ssid)
                    .map(|(_, why)| why.clone())
            })
        }
    });
    let name = ssid.clone();
    div()
        .flex()
        .flex_col()
        .child(
            item(
                move || ap.with(|a| a.as_ref().map_or(icons::WIFI, wifi_symbol)),
                move || name.clone(),
                move || {
                    if busy.get() {
                        return "Connecting…".into();
                    }
                    if let Some(why) = error.get() {
                        return why;
                    }
                    ap.with(|a| match a {
                        Some(a) => {
                            let what = if a.known {
                                "Saved"
                            } else if a.security == Security::Open {
                                "Open"
                            } else {
                                "Secured"
                            };
                            format!("{what} · {}", band(a.frequency))
                        }
                        None => String::new(),
                    })
                },
                || false,
                div().child(show(move || busy.get(), spinner)),
            )
            .on_click(move |_| {
                if let Some(a) = untrack(|| ap.get()) {
                    w.pick(a);
                }
            }),
        )
        .child(
            show(
                move || asking.get(),
                move || {
                    let (field_ssid, button_ssid) = (ssid.clone(), ssid.clone());
                    div()
                        .flex()
                        .flex_col()
                        .gap(8.0_f32)
                        .px(8.0_f32)
                        .pt(4.0_f32)
                        .pb(8.0_f32)
                        .child(
                            text_field(
                                w.password,
                                TextFieldStyle::new()
                                    .label("Password")
                                    .password()
                                    .error_when(move || error.with(Option::is_some))
                                    .on_submit(move |_| w.submit(&field_ssid)),
                            )
                            .w_full(),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap(8.0_f32)
                                .child(
                                    styled_button("Cancel", ButtonStyle::text())
                                        .on_click(move |_| w.asking.set(None)),
                                )
                                .child(
                                    styled_button("Connect", ButtonStyle::filled())
                                        .on_click(move |_| w.submit(&button_ssid)),
                                ),
                        )
                },
            )
            .enter(|s| s.opacity(0.0_f32).translate_y(-8.0_f32))
            .exit(|s| s.opacity(0.0_f32))
            .transition(emphasized(250)),
        )
}

// Bluetooth.

/// What the Bluetooth page keeps while it is open.
#[derive(Clone, Copy)]
struct Bt {
    bt: Bluetooth,
    /// The device being connected to, paired or let go of, by address.
    busy: Signal<Option<String>>,
    /// The last device that didn't do what it was asked, and why.
    failed: Signal<Option<(String, String)>>,
}

impl Bt {
    fn device(self, address: &str) -> Option<BluetoothDevice> {
        untrack(|| self.bt.devices())
            .into_iter()
            .find(|d| d.address == address)
    }

    /// Runs `task` for the device at `address`, showing it as busy until it is done.
    fn run(
        self,
        address: String,
        task: Task<Result<(), ServiceError>>,
        then: impl FnOnce(Self) + 'static,
    ) {
        self.busy.set(Some(address.clone()));
        self.failed.set(None);
        task.then(move |done| {
            if !self.busy.is_alive() {
                return;
            }
            self.busy.set(None);
            match done {
                Ok(()) => then(self),
                Err(e) => self.failed.set(Some((address, e.to_string()))),
            }
        });
    }

    /// What a click on a device does: connects or disconnects one that is paired, and pairs with
    /// and connects to one that isn't.
    fn pick(self, address: String) {
        let Some(d) = self.device(&address) else {
            return;
        };
        if d.connected {
            self.run(address, d.disconnect(), |_| {});
        } else if d.paired {
            self.run(address, d.connect(), |_| {});
        } else {
            let again = address.clone();
            self.run(address, d.pair(), move |me| {
                if let Some(d) = me.device(&again) {
                    drop(d.set_trusted(true));
                    me.run(again, d.connect(), |_| {});
                }
            });
        }
    }

    fn forget(self, address: String) {
        if let Some(d) = self.device(&address) {
            self.run(address, d.forget(), |_| {});
        }
    }
}

fn device_symbol(icon: &str) -> &'static Symbol {
    match icon {
        "audio-headset" | "audio-headphones" => icons::HEADPHONES,
        "audio-card" | "audio-speakers" => icons::SPEAKER,
        "input-keyboard" => icons::KEYBOARD,
        "input-mouse" | "input-tablet" => icons::MOUSE,
        "input-gaming" => icons::SPORTS_ESPORTS,
        "phone" => icons::SMARTPHONE,
        "computer" => icons::LAPTOP_MAC,
        "video-display" => icons::TV,
        _ => icons::BLUETOOTH,
    }
}

/// Bluetooth: the switch, the devices it is paired with, and the others it finds while the page
/// is open.
pub fn bluetooth_page() -> Div {
    let bt = bluetooth();
    let b = Bt {
        bt,
        busy: signal(None),
        failed: signal(None),
    };
    let powered = memo(move || bt.adapter().is_some_and(|a| a.powered));
    // Looks for devices while the page is open. The cleanup runs once the page's reactive values
    // are gone, so whether it looked is kept apart from them.
    let looking = Rc::new(Cell::new(false));
    effect({
        let looking = looking.clone();
        move || {
            if powered.get() {
                looking.set(true);
                drop(bt.set_discovering(true));
            }
        }
    });
    on_cleanup(move || {
        if looking.get() {
            drop(bt.set_discovering(false));
        }
    });
    let enabled = Binding::new(move || powered.get(), move |v| drop(bt.set_powered(v)));
    let addresses = move |paired: bool| {
        memo(move || {
            bt.devices()
                .into_iter()
                // Devices with nothing to go by but their address.
                .filter(|d| d.paired == paired && d.name.replace('-', ":") != d.address)
                .map(|d| d.address)
                .collect::<Vec<_>>()
        })
    };
    let (paired, around) = (addresses(true), addresses(false));
    let list = move |which: Memo<Vec<String>>| {
        scrolling(
            for_each(
                move || which.get(),
                |a| a.clone(),
                move |address| device_row(b, address),
            )
            .enter(|s| s.opacity(0.0_f32).translate_y(-8.0_f32))
            .exit(|s| s.opacity(0.0_f32))
            .moves(emphasized(300))
            .transition(emphasized(300)),
        )
    };
    div()
        .flex()
        .flex_col()
        .gap(10.0_f32)
        .child(header("Bluetooth", switch(enabled).mr(4.0)))
        .child(busy_line(move || {
            powered.get() && bt.adapter().is_some_and(|a| a.discovering)
        }))
        .child(show(
            move || !powered.get(),
            || {
                section().child(nothing(icons::BLUETOOTH_DISABLED, || {
                    "Bluetooth is off".into()
                }))
            },
        ))
        .child(show(
            move || powered.get() && !paired.with(Vec::is_empty),
            move || section().child(subheading("Paired")).child(list(paired)),
        ))
        .child(show(
            move || powered.get(),
            move || {
                section()
                    .child(subheading("Available"))
                    .child(show(
                        move || around.with(Vec::is_empty),
                        || nothing(icons::BLUETOOTH_SEARCHING, || "Looking for devices…".into()),
                    ))
                    .child(show(
                        move || !around.with(Vec::is_empty),
                        move || list(around),
                    ))
            },
        ))
}

fn device_row(b: Bt, address: String) -> Div {
    let device = memo({
        let address = address.clone();
        move || b.bt.devices().into_iter().find(|d| d.address == address)
    });
    let busy = memo({
        let address = address.clone();
        move || b.busy.with(|a| a.as_ref() == Some(&address))
    });
    let error = memo({
        let address = address.clone();
        move || {
            b.failed.with(|f| {
                f.as_ref()
                    .filter(|(a, _)| *a == address)
                    .map(|(_, why)| why.clone())
            })
        }
    });
    let connected = move || device.with(|d| d.as_ref().is_some_and(|d| d.connected));
    let paired = move || device.with(|d| d.as_ref().is_some_and(|d| d.paired));
    let (picked, forgotten) = (address.clone(), address);
    item(
        move || {
            device.with(|d| {
                d.as_ref()
                    .map_or(icons::BLUETOOTH, |d| device_symbol(&d.icon))
            })
        },
        move || device.with(|d| d.as_ref().map(|d| d.name.clone()).unwrap_or_default()),
        move || {
            if busy.get() {
                return if connected() {
                    "Disconnecting…".into()
                } else if paired() {
                    "Connecting…".into()
                } else {
                    "Pairing…".into()
                };
            }
            if let Some(why) = error.get() {
                return why;
            }
            device.with(|d| match d {
                Some(d) if d.connected => match d.battery {
                    Some(level) => format!("Connected · {level}%"),
                    None => "Connected".into(),
                },
                Some(d) if d.paired => "Paired".into(),
                _ => "Click to pair".into(),
            })
        },
        connected,
        div()
            .flex()
            .flex_row()
            .items_center()
            .child(show(move || busy.get(), spinner))
            .child(show(
                move || paired() && !busy.get(),
                move || {
                    let address = forgotten.clone();
                    icon_btn(icons::DELETE, 34.0, |c| c.on_surface_variant).on_click(move |e| {
                        e.stop_propagation();
                        b.forget(address.clone());
                    })
                },
            )),
    )
    .on_click(move |_| {
        if !untrack(|| busy.get()) {
            b.pick(picked.clone());
        }
    })
}

// Sound.

fn output_symbol(description: &str) -> &'static Symbol {
    let d = description.to_lowercase();
    if d.contains("headphone") || d.contains("headset") {
        icons::HEADPHONES
    } else if d.contains("hdmi") || d.contains("displayport") {
        icons::TV
    } else {
        icons::SPEAKER
    }
}

/// Sound: how loud, where it goes and where it comes from, and how loud each program is.
pub fn sound_page() -> Div {
    let a = audio();
    let device_list = move |input: bool| {
        let ids = memo(move || {
            let all = if input { a.sources() } else { a.sinks() };
            all.into_iter()
                .filter(|d| !d.name.ends_with(".monitor"))
                .map(|d| d.id)
                .collect::<Vec<_>>()
        });
        for_each(
            move || ids.get(),
            |id| *id,
            move |id| {
                let device = memo(move || {
                    let all = if input { a.sources() } else { a.sinks() };
                    all.into_iter().find(|d| d.id == id)
                });
                let chosen = move || {
                    let now = if input {
                        a.default_source()
                    } else {
                        a.default_sink()
                    };
                    now.is_some_and(|d| d.id == id)
                };
                item(
                    move || {
                        if input {
                            icons::MIC
                        } else {
                            device.with(|d| {
                                d.as_ref()
                                    .map_or(icons::SPEAKER, |d| output_symbol(&d.description))
                            })
                        }
                    },
                    move || {
                        device.with(|d| {
                            d.as_ref()
                                .map(|d| d.description.clone())
                                .unwrap_or_default()
                        })
                    },
                    move || {
                        if chosen() {
                            "In use".into()
                        } else {
                            String::new()
                        }
                    },
                    chosen,
                    div().child(show(chosen, || {
                        sym(icons::CHECK, 20.0, |c| c.primary).mr(8.0)
                    })),
                )
                .on_click(move |_| {
                    if let Some(d) = untrack(|| device.get()) {
                        drop(a.set_default(&d));
                    }
                })
            },
        )
    };
    let level = move |input: bool| {
        let now = move || {
            if input {
                a.default_source()
            } else {
                a.default_sink()
            }
        };
        let value = Binding::new(
            move || now().map_or(0.0, |d| d.volume as f32),
            move |v| {
                if let Some(d) = untrack(now) {
                    drop(d.set_volume(v as f64));
                }
            },
        );
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(10.0_f32)
            .child(pressable(
                div()
                    .size(36.0)
                    .rounded(18.0_f32)
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_none()
                    .on_click(move |_| {
                        if let Some(d) = now() {
                            drop(d.set_muted(!d.muted));
                        }
                    })
                    .child(dyn_sym(
                        move || match (input, now().is_some_and(|d| d.muted)) {
                            (true, true) => icons::MIC_OFF,
                            (true, false) => icons::MIC,
                            (false, true) => icons::VOLUME_OFF,
                            (false, false) => icons::VOLUME_UP,
                        },
                        18.0,
                        |c| c.on_surface_variant,
                    )),
                |c| c.on_surface,
            ))
            .child(
                styled_slider(value, slider_style())
                    .flex_1()
                    .min_w(0.0)
                    .my(-8.0),
            )
    };
    let streams = memo(move || {
        a.streams()
            .into_iter()
            .filter(|s| !s.input)
            .map(|s| s.id)
            .collect::<Vec<_>>()
    });
    div()
        .flex()
        .flex_col()
        .gap(10.0_f32)
        .child(header("Sound", div()))
        .child(
            section()
                .child(subheading("Output"))
                .child(level(false))
                .child(scrolling(device_list(false))),
        )
        .child(
            section()
                .child(subheading("Input"))
                .child(level(true))
                .child(scrolling(device_list(true))),
        )
        .child(show(
            move || !streams.with(Vec::is_empty),
            move || {
                section()
                    .child(subheading("Apps"))
                    .child(scrolling(for_each(
                        move || streams.get(),
                        |id| *id,
                        move |id| stream_row(id),
                    )))
            },
        ))
}

/// A program that plays: its icon, its name and what it plays, and how loud it is.
fn stream_row(id: u32) -> Div {
    let a = audio();
    let stream = memo(move || a.streams().into_iter().find(|s| s.id == id));
    let icon = memo(move || {
        stream.with(|s| {
            let s = s.as_ref()?;
            [s.icon_name.as_str(), &s.app_name.to_lowercase()]
                .into_iter()
                .filter(|n| !n.is_empty())
                .find_map(|n| icon_file(n, 64))
        })
    });
    let value = Binding::new(
        move || stream.with(|s| s.as_ref().map_or(0.0, |s| s.volume as f32)),
        move |v| {
            if let Some(s) = untrack(|| stream.get()) {
                drop(s.set_volume(v as f64));
            }
        },
    );
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(12.0_f32)
        .px(8.0_f32)
        .py(4.0_f32)
        .child(dyn_view(move || match icon.get() {
            Some(path) => img(path).size(28.0).flex_none().into_view(),
            None => sym(icons::GRAPHIC_EQ, 22.0, |c| c.on_surface_variant)
                .size(28.0)
                .into_view(),
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(0.0)
                .child(
                    txt(TextStyle::TITLE_SMALL, move || {
                        stream.with(|s| {
                            s.as_ref()
                                .map(|s| match s.media_name.as_str() {
                                    "" => s.app_name.clone(),
                                    m => format!("{} · {m}", s.app_name),
                                })
                                .unwrap_or_default()
                        })
                    })
                    .overflow_hidden(),
                )
                .child(styled_slider(value, slider_style()).w_full().my(-10.0)),
        )
}
