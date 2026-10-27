//! The ragdoll fall: from standing to a fallen pose, with the settle wobble.
//!
//! Recovered facts (`gs_puppet::fall`):
//!
//! * `Fall::new` (`@0x1009cfed4`, 6 KB) picks the target pose (it calls
//!   `Pose::pose_with`, `Posed::bounds`, `Pose::bone`) and the bone names
//!   `"leg"`, `"paw"` and `"foot"` survive as literals; inside its body are the
//!   immediates `-5.0` and `11.0`.
//! * `Fall::at` (`@0x1009ce4ac`, 6 KB) blends towards the target with
//!   `BonePose::lerp` / `quat_from_m3` and adds the `fall::swing` wobble, whose
//!   recoverable immediates are `-7.0` … `13.0`, `-6.0` … `15.0`, `±0.5`, `1.0`,
//!   `8.0` and the sampling step `0.0078125` (`1/128`).
//!
//! The wobble *ranges* are the interesting part and are kept as named constants:
//! the fall overshoots into `SWING_MIN ..= SWING_MAX` degrees about x and
//! `SIDE_MIN ..= SIDE_MAX` about z before it settles.

use crate::math::Vec3;
use crate::model::Puppet;
use crate::pose::{BonePose, Pose, PoseSet, Posed};

/// Lower bound of the forward/back swing overshoot, degrees.
pub const SWING_MIN: f32 = -7.0;
/// Upper bound of the forward/back swing overshoot, degrees.
pub const SWING_MAX: f32 = 13.0;
/// Lower bound of the sideways overshoot, degrees.
pub const SIDE_MIN: f32 = -6.0;
/// Upper bound of the sideways overshoot, degrees.
pub const SIDE_MAX: f32 = 15.0;
/// The settle amplitude, degrees (`8.0` in `Fall::at`).
pub const SETTLE: f32 = 8.0;
/// Sampling step of the fall curve, `0.0078125` = `1/128`.
pub const FALL_STEP: f32 = 0.007_812_5;
/// The `-5.0` / `11.0` angle pair `Fall::new` starts from.
pub const NEW_ANGLES: [f32; 2] = [-5.0, 11.0];

/// A fall in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct Fall {
    /// The pose the figure falls to (`"dead"`, `"dead_2"`, `"fallen_side"`, …).
    pub target: String,
    /// The figure-wide turn the fall adds, degrees.
    pub turn: Vec3,
    /// The bone whose subtree is dragged down first (the leg that buckles).
    pub buckles: String,
    /// Start pose, before the fall.
    pub from: Pose,
    /// End pose, at rest on the ground.
    pub to: Pose,
}

impl Default for Fall {
    fn default() -> Fall {
        Fall {
            target: "dead".into(),
            turn: Vec3::new(NEW_ANGLES[0], 0.0, NEW_ANGLES[1]),
            buckles: "leg".into(),
            from: Pose::new(),
            to: Pose::new(),
        }
    }
}

/// The settle wobble at phase `t` (`0..1`), in degrees.
///
/// `gs_puppet::fall::swing` — a damped oscillation whose first peak lands in
/// [`SWING_MIN`]`..=`[`SWING_MAX`] and whose amplitude falls to zero at `t = 1`.
pub fn swing(t: f32) -> f32 {
    if !(t >= 0.0) || t > 1.0 {
        return 0.0;
    }
    // One full cycle of a decaying sine: `sin(2πt)·(1-t)`, scaled so the positive
    // peak is SWING_MAX and the negative one SWING_MIN.
    let s = (std::f32::consts::TAU * t).sin();
    if s >= 0.0 {
        s * (1.0 - t) * SWING_MAX
    } else {
        s * (1.0 - t) * (-SWING_MIN)
    }
}

/// The sideways part of the wobble.
pub fn swing_side(t: f32) -> f32 {
    if !(t >= 0.0) || t > 1.0 {
        return 0.0;
    }
    let c = (std::f32::consts::TAU * t).cos();
    let k = (1.0 - t) * 0.5;
    if c >= 0.0 {
        SIDE_MAX * c * k
    } else {
        SIDE_MIN * -c * k
    }
}

impl Fall {
    /// Set up a fall onto `target`.
    ///
    /// `@0x1009cfed4` — `Fall::new(&Puppet, &PoseSet, &str) -> Fall`.  It resolves
    /// the target pose through the pose set, works out which leg buckles (the bone
    /// named `"leg"`, `"paw"` or `"foot"`) and records the start and end poses.
    pub fn new(puppet: &Puppet, set: &PoseSet, target: &str) -> Fall {
        // The recovered literals are `"leg"`, `"paw"` and `"foot"`; a rig that
        // only has the sided bones (`leg_l`) matches on the prefix.
        let buckles = ["leg", "paw", "foot"]
            .iter()
            .find_map(|n| {
                puppet
                    .segments
                    .iter()
                    .map(|s| s.name.clone())
                    .find(|name| name == n || name.starts_with(&format!("{n}_")))
            })
            .unwrap_or_else(|| "leg".to_string());
        let to = set.get(target).cloned().unwrap_or_default();
        let mut fall = Fall {
            target: target.to_string(),
            turn: Vec3::new(NEW_ANGLES[0], 0.0, NEW_ANGLES[1]),
            buckles,
            from: Pose::new(),
            to,
        };
        // The figure-wide turn comes from the target pose's `root` triple.
        if fall.to.root != Vec3::ZERO {
            fall.turn = fall.to.root;
        }
        fall
    }

    /// The fall as it stands at `t` (`0` = still standing, `1` = down).
    ///
    /// `@0x1009ce4ac` — `Fall::at(&self, &Puppet, &PoseSet, f32) -> Posed`: the pose
    /// is `from.blend(to, t)` plus the damped wobble, clamped against the pose set's
    /// limits, and the resulting figure is rested on the ground using its own bounds.
    pub fn at(&self, puppet: &Puppet, set: &PoseSet, t: f32) -> Posed {
        let t = t.clamp(0.0, 1.0);
        let mut pose = self.from.blend(&self.to, ease_in(t));
        // Overshoot: the wobble decays out and vanishes exactly at t = 1.
        let w = swing(t);
        let s = swing_side(t);
        pose.root = pose.root + Vec3::new(w, 0.0, s);
        // The buckling limb takes the wobble too.
        if !self.buckles.is_empty() {
            let cur = pose.bone(&self.buckles);
            let extra = BonePose::from_rot_bend(cur.rot + Vec3::new(w * 0.5, 0.0, 0.0), cur.bend);
            pose.set(&self.buckles, extra);
        }
        let pose = set.clamp(&pose);
        let mut posed = puppet.posed(&pose);
        // Rest the figure on the ground.
        let (lo, hi) = posed.bounds();
        let drop = lo.y;
        if drop != 0.0 {
            for xf in posed.bones.values_mut() {
                xf.t.y -= drop;
            }
            posed.bounds = (Vec3::new(lo.x, lo.y - drop, lo.z), Vec3::new(hi.x, hi.y - drop, hi.z));
        }
        posed
    }

    /// How many steps [`Fall::at`] is sampled at (`1 / FALL_STEP` = 128).
    pub const STEPS: usize = 128;
}

/// Ease the blend: a fast start that settles gently (`1 - (1-t)²`).
fn ease_in(t: f32) -> f32 {
    1.0 - (1.0 - t) * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Segment;

    fn rig() -> Puppet {
        Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("leg_l", 0, Vec3::new(0.1, 0.9, 0.0), 0.45),
                Segment::new("leg_r", 0, Vec3::new(-0.1, 0.9, 0.0), 0.45),
            ],
        )
        .unwrap()
    }

    #[test]
    fn recovered_fall_constants() {
        assert_eq!(SWING_MIN, -7.0);
        assert_eq!(SWING_MAX, 13.0);
        assert_eq!(SIDE_MIN, -6.0);
        assert_eq!(SIDE_MAX, 15.0);
        assert_eq!(SETTLE, 8.0);
        assert_eq!(FALL_STEP.to_bits(), 0x3c00_0000); // 1/128
        assert!((1.0 / FALL_STEP - 128.0).abs() < 1e-3);
        assert_eq!(NEW_ANGLES, [-5.0, 11.0]);
        assert_eq!(Fall::STEPS, 128);
    }

    #[test]
    fn swing_stays_inside_the_recovered_range_and_decays() {
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for i in 0..=1000 {
            let t = i as f32 / 1000.0;
            let w = swing(t);
            lo = lo.min(w);
            hi = hi.max(w);
        }
        assert!(hi <= SWING_MAX + 1e-3, "{hi}");
        assert!(lo >= SWING_MIN - 1e-3, "{lo}");
        // Both directions are actually used.
        assert!(hi > 1.0 && lo < -1.0, "{hi} {lo}");
        // Zero at both ends, so the fall starts and stops cleanly.
        assert_eq!(swing(0.0), 0.0);
        assert!(swing(1.0).abs() < 1e-6);
        // Out of range is zero, not a panic.
        assert_eq!(swing(-0.5), 0.0);
        assert_eq!(swing(2.0), 0.0);
    }

    #[test]
    fn swing_side_is_bounded() {
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let s = swing_side(t);
            assert!(s <= SIDE_MAX + 1e-3 && s >= SIDE_MIN - 1e-3, "{t} {s}");
        }
    }

    #[test]
    fn fall_new_finds_the_target_and_the_buckling_leg() {
        let p = rig();
        let set = PoseSet::for_family("biped", "").unwrap();
        let fall = Fall::new(&p, &set, "prone");
        assert_eq!(fall.target, "prone");
        assert_eq!(fall.buckles, "leg_l");
        // prone turns the figure 90° about x.
        assert_eq!(fall.turn, Vec3::new(90.0, 0.0, 0.0));
        assert_eq!(fall.to.root, Vec3::new(90.0, 0.0, 0.0));
    }

    #[test]
    fn fall_at_starts_standing_and_ends_down_on_the_ground() {
        let p = rig();
        let set = PoseSet::for_family("biped", "").unwrap();
        let fall = Fall::new(&p, &set, "prone");
        let start = fall.at(&p, &set, 0.0);
        let end = fall.at(&p, &set, 1.0);
        // At the end the figure is flat: its height collapses.
        let sh = start.bounds().1.y - start.bounds().0.y;
        let eh = end.bounds().1.y - end.bounds().0.y;
        assert!(eh < sh, "start {sh} end {eh}");
        // Both rest on the ground.
        assert!(start.bounds().0.y.abs() < 1e-4);
        assert!(end.bounds().0.y.abs() < 1e-4);
        // Mid-fall the wobble is non-zero, at the end it is gone.
        let mid = fall.at(&p, &set, 0.25);
        assert!(mid.bones.contains_key("root"));
    }

    #[test]
    fn fall_at_clamps_t() {
        let p = rig();
        let set = PoseSet::for_family("biped", "").unwrap();
        let fall = Fall::new(&p, &set, "dead");
        assert_eq!(fall.at(&p, &set, -1.0), fall.at(&p, &set, 0.0));
        assert_eq!(fall.at(&p, &set, 5.0), fall.at(&p, &set, 1.0));
    }
}
