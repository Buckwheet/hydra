//! The `epaint` bridge: the recovered data as `egui` colours and fonts.
//!
//! Phase 4 put drawing back into `hydra_ui`.  Nothing here invents data:
//!
//! * colours come from [`crate::theme::Token::bare`] / [`Palette::get`], whose
//!   packed word is `r | g << 8 | b << 16` (see [`crate::theme`]) — [`color`]
//!   unpacks it into an `egui::Color32` and [`packed`] packs it back;
//! * fonts come from the theme's `[type]` section (`ui_font ui_size story_font
//!   story_size mono_font mono_size line_spacing fallbacks`,
//!   `theme::typeface` @0x103993400), and fall back to egui's own resolved
//!   `TextStyle` when the theme sets nothing;
//! * the prompt accents come from `Palette::prompt` @0x101847cb4.
//!
//! `egui` is only reached through `Ui`, `Painter`, `Color32`, `FontId` and
//! `TextFormat`, which is the dependency surface docs/EGUI.md §3.6 asks for.

use egui::text::TextFormat;
use egui::{Color32, FontFamily, FontId, TextStyle, Ui};

use crate::theme::file::Theme;
use crate::theme::typeface::{Face, Type, TypeFile};
use crate::theme::{blue, green, red, Palette, Rgb, Token};

/// The packed token colour (`r | g << 8 | b << 16`) as an [`egui::Color32`].
///
/// The packing is the one `Token::bare` @0x101842dd0 writes and `oklch::contrast`
/// reads; the alpha byte is implicit (`0xff`), which is why `Color32` is the
/// natural host type for it.
pub fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(red(rgb), green(rgb), blue(rgb))
}

/// The inverse of [`color`]: an [`egui::Color32`] as the crate's packed [`Rgb`].
pub fn packed(c: Color32) -> Rgb {
    crate::theme::rgb(c.r(), c.g(), c.b())
}

/// A token's colour in a palette, ready to paint.
pub fn ink(palette: &Palette, token: Token) -> Color32 {
    color(palette.get(token))
}

/// `Palette::prompt` @0x101847cb4 — the accent token registered for `at`
/// (`palette.prompt()` answers `2` when the map has no entry, which is
/// [`Token::Raised`]).
pub fn accent(palette: &Palette, at: u32) -> (Token, Color32) {
    let token = Token::from_u8(palette.prompt(at)).unwrap_or(Token::Text);
    (token, ink(palette, token))
}

/// A [`TextFormat`] painting `text`-sized glyphs in `token`'s colour.
pub fn format(palette: &Palette, token: Token, font: FontId) -> TextFormat {
    TextFormat {
        font_id: font,
        color: ink(palette, token),
        ..Default::default()
    }
}

/// The font families the live context actually bound.
///
/// A [`FontFamily::Name`] that is not bound panics inside `epaint`
/// (`"FontFamily::… is not bound to any fonts"`), so a theme that names a font
/// the app never installed has to be checked against this list first.
pub fn families(ui: &Ui) -> Vec<FontFamily> {
    ui.ctx().fonts(|f| f.families())
}

/// The family a theme's font name selects.
///
/// `proportional` and `monospace` are egui's two built-in families and are
/// always bound; any other name is a [`FontFamily::Name`] and is only usable when
/// the app registered it.
fn family(name: &str, bound: &[FontFamily]) -> Option<FontFamily> {
    if name.eq_ignore_ascii_case("proportional") {
        return Some(FontFamily::Proportional);
    }
    if name.eq_ignore_ascii_case("monospace") {
        return Some(FontFamily::Monospace);
    }
    let named = FontFamily::Name(name.into());
    bound.contains(&named).then_some(named)
}

/// A themed [`Face`] as an [`egui::FontId`].
///
/// The recovered `Face` is `{font: Option<String>, weight: u32, size: f32}` with
/// `0.0` meaning "unset" (`Face::under` @0x101951438 branches on the size word).
/// A named family is used only when the context bound it; otherwise the fallback
/// family is kept and only the size is applied.
///
/// `// UNKNOWN:` `Face::weight` has no counterpart in `egui::FontId` (egui
/// selects weight through `FontTweak`/the font data at load time), so it is
/// ignored here rather than invented into a stretch or a second family.
pub fn face(face: &Face, fallback: &FontId, bound: &[FontFamily]) -> FontId {
    let family = match face.font.as_deref() {
        Some(name) if !name.is_empty() => family(name, bound).unwrap_or_else(|| fallback.family.clone()),
        _ => fallback.family.clone(),
    };
    let size = if face.size > 0.0 {
        face.size
    } else {
        fallback.size
    };
    FontId::new(size, family)
}

/// A theme's `[type]` section resolved against an empty base layer.
///
/// `TypeFile::resolve` is the recovered layering (`Face::under` @0x101951438);
/// resolving over the empty file just applies `font`/`size` to each of the
/// three faces.
pub fn faces(theme: &Theme) -> Type {
    theme.fonts.resolve(&TypeFile::default())
}

/// The UI face of a theme, falling back to egui's `TextStyle::Body`.
pub fn ui_font(theme: &Theme, ui: &Ui) -> FontId {
    face(
        &faces(theme).ui,
        &TextStyle::Body.resolve(ui.style()),
        &families(ui),
    )
}

/// The story face of a theme, falling back to egui's `TextStyle::Body`.
pub fn story_font(theme: &Theme, ui: &Ui) -> FontId {
    face(
        &faces(theme).story,
        &TextStyle::Body.resolve(ui.style()),
        &families(ui),
    )
}

/// The `[type] line_spacing` of a theme (`0.0` when unset).
pub fn line_spacing(theme: &Theme) -> f32 {
    faces(theme).line_spacing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::TABLE;

    #[test]
    fn packed_colours_round_trip() {
        // The three first tokens of the recovered table, byte for byte.
        assert_eq!(color(0x001b1b1b), Color32::from_rgb(0x1b, 0x1b, 0x1b));
        assert_eq!(color(Token::Health.bare()), Color32::from_rgb(0xcd, 0x4d, 0x4d));
        assert_eq!(color(Token::Mana.bare()), Color32::from_rgb(0x47, 0x84, 0xd9));
        for row in TABLE {
            assert_eq!(packed(color(row.3)), row.3);
        }
    }

    #[test]
    fn ink_is_the_palette_colour_of_the_token() {
        let p = Palette::bare();
        for token in Token::ALL {
            assert_eq!(ink(&p, token), color(token.bare()));
        }
    }

    #[test]
    fn prompt_accents_come_from_the_palette() {
        let p = Palette::bare();
        // The four recovered accents: 82 -> 15, 33 -> 15, 83 -> 14, 72 -> 12.
        assert_eq!(accent(&p, 82), (Token::Wrong, color(Token::Wrong.bare())));
        assert_eq!(accent(&p, 83), (Token::Warning, color(Token::Warning.bare())));
        assert_eq!(accent(&p, 72), (Token::Link, color(Token::Link.bare())));
        // An unregistered prompt falls back to the binary's `mov w0, #2`.
        assert_eq!(accent(&p, 0).0, Token::Raised);
    }

    #[test]
    fn a_face_uses_the_themed_size_and_only_bound_families() {
        let fallback = FontId::new(12.0, FontFamily::Proportional);
        let bound = vec![FontFamily::Proportional, FontFamily::Monospace];
        // Size only.
        let f = face(
            &Face {
                font: None,
                weight: 0,
                size: 18.0,
            },
            &fallback,
            &bound,
        );
        assert_eq!(f, FontId::new(18.0, FontFamily::Proportional));
        // An unbound family keeps the fallback family but takes the size.
        let f = face(
            &Face {
                font: Some("PragmataPro".into()),
                weight: 0,
                size: 20.0,
            },
            &fallback,
            &bound,
        );
        assert_eq!(f, FontId::new(20.0, FontFamily::Proportional));
        // A bound family is used.
        let f = face(
            &Face {
                font: Some("Monospace".into()),
                weight: 0,
                size: 0.0,
            },
            &fallback,
            &bound,
        );
        assert_eq!(f, FontId::new(12.0, FontFamily::Monospace));
        // A family the app did register is used.
        let custom = FontFamily::Name("Pragmata".into());
        let f = face(
            &Face {
                font: Some("Pragmata".into()),
                weight: 0,
                size: 16.0,
            },
            &fallback,
            &[FontFamily::Proportional, custom.clone()],
        );
        assert_eq!(f, FontId::new(16.0, custom));
        // Nothing set at all is the fallback.
        assert_eq!(face(&Face::default(), &fallback, &bound), fallback);
    }

    #[test]
    fn formats_carry_the_token_colour() {
        let p = Palette::bare();
        let f = format(&p, Token::Warning, FontId::proportional(14.0));
        assert_eq!(f.color, Color32::from_rgb(0xd7, 0xad, 0x63));
        assert_eq!(f.font_id, FontId::proportional(14.0));
    }
}
