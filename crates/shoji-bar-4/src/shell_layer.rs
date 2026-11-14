//! Shell-surface lifecycle behind one interface. The Wayland adapter opens real layers;
//! tests use a recording adapter through the same functions.

use crate::panels::{self, Panel};
use crate::placement::{self, BarPlacement};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;
use std::cell::Cell;
use std::time::Duration;

const PANEL_EXIT_TIME: Duration = Duration::from_millis(380);
const NOTIFICATION_HEIGHT: i32 = 560;

/// A domain-level layer request, before Wayland options are derived.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Request {
    output: Option<Output>,
    namespace: String,
    kind: RequestKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RequestKind {
    Bar(BarPlacement),
    Panel(Panel),
    Notifications,
}

impl Request {
    fn bar(output: Output, at: BarPlacement) -> Self {
        Self {
            output: Some(output),
            namespace: "shoji-bar-4-bar".to_owned(),
            kind: RequestKind::Bar(at),
        }
    }

    fn panel(output: Output, panel: Panel) -> Self {
        Self {
            output: Some(output),
            namespace: format!("shoji-bar-4-{}", panel.name()),
            kind: RequestKind::Panel(panel),
        }
    }

    fn notifications() -> Self {
        Self {
            output: None,
            namespace: "shoji-bar-4-notifications".to_owned(),
            kind: RequestKind::Notifications,
        }
    }

    fn into_wayland_options(self) -> LayerOptions {
        let mut options = match self.kind {
            RequestKind::Bar(at) => LayerOptions::bar(at.edge(), at.thickness()),
            RequestKind::Panel(_) => LayerOptions::new()
                .layer(Layer::Top)
                .anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT)
                // Over the bar and the dock too, so that a click anywhere closes it.
                .exclusive(Exclusive::Ignore)
                // And Escape, without clicking it first.
                .keyboard(Keyboard::Exclusive),
            RequestKind::Notifications => LayerOptions::new()
                .layer(Layer::Overlay)
                .anchor(Anchor::TOP | Anchor::RIGHT)
                .margin(Margin {
                    top: 6,
                    right: 10,
                    ..Default::default()
                })
                .width(crate::notify::WIDTH as i32 + 28)
                .height(NOTIFICATION_HEIGHT)
                .input_region(InputRegion::Empty),
        };
        options = match self.output {
            Some(output) => options.output(&output),
            None => options.any_output(),
        };
        options.namespace(self.namespace)
    }
}

/// The layer operations used after opening.
pub(crate) trait Handle: Clone {
    fn set_input_region(&self, region: InputRegion);
    fn set_placement(&self, at: BarPlacement);
}

/// A concrete shell-surface implementation at the lifecycle seam.
pub(crate) trait Adapter: Clone + 'static {
    type Handle: Handle;

    fn open(&self, request: Request, root: Box<dyn FnOnce() -> View>) -> Self::Handle;
}

/// The production Wayland implementation.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Wayland;

#[derive(Clone)]
pub(crate) struct WaylandHandle {
    layer: LayerHandle,
}

impl Handle for WaylandHandle {
    fn set_input_region(&self, region: InputRegion) {
        self.layer.set_input_region(region);
    }

    fn set_placement(&self, at: BarPlacement) {
        // The edge, both sizes and the reserved thickness in one commit, so the bar never shows
        // half of the turn or keeps the old edge's length as its zone.
        let (across, width, height) = if at.is_vertical() {
            (
                Anchor::TOP | Anchor::BOTTOM,
                LayerSize::Fixed(at.thickness()),
                LayerSize::Stretch,
            )
        } else {
            (
                Anchor::LEFT | Anchor::RIGHT,
                LayerSize::Stretch,
                LayerSize::Fixed(at.thickness()),
            )
        };
        self.layer.set_geometry(at.edge() | across, width, height);
    }
}

impl Adapter for Wayland {
    type Handle = WaylandHandle;

    fn open(&self, request: Request, root: Box<dyn FnOnce() -> View>) -> Self::Handle {
        WaylandHandle {
            layer: open_layer(request.into_wayland_options(), move || root()),
        }
    }
}

/// Opens one bar per output and updates it in place when placement changes.
pub(crate) fn open_bar<A: Adapter>(output: Output, adapter: A) {
    let placement = placement::memoized();
    let at = untrack(|| placement.get());
    let request = Request::bar(output.clone(), at);
    let view_output = output.clone();
    let layer = untrack(|| {
        adapter.open(
            request,
            Box::new(move || {
                div()
                    .size_full()
                    .child(dyn_view(move || {
                        crate::bar::view(view_output.clone(), placement.get())
                    }))
                    .into_view()
            }),
        )
    });
    let initial = Cell::new(true);
    effect(move || {
        let at = placement.get();
        if !initial.replace(false) {
            layer.set_placement(at);
        }
    });
}

/// Keeps every panel's layer mounted while it opens, remains open and closes.
pub(crate) fn install_panels<A: Adapter>(output: Output, adapter: A) {
    for panel in Panel::ALL {
        install_panel(output.clone(), panel, adapter.clone());
    }
}

fn install_panel<A: Adapter>(output: Output, panel: Panel, adapter: A) {
    let invocation = panels::invocation(&output, panel);
    let wanted = memo(move || invocation.get().is_some());
    let mounted = signal(false);
    let pending: Cell<Option<Timeout>> = Cell::new(None);
    effect(move || {
        if let Some(t) = pending.take() {
            t.cancel();
        }
        if wanted.get() {
            mounted.set(true);
        } else if mounted.get_untracked() {
            pending.set(Some(set_timeout(PANEL_EXIT_TIME, move || {
                if !untrack(|| wanted.get()) {
                    mounted.set(false);
                }
            })));
        }
    });

    let placement = placement::memoized();
    effect(move || {
        if !mounted.get() {
            return;
        }
        let request = Request::panel(output.clone(), panel);
        let content_output = output.clone();
        // Not built again when the placement changes, so what is open in it stays as it is.
        let layer = untrack(|| {
            adapter.open(
                request,
                Box::new(move || {
                    panels::surface_at(content_output, panel, wanted, invocation, placement)
                        .into_view()
                }),
            )
        });
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

/// Keeps notification popups mounted while cards arrive and leave.
pub(crate) fn open_popups<A: Adapter>(adapter: A) {
    let Some(server) = crate::notify::server() else {
        return;
    };
    let any = memo({
        let server = server.clone();
        move || !server.popups().is_empty()
    });
    let mounted = signal(false);
    let pending: Cell<Option<Timeout>> = Cell::new(None);
    effect(move || {
        if let Some(t) = pending.take() {
            t.cancel();
        }
        if any.get() {
            mounted.set(true);
        } else {
            pending.set(Some(set_timeout(Duration::from_millis(700), move || {
                mounted.set(false)
            })));
        }
    });

    effect(move || {
        if !mounted.get() {
            return;
        }
        let server = server.clone();
        let region = signal(InputRegion::Empty);
        let layer = untrack(|| {
            adapter.open(
                Request::notifications(),
                Box::new(move || crate::notify::popups(server.clone(), region).into_view()),
            )
        });
        effect(move || layer.set_input_region(region.get()));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::placement::PanelSide;
    use crate::settings::{Settings, ThemeMode};
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone, Default)]
    struct Recording {
        records: Rc<RefCell<Vec<Record>>>,
    }

    struct Record {
        request: Request,
        active: bool,
        inputs: Vec<InputRegion>,
        placements: Vec<BarPlacement>,
    }

    #[derive(Clone)]
    struct RecordingHandle {
        records: Rc<RefCell<Vec<Record>>>,
        id: usize,
    }

    impl Handle for RecordingHandle {
        fn set_input_region(&self, region: InputRegion) {
            self.records.borrow_mut()[self.id].inputs.push(region);
        }

        fn set_placement(&self, at: BarPlacement) {
            self.records.borrow_mut()[self.id].placements.push(at);
        }
    }

    impl Adapter for Recording {
        type Handle = RecordingHandle;

        fn open(&self, request: Request, root: Box<dyn FnOnce() -> View>) -> Self::Handle {
            // A recording never constructs the real services or view tree.
            drop(root);
            let id = self.records.borrow().len();
            self.records.borrow_mut().push(Record {
                request,
                active: true,
                inputs: Vec::new(),
                placements: Vec::new(),
            });
            let records = self.records.clone();
            on_cleanup(move || records.borrow_mut()[id].active = false);
            RecordingHandle {
                records: self.records.clone(),
                id,
            }
        }
    }

    impl Recording {
        fn len(&self) -> usize {
            self.records.borrow().len()
        }

        fn active_names(&self) -> Vec<String> {
            let mut names: Vec<_> = self
                .records
                .borrow()
                .iter()
                .filter(|r| r.active)
                .filter_map(|r| r.request.output.as_ref().map(|o| o.name.clone()))
                .collect();
            names.sort();
            names
        }

        fn active_count(&self) -> usize {
            self.records.borrow().iter().filter(|r| r.active).count()
        }
    }

    fn output(name: &str) -> Output {
        Output {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn reset_settings() {
        crate::settings::never_save();
        crate::settings::settings().set(Settings::default());
    }

    #[test]
    fn bars_stay_on_their_outputs_and_update_in_place() {
        reset_settings();
        panels::open("DP-1", Panel::Launcher);
        let recording = Recording::default();
        let ((), owner) = create_root(|| {
            for name in ["DP-1", "DP-2", "HDMI-A-2"] {
                open_bar(output(name), recording.clone());
            }
        });
        assert_eq!(recording.len(), 3);
        assert_eq!(recording.active_names(), ["DP-1", "DP-2", "HDMI-A-2"]);
        assert!(panels::is_open(&output("DP-1"), Panel::Launcher));

        crate::settings::settings().update(|s| s.mode = ThemeMode::Light);
        assert_eq!(
            recording.len(),
            3,
            "an unrelated setting must not reopen bars"
        );

        crate::settings::settings().update(|s| s.bar_position = crate::settings::BarPosition::Left);
        assert_eq!(
            recording.len(),
            3,
            "placement must update the existing bars"
        );
        assert!(panels::is_open(&output("DP-1"), Panel::Launcher));
        assert_eq!(recording.active_names(), ["DP-1", "DP-2", "HDMI-A-2"]);
        assert!(
            recording
                .records
                .borrow()
                .iter()
                .filter(|r| r.active)
                .all(|r| r
                    .placements
                    .last()
                    .is_some_and(|at| at.side() == PanelSide::Left))
        );

        owner.dispose();
        assert_eq!(recording.active_count(), 0);
    }

    #[test]
    fn a_panel_updates_for_placement_but_not_for_an_unrelated_setting() {
        reset_settings();
        panels::close();
        let recording = Recording::default();
        let output = output("DP-1");
        let ((), owner) = create_root(|| {
            install_panels(output.clone(), recording.clone());
            panels::open("DP-1", Panel::Calendar);
        });
        assert_eq!(recording.len(), 1);
        assert!(matches!(
            recording.records.borrow()[0].request.kind,
            RequestKind::Panel(Panel::Calendar)
        ));
        assert_eq!(recording.records.borrow()[0].inputs, [InputRegion::Full]);

        crate::settings::settings().update(|s| s.mode = ThemeMode::Light);
        assert_eq!(recording.len(), 1);

        crate::settings::settings()
            .update(|s| s.bar_position = crate::settings::BarPosition::Right);
        assert_eq!(
            recording.len(),
            1,
            "placement must update the existing panel"
        );
        assert_eq!(recording.active_count(), 1);
        assert_eq!(recording.active_names(), ["DP-1"]);

        panels::close();
        assert_eq!(
            recording.records.borrow()[0].inputs.last(),
            Some(&InputRegion::Empty)
        );
        owner.dispose();
        assert_eq!(recording.active_count(), 0);
    }
}
