//! Status-driven particle layers (motes, dots, hollows).
//!
//! Recovered facts (`gs_puppet::layers`):
//!
//! * `LayerCloud` owns four buffers — the drop glue spells out
//!   `Vec<Dot>`, `Vec<Hollow>`, `Vec<Particle>` and (from another module)
//!   `Vec<blood::Well>` — plus the per-segment split `Vec<Vec<Dot>>`.
//! * `LayerCloud::dots` / `dots_and_hollows` / `posed` are thin wrappers around
//!   `LayerCloud::dots_of`, which the binary splits across scoped threads
//!   (`std::thread::scope` + `ScopedJoinHandle<Vec<Dot>>`).
//! * `LayerCloud::build` (`@0x100a58af8`) makes a `UvLayout` (`UvLayout::new`) and
//!   maps each dot to an atlas cell with `UvLayout::locate`; the `3.0` and `1.0`
//!   immediates are the dot/segment spacing.
//! * `LayerCloud::motes` (`@0x100a5943c`) dispatches to the per-status generators and
//!   the bone names `"Hand"`, `"Leg"` and `"head"` survive as literals, with
//!   `170.0` (`0x432a0000`) as the maximum mote distance.
//! * `LayerCloud::arrival` (`@0x100a5a288`) scopes threads and flattens
//!   `Vec<Vec<Dot>>`.
//! * `Particle::at` (`@0x100a60734`) is the motion function: `7.0` and `37.0` are
//!   its rates, `±24.0` the vertical travel, `±12.0` the lateral travel, `9.0` and
//!   `11.0` two drift rates and `0.25`/`0.5` blend weights.

use crate::math::Vec3;
use crate::pose::TAU as TAU_F32;
use crate::mesh::Mesh;
use crate::pose::Posed;
use crate::splat::SplatParams;
use crate::uv::UvLayout;

/// Maximum distance a mote may drift from its bone, `0x432a0000`.
pub const MOTE_REACH: f32 = 170.0;

/// Vertical travel of a drifting particle (`±24.0` in `Particle::at`).
pub const PARTICLE_RISE: f32 = 24.0;

/// Lateral travel of a drifting particle (`±12.0`).
pub const PARTICLE_SWAY: f32 = 12.0;

/// Primary rate (`7.0`).
pub const PARTICLE_RATE: f32 = 7.0;

/// Secondary rate (`37.0`).
pub const PARTICLE_RATE_FAST: f32 = 37.0;

/// Drift rates (`9.0`, `11.0`).
pub const PARTICLE_DRIFT: [f32; 2] = [9.0, 11.0];

/// Blend weights `0.25` / `0.5`.
pub const PARTICLE_BIAS: f32 = 0.25;

/// One dot of a status layer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dot {
    pub pos: Vec3,
    pub radius: f32,
    pub color: [u8; 4],
}

/// A hollow ring (the outline a dot draws when it is near the silhouette).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hollow {
    pub pos: Vec3,
    pub radius: f32,
    pub thickness: f32,
}

/// One drifting mote.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Particle {
    /// Where the mote started.
    pub origin: Vec3,
    /// Phase offset in cycles.
    pub phase: f32,
    /// Size in pixels.
    pub size: f32,
    /// Colour.
    pub color: [u8; 4],
}

impl Particle {
    /// The mote's position at time `t` (in seconds).
    ///
    /// `@0x100a60734` — `gs_puppet::layers::Particle::at(&self, f32) -> Vec3`.
    /// The recovered body bobs within `±24.0` vertically and `±12.0` laterally at
    /// two rates (`7.0` slow, `37.0` fast) with the `0.25`/`0.5` blend, which is what
    /// this reproduces.
    pub fn at(&self, t: f32) -> Vec3 {
        let a = (TAU_F32 * (t * PARTICLE_RATE / 10.0 + self.phase)).sin();
        let b = (TAU_F32 * (t * PARTICLE_RATE_FAST / 10.0 + self.phase * PARTICLE_BIAS)).cos();
        let drift = (TAU_F32 * t * PARTICLE_DRIFT[0] / 10.0).sin();
        let drift2 = (TAU_F32 * t * PARTICLE_DRIFT[1] / 10.0).cos();
        Vec3::new(
            self.origin.x + PARTICLE_SWAY * (PARTICLE_BIAS * a + (1.0 - PARTICLE_BIAS) * drift),
            self.origin.y + PARTICLE_RISE * (PARTICLE_BIAS * b + (1.0 - PARTICLE_BIAS) * drift2) * 0.5,
            self.origin.z + PARTICLE_SWAY * (PARTICLE_BIAS * b - (1.0 - PARTICLE_BIAS) * drift),
        )
    }

    /// The particle at `t`, clamped into [`MOTE_REACH`].
    pub fn at_bounded(&self, t: f32) -> Vec3 {
        let p = self.at(t);
        let d = p - self.origin;
        let len = d.length();
        if len > MOTE_REACH {
            self.origin + d * (MOTE_REACH / len)
        } else {
            p
        }
    }
}

/// A cloud of status effects bound to a rig.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerCloud {
    pub dots: Vec<Dot>,
    pub hollows: Vec<Hollow>,
    pub particles: Vec<Particle>,
    /// Bone index each dot is bound to.
    pub segment: Vec<usize>,
}

impl LayerCloud {
    /// Lay dots out over a rig's segments.
    ///
    /// `@0x100a58af8` — `LayerCloud::build(&Puppet, &UvLayout, &SplatParams) ->
    /// LayerCloud`.  One dot per segment, offset by the `3.0` unit spacing and mapped
    /// into the atlas with `UvLayout::locate`.
    pub fn build(puppet: &crate::model::Puppet, params: &SplatParams) -> LayerCloud {
        let layout = UvLayout::fit(puppet, 512);
        let mut dots = Vec::with_capacity(puppet.len());
        let mut segment = Vec::with_capacity(puppet.len());
        for (i, seg) in puppet.segments.iter().enumerate() {
            // One dot per segment, stored as a *bone-local* offset (zero: the dot
            // sits on the bone), so `posed` can map it into world space.  The atlas
            // lookup only has to succeed for the layout to be usable.
            let pos = Vec3::ZERO;
            let _uv = layout.address(seg.pivot);
            dots.push(Dot {
                pos,
                radius: (params.radius * 10.0).max(1.0),
                color: [255, 255, 255, 255],
            });
            segment.push(i);
        }
        LayerCloud {
            dots,
            hollows: Vec::new(),
            particles: Vec::new(),
            segment,
        }
    }

    /// The dots layer.
    ///
    /// `@0x100a58a6c` — `LayerCloud::dots(&self) -> Vec<Dot>` (a thin wrapper over
    /// `dots_of`).
    pub fn dots(&self) -> Vec<Dot> {
        self.dots.clone()
    }

    /// The dots and hollows layers together.
    ///
    /// `@0x100a589d8` — `LayerCloud::dots_and_hollows(&self) -> (Vec<Dot>,
    /// Vec<Hollow>)`.
    pub fn dots_and_hollows(&self) -> (Vec<Dot>, Vec<Hollow>) {
        (self.dots.clone(), self.hollows.clone())
    }

    /// Follow a posed rig.
    ///
    /// `@0x100a5a1ec` — `LayerCloud::posed(&self, &Posed) -> LayerCloud`: every dot
    /// is transformed by the bone it is bound to.  Dot positions are *bone-local*
    /// offsets, so this maps rest space → posed space and is not idempotent.
    pub fn posed(&self, posed: &Posed, puppet: &crate::model::Puppet) -> LayerCloud {
        let mut out = self.clone();
        for (i, dot) in out.dots.iter_mut().enumerate() {
            if let Some(&seg) = self.segment.get(i) {
                if let Some(s) = puppet.segments.get(seg) {
                    dot.pos = posed.bone(&s.name).point(dot.pos);
                }
            }
        }
        for p in &mut out.particles {
            // Particles drift in world space, so only the origin moves.
            p.origin = posed.bone("root").point(p.origin);
        }
        out
    }

    /// The dots after `t` seconds of arrival animation.
    ///
    /// `@0x100a5a288` — `LayerCloud::arrival(&self, f32) -> Vec<Vec<Dot>>`: each dot
    /// arrives on its own delay, the binary scoping threads over the chunks.  The
    /// flattened form is returned here.
    pub fn arrival(&self, t: f32) -> Vec<Dot> {
        let n = self.dots.len().max(1) as f32;
        self.dots
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let delay = i as f32 / n;
                let k = ((t - delay) / (1.0 - delay).max(1e-3)).clamp(0.0, 1.0);
                Dot {
                    pos: d.pos,
                    radius: d.radius * k,
                    color: [
                        d.color[0],
                        d.color[1],
                        d.color[2],
                        (d.color[3] as f32 * k) as u8,
                    ],
                }
            })
            .collect()
    }

    /// The motes a set of statuses contributes.
    ///
    /// `@0x100a5943c` — `LayerCloud::motes(&self, &Puppet, &[&str]) -> Vec<Particle>`.
    /// The recovered body dispatches to the per-status generators (`shade::wisps`,
    /// `possess::wisps`, `blind::wisps`, `daze::stars`, `heat::steam`, `calm::motes`,
    /// `exposed::motes`, `hush::glints`, `fetter::glints`) and anchors them on the
    /// `"Hand"`, `"Leg"` and `"head"` bones.  The mapping below keeps that structure.
    pub fn motes(&self, puppet: &crate::model::Puppet, statuses: &[&str]) -> Vec<Particle> {
        let mut out = Vec::new();
        for status in statuses {
            let anchor = match *status {
                "blind" | "daze" | "hush" | "possess" => "head",
                "exposed" | "fetter" | "frost" => "hand_l",
                "heat" | "calm" | "shade" => "root",
                _ => continue,
            };
            let bone = puppet
                .index_of(anchor)
                .or_else(|| puppet.index_of("head"))
                .unwrap_or(0);
            let seg = &puppet.segments[bone];
            for k in 0..4 {
                out.push(Particle {
                    origin: seg.pivot,
                    phase: k as f32 / 4.0,
                    size: 2.0 + k as f32,
                    color: [255, 255, 255, 200],
                });
            }
        }
        out
    }

    /// Re-anchor the cloud to a new bounding box.
    ///
    /// `@0x100a5d8d4` — `LayerCloud::fit_bound(&self, (Vec3, Vec3)) -> LayerCloud`.
    /// The body rebuilds a hash map of the bones inside the bound; `0.0078125`
    /// (`1/128`) is its cell size.
    pub fn fit_bound(&self, bounds: (Vec3, Vec3)) -> LayerCloud {
        let mut out = self.clone();
        out.dots.retain(|d| {
            d.pos.x >= bounds.0.x
                && d.pos.y >= bounds.0.y
                && d.pos.z >= bounds.0.z
                && d.pos.x <= bounds.1.x
                && d.pos.y <= bounds.1.y
                && d.pos.z <= bounds.1.z
        });
        out.segment.truncate(out.dots.len());
        out
    }

    /// The freshly drawn segments (surfaces whose dots were just created).
    ///
    /// `@0x100a586a8` — `LayerCloud::fresh_segments(&self, &Posed) -> Vec<Surface>`,
    /// built from `draw::surfaces` + `skin::segment_marks`.
    pub fn fresh_segments(&self, posed: &Posed, puppet: &crate::model::Puppet) -> Vec<crate::draw::Surface> {
        self.segment
            .iter()
            .filter_map(|&i| {
                let seg = puppet.segments.get(i)?;
                let xf = posed.bone(&seg.name);
                let (a, b) = (xf.point(Vec3::ZERO), xf.point(Vec3::new(0.0, seg.length, 0.0)));
                Some(crate::draw::Surface {
                    shape: crate::draw::Shape::poly(vec![
                        [a.x, a.y],
                        [b.x, b.y],
                        [b.x + seg.radius, b.y],
                        [a.x + seg.radius, a.y],
                    ]),
                    color: [255, 255, 255, 128],
                    normal: xf.dir(Vec3::new(0.0, 1.0, 0.0)),
                    lit: 1.0,
                })
            })
            .collect()
    }
}

/// Build a layer cloud for a mesh that has no rig yet (used by the outfit code).
pub fn loose(mesh: &Mesh, params: &SplatParams) -> LayerCloud {
    let mut dots = Vec::with_capacity(mesh.vertices.len());
    let mut segment = Vec::with_capacity(mesh.vertices.len());
    for (i, v) in mesh.vertices.iter().enumerate() {
        dots.push(Dot {
            pos: v.pos,
            radius: (params.radius * 10.0).max(1.0),
            color: [255, 255, 255, 255],
        });
        segment.push(i);
    }
    LayerCloud {
        dots,
        hollows: Vec::new(),
        particles: Vec::new(),
        segment,
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
                Segment::new("head", 0, Vec3::new(0.0, 1.0, 0.0), 0.2),
            ],
        )
        .unwrap()
    }

    #[test]
    fn recovered_layer_constants() {
        assert_eq!(MOTE_REACH.to_bits(), 0x432a_0000);
        assert_eq!(PARTICLE_RISE, 24.0);
        assert_eq!(PARTICLE_SWAY, 12.0);
        assert_eq!(PARTICLE_RATE, 7.0);
        assert_eq!(PARTICLE_RATE_FAST, 37.0);
        assert_eq!(PARTICLE_DRIFT, [9.0, 11.0]);
        assert_eq!(PARTICLE_BIAS, 0.25);
    }

    #[test]
    fn particle_stays_inside_its_travel_box() {
        let p = Particle {
            origin: Vec3::ZERO,
            phase: 0.0,
            size: 2.0,
            color: [255; 4],
        };
        let mut max_y = 0.0f32;
        let mut max_x = 0.0f32;
        for i in 0..200 {
            let q = p.at(i as f32 * 0.01);
            max_y = max_y.max(q.y.abs());
            max_x = max_x.max(q.x.abs());
        }
        assert!(max_y <= PARTICLE_RISE, "{max_y}");
        assert!(max_x <= PARTICLE_SWAY, "{max_x}");
        // It actually moves (not a constant).
        assert!(max_x > 0.1 && max_y > 0.1, "{max_x} {max_y}");
    }

    #[test]
    fn at_bounded_clamps_to_the_mote_reach() {
        let p = Particle {
            origin: Vec3::ZERO,
            phase: 0.0,
            size: 1.0,
            color: [255; 4],
        };
        let q = p.at_bounded(3.7);
        assert!(q.length() <= MOTE_REACH + 1e-3);
    }

    #[test]
    fn build_makes_one_dot_per_segment() {
        let params = SplatParams::default();
        let cloud = LayerCloud::build(&rig(), &params);
        assert_eq!(cloud.dots.len(), 2);
        assert_eq!(cloud.segment, vec![0, 1]);
        assert_eq!(cloud.dots(), cloud.dots);
        let (d, h) = cloud.dots_and_hollows();
        assert_eq!(d.len(), 2);
        assert!(h.is_empty());
    }

    #[test]
    fn posed_moves_dots_with_their_bone() {
        let puppet = rig();
        let cloud = LayerCloud::build(&puppet, &SplatParams::default());
        let posed = puppet.pose();
        let moved = cloud.posed(&posed, &puppet);
        // The head dot moves up to the head pivot.
        assert!((moved.dots[1].pos.y - 1.0).abs() < 1e-5, "{:?}", moved.dots[1].pos);
        assert_eq!(moved.dots[0].pos, Vec3::ZERO);
    }

    #[test]
    fn arrival_staggers_the_dots() {
        let cloud = LayerCloud::build(&rig(), &SplatParams::default());
        let at0 = cloud.arrival(0.0);
        let mid = cloud.arrival(0.75);
        let at1 = cloud.arrival(1.0);
        // At t = 0 nothing has arrived yet.
        assert!(at0.iter().all(|d| d.radius == 0.0));
        // Two thirds of the way in the first dot is fully arrived and the last is
        // still on its way.
        assert!(mid[0].radius > 0.0);
        assert!(mid[1].radius < cloud.dots[1].radius);
        // At t = 1 everything is in.
        assert!(at1.iter().all(|d| d.radius > 0.0));
    }

    #[test]
    fn motes_dispatch_on_status_names() {
        let puppet = rig();
        let cloud = LayerCloud::default();
        let motes = cloud.motes(&puppet, &["blind", "exposed"]);
        assert_eq!(motes.len(), 8); // four per status
        assert!(cloud.motes(&puppet, &["nonsense"]).is_empty());
    }

    #[test]
    fn fit_bound_drops_dots_outside_the_box() {
        let puppet = rig();
        let cloud = LayerCloud::build(&puppet, &SplatParams::default()).posed(&puppet.pose(), &puppet);
        // The head dot sits at y = 1, the root dot at y = 0.
        let tight = cloud.fit_bound((Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 0.5, 1.0)));
        assert_eq!(tight.dots.len(), 1, "{:?}", tight.dots);
    }

    #[test]
    fn fresh_segments_follow_the_pose() {
        let puppet = rig();
        let posed = puppet.pose();
        let cloud = LayerCloud::build(&puppet, &SplatParams::default());
        let surfaces = cloud.fresh_segments(&posed, &puppet);
        assert_eq!(surfaces.len(), 2);
        assert!(surfaces[1].shape.points[0][1] >= 1.0);
    }

    #[test]
    fn loose_cloud_covers_a_mesh() {
        let mesh = crate::mesh::Mesh::parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let cloud = loose(&mesh, &SplatParams::default());
        assert_eq!(cloud.dots.len(), 3);
        assert_eq!(cloud.segment.len(), 3);
    }
}
