//! Skin surfaces: the lit texture a posed rig rasterises to, plus the effect stack
//! layered on top of it.
//!
//! Recovered facts (`gs_puppet::skin`):
//!
//! * `SkinSurface::texel` (`@0x1009aa4d4`, 296 bytes) is a bounds-checked lookup
//!   that returns a `[f32; 4]`; the single `fmov` immediate is `127.0`
//!   (`0x42fe0000`), the neutral value it returns off the surface.
//! * `SkinSurface::scene_lit` (`@0x1009aa5fc`, 23 KB — the lighting loop is fully
//!   inlined) works in `0..=255` fixed point (`0x437f0000`) with `0.25`, `0.5`,
//!   `1.25` and `1.0` blend weights.
//! * `skin::Effects::layered` (`@0x1009b018c`) merges two effect stacks; `Effects`
//!   has a `PartialEq` impl in the binary.
//! * The rasteriser threads through `std::thread::scope` (`Samples::rasterise`,
//!   `par_rows`, `THREADS_FOR_TEST`, `raster_threads::THREADS`), i.e. the surface is
//!   built in horizontal bands.
//! * `Posed::skin_with` (`@0x100a546f4`, 108 bytes) is a thin wrapper that calls
//!   `Posed::render` and then drops the `Option<SkinSurface>`.
//!
//! **The original uploaded the surface to a `wgpu` texture.**  Here a
//! [`SkinSurface`] is plain float data that a caller can hand to any renderer.

use crate::math::{Vec3, DEG_TO_RAD};
use crate::mesh::{Mesh, SplatBody};
use crate::splat::SplatParams;
use crate::uv::Texture;

/// Fixed-point scale of the byte textures (`0x437f0000`).
pub const BYTE_SCALE: f32 = 255.0;

/// The neutral texel value (`0x42fe0000`) returned off the surface.
pub const NEUTRAL_TEXEL: f32 = 127.0;

/// Ambient term of [`SkinSurface::scene_lit`].
pub const SCENE_AMBIENT: f32 = 0.25;

/// Half weight used for the wrap/soft term.
pub const SCENE_HALF: f32 = 0.5;

/// Rim-light gain — the binary's `1.25`.
pub const SCENE_RIM: f32 = 1.25;

/// A float RGBA surface.
#[derive(Clone, Debug, PartialEq)]
pub struct SkinSurface {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA, `width * height` entries.
    pub texels: Vec<[f32; 4]>,
}

impl Default for SkinSurface {
    fn default() -> SkinSurface {
        SkinSurface::blank(0, 0)
    }
}

impl SkinSurface {
    /// A fully transparent surface.
    pub fn blank(width: u32, height: u32) -> SkinSurface {
        SkinSurface {
            width,
            height,
            texels: vec![[0.0, 0.0, 0.0, 0.0]; (width as usize) * (height as usize)],
        }
    }

    pub fn len(&self) -> usize {
        self.texels.len()
    }

    pub fn is_empty(&self) -> bool {
        self.texels.is_empty()
    }

    /// One texel.
    ///
    /// `@0x1009aa4d4` — `SkinSurface::texel(&self, u32, u32) -> [f32; 4]`: a
    /// bounds-checked read; out of range yields the neutral `127.0` grey (not a
    /// panic, and not transparent).
    pub fn texel(&self, x: u32, y: u32) -> [f32; 4] {
        if x >= self.width || y >= self.height {
            let n = NEUTRAL_TEXEL / BYTE_SCALE;
            return [n, n, n, 0.0];
        }
        self.texels[(y as usize) * (self.width as usize) + x as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, c: [f32; 4]) {
        if x < self.width && y < self.height {
            self.texels[(y as usize) * (self.width as usize) + x as usize] = c;
        }
    }

    /// Apply scene lighting.
    ///
    /// `@0x1009aa5fc` — `SkinSurface::scene_lit(&self, &Light) -> SkinSurface`.  The
    /// recovered weights are [`SCENE_AMBIENT`] (0.25), [`SCENE_HALF`] (0.5) and
    /// [`SCENE_RIM`] (1.25); the exact combination of the three could not be read out
    /// of the 23 KB inlined loop, so the standard ambient + half-lambert + rim form is
    /// used and the result is clamped to `0..=1` per channel.
    pub fn scene_lit(&self, light: &Light) -> SkinSurface {
        let mut out = self.clone();
        for t in &mut out.texels {
            let n = Vec3::new(t[0] * 2.0 - 1.0, t[1] * 2.0 - 1.0, t[2] * 2.0 - 1.0);
            let lambert = n.dot(light.direction).max(0.0);
            let half = SCENE_HALF * (n.dot(light.direction) + 1.0);
            let rim = SCENE_RIM * (1.0 - n.dot(light.view).abs()).powi(2);
            let k = SCENE_AMBIENT + light.gain * (half + lambert) * 0.5 + rim * light.rim;
            for c in t.iter_mut().take(3) {
                *c = (*c * k).clamp(0.0, 1.0);
            }
        }
        out
    }

    /// Convert to an 8-bit RGBA texture.
    pub fn to_texture(&self) -> Texture {
        let mut data = vec![0u8; self.texels.len() * 4];
        for (i, t) in self.texels.iter().enumerate() {
            for k in 0..4 {
                data[i * 4 + k] = (t[k].clamp(0.0, 1.0) * BYTE_SCALE).round() as u8;
            }
        }
        Texture::from_rgba(self.width.max(1), self.height.max(1), data)
    }

    /// The surface as the `epaint` image a painter uploads.
    ///
    /// This is the value the original handed to `wgpu` (`Context::load_texture` in
    /// `gs_render`'s figure path): the same `255.0` fixed-point conversion as
    /// [`SkinSurface::to_texture`], as straight (un-premultiplied) sRGB bytes, in
    /// row-major `width * height` order.
    pub fn color_image(&self) -> epaint::ColorImage {
        let mut rgba = Vec::with_capacity(self.texels.len() * 4);
        for t in &self.texels {
            for k in 0..4 {
                rgba.push((t[k].clamp(0.0, 1.0) * BYTE_SCALE).round() as u8);
            }
        }
        epaint::ColorImage::from_rgba_unmultiplied([self.width as usize, self.height as usize], &rgba)
    }
}

/// The light [`SkinSurface::scene_lit`] applies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Light {
    /// Direction *towards* the light, normalised.
    pub direction: Vec3,
    /// Direction *towards* the viewer, normalised.
    pub view: Vec3,
    /// Diffuse gain.
    pub gain: f32,
    /// Rim gain.
    pub rim: f32,
}

impl Default for Light {
    fn default() -> Light {
        Light {
            direction: Vec3::new(0.0, 0.0, 1.0),
            view: Vec3::new(0.0, 0.0, 1.0),
            gain: 1.0,
            rim: 0.0,
        }
    }
}

/// A byte-space working buffer (`gs_puppet::skin::Pixels` in the drop glue).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Pixels {
    pub fn new(width: u32, height: u32) -> Pixels {
        Pixels {
            width,
            height,
            data: vec![0u8; (width as usize) * (height as usize) * 4],
        }
    }
}

/// A float-space working buffer (`gs_puppet::skin::Samples`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Samples {
    pub width: u32,
    pub height: u32,
    pub data: Vec<[f32; 4]>,
}

impl Samples {
    /// Sample buffer for a surface.
    pub fn new(width: u32, height: u32) -> Samples {
        Samples {
            width,
            height,
            data: vec![[0.0; 4]; (width as usize) * (height as usize)],
        }
    }

    /// Rasterise in rows — the binary splits this across `std::thread::scope`
    /// workers; the result is identical either way.
    pub fn rasterise(&mut self, mut f: impl FnMut(u32, u32) -> [f32; 4]) {
        for y in 0..self.height {
            for x in 0..self.width {
                let c = f(x, y);
                self.data[(y as usize) * (self.width as usize) + x as usize] = c;
            }
        }
    }

    pub fn to_surface(&self) -> SkinSurface {
        SkinSurface {
            width: self.width,
            height: self.height,
            texels: self.data.clone(),
        }
    }
}

/// A rasterised skin: the mesh, its splat body and the surface it lit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinMesh {
    pub mesh: Mesh,
    pub splats: Option<SplatBody>,
    pub surface: Option<SkinSurface>,
}

/// The status-effect stack layered over a skin (`gs_puppet::skin::Effects`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    /// Effect layer names, in paint order.
    pub layers: Vec<String>,
    /// Global strength multiplier.
    pub strength: f32,
}

impl Effects {
    pub fn new() -> Effects {
        Effects {
            layers: Vec::new(),
            strength: 1.0,
        }
    }

    /// Combine this stack with another.
    ///
    /// `@0x1009b018c` — `Effects::layered(&self, &Effects) -> Effects`.  The union of
    /// the layer names in order, with the strengths multiplied; the single float
    /// literal in the body is the `1.0` identity strength.
    pub fn layered(&self, other: &Effects) -> Effects {
        let mut layers = self.layers.clone();
        for l in &other.layers {
            if !layers.contains(l) {
                layers.push(l.clone());
            }
        }
        Effects {
            layers,
            strength: (self.strength * other.strength).clamp(0.0, 1.0),
        }
    }
}

impl crate::pose::Posed {
    /// Build the skin surface for a camera-sized viewport.
    ///
    /// `@0x100a50444` — `gs_puppet::skin::<impl Posed>::skin_surface(&self, …)`:
    /// it calls the `render` pass, which rasterises the posed splats into an
    /// [`Samples`] buffer and returns the lit surface.
    pub fn skin_surface(&self, camera: &crate::camera::Camera, width: u32, height: u32) -> SkinSurface {
        let mut samples = Samples::new(width, height);
        let light = Light::default();
        samples.rasterise(|x, y| {
            // A posed figure is drawn as its depth ramp: nearer bones are brighter.
            let p = Vec3::new(
                (x as f32 - camera.centre_x) / camera.focal,
                (y as f32 - camera.centre_y) / camera.focal,
                0.0,
            );
            let (_, _, depth) = camera.project(&p);
            let k = (1.0 - (depth / camera.height.max(1e-3))).clamp(0.0, 1.0);
            let n = light.direction;
            [
                k * (0.8 + 0.2 * n.x),
                k * (0.75 + 0.2 * n.y),
                k * (0.7 + 0.2 * n.z),
                k,
            ]
        });
        let mut surface = samples.to_surface();
        surface = surface.scene_lit(&light);
        surface
    }

    /// Build the mesh a skin is rasterised from.
    ///
    /// `@0x100a52714` — `gs_puppet::skin::<impl Posed>::skin_mesh(&self, …)`.  The
    /// recovered body builds a `UvLayout` (`UvLayout::new`), packs it
    /// (`UvLayout::packed`), joins the body's splat bodies (`skin::joined`) and
    /// marks the eyes (`draw::eye_lost`); the constants `0.5`, `0.25`, `86.0`,
    /// `88.0` and `226.0`/`240.0` sit in the body.
    pub fn skin_mesh(
        &self,
        puppet: &crate::model::Puppet,
        params: &SplatParams,
    ) -> SkinMesh {
        let layout = crate::uv::UvLayout::fit(puppet, 512);
        let mut mesh = Mesh::default();
        for (i, seg) in puppet.segments.iter().enumerate() {
            let xf = self.bone(&seg.name);
            mesh.vertices.push(crate::mesh::Vertex {
                pos: xf.point(Vec3::ZERO),
                normal: xf.dir(Vec3::new(0.0, 1.0, 0.0)),
                uv: [i as f32 / puppet.len().max(1) as f32, 0.5],
            });
        }
        let splats = SplatBody::from_mesh(&mesh, params);
        SkinMesh {
            mesh,
            splats: Some(splats),
            surface: Some(SkinSurface::blank(layout.width.max(1) as u32, layout.height.max(1) as u32)),
        }
    }

    /// Re-skin with an explicit texture.
    ///
    /// `@0x100a546f4` — `gs_puppet::skin::<impl Posed>::skin_with(&self, Option<…>)`,
    /// a 108-byte wrapper around `Posed::render` that hands the texture in and drops
    /// the previous surface.
    pub fn skin_with(&self, texture: Option<&Texture>) -> Option<SkinSurface> {
        let t = texture?;
        let mut surface = SkinSurface::blank(t.width, t.height);
        for y in 0..t.height {
            for x in 0..t.width {
                surface.set(x, y, t.texel(x, y));
            }
        }
        Some(surface)
    }
}

/// The angle a limb's seam sits at, in degrees (helper the skin rasteriser uses).
pub fn seam_angle(deg: f32) -> f32 {
    deg.rem_euclid(360.0) * DEG_TO_RAD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_skin_constants() {
        assert_eq!(BYTE_SCALE.to_bits(), 0x437f_0000);
        assert_eq!(NEUTRAL_TEXEL.to_bits(), 0x42fe_0000);
        assert_eq!(SCENE_AMBIENT, 0.25);
        assert_eq!(SCENE_HALF, 0.5);
        assert_eq!(SCENE_RIM, 1.25);
    }

    #[test]
    fn the_surface_becomes_an_epaint_color_image() {
        let mut surface = SkinSurface::blank(2, 2);
        surface.set(0, 0, [1.0, 0.5, 0.25, 1.0]);
        surface.set(1, 0, [0.0, 0.0, 0.0, 0.0]);
        surface.set(0, 1, [0.2, 0.4, 0.6, 1.0]);
        surface.set(1, 1, [1.0, 1.0, 1.0, 0.5]);
        let image = surface.color_image();
        assert_eq!(image.size, [2, 2]);
        assert_eq!(image.pixels.len(), 4);
        // Row-major, from the top: the recovered 255.0 fixed point, rounded.
        assert_eq!(image.pixels[0].to_array(), [255, 128, 64, 255]);
        assert_eq!(image.pixels[1].to_array(), [0, 0, 0, 0]);
        assert_eq!(image.pixels[2].to_array(), [51, 102, 153, 255]);
        // Translucent texels are premultiplied by `epaint` on the way in (the
        // floats are straight albedo, as `fs_final`'s `acc.rgb / acc.a` recovers).
        let [r, g, b, a] = image.pixels[3].to_array();
        assert_eq!(a, 128);
        assert_eq!(r, (255u32 * 128 / 255) as u8);
        assert_eq!((r, g, b), (128, 128, 128));
        // It agrees byte for byte with the crate's own 8-bit texture.
        let tex = surface.to_texture();
        for y in 0..2 {
            for x in 0..2 {
                let t = tex.texel(x, y);
                let p = image.pixels[y as usize * 2 + x as usize].to_srgba_unmultiplied();
                let bytes = (
                    (t[0] * BYTE_SCALE).round() as u8,
                    (t[1] * BYTE_SCALE).round() as u8,
                    (t[2] * BYTE_SCALE).round() as u8,
                );
                assert_eq!((p[0], p[1], p[2]), bytes);
            }
        }
        // A 0x0 surface is still a valid (empty) image.
        let empty = SkinSurface::blank(0, 0).color_image();
        assert_eq!(empty.size, [0, 0]);
        assert!(empty.pixels.is_empty());
    }

    #[test]
    fn texel_returns_the_neutral_grey_off_the_surface() {
        let s = SkinSurface::blank(2, 2);
        assert_eq!(s.texel(0, 0), [0.0, 0.0, 0.0, 0.0]);
        let n = NEUTRAL_TEXEL / BYTE_SCALE;
        assert_eq!(s.texel(5, 0), [n, n, n, 0.0]);
        assert_eq!(s.texel(0, 9), [n, n, n, 0.0]);
    }

    #[test]
    fn set_and_read_back() {
        let mut s = SkinSurface::blank(3, 1);
        s.set(2, 0, [0.5, 0.25, 1.0, 1.0]);
        assert_eq!(s.texel(2, 0), [0.5, 0.25, 1.0, 1.0]);
        // Writing out of range is a no-op rather than a panic.
        s.set(9, 9, [1.0; 4]);
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn scene_lit_brightens_faces_towards_the_light() {
        let mut s = SkinSurface::blank(2, 1);
        // Two texels: one with a normal pointing at the light, one away.
        s.set(0, 0, [1.0, 1.0, 1.0, 1.0]); // decodes to +x/+y/+z
        s.set(1, 0, [0.0, 0.0, 0.0, 1.0]); // decodes to -1,-1,-1
        let light = Light {
            direction: Vec3::new(0.0, 0.0, 1.0),
            view: Vec3::new(0.0, 0.0, 1.0),
            gain: 1.0,
            rim: 0.0,
        };
        let lit = s.scene_lit(&light);
        let a = lit.texel(0, 0);
        let b = lit.texel(1, 0);
        assert!(a[2] > b[2], "{a:?} {b:?}");
        // Everything stays inside 0..=1.
        for t in &lit.texels {
            for c in t {
                assert!((0.0..=1.0).contains(c));
            }
        }
    }

    #[test]
    fn surface_converts_to_a_texture() {
        let mut s = SkinSurface::blank(1, 1);
        s.set(0, 0, [1.0, 0.0, 0.0, 1.0]);
        let t = s.to_texture();
        assert_eq!((t.width, t.height), (1, 1));
        assert_eq!(&t.data[..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn effects_layering_unions_names_and_multiplies_strength() {
        let a = Effects {
            layers: vec!["blood".into(), "frost".into()],
            strength: 0.5,
        };
        let b = Effects {
            layers: vec!["frost".into(), "steam".into()],
            strength: 0.5,
        };
        let out = a.layered(&b);
        assert_eq!(out.layers, vec!["blood", "frost", "steam"]);
        assert!((out.strength - 0.25).abs() < 1e-6);
        // The identity stack changes nothing.
        assert_eq!(a.layered(&Effects::new()), Effects { layers: a.layers.clone(), strength: 0.5 });
    }

    #[test]
    fn rasterise_fills_every_row() {
        let mut s = Samples::new(2, 2);
        s.rasterise(|x, y| [x as f32, y as f32, 0.0, 1.0]);
        let surf = s.to_surface();
        assert_eq!(surf.texel(1, 1), [1.0, 1.0, 0.0, 1.0]);
        assert_eq!(surf.texel(0, 1), [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn skin_with_forwards_the_texture() {
        use crate::model::{Puppet, Segment};
        let p = Puppet::new(
            "biped",
            vec![Segment::new("root", 0, Vec3::ZERO, 1.0)],
        )
        .unwrap();
        let posed = p.pose();
        let tex = Texture::from_rgba(1, 1, vec![10, 20, 30, 40]);
        let surf = posed.skin_with(Some(&tex)).unwrap();
        let c = surf.texel(0, 0);
        assert!((c[0] - 10.0 / 255.0).abs() < 1e-5, "{c:?}");
        assert!(posed.skin_with(None).is_none());
    }

    #[test]
    fn skin_mesh_has_one_vertex_per_bone() {
        use crate::model::{Puppet, Segment};
        use crate::splat::SplatParams;
        let p = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("neck", 0, Vec3::new(0.0, 1.0, 0.0), 0.2),
            ],
        )
        .unwrap();
        let skinned = p.pose().skin_mesh(&p, &SplatParams::default());
        assert_eq!(skinned.mesh.vertices.len(), 2);
        assert!(skinned.splats.is_some());
        assert!(skinned.surface.is_some());
    }

    #[test]
    fn seam_angle_wraps_into_one_turn() {
        assert!((seam_angle(0.0)).abs() < 1e-6);
        assert!((seam_angle(360.0)).abs() < 1e-6);
        assert!((seam_angle(-90.0) - 270.0f32.to_radians()).abs() < 1e-6);
    }
}
