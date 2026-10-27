//! The rig: a flat list of [`Segment`]s in parent-before-child order.
//!
//! Recovered facts (`gs_puppet::model`):
//!
//! * `Puppet` owns a `Vec<Segment>` at `+0x30` (cap), `+0x38` (ptr),
//!   `+0x40` (len); `Puppet::subtree` (`0x1009d6734`) reads exactly those two
//!   words.
//! * A `Segment` is **224 bytes** (stride `0xe0` in `subtree`).  Its first word is
//!   a flag the subtree walk tests with `tbz w9, #0` (so a one-byte `bool`), and
//!   `+0x08` holds the **parent index** (`usize`) that the walk compares against
//!   the already-collected set.
//! * `Puppet::subtree` seeds the result with the root index, then walks indices
//!   `root+1 .. len` in order and keeps the ones whose parent is already in the
//!   set — i.e. the transitive descendants, relying on parents sorting before
//!   children.
//! * `PuppetError` has a `Display` impl (`<gs_puppet::model::PuppetError as
//!   core::fmt::Display>::fmt` @ `0x1009c9c7c`).
//!
//! Only the two fields the binary pins down (flag, parent) keep their recovered
//! meaning; the rest of the 224-byte record is [`UNKNOWN`] and modelled with the
//! minimum a rig needs: a name, a pivot offset and a length.

use crate::math::{Vec3, Xf};
use std::collections::BTreeMap;
use std::fmt;

/// A bone / rig segment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Segment {
    /// Recovered flag (word 0, tested with `tbz #0`): `true` when this segment
    /// only follows its parent's frame.  The model builder uses it for the
    /// "no independent motion" case.
    pub follower: bool,
    /// Recovered field `+0x08`: index of the parent segment.  The root's parent
    /// is itself; `subtree` relies on `parent <= index`.
    pub parent: usize,
    /// Bone name, the key the pose data uses (`"leg_l"`, `"torso_upper"`, …).
    pub name: String,
    /// Pivot of this segment inside its parent's frame, in model units.
    pub pivot: Vec3,
    /// Length along the segment's own axis.
    pub length: f32,
    /// Radius used by the splat/skin builders.
    pub radius: f32,
}

impl Segment {
    pub fn new(name: impl Into<String>, parent: usize, pivot: Vec3, length: f32) -> Segment {
        Segment {
            follower: false,
            parent,
            name: name.into(),
            pivot,
            length,
            radius: length * 0.5,
        }
    }

    pub fn with_radius(mut self, radius: f32) -> Segment {
        self.radius = radius;
        self
    }
}

/// Why a puppet could not be built or posed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PuppetError {
    /// A bone name the rig does not have.
    UnknownBone(String),
    /// The segment list is not a tree (parent index out of order / cyclic).
    BadRig(String),
    /// The baked TOML did not parse.
    BadBaked(String),
    /// Underlying I/O failure (the original carried an `std::io::Error`).
    Io(String),
}

impl fmt::Display for PuppetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PuppetError::UnknownBone(b) => write!(f, "no bone named {b}"),
            PuppetError::BadRig(m) => write!(f, "bad rig: {m}"),
            PuppetError::BadBaked(m) => write!(f, "bad baked puppet: {m}"),
            PuppetError::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for PuppetError {}

impl From<std::io::Error> for PuppetError {
    fn from(e: std::io::Error) -> PuppetError {
        PuppetError::Io(e.to_string())
    }
}

/// A rigged figure: parents-first segment list plus the bookkeeping `gs_render`
/// needs to bind poses, splats and skins to it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Puppet {
    /// `"biped"`, `"quadruped"`, … — selects the shipped pose file.
    pub body_type: String,
    /// Optional creature family, e.g. `"elephantid"`; family pose/limit
    /// overrides key off this.
    pub family: String,
    /// Parents-first segment list (see [`Segment::parent`]).
    pub segments: Vec<Segment>,
    /// Total rest height in model units.
    pub height: f32,
    /// Segment index of the left / right hand, when the rig has them.
    pub hands: [Option<usize>; 2],
    /// Segment index of the left / right foot, when the rig has them.
    pub feet: [Option<usize>; 2],
    /// Index of the root segment (always 0 for the recovered rigs).
    pub root: usize,
}

impl Puppet {
    /// Build a rig, validating the parent ordering the subtree walk assumes.
    pub fn new(body_type: impl Into<String>, segments: Vec<Segment>) -> Result<Puppet, PuppetError> {
        for (i, s) in segments.iter().enumerate() {
            if s.parent > i {
                return Err(PuppetError::BadRig(format!(
                    "segment {i} ({}) has parent {} which comes later",
                    s.name, s.parent
                )));
            }
        }
        if segments.is_empty() {
            return Err(PuppetError::BadRig("no segments".into()));
        }
        let mut p = Puppet {
            body_type: body_type.into(),
            family: String::new(),
            segments,
            height: 1.0,
            hands: [None, None],
            feet: [None, None],
            root: 0,
        };
        p.name_hands_and_feet();
        Ok(p)
    }

    fn name_hands_and_feet(&mut self) {
        for (i, s) in self.segments.iter().enumerate() {
            match s.name.as_str() {
                "hand_l" | "paw_l" | "forepaw_l" => self.hands[0] = Some(i),
                "hand_r" | "paw_r" | "forepaw_r" => self.hands[1] = Some(i),
                "foot_l" | "paw_l" | "forepaw_l" | "leg_l" | "hindleg_l" => {
                    self.feet[0] = Some(i)
                }
                "foot_r" | "paw_r" | "forepaw_r" | "leg_r" | "hindleg_r" => {
                    self.feet[1] = Some(i)
                }
                _ => {}
            }
        }
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.segments.iter().position(|s| s.name == name)
    }

    pub fn segment(&self, name: &str) -> Option<&Segment> {
        self.index_of(name).map(|i| &self.segments[i])
    }

    /// All descendants of `root`, including `root` itself, in rig order.
    ///
    /// `@0x1009d6734` — `gs_puppet::model::Puppet::subtree(&self, root: usize)
    /// -> Vec<usize>`.  The binary pushes `root`, then scans `root+1..len` in
    /// order, pushing each segment whose flag word bit 0 is set and whose parent
    /// index is already in the result.  Because parents always have smaller
    /// indices this is exactly the transitive descendant set, which is what the
    /// `hang`, `draw::severed` and outfit code wants.
    pub fn subtree(&self, root: usize) -> Vec<usize> {
        if root >= self.segments.len() {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(4);
        out.push(root);
        for i in (root + 1)..self.segments.len() {
            let seg = &self.segments[i];
            if seg.follower && !out.contains(&seg.parent) {
                // A follower whose parent is outside the subtree stays out — the
                // recovered walk tests membership before pushing.
                continue;
            }
            if out.contains(&seg.parent) {
                out.push(i);
            }
        }
        out
    }

    /// The bones that hang `from` off the rig, i.e. the subtree minus its root.
    ///
    /// `gs_puppet::hang::<impl Puppet>::hangs` (`0x1009d173c` is the thin
    /// wrapper `arm_hangs_at`; `hangs` collects the descendant indices into a
    /// `HashSet<usize>`).
    pub fn hangs(&self, from: usize) -> Vec<usize> {
        let mut v = self.subtree(from);
        if !v.is_empty() {
            v.remove(0);
        }
        v
    }

    /// Forward kinematics with no pose applied: every bone at its rest pivot.
    ///
    /// This is the pose the `["pose"].stand` sections describe (an empty table,
    /// so `stand` really is the rest pose).
    pub fn rest(&self) -> BTreeMap<String, Xf> {
        let mut out = BTreeMap::new();
        for (i, seg) in self.segments.iter().enumerate() {
            let parent = if i == 0 {
                Xf::IDENTITY
            } else {
                *out.get(&self.segments[seg.parent].name).unwrap_or(&Xf::IDENTITY)
            };
            let world = parent.then_inner(&Xf::from_translation(seg.pivot));
            out.insert(seg.name.clone(), world);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain() -> Puppet {
        // 0 root  -+- 1 chest -+- 2 neck
        //                 \---- 3 arm_l --- 4 hand_l
        //         \- 5 leg_l
        let segs = vec![
            Segment::new("root", 0, Vec3::ZERO, 0.9),
            Segment::new("chest", 0, Vec3::new(0.0, 0.55, 0.0), 0.3),
            Segment::new("neck", 1, Vec3::new(0.0, 0.3, 0.0), 0.15),
            Segment::new("arm_l", 1, Vec3::new(0.2, 0.25, 0.0), 0.3),
            Segment::new("hand_l", 3, Vec3::new(0.0, -0.3, 0.0), 0.1),
            Segment::new("leg_l", 0, Vec3::new(0.1, 0.0, 0.0), 0.45),
        ];
        Puppet::new("biped", segs).unwrap()
    }

    #[test]
    fn subtree_of_the_root_is_everything() {
        let p = chain();
        assert_eq!(p.subtree(0), vec![0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn subtree_of_an_inner_bone_is_its_descendants() {
        let p = chain();
        // chest keeps neck, arm_l and hand_l, but not the root or leg_l.
        assert_eq!(p.subtree(1), vec![1, 2, 3, 4]);
        // arm_l keeps only hand_l.
        assert_eq!(p.subtree(3), vec![3, 4]);
        // A leaf keeps itself.
        assert_eq!(p.subtree(5), vec![5]);
        assert_eq!(p.hangs(3), vec![4]);
    }

    #[test]
    fn subtree_of_an_out_of_range_root_is_empty() {
        let p = chain();
        assert!(p.subtree(99).is_empty());
    }

    #[test]
    fn bad_rigs_are_rejected() {
        let segs = vec![
            Segment::new("a", 1, Vec3::ZERO, 1.0),
            Segment::new("b", 0, Vec3::ZERO, 1.0),
        ];
        let err = Puppet::new("biped", segs).unwrap_err();
        assert!(matches!(err, PuppetError::BadRig(_)));
        assert!(Puppet::new("biped", vec![]).is_err());
    }

    #[test]
    fn hand_and_foot_indices_are_discovered() {
        let p = chain();
        assert_eq!(p.hands, [Some(4), None]);
        assert_eq!(p.feet, [Some(5), None]);
        assert_eq!(p.index_of("neck"), Some(2));
    }

    #[test]
    fn rest_pose_accumulates_pivots() {
        let p = chain();
        let rest = p.rest();
        // hand_l sits at chest.pivot + arm_l.pivot + hand_l.pivot down the chain.
        let h = rest["hand_l"].t;
        assert!((h.x - 0.2).abs() < 1e-6, "{h:?}");
        assert!((h.y - (0.55 + 0.25 - 0.3)).abs() < 1e-6, "{h:?}");
        assert!(h.z.abs() < 1e-6);
    }

    #[test]
    fn display_messages() {
        assert_eq!(
            PuppetError::UnknownBone("tail".into()).to_string(),
            "no bone named tail"
        );
        assert_eq!(PuppetError::BadRig("x".into()).to_string(), "bad rig: x");
    }
}
