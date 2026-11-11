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

    /// Puts the interface on the screen. Call it from the root of the shell (see
    /// [`run_with`](crate::run_with)).
    pub fn install(self) {
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
