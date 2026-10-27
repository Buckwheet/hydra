//! Which bones hang from a held or severed point.
//!
//! `gs_puppet::hang::<impl model::Puppet>::arm_hangs_at` (`@0x1009d173c`, 6 KB) and
//! the local `hangs` helper it shares with `outfit::hang`: the pair walks a bone's
//! [`crate::model::Puppet::subtree`] and collects the indices into a
//! `HashSet<usize>` (the closure shapes `FlatMap<…, HashSet<usize> as Extend>::…`
//! survive in the drop glue).  `arm_hangs_at` additionally filters for the arm's
//! descendants that actually swing *downwards* at a given pose.

use crate::math::Vec3;
use crate::model::Puppet;
use crate::pose::Pose;

impl Puppet {
    /// The bones that hang from `from`, in rig order, without `from` itself.
    ///
    /// `gs_puppet::hang::<impl Puppet>::hangs`.
    pub fn hangs_from(&self, from: usize) -> Vec<usize> {
        self.hangs(from)
    }

    /// The bones that hang from the arm on `side` (`0` = left, `1` = right) when the
    /// figure holds that pose.
    ///
    /// `@0x1009d173c` — `gs_puppet::hang::<impl Puppet>::arm_hangs_at(&self, &Pose,
    /// usize) -> Vec<usize>`.  A bone counts as hanging when at least one of its
    /// descendants ends up *below* the bone's own pivot in the posed figure; the
    /// exact threshold could not be recovered, so the strict "below" test is used.
    pub fn arm_hangs_at(&self, pose: &Pose, side: usize) -> Vec<usize> {
        let arm = match self.index_of(if side == 0 { "arm_l" } else { "arm_r" }) {
            Some(i) => i,
            None => {
                // Fall back to the pair key.
                match self.index_of("arm") {
                    Some(i) => i,
                    None => return Vec::new(),
                }
            }
        };
        let posed = self.posed(pose);
        let pivot = posed.bone(&self.segments[arm].name).point(Vec3::ZERO);
        let mut out = Vec::new();
        for i in self.hangs(arm) {
            let name = &self.segments[i].name;
            let tip = posed.bone(name).point(Vec3::ZERO);
            if tip.y < pivot.y {
                out.push(i);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Segment;
    use crate::pose::BonePose;

    fn rig() -> Puppet {
        Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("arm_l", 0, Vec3::new(0.2, 0.6, 0.0), 0.3),
                Segment::new("hand_l", 1, Vec3::new(0.0, -0.3, 0.0), 0.1),
                Segment::new("arm_r", 0, Vec3::new(-0.2, 0.6, 0.0), 0.3),
            ],
        )
        .unwrap()
    }

    #[test]
    fn hangs_from_is_the_subtree_minus_its_root() {
        let p = rig();
        assert_eq!(p.hangs_from(1), vec![2]);
        assert!(p.hangs_from(2).is_empty());
    }

    #[test]
    fn arm_hangs_at_finds_the_hand_at_rest() {
        let p = rig();
        let pose = Pose::new();
        // The rest arm points down the bone's +y, which the rig places downwards,
        // so the hand counts as hanging.
        let hangs = p.arm_hangs_at(&pose, 0);
        assert!(hangs.contains(&2), "{hangs:?}");
        // The right arm has no descendants.
        assert!(p.arm_hangs_at(&pose, 1).is_empty());
        // A rig without arms answers with nothing at all.
        let bare = Puppet::new("mound", vec![Segment::new("root", 0, Vec3::ZERO, 1.0)]).unwrap();
        assert!(bare.arm_hangs_at(&pose, 0).is_empty());
    }

    #[test]
    fn an_arm_raised_above_the_pivot_stops_hanging() {
        let p = rig();
        let mut pose = Pose::new();
        // Rotate the arm 180° about z: the hand now points up.
        pose.set("arm_l", BonePose::from_rot(Vec3::new(0.0, 0.0, 180.0)));
        let hangs = p.arm_hangs_at(&pose, 0);
        assert!(!hangs.contains(&2), "{hangs:?}");
    }
}
