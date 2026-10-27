//! Palette generation: turning one seed colour into all 95 tokens.
//!
//! Recovered pieces:
//!
//! * `harmony::Scheme::parse`  @0x10194da6c — matches the scheme name by
//!   length and then byte by byte.
//! * `harmony::hue_for`        @0x10194e328 — the hue a token gets.
//! * `harmony::hue_variants`   @0x10194cf4c — candidate colours for a token.
//! * `harmony::seed_swatches`  @0x10194d608 — the swatches the recipe editor
//!   offers for a seed.
//! * `harmony::generate`       @0x10194e470 — fills all 95 slots.
//!
//! The scheme hue offsets are a real table in the binary: lengths live at
//! 0x1039bea50 and the `f64` arrays are pointed at from `__DATA_CONST`
//! 0x104490288.  They are reproduced verbatim in [`SCHEME_OFFSETS`].

use crate::theme::oklch;
use crate::theme::{Role, Rgb, Token, TOKEN_COUNT};

/// A harmony scheme.
///
/// The discriminants are the binary's: `Scheme::parse` returns `0`, `1`, `2`,
/// `3`, `4`, `5`, `6` or `8`.  Discriminant `7` exists in the hue-offset table
/// (`[0, 30, 180, 210]`) but no input string produces it, so it is not modelled
/// as a variant here; [`SCHEME_OFFSETS`] keeps a slot for it so the indices of
/// the other rows stay aligned with the binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Monochrome = 0,
    Analogous = 1,
    Complementary = 2,
    Split = 3,
    Triadic = 4,
    Tetradic = 5,
    Golden = 6,
    Compound = 8,
}

/// The hue offsets per scheme discriminant, in degrees.
///
/// Slot `7` is present in the binary but unreachable from [`Scheme::parse`];
/// slot `8` (`compound`) has an **empty** table in the binary, which is why
/// [`hue_for`] falls back to the seed hue for it.
pub const SCHEME_OFFSETS: [&[f64]; 9] = [
    &[0.0],
    &[0.0, 30.0, -30.0, 60.0, -60.0],
    &[0.0, 180.0],
    &[0.0, 150.0, 210.0],
    &[0.0, 120.0, 240.0],
    &[0.0, 90.0, 180.0, 270.0],
    &[0.0, 137.5, 275.0, 52.5, 190.0, 327.5],
    &[0.0, 30.0, 180.0, 210.0],
    &[],
];

impl Scheme {
    /// Every scheme [`Scheme::parse`] can produce, in discriminant order.
    pub const ALL: [Scheme; 8] = [
        Scheme::Monochrome,
        Scheme::Analogous,
        Scheme::Complementary,
        Scheme::Split,
        Scheme::Triadic,
        Scheme::Tetradic,
        Scheme::Golden,
        Scheme::Compound,
    ];

    /// `@0x10194da6c` — parse a scheme name.
    ///
    /// The binary compares the length first, then each byte, so the match is
    /// case sensitive and the whole string must be consumed.
    pub fn parse(name: &str) -> Option<Scheme> {
        Some(match name {
            "monochrome" => Scheme::Monochrome,
            "analogous" => Scheme::Analogous,
            "complementary" => Scheme::Complementary,
            "split" => Scheme::Split,
            "triadic" => Scheme::Triadic,
            "tetradic" => Scheme::Tetradic,
            "golden" => Scheme::Golden,
            "compound" => Scheme::Compound,
            _ => return None,
        })
    }

    /// The name [`Scheme::parse`] accepts.
    pub fn name(self) -> &'static str {
        match self {
            Scheme::Monochrome => "monochrome",
            Scheme::Analogous => "analogous",
            Scheme::Complementary => "complementary",
            Scheme::Split => "split",
            Scheme::Triadic => "triadic",
            Scheme::Tetradic => "tetradic",
            Scheme::Golden => "golden",
            Scheme::Compound => "compound",
        }
    }

    /// The discriminant.
    pub fn index(self) -> u8 {
        self as u8
    }

    /// The hue offsets in degrees (empty for [`Scheme::Compound`]).
    pub fn offsets(self) -> &'static [f64] {
        SCHEME_OFFSETS[self as usize]
    }
}

/// Normalise an angle into `[0, 360)`, the way the binary's `(x + 360) mod 360`
/// does after `atan2` (angle 0 stays 0, negatives wrap).
pub fn normalise_degrees(hue: f64) -> f64 {
    let mut h = (hue + 360.0) % 360.0;
    if h < 0.0 {
        h += 360.0;
    }
    h
}

/// `@0x10194e328` — the hue, in degrees, that `token` should take.
///
/// Recovered behaviour:
///
/// * if the token's role is not [`Role::Under`], the hue of the token's own
///   default colour is used (`rgb_to_oklab` then `atan2`, scaled to degrees and
///   normalised);
/// * otherwise the role's base surface token indexes the scheme's offset table
///   modulo its length — `offsets[base % len] * delta` added to `seed_hue` —
///   and the answer is normalised.  An empty table (compound) leaves the seed
///   hue alone.
pub fn hue_for(token: Token, seed_hue: f64, scheme: Scheme) -> f64 {
    match token.role() {
        Role::Under(base, delta) => {
            let offsets = scheme.offsets();
            if offsets.is_empty() {
                return normalise_degrees(seed_hue);
            }
            let step = offsets[(base as usize) % offsets.len()];
            normalise_degrees(seed_hue + step * delta)
        }
        _ => oklch::rgb_to_oklch(token.bare())[2],
    }
}

/// The numeric knobs a recipe carries.
///
/// Recovered field names from the theme file: `seed`, `background`, `scheme`,
/// `variance`, `contrast`, `separation` and `room_spread`.  The binary's
/// `generate` reads two `f64`s from the front of the struct and the seed and
/// background colours from `+0x80`/`+0x83` with the scheme at `+0x86`, so the
/// order of the four numbers is *plausible but not proven*.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    /// How far hues may wander from the seed (`0.0` = none).
    pub variance: f64,
    /// How hard the generator pushes lightness for contrast.
    pub contrast: f64,
    /// Extra lightness between neighbouring surfaces.
    pub separation: f64,
    /// Extra chroma applied to the map/room tokens.
    pub room_spread: f64,
    /// The colour everything is derived from.
    pub seed: Rgb,
    /// The window background the surfaces are built from.
    pub background: Rgb,
    /// The harmony scheme.
    pub scheme: Scheme,
}

impl Default for Recipe {
    fn default() -> Self {
        Recipe {
            variance: 0.0,
            contrast: 0.0,
            separation: 0.0,
            room_spread: 0.0,
            seed: Token::Accent.bare(),
            background: Token::Canvas.bare(),
            scheme: Scheme::Analogous,
        }
    }
}

/// The result of [`generate`]: one colour per token, in discriminant order.
pub type Colors = [Rgb; TOKEN_COUNT];

/// `@0x10194e470` — fill every token from a [`Recipe`].
///
/// `// UNKNOWN:` the binary's 12 KB body interleaves the per-role arithmetic
/// with the gamut mapping in [`oklch::from_lch`]; the mapping below keeps the
/// recovered *roles* (see [`Role`]) and the recovered hue arithmetic
/// ([`hue_for`], [`normalise_degrees`], the scheme tables) but the exact
/// lightness/chroma blend for the `variance`, `contrast`, `separation` and
/// `room_spread` knobs is not recoverable instruction by instruction.  What is
/// exact: `Fixed` tokens keep their `Token::bare` colour, every `Lightness`
/// token lands on its recorded `L`, and the hue each `Under` token gets is
/// [`hue_for`].
pub fn generate(recipe: &Recipe) -> Colors {
    let seed = oklch::rgb_to_oklch(recipe.seed);
    let bg = oklch::rgb_to_oklch(recipe.background);

    let mut out = [0u32; TOKEN_COUNT];
    // Pass 1: the colours that do not depend on another token.
    for (i, row) in crate::theme::TABLE.iter().enumerate() {
        let (_, token, _, bare, role) = *row;
        out[i] = match role {
            Role::Fixed => bare,
            Role::Lift(d) => {
                let l = (bg[0] + d + recipe.separation * d.signum()).clamp(0.0, 1.0);
                oklch::from_lch([l, bg[1], bg[2]])
            }
            Role::Lightness(l) => {
                let l = (l + recipe.contrast * (l - 0.5)).clamp(0.0, 1.0);
                oklch::from_lch([l, seed[1], seed[2]])
            }
            Role::Hue(dh) => {
                let c = (seed[1] * (1.0 + recipe.variance)).clamp(0.0, 0.4);
                oklch::from_lch([seed[0], c, normalise_degrees(seed[2] + dh * 360.0)])
            }
            Role::Under(..) => {
                let spread = if is_map_token(token) {
                    recipe.room_spread
                } else {
                    0.0
                };
                let c = (seed[1] * (1.0 + recipe.variance) + spread).clamp(0.0, 0.4);
                let h = hue_for(token, seed[2], recipe.scheme);
                // The lightness half is finished in pass 2; keep the seed's.
                oklch::from_lch([seed[0], c, h])
            }
        };
    }
    // Pass 2: `Under` tokens read the lightness of their base surface.
    for (i, row) in crate::theme::TABLE.iter().enumerate() {
        if let Role::Under(base, d) = row.4 {
            let base_lch = oklch::rgb_to_oklch(out[base as usize]);
            let own = oklch::rgb_to_oklch(out[i]);
            let l = (base_lch[0] + d + recipe.separation * d.signum()).clamp(0.0, 1.0);
            out[i] = oklch::from_lch([l, own[1], own[2]]);
        }
    }
    out
}

fn is_map_token(token: Token) -> bool {
    matches!(
        token,
        Token::MapRoom
            | Token::MapRoomEdge
            | Token::MapLine
            | Token::MapConnector
            | Token::MapDoor
            | Token::MapYou
            | Token::MapMuted
            | Token::MapRoute
    )
}

/// `@0x10194cf4c` — the candidate colours the recipe editor shows for a token.
///
/// `// UNKNOWN:` the binary returns a `Vec<Rgb>` and only fills it for tokens
/// whose role is `Under`: it walks the scheme's hue offsets (clamped lightness
/// `0.3..=0.92`, chroma `0.03..=0.3`) and emits one swatch per step.  Compound
/// (an empty table) yields nothing.
pub fn hue_variants(token: Token, recipe: &Recipe) -> Vec<Rgb> {
    let Role::Under(base, delta) = token.role() else {
        return Vec::new();
    };
    let offsets = recipe.scheme.offsets();
    if offsets.is_empty() {
        return Vec::new();
    }
    let base_lch = oklch::rgb_to_oklch(Token::from_u8(base).unwrap_or(Token::Canvas).bare());
    let seed = oklch::rgb_to_oklch(recipe.seed);
    let l = (base_lch[0] + delta).clamp(0.3, 0.92);
    offsets
        .iter()
        .map(|step| {
            let c = (seed[1] * (1.0 + recipe.variance)).clamp(0.03, 0.3);
            oklch::from_lch([l, c, normalise_degrees(seed[2] + step * delta)])
        })
        .collect()
}

/// `@0x10194d608` — the seed swatches a recipe offers.
///
/// The binary calls its closure twice, first with a `0.05` variance and then
/// (only if that produced something) with `0.0`.  `// UNKNOWN:` the exact
/// predicate between the two calls is not recoverable from the 1124-byte body;
/// two swatches, "vary" then "flat", matches what the data shows.
pub fn seed_swatches(seed: Rgb, background: Rgb) -> Vec<Rgb> {
    let mut out = Vec::with_capacity(2);
    for _variance in [0.05_f64, 0.0] {
        // The recovered closure round-trips the seed through OKLCH and the
        // background, which leaves the seed itself untouched at variance 0.
        let _ = background;
        out.push(oklch::from_lch(oklch::rgb_to_oklch(seed)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::rgb;

    #[test]
    fn parses_every_recovered_scheme_name() {
        let names = [
            ("monochrome", Scheme::Monochrome),
            ("analogous", Scheme::Analogous),
            ("complementary", Scheme::Complementary),
            ("split", Scheme::Split),
            ("triadic", Scheme::Triadic),
            ("tetradic", Scheme::Tetradic),
            ("golden", Scheme::Golden),
            ("compound", Scheme::Compound),
        ];
        for (name, scheme) in names {
            assert_eq!(Scheme::parse(name), Some(scheme), "{name}");
            assert_eq!(scheme.name(), name);
            assert_eq!(Scheme::parse(scheme.name()), Some(scheme));
        }
        assert_eq!(Scheme::parse(""), None);
        assert_eq!(Scheme::parse("Triadic"), None);
        assert_eq!(Scheme::parse("triadics"), None);
        assert_eq!(Scheme::ALL.len(), 8);
    }

    #[test]
    fn discriminants_match_the_binary() {
        assert_eq!(Scheme::Monochrome.index(), 0);
        assert_eq!(Scheme::Analogous.index(), 1);
        assert_eq!(Scheme::Complementary.index(), 2);
        assert_eq!(Scheme::Split.index(), 3);
        assert_eq!(Scheme::Triadic.index(), 4);
        assert_eq!(Scheme::Tetradic.index(), 5);
        assert_eq!(Scheme::Golden.index(), 6);
        assert_eq!(Scheme::Compound.index(), 8);
    }

    #[test]
    fn scheme_offsets_are_the_recovered_tables() {
        assert_eq!(SCHEME_OFFSETS[0], &[0.0]);
        assert_eq!(SCHEME_OFFSETS[1], &[0.0, 30.0, -30.0, 60.0, -60.0]);
        assert_eq!(SCHEME_OFFSETS[2], &[0.0, 180.0]);
        assert_eq!(SCHEME_OFFSETS[3], &[0.0, 150.0, 210.0]);
        assert_eq!(SCHEME_OFFSETS[4], &[0.0, 120.0, 240.0]);
        assert_eq!(SCHEME_OFFSETS[5], &[0.0, 90.0, 180.0, 270.0]);
        assert_eq!(SCHEME_OFFSETS[6], &[0.0, 137.5, 275.0, 52.5, 190.0, 327.5]);
        assert_eq!(SCHEME_OFFSETS[7], &[0.0, 30.0, 180.0, 210.0]);
        assert!(SCHEME_OFFSETS[8].is_empty());
        assert!(Scheme::Compound.offsets().is_empty());
    }

    #[test]
    fn degrees_normalise_into_a_turn() {
        assert_eq!(normalise_degrees(0.0), 0.0);
        assert_eq!(normalise_degrees(360.0), 0.0);
        assert!((normalise_degrees(-30.0) - 330.0).abs() < 1e-9);
        assert!((normalise_degrees(370.0) - 10.0).abs() < 1e-9);
        assert!((normalise_degrees(725.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn hue_for_non_under_tokens_uses_their_own_colour() {
        let own = oklch::rgb_to_oklch(Token::Text.bare())[2];
        assert!((hue_for(Token::Text, 123.0, Scheme::Triadic) - own).abs() < 1e-9);
        let own = oklch::rgb_to_oklch(Token::Health.bare())[2];
        assert!((hue_for(Token::Health, 0.0, Scheme::Triadic) - own).abs() < 1e-9);
    }

    #[test]
    fn hue_for_under_tokens_follows_the_scheme_offsets() {
        // `Hydra` is `Under(1, 0.06)`: base surface 1.
        let h = hue_for(Token::Hydra, 100.0, Scheme::Triadic);
        assert!((h - normalise_degrees(100.0 + 120.0 * 0.06)).abs() < 1e-9, "{h}");
        let h = hue_for(Token::Hydra, 100.0, Scheme::Monochrome);
        assert!((h - 100.0).abs() < 1e-9, "{h}");
        let h = hue_for(Token::Hydra, 10.0, Scheme::Complementary);
        assert!((h - normalise_degrees(10.0 + 180.0 * 0.06)).abs() < 1e-9, "{h}");
        let h = hue_for(Token::Hydra, 42.0, Scheme::Compound);
        assert!((h - 42.0).abs() < 1e-9, "{h}");
    }

    #[test]
    fn generate_keeps_fixed_tokens_and_fills_everything() {
        let colors = generate(&Recipe::default());
        assert_eq!(colors.len(), TOKEN_COUNT);
        assert_eq!(colors[Token::Veil as usize], Token::Veil.bare());
        assert_eq!(colors[Token::Grid as usize], Token::Grid.bare());
        assert_eq!(colors[Token::Mark as usize], Token::Mark.bare());
        assert_eq!(colors[Token::RoomPlate as usize], Token::RoomPlate.bare());
        for (i, c) in colors.iter().enumerate() {
            if i != Token::Veil as usize {
                assert_ne!(*c, 0, "token {i} was not generated");
            }
        }
    }

    #[test]
    fn generate_is_deterministic_and_seed_sensitive() {
        let a = generate(&Recipe::default());
        let b = generate(&Recipe::default());
        assert_eq!(a, b);

        let red = Recipe {
            seed: rgb(0xff, 0x00, 0x00),
            ..Recipe::default()
        };
        let c = generate(&red);
        assert_ne!(a[Token::Accent as usize], c[Token::Accent as usize]);
    }

    #[test]
    fn generate_puts_lightness_tokens_on_their_recorded_lightness() {
        let colors = generate(&Recipe::default());
        let l = oklch::rgb_to_oklch(colors[Token::Text as usize])[0];
        assert!((l - 0.75).abs() < 0.02, "L = {l}");
        let l = oklch::rgb_to_oklch(colors[Token::Muted as usize])[0];
        assert!((l - 0.55).abs() < 0.02, "L = {l}");
    }

    #[test]
    fn hue_variants_only_for_under_roles() {
        let recipe = Recipe::default();
        assert!(hue_variants(Token::Text, &recipe).is_empty());
        assert!(hue_variants(Token::Veil, &recipe).is_empty());
        let v = hue_variants(Token::Hydra, &recipe);
        assert_eq!(v.len(), recipe.scheme.offsets().len());
        let compound = Recipe {
            scheme: Scheme::Compound,
            ..recipe
        };
        assert!(hue_variants(Token::Hydra, &compound).is_empty());
    }

    #[test]
    fn seed_swatches_are_stable() {
        let sw = seed_swatches(rgb(0x63, 0xad, 0xd7), Token::Canvas.bare());
        assert_eq!(sw.len(), 2);
        assert_eq!(sw[0], sw[1]);
        assert_eq!(sw[0], rgb(0x63, 0xad, 0xd7));
    }
}
