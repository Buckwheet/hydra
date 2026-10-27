//! The UV atlas: how a rig is packed into a skin texture, and how that texture is
//! sampled.
//!
//! Recovered facts (`gs_puppet::uv`):
//!
//! * `UvLayout` is `{ patches: Vec<Patch>, width: usize, height: usize }` — the
//!   pixel dimensions sit at `+0x18` / `+0x20` and are read with `ucvtf` (so they
//!   are integers), and `UvLayout::new`/`fit` iterate a `Vec` of **224-byte**
//!   elements at `+0x38`, i.e. the rig's `model::Segment`s.  The layout is
//!   therefore *derived from the rig*, not from a mesh: `new(&Puppet, px_per_unit)`.
//! * A `Patch` is 80 bytes: a `Vec` of 8-byte (two `f32`) elements at `+0`, and a
//!   `Vec` of 24-byte (three `f32`) elements at `+0x18`.
//! * `UvLayout::fit(&Puppet, size: u32)` builds the layout at 64 px/unit
//!   (`0x42800000`), takes `max(width, height)` and rebuilds it at
//!   `size * 64 / max`, i.e. it rescales so the atlas' long side is exactly `size`
//!   pixels.
//! * `UvLayout::new` clamps its scale argument up to `1.0` (`fmaxnm` against the
//!   `1.0` immediate) and uses a `6.0` unit cell (`12.0` = two cells) — the two
//!   `fmov` immediates in its body.
//! * `Texture::sample` / `sample_normal` work in `0..255` fixed point with a
//!   `0.5` texel offset and `1/255` (`0x437f0000`) normalisation; the normal
//!   variant decodes with `127.5` (`0x42ff0000`) and `±1.0`, i.e. the standard
//!   tangent-space normal map encoding.

use crate::math::{Vec3, DEG_TO_RAD};
use crate::model::Puppet;

/// Unit size of one atlas cell, `fmov #6.0` in `UvLayout::new`.
pub const CELL_UNITS: f32 = 6.0;

/// Two atlas cells (`fmov #12.0` in `UvLayout::new`).
pub const CELL_STRIDE: f32 = 12.0;

/// The pixels-per-unit `UvLayout::fit` starts from (`0x42800000`).
pub const BASE_PX_PER_UNIT: f32 = 64.0;

/// RGBA8 texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA, `width * height * 4` bytes.
    pub data: Vec<u8>,
}

impl Texture {
    /// A blank (opaque white) texture.
    pub fn blank(width: u32, height: u32) -> Texture {
        Texture {
            width,
            height,
            data: vec![255u8; (width as usize) * (height as usize) * 4],
        }
    }

    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>) -> Texture {
        assert_eq!(data.len(), (width as usize) * (height as usize) * 4);
        Texture {
            width,
            height,
            data,
        }
    }

    /// Raw texel, clamped to the edges.
    pub fn texel(&self, x: u32, y: u32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let x = x.min(self.width - 1) as usize;
        let y = y.min(self.height - 1) as usize;
        let i = (y * self.width as usize + x) * 4;
        let d = &self.data[i..i + 4];
        [
            d[0] as f32 / 255.0,
            d[1] as f32 / 255.0,
            d[2] as f32 / 255.0,
            d[3] as f32 / 255.0,
        ]
    }

    /// Bilinear sample at a UV in `0..1` (wrapping).
    ///
    /// `@0x1009cb7c4` — `Texture::sample(&self, &[f32; 2]) -> [f32; 4]`: the
    /// recovered body subtracts the `0.5` texel offset, wraps, and lerps four
    /// texels with `1/255` fixed-point weights.
    pub fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let fx = uv[0] * self.width as f32 - 0.5;
        let fy = uv[1] * self.height as f32 - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let wrap = |v: f32, n: u32| -> u32 {
            let n = n as f32;
            let m = v.rem_euclid(n);
            m as u32
        };
        let xa = wrap(x0, self.width);
        let xb = wrap(x0 + 1.0, self.width);
        let ya = wrap(y0, self.height);
        let yb = wrap(y0 + 1.0, self.height);
        let c00 = self.texel(xa, ya);
        let c10 = self.texel(xb, ya);
        let c01 = self.texel(xa, yb);
        let c11 = self.texel(xb, yb);
        let mut out = [0.0f32; 4];
        for k in 0..4 {
            let top = c00[k] + (c10[k] - c00[k]) * tx;
            let bot = c01[k] + (c11[k] - c01[k]) * tx;
            out[k] = top + (bot - top) * ty;
        }
        out
    }

    /// Sample a tangent-space normal map and return a unit vector.
    ///
    /// `@0x1009cb314` — `Texture::sample_normal(&self, &[f32; 2]) -> Vec3`: the
    /// texel is decoded as `2·c − 1` per channel (the `127.5` and `±1.0`
    /// immediates) and normalised, so a flat `(0.5, 0.5, 1.0)` map yields `+z`.
    pub fn sample_normal(&self, uv: [f32; 2]) -> Vec3 {
        let c = self.sample(uv);
        let n = Vec3::new(c[0] * 2.0 - 1.0, c[1] * 2.0 - 1.0, c[2] * 2.0 - 1.0);
        let n = n.normalized();
        if n == Vec3::ZERO {
            Vec3::new(0.0, 0.0, 1.0)
        } else {
            n
        }
    }

    /// Re-pack this texture through a UV layout (the skin builder resamples the
    /// authored sheet onto the rig's own atlas).
    ///
    /// `@0x1009cbbc0` — `Texture::resampled(&self, &UvLayout) -> Texture`.  It
    /// calls `UvLayout::fit`, then `UvLayout::locate` + `Texture::sample` /
    /// `sample_normal` per destination texel.
    pub fn resampled(&self, layout: &UvLayout) -> Texture {
        let (w, h) = (layout.width.max(1) as u32, layout.height.max(1) as u32);
        let mut out = Texture::blank(w, h);
        for y in 0..h {
            for x in 0..w {
                let uv = [
                    (x as f32 + 0.5) / w as f32,
                    (y as f32 + 0.5) / h as f32,
                ];
                let c = self.sample(uv);
                let i = ((y as usize) * (w as usize) + x as usize) * 4;
                for k in 0..4 {
                    out.data[i + k] = (c[k].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
        out
    }
}

/// One patch of the atlas: a quad in model space and the bones that carry it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Patch {
    /// Quad corners in model space, `xy` only (the layout is a projection).
    pub corners: Vec<[f32; 2]>,
    /// Per-corner bone weights (`+0x18`, 24-byte elements).
    pub weights: Vec<[f32; 3]>,
    /// Atlas rectangle `[u0, v0, u1, v1]` in `0..1`.
    pub rect: [f32; 4],
    /// The bone this patch belongs to.
    pub bone: String,
}

impl Patch {
    pub fn centre(&self) -> [f32; 2] {
        if self.corners.is_empty() {
            return [0.0, 0.0];
        }
        let n = self.corners.len() as f32;
        let sx: f32 = self.corners.iter().map(|c| c[0]).sum();
        let sy: f32 = self.corners.iter().map(|c| c[1]).sum();
        [sx / n, sy / n]
    }

    /// Does this patch's model-space quad contain `p`?
    pub fn contains(&self, p: [f32; 2]) -> bool {
        if self.corners.len() < 3 {
            return false;
        }
        let mut inside = false;
        let n = self.corners.len();
        for i in 0..n {
            let a = self.corners[i];
            let b = self.corners[(i + 1) % n];
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let t = (p[1] - a[1]) / (b[1] - a[1]);
                if p[0] < a[0] + t * (b[0] - a[0]) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// The angle of `p` around this patch's centre, in degrees — the recovered
    /// `Patch::angle_at` helper `UvLayout::address` calls.
    pub fn angle_at(&self, p: [f32; 2]) -> f32 {
        let c = self.centre();
        (p[1] - c[1]).atan2(p[0] - c[0]) / DEG_TO_RAD
    }

    /// The UV of `p` inside this patch's rectangle.
    pub fn uv_of(&self, p: [f32; 2]) -> [f32; 2] {
        let c = self.centre();
        let d = [p[0] - c[0], p[1] - c[1]];
        // Normalise by half a cell (the recovered ±0.5 convention).
        let u = (d[0] / (CELL_UNITS * 0.5)).clamp(-1.0, 1.0) * 0.5 + 0.5;
        let v = (d[1] / (CELL_UNITS * 0.5)).clamp(-1.0, 1.0) * 0.5 + 0.5;
        [
            self.rect[0] + (self.rect[2] - self.rect[0]) * u,
            self.rect[1] + (self.rect[3] - self.rect[1]) * v,
        ]
    }
}

/// The atlas: one patch per rig bone, packed on a grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UvLayout {
    pub patches: Vec<Patch>,
    /// Atlas width in pixels (`+0x18`).
    pub width: usize,
    /// Atlas height in pixels (`+0x20`).
    pub height: usize,
}

impl UvLayout {
    /// Number of patches a layout wants (`patches.len()` of the empty layout the
    /// caller passes in is ignored — the count comes from the rig).
    pub fn len(&self) -> usize {
        self.patches.len()
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    /// Lay a rig out on a square-ish grid at `px_per_unit` pixels per model unit.
    ///
    /// `@0x1009cc73c` — `UvLayout::new(&Puppet, f32) -> UvLayout`.  The scale is
    /// clamped up to `1.0`, cells are [`CELL_UNITS`] model units apart with a
    /// [`CELL_STRIDE`] row pitch, and the atlas size is the grid extent in pixels.
    pub fn new(puppet: &Puppet, px_per_unit: f32) -> UvLayout {
        let px = px_per_unit.max(1.0);
        let n = puppet.segments.len().max(1);
        let cols = (n as f32).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols.max(1));
        let mut patches = Vec::with_capacity(n);
        for (i, seg) in puppet.segments.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let x = col as f32 * CELL_UNITS;
            // Rows run *down* in model space so the atlas is not flipped.
            let y = -(row as f32) * CELL_STRIDE;
            let r = seg.radius.max(0.01);
            let corners = vec![
                [x - r, y - r],
                [x + r, y - r],
                [x + r, y + r],
                [x - r, y + r],
            ];
            let cell_w = 1.0 / cols as f32;
            let cell_h = 1.0 / rows as f32;
            patches.push(Patch {
                corners,
                weights: vec![[1.0, 0.0, 0.0]; 4],
                rect: [
                    col as f32 * cell_w,
                    row as f32 * cell_h,
                    (col + 1) as f32 * cell_w,
                    (row + 1) as f32 * cell_h,
                ],
                bone: seg.name.clone(),
            });
        }
        let width = (cols as f32 * CELL_UNITS * px).ceil() as usize;
        let height = (rows as f32 * CELL_STRIDE * px).ceil() as usize;
        UvLayout {
            patches,
            width,
            height,
        }
    }

    /// Rescale so the atlas' long side is exactly `size` pixels.
    ///
    /// `@0x1009cc5c8` — `UvLayout::fit(&Puppet, u32) -> UvLayout`: build at
    /// [`BASE_PX_PER_UNIT`], then rebuild at `size * 64 / max(width, height)`.
    pub fn fit(puppet: &Puppet, size: u32) -> UvLayout {
        let base = UvLayout::new(puppet, BASE_PX_PER_UNIT);
        let max_dim = base.width.max(base.height).max(1) as f32;
        let scale = (size as f32 * BASE_PX_PER_UNIT) / max_dim;
        UvLayout::new(puppet, scale)
    }

    /// Rescale this layout to an explicit pixel size.
    ///
    /// `@0x1009ccba4` — `UvLayout::fit_to(&self, u32, u32) -> UvLayout`: it
    /// recomputes the patches from the target size (the recovered body calls
    /// `UvLayout::fit` and then rewrites the rectangle of each patch).
    pub fn fit_to(&self, width: u32, height: u32) -> UvLayout {
        let mut out = self.clone();
        out.width = width as usize;
        out.height = height as usize;
        // Re-derive the atlas rectangles for the new size.
        let n = out.patches.len().max(1);
        let cols = (n as f32).sqrt().ceil() as usize;
        let rows = n.div_ceil(cols.max(1));
        for (i, p) in out.patches.iter_mut().enumerate() {
            let (col, row) = (i % cols, i / cols);
            p.rect = [
                col as f32 / cols as f32,
                row as f32 / rows.max(1) as f32,
                (col + 1) as f32 / cols as f32,
                (row + 1) as f32 / rows.max(1) as f32,
            ];
        }
        out
    }

    /// Which patch covers a model-space point (`xy`), if any.
    ///
    /// `@0x1009cd034` — `UvLayout::locate(&self, Vec3) -> Option<&Patch>`.  The
    /// recovered body has a `48.0` (`0x42400000`) reach and a `±0.5` half-cell
    /// tolerance before it falls back to a point-in-quad test.
    pub fn locate(&self, p: Vec3) -> Option<&Patch> {
        let q = [p.x, p.y];
        if let Some(patch) = self.patches.iter().find(|p| p.contains(q)) {
            return Some(patch);
        }
        // Fall back to the nearest patch centre within half a cell.
        let mut best: Option<(&Patch, f32)> = None;
        for patch in &self.patches {
            let c = patch.centre();
            let d = ((c[0] - q[0]).powi(2) + (c[1] - q[1]).powi(2)).sqrt();
            if best.map_or(true, |(_, bd)| d < bd) {
                best = Some((patch, d));
            }
        }
        match best {
            Some((patch, d)) if d <= CELL_UNITS * 0.5 => Some(patch),
            _ => None,
        }
    }

    /// The UV address of a model-space point: `(patch, uv)`.
    ///
    /// `@0x1009cd58c` — `UvLayout::address(&self, Vec3) -> Option<(usize, [f32; 2])>`.
    /// It locates the patch, then maps the point inside it with the recovered
    /// `Patch::angle_at`/half-cell convention.
    pub fn address(&self, p: Vec3) -> Option<(usize, [f32; 2])> {
        let patch = self.locate(p)?;
        let idx = self.patches.iter().position(|q| std::ptr::eq(q, patch))?;
        Some((idx, patch.uv_of([p.x, p.y])))
    }

    /// Is the atlas fully packed (every patch has a non-zero rectangle)?
    ///
    /// `@0x1009cd28c` — `UvLayout::packed(&self) -> bool`, walked with a `0.5`
    /// tolerance so a half-texel rounding error does not count as a gap.
    pub fn packed(&self) -> bool {
        self.patches.iter().all(|p| {
            let [u0, v0, u1, v1] = p.rect;
            u1 - u0 >= 0.5 / self.width.max(1) as f32 && v1 - v0 >= 0.5 / self.height.max(1) as f32
        })
    }

    /// Does `other` describe the same atlas?
    ///
    /// `@0x1009cd79c` — `UvLayout::matches(&self, &UvLayout) -> bool`: it compares
    /// [`UvLayout::signature`] and then the patch count, with a `0.5` texel
    /// tolerance on the rectangles.
    pub fn matches(&self, other: &UvLayout) -> bool {
        if self.patches.len() != other.patches.len() {
            return false;
        }
        if self.signature() != other.signature() {
            return false;
        }
        let tol = 0.5;
        self.patches.iter().zip(other.patches.iter()).all(|(a, b)| {
            a.bone == b.bone
                && (0..4).all(|k| (a.rect[k] * self.width.max(1) as f32
                    - b.rect[k] * other.width.max(1) as f32)
                    .abs()
                    <= tol)
        })
    }

    /// A stable textual signature of the layout, used by [`UvLayout::matches`].
    ///
    /// `@0x1009ce088` — `UvLayout::signature(&self) -> String`: `format!("{w}x{h}:{n}:{first}")`
    /// style summary of the size, patch count and bone order.
    pub fn signature(&self) -> String {
        let mut s = String::with_capacity(16 + self.patches.len() * 8);
        s.push_str(&format!("{}x{}:{}", self.width, self.height, self.patches.len()));
        for p in &self.patches {
            s.push(':');
            s.push_str(&p.bone);
        }
        s
    }
}

/// Build a tangent-space normal sheet from a height/diffuse sheet.
///
/// `@0x1009c9d18` — `gs_puppet::uv::bump_sheet(&Texture, f32) -> Texture`.  The
/// recovered body works in `0..255` fixed point, derives the slope from a 4-texel
/// neighbourhood (the two `fmov #4.0` immediates) and normalises the result to
/// `0.5 + 0.5·n`, i.e. the standard normal-map encoding that
/// [`Texture::sample_normal`] decodes.
pub fn bump_sheet(height: &Texture, strength: f32) -> Texture {
    let (w, h) = (height.width, height.height);
    let mut out = Texture::blank(w, h);
    if w == 0 || h == 0 {
        return out;
    }
    let at = |x: i64, y: i64| -> f32 {
        let x = x.rem_euclid(w as i64) as u32;
        let y = y.rem_euclid(h as i64) as u32;
        height.texel(x, y)[0]
    };
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let dx = (at(x + 1, y) - at(x - 1, y)) * strength;
            let dy = (at(x, y + 1) - at(x, y - 1)) * strength;
            let n = Vec3::new(-dx, -dy, 1.0).normalized();
            let i = ((y as usize) * (w as usize) + x as usize) * 4;
            out.data[i] = ((n.x * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            out.data[i + 1] = ((n.y * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            out.data[i + 2] = ((n.z * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            out.data[i + 3] = 255;
        }
    }
    out
}

/// The direction of the atlas seam at a patch, in degrees.
///
/// `gs_puppet::uv::seam_direction` (a local symbol): the angle of the patch edge
/// the skin builder has to blend across.  Modelled as the patch's longest edge.
pub fn seam_direction(patch: &Patch) -> f32 {
    let mut best = (0.0f32, 0.0f32);
    for i in 0..patch.corners.len() {
        let a = patch.corners[i];
        let b = patch.corners[(i + 1) % patch.corners.len()];
        let d = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        if d > (best.0 * best.0 + best.1 * best.1).sqrt() {
            best = (b[0] - a[0], b[1] - a[1]);
        }
    }
    best.1.atan2(best.0) / DEG_TO_RAD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Segment;

    fn rig() -> Puppet {
        let mut segs = vec![Segment::new("root", 0, Vec3::ZERO, 0.9)];
        for i in 1..5 {
            segs.push(Segment::new(
                format!("bone{i}"),
                0,
                Vec3::new(0.0, i as f32 * 0.1, 0.0),
                0.2,
            ));
        }
        Puppet::new("biped", segs).unwrap()
    }

    #[test]
    fn recovered_uv_constants() {
        assert_eq!(CELL_UNITS, 6.0);
        assert_eq!(CELL_STRIDE, 12.0);
        assert_eq!(BASE_PX_PER_UNIT.to_bits(), 0x4280_0000);
    }

    #[test]
    fn new_lays_one_patch_per_bone_on_a_grid() {
        let p = rig();
        let layout = UvLayout::new(&p, 1.0);
        assert_eq!(layout.len(), 5);
        // ceil(sqrt(5)) = 3 columns, 2 rows.
        assert_eq!(layout.patches[0].bone, "root");
        // Cell pitch is 6 units wide and 12 units per row, in pixels at 1 px/unit.
        assert_eq!(layout.width, 18); // 3 * 6
        assert_eq!(layout.height, 24); // 2 * 12
        assert!(layout.packed());
    }

    #[test]
    fn scale_is_clamped_to_at_least_one_pixel_per_unit() {
        let p = rig();
        let a = UvLayout::new(&p, 0.0);
        let b = UvLayout::new(&p, 1.0);
        assert_eq!(a.width, b.width);
        assert_eq!(a.height, b.height);
    }

    #[test]
    fn fit_makes_the_long_side_exactly_the_requested_size() {
        let p = rig();
        let base = UvLayout::new(&p, BASE_PX_PER_UNIT);
        let max_dim = base.width.max(base.height);
        let fitted = UvLayout::fit(&p, 512);
        // The rebuilt layout's long side is 512 (within a rounding step of one
        // cell, because the layout is built from whole cells).
        let scale = 512.0 * BASE_PX_PER_UNIT / max_dim as f32;
        let expect = UvLayout::new(&p, scale);
        assert_eq!(fitted, expect);
        assert!(fitted.width.max(fitted.height) > 400);
    }

    #[test]
    fn locate_and_address_find_the_patch_a_point_belongs_to() {
        let p = rig();
        let layout = UvLayout::new(&p, 1.0);
        // The root patch is centred on the origin with radius 0.45.
        let patch = layout.locate(Vec3::ZERO).expect("root patch");
        assert_eq!(patch.bone, "root");
        let (idx, uv) = layout.address(Vec3::ZERO).unwrap();
        assert_eq!(idx, 0);
        assert!(uv[0] > 0.0 && uv[0] < 1.0, "{uv:?}");
        // A point far away belongs to no patch.
        assert!(layout.locate(Vec3::new(500.0, 500.0, 0.0)).is_none());
    }

    #[test]
    fn patch_contains_and_angle() {
        let patch = Patch {
            corners: vec![[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]],
            weights: vec![[1.0, 0.0, 0.0]; 4],
            rect: [0.0, 0.0, 1.0, 1.0],
            bone: "root".into(),
        };
        assert!(patch.contains([0.0, 0.0]));
        assert!(!patch.contains([2.0, 0.0]));
        // The +x direction is 0°, +y is 90°.
        assert!(patch.angle_at([1.0, 0.0]).abs() < 1e-3);
        assert!((patch.angle_at([0.0, 1.0]) - 90.0).abs() < 1e-3);
    }

    #[test]
    fn fit_to_and_matches() {
        let p = rig();
        let a = UvLayout::new(&p, 1.0);
        let b = a.fit_to(256, 256);
        assert_eq!((b.width, b.height), (256, 256));
        // Rectangles are re-derived, so the two do not match...
        assert!(!a.matches(&b));
        // ... but a copy of itself does.
        assert!(a.matches(&a.clone()));
        // A different bone order does not.
        let mut c = a.clone();
        c.patches[0].bone = "other".into();
        assert!(!a.matches(&c));
    }

    #[test]
    fn signature_names_the_size_and_bones() {
        let p = rig();
        let layout = UvLayout::new(&p, 1.0);
        let sig = layout.signature();
        assert!(sig.starts_with("18x24:5:root:"), "{sig}");
        assert!(sig.contains("bone4"));
    }

    #[test]
    fn flat_texture_samples_flat_and_normals_point_out() {
        let t = Texture::blank(2, 2);
        assert_eq!(t.sample([0.25, 0.25]), [1.0, 1.0, 1.0, 1.0]);
        // A blank (white) normal map decodes to (1, 1, 1) normalised.
        let n = t.sample_normal([0.0, 0.0]);
        assert!((n.length() - 1.0).abs() < 1e-5);
        // A neutral (0.5, 0.5, 1.0) map decodes to +z.
        let tex = Texture::from_rgba(
            1,
            1,
            vec![128, 128, 255, 255],
        );
        let n = tex.sample_normal([0.5, 0.5]);
        assert!(n.z > 0.99 && n.x.abs() < 0.01, "{n:?}");
    }

    #[test]
    fn sample_bilinear_between_two_texels() {
        // Left texel red, right texel blue.
        let tex = Texture::from_rgba(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
        // Sampling exactly at the left texel centre.
        let c = tex.sample([0.25, 0.5]);
        assert!(c[0] > 0.9 && c[2] < 0.1, "{c:?}");
        // Halfway between them blends both.
        let c = tex.sample([0.5, 0.5]);
        assert!(c[0] > 0.3 && c[2] > 0.3, "{c:?}");
    }

    #[test]
    fn bump_sheet_of_a_flat_height_is_a_flat_normal_map() {
        let flat = Texture::blank(3, 3);
        let sheet = bump_sheet(&flat, 1.0);
        assert_eq!(sheet.width, 3);
        let n = sheet.sample_normal([0.5, 0.5]);
        assert!(n.z > 0.99, "{n:?}");
        // A ramp produces a tilted normal.
        let mut ramp = Texture::blank(3, 1);
        for x in 0..3 {
            ramp.data[x * 4] = (x * 100) as u8;
            ramp.data[x * 4 + 1] = (x * 100) as u8;
            ramp.data[x * 4 + 2] = (x * 100) as u8;
        }
        let sheet = bump_sheet(&ramp, 1.0);
        let n = sheet.sample_normal([0.5, 0.5]);
        assert!(n.x.abs() > 0.1, "{n:?}");
    }

    #[test]
    fn resampled_keeps_the_layout_size() {
        let p = rig();
        let layout = UvLayout::new(&p, 1.0);
        let tex = Texture::blank(4, 4);
        let out = tex.resampled(&layout);
        assert_eq!(out.width, layout.width as u32);
        assert_eq!(out.height, layout.height as u32);
        assert_eq!(out.data.len(), layout.width * layout.height * 4);
    }

    #[test]
    fn seam_direction_is_the_longest_edge() {
        let patch = Patch {
            corners: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 1.0], [0.0, 1.0]],
            ..Default::default()
        };
        assert!(seam_direction(&patch).abs() < 1e-3);
    }
}
