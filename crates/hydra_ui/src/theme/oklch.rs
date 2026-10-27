//! OKLCH colour maths.
//!
//! Recovered from three functions in the binary:
//!
//! * `theme::oklch::rgb_to_oklab` @0x101871c54,
//! * `theme::oklch::contrast`     @0x101871fa4,
//! * `theme::oklch::from_lch`     @0x1018723ac.
//!
//! All three work in `f64` and use the published Oklab formulas; the constants
//! below are the exact `f64`s the binary loads (`fmov` immediates), so the
//! results match the original.

use crate::theme::{blue, green, red, rgb, Rgb};

/// sRGB transfer-function knee (`0x3fa4b5dcc63f1412`).
const SRGB_KNEE: f64 = 0.04045;
/// `0x3fac28f5c28f5c29`.
const SRGB_A: f64 = 0.055;
/// `0x3ff0e147ae147ae1`.
const SRGB_B: f64 = 1.055;
/// `0x4003333333333333`.
const SRGB_GAMMA: f64 = 2.4;
/// `0x4029d70a3d70a3d7`.
const SRGB_SCALE: f64 = 12.92;
/// `0x406fe00000000000`.
const BYTE_MAX: f64 = 255.0;
/// `0x404ca5dc1a63c1f8` — `180 / pi`.
pub const DEG_PER_RAD: f64 = 57.295_779_513_082_32;
/// `0x3f91df46a2529d39` — `pi / 180`.
pub const RAD_PER_DEG: f64 = 0.017_453_292_519_943_295;

/// One sRGB channel (0..=1) to linear light, exactly as `rgb_to_oklab` does it.
fn to_linear(c: f64) -> f64 {
    if c > SRGB_KNEE {
        ((c + SRGB_A) / SRGB_B).powf(SRGB_GAMMA)
    } else {
        c / SRGB_SCALE
    }
}

/// The linear-light knee for the inverse transfer function.
const LINEAR_KNEE: f64 = 0.003_130_8;

/// Linear light back to an sRGB channel, exactly as `from_lch` does it.
fn from_linear(c: f64) -> f64 {
    if c > LINEAR_KNEE {
        SRGB_B * c.powf(1.0 / SRGB_GAMMA) - SRGB_A
    } else {
        SRGB_SCALE * c
    }
}

/// `rgb_to_oklab` @0x101871c54 — an 8-bit sRGB colour as `[L, a, b]`.
pub fn rgb_to_oklab(c: Rgb) -> [f64; 3] {
    let r = to_linear(red(c) as f64 / BYTE_MAX);
    let g = to_linear(green(c) as f64 / BYTE_MAX);
    let b = to_linear(blue(c) as f64 / BYTE_MAX);

    // M1 (linear sRGB -> LMS), recovered as twelve `fmov` immediates.
    let l = 0.412_221_470_799_999_93 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;

    let l = l.cbrt();
    let m = m.cbrt();
    let s = s.cbrt();

    // M2 (LMS' -> Lab).
    [
        0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
    ]
}

/// The same colour as OKLCH: `[L, chroma, hue in degrees]`.
pub fn rgb_to_oklch(c: Rgb) -> [f64; 3] {
    lab_to_lch(rgb_to_oklab(c))
}

/// Oklab to OKLCH.  The hue is returned in **degrees** in `[0, 360)`, which is
/// what `from_lch` expects and what `harmony::hue_for` normalises to.
pub fn lab_to_lch(lab: [f64; 3]) -> [f64; 3] {
    let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let mut hue = lab[2].atan2(lab[1]) * DEG_PER_RAD + 360.0;
    hue %= 360.0;
    if hue < 0.0 {
        hue += 360.0;
    }
    [lab[0], chroma, hue]
}

/// `from_lch` @0x1018723ac — OKLCH back to an 8-bit sRGB colour.
///
/// The hue is in degrees; each linear channel is clamped into `[0, 1]` before
/// the transfer function (the binary's `fcmlt`/`fcmgt`/`bit` sequence), then
/// rounded half-away-from-zero (`frinta`) and saturated at 255.
pub fn from_lch(lch: [f64; 3]) -> Rgb {
    let l = lch[0];
    let chroma = lch[1].max(0.0);
    let hue = lch[2] * RAD_PER_DEG;
    let a = chroma * hue.cos();
    let b = chroma * hue.sin();

    // Inverse M2.
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;

    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;

    // Inverse M1.
    let r = 4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3;
    let g = -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3;
    let bb = -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701 * s3;

    let to_byte = |c: f64| -> u8 {
        let c = from_linear(c.clamp(0.0, 1.0));
        let v = (c * BYTE_MAX).round();
        if v < BYTE_MAX {
            v as u8
        } else {
            255
        }
    };
    rgb(to_byte(r), to_byte(g), to_byte(bb))
}

/// Relative luminance, exactly the coefficients `contrast` uses.
///
/// `0x3fcb367a0f9096bc` (0.2126), `0x3fe6e2eb1c432ca5` (0.7152) and
/// `0x3fb27bb2fec56d5d` (0.0722).
pub fn luminance(c: Rgb) -> f64 {
    let r = to_linear(red(c) as f64 / BYTE_MAX);
    let g = to_linear(green(c) as f64 / BYTE_MAX);
    let b = to_linear(blue(c) as f64 / BYTE_MAX);
    0.212_6 * r + 0.715_2 * g + 0.072_2 * b
}

/// `@0x101871fa4` — the WCAG contrast ratio between two colours.
///
/// `(lighter + 0.05) / (darker + 0.05)`; `0x3fa999999999999a` is the `0.05`
/// the binary adds to both sides.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let la = luminance(a);
    let lb = luminance(b);
    let hi = la.max(lb) + 0.05;
    let lo = la.min(lb) + 0.05;
    hi / lo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn black_and_white_are_exact() {
        let w = rgb(255, 255, 255);
        let k = rgb(0, 0, 0);
        assert_eq!(contrast(w, k), 21.0);
        assert_eq!(contrast(k, w), 21.0);
        assert_eq!(contrast(w, w), 1.0);
        assert_eq!(contrast(k, k), 1.0);
    }

    #[test]
    fn luminance_matches_rec709() {
        assert!(close(luminance(rgb(255, 255, 255)), 1.0, 1e-12));
        assert!(close(luminance(rgb(0, 0, 0)), 0.0, 1e-12));
        assert!(luminance(rgb(0, 255, 0)) > luminance(rgb(255, 0, 0)));
        assert!(luminance(rgb(255, 0, 0)) > luminance(rgb(0, 0, 255)));
    }

    #[test]
    fn oklab_of_greys_is_neutral() {
        let lab = rgb_to_oklab(rgb(0x3c, 0x3c, 0x3c));
        assert!(lab[1].abs() < 1e-6, "a = {}", lab[1]);
        assert!(lab[2].abs() < 1e-6, "b = {}", lab[2]);
        assert!(lab[0] > 0.3 && lab[0] < 0.45, "L = {}", lab[0]);
        assert!(close(rgb_to_oklab(rgb(255, 255, 255))[0], 1.0, 1e-6));
        assert!(close(rgb_to_oklab(rgb(0, 0, 0))[0], 0.0, 1e-9));
    }

    #[test]
    fn oklch_hue_of_red_is_around_29_degrees() {
        let lch = rgb_to_oklch(rgb(255, 0, 0));
        assert!((lch[2] - 29.23).abs() < 0.5, "hue = {}", lch[2]);
        assert!(rgb_to_oklch(rgb(0x80, 0x80, 0x80))[1] < 1e-4);
    }

    #[test]
    fn lch_round_trips_through_rgb() {
        for c in [
            rgb(0xcd, 0x4d, 0x4d),
            rgb(0x47, 0x84, 0xd9),
            rgb(0x1b, 0x1b, 0x1b),
            rgb(0xff, 0xff, 0xff),
        ] {
            let back = from_lch(rgb_to_oklch(c));
            assert_eq!(back, c, "{c:#08x}");
        }
    }

    #[test]
    fn from_lch_clamps_and_saturates() {
        // A wildly out-of-gamut LCH must still land on real bytes.
        let c = from_lch([2.0, 5.0, 120.0]);
        for channel in [red(c), green(c), blue(c)] {
            assert!(channel == 0 || channel == 255, "{c:#08x}");
        }
        assert_eq!(from_lch([-1.0, 0.0, 0.0]), rgb(0, 0, 0));
        assert_eq!(from_lch([1.0, 0.0, 0.0]), rgb(255, 255, 255));
    }

    #[test]
    fn hue_is_normalised_into_a_turn() {
        let lch = rgb_to_oklch(rgb(0x63, 0xad, 0xd7));
        assert!(lch[2] >= 0.0 && lch[2] < 360.0, "hue = {}", lch[2]);
    }
}
