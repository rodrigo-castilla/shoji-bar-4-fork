//! Geometry derived from the configured bar edge. One value is shared by one built view,
//! so every island and panel uses the same orientation snapshot.

use crate::settings::{BarPosition, settings};
use anti_webui::prelude::*;
use anti_webui::shell::prelude::*;

pub const HEIGHT: i32 = 38;
pub const WIDTH: i32 = 52;
const CLEARANCE: f32 = 6.0;

/// Which side of an output a bar-adjacent panel belongs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSide {
    Below,
    Left,
    Right,
}

/// An invoking island's centre in output coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorPoint {
    pub x: f32,
    pub y: f32,
}

/// The bar's position and every geometric fact derived from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarPlacement {
    position: BarPosition,
}

impl BarPlacement {
    pub const fn new(position: BarPosition) -> Self {
        Self { position }
    }

    pub fn is_vertical(self) -> bool {
        self.position.is_vertical()
    }

    pub fn edge(self) -> Anchor {
        match self.position {
            BarPosition::Top => Anchor::TOP,
            BarPosition::Left => Anchor::LEFT,
            BarPosition::Right => Anchor::RIGHT,
        }
    }

    pub fn side(self) -> PanelSide {
        match self.position {
            BarPosition::Top => PanelSide::Below,
            BarPosition::Left => PanelSide::Left,
            BarPosition::Right => PanelSide::Right,
        }
    }

    pub fn thickness(self) -> i32 {
        if self.is_vertical() { WIDTH } else { HEIGHT }
    }

    pub fn clearance(self) -> f32 {
        self.thickness() as f32 + CLEARANCE
    }

    /// Entry displacement for a panel adjacent to the bar.
    pub fn enter_offset(self) -> (f32, f32) {
        match self.side() {
            PanelSide::Below => (0.0, -80.0),
            PanelSide::Left => (-80.0, 0.0),
            PanelSide::Right => (80.0, 0.0),
        }
    }

    /// Exit displacement for a panel adjacent to the bar.
    pub fn exit_offset(self) -> (f32, f32) {
        match self.side() {
            PanelSide::Below => (0.0, -8.0),
            PanelSide::Left => (-8.0, 0.0),
            PanelSide::Right => (8.0, 0.0),
        }
    }
}

/// The current setting, as one immutable placement snapshot.
pub fn current() -> BarPlacement {
    BarPlacement::new(settings().with(|s| s.bar_position))
}

/// Follows placement without waking downstream users for unrelated settings changes.
pub fn memoized() -> Memo<BarPlacement> {
    memo(current)
}
