//! CPU-side image building for the figure/ground quads and the skin atlas.
//!
//! Recovered from `gs_render::figure_image::{ground_geo, skin_image, with_glow}`
//! (`0x1009644cc`, `0x1009647a8`, `0x10096508c`). The original built `egui`
//! meshes and `image::RgbaImage`s; this module keeps the arithmetic, the
//! per-texel slicing and the output buffers and hands GPU work to
//! [`crate::gpu::Gpu`] (no-op by default).

use std::sync::Arc;

/// One figure's ground plane: the rotating quad the character stands on.
///
/// Field offsets are the ones `ground_geo` reads from its first argument
/// (`ldr s0, [x0, #0x14]`, `ldr s15, [x22, #0x10]`, `ldr s10, [x22, #0x8]`,
/// `ldr s11, [x22, #0x18]`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundPlane {
    pub unknown_00: f32,
    pub unknown_04: f32,
    /// `+0x08` — the depth spacing of one grid row before the projection.
    pub spacing: f32,
    pub unknown_0c: f32,
    /// `+0x10` — the horizon offset subtracted from the row depth.
    pub horizon: f32,
    /// `+0x14` — the field angle; `sin_cos()` of it rotates the quad's uv axes.
    pub angle: f32,
    /// `+0x18` — the plane's distance, divided by the projected row depth.
    pub distance: f32,
}

/// The camera slice used for one field: how many units a row covers and where
/// the near plane sits (`ldr s0, [x23, #0x4]`, `ldr s13, [x23, #0xc]`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundView {
    pub unknown_00: f32,
    /// `+0x04` — depth of the first row.
    pub depth: f32,
    pub unknown_08: f32,
    /// `+0x0c` — total depth span across all rows.
    pub span: f32,
}

impl Default for GroundView {
    fn default() -> Self {
        // The defaults only matter for callers that do not care; every value the
        // binary uses comes from the caller's view.
        GroundView {
            unknown_00: 0.0,
            depth: 0.0,
            unknown_08: 0.0,
            span: 1.0,
        }
    }
}

/// `ground_geo` @0x1009644cc — build the ground quad's vertices.
///
/// `rows` values are pushed for each of `cols` rows, so the returned vector has
/// exactly `rows * cols` entries (the binary computes `mul x26, x20, x21` and
/// allocates `count * 16` bytes, i.e. `[f32; 4]` vertices).
///
/// Each vertex is `[0.0, u, v, depth]` where
///
/// ```text
/// t     = (i + 0.5) * view.span / cols + view.depth - plane.horizon
/// denom = sin + t * cos / plane.spacing
/// raw   = plane.distance / denom                  (or +inf when denom <= 1e-4)
/// depth = min(raw, 100.0)
/// u, v  = if raw < 100 { (-cos, -sin) } else { (0.0, -1.0) }
/// ```
///
/// The per-row values are duplicated for every column: the inner store loop in
/// the binary is `stp q0, q0, ...` repeated, i.e. the same 16 bytes written
/// `rows` times.
pub fn ground_geo(
    plane: &GroundPlane,
    rows: usize,
    cols: usize,
    view: &GroundView,
) -> Vec<[f32; 4]> {
    // `ldr s0, [x0, #0x14]; bl __sincosf_stret` — `f32::sin_cos`.
    let (sin, cos) = plane.angle.sin_cos();
    let mut out: Vec<[f32; 4]> = Vec::with_capacity(rows.saturating_mul(cols));
    if rows == 0 || cols == 0 {
        return out;
    }
    let cols_f = cols as f32;
    for i in 0..cols {
        let t = (i as f32 + 0.5) * view.span / cols_f + view.depth - plane.horizon;
        let denom = sin + t * cos / plane.spacing;
        let raw = if denom > DEPTH_EPSILON {
            plane.distance / denom
        } else {
            f32::INFINITY
        };
        let depth = raw.min(DEPTH_CLAMP);
        // The two `fcsel`s at 0x100964650/0x100964664 pick the fallbacks when
        // the projection is behind the near plane.
        let (u, v) = if raw < DEPTH_CLAMP {
            (-cos, -sin)
        } else {
            (0.0, -1.0)
        };
        let vertex = [0.0f32, u, v, depth];
        for _ in 0..rows {
            out.push(vertex);
        }
    }
    out
}

/// `1e-4f` — the `fcmp s0, s8` constant at `0x10096462c`
/// (`mov w8,#0xb717; movk w8,#0x38d1,lsl #16` = `0x38D1B717`).
pub const DEPTH_EPSILON: f32 = 1.0e-4;

/// `100.0f` — the far clamp (`fcsel` target `0x42C80000`).
pub const DEPTH_CLAMP: f32 = 100.0;

/// The raw RGBA sheet a skin is drawn from plus its atlas dimensions.
///
/// `skin_image` reads the pixel bytes through `(ptr @+0x08, len @+0x10)` and the
/// atlas size through `ldp x19, x23, [x1, #0x20]` (two `usize` fields).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinSheet {
    pub pixels: Vec<u8>,
    /// `+0x20`
    pub width: usize,
    /// `+0x28`
    pub height: usize,
}

impl SkinSheet {
    pub fn new(pixels: Vec<u8>, width: usize, height: usize) -> Self {
        SkinSheet {
            pixels,
            width,
            height,
        }
    }

    /// `width * height` — the texel count `mul x21, x23, x19` at `0x1009647dc`.
    pub fn texels(&self) -> usize {
        self.width.saturating_mul(self.height)
    }

    /// The alpha byte of texel `i`, read exactly like the binary does:
    /// `ldrb w24, [x28, x26]` with `x28 = ptr + 3` and `x26 = i * 4`.
    pub fn alpha_of(&self, i: usize) -> Option<u8> {
        self.pixels.get(i * 4 + 3).copied()
    }

    /// The RGB colour of texel `i`, from the texel struct the surface returns
    /// (`ldurh`/`ldurb` at `+0x1c..+0x1f`).
    pub fn rgb_of(&self, i: usize) -> Option<[u8; 3]> {
        let base = i * 4;
        Some([
            *self.pixels.get(base)?,
            *self.pixels.get(base + 1)?,
            *self.pixels.get(base + 2)?,
        ])
    }
}

/// One texel of the smooth skin surface, as returned by
/// `gs_puppet::skin::SkinSurface::texel` (called at `0x1009649b0`).
///
/// `Some` is the `w8 == 1` case at `0x1009649b8`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinTexel {
    /// Colour bytes written at `+0x1c` of the returned struct (3 bytes).
    pub rgb: [u8; 3],
    /// The two-component sheet normal read at `+0xc8` (a `d`-register load).
    pub normal: [f32; 2],
    /// The third normal/quality component read at `+0xd0`.
    pub z: f32,
}

/// The smooth surface `skin_image` samples: a light direction at `+0xa0`
/// (`ldp s9, s12, [x2, #0xa0]; ldr s10, [x2, #0xa8]`) and a texel accessor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkinSurface {
    /// `+0xa0` — light direction `(x, y, z)`.
    pub light: [f32; 3],
    /// Texels in atlas order (row major over [`SkinSheet::width`]).
    pub texels: Vec<Option<SkinTexel>>,
}

impl SkinSurface {
    /// `SkinSurface::texel(&self, i)`: `None` when the texel is not covered.
    pub fn texel(&self, i: usize) -> Option<SkinTexel> {
        self.texels.get(i).copied().flatten()
    }
}

/// What `skin_image` returns.
///
/// The binary returns a 40-byte struct holding three `Arc`s (allocated with
/// `mov w0,#0x28` / `__rust_alloc` at `0x100964d48` and friends) plus the atlas
/// dimensions written by `stp w19, w28, [x9, #0x20]`. `// UNKNOWN:` the exact
/// field order of the three `Arc`s; the roles below are read from how the
/// buffers are filled (4-byte RGBA, 16-byte normal vertices, 1-byte coverage).
#[derive(Debug, Clone, PartialEq)]
pub struct SkinImage {
    /// `count * 4` zeroed bytes; RGB from the texel, alpha from the sheet.
    pub albedo: Arc<Vec<u8>>,
    /// `count` 16-byte vertices `[n.x, n.y, shade, glow]`.
    pub normals: Arc<Vec<[f32; 4]>>,
    /// `count` flags: 1 when the surface produced a texel.
    pub coverage: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}

/// `skin_image` @0x1009647a8 — turn a smooth skin surface back into an image.
///
/// Recovered behaviour:
///
/// * the texel count is `sheet.width * sheet.height` and three buffers that size
///   are allocated: `count * 4` zeroed bytes (RGBA), `count` vertices of 16
///   bytes, and `count` zeroed bytes of coverage;
/// * for every texel the 4th byte (`pixels[i * 4 + 3]`) is copied into the alpha
///   channel *whether or not* the surface covers the texel (`strb w24, [x8, #0x3]`
///   at `0x100964ac0` runs on both paths);
/// * when `SkinSurface::texel` returns `Some`, the RGB comes from the texel
///   (`+0x1c..+0x1f`), a shaded normal is written to the 16-byte vertex buffer and
///   the coverage flag is set to 1.
///
/// `// UNKNOWN:` the 3x3 basis the binary builds at `0x100964898`-`0x100964990`
/// (light-vector orthonormalisation, arithmetic listed in `NOTES.md`) was not
/// decoded lane by lane, so the shading below is the documented simplification
/// `N.L` clamped to `0..1` instead of the binary's matrix product.
pub fn skin_image(scale: f32, sheet: &SkinSheet, surface: &SkinSurface) -> SkinImage {
    let count = sheet.texels();
    let mut albedo = vec![0u8; count * 4];
    let mut normals = vec![[0.0f32; 4]; count];
    let mut coverage = vec![0u8; count];
    let light = surface.light;
    let light_len = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2])
        .sqrt()
        .max(1.0e-6);
    let l = [
        light[0] / light_len,
        light[1] / light_len,
        light[2] / light_len,
    ];
    for i in 0..count {
        let alpha = sheet.alpha_of(i).unwrap_or(0);
        if let Some(t) = surface.texel(i) {
            albedo[i * 4] = t.rgb[0];
            albedo[i * 4 + 1] = t.rgb[1];
            albedo[i * 4 + 2] = t.rgb[2];
            // Rebuild the third component of the sheet normal (the surface stores
            // two, `+0xc8`, plus a third scalar at `+0xd0`).
            let n = [
                t.normal[0],
                t.normal[1],
                (1.0 - t.normal[0] * t.normal[0] - t.normal[1] * t.normal[1])
                    .max(0.0)
                    .sqrt(),
            ];
            let shade = (n[0] * l[0] + n[1] * l[1] + n[2] * l[2]).clamp(0.0, 1.0);
            normals[i] = [n[0], n[1], shade, (t.z * scale).clamp(0.0, 1.0)];
            coverage[i] = 1;
        }
        albedo[i * 4 + 3] = alpha;
    }
    SkinImage {
        albedo: Arc::new(albedo),
        normals: Arc::new(normals),
        coverage: Arc::new(coverage),
        width: sheet.width as u32,
        height: sheet.height as u32,
    }
}

/// `with_glow` @0x10096508c — copy an RGBA image and overwrite its alpha channel
/// from a one-byte-per-texel glow buffer.
///
/// Fully recovered: `rgba.to_vec()` (`__rust_alloc` + `memcpy` at
/// `0x1009650c0..0x1009650e0`) followed by
///
/// ```text
/// for (i, g) in glow.iter().enumerate() { out[i * 4 + 3] = g }
/// ```
///
/// which is the `sturb w14, [x12, #-0x1c] ... add x12, x12, #0x40` store pattern
/// at `0x1009651e4`. The trailing `panic_bounds_check` at `0x100965278` is the
/// ordinary slice index check, so a `glow` longer than the pixel count panics.
/// An empty `glow` leaves the alpha channel zeroed (`0x10096511c` stores `wzr`
/// into every 4th byte).
pub fn with_glow(rgba: &[u8], glow: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    if glow.is_empty() {
        let mut i = 3;
        while i < out.len() {
            out[i] = 0;
            i += 4;
        }
        return out;
    }
    for (i, g) in glow.iter().enumerate() {
        out[i * 4 + 3] = *g;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(angle: f32) -> GroundPlane {
        GroundPlane {
            unknown_00: 0.0,
            unknown_04: 0.0,
            spacing: 2.0,
            unknown_0c: 0.0,
            horizon: 0.0,
            angle,
            distance: 8.0,
        }
    }

    #[test]
    fn ground_geo_pushes_rows_per_column() {
        let view = GroundView {
            unknown_00: 0.0,
            depth: 0.0,
            unknown_08: 0.0,
            span: 4.0,
        };
        // 3 rows x 5 columns = 15 vertices, 16 bytes each.
        let v = ground_geo(&plane(0.0), 3, 5, &view);
        assert_eq!(v.len(), 15);
        assert_eq!(std::mem::size_of_val(&v[0]), 16);
    }

    #[test]
    fn ground_geo_angle_zero_is_axis_aligned() {
        let view = GroundView {
            unknown_00: 0.0,
            depth: 0.0,
            unknown_08: 0.0,
            span: 4.0,
        };
        let v = ground_geo(&plane(0.0), 1, 2, &view);
        // sin = 0, cos = 1 -> u = -1, v = -0 (=-sin).
        for vert in &v {
            assert_eq!(vert[0], 0.0);
            assert_eq!(vert[1], -1.0);
            assert_eq!(vert[2].to_bits(), (-0.0f32).to_bits());
        }
    }

    #[test]
    fn ground_geo_depth_is_the_plane_distance_over_the_ray() {
        // angle 0 -> denom = sin + t*cos/spacing = t/2.
        // i = 0, cols = 1: t = 0.5 * span + depth - horizon.
        let view = GroundView {
            unknown_00: 0.0,
            depth: 2.0,
            unknown_08: 0.0,
            span: 4.0,
        };
        let v = ground_geo(&plane(0.0), 1, 1, &view);
        // t = 0.5*4 + 2 = 4 ; denom = 4/2 = 2 ; depth = 8/2 = 4.
        assert_eq!(v[0][3], 4.0);
    }

    #[test]
    fn ground_geo_far_plane_falls_back_to_the_flat_frame() {
        // Huge distance: raw > 100 -> clamp + fallback uv.
        let mut p = plane(0.0);
        p.distance = 1.0e6;
        let view = GroundView {
            unknown_00: 0.0,
            depth: 1.0,
            unknown_08: 0.0,
            span: 1.0,
        };
        let v = ground_geo(&p, 1, 1, &view);
        assert_eq!(v[0][3], DEPTH_CLAMP);
        assert_eq!(v[0][1], 0.0);
        assert_eq!(v[0][2], -1.0);
    }

    #[test]
    fn ground_geo_behind_the_eye_is_clamped() {
        let mut p = plane(0.0);
        p.spacing = 1.0;
        p.horizon = 100.0; // t becomes negative -> denom negative
        let view = GroundView {
            unknown_00: 0.0,
            depth: 0.0,
            unknown_08: 0.0,
            span: 0.0,
        };
        let v = ground_geo(&p, 1, 1, &view);
        assert_eq!(v[0][3], DEPTH_CLAMP);
        assert_eq!(v[0][1], 0.0);
        assert_eq!(v[0][2], -1.0);
    }

    #[test]
    fn with_glow_replaces_alpha_and_keeps_rgb() {
        let rgba = vec![
            1, 2, 3, 4, //
            5, 6, 7, 8, //
            9, 10, 11, 12,
        ];
        let out = with_glow(&rgba, &[200, 201, 202]);
        assert_eq!(out, vec![1, 2, 3, 200, 5, 6, 7, 201, 9, 10, 11, 202]);
    }

    #[test]
    fn with_glow_empty_zeroes_alpha() {
        let rgba = vec![1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(with_glow(&rgba, &[]), vec![1, 2, 3, 0, 5, 6, 7, 0]);
    }

    #[test]
    #[should_panic]
    fn with_glow_longer_than_pixels_panics() {
        let rgba = vec![0u8; 4];
        let _ = with_glow(&rgba, &[1, 2]);
    }

    fn sheet() -> SkinSheet {
        // 2x2 atlas: 4 texels, RGBA, alpha byte of texel i is 0x10 * (i + 1).
        let pixels = vec![
            10, 11, 12, 0x10, //
            20, 21, 22, 0x20, //
            30, 31, 32, 0x30, //
            40, 41, 42, 0x40,
        ];
        SkinSheet::new(pixels, 2, 2)
    }

    #[test]
    fn skin_sheet_slicing_offsets_are_4_byte_texels() {
        let s = sheet();
        assert_eq!(s.texels(), 4);
        assert_eq!(s.alpha_of(0), Some(0x10));
        assert_eq!(s.alpha_of(3), Some(0x40));
        assert_eq!(s.rgb_of(2), Some([30, 31, 32]));
        assert_eq!(s.alpha_of(4), None);
    }

    #[test]
    fn skin_image_keeps_alpha_even_without_surface_texels() {
        let s = sheet();
        let surf = SkinSurface {
            light: [0.0, 0.0, 1.0],
            texels: vec![None, None, None, None],
        };
        let img = skin_image(1.0, &s, &surf);
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.albedo.len(), 16);
        assert_eq!(img.coverage, Arc::new(vec![0u8; 4]));
        for i in 0..4 {
            assert_eq!(img.albedo[i * 4 + 3], 0x10 * (i as u8 + 1));
            assert_eq!(&img.albedo[i * 4..i * 4 + 3], &[0, 0, 0]);
        }
    }

    #[test]
    fn skin_image_copies_colour_and_flags_coverage() {
        let s = sheet();
        let texel = SkinTexel {
            rgb: [1, 2, 3],
            normal: [0.0, 0.0],
            z: 0.5,
        };
        let surf = SkinSurface {
            light: [0.0, 0.0, 1.0],
            texels: vec![Some(texel), None, Some(texel), None],
        };
        let img = skin_image(2.0, &s, &surf);
        assert_eq!(&img.albedo[0..4], &[1, 2, 3, 0x10]);
        assert_eq!(&img.albedo[4..8], &[0, 0, 0, 0x20]);
        assert_eq!(img.coverage.as_slice(), &[1, 0, 1, 0]);
        // Straight-on light on a flat normal (0,0,1) -> shade 1.
        assert_eq!(img.normals[0][2], 1.0);
        // z * scale, clamped.
        assert_eq!(img.normals[0][3], 1.0);
        assert_eq!(img.normals[1], [0.0, 0.0, 0.0, 0.0]);
    }
}
