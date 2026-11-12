//! The panel manager: panels that open over the desktop (quick settings, a calendar, a launcher,
//! a dialog...), whoever defines them.
//!
//! A panel is a [`PanelDef`] registered under an id with [`register`]. At most one panel is open
//! on an output. Each open panel is a layer over all of the output that exists from when the panel
//! starts to open until it has finished closing; a click outside the panel closes it, and so does
//! Escape.
//!
//! ```ignore
//! use shoji_core::panels::{PanelDef, register, toggle};
//!
//! // Once, before the outputs are set up:
//! register(
//!     PanelDef::new("my-panel", |_output| div().child("hello"))
//!         .place(|root| root.items_center().justify_center()),
//! );
//!
//! // Anywhere, for example from a button: open it on `output` (or close it if it is open).
//! toggle(&output, "my-panel");
//! ```
//!
//! `shoji-bar-4 msg toggle my-panel` opens any registered panel from outside, too.

use crate::theme::colors;
use crate::ui::overshoot;
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;
use std::time::Duration;

/// The name of a panel: what [`register`] files it under and `msg toggle <id>` asks for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PanelId(pub &'static str);

impl fmt::Display for PanelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl From<&'static str> for PanelId {
    fn from(id: &'static str) -> Self {
        Self(id)
    }
}

/// How a panel is defined: what it shows, where it sits and how it comes in.
///
/// Built with [`PanelDef::new`] and the methods that follow it.
#[derive(Clone)]
pub struct PanelDef {
    id: PanelId,
    content: Rc<dyn Fn(Output) -> Div>,
    place: Rc<dyn Fn(Div) -> Div>,
    enter: Rc<dyn Fn(Style) -> Style>,
    has_field: bool,
    veiled: bool,
    on_escape: Option<Rc<dyn Fn() -> bool>>,
}

impl PanelDef {
    /// A panel called `id` that shows what `content` builds for the output it opens on.
    ///
    /// Unless changed it sits in the middle of the output and comes up a little as it grows.
    pub fn new(id: impl Into<PanelId>, content: impl Fn(Output) -> Div + 'static) -> Self {
        Self {
            id: id.into(),
            content: Rc::new(content),
            place: Rc::new(|root| root.items_center().justify_center()),
            enter: Rc::new(rise_in),
            has_field: false,
            veiled: false,
            on_escape: None,
        }
    }

    /// Where the panel goes on the output. `root` is a column that covers all of the output (the
    /// bar and the dock too); set how it aligns its child, and keep clear of the bar if the panel
    /// is at the top.
    pub fn place(mut self, place: impl Fn(Div) -> Div + 'static) -> Self {
        self.place = Rc::new(place);
        self
    }

    /// The style the panel starts from as it comes in, and goes back to a little as it leaves.
    /// See [`from_above`] and [`rise_in`].
    pub fn enter(mut self, enter: impl Fn(Style) -> Style + 'static) -> Self {
        self.enter = Rc::new(enter);
        self
    }

    /// The panel has a field of its own (a search box) that takes the keyboard focus as it opens.
    /// Without one, the panel itself takes the focus, to hear Escape.
    pub fn with_field(mut self) -> Self {
        self.has_field = true;
        self
    }

    /// Veils the desktop around the panel, which the compositor then blurs. For what is in the
    /// middle of the output.
    pub fn veiled(mut self) -> Self {
        self.veiled = true;
        self
    }

    /// Runs when Escape is pressed. Return `true` to say it was used (to leave a page inside the
    /// panel, say) and so the panel stays open; `false` closes the panel.
    pub fn on_escape(mut self, on_escape: impl Fn() -> bool + 'static) -> Self {
        self.on_escape = Some(Rc::new(on_escape));
        self
    }

    /// The id the panel was registered under.
    /// Whether the panel has a text field that takes the keyboard.
    pub fn has_field(&self) -> bool {
        self.has_field
    }

    /// Whether a veil goes over the screen behind the panel.
    pub fn is_veiled(&self) -> bool {
        self.veiled
    }

    pub fn id(&self) -> PanelId {
        self.id
    }
}

/// Down from above, growing as it fades in: for panels that hang from the top of the output.
pub fn from_above(s: Style) -> Style {
    s.opacity(0.0_f32).scale(0.7_f32).translate_y(-80.0_f32)
}

/// Up a little as it grows: for panels in the middle of the output.
pub fn rise_in(s: Style) -> Style {
    s.opacity(0.0_f32).scale(0.8_f32).translate_y(40.0_f32)
}

thread_local! {
    static REGISTRY: RefCell<Vec<PanelDef>> = const { RefCell::new(Vec::new()) };
}

/// Makes a panel available. Call it before [`install`] runs for the outputs, once per panel:
/// registering an id again replaces the earlier definition, so an interface can swap one of the
/// stock panels for its own.
pub fn register(def: PanelDef) {
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        match r.iter_mut().find(|d| d.id == def.id) {
            Some(slot) => *slot = def,
            None => r.push(def),
        }
    });
}

/// The registered panel called `name`, for commands that name it as text.
pub fn lookup(name: &str) -> Option<PanelId> {
    REGISTRY.with(|r| r.borrow().iter().find(|d| d.id.0 == name).map(|d| d.id))
}

/// Every registered panel, in the order they were registered.
pub fn registered() -> Vec<PanelId> {
    REGISTRY.with(|r| r.borrow().iter().map(|d| d.id).collect())
}

fn definition(id: PanelId) -> Option<PanelDef> {
    REGISTRY.with(|r| r.borrow().iter().find(|d| d.id == id).cloned())
}

/// How long a panel's closing takes, after which the layer goes.
const EXIT_TIME: Duration = Duration::from_millis(380);

/// Which panel is open on which output, by connector name.
type Open = Signal<Option<(String, PanelId)>>;

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

/// Whether `panel` is open on `output`.
pub fn is_open(output: &Output, panel: impl Into<PanelId>) -> bool {
    let panel = panel.into();
    state().with(|s| {
        s.as_ref()
            .is_some_and(|(o, p)| *o == output.name && *p == panel)
    })
}

/// Opens `panel` on `output`, or closes it if it is open there. Whatever else was open closes.
pub fn toggle(output: &Output, panel: impl Into<PanelId>) {
    let panel = panel.into();
    let now = is_open(output, panel);
    state().set((!now).then(|| (output.name.clone(), panel)));
}

/// Opens `panel` on the output called `output_name`.
pub fn open(output_name: &str, panel: impl Into<PanelId>) {
    state().set(Some((output_name.to_owned(), panel.into())));
}

/// Closes whatever is open.
pub fn close() {
    state().set(None);
}

/// Whether anything is open, and where.
pub fn current() -> Option<(String, PanelId)> {
    state().get()
}

/// Keeps the layers of the panels of `output` as they should be. Call once per output, after the
/// panels are registered.
pub fn install(output: Output) {
    for id in registered() {
        let wanted = memo({
            let output = output.clone();
            move || is_open(&output, id)
        });
        // There while it opens, is open, and closes.
        let mounted = signal(false);
        let pending: Cell<Option<Timeout>> = Cell::new(None);
        effect(move || {
            if let Some(t) = pending.take() {
                t.cancel();
            }
            if wanted.get() {
                mounted.set(true);
            } else if mounted.get_untracked() {
                pending.set(Some(set_timeout(EXIT_TIME, move || {
                    if !untrack(|| wanted.get()) {
                        mounted.set(false);
                    }
                })));
            }
        });
        let output = output.clone();
        effect(move || {
            if !mounted.get() {
                return;
            }
            let content_output = output.clone();
            let layer = open_layer(
                LayerOptions::new()
                    .layer(Layer::Top)
                    .anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
                    // Over the bar and the dock too, so that a click anywhere closes it.
                    .exclusive(Exclusive::Ignore)
                    // And Escape, without clicking it first.
                    .keyboard(Keyboard::Exclusive)
                    .output(&output)
                    .namespace(format!("shoji-bar-4-{id}")),
                move || surface(content_output, id, wanted),
            );
            // While it closes, clicks go to what is under it.
            effect(move || {
                layer.set_input_region(if wanted.get() {
                    InputRegion::Full
                } else {
                    InputRegion::Empty
                });
            });
        });
    }
}

/// A panel as it is when open, for pictures of it.
pub fn view(output: Output, panel: impl Into<PanelId>) -> Div {
    let panel = panel.into();
    let wanted = memo({
        let output = output.clone();
        move || is_open(&output, panel)
    });
    surface(output, panel, wanted)
}

/// All of the output: a catcher of clicks outside the panel, and the panel where it goes.
fn surface(output: Output, id: PanelId, wanted: Memo<bool>) -> Div {
    let Some(def) = definition(id) else {
        return div().child(format!("no panel called {id}"));
    };
    let content = {
        let content = def.content.clone();
        move || content(output.clone())
    };
    // Keys go to what has the focus, and with nothing focused not to here but to what holds it: the
    // panel takes the focus as it opens, for Escape. A panel with a field of its own gives the
    // focus to that instead.
    let focus = node_ref();
    if !def.has_field {
        request_frame(move |_| focus.focus());
    }
    let on_escape = def.on_escape.clone();
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
            // Out of a page inside the panel first, then out of the panel.
            if e.key == Key::Named(NamedKey::Escape)
                && !on_escape.as_ref().is_some_and(|handled| handled())
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
    let mut root = (def.place)(root);
    if def.veiled {
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
                        .dyn_style(|s| s.bg(colors().scrim.with_alpha(0.32)))
                },
            )
            .appear()
            .enter(|s| s.opacity(0.0_f32))
            .exit(|s| s.opacity(0.0_f32))
            .transition(Transition::ms(300).ease_out()),
        );
    }
    let enter = def.enter.clone();
    root.child(
        show(move || wanted.get(), content)
            .appear()
            .enter(move |s| enter(s))
            .exit(|s| s.opacity(0.0_f32).scale(0.9_f32).translate_y(-8.0_f32))
            .transition(overshoot(420)),
    )
}
