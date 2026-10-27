//! One fixed headless frame, and readers for what it painted.
//!
//! Test builds only: every draw test in this crate builds an [`egui::Context`],
//! runs exactly one frame over a fixed [`egui::RawInput`], and then asserts on
//! the values in the resulting shapes (rect coordinates, colours, polygon
//! points, galley text) — never merely that a frame did not panic.

use egui::epaint::{PathShape, RectShape, Shape, TextShape};
use egui::{Color32, Context, FullOutput, Pos2, RawInput, Rect};

/// The window the draw tests use unless they say otherwise.
pub const SCREEN: egui::Vec2 = egui::vec2(800.0, 600.0);

/// A fixed input covering `screen`.
pub fn input(screen: egui::Vec2) -> RawInput {
    let mut input = RawInput::default();
    input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, screen));
    input
}

/// One frame over [`SCREEN`].
pub fn frame(run: impl FnMut(&mut egui::Ui)) -> (Context, FullOutput) {
    frame_of(SCREEN, run)
}

/// One frame over a screen of the given size.
pub fn frame_of(screen: egui::Vec2, run: impl FnMut(&mut egui::Ui)) -> (Context, FullOutput) {
    let ctx = Context::default();
    let out = ctx.run_ui(input(screen), run);
    (ctx, out)
}

/// Every shape the frame painted, with nested [`Shape::Vec`]s flattened out.
pub fn shapes(out: &FullOutput) -> Vec<&Shape> {
    fn walk<'a>(shape: &'a Shape, into: &mut Vec<&'a Shape>) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, into);
                }
            }
            other => into.push(other),
        }
    }
    let mut into = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, &mut into);
    }
    into
}

/// The text of every text shape, in paint order.
pub fn words(out: &FullOutput) -> Vec<String> {
    text_shapes(out)
        .into_iter()
        .map(|shape| shape.galley.text().to_string())
        .collect()
}

/// Every text shape.
pub fn text_shapes(out: &FullOutput) -> Vec<&TextShape> {
    shapes(out)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// `(text, colour)` for every text shape, in paint order.
pub fn text_colours(out: &FullOutput) -> Vec<(String, Color32)> {
    text_shapes(out)
        .into_iter()
        .map(|shape| {
            (
                shape.galley.text().to_string(),
                text_colour(shape),
            )
        })
        .collect()
}

/// The colour a text shape's glyphs are painted in.
pub fn text_colour(shape: &TextShape) -> Color32 {
    shape
        .galley
        .job
        .sections
        .first()
        .map(|section| section.format.color)
        .unwrap_or(shape.fallback_color)
}

/// Every rectangle, as `(rect, fill)`.
pub fn rect_fills(out: &FullOutput) -> Vec<(Rect, Color32)> {
    rect_shapes(out)
        .into_iter()
        .map(|shape| (shape.rect, shape.fill))
        .collect()
}

/// Every rectangle shape.
pub fn rect_shapes(out: &FullOutput) -> Vec<&RectShape> {
    shapes(out)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Rect(rect) => Some(rect),
            _ => None,
        })
        .collect()
}

/// Every filled polygon, as `(points, fill)`.
pub fn polygons(out: &FullOutput) -> Vec<(Vec<Pos2>, Color32)> {
    path_shapes(out)
        .into_iter()
        .map(|shape| (shape.points.clone(), shape.fill))
        .collect()
}

/// Every path shape.
pub fn path_shapes(out: &FullOutput) -> Vec<&PathShape> {
    shapes(out)
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            _ => None,
        })
        .collect()
}

/// The angle of `point` around `centre`, in degrees, the way
/// [`crate::pad::wheel::seat_at_angle`] measures it (`atan2f(y - y0, x - x0)`).
pub fn angle_of(centre: Pos2, point: Pos2) -> f32 {
    (point.y - centre.y).atan2(point.x - centre.x).to_degrees()
}

/// Fold an angle into `[0, 360)`.
pub fn folded(degrees: f32) -> f32 {
    let mut angle = degrees % 360.0;
    if angle < 0.0 {
        angle += 360.0;
    }
    angle
}
