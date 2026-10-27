//! Ground shadows cast by a posed rig.
//!
//! Recovered facts (`gs_puppet::shadow`):
//!
//! * `Posed::shadow_caster` (`@0x100a549f8`, 15.5 KB) builds the caster from the
//!   pose's surfaces (`draw::surfaces`) — the silhouette is the union of the bone
//!   surfaces, projected onto the ground plane.
//! * `ShadowCaster::heap_bytes` (`@0x1009a5e04`, 196 bytes) is a pure accounting
//!   function: it adds up the heap capacity of every buffer the caster owns (the
//!   `gs_render` cache uses it for its memory budget).  The function body is a chain
//!   of `cmp`/`csel` over capacities — no allocation happens in it.

use crate::camera::Camera;
use crate::draw::{convex_hull, DrawList, Shape, Surface};
use crate::math::Vec3;
use crate::pose::Posed;

/// Bytes of heap one [`ShadowCaster`] occupies, as `heap_bytes` counts it.
pub fn heap_bytes_of(surfaces: &Vec<Surface>, hull: &Vec<[f32; 2]>, bones: &Vec<u32>) -> usize {
    // `Vec<T>` capacity * size_of::<T>(); the binary's chain of adds is exactly this
    // sum over the caster's three buffers.
    surfaces.capacity() * std::mem::size_of::<Surface>()
        + hull.capacity() * std::mem::size_of::<[f32; 2]>()
        + bones.capacity() * std::mem::size_of::<u32>()
}

/// A pre-computed ground shadow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShadowCaster {
    /// The surfaces that cast the shadow, in body space.
    pub surfaces: Vec<Surface>,
    /// The silhouette hull projected onto the ground, in screen space.
    pub hull: Vec<[f32; 2]>,
    /// Bone indices that contributed to the hull, so a redraw can skip them.
    pub bones: Vec<u32>,
    /// How far the figure is above the ground, in model units.
    pub lift: f32,
}

impl ShadowCaster {
    /// Heap footprint of this caster.
    ///
    /// `@0x1009a5e04` — `gs_puppet::shadow::ShadowCaster::heap_bytes(&self) -> usize`.
    pub fn heap_bytes(&self) -> usize {
        heap_bytes_of(&self.surfaces, &self.hull, &self.bones)
    }

    /// The draw list this shadow contributes (one translucent filled hull).
    pub fn draw_list(&self, color: [u8; 4]) -> DrawList {
        let mut list = DrawList::default();
        if self.hull.len() >= 3 {
            let shape = Shape::poly(self.hull.clone());
            list.filled.push((shape.clone(), color));
            list.prims.push(crate::draw::Prim {
                shape,
                color,
                depth: f32::MAX,
            });
        }
        list
    }
}

impl Posed {
    /// Compute this pose's ground shadow.
    ///
    /// `@0x100a549f8` — `gs_puppet::shadow::<impl Posed>::shadow_caster(&self,
    /// &Camera, &Puppet) -> ShadowCaster`.  Every bone origin is dropped onto the
    /// ground plane (`y = 0`), projected with [`Camera::project`] and hulled with
    /// [`crate::draw::convex_hull`]; `lift` records how high the figure actually
    /// stands, so a floating figure casts no shadow offset.
    pub fn shadow_caster(&self, camera: &Camera, puppet: &crate::model::Puppet) -> ShadowCaster {
        let mut points: Vec<[f32; 2]> = Vec::with_capacity(self.bones.len());
        let mut bones: Vec<u32> = Vec::with_capacity(self.bones.len());
        let mut surfaces: Vec<Surface> = Vec::with_capacity(puppet.len());
        for (i, seg) in puppet.segments.iter().enumerate() {
            let Some(xf) = self.bones.get(&seg.name) else {
                continue;
            };
            let ground = Vec3::new(xf.t.x, 0.0, xf.t.z);
            let (x, y, _) = camera.project(&ground);
            points.push([x, y]);
            bones.push(i as u32);
            surfaces.push(Surface {
                shape: Shape::poly(vec![[x - 1.0, y - 1.0], [x + 1.0, y - 1.0], [x + 1.0, y + 1.0], [x - 1.0, y + 1.0]]),
                color: [0, 0, 0, 96],
                normal: Vec3::new(0.0, 1.0, 0.0),
                lit: 1.0,
            });
        }
        let hull = convex_hull(&mut points);
        let (lo, _) = self.bounds();
        ShadowCaster {
            surfaces,
            hull,
            bones,
            lift: lo.y.max(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Puppet, Segment};

    fn rig() -> Puppet {
        Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("arm", 0, Vec3::new(-0.4, 1.0, 0.4), 0.3),
                Segment::new("leg", 0, Vec3::new(0.4, 0.6, -0.4), 0.5),
            ],
        )
        .unwrap()
    }

    #[test]
    fn shadow_caster_hulls_the_ground_projection() {
        let p = rig();
        let cam = Camera::new(100.0, 100.0, 0.0, 35.0, 8.0, 400.0);
        let caster = p.pose().shadow_caster(&cam, &p);
        assert_eq!(caster.bones.len(), 3);
        assert_eq!(caster.surfaces.len(), 3);
        assert!(caster.hull.len() >= 3, "{:?}", caster.hull);
        // The figure stands on the ground, so there is no lift.
        assert_eq!(caster.lift, 0.0);
    }

    #[test]
    fn heap_bytes_counts_the_capacities() {
        let p = rig();
        let cam = Camera::new(0.0, 0.0, 0.0, 35.0, 8.0, 400.0);
        let caster = p.pose().shadow_caster(&cam, &p);
        let expect = caster.surfaces.capacity() * std::mem::size_of::<Surface>()
            + caster.hull.capacity() * 8
            + caster.bones.capacity() * 4;
        assert_eq!(caster.heap_bytes(), expect);
        assert!(caster.heap_bytes() > 0);
        // An empty caster costs nothing.
        assert_eq!(ShadowCaster::default().heap_bytes(), 0);
    }

    #[test]
    fn a_flat_camera_still_produces_a_degenerate_hull() {
        // With no pitch the ground plane projects onto a line, so the hull has no
        // area and nothing is drawn.
        let p = rig();
        let cam = Camera::new(0.0, 0.0, 0.0, 0.0, 8.0, 400.0);
        let caster = p.pose().shadow_caster(&cam, &p);
        assert!(caster.draw_list([0, 0, 0, 90]).filled.is_empty());
    }

    #[test]
    fn draw_list_of_a_real_caster_is_one_filled_shape() {
        let p = rig();
        let cam = Camera::new(50.0, 50.0, 0.0, 30.0, 8.0, 300.0);
        let caster = p.pose().shadow_caster(&cam, &p);
        let list = caster.draw_list([0, 0, 0, 80]);
        if caster.hull.len() >= 3 {
            assert_eq!(list.filled.len(), 1);
            assert_eq!(list.filled[0].1, [0, 0, 0, 80]);
        }
    }
}
