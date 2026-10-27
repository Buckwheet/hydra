//! Joint sharing and the "ring" interpolation between two bone frames.
//!
//! Recovered facts (`gs_puppet::joints`):
//!
//! * `joints::<impl Posed>::ring_xf` (`@0x100a54760`) converts both frames'
//!   rotations to quaternions with `math::quat_from_m3`, interpolates them with
//!   `math::slerp` and lerps the translations — the two `fmov` immediates in its
//!   body are `0.5` (the default midpoint) and `1.0` (the identity weight).
//! * The binary has a `Vec<Vec<Option<gs_puppet::joints::Share>>>` (see the drop
//!   glue), i.e. a joint can be *shared* with a weighted list of other bones.
//!
//! [`Share`] models that shared-joint entry.

use crate::math::{quat_from_m3, quat_to_m3, slerp, Xf};
use crate::pose::Posed;

/// Where a joint's motion is blended in from.
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    /// Name of the bone the joint follows.
    pub bone: String,
    /// Blend weight, `0.0..=1.0`.
    pub weight: f32,
}

impl Share {
    pub fn new(bone: impl Into<String>, weight: f32) -> Share {
        Share {
            bone: bone.into(),
            weight: weight.clamp(0.0, 1.0),
        }
    }
}

/// Interpolate between two rigid frames.
///
/// `@0x100a54760` — `gs_puppet::joints::ring_xf`: `quat_from_m3` on both rotations,
/// `slerp`, then a linear translation blend.  This is the same operation as
/// [`crate::math::Xf::lerp`] but it is spelled out here because the joint code
/// calls it directly.
pub fn ring_xf(a: &Xf, b: &Xf, t: f32) -> Xf {
    let qa = quat_from_m3(&a.m);
    let qb = quat_from_m3(&b.m);
    Xf {
        m: quat_to_m3(&slerp(&qa, &qb, t)),
        t: a.t * (1.0 - t) + b.t * t,
    }
}

impl Posed {
    /// The ring transform between two bones of this pose, at `t`.
    ///
    /// `@0x100a54760` — `gs_puppet::joints::<impl Posed>::ring_xf(&self, &str,
    /// &str, f32) -> Xf`.  An unknown bone name falls back to the identity frame.
    pub fn ring_xf(&self, from: &str, to: &str, t: f32) -> Xf {
        ring_xf(&self.bone(from), &self.bone(to), t)
    }

    /// Blend two bones through a list of [`Share`]s.
    ///
    /// The recovered `Vec<Vec<Option<Share>>>` is a per-joint list of optional
    /// contributors; the weighted average is what the skinning code consumes.
    pub fn shared(&self, shares: &[Share]) -> Xf {
        let mut acc = Xf::default();
        let mut total = 0.0;
        for s in shares {
            if s.weight <= 0.0 {
                continue;
            }
            let xf = self.bone(&s.bone);
            acc.t = acc.t + xf.t * s.weight;
            for k in 0..9 {
                acc.m.m[k] += xf.m.m[k] * s.weight;
            }
            total += s.weight;
        }
        if total > 0.0 && total.is_finite() {
            let inv = 1.0 / total;
            acc.t = acc.t * inv;
            for k in 0..9 {
                acc.m.m[k] *= inv;
            }
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    #[test]
    fn ring_xf_hits_both_endpoints() {
        let a = Xf::from_translation(Vec3::new(0.0, 0.0, 0.0));
        let b = Xf::new(
            crate::math::Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(2.0, 0.0, 0.0),
        );
        assert_eq!(ring_xf(&a, &b, 0.0), a);
        let end = ring_xf(&a, &b, 1.0);
        assert!((end.t.x - 2.0).abs() < 1e-6);
        // Row-major Rz(90°): row 1 starts at +1.
        assert!((end.m.m[3] - 1.0).abs() < 1e-6, "{:?}", end.m.m);
        // The midpoint translation is halfway.
        let mid = ring_xf(&a, &b, 0.5);
        assert!((mid.t.x - 1.0).abs() < 1e-6);
    }

    #[test]
    fn posed_ring_xf_uses_bone_names() {
        use crate::model::{Puppet, Segment};
        let p = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("neck", 0, Vec3::new(0.0, 1.0, 0.0), 0.2),
            ],
        )
        .unwrap();
        let posed = p.pose();
        let mid = posed.ring_xf("root", "neck", 0.5);
        assert!((mid.t.y - 0.5).abs() < 1e-6, "{:?}", mid.t);
        // Unknown bones give the identity frame.
        assert_eq!(posed.ring_xf("nope", "nothing", 0.5), Xf::IDENTITY);
    }

    #[test]
    fn shares_are_weighted_averages() {
        use crate::model::{Puppet, Segment};
        let p = Puppet::new(
            "biped",
            vec![
                Segment::new("a", 0, Vec3::ZERO, 1.0),
                Segment::new("b", 0, Vec3::new(2.0, 0.0, 0.0), 1.0),
            ],
        )
        .unwrap();
        let posed = p.pose();
        let shares = [Share::new("a", 1.0), Share::new("b", 1.0)];
        let xf = posed.shared(&shares);
        assert!((xf.t.x - 1.0).abs() < 1e-6, "{:?}", xf.t);
        // A zero-weight list is the identity.
        assert_eq!(posed.shared(&[Share::new("a", 0.0)]), Xf::IDENTITY);
        // Weights clamp to 0..=1.
        assert_eq!(Share::new("a", 5.0).weight, 1.0);
        assert_eq!(Share::new("a", -1.0).weight, 0.0);
    }
}
