//! The two pieces of painting every window in this crate needs: a `#rrggbb`
//! colour, and how much of a string fits in the width a [`egui::Ui`] has left.
//!
//! **ESTABLISHED**: every colour this crate draws with arrives as a string out
//! of a settings file (`chips.toml`, `targets.toml`, `controller_wheels`,
//! `banner.toml`), and the shape of the parse is the one
//! [`crate::banner::parse_hex`] recovered from `hydra_ui::theme::parse_hex`
//! (`#rrggbb`, `#rrggbbaa`, `#rgb`, with or without the `#`).
//!
//! **UNKNOWN**: which glyph the binary measured when it decided how much of a
//! target's name fits — [`fit_chars`] measures `x`, the usual stand-in for an
//! average character.

use egui::{Color32, FontId, Ui};

/// A colour out of a settings file, or `None` when the file holds nonsense.
///
/// Same three spellings as [`crate::banner::parse_hex`], which is the recovered
/// `hydra_ui::theme::parse_hex` shape.
pub fn color(hex: &str) -> Option<Color32> {
    let [r, g, b, a] = crate::banner::parse_hex(hex)?;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some(Color32::from_rgba_unmultiplied(
        byte(r),
        byte(g),
        byte(b),
        byte(a),
    ))
}

/// [`color`], or `fallback` when the string is not a colour.
///
/// The binary reaches the same fallback through `Color32::from_hex` returning
/// `None`; the caller then keeps the colour it already had.
pub fn color_or(hex: &str, fallback: Color32) -> Color32 {
    color(hex).unwrap_or(fallback)
}

/// How many characters of `font` fit in `width`.
///
/// This is the width [`crate::targets::NameShown::Fit`] is measured against:
/// the recovered `Target::named` calls `egui::Ui::available_width` (see
/// `docs/EGUI.md` §3.5) and hands the count to [`crate::targets::Target::shown`].
pub fn fit_chars(ui: &Ui, font: &FontId, width: f32) -> usize {
    let wide = ui.fonts_mut(|fonts| fonts.glyph_width(font, 'x'));
    if !(wide.is_finite() && wide > 0.0) {
        return 0;
    }
    (width / wide).floor().max(0.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_like_the_theme_does() {
        assert_eq!(color("#ff0000"), Some(Color32::from_rgb(255, 0, 0)));
        assert_eq!(color("00ff00"), Some(Color32::from_rgb(0, 255, 0)));
        assert_eq!(color("#00f"), Some(Color32::from_rgb(0, 0, 255)));
        assert_eq!(
            color("#00000080"),
            Some(Color32::from_rgba_unmultiplied(0, 0, 0, 128))
        );
        assert_eq!(color("nope"), None);
        let gray = Color32::from_rgb(1, 2, 3);
        assert_eq!(color_or("nope", gray), gray);
        assert_eq!(color_or("#b03030", gray), Color32::from_rgb(0xb0, 0x30, 0x30));
    }

    #[test]
    fn a_thumb_measures_the_width_it_is_given() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 100.0),
        ));
        let out = ctx.run_ui(input, |ui| {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let wide = ui.fonts_mut(|fonts| fonts.glyph_width(&font, 'x'));
            // The count is the width divided by one glyph, floored.
            assert_eq!(fit_chars(ui, &font, 100.0), (100.0 / wide).floor() as usize);
            // Wider windows fit more; a zero width fits nothing.
            assert!(fit_chars(ui, &font, 200.0) > fit_chars(ui, &font, 100.0));
            assert_eq!(fit_chars(ui, &font, 0.0), 0);
        });
        assert!(out.shapes.is_empty(), "{:?}", out.shapes.len());
    }
}
