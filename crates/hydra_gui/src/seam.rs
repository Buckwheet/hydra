//! The `egui` seam.
//!
//! The original crate was an `eframe`/`egui` application: the recovered code
//! paints through `egui::Painter`, lays text out with
//! `egui::Context::fonts(..).layout_no_wrap(..)` and asks the surrounding `Ui`
//! for `available_size()`.  `egui` is not available in this build, so the three
//! things the recovered functions actually touch are modelled here:
//!
//! * [`Vec2`], [`Rect`] and [`Color32`] — the geometry/colour value types
//!   (`egui::Vec2` is `{x: f32, y: f32}`, `egui::Color32` is four bytes with
//!   alpha last, which is exactly how the recovered code builds colours: it
//!   ORs `0xff000000` into the alpha byte, e.g. `bar::Bar::look` @0x10070863c).
//! * [`Painter`] — every draw call the recovered code makes, as explicit
//!   primitives.  [`RecordingPainter`] records them so tests can assert them.
//! * [`Ui`] — the two queries the recovered layout code performs
//!   (`available_size` and a text measurement).
//!
//! See `NOTES.md` for the departure list.

/// `egui::Vec2`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }

    pub fn splat(v: f32) -> Self {
        Vec2 { x: v, y: v }
    }

    pub fn min(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x.min(other.x), self.y.min(other.y))
    }

    pub fn max(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x.max(other.x), self.y.max(other.y))
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

/// `egui::Rect` — always `min`/`max`, never `x`/`y`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Rect { min, max: min + size }
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(
            self.min.x + self.width() / 2.0,
            self.min.y + self.height() / 2.0,
        )
    }

    pub fn contains(&self, p: Vec2) -> bool {
        self.min.x <= p.x && p.x <= self.max.x && self.min.y <= p.y && p.y <= self.max.y
    }

    /// Grow/shrink on every side; positive `by` shrinks.
    pub fn shrink(&self, by: f32) -> Rect {
        Rect {
            min: Vec2::new(self.min.x + by, self.min.y + by),
            max: Vec2::new(self.max.x - by, self.max.y - by),
        }
    }
}

/// `egui::Color32` — `r`, `g`, `b`, `a` in memory order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color32(pub u8, pub u8, pub u8, pub u8);

impl Color32 {
    pub const TRANSPARENT: Color32 = Color32(0, 0, 0, 0);
    pub const BLACK: Color32 = Color32(0, 0, 0, 255);
    pub const WHITE: Color32 = Color32(255, 255, 255, 255);

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color32(r, g, b, 255)
    }

    pub const fn from_rgba_premultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color32(r, g, b, a)
    }

    /// The `u32` an `egui::Color32` transposes to (little endian `r,g,b,a`).
    pub fn to_u32(self) -> u32 {
        u32::from_le_bytes([self.0, self.1, self.2, self.3])
    }

    pub fn from_u32(v: u32) -> Self {
        let [r, g, b, a] = v.to_le_bytes();
        Color32(r, g, b, a)
    }

    pub fn r(&self) -> u8 {
        self.0
    }
    pub fn g(&self) -> u8 {
        self.1
    }
    pub fn b(&self) -> u8 {
        self.2
    }
    pub fn a(&self) -> u8 {
        self.3
    }
}

/// A laid-out run of text (`egui::Galley`, of which the recovered code only
/// ever reads `size`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Galley {
    pub size: Vec2,
}

/// One primitive handed to a [`Painter`].
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Rect {
        rect: Rect,
        fill: Color32,
        rounding: f32,
    },
    Text {
        pos: Vec2,
        text: String,
        size: f32,
        color: Color32,
    },
    Line {
        from: Vec2,
        to: Vec2,
        width: f32,
        color: Color32,
    },
    Circle {
        center: Vec2,
        radius: f32,
        fill: Color32,
    },
    /// A raster image, as raw RGBA rows (see `badge::pixels`).
    Image {
        rect: Rect,
        size: [u32; 2],
    },
}

/// Everything the recovered code asks a painter to do.
pub trait Painter {
    /// `Context::fonts(..).layout_no_wrap(text, font_id, color).size()`
    /// (`bar::Bar::fitted` @0x1007094cc does exactly this).
    fn layout_no_wrap(&mut self, text: &str, size: f32, color: Color32) -> Galley;
    fn rect_filled(&mut self, rect: Rect, rounding: f32, fill: Color32);
    fn text(&mut self, pos: Vec2, text: &str, size: f32, color: Color32);
    fn line_segment(&mut self, from: Vec2, to: Vec2, width: f32, color: Color32);
    fn circle_filled(&mut self, center: Vec2, radius: f32, fill: Color32);
    fn image(&mut self, rect: Rect, size: [u32; 2]);
}

/// A [`Painter`] that records every primitive and measures text with a fixed
/// monospace metric, so layout tests are deterministic.
#[derive(Clone, Debug, Default)]
pub struct RecordingPainter {
    pub draws: Vec<Draw>,
    /// Advance per character, as a fraction of the font size.
    pub advance: f32,
    /// Line height, as a multiple of the font size.
    pub line_height: f32,
}

impl RecordingPainter {
    pub fn new() -> Self {
        RecordingPainter {
            draws: Vec::new(),
            advance: 0.6,
            line_height: 1.2,
        }
    }

    pub fn rects(&self) -> impl Iterator<Item = (&Rect, &Color32)> {
        self.draws.iter().filter_map(|d| match d {
            Draw::Rect { rect, fill, .. } => Some((rect, fill)),
            _ => None,
        })
    }

    pub fn texts(&self) -> impl Iterator<Item = (&str, Color32)> {
        self.draws.iter().filter_map(|d| match d {
            Draw::Text { text, color, .. } => Some((text.as_str(), *color)),
            _ => None,
        })
    }
}

impl Painter for RecordingPainter {
    fn layout_no_wrap(&mut self, text: &str, size: f32, _color: Color32) -> Galley {
        let chars = text.chars().count() as f32;
        Galley {
            size: Vec2::new(chars * self.advance * size, self.line_height * size),
        }
    }

    fn rect_filled(&mut self, rect: Rect, rounding: f32, fill: Color32) {
        self.draws.push(Draw::Rect { rect, fill, rounding });
    }

    fn text(&mut self, pos: Vec2, text: &str, size: f32, color: Color32) {
        self.draws.push(Draw::Text {
            pos,
            text: text.to_string(),
            size,
            color,
        });
    }

    fn line_segment(&mut self, from: Vec2, to: Vec2, width: f32, color: Color32) {
        self.draws.push(Draw::Line {
            from,
            to,
            width,
            color,
        });
    }

    fn circle_filled(&mut self, center: Vec2, radius: f32, fill: Color32) {
        self.draws.push(Draw::Circle {
            center,
            radius,
            fill,
        });
    }

    fn image(&mut self, rect: Rect, size: [u32; 2]) {
        self.draws.push(Draw::Image { rect, size });
    }
}

/// The parts of `egui::Ui` the recovered code uses.
pub trait Ui {
    /// `Ui::available_size()` (`bar::Bar::fitted` @0x100709404).
    fn available_size(&self) -> Vec2;
    /// The painter for this frame.
    fn painter(&mut self) -> &mut dyn Painter;
    /// Body text size, i.e. what `TextStyle::Body.resolve(..)` yields
    /// (`bar::Bar::fitted` @0x100709488).
    fn text_size(&self) -> f32;
}

/// A minimal [`Ui`] over a [`RecordingPainter`] with a fixed frame size.
pub struct TestUi {
    pub available: Vec2,
    pub text_size: f32,
    pub painter: RecordingPainter,
}

impl TestUi {
    pub fn new(available: Vec2) -> Self {
        TestUi {
            available,
            text_size: 14.0,
            painter: RecordingPainter::new(),
        }
    }
}

impl Default for TestUi {
    fn default() -> Self {
        TestUi::new(Vec2::new(400.0, 300.0))
    }
}

impl Ui for TestUi {
    fn available_size(&self) -> Vec2 {
        self.available
    }

    fn painter(&mut self) -> &mut dyn Painter {
        &mut self.painter
    }

    fn text_size(&self) -> f32 {
        self.text_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color32_packs_like_egui() {
        // The recovered code builds a colour as r | g<<8 | b<<16 | 0xff000000.
        let packed = 0x10u32 | 0x20u32 << 8 | 0x30u32 << 16 | 0xff00_0000;
        let c = Color32::from_u32(packed);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), (0x10, 0x20, 0x30, 0xff));
        assert_eq!(c.to_u32(), packed);
    }

    #[test]
    fn rect_geometry() {
        let r = Rect::from_min_size(Vec2::new(10.0, 20.0), Vec2::new(30.0, 40.0));
        assert_eq!(r.width(), 30.0);
        assert_eq!(r.height(), 40.0);
        assert_eq!(r.center(), Vec2::new(25.0, 40.0));
        assert!(r.contains(Vec2::new(11.0, 21.0)));
        assert!(!r.contains(Vec2::new(41.0, 21.0)));
        assert_eq!(r.shrink(5.0).size(), Vec2::new(20.0, 30.0));
    }

    #[test]
    fn recording_painter_measures_and_records() {
        let mut p = RecordingPainter::new();
        let g = p.layout_no_wrap("north", 10.0, Color32::WHITE);
        assert_eq!(g.size, Vec2::new(5.0 * 6.0, 12.0));
        p.rect_filled(
            Rect::from_min_size(Vec2::ZERO, Vec2::splat(4.0)),
            0.0,
            Color32::WHITE,
        );
        p.text(Vec2::ZERO, "hi", 10.0, Color32::BLACK);
        assert_eq!(p.rects().count(), 1);
        assert_eq!(p.texts().next(), Some(("hi", Color32::BLACK)));
    }
}
