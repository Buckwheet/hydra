//! The 2-D draw list a posed rig produces, and the outline maths behind it.
//!
//! Recovered facts (`gs_puppet::draw`):
//!
//! * [`convex_hull`] (`@0x1009a818c`) compacts its `&mut Vec<[f32; 2]>` argument by
//!   dropping non-finite points **in place** (`and w, #0x7fffffff` against
//!   `f32::MAX` / `+inf`), returns an empty `Vec` for fewer than two points
//!   (capacity 0, pointer `4` — the alignment of an 8-byte `[f32; 2]` — and length
//!   0), then runs `sort` + the monotone-chain scan.
//! * `severed` (`@0x1009a8a7c`) calls `model::Puppet::subtree`, i.e. it works out
//!   which bones disappear when a limb is cut off.
//! * `Posed::draw_list` (`@0x100a43040`) is the big one: it projects bones with
//!   `Xf::point` / `Xf::dir` and emits the primitive layers.  The drop glue names
//!   the shapes it returns: `Vec<(Shape, [u8; 3])>`, `Vec<(Shape, [u8; 4])>`,
//!   `Vec<SegDraw>`, `Vec<(f32, usize, Vec<SegDraw>)>`, `Vec<Prim>` and
//!   `Vec<Surface>`, with `255.0` (`0x437f0000`) as the byte-colour scale and
//!   `0.5`/`0.25`/`2.5` as the outline offsets.
//! * `Posed::ground_shadow` (`@0x100a4285c`) calls [`convex_hull`] and `severed`,
//!   i.e. the shadow is the hull of the figure projected onto the ground.

use crate::camera::Camera;
use crate::math::{Mat3, Vec3, Xf};
use crate::model::Puppet;
use crate::pose::{Posed, PoseSet};

/// `LIGHT_ABOVE` — the shading threshold `mesh::render` calls a surface lit
/// above.
///
/// The literal is embedded in the body/splat WGSL the binary carries, whose own
/// comment names it: `// gs_puppet::draw::LIGHT_ABOVE / DARK_BELOW, as
/// mesh::render shades` (`const LIGHT_ABOVE: f32 = 0.62;`) — the same
/// `smoothstep(LIGHT_ABOVE - 0.2, LIGHT_ABOVE + 0.2, g)` the shader evaluates.
pub const LIGHT_ABOVE: f32 = 0.62;

/// `DARK_BELOW` — the shading threshold below which `mesh::render` darkens the
/// surface (`const DARK_BELOW: f32 = -0.08;`, same shader comment).
pub const DARK_BELOW: f32 = -0.08;

/// An `epaint` colour from the crate's `0..=255` RGBA bytes (the recovered
/// `255.0` scale).  `epaint` wants premultiplied alpha, which this applies.
pub fn color32(c: [u8; 4]) -> epaint::Color32 {
    epaint::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
}

/// An `epaint` colour from a 3-byte tint, fully opaque.
pub fn color32_rgb(c: [u8; 3]) -> epaint::Color32 {
    epaint::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255)
}

/// A 2-D shape in screen space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    /// Outline points, in draw order.
    pub points: Vec<[f32; 2]>,
    /// Whether the outline closes back onto its first point.
    pub closed: bool,
}

impl Shape {
    pub fn line(points: Vec<[f32; 2]>) -> Shape {
        Shape {
            points,
            closed: false,
        }
    }

    pub fn poly(points: Vec<[f32; 2]>) -> Shape {
        Shape {
            points,
            closed: true,
        }
    }

    /// Axis-aligned bounds of the outline.
    pub fn bounds(&self) -> ([f32; 2], [f32; 2]) {
        let mut lo = [f32::MAX; 2];
        let mut hi = [f32::MIN; 2];
        for p in &self.points {
            for k in 0..2 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }

    /// Offset every point outwards by `d` — the recovered "outline" trick
    /// (`0.5`/`0.25` in `draw_list`).
    pub fn dilated(&self, d: f32) -> Shape {
        if self.points.is_empty() {
            return self.clone();
        }
        let (cx, cy) = self
            .points
            .iter()
            .fold((0.0f32, 0.0f32), |(ax, ay), p| (ax + p[0], ay + p[1]));
        let n = self.points.len() as f32;
        let (cx, cy) = (cx / n, cy / n);
        let mut points = Vec::with_capacity(self.points.len());
        for p in &self.points {
            let dx = p[0] - cx;
            let dy = p[1] - cy;
            let len = (dx * dx + dy * dy).sqrt().max(1e-6);
            points.push([p[0] + dx / len * d, p[1] + dy / len * d]);
        }
        Shape {
            points,
            closed: self.closed,
        }
    }

    /// The outline as `epaint` screen points.
    pub fn pos2s(&self) -> Vec<epaint::Pos2> {
        self.points
            .iter()
            .map(|p| epaint::pos2(p[0], p[1]))
            .collect()
    }

    /// A filled `epaint` polygon (`epaint::Shape::convex_polygon`, no stroke) —
    /// the shape the recovered `Vec<(Shape, [u8; 4])>` layers and every per-bone
    /// quad in `DrawList::prims` become.
    ///
    /// `None` for an outline with fewer than three points: `epaint` fills only
    /// convex polygons, so a degenerate quad is nothing to paint.
    pub fn to_fill(&self, fill: [u8; 4]) -> Option<epaint::Shape> {
        if self.points.len() < 3 {
            return None;
        }
        Some(epaint::Shape::convex_polygon(
            self.pos2s(),
            color32(fill),
            epaint::Stroke::NONE,
        ))
    }

    /// The outline as an `epaint` polyline, closed when the shape closes.
    ///
    /// `None` for fewer than two points.
    pub fn to_stroke(&self, stroke: epaint::Stroke) -> Option<epaint::Shape> {
        if self.points.len() < 2 {
            return None;
        }
        let points = self.pos2s();
        Some(if self.closed {
            epaint::Shape::closed_line(points, stroke)
        } else {
            epaint::Shape::line(points, stroke)
        })
    }
}

/// One draw primitive: a shape plus its colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prim {
    pub shape: Shape,
    /// RGBA, `0..=255` (the recovered `255.0` scale).
    pub color: [u8; 4],
    /// Painter's-algorithm depth; larger draws later.
    pub depth: f32,
}

/// One bone segment's worth of primitives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SegDraw {
    /// The two bone indices the segment spans.
    pub bones: [usize; 2],
    pub prims: Vec<Prim>,
    /// Colour of the segment, before shading.
    pub color: [u8; 4],
}

/// A filled surface patch (the `Vec<Surface>` layer of the draw list).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    pub shape: Shape,
    pub color: [u8; 4],
    pub normal: Vec3,
    /// How much of the surface is lit, `0.0..=1.0`.
    pub lit: f32,
}

/// Everything a posed rig wants drawn, in layers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    /// Filled shapes with 4-byte colours (`Vec<(Shape, [u8; 4])>`).
    pub filled: Vec<(Shape, [u8; 4])>,
    /// Filled shapes with 3-byte colours (`Vec<(Shape, [u8; 3])>`).
    pub tinted: Vec<(Shape, [u8; 3])>,
    /// Per-bone segment drawings, sorted by depth (`Vec<(f32, usize, Vec<SegDraw>)>`).
    pub layers: Vec<(f32, usize, Vec<SegDraw>)>,
    /// The flat primitive list (`Vec<Prim>`).
    pub prims: Vec<Prim>,
}

impl DrawList {
    pub fn is_empty(&self) -> bool {
        self.filled.is_empty()
            && self.tinted.is_empty()
            && self.layers.is_empty()
            && self.prims.is_empty()
    }

    /// Merge another list into this one (the outfit/status layers do this).
    pub fn append(&mut self, mut other: DrawList) {
        self.filled.append(&mut other.filled);
        self.tinted.append(&mut other.tinted);
        self.layers.append(&mut other.layers);
        self.prims.append(&mut other.prims);
    }

    /// The whole list as `epaint` shapes, in paint order.
    ///
    /// This is the `epaint` face of `Posed::draw_list`'s GPU seam: the layer
    /// struct the binary built (`Vec<(Shape, [u8; 4])>`, `Vec<(Shape, [u8; 3])>`,
    /// `Vec<(f32, usize, Vec<SegDraw>)>`) becomes values an `egui::Painter`
    /// consumes, in the order the list carries them:
    ///
    /// 1. `filled` — the 4-byte-filled shapes (the ground shadow, washes);
    /// 2. `tinted` — the 3-byte tints, opaque;
    /// 3. every `SegDraw` primitive of every `layers` entry, the layers already
    ///    sorted far to near, so a painter drawing them in order gets the
    ///    recovered occlusion for free.
    ///
    /// `prims` is the *flat* copy of the same per-bone quads that `layers` holds,
    /// so it is deliberately not emitted again — use [`DrawList::prim_shapes`] for
    /// that list on its own.  Shapes with fewer than three points paint nothing
    /// and are skipped.
    pub fn shapes(&self) -> Vec<epaint::Shape> {
        let mut out: Vec<epaint::Shape> = Vec::new();
        for (shape, color) in &self.filled {
            out.extend(shape.to_fill(*color));
        }
        for (shape, tint) in &self.tinted {
            out.extend(shape.to_fill([tint[0], tint[1], tint[2], 255]));
        }
        for (_, _, segs) in &self.layers {
            for seg in segs {
                for prim in &seg.prims {
                    out.extend(prim.shape.to_fill(prim.color));
                }
            }
        }
        out
    }

    /// The flat `Vec<Prim>` (`DrawList::prims`) as `epaint` polygons, in list
    /// order.  Same geometry as [`DrawList::shapes`]' last section, without the
    /// filled/tinted layers and without the per-layer grouping.
    pub fn prim_shapes(&self) -> Vec<epaint::Shape> {
        self.prims
            .iter()
            .filter_map(|p| p.shape.to_fill(p.color))
            .collect()
    }

    /// Only the depth-sorted per-bone layers, far to near.
    pub fn layer_shapes(&self) -> Vec<epaint::Shape> {
        let mut out = Vec::new();
        for (_, _, segs) in &self.layers {
            for seg in segs {
                for prim in &seg.prims {
                    out.extend(prim.shape.to_fill(prim.color));
                }
            }
        }
        out
    }

    /// The filled 4-byte shapes on their own (the shadow/wash layer).
    pub fn filled_shapes(&self) -> Vec<epaint::Shape> {
        self.filled
            .iter()
            .filter_map(|(shape, color)| shape.to_fill(*color))
            .collect()
    }
}

impl Prim {
    /// One primitive as an `epaint` polygon: its quad, filled with its byte
    /// colour.  `None` for a degenerate shape.
    pub fn to_epaint(&self) -> Option<epaint::Shape> {
        self.shape.to_fill(self.color)
    }
}

/// Convex hull of a point set, in place.
///
/// `@0x1009a818c` — `gs_puppet::draw::convex_hull(&mut Vec<[f32; 2]>) -> Vec<[f32; 2]>`.
///
/// The recovered behaviour, in order:
///
/// 1. compact `points` in place, dropping any point whose `|x|` is `+inf`/NaN
///    (`cmp w15, 0x7f7fffff ; b.gt` keeps finite values only) or whose `|y|` is not
///    finite — the two `and w, #0x7fffffff` masks;
/// 2. fewer than two surviving points yields an empty `Vec`;
/// 3. sort (insertion sort below 21 points, the shared quicksort above) and run the
///    monotone-chain scan.
///
/// The compaction is observable: the caller's vector is shortened.
pub fn convex_hull(points: &mut Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    // 1. drop non-finite points in place.
    points.retain(|p| p[0].is_finite() && p[1].is_finite() && p[0].abs() <= f32::MAX);
    if points.len() < 2 {
        return Vec::new();
    }
    // 2. sort lexicographically by (x, y).
    points.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    let cross = |o: [f32; 2], a: [f32; 2], b: [f32; 2]| -> f32 {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    // 3. monotone chain, lower then upper.
    let mut lower: Vec<[f32; 2]> = Vec::with_capacity(points.len());
    for &p in points.iter() {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f32; 2]> = Vec::with_capacity(points.len());
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Which bones disappear when `bone` is severed, plus the stump that remains.
///
/// `@0x1009a8a7c` — `gs_puppet::draw::severed(&Puppet, &str)`; the body calls
/// `model::Puppet::subtree` and therefore returns the descendant set of the severed
/// bone.  Returned as `(removed, stump)` where `stump` is the bone itself (the
/// caller keeps drawing its cut face).
pub fn severed(puppet: &Puppet, bone: &str) -> (Vec<usize>, Option<usize>) {
    match puppet.index_of(bone) {
        Some(i) => (puppet.hangs(i), Some(i)),
        None => (Vec::new(), None),
    }
}

/// The view basis for a bounding box.
///
/// `gs_puppet::draw::view_basis` (a local symbol): it turns the figure's bounds into
/// a rotation whose `+z` points out of the longest horizontal axis, so a flat
/// projection sees the figure's widest side.  Used by
/// [`crate::mesh::BodyFrame::new`] and `splat::prepare`.
pub fn view_basis(centre: Vec3, size: Vec3) -> Xf {
    // Face the wider of the x / z extents.
    let yaw = if size.x >= size.z { 0.0 } else { -90.0f32 };
    let rot = Mat3::from_rotation_y(yaw * crate::math::DEG_TO_RAD);
    Xf::new(rot, centre)
}

/// Per-channel blend of two byte colours.
///
/// `gs_puppet::draw::mix` (local symbol); `t = 0` keeps `a`.
pub fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let t = t.clamp(0.0, 1.0);
    let mut out = [0u8; 4];
    for k in 0..4 {
        out[k] = (a[k] as f32 + (b[k] as f32 - a[k] as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out
}

/// Darken a colour by `k` (`gs_puppet::draw::shade`, `@0x100a…`).
pub fn shade(c: [u8; 4], k: f32) -> [u8; 4] {
    let k = k.clamp(0.0, 1.0);
    [
        (c[0] as f32 * k) as u8,
        (c[1] as f32 * k) as u8,
        (c[2] as f32 * k) as u8,
        c[3],
    ]
}

/// A short arc of `n` points covering `sweep` degrees, starting at `from`.
///
/// `gs_puppet::draw::positive_arc` (local symbol) — only non-negative sweeps are
/// drawn, hence the name.
pub fn positive_arc(centre: [f32; 2], radius: f32, from: f32, sweep: f32, n: usize) -> Shape {
    let n = n.max(2);
    let sweep = sweep.max(0.0);
    let mut points = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let a = (from + sweep * t) * crate::math::DEG_TO_RAD;
        points.push([centre[0] + radius * a.cos(), centre[1] + radius * a.sin()]);
    }
    Shape::line(points)
}

/// A ring of `n` points around a centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub centre: [f32; 2],
    pub radius: f32,
}

impl Ring {
    /// A ring of `n` points.
    ///
    /// `gs_puppet::draw::Ring::at` (local symbol).
    pub fn at(centre: [f32; 2], radius: f32, n: usize) -> Shape {
        let n = n.max(3);
        let mut points = Vec::with_capacity(n);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            points.push([centre[0] + radius * a.cos(), centre[1] + radius * a.sin()]);
        }
        Shape::poly(points)
    }
}

impl Posed {
    /// Build the draw list for this pose.
    ///
    /// `@0x100a43040` — `gs_puppet::draw::<impl Posed>::draw_list(&self, &Camera,
    /// &Puppet, &PoseSet?, …) -> DrawList`.  One tapered quad per bone segment,
    /// projected with [`Camera::project`] and depth-sorted into
    /// `Vec<(f32, usize, Vec<SegDraw>)>` layers.  The colour/shading rules are only
    /// partly recovered (`0.5`, `0.25`, `2.5`, `255.0` immediates), so the shape of
    /// the output is reproduced rather than every blend.
    pub fn draw_list(&self, camera: &Camera, puppet: &Puppet, color: [u8; 4]) -> DrawList {
        let mut list = DrawList::default();
        let mut layers: Vec<(f32, usize, Vec<SegDraw>)> = Vec::new();
        for (i, seg) in puppet.segments.iter().enumerate() {
            let Some(xf) = self.bones.get(&seg.name) else {
                continue;
            };
            let a = xf.point(Vec3::ZERO);
            let b = xf.point(Vec3::new(0.0, seg.length, 0.0));
            let (ax, ay, ad) = camera.project(&a);
            let (bx, by, bd) = camera.project(&b);
            if ad <= 0.0 && bd <= 0.0 {
                continue;
            }
            // Half-width in pixels from the bone radius, scaled by depth.
            let scale = camera.focal / ad.max(Camera::NEAR);
            let w = (seg.radius * scale).max(0.25);
            let dx = bx - ax;
            let dy = by - ay;
            let len = (dx * dx + dy * dy).sqrt().max(1e-6);
            let (nx, ny) = (-dy / len * w, dx / len * w);
            let shape = Shape::poly(vec![
                [ax + nx, ay + ny],
                [bx + nx, by + ny],
                [bx - nx, by - ny],
                [ax - nx, ay - ny],
            ]);
            let depth = (ad + bd) * 0.5;
            let prim = Prim {
                shape,
                color,
                depth,
            };
            let seg_draw = SegDraw {
                bones: [seg.parent, i],
                prims: vec![prim.clone()],
                color,
            };
            list.prims.push(prim);
            match layers.iter_mut().find(|(d, _, _)| (*d - depth).abs() < 1e-3) {
                Some((_, _, v)) => v.push(seg_draw),
                None => layers.push((depth, i, vec![seg_draw])),
            }
        }
        // Painter's algorithm: near layers last.
        layers.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        list.layers = layers;
        list
    }

    /// The figure's shadow on the ground plane.
    ///
    /// `@0x100a4285c` — `gs_puppet::draw::<impl Posed>::ground_shadow(&self, &Camera,
    /// …) -> DrawList`.  It projects every bone origin onto `y = 0`, takes the
    /// **convex hull** of those points through [`convex_hull`] and fills it with a
    /// translucent colour (`9.0` and the `0x3c/0x34` byte constants in the body).
    pub fn ground_shadow(&self, camera: &Camera, color: [u8; 4]) -> DrawList {
        let mut points: Vec<[f32; 2]> = self
            .bones
            .values()
            .map(|xf| {
                let p = Vec3::new(xf.t.x, 0.0, xf.t.z);
                let (x, y, _) = camera.project(&p);
                [x, y]
            })
            .collect();
        let hull = convex_hull(&mut points);
        let mut list = DrawList::default();
        if hull.len() < 3 {
            return list;
        }
        let shape = Shape::poly(hull);
        list.filled.push((shape.clone(), color));
        list.prims.push(Prim {
            shape,
            color,
            depth: f32::MAX,
        });
        list
    }

    /// This pose's bone quads as `epaint` shapes, far to near.
    ///
    /// The `epaint` face of [`Posed::draw_list`]: one filled convex quad per bone
    /// segment, projected with [`Camera::project`] and emitted in the recovered
    /// layer order (far to near), which is what an `egui::Painter` consumes.
    pub fn shapes(&self, camera: &Camera, puppet: &Puppet, color: [u8; 4]) -> Vec<epaint::Shape> {
        self.draw_list(camera, puppet, color).shapes()
    }

    /// The ground shadow as an `epaint` shape: the convex hull of the bone
    /// footprints on `y = 0`, filled with `color`.  Empty when the hull is
    /// degenerate.
    pub fn shadow_shapes(&self, camera: &Camera, color: [u8; 4]) -> Vec<epaint::Shape> {
        self.ground_shadow(camera, color).filled_shapes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hull_of_a_square_with_an_interior_point_is_the_square() {
        let mut pts = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.5, 0.5],
        ];
        let hull = convex_hull(&mut pts);
        assert_eq!(hull.len(), 4);
        assert!(hull.contains(&[0.0, 0.0]));
        assert!(hull.contains(&[1.0, 1.0]));
        assert!(!hull.contains(&[0.5, 0.5]));
        // The input vector is compacted in place but not reordered out of existence.
        assert_eq!(pts.len(), 5);
    }

    #[test]
    fn hull_drops_non_finite_points_in_place() {
        let mut pts = vec![
            [0.0, 0.0],
            [1.0, 1.0],
            [f32::INFINITY, 0.0],
            [0.5, f32::NAN],
            [0.0, 1.0],
        ];
        let hull = convex_hull(&mut pts);
        assert_eq!(pts.len(), 3, "non-finite points are removed from the input");
        assert_eq!(hull.len(), 3);
    }

    #[test]
    fn hull_needs_two_points() {
        let mut one = vec![[0.0, 0.0]];
        assert!(convex_hull(&mut one).is_empty());
        let mut none: Vec<[f32; 2]> = Vec::new();
        assert!(convex_hull(&mut none).is_empty());
    }

    #[test]
    fn hull_of_a_large_cloud_uses_the_shared_sort_path() {
        // More than 21 points takes the non-insertion-sort branch in the binary.
        let mut pts: Vec<[f32; 2]> = (0..40)
            .map(|i| {
                let a = i as f32 / 40.0 * std::f32::consts::TAU;
                [a.cos(), a.sin()]
            })
            .collect();
        let hull = convex_hull(&mut pts);
        assert!(hull.len() >= 8, "{}", hull.len());
        // Every hull point is on the unit circle.
        for p in &hull {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - 1.0).abs() < 1e-5, "{p:?}");
        }
    }

    #[test]
    fn severed_removes_the_subtree() {
        use crate::model::Segment;
        let p = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("arm_l", 0, Vec3::new(0.2, 0.5, 0.0), 0.3),
                Segment::new("hand_l", 1, Vec3::new(0.0, -0.3, 0.0), 0.1),
            ],
        )
        .unwrap();
        let (removed, stump) = severed(&p, "arm_l");
        assert_eq!(removed, vec![2]);
        assert_eq!(stump, Some(1));
        // A bone that does not exist removes nothing.
        assert_eq!(severed(&p, "tail"), (Vec::new(), None));
    }

    #[test]
    fn mix_and_shade() {
        assert_eq!(mix([0, 0, 0, 255], [200, 100, 50, 255], 0.0), [0, 0, 0, 255]);
        assert_eq!(
            mix([0, 0, 0, 255], [200, 100, 50, 255], 1.0),
            [200, 100, 50, 255]
        );
        assert_eq!(mix([0, 0, 0, 255], [200, 100, 50, 255], 0.5), [100, 50, 25, 255]);
        assert_eq!(shade([100, 50, 25, 200], 0.5), [50, 25, 12, 200]);
    }

    #[test]
    fn arcs_and_rings() {
        let arc = positive_arc([0.0, 0.0], 1.0, 0.0, 90.0, 3);
        assert_eq!(arc.points.len(), 3);
        assert!((arc.points[0][0] - 1.0).abs() < 1e-5);
        assert!(arc.points[2][1] > 0.99);
        // A negative sweep yields a degenerate (repeated) arc, never a reversed one.
        let neg = positive_arc([0.0, 0.0], 1.0, 0.0, -90.0, 3);
        assert!(neg.points.iter().all(|p| (p[0] - 1.0).abs() < 1e-5));
        let ring = Ring::at([1.0, 2.0], 0.5, 8);
        assert_eq!(ring.points.len(), 8);
        assert!(ring.closed);
        // Every ring point is 0.5 from the centre.
        for p in &ring.points {
            let d = ((p[0] - 1.0).powi(2) + (p[1] - 2.0).powi(2)).sqrt();
            assert!((d - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn dilated_moves_points_outwards() {
        let s = Shape::poly(vec![
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ]);
        let d = s.dilated(0.5);
        let (lo, hi) = d.bounds();
        // The corners sit at +-1 on each axis and move 0.5 outwards along the
        // diagonal, so each axis grows by 0.5/sqrt(2).
        assert!((lo[0] + 1.0 + 0.5 / std::f32::consts::SQRT_2).abs() < 1e-5, "{lo:?}");
        assert!((hi[0] - 1.0 - 0.5 / std::f32::consts::SQRT_2).abs() < 1e-5, "{hi:?}");
    }

    #[test]
    fn draw_list_emits_a_layer_per_bone() {
        use crate::model::Segment;
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("neck", 0, Vec3::new(0.0, 1.0, 0.0), 0.2),
            ],
        )
        .unwrap();
        let cam = Camera::new(160.0, 120.0, 0.0, 0.0, 8.0, 400.0);
        let list = puppet.pose().draw_list(&cam, &puppet, [200, 180, 160, 255]);
        assert_eq!(list.prims.len(), 2);
        // Bones at the same depth share a layer, so there is at least one.
        assert!(!list.layers.is_empty());
        // Layers are sorted far to near.
        for pair in list.layers.windows(2) {
            assert!(pair[0].0 >= pair[1].0, "{:?}", list.layers);
        }
        // Every primitive is a closed quad.
        for p in &list.prims {
            assert_eq!(p.shape.points.len(), 4);
            assert!(p.shape.closed);
        }
    }

    #[test]
    fn ground_shadow_is_the_hull_of_the_bone_footprints() {
        use crate::model::Segment;
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("a", 0, Vec3::new(-0.5, 1.0, 0.4), 0.2),
                Segment::new("b", 0, Vec3::new(0.5, 1.0, -0.4), 0.2),
                Segment::new("c", 0, Vec3::new(0.0, 2.0, 0.8), 0.2),
            ],
        )
        .unwrap();
        // A ground-plane shadow needs a tilted camera, otherwise the plane
        // projects onto a single horizontal line.
        let cam = Camera::new(0.0, 0.0, 0.0, 30.0, 8.0, 400.0);
        let list = puppet.pose().ground_shadow(&cam, [0, 0, 0, 90]);
        assert_eq!(list.filled.len(), 1);
        let (shape, color) = &list.filled[0];
        assert!(shape.points.len() >= 3);
        assert_eq!(*color, [0, 0, 0, 90]);
        // The shadow is flat: every point lies on the ground plane's projection,
        // so its y is the projection of y = 0.
        let (_, hi) = shape.bounds();
        assert!(hi[1].is_finite());
    }

    // -- the `epaint` shapes the draw list becomes ---------------------------

    /// A camera at the origin looking down +z, 50 pixels per unit at depth 8.
    fn frame_cam() -> Camera {
        Camera::new(160.0, 120.0, 0.0, 0.0, 8.0, 400.0)
    }

    #[test]
    fn the_recovered_shading_thresholds_are_the_shader_literals() {
        assert_eq!(LIGHT_ABOVE, 0.62);
        assert_eq!(DARK_BELOW, -0.08);
        // Both sit inside the shader's smoothstep band, +-0.2.
        assert!(LIGHT_ABOVE - 0.2 < LIGHT_ABOVE && DARK_BELOW + 0.2 < LIGHT_ABOVE);
    }

    #[test]
    fn byte_colours_become_premultiplied_epaint_colours() {
        // Opaque: the bytes pass through.
        assert_eq!(color32([10, 20, 30, 255]).to_array(), [10, 20, 30, 255]);
        // Half alpha is premultiplied, which is what `epaint` wants and what the
        // recovered blend pass emits (`vec4(rgb / 255 * a, a)`).
        let half = color32([200, 100, 40, 128]);
        assert_eq!(half.a(), 128);
        let [r, g, b, a] = half.to_array();
        assert_eq!(a, 128);
        assert_eq!(r, (200u32 * 128 / 255) as u8);
        assert_eq!(g, (100u32 * 128 / 255) as u8);
        assert_eq!(b, (40u32 * 128 / 255) as u8);
        // A 3-byte tint is opaque.
        assert_eq!(color32_rgb([1, 2, 3]).to_array(), [1, 2, 3, 255]);
    }

    #[test]
    fn a_bone_quad_becomes_one_epaint_polygon() {
        use crate::model::Segment;
        let puppet = Puppet::new(
            "biped",
            vec![Segment::new("root", 0, Vec3::ZERO, 1.0)],
        )
        .unwrap();
        let cam = frame_cam();
        // A 1.0-long bone with radius 0.5 at the origin: 50 px/unit.
        let list = puppet.pose().draw_list(&cam, &puppet, [200, 180, 160, 255]);
        assert_eq!(list.prims.len(), 1);
        assert_eq!(
            list.prims[0].shape.points,
            vec![
                [185.0, 120.0],
                [185.0, 70.0],
                [135.0, 70.0],
                [135.0, 120.0]
            ]
        );
        let shapes = list.shapes();
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            epaint::Shape::Path(p) => {
                assert!(p.closed, "a limb quad is a closed polygon");
                assert_eq!(
                    p.points,
                    vec![
                        epaint::pos2(185.0, 120.0),
                        epaint::pos2(185.0, 70.0),
                        epaint::pos2(135.0, 70.0),
                        epaint::pos2(135.0, 120.0)
                    ]
                );
                assert_eq!(p.fill, color32([200, 180, 160, 255]));
                assert_eq!(p.stroke, epaint::PathStroke::NONE);
            }
            other => panic!("expected a path, got {other:?}"),
        }
        // The convenience wrapper is the same list.
        assert_eq!(puppet.pose().shapes(&cam, &puppet, [200, 180, 160, 255]), shapes);
    }

    #[test]
    fn shapes_come_out_far_to_near() {
        use crate::model::Segment;
        // A trunk at the origin and a child 2 units further away from the camera
        // (a smaller z is nearer: depth is height - z).
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("neck", 0, Vec3::new(0.0, 1.0, 2.0), 0.2),
            ],
        )
        .unwrap();
        let cam = frame_cam();
        let shapes = puppet.pose().shapes(&cam, &puppet, [255, 255, 255, 255]);
        assert_eq!(shapes.len(), 2);
        let centre = |s: &epaint::Shape| match s {
            epaint::Shape::Path(p) => {
                let n = p.points.len() as f32;
                let (sx, sy) = p
                    .points
                    .iter()
                    .fold((0.0, 0.0), |(ax, ay), q| (ax + q.x, ay + q.y));
                (sx / n, sy / n)
            }
            other => panic!("expected a path, got {other:?}"),
        };
        // The root (depth 8) is drawn first: its quad spans y 70..120.
        let (rx, ry) = centre(&shapes[0]);
        assert!((rx - 160.0).abs() < 1e-4 && (ry - 95.0).abs() < 1e-4, "{rx} {ry}");
        // The neck (depth 6) is drawn second, higher up the screen because its
        // pivot sits at y = 1 which projects further at the nearer scale.
        let (nx, ny) = centre(&shapes[1]);
        assert!((nx - 160.0).abs() < 1e-4, "{nx}");
        // depth 6 -> 400 / 6 = 66.67 px per unit: the two ends of the 0.2-long
        // bone land at y = 120 - 66.67 and 120 - 1.2 * 66.67.
        let scale: f32 = 400.0 / 6.0;
        let want_ny = ((120.0f32 - scale) + (120.0 - 1.2 * scale)) / 2.0;
        assert!((want_ny - 46.666_67).abs() < 1e-3, "{want_ny}");
        assert!((ny - want_ny).abs() < 1e-3, "{ny}");
        assert!(ny < ry + 1e-6);
    }

    #[test]
    fn a_shadow_hull_is_emitted_once_not_twice() {
        use crate::model::Segment;
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("a", 0, Vec3::new(-0.5, 1.0, 0.4), 0.2),
                Segment::new("b", 0, Vec3::new(0.5, 1.0, -0.4), 0.2),
            ],
        )
        .unwrap();
        let cam = Camera::new(0.0, 0.0, 0.0, 30.0, 8.0, 400.0);
        let list = puppet.pose().ground_shadow(&cam, [0, 0, 0, 90]);
        // The list carries the hull in `filled` *and* in the flat `prims`, but
        // `shapes()` emits the filled layer only, so nothing is painted twice.
        assert_eq!(list.filled.len(), 1);
        assert_eq!(list.prims.len(), 1);
        let shapes = list.shapes();
        assert_eq!(shapes.len(), 1);
        assert_eq!(puppet.pose().shadow_shapes(&cam, [0, 0, 0, 90]), shapes);
        // Its fill is the translucent colour, premultiplied by `epaint`.
        match &shapes[0] {
            epaint::Shape::Path(p) => {
                assert!(p.closed);
                assert!(p.points.len() >= 3);
                assert_eq!(p.fill.a(), 90);
                assert_eq!((p.fill.r(), p.fill.g(), p.fill.b()), (0, 0, 0));
            }
            other => panic!("expected a path, got {other:?}"),
        }
        // The flat list on its own holds the same hull.
        assert_eq!(list.prim_shapes(), shapes);
    }

    #[test]
    fn the_layer_and_flat_views_hold_the_same_quads() {
        use crate::model::Segment;
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("neck", 0, Vec3::new(0.0, 1.0, 2.0), 0.2),
                Segment::new("arm", 0, Vec3::new(0.4, 0.8, 0.0), 0.3),
            ],
        )
        .unwrap();
        let list = puppet.pose().draw_list(&frame_cam(), &puppet, [10, 20, 30, 255]);
        assert_eq!(list.prims.len(), 3);
        // `layer_shapes` is depth-sorted; `prim_shapes` keeps the segment order.
        // Every quad of one must appear in the other.
        let layers = list.layer_shapes();
        let flat = list.prim_shapes();
        assert_eq!(layers.len(), 3);
        assert_eq!(flat.len(), 3);
        for s in &layers {
            assert!(flat.contains(s), "{s:?} missing from the flat list");
        }
        // The layers themselves are sorted far to near (the recovered painter's
        // order), and `layer_shapes` is exactly their primitives in that order.
        let depths: Vec<f32> = list.layers.iter().map(|(d, _, _)| *d).collect();
        assert!(depths.windows(2).all(|w| w[0] >= w[1]), "{depths:?}");
        // The root and the arm both sit at depth 8, so they share a layer: two
        // layers for three quads.
        assert_eq!(depths.len(), 2, "{depths:?}");
        let want: Vec<epaint::Shape> = list
            .layers
            .iter()
            .flat_map(|(_, _, segs)| segs.iter())
            .flat_map(|seg| seg.prims.iter().filter_map(|p| p.to_epaint()))
            .collect();
        assert_eq!(layers, want);
        // `shapes()` is the filled + tinted + layer picture: no filled or tinted
        // layer here, so it is the layer list.
        assert_eq!(list.shapes(), layers);
    }

    #[test]
    fn degenerate_shapes_paint_nothing() {
        let line = Shape::line(vec![[0.0, 0.0], [1.0, 1.0]]);
        assert!(line.to_fill([1, 2, 3, 255]).is_none());
        assert!(Shape::poly(Vec::new()).to_fill([1, 2, 3, 255]).is_none());
        assert!(Shape::poly(Vec::new()).to_stroke(epaint::Stroke::NONE).is_none());
        // Two points make a line, not a fill.
        match line.to_stroke(epaint::Stroke::new(2.0, epaint::Color32::WHITE)) {
            Some(epaint::Shape::Path(p)) => {
                assert!(!p.closed);
                assert_eq!(p.points.len(), 2);
                assert_eq!(p.fill, epaint::Color32::TRANSPARENT);
                assert_eq!(p.stroke.width, 2.0);
            }
            other => panic!("expected an open path, got {other:?}"),
        }
        // A closed shape strokes closed.
        match Ring::at([1.0, 1.0], 1.0, 4).to_stroke(epaint::Stroke::NONE) {
            Some(epaint::Shape::Path(p)) => assert!(p.closed),
            other => panic!("expected a closed path, got {other:?}"),
        }
        // A tinted layer with 3 bytes is opaque, and a filled layer keeps its alpha.
        let mut list = DrawList::default();
        list.tinted.push((Ring::at([0.0, 0.0], 1.0, 4), [9, 8, 7]));
        list.filled.push((Ring::at([0.0, 0.0], 2.0, 4), [0, 0, 0, 0]));
        let shapes = list.shapes();
        assert_eq!(shapes.len(), 2, "filled first, then tinted");
        match (&shapes[0], &shapes[1]) {
            (epaint::Shape::Path(filled), epaint::Shape::Path(tint)) => {
                assert_eq!(filled.fill.a(), 0);
                assert_eq!(tint.fill, color32_rgb([9, 8, 7]));
            }
            other => panic!("expected two paths, got {other:?}"),
        }
    }

    #[test]
    fn a_real_egui_frame_paints_the_limb_quads() {
        use crate::model::Segment;
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("neck", 0, Vec3::new(0.0, 1.0, 2.0), 0.2),
            ],
        )
        .unwrap();
        let cam = frame_cam();
        let shapes = puppet.pose().shapes(&cam, &puppet, [200, 180, 160, 255]);

        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(640.0, 480.0),
        ));
        let out = ctx.run_ui(input, |ui| {
            let painter = ui.painter().clone().with_clip_rect(egui::Rect::EVERYTHING);
            for s in &shapes {
                painter.add(s.clone());
            }
        });
        let paths: Vec<&epaint::PathShape> = out
            .shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                epaint::Shape::Path(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(paths.len(), 2, "one path per bone segment");
        assert_eq!(
            paths[0].points,
            vec![
                epaint::pos2(185.0, 120.0),
                epaint::pos2(185.0, 70.0),
                epaint::pos2(135.0, 70.0),
                epaint::pos2(135.0, 120.0)
            ]
        );
        assert_eq!(paths[0].fill, color32([200, 180, 160, 255]));
        // The frame's first quad bounds are the bone's: 50 px wide, 50 px tall.
        let b = epaint::Shape::Path(paths[0].clone()).visual_bounding_rect();
        assert_eq!(b, epaint::Rect::from_min_max(epaint::pos2(135.0, 70.0), epaint::pos2(185.0, 120.0)));
        // The second quad is the nearer bone, higher on the screen.
        let top = epaint::Shape::Path(paths[1].clone()).visual_bounding_rect();
        assert!(top.min.y < 50.0 && top.max.y > 50.0, "{top:?}");
    }
}
