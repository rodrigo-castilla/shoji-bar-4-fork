//! The panels that open over the desktop: quick settings, the calendar, the launcher, the
//! clipboard's history, what is playing and the question before shutting down. At most
//! one is open on an output. Each is a layer over all of the output that is there from when the
//! panel starts to open until it has finished closing; a click outside the panel closes it.

use crate::placement::{AnchorPoint, BarPlacement, PanelSide};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    QuickSettings,
    Calendar,
    Launcher,
    Clipboard,
    Media,
    Power,
}

impl Panel {
    pub(crate) const ALL: [Panel; 6] = [
        Self::QuickSettings,
        Self::Calendar,
        Self::Launcher,
        Self::Clipboard,
        Self::Media,
        Self::Power,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::QuickSettings => "quick-settings",
            Self::Calendar => "calendar",
            Self::Launcher => "launcher",
            Self::Clipboard => "clipboard",
            Self::Media => "media",
            Self::Power => "power",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// Whether it gives the focus to a field of its own as it opens.
    fn has_field(self) -> bool {
        matches!(self, Self::Launcher | Self::Clipboard)
    }

    fn centered(self) -> bool {
        matches!(self, Self::Launcher | Self::Clipboard | Self::Power)
    }

    /// Whether the desktop around it is veiled (and so blurred by the compositor), for what is in
    /// the middle of the output.
    fn veiled(self) -> bool {
        self.centered()
    }
}

/// A panel invocation on one output. Media carries its invoking island's centre.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Invocation {
    output: String,
    panel: Panel,
    anchor: Option<AnchorPoint>,
}

/// Which panel is open on which output, by connector name.
type Open = Signal<Option<Invocation>>;

thread_local! {
    static OPEN: Cell<Option<Open>> = const { Cell::new(None) };
}

fn state() -> Open {
    OPEN.with(|o| {
        o.get().unwrap_or_else(|| {
            let (s, _) = create_root(|| signal(None));
            o.set(Some(s));
            s
        })
    })
}

pub fn is_open(output: &Output, panel: Panel) -> bool {
    state().with(|s| {
        s.as_ref()
            .is_some_and(|o| o.output == output.name && o.panel == panel)
    })
}

pub fn toggle(output: &Output, panel: Panel) {
    toggle_with(output, panel, None);
}

pub fn toggle_at(output: &Output, panel: Panel, anchor: AnchorPoint) {
    toggle_with(output, panel, Some(anchor));
}

fn toggle_with(output: &Output, panel: Panel, anchor: Option<AnchorPoint>) {
    let now = is_open(output, panel);
    state().set((!now).then(|| Invocation {
        output: output.name.clone(),
        panel,
        anchor,
    }));
}

pub fn open(output_name: &str, panel: Panel) {
    state().set(Some(Invocation {
        output: output_name.to_owned(),
        panel,
        anchor: None,
    }));
}

pub fn close() {
    state().set(None);
}

/// Whether anything is open, and where.
pub fn current() -> Option<(String, Panel)> {
    state().get().map(|o| (o.output, o.panel))
}

/// The current invocation of one output and panel, including its optional anchor.
pub(crate) fn invocation(output: &Output, panel: Panel) -> Memo<Option<Invocation>> {
    let name = output.name.clone();
    memo(move || {
        state().with(|s| {
            s.as_ref()
                .filter(|o| o.output == name && o.panel == panel)
                .cloned()
        })
    })
}

/// A panel as it is when open, for pictures of it.
pub fn view(output: Output, panel: Panel) -> Div {
    let invocation = invocation(&output, panel);
    let wanted = memo(move || invocation.get().is_some());
    surface_at(output, panel, wanted, invocation, crate::placement::memoized())
}

fn place_panel(root: Style, panel: Panel, at: BarPlacement) -> Style {
    let clearance = at.clearance();
    match panel {
        Panel::QuickSettings => match at.side() {
            PanelSide::Below => root.items_end().justify_start().pt(clearance).pr(14.0_f32),
            PanelSide::Left => root.items_start().justify_end().pl(clearance).pb(14.0_f32),
            PanelSide::Right => root.items_end().justify_end().pr(clearance).pb(14.0_f32),
        },
        Panel::Calendar => match at.side() {
            PanelSide::Below => root.items_center().justify_start().pt(clearance),
            PanelSide::Left => root.items_start().justify_center().pl(clearance),
            PanelSide::Right => root.items_end().justify_center().pr(clearance),
        },
        // Next to the bar's island of it, which supplies the other coordinate.
        Panel::Media => match at.side() {
            PanelSide::Below => root.items_start().justify_start().pt(clearance),
            PanelSide::Left => root.items_start().justify_start().pl(clearance),
            PanelSide::Right => root.items_end().justify_start().pr(clearance),
        },
        Panel::Launcher | Panel::Clipboard | Panel::Power => root.items_center().justify_center(),
    }
}

fn enter_motion(s: Style, panel: Panel, at: BarPlacement) -> Style {
    if panel.centered() {
        return s.opacity(0.0_f32).scale(0.8_f32).translate_y(40.0_f32);
    }
    let (x, y) = at.enter_offset();
    s.opacity(0.0_f32)
        .scale(0.7_f32)
        .translate_x(x)
        .translate_y(y)
}

fn exit_motion(s: Style, panel: Panel, at: BarPlacement) -> Style {
    if panel.centered() {
        return s.opacity(0.0_f32).scale(0.9_f32).translate_y(-8.0_f32);
    }
    let (x, y) = at.exit_offset();
    s.opacity(0.0_f32)
        .scale(0.9_f32)
        .translate_x(x)
        .translate_y(y)
}

/// All of the output: a catcher of clicks outside the panel, and the panel where it goes.
pub(crate) fn surface_at(
    output: Output,
    panel: Panel,
    wanted: Memo<bool>,
    invocation: Memo<Option<Invocation>>,
    placement: Memo<BarPlacement>,
) -> Div {
    let content = move || match panel {
        Panel::QuickSettings => crate::quick_settings::panel(output.clone()),
        Panel::Calendar => crate::calendar::panel(),
        Panel::Launcher => crate::launcher::panel(output.clone()),
        Panel::Clipboard => crate::clipboard::panel(output.clone()),
        Panel::Media => {
            let anchor = untrack(|| invocation.get()).and_then(|i| i.anchor);
            crate::media_panel::panel(output.clone(), anchor, untrack(|| placement.get()))
        }
        Panel::Power => crate::power_dialog::panel(),
    };
    // Keys go to what has the focus, and with nothing focused not to here but to what holds it: the
    // panel takes the focus as it opens, for Escape. The launcher and the clipboard give it to their
    // search instead.
    let focus = node_ref();
    if !panel.has_field() {
        request_frame(move |_| focus.focus());
    }
    let root = div()
        .node_ref(focus)
        .focusable()
        .size_full()
        .relative()
        // Its own stacking context, so that the catcher below is behind the panel but not behind
        // this, which would take its clicks.
        .z_index(0)
        .flex()
        .flex_col()
        .select_none()
        .on_key_down(move |e| {
            // Out of a page of quick settings first, then out of the panel.
            if e.key == Key::Named(NamedKey::Escape)
                && !(panel == Panel::QuickSettings && crate::quick_settings::back())
            {
                close();
            }
        })
        // Behind the panel (`absolute` alone would put it above): what is clicked outside it.
        .child(div().absolute().inset(0.0).z_index(-1).on_click(move |_| {
            if untrack(|| wanted.get()) {
                close();
            }
        }));
    // Where it is follows the bar, without what is in it being built again.
    let mut root = root.dyn_style(move |s| place_panel(s, panel, placement.get()));
    if panel == Panel::Media {
        // It is put by the island it was opened from, which has just gone with the bar.
        let first = Cell::new(true);
        effect(move || {
            placement.get();
            if !first.replace(false) && untrack(|| wanted.get()) {
                close();
            }
        });
    }
    if panel.veiled() {
        // A veil over the desktop, so that the panel in the middle stands out.
        root = root.child(
            show(
                move || wanted.get(),
                || {
                    div()
                        .absolute()
                        .z_index(-1)
                        .inset(0.0)
                        .pointer_events_none()
                        .dyn_style(|s| s.bg(crate::theme::colors().scrim.with_alpha(0.32)))
                },
            )
            .appear()
            .enter(|s| s.opacity(0.0_f32))
            .exit(|s| s.opacity(0.0_f32))
            .transition(Transition::ms(300).ease_out()),
        );
    }
    root.child(
        show(move || wanted.get(), content)
            .appear()
            .enter(move |s| enter_motion(s, panel, untrack(|| placement.get())))
            .exit(move |s| exit_motion(s, panel, untrack(|| placement.get())))
            .transition(crate::ui::overshoot(420)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_anchor_belongs_to_its_invocation() {
        close();
        let output = Output {
            name: "DP-1".to_owned(),
            ..Default::default()
        };
        let anchor = AnchorPoint { x: 40.0, y: 320.0 };
        let ((), owner) = create_root(|| {
            toggle_at(&output, Panel::Media, anchor);
            let opened = invocation(&output, Panel::Media);
            assert_eq!(opened.get().and_then(|i| i.anchor), Some(anchor));

            // An IPC opening on another output must not reuse the old island's coordinates.
            open("DP-2", Panel::Media);
            let other = Output {
                name: "DP-2".to_owned(),
                ..Default::default()
            };
            let opened = invocation(&other, Panel::Media);
            assert_eq!(opened.get().and_then(|i| i.anchor), None);
        });
        owner.dispose();
        close();
    }
}
