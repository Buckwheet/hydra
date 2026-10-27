//! The embedded badge image, rasterised to RGBA.
//!
//! `hydra_gui::badge::pixels` @`0x1007cdcd0` is **full**:
//!
//! ```text
//! load_from_memory(<136269 bytes at 0x102157f69>) -> DynamicImage
//!     (mov w1, #0x144d; movk w1, #0x2, lsl #16   @0x1007cdcfc)
//! if square && size != image.width() {                       // 0x1007cdd28
//!     image = image.resize_exact(size, size, Lanczos3)       // 0x1007cdd4c, w3 = 4
//! }
//! (image.to_rgba8().into_raw(), image.width())               // 0x1007cdd90
//! ```
//!
//! The failure path (`cmp w8, #0xa; b.ne` @`0x1007cdd0c`) drops the
//! `ImageError` and returns an empty `Vec` with size `0` (`stp xzr, x8(=1),
//! [x19]; str xzr, [x19, #0x10]; str wzr, [x19, #0x18]` @`0x1007cdd60`).
//!
//! The two call sites are the `OnceLock`-style initialisers at `0x100546d08`
//! and `0x100556510`, which wrap the result in an `Arc<image::RgbaImage>`
//! (`stp w22, w22, [x0, #0x28]` — the width *and* height are both taken from
//! the returned size, so the badge is always square).
//!
//! ## The asset
//!
//! `src/badge/badge.png` is the verbatim 136269-byte PNG recovered from
//! `0x102157f69` (IHDR: 256×256, 8 bits, colour type 6 = RGBA, not interlaced).
//!
//! ## Departure
//!
//! The original decoded it with the `image` crate. That crate is unavailable
//! here, so `src/badge/badge.rgba` holds the *same* image decoded ahead of time
//! (exactly what `to_rgba8()` yields) and [`decode`] reads that. `badge.png` is
//! kept because it is the recovered asset; decoding it in-tree would need a
//! zlib inflater, which is a dependency replacement rather than recovered crate
//! logic. See `NOTES.md`.

/// The length of the embedded PNG: `0x2144d` (136269) from
/// `mov w1, #0x144d; movk w1, #0x2, lsl #16` @`0x1007cdcfc`.
pub const PNG_LEN: usize = 0x0002_144d;

/// The badge side length, from the PNG's `IHDR` at `0x102157f71`
/// (`00 00 01 00 00 00 01 00` = 256 × 256).
pub const SIDE: u32 = 256;

/// The PNG file, verbatim.
pub const PNG: &[u8] = include_bytes!("badge/badge.png");

/// The decoded RGBA rows (`SIDE * SIDE * 4` bytes).
const RGBA: &[u8] = include_bytes!("badge/badge.rgba");

/// The decoded badge: `(rgba, side)`.
pub fn decode() -> (Vec<u8>, u32) {
    (RGBA.to_vec(), SIDE)
}

/// Rasterise the badge, optionally squared off to `size`.
///
/// Returns `(rgba rows, side)`; the side is the image's width *after* any
/// resize, exactly as the recovered code returns it.
///
/// The only call sites pass `square = false` (see the module docs), so `size`
/// is unused there; the resize branch is the other half of the recovered
/// control flow.
pub fn pixels(square: bool, size: u32) -> (Vec<u8>, u32) {
    let (rgba, side) = decode();
    if !square || size == side {
        return (rgba, side);
    }
    let resized = resize_exact(&rgba, side, side, size, size);
    (resized, size)
}

/// The filter `image::imageops::FilterType` the recovered code selects:
/// `mov w3, #0x4` @`0x1007cdd4c` is `Lanczos3` (the fifth variant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterType {
    Nearest,
    Triangle,
    CatmullRom,
    Gaussian,
    Lanczos3,
}

impl FilterType {
    pub fn support(self) -> f64 {
        match self {
            FilterType::Nearest => 0.0,
            FilterType::Triangle => 1.0,
            FilterType::CatmullRom => 2.0,
            FilterType::Gaussian => 3.0,
            FilterType::Lanczos3 => 3.0,
        }
    }

    pub fn kernel(self, x: f64) -> f64 {
        let x = x.abs();
        match self {
            FilterType::Nearest => {
                if x < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            FilterType::Triangle => (1.0 - x).max(0.0),
            FilterType::CatmullRom => {
                if x < 1.0 {
                    (1.5 * x - 2.5) * x * x + 1.0
                } else if x < 2.0 {
                    ((-0.5 * x + 2.5) * x - 4.0) * x + 2.0
                } else {
                    0.0
                }
            }
            FilterType::Gaussian => {
                if x < 3.0 {
                    ((2.0 * x - 3.0) * x * x + 1.0).max(0.0)
                } else {
                    0.0
                }
            }
            FilterType::Lanczos3 => {
                if x == 0.0 {
                    1.0
                } else if x < 3.0 {
                    let pix = std::f64::consts::PI * x;
                    3.0 * pix.sin() * (pix / 3.0).sin() / (pix * pix)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Two-pass resize of an RGBA8 buffer.
///
/// Mirrors `image::imageops::resize`: the filter support is widened by the
/// sampling ratio, the weights are normalised per output pixel, and the
/// horizontal pass runs before the vertical one.
pub fn resize_exact(
    rgba: &[u8],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> Vec<u8> {
    resize_with(rgba, width, height, new_width, new_height, FilterType::Lanczos3)
}

fn resize_with(
    rgba: &[u8],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
    filter: FilterType,
) -> Vec<u8> {
    if new_width == 0 || new_height == 0 || width == 0 || height == 0 {
        return Vec::new();
    }
    let horizontal = resample_axis(rgba, width, height, new_width, filter, Axis::X);
    resample_axis(&horizontal, new_width, height, new_height, filter, Axis::Y)
}

enum Axis {
    X,
    Y,
}

/// Resample one axis: `X` turns `width × height` into `new_len × height`,
/// `Y` turns `width × height` into `width × new_len`.
fn resample_axis(
    rgba: &[u8],
    width: u32,
    height: u32,
    new_len: u32,
    filter: FilterType,
    axis: Axis,
) -> Vec<u8> {
    let old_len = match axis {
        Axis::X => width,
        Axis::Y => height,
    };
    let ratio = old_len as f64 / new_len as f64;
    let sratio = ratio.max(1.0);
    let support = filter.support() * sratio;
    let (out_w, out_h) = match axis {
        Axis::X => (new_len, height),
        Axis::Y => (width, new_len),
    };
    let mut out = vec![0u8; out_w as usize * out_h as usize * 4];

    for out_index in 0..new_len {
        let center = (out_index as f64 + 0.5) * ratio;
        let left = ((center - support).floor() as i64).max(0) as u32;
        let right = ((center + support).ceil() as i64).min(old_len as i64).max(0) as u32;
        let mut weights: Vec<(u32, f64)> = Vec::new();
        let mut sum = 0.0;
        for i in left..right {
            let w = filter.kernel((i as f64 - center + 0.5) / sratio);
            if w != 0.0 {
                weights.push((i, w));
                sum += w;
            }
        }
        let other_len = match axis {
            Axis::X => height,
            Axis::Y => width,
        };
        for other in 0..other_len {
            let mut acc = [0.0f64; 4];
            for (i, w) in &weights {
                let src = match axis {
                    Axis::X => (*i as usize + other as usize * width as usize) * 4,
                    Axis::Y => (other as usize + *i as usize * width as usize) * 4,
                };
                for c in 0..4 {
                    acc[c] += rgba[src + c] as f64 * w;
                }
            }
            let dst = match axis {
                Axis::X => (out_index as usize + other as usize * out_w as usize) * 4,
                Axis::Y => (other as usize + out_index as usize * out_w as usize) * 4,
            };
            for c in 0..4 {
                let v = if sum != 0.0 { acc[c] / sum } else { 0.0 };
                out[dst + c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_png_is_the_recovered_asset() {
        assert_eq!(PNG.len(), PNG_LEN);
        assert_eq!(&PNG[..8], b"\x89PNG\r\n\x1a\n");
        // IHDR: width, height, bit depth, colour type 6 (RGBA), no interlace.
        assert_eq!(&PNG[12..16], b"IHDR");
        assert_eq!(&PNG[16..20], &[0, 0, 1, 0]);
        assert_eq!(&PNG[20..24], &[0, 0, 1, 0]);
        assert_eq!(PNG[24], 8);
        assert_eq!(PNG[25], 6);
        assert_eq!(PNG[28], 0);
    }

    #[test]
    fn decode_is_a_256_square_of_rgba() {
        let (rgba, side) = decode();
        assert_eq!(side, SIDE);
        assert_eq!(rgba.len(), (SIDE * SIDE * 4) as usize);
        // Spot colours from the recovered image.
        let px = |x: usize, y: usize| {
            let i = (y * SIDE as usize + x) * 4;
            [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
        };
        assert_eq!(px(0, 0), [0, 0, 0, 0]);
        assert_eq!(px(128, 128), [48, 83, 118, 255]);
        assert_eq!(px(200, 64), [30, 35, 93, 255]);
        assert_eq!(px(64, 200), [62, 62, 123, 255]);
    }

    #[test]
    fn pixels_without_squaring_returns_the_native_image() {
        let (rgba, side) = pixels(false, 64);
        assert_eq!(side, 256);
        assert_eq!(rgba.len(), 256 * 256 * 4);
        // `square = false` never resizes, whatever the size argument says.
        assert_eq!(pixels(false, 0).1, 256);
    }

    #[test]
    fn pixels_squares_only_when_the_size_differs() {
        // size == width: the recovered `size != image.width()` guard means no
        // resize happens at all.
        let (same, side) = pixels(true, 256);
        assert_eq!(side, 256);
        assert_eq!(same.len(), 256 * 256 * 4);

        let (small, side) = pixels(true, 32);
        assert_eq!(side, 32);
        assert_eq!(small.len(), 32 * 32 * 4);
    }

    #[test]
    fn lanczos3_kernel_matches_its_definition() {
        let filter = FilterType::Lanczos3;
        let k = |x: f64| filter.kernel(x);
        assert_eq!(k(0.0), 1.0);
        assert!(k(3.0).abs() < f64::EPSILON);
        assert!(k(4.0) == 0.0);
        // Zero crossings at every integer.
        for n in 1..3 {
            assert!(k(n as f64).abs() < 1e-12, "k({n})");
        }
        assert!(k(0.5) > 0.0 && k(1.5) < 0.0);
    }

    #[test]
    fn resizing_a_solid_image_keeps_the_colour() {
        let (w, h) = (8u32, 8u32);
        let mut flat = Vec::new();
        for _ in 0..w * h {
            flat.extend_from_slice(&[10, 20, 30, 255]);
        }
        let small = resize_exact(&flat, w, h, 4, 4);
        assert_eq!(small.len(), 4 * 4 * 4);
        for px in small.chunks(4) {
            assert_eq!(px, [10, 20, 30, 255]);
        }
    }

    #[test]
    fn resizing_the_real_badge_keeps_the_alpha_shape() {
        let (rgba, side) = pixels(true, 64);
        assert_eq!(side, 64);
        let opaque = rgba.chunks(4).filter(|p| p[3] > 0).count();
        // The source is 52448/65536 = 80.0% opaque; the 64x64 version keeps the
        // same shape to within a filter's tolerance.
        let source = decode().0.chunks(4).filter(|p| p[3] > 0).count() as f32 / 65536.0;
        let scaled = opaque as f32 / 4096.0;
        assert!((scaled - source).abs() < 0.05, "{scaled} vs {source}");
    }
}
