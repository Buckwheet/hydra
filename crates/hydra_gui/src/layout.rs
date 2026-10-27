//! Window layout: the drawers and the types the layout file is made of.
//!
//! Only `layout::drawers::Drawers::is_default` was stubbed; the surrounding
//! types (`Holder`, `Layout`, `Placed`) are named by the unresolved-symbol list
//! in the original `lib.rs` (their `Deserialize` visitors at `0x1003c6d34`,
//! `0x1003c6d48` and `0x1003c6d5c`). The originals are large — `Layout::fitted`
//! is 1 728 bytes (`0x1003e3018`), `Serialize for Layout` is 14 856
//! (`0x1003dbbfc`) — and they are **not** reconstructed here beyond what the
//! drawers need; see `NOTES.md`.

pub mod drawers;

use crate::seam::Rect;

/// A widget slot in a window: which widget, and where it sits.
///
/// `layout::Placed` — the name comes from the unresolved-symbol list
/// (`Deserialize for Placed` @`0x1003c6d5c`). Only the shape [`Holder`] needs is
/// kept here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placed {
    pub widget: String,
    pub tab: Option<usize>,
}

/// One window's worth of placed widgets.
///
/// `layout::Holder` (`Deserialize for Holder` @`0x1003c6d34`, and
/// `layout::custom::<impl Holds>::placed` @`0x1003e5070`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Holder {
    pub window: String,
    pub placed: Vec<Placed>,
}

/// A whole window layout.
///
/// `layout::Layout` (`Deserialize for Layout` @`0x1003c6d48`). Its `fitted`
/// (`0x1003e3018`) and `fit_to` (`0x1003e2f10`) shrink windows into the space
/// available; that maths is not reconstructed. What *is* recovered is the drawer
/// geometry, which [`drawers::Zones`] reproduces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub name: String,
    pub holders: Vec<Holder>,
    pub drawers: drawers::Drawers,
}

impl Layout {
    /// The area the main windows get once the drawers have taken their room.
    pub fn main_rect(&self, window: crate::seam::Vec2) -> Rect {
        drawers::Zones::of(&self.drawers, window).main_rect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_layout_with_shut_drawers_hands_the_whole_window_over() {
        let layout = Layout {
            name: "default".into(),
            ..Layout::default()
        };
        let rect = layout.main_rect(crate::seam::Vec2::new(800.0, 600.0));
        assert_eq!(rect.min, crate::seam::Vec2::new(0.0, 0.0));
        assert_eq!(rect.size(), crate::seam::Vec2::new(800.0, 600.0));
    }

    #[test]
    fn an_open_top_drawer_takes_its_size_off_the_height() {
        let mut layout = Layout::default();
        layout.drawers.top.open = true;
        let rect = layout.main_rect(crate::seam::Vec2::new(800.0, 600.0));
        assert_eq!(rect.min, crate::seam::Vec2::new(0.0, 160.0));
        assert_eq!(rect.size(), crate::seam::Vec2::new(800.0, 440.0));
    }
}
