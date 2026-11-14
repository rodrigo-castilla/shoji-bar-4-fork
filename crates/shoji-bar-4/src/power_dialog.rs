//! Asking before logging out, restarting or shutting down: a dialog in the middle of the output, over a veil
//! like the launcher's. Its icon sits on a shape that spins in as it opens and turns into a sun
//! while the pointer is on the button that does it. Escape, Cancel and a click outside it keep
//! everything as it is.

use crate::panels::Panel;
use crate::theme::{TextStyle, colors, elevation};
use crate::ui::*;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use anti_webui_md3e::components::{MaterialShape, ShapeStyle, styled_shape, text_button};
use anti_webui_md3e::icons::{self, Symbol};
use anti_webui_md3e::motion::fast_spatial;
use std::cell::Cell;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Ends the session: quits ShojiWM, as Super+Shift+Q does.
    LogOut,
    Restart,
    PowerOff,
}

impl Ask {
    fn icon(self) -> &'static Symbol {
        match self {
            Ask::LogOut => icons::LOGOUT,
            Ask::Restart => icons::RESTART_ALT,
            Ask::PowerOff => icons::POWER_SETTINGS_NEW,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Ask::LogOut => "Log out?",
            Ask::Restart => "Restart?",
            Ask::PowerOff => "Shut down?",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Ask::LogOut => "Open apps will be closed and the session will end.",
            Ask::Restart => "Open apps will be closed and the computer will start again.",
            Ask::PowerOff => "Open apps will be closed and the computer will turn off.",
        }
    }

    fn action(self) -> &'static str {
        match self {
            Ask::LogOut => "Log out",
            Ask::Restart => "Restart",
            Ask::PowerOff => "Shut down",
        }
    }
}

thread_local! {
    /// What the dialog asks about; set before it opens.
    static ASKING: Cell<Ask> = const { Cell::new(Ask::PowerOff) };
}

/// Opens the dialog on the output that has a panel open, or else the focused one, or else the
/// first.
pub fn ask(what: Ask) {
    ASKING.with(|a| a.set(what));
    let output = crate::panels::current()
        .map(|(output, _)| output)
        .or_else(crate::ipc::focused_output)
        .or_else(|| outputs().into_iter().next().map(|o| o.name));
    if let Some(output) = output {
        crate::panels::open(&output, Panel::Power);
    }
}

const WIDTH: f32 = 340.0;

/// The dialog itself.
pub fn panel() -> Div {
    let what = ASKING.with(Cell::get);
    let on_action = signal(false);
    // Spins in from a quarter turn back, growing, on a spring that goes a little past.
    let spin = animated(0.0f32);
    spin.spring_to(1.0, fast_spatial());
    div()
        .w(WIDTH)
        .p(24.0_f32)
        .rounded(28.0_f32)
        .flex()
        .flex_col()
        .items_center()
        .gap(16.0_f32)
        .shadows(elevation(3))
        .dyn_style(|s| {
            let c = colors();
            s.bg(c.surface_container_high).text_color(c.on_surface)
        })
        // Clicks on it are not clicks outside it.
        .on_click(|e| e.stop_propagation())
        .child(
            div()
                .size(72.0)
                .relative()
                .flex_none()
                .dyn_style(move |s| s.scale(0.4 + 0.6 * spin.get()))
                .child(
                    styled_shape(
                        move || {
                            if on_action.get() {
                                MaterialShape::Sunny
                            } else {
                                MaterialShape::Cookie9Sided
                            }
                        },
                        ShapeStyle::new()
                            .color(|c| c.error_container)
                            .rotation(move || {
                                -90.0 * (1.0 - spin.get())
                                    + if on_action.get() { 45.0 } else { 0.0 }
                            }),
                    )
                    .absolute()
                    .inset(0.0)
                    .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .inset(0.0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(sym(what.icon(), 32.0, |c| c.on_error_container)),
                ),
        )
        .child(static_txt(TextStyle::HEADLINE_SMALL, what.title()))
        .child(
            // Wrapping, unlike `static_txt`.
            TextStyle::BODY_MEDIUM
                .apply(div().child(what.text()))
                .w_full()
                .text_align(TextAlign::Center)
                .dyn_style(|s| s.text_color(colors().on_surface_variant)),
        )
        .child(
            div()
                .w_full()
                .pt(8.0_f32)
                .flex()
                .flex_row()
                .justify_end()
                .items_center()
                .gap(8.0_f32)
                .child(text_button("Cancel").on_click(|_| crate::panels::close()))
                .child(action_button(what, on_action)),
        )
}

/// The button that does it, in the error colors; it squares off a little while pressed.
fn action_button(what: Ask, hovered: Signal<bool>) -> Div {
    let pressed = signal(false);
    let corner = follow(
        move || if pressed.get() { 10.0 } else { 20.0 },
        fast_spatial(),
    );
    pressable(
        div()
            .h(40.0)
            .px(24.0_f32)
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorIcon::Pointer)
            .on_hover(move |h| {
                hovered.set(h);
                if !h {
                    pressed.set(false);
                }
            })
            .on_pointer_down(move |_| pressed.set(true))
            .on_pointer_up(move |_| pressed.set(false))
            .dyn_style(move |s| {
                let c = colors();
                s.rounded(corner.get()).bg(c.error).text_color(c.on_error)
            })
            .on_click(move |_| {
                crate::panels::close();
                let s = session();
                match what {
                    Ask::LogOut => log_out(),
                    Ask::Restart => drop(s.reboot()),
                    Ask::PowerOff => drop(s.power_off()),
                }
            })
            .child(static_txt(TextStyle::LABEL_LARGE, what.action())),
        |c| c.on_error,
    )
}

/// Quits the compositor this shell runs in, which ends the session; the socket comes from the
/// environment ShojiWM started the shell with.
fn log_out() {
    match std::process::Command::new("shoji_wm").arg("--quit").spawn() {
        // Waited for, so that it is not left behind if the compositor stays.
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => eprintln!("shoji-bar-4: couldn't log out: {e}"),
    }
}
