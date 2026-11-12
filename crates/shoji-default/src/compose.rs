//! The stock interface as data: what [`Ui::stock`] holds is what the stock `shoji-bar-4` shows. An
//! interface of your own can start from it, change it, and [`install`](Ui::install) the result.
//!
//! ```ignore
//! let mut ui = shoji_default::Ui::stock();
//! if let Some(bar) = ui.bar.as_mut() {
//!     bar.height = 34;
//!     bar.remove("tray");
//!     bar.insert_after("clock", Island::new("hello", |_output| div().child("hello")));
//! }
//! ui.replace_panel(PanelDef::new("calendar", |_| div().child("no calendar today")));
//! ui.install();
//! ```

use crate::bar::{self, BarLayout};
use crate::panels::{self, PanelDef, PanelId};
use crate::{dock, ipc, keys, lock, notify, omarchy, osd, theme, wallpaper, wm};
use anti_webui::shell::prelude::*;
use std::rc::Rc;
use std::sync::OnceLock;

static OUTLINE: OnceLock<String> = OnceLock::new();

/// What `shoji-bar-4 msg layout` prints: the interface that was installed, as text.
pub fn outline() -> Option<&'static str> {
    OUTLINE.get().map(String::as_str)
}

/// The services that come with the stock interface and have a screen of their own. Each is on in
/// the stock interface; turn one off to draw it yourself (or to go without).
#[derive(Clone, Copy, Debug)]
pub struct Services {
    /// Becomes the notification daemon and shows the popups.
    pub notifications: bool,
    /// Shows the volume and brightness pill.
    pub osd: bool,
    /// The lock screen.
    pub lock: bool,
}

/// What the stock interface is made of.
pub struct Ui {
    pub services: Services,
    /// The bar along the top, or none.
    pub bar: Option<BarLayout>,
    /// The dock that rises from the bottom edge.
    pub dock: bool,
    /// The wallpaper layer.
    pub wallpaper: bool,
    /// The panels, in the order they are registered.
    pub panels: Vec<PanelDef>,
    on_start: Vec<Box<dyn FnOnce()>>,
    on_output: Vec<Rc<dyn Fn(Output)>>,
}

impl Ui {
    /// The stock interface.
    pub fn stock() -> Self {
        Self {
            services: Services {
                notifications: true,
                osd: true,
                lock: true,
            },
            bar: Some(BarLayout::stock()),
            dock: true,
            wallpaper: true,
            panels: panels::stock(),
            on_start: Vec::new(),
            on_output: Vec::new(),
        }
    }

    /// Adds a panel, or replaces the one with the same id.
    pub fn replace_panel(&mut self, def: PanelDef) -> &mut Self {
        match self.panels.iter_mut().find(|p| p.id() == def.id()) {
            Some(slot) => *slot = def,
            None => self.panels.push(def),
        }
        self
    }

    /// Takes a panel away.
    pub fn remove_panel(&mut self, id: impl Into<PanelId>) -> &mut Self {
        let id = id.into();
        self.panels.retain(|p| p.id() != id);
        self
    }

    /// Runs `f` once the services are up and before any output is set up.
    pub fn on_start(&mut self, f: impl FnOnce() + 'static) -> &mut Self {
        self.on_start.push(Box::new(f));
        self
    }

    /// Runs `f` for every output as it appears, after the bar, the dock and the wallpaper: the
    /// place to open layers of your own.
    pub fn on_output(&mut self, f: impl Fn(Output) + 'static) -> &mut Self {
        self.on_output.push(Rc::new(f));
        self
    }

    /// The interface as text: the bar's islands by zone, the panels, what else is on. This is what
    /// `shoji-bar-4 msg layout` prints, so a developer can see what to name in `insert_after`,
    /// `replace` or `msg toggle`.
    pub fn outline(&self) -> String {
        use std::fmt::Write;
        let mut out = format!("shoji-bar-4 {} interface\n\n", env!("CARGO_PKG_VERSION"));
        match &self.bar {
            Some(bar) => out.push_str(&bar.outline()),
            None => out.push_str("bar  none\n"),
        }
        let _ = writeln!(out, "\npanels ({})   open one with: shoji-bar-4 msg toggle <id>", self.panels.len());
        for panel in &self.panels {
            let mut flags = Vec::new();
            if panel.has_field() {
                flags.push("text field");
            }
            if panel.is_veiled() {
                flags.push("veil");
            }
            let _ = writeln!(out, "  {:<16} {}", panel.id().to_string(), flags.join(", "));
        }
        let on = |b: bool| if b { "on" } else { "off" };
        let _ = write!(
            out,
            "\nother layers: dock={} wallpaper={}\nservices: notifications={} osd={} lock={}\n",
            on(self.dock),
            on(self.wallpaper),
            on(self.services.notifications),
            on(self.services.osd),
            on(self.services.lock),
        );
        out
    }

    /// Puts the interface on the screen. Call it from the root of the shell (see
    /// [`run_with`](crate::run_with)).
    pub fn install(self) {
        let _ = OUTLINE.set(self.outline());
        ipc::serve();
        theme::install();
        wm::share_theme();
        if self.services.notifications {
            notify::open_popups();
        }
        if self.services.osd {
            osd::install();
            omarchy::serve();
            keys::take();
            crate::clipboard::watch();
            crate::shutdown::install();
        }
        if self.services.lock {
            lock::install();
        }
        for def in self.panels {
            panels::register(def);
        }
        for f in self.on_start {
            f();
        }
        let Self {
            bar,
            dock: with_dock,
            wallpaper: with_wallpaper,
            on_output,
            ..
        } = self;
        for_each_output(move |output| {
            if let Some(layout) = &bar {
                bar::open_with(output.clone(), layout);
            }
            if with_dock {
                dock::open(output.clone());
            }
            if with_wallpaper {
                wallpaper::open(output.clone());
            }
            for f in &on_output {
                f(output.clone());
            }
            panels::install(output);
        });
    }
}
