//! The `egui` seam — now the real `egui`.
//!
//! Phase 1/2 modelled the three things the recovered draw code touches
//! (`Vec2`/`Rect`/`Color32`, the painter's primitives and `Ui`'s two queries)
//! because `egui` was not a dependency yet. Phase 4 re-bases the seam on the
//! pinned `egui` (git rev `3da022d`, see `docs/EGUI.md`): the types *are*
//! `egui`'s, and the names the recovered code used are kept as re-exports and
//! aliases so the rest of the crate still reads the same.
//!
//! The layout identity the old seam asserted still holds and is what makes the
//! swap safe — `Color32` is four bytes with alpha last (`to_u32` packs
//! `r | g << 8 | b << 16 | a << 24`, which is how the recovered code builds a
//! colour, e.g. `bar::Bar::look` @`0x10070863c`), `Vec2` is 8 bytes, `Rect` 16,
//! `egui::Event` 72 (proved by `keys::Keys::take`'s `0x48` stride
//! @`0x1003d80dc`). [`tests::the_layout_identity_is_the_recovered_one`] asserts
//! all of that, plus the `egui::Key` table the recovered `names::winit_name`
//! walks.

pub use egui::{Color32, Pos2, Rect, Vec2};

/// A laid-out run of text. The recovered code only ever reads `Galley::size`
/// (`bar::Bar::fitted` @`0x1007094d4` loads the two `f32` at `galley + 0x30`).
pub type Galley = egui::Galley;

/// The painter the recovered draw code hands its primitives to
/// (`egui::Painter`; `bar::stack::Stack::paint` and friends take `&Painter`).
pub type Painter = egui::Painter;

/// The layout context: `egui::Ui`. The recovered queries are
/// `Ui::available_size` (`bar::Bar::fitted` @`0x100709404`) and `Ui::style`'s
/// text styles (`TextStyle::resolve` @`0x100709488`).
pub type Ui = egui::Ui;

/// The packed-`u32` accessors the recovered code uses.
///
/// `egui::Color32` stores `[r, g, b, a]` in memory and has no `to_u32` of its
/// own, but `bar::Bar::look` (@`0x100708610`) forces alpha with
/// `orr w12, w12, #0xff000000` on the packed word and `Look`'s JSON writes the
/// packed word, so the two conversions stay.
pub trait Color32Ext: Copy {
    /// The `u32` a colour transposes to (little-endian `r, g, b, a`).
    fn to_u32(self) -> u32;
    /// The inverse of [`Color32Ext::to_u32`].
    fn from_u32(v: u32) -> Self;
    /// The same colour with a forced `0xff` alpha — the recovered
    /// `orr w12, w12, #0xff000000`.
    fn opaque(self) -> Self;
}

impl Color32Ext for Color32 {
    fn to_u32(self) -> u32 {
        u32::from_le_bytes([self.r(), self.g(), self.b(), self.a()])
    }

    fn from_u32(v: u32) -> Self {
        let [r, g, b, a] = v.to_le_bytes();
        Color32::from_rgba_premultiplied(r, g, b, a)
    }

    fn opaque(self) -> Self {
        Color32::from_rgba_premultiplied(self.r(), self.g(), self.b(), 0xff)
    }
}

/// A `Vec2` as a point (they are layout-identical: two `f32`).
///
/// `egui::Rect::from_min_size` takes a [`Pos2`] while the recovered layout code
/// thinks in [`Vec2`] offsets, so the helpers below bridge at the seam instead
/// of at every call site.
pub fn pos(v: Vec2) -> Pos2 {
    Pos2::new(v.x, v.y)
}

/// A rectangle from a corner and a size — `Rect::from_min_size` with a [`Vec2`]
/// origin.
pub fn rect(min: Vec2, size: Vec2) -> Rect {
    Rect::from_min_size(pos(min), size)
}

/// A rectangle from two corners given as [`Vec2`]s.
pub fn rect_from_min_max(min: Vec2, max: Vec2) -> Rect {
    Rect::from_min_max(pos(min), pos(max))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_identity_is_the_recovered_one() {
        // The ABI facts the crate notes record (`docs/EGUI.md` §5.3).
        assert_eq!(std::mem::size_of::<Color32>(), 4);
        assert_eq!(std::mem::size_of::<Vec2>(), 8);
        assert_eq!(std::mem::size_of::<Pos2>(), 8);
        assert_eq!(std::mem::size_of::<Rect>(), 16);
        // `Keys::take` walks `Vec<egui::Event>` with a 0x48 stride
        // (`add x21, x21, #0x48` @0x1003d80dc).
        assert_eq!(std::mem::size_of::<egui::Event>(), 72);
        // `names::winit_name` indexes the `egui::Key` name table with 117 codes.
        assert_eq!(egui::Key::ALL.len(), 117);
        assert_eq!(egui::Key::H as u8, 0x35);
        assert_eq!(egui::Key::I as u8, 0x36);
        assert_eq!(egui::Key::IntlBackslash as u8, 0x74);
    }

    #[test]
    fn color32_packs_like_the_recovered_code() {
        // The recovered code builds a colour as r | g<<8 | b<<16 | 0xff000000.
        let packed = 0x10u32 | 0x20u32 << 8 | 0x30u32 << 16 | 0xff00_0000;
        let c = <Color32 as Color32Ext>::from_u32(packed);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), (0x10, 0x20, 0x30, 0xff));
        assert_eq!(c.to_u32(), packed);
        assert_eq!(c.opaque(), c);
        let faded = Color32::from_rgba_premultiplied(1, 2, 3, 0x40);
        assert_eq!(faded.opaque(), Color32::from_rgb(1, 2, 3));
    }

    #[test]
    fn rect_geometry() {
        let r = rect(Vec2::new(10.0, 20.0), Vec2::new(30.0, 40.0));
        assert_eq!(r.width(), 30.0);
        assert_eq!(r.height(), 40.0);
        assert_eq!(r.center(), Pos2::new(25.0, 40.0));
        assert!(r.contains(Pos2::new(11.0, 21.0)));
        assert!(!r.contains(Pos2::new(41.0, 21.0)));
        assert_eq!(r.shrink(5.0).size(), Vec2::new(20.0, 30.0));
        assert_eq!(
            rect_from_min_max(Vec2::ZERO, Vec2::splat(4.0)),
            rect(Vec2::ZERO, Vec2::splat(4.0))
        );
    }
}
