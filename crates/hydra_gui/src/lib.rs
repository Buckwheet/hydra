//! Hydra's `egui` front end, reconstructed from the arm64 binary.
//!
//! Phase 4 reconnects the real stack: the drawing goes through `egui` at the
//! rev the binary was built with (git `3da022d`, see `docs/EGUI.md`), the
//! window is an `eframe` window ([`app::build`]/[`app::run`]), and [`app::Hub`]
//! paints the recovered chrome — the title bar items, the play area and the
//! four drawers. `serde`/`serde_json` are still replaced by the hand-written
//! [`json`] reader/writer and `image` by a pre-decoded badge asset (see
//! `NOTES.md`, which also records the evidence — unslid addresses — for each
//! recovered function).
//!
//! Original crate layout recovered from the symbol table: `app`, `badge`,
//! `bar`, `card`, `chat`, `compass_art`, `doll_art`, `hub`, `keys`, `layout`,
//! `logs`, `main_menu`, `menu`, `pad`, `placement`, `play`, `reports`,
//! `theme_editor`, `titlebar_page`, `tracked` and `widget`. Only the modules
//! that carried a stub (plus their evidenced types) are reconstructed here.

pub mod app;
pub mod badge;
pub mod bar;
pub mod chat;
pub mod json;
pub mod keys;
pub mod layout;
pub mod play;
pub mod reports;
pub mod seam;
pub mod theme_editor;
pub mod widget;

/// Headless frame plumbing for the tests.
///
/// Nothing here opens a window: every test builds an [`egui::Context`], feeds
/// it one [`egui::RawInput`] with a fixed `screen_rect` and runs a single frame
/// with `Context::run_ui`. The shapes that frame produced are then asserted on
/// (rect fills, circles, text galleys), never just "it did not panic".
#[cfg(test)]
pub mod headless {
    use egui::{Color32, Context, FullOutput, Pos2, RawInput, Rect, Shape, Ui, Vec2};
    use epaint::{CircleShape, RectShape, TextShape};

    /// The window the recovered `app::run` asks for: 660 × 520
    /// (`0x44250000`/`0x44020000` packed into `x3` @`0x100512fe8`).
    pub const SIZE: Vec2 = Vec2::new(660.0, 520.0);

    /// A [`RawInput`] with a fixed screen rectangle and no time/events.
    pub fn input(size: Vec2) -> RawInput {
        let mut input = RawInput::default();
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, size));
        input
    }

    /// Run one headless frame of `f` and return its value plus the frame output.
    pub fn frame<R>(ctx: &Context, size: Vec2, f: impl FnOnce(&mut Ui) -> R) -> (R, FullOutput) {
        let mut value = None;
        let mut f = Some(f);
        let output = ctx.run_ui(input(size), |ui| {
            let f = f.take().expect("the frame body runs once");
            value = Some(f(ui));
        });
        (value.expect("the frame body ran"), output)
    }

    /// One frame of `f` in a fresh [`Context`] sized [`SIZE`].
    pub fn one_frame<R>(f: impl FnOnce(&mut Ui) -> R) -> (R, FullOutput) {
        let ctx = Context::default();
        frame(&ctx, SIZE, f)
    }

    /// Every shape of the frame, with `Shape::Vec` flattened.
    pub fn shapes(output: &FullOutput) -> Vec<Shape> {
        fn walk(shape: &Shape, out: &mut Vec<Shape>) {
            match shape {
                Shape::Vec(shapes) => {
                    for shape in shapes {
                        walk(shape, out);
                    }
                }
                other => out.push(other.clone()),
            }
        }
        let mut out = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    /// The filled rectangles of the frame, in paint order, with `ff`-clamped
    /// fill colours.
    pub fn rect_fills(output: &FullOutput) -> Vec<RectShape> {
        shapes(output)
            .iter()
            .filter_map(|shape| match shape {
                Shape::Rect(rect) if rect.fill.a() > 0 => Some(rect.clone()),
                _ => None,
            })
            .collect()
    }

    /// The rect fills drawn with `fill`, in paint order.
    pub fn rects_of(output: &FullOutput, fill: Color32) -> Vec<Rect> {
        rect_fills(output)
            .iter()
            .filter(|rect| rect.fill == fill)
            .map(|rect| rect.rect)
            .collect()
    }

    /// The circles of the frame, in paint order.
    pub fn circles(output: &FullOutput) -> Vec<CircleShape> {
        shapes(output)
            .iter()
            .filter_map(|shape| match shape {
                Shape::Circle(circle) => Some(circle.clone()),
                _ => None,
            })
            .collect()
    }

    /// The strings of every text shape in the frame, in paint order.
    pub fn texts(output: &FullOutput) -> Vec<String> {
        shapes(output)
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) => Some(text.clone()),
                _ => None,
            })
            .map(|text: TextShape| text.galley.text().to_string())
            .collect()
    }

    /// The text shapes of the frame, in paint order.
    pub fn text_shapes(output: &FullOutput) -> Vec<TextShape> {
        shapes(output)
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}
