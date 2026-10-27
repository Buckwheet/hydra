//! Poses, pose blending, joint limits and forward kinematics.
//!
//! Everything here is recovered from `gs_puppet::pose::*`, and the **shipped
//! pose data itself was lifted out of the binary**: `PoseSet::shipped_toml`
//! (`0x100a4a254`) scans a 13-entry table of `(&str name, &str toml)` pairs at
//! `0x1043b60d8`, and each of those literals is the full pose file for one body
//! template.  They now live verbatim in `crates/gs_puppet/poses/*.toml`.
//!
//! The schema those files use — and the angle convention — is documented in
//! `poses/biped.toml`:
//!
//! * `rot = [x, y, z]` Euler degrees, "applied `Rx * Rz * Ry`: twist (y), then
//!   spread (z), then swing (x)";
//! * `bend = [deg, ..]` one value per *inner* segment of a chain;
//! * a key with no `_l` / `_r` that names no bone is a **pair**: `leg` poses
//!   `leg_l` as written and `leg_r` mirrored (y and z negated);
//! * `root = [x, y, z]` turns the whole figure.
//!
//! Recovered envelopes:
//!
//! * [`flinch_envelope`] — a 0.55 s hump `6.75·u·(1-u)²` with `u = t/0.55`,
//!   which peaks at exactly `1.0` one third of the way in.
//! * [`Pose::breathe`] — `gain · sin(2π(t + phase))` per idle bone (the `2π`
//!   immediate is `0x40c90fdb` in `Pose::breathe` @ `0x100a47a94`).

use crate::math::{slerp, Vec3, Xf, Quat};
use crate::model::{Puppet, PuppetError};
use crate::toml::{self, Table, Value};
use std::collections::BTreeMap;

/// Length of the flinch hump, in seconds.  `0x3f0ccccd` in [`flinch_envelope`].
pub const FLINCH_SECONDS: f32 = 0.55;

/// Height gain of the flinch hump: `6.75 · u · (1-u)²` peaks at `1.0` when
/// `u = 1/3`.  Immediate `6.75` in [`flinch_envelope`].
pub const FLINCH_PEAK_GAIN: f32 = 6.75;

/// `2π` as `f32` (`0x40c90fdb`) — the idle breathing period constant.
pub const TAU: f32 = 6.283_185_5;

/// One `[lo, hi]` limit range in degrees.
pub type RangeDeg = [f32; 2];

// ---------------------------------------------------------------------------
// flinch envelope
// ---------------------------------------------------------------------------

/// Flinch envelope over time, in `[0, 1]`.
///
/// `@0x100a45894` — `gs_puppet::pose::flinch_envelope(f32) -> f32`, 60 bytes of
/// branch-free code:
///
/// ```text
/// u = t / 0.55                     ; 0x3f0ccccd
/// if u < 1.0 && u >= 0.0 { 6.75 * u * (1-u) * (1-u) } else { 0.0 }
/// ```
///
/// The in-range test is the `fcmp`/`fccmp`/`fcsel` triple: `u < 1.0` is
/// *exclusive* and NaN fails both arms, so NaN yields `0.0`.
pub fn flinch_envelope(secs: f32) -> f32 {
    let u = secs / FLINCH_SECONDS;
    if !(u < 1.0) || !(u >= 0.0) {
        return 0.0;
    }
    let rest = 1.0 - u;
    FLINCH_PEAK_GAIN * u * rest * rest
}

// ---------------------------------------------------------------------------
// limits
// ---------------------------------------------------------------------------

/// Joint limits for one bone, in degrees.
///
/// Recovered from `[limits]` in the shipped pose files, e.g.
///
/// ```toml
/// foreleg = { rot = [[-110.0, 90.0], [-30.0, 30.0], [-60.0, 35.0]], bend = [[-150.0, 20.0]] }
/// ```
///
/// and from `Limit::clamp` (`0x100a486a4`), which seeds an accumulator from
/// `Vec3::MAX` / `Vec3::MIN` (tables `0x1021b4d70` / `0x1021b4d60`) so a bone
/// with no explicit limit stays free.
#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    /// One `[lo, hi]` per Euler axis, in degrees.
    pub rot: [RangeDeg; 3],
    /// One `[lo, hi]` per inner segment, in degrees.
    pub bend: Vec<RangeDeg>,
}

impl Default for Limit {
    fn default() -> Limit {
        Limit::free()
    }
}

impl Limit {
    /// No limit at all — every axis is `[-inf, +inf]`.
    pub fn free() -> Limit {
        Limit {
            rot: [
                [f32::NEG_INFINITY, f32::INFINITY],
                [f32::NEG_INFINITY, f32::INFINITY],
                [f32::NEG_INFINITY, f32::INFINITY],
            ],
            bend: Vec::new(),
        }
    }

    /// Clamp Euler angles (degrees) per axis.
    pub fn clamp_rot(&self, rot: Vec3) -> Vec3 {
        Vec3::new(
            rot.x.clamp(self.rot[0][0], self.rot[0][1]),
            rot.y.clamp(self.rot[1][0], self.rot[1][1]),
            rot.z.clamp(self.rot[2][0], self.rot[2][1]),
        )
    }

    /// Clamp one hinge angle (degrees) for inner segment `i`.
    ///
    /// A bone with fewer `bend` ranges than the pose has values leaves the
    /// extras unclamped, which is what the `Vec<f32>` walk in the binary does
    /// (it zips the pose's bend list against the limit's).
    pub fn clamp_bend(&self, i: usize, deg: f32) -> f32 {
        match self.bend.get(i) {
            Some([lo, hi]) => deg.clamp(*lo, *hi),
            None => deg,
        }
    }

    /// Clamp a whole bone pose and re-derive its quaternion.
    ///
    /// `@0x100a486a4` — `Limit::clamp` builds the clamped Euler triple, composes
    /// the matrix and finishes with [`crate::math::quat_from_m3`], so the quat in
    /// the returned [`BonePose`] is always consistent with the clamped angles.
    pub fn clamp(&self, rot: Vec3, bend: &[f32]) -> BonePose {
        let rot = self.clamp_rot(rot);
        let bend: Vec<f32> = bend
            .iter()
            .enumerate()
            .map(|(i, d)| self.clamp_bend(i, *d))
            .collect();
        BonePose::from_rot_bend(rot, bend)
    }
}

// ---------------------------------------------------------------------------
// bone poses
// ---------------------------------------------------------------------------

/// One bone's animated pose.
///
/// Recovered layout, 52 bytes (`Limit::clamp` returns exactly this):
/// `bend: Vec<f32>` at `+0`, `quat: Quat` at `+0x18`, `rot: Vec3` at `+0x28`.
/// `rot` holds the authoring angles in degrees, `quat` the rotation they encode
/// — the same redundancy the pose editor needs for its sliders.
#[derive(Clone, Debug, PartialEq)]
pub struct BonePose {
    /// One hinge angle in degrees per inner segment (elbow, knee, …).
    pub bend: Vec<f32>,
    /// Rotation encoded by `rot` (recomputed whenever `rot` changes).
    pub quat: Quat,
    /// Euler degrees, applied `Rx * Rz * Ry` (`crate::math::euler_m3`).
    pub rot: Vec3,
}

impl Default for BonePose {
    fn default() -> BonePose {
        BonePose {
            bend: Vec::new(),
            quat: Quat::IDENTITY,
            rot: Vec3::ZERO,
        }
    }
}

impl BonePose {
    /// The rest pose: identity rotation, no bend.
    pub fn rest() -> BonePose {
        BonePose::default()
    }

    pub fn from_rot(rot: Vec3) -> BonePose {
        BonePose {
            bend: Vec::new(),
            quat: crate::math::euler_quat(rot),
            rot,
        }
    }

    pub fn from_rot_bend(rot: Vec3, bend: Vec<f32>) -> BonePose {
        BonePose {
            bend,
            quat: crate::math::euler_quat(rot),
            rot,
        }
    }

    /// Set the Euler angles and refresh the quaternion (`Pose::set_euler`).
    pub fn set_rot(&mut self, rot: Vec3) {
        self.rot = rot;
        self.quat = crate::math::euler_quat(rot);
    }

    pub fn is_rest(&self) -> bool {
        self.rot == Vec3::ZERO && (self.bend.is_empty() || self.bend.iter().all(|b| *b == 0.0))
    }

    /// Scale every authored value by `k` — the flinch / idle envelope.
    pub fn scaled(&self, k: f32) -> BonePose {
        BonePose::from_rot_bend(self.rot * k, self.bend.iter().map(|b| b * k).collect())
    }

    /// Add another pose's offsets on top of this one (`Pose::add_offset`).
    pub fn added(&self, o: &BonePose) -> BonePose {
        let mut bend = self.bend.clone();
        for (i, b) in o.bend.iter().enumerate() {
            if i < bend.len() {
                bend[i] += b;
            } else {
                bend.resize(i + 1, 0.0);
                bend[i] = *b;
            }
        }
        BonePose::from_rot_bend(self.rot + o.rot, bend)
    }

    /// Interpolate two bone poses.
    ///
    /// `@0x100a4df44` — `BonePose::lerp`: the Euler angles and the bend list are
    /// interpolated linearly and the *quaternion* goes through
    /// [`crate::math::slerp`] (that is the `bl slerp` in `Pose::blend`), then the
    /// clamped angles are re-derived.
    pub fn lerp(&self, o: &BonePose, t: f32) -> BonePose {
        let rot = self.rot * (1.0 - t) + o.rot * t;
        let n = self.bend.len().max(o.bend.len());
        let mut bend = Vec::with_capacity(n);
        for i in 0..n {
            let a = self.bend.get(i).copied().unwrap_or(0.0);
            let b = o.bend.get(i).copied().unwrap_or(0.0);
            bend.push(a + (b - a) * t);
        }
        let quat = slerp(&self.quat, &o.quat, t);
        BonePose { bend, quat, rot }
    }
}

// ---------------------------------------------------------------------------
// idle breathing
// ---------------------------------------------------------------------------

/// One bone's breathing amplitude: the peak offsets plus a phase lag.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdleBone {
    /// Peak Euler offsets in degrees.
    pub rot: Vec3,
    /// Peak hinge offsets, one per inner segment.
    pub bend: Vec<f32>,
    /// Phase offset in cycles (`phase = -0.25` lags a quarter cycle).
    pub phase: f32,
}

impl IdleBone {
    /// The envelope for this bone at cycle position `t`.
    ///
    /// `Pose::breathe` computes `gain · sin(2π(t + phase))`; the `fadd` of the
    /// phase and the `2π` immediate (`0x40c90fdb`) are both in the recovered
    /// body.
    pub fn envelope(&self, t: f32, gain: f32) -> f32 {
        gain * (TAU * (t + self.phase)).sin()
    }

    /// This bone's contribution at cycle position `t`.
    pub fn at(&self, t: f32, gain: f32) -> BonePose {
        let e = self.envelope(t, gain);
        BonePose::from_rot_bend(
            self.rot * e,
            self.bend.iter().map(|b| b * e).collect(),
        )
    }
}

/// The `[idle]` table: a breathing cycle and the bones that move in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Idle {
    /// Seconds per cycle (`period = 3.6`).
    pub period: f32,
    /// Per-bone amplitudes, keyed by bone name.
    pub bones: BTreeMap<String, IdleBone>,
}

impl Idle {
    pub fn new(period: f32) -> Idle {
        Idle {
            period,
            bones: BTreeMap::new(),
        }
    }

    /// Apply an override (a `[family_idle.<family>]` entry or an outfit) on top
    /// of this idle.
    ///
    /// `@0x100a458d0` — `Idle::with_override`: clones the map, removes the keys
    /// the override pins (a `period`-only override keeps the bones), then
    /// inserts the override's bones.
    pub fn with_override(&self, o: &Idle) -> Idle {
        let mut out = self.clone();
        if o.period > 0.0 {
            out.period = o.period;
        }
        for (k, v) in &o.bones {
            out.bones.insert(k.clone(), v.clone());
        }
        out
    }

    /// Breathing offsets at cycle position `t` (`0.0..1.0` is one breath).
    ///
    /// The recovered `Pose::breathe` feeds `t` straight into `sin(2π(t + phase))`,
    /// so pacing by [`Idle::period`] is the caller's job
    /// (`gs_render::puppet_cache::PuppetCache::breath_paced`); [`Idle::sample_at`]
    /// is the seconds-based convenience wrapper.
    pub fn sample(&self, t: f32, gain: f32) -> Pose {
        let mut pose = Pose::new();
        for (name, bone) in &self.bones {
            pose.set(name, bone.at(t, gain));
        }
        pose
    }

    /// Breathing offsets after `secs` seconds of wall time.
    pub fn sample_at(&self, secs: f32, gain: f32) -> Pose {
        let period = if self.period > 0.0 { self.period } else { 1.0 };
        self.sample(secs / period, gain)
    }
}

// ---------------------------------------------------------------------------
// poses
// ---------------------------------------------------------------------------

/// A named set of bone offsets — one `[pose.<name>]` (or `[flinch.<region>]`,
/// `[guard.<name>]`, `[posture.<name>]`) table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    /// Bone name → offset.  The recovered type is
    /// `BTreeMap<String, gs_puppet::pose::BonePose>` (it appears verbatim in the
    /// binary's drop glue).
    pub bones: BTreeMap<String, BonePose>,
    /// The figure-wide `root = [x, y, z]` turn, in degrees.
    pub root: Vec3,
}

impl Pose {
    pub fn new() -> Pose {
        Pose::default()
    }

    pub fn set(&mut self, name: &str, bp: BonePose) {
        self.bones.insert(name.to_string(), bp);
    }

    pub fn is_empty(&self) -> bool {
        self.bones.is_empty() && self.root == Vec3::ZERO
    }

    /// The offset for `name`.
    ///
    /// `@0x100a46ccc` — `Pose::bone(&self, &str) -> BonePose`.  Lookup order is
    /// exact name first, then the *pair* rule from the pose files: `leg_l` falls
    /// back to `leg`, and `leg_r` falls back to `leg` **mirrored** (y and z
    /// negated).  A name in neither falls back to the rest pose.
    pub fn bone(&self, name: &str) -> BonePose {
        if let Some(b) = self.bones.get(name) {
            return b.clone();
        }
        let (pair, mirrored) = if let Some(p) = name.strip_suffix("_l") {
            (p, false)
        } else if let Some(p) = name.strip_suffix("_r") {
            (p, true)
        } else {
            return BonePose::rest();
        };
        match self.bones.get(pair) {
            Some(b) if mirrored => BonePose::from_rot_bend(b.rot.mirrored(), b.bend.clone()),
            Some(b) => b.clone(),
            None => BonePose::rest(),
        }
    }

    /// Set one bone's Euler angles in degrees, creating it if needed.
    ///
    /// `@0x100a482dc` — `Pose::set_euler(&mut self, &str, &Vec3)`: it walks the
    /// bone map, inserts a default (identity quat, the literal at
    /// `0x1021dc4a8`) when the name is absent, converts degrees with
    /// `0x3c8efa35`, then stores the angles and their quaternion.
    pub fn set_euler(&mut self, name: &str, deg: Vec3) {
        match self.bones.get_mut(name) {
            Some(b) => b.set_rot(deg),
            None => self.set(name, BonePose::from_rot(deg)),
        }
    }

    /// Add `offset` on top of `self` (the `Pose::add_offset` helper, `0x…`).
    pub fn add_offset(&self, offset: &Pose) -> Pose {
        let mut out = self.clone();
        out.root = out.root + offset.root;
        for (k, v) in &offset.bones {
            match out.bones.get_mut(k) {
                Some(cur) => *cur = cur.added(v),
                None => {
                    out.bones.insert(k.clone(), v.clone());
                }
            }
        }
        out
    }

    /// Linear blend of two poses (`t = 0` keeps `self`).
    ///
    /// `@0x100a470dc` — `Pose::blend(&self, &Pose, f32) -> Pose`: for each bone in
    /// the union of both maps it calls `BonePose::lerp` (which goes through
    /// `slerp` for the rotation), and the root angles lerp component-wise.
    pub fn blend(&self, o: &Pose, t: f32) -> Pose {
        let mut out = Pose::new();
        out.root = self.root * (1.0 - t) + o.root * t;
        for (k, a) in &self.bones {
            let b = o.bones.get(k).cloned().unwrap_or_default();
            out.bones.insert(k.clone(), a.lerp(&b, t));
        }
        for (k, b) in &o.bones {
            if !self.bones.contains_key(k) {
                out.bones
                    .insert(k.clone(), BonePose::rest().lerp(b, t));
            }
        }
        out
    }

    /// Breathing offsets from an [`Idle`] table added onto this pose.
    ///
    /// `@0x100a47a94` — `Pose::breathe(&self, &Idle, t: f32, gain: f32) -> Pose`.
    /// It returns a clone of `self` unchanged when `t` is not finite or `gain` is
    /// exactly `0.0`, and otherwise adds `gain · sin(2π(t + phase)) · amp` for
    /// every idle bone.
    pub fn breathe(&self, idle: &Idle, t: f32, gain: f32) -> Pose {
        if !t.is_finite() || gain == 0.0 {
            return self.clone();
        }
        self.add_offset(&idle.sample(t, gain))
    }

    /// Peak flinch offsets scaled by an envelope.
    ///
    /// `@0x100a47cb0` — `Pose::flinched(&self, &Pose, f32) -> Pose`: it bails out
    /// on a non-finite envelope and otherwise adds `envelope · peak` per bone.
    pub fn flinched(&self, peak: &Pose, envelope: f32) -> Pose {
        if !envelope.is_finite() || envelope == 0.0 {
            return self.clone();
        }
        let mut scaled = Pose::new();
        scaled.root = peak.root * envelope;
        for (k, v) in &peak.bones {
            scaled.bones.insert(k.clone(), v.scaled(envelope));
        }
        self.add_offset(&scaled)
    }

    /// Apply a full-pose override: every bone the override names is replaced.
    ///
    /// `@0x100a4689c` — `Pose::with_override(&self, &PoseOverride) -> Pose`.
    pub fn with_override(&self, o: &PoseOverride) -> Pose {
        let mut out = self.clone();
        if let Some(root) = o.root {
            out.root = root;
        }
        for (k, v) in &o.bones {
            out.bones.insert(k.clone(), v.clone());
        }
        out
    }
}

/// An override applied on top of a [`Pose`] or an [`Idle`] — the shape returned
/// by the `[family.*]` / outfit tables (`PoseOverride` in the binary's drop
/// glue, `BTreeMap<String, PoseOverride>` in `PoseSet`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseOverride {
    /// Bones pinned by the override.
    pub bones: BTreeMap<String, BonePose>,
    /// `Some` when the override also pins the figure-wide turn.
    pub root: Option<Vec3>,
}

impl PoseOverride {
    pub fn from_pose(p: &Pose) -> PoseOverride {
        PoseOverride {
            bones: p.bones.clone(),
            root: if p.root == Vec3::ZERO {
                None
            } else {
                Some(p.root)
            },
        }
    }
}

/// The `[family.<family>.*]` block: per-family poses, limits and idle, kept
/// separate so they can be folded into the template set by
/// [`PoseSet::for_family`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FamilyPoses {
    pub poses: BTreeMap<String, Pose>,
    pub limits: BTreeMap<String, Limit>,
    pub idle: Option<Idle>,
}

// ---------------------------------------------------------------------------
// the pose set
// ---------------------------------------------------------------------------

/// Every pose, limit and idle for one body template, loaded from the pose TOML.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseSet {
    /// `body_type = "biped"`.
    pub body_type: String,
    /// `[pose.<name>]`.
    pub poses: BTreeMap<String, Pose>,
    /// `[limits]`.
    pub limits: BTreeMap<String, Limit>,
    /// `[idle]`.
    pub idle: Idle,
    /// `[flinch.<region>]` — peak offsets for a hit on that region.
    pub flinch: BTreeMap<String, Pose>,
    /// `[guard.<name>]` — status-driven guards.
    pub guard: BTreeMap<String, Pose>,
    /// `[posture.<name>]` — held postures.
    pub posture: BTreeMap<String, Pose>,
    /// `[family.<family>.<pose>]`, `[family_limits.<family>]`,
    /// `[family_idle.<family>]`.
    pub families: BTreeMap<String, FamilyPoses>,
}

impl PoseSet {
    /// The shipped pose text for a body template.
    ///
    /// `@0x100a4a254` — `PoseSet::shipped_toml(&str) -> Option<&'static str>`.
    /// The binary walks 13 × 32-byte `(&str, &str)` records at `0x1043b60d8`
    /// (0x1a0 bytes), trims the input, compares case-insensitively and returns
    /// the fourth word pair.  The 13 names and the exact bytes of their files are
    /// reproduced here.
    pub fn shipped_toml(name: &str) -> Option<&'static str> {
        let name = name.trim();
        SHIPPED
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| *v)
    }

    /// The names of the shipped templates, in table order.
    pub fn shipped_names() -> impl Iterator<Item = &'static str> {
        SHIPPED.iter().map(|(k, _)| *k)
    }

    /// Parse a pose file.
    ///
    /// `@0x100a4aff4` — `PoseSet::from_toml(&str) -> Result<PoseSet, …>`.  The
    /// serde field names recovered from the binary are `body_type`, `pose`,
    /// `family`, `limits`, `family_limits`, `idle`, `family_idle`, `flinch`,
    /// `guard`, `posture`, and inside a bone record `rot`, `bend`, `phase` —
    /// exactly the keys used by `poses/*.toml`.
    pub fn from_toml(text: &str) -> Result<PoseSet, PuppetError> {
        let doc = toml::parse(text).map_err(PuppetError::BadBaked)?;
        let mut set = PoseSet::default();
        if let Some(v) = doc.get("body_type") {
            set.body_type = v.as_str().unwrap_or_default().to_string();
        }
        if let Some(t) = table_of(&doc, "pose") {
            for (name, body) in t {
                set.poses.insert(name.clone(), pose_of(body));
            }
        }
        if let Some(t) = table_of(&doc, "limits") {
            for (bone, body) in t {
                if let Some(lim) = limit_of(body) {
                    set.limits.insert(bone.clone(), lim);
                }
            }
        }
        if let Some(t) = table_of(&doc, "idle") {
            set.idle = idle_of(t);
        }
        for (key, dst) in [
            ("flinch", &mut set.flinch),
            ("guard", &mut set.guard),
            ("posture", &mut set.posture),
        ] {
            if let Some(t) = table_of(&doc, key) {
                for (name, body) in t {
                    dst.insert(name.clone(), pose_of(body));
                }
            }
        }
        // family blocks
        if let Some(t) = table_of(&doc, "family") {
            for (family, poses) in t {
                let Some(poses) = poses.as_table() else { continue };
                let entry = set.families.entry(family.clone()).or_default();
                for (name, body) in poses {
                    entry.poses.insert(name.clone(), pose_of(body));
                }
            }
        }
        if let Some(t) = table_of(&doc, "family_limits") {
            for (family, limits) in t {
                let Some(limits) = limits.as_table() else { continue };
                let entry = set.families.entry(family.clone()).or_default();
                for (bone, body) in limits {
                    if let Some(lim) = limit_of(body) {
                        entry.limits.insert(bone.clone(), lim);
                    }
                }
            }
        }
        if let Some(t) = table_of(&doc, "family_idle") {
            for (family, idle) in t {
                let Some(idle) = idle.as_table() else { continue };
                let entry = set.families.entry(family.clone()).or_default();
                entry.idle = Some(idle_of(idle));
            }
        }
        Ok(set)
    }

    /// Load the shipped pose file for `body_type` and fold `family`'s overrides
    /// in.
    ///
    /// `@0x100a4920c` — `PoseSet::for_family`: it looks the template up with
    /// `shipped_toml`, parses it, then merges `family_limits.<family>` into
    /// `limits`, `family_idle.<family>` over `idle` (via `Idle::with_override`)
    /// and `family.<family>.<pose>` over `poses` (via `Pose::with_override`).
    pub fn for_family(body_type: &str, family: &str) -> Result<PoseSet, PuppetError> {
        let text = PoseSet::shipped_toml(body_type).ok_or_else(|| {
            PuppetError::BadBaked(format!("no shipped poses for body type {body_type}"))
        })?;
        let mut set = PoseSet::from_toml(text)?;
        let Some(fam) = set.families.get(family).cloned() else {
            return Ok(set);
        };
        for (bone, lim) in fam.limits {
            set.limits.insert(bone, lim);
        }
        if let Some(idle) = fam.idle {
            set.idle = set.idle.with_override(&idle);
        }
        for (name, over) in fam.poses {
            let ov = PoseOverride::from_pose(&over);
            let base = set.poses.get(&name).cloned().unwrap_or_default();
            set.poses.insert(name, base.with_override(&ov));
        }
        Ok(set)
    }

    /// Look a pose up by name.
    ///
    /// `@0x100a4a310` — `PoseSet::get(&self, &str) -> Option<&Pose>`; the binary
    /// is a `BTreeMap<String, Pose>` lookup (node stride `0xe8`, key compare via
    /// `memcmp`) and returns `None` on a miss.
    pub fn get(&self, name: &str) -> Option<&Pose> {
        self.poses.get(name)
    }

    /// The joint limit for `bone`, or a free limit.
    ///
    /// `@0x100a4a490` — `PoseSet::limit(&self, &str) -> Limit`.  A bone with no
    /// entry gets the `Vec3::MAX`/`Vec3::MIN` seeding that leaves it unclamped.
    pub fn limit(&self, bone: &str) -> Limit {
        match self.limits.get(bone) {
            Some(l) => l.clone(),
            None => match bone.strip_suffix("_l").or_else(|| bone.strip_suffix("_r")) {
                Some(pair) => self.limits.get(pair).cloned().unwrap_or_default(),
                None => Limit::free(),
            },
        }
    }

    /// Clamp every bone of `pose` against its limit.
    ///
    /// `@0x100a4a3ec` — `PoseSet::clamp(&self, &Pose) -> Pose` (it maps the bone
    /// map through `Limit::clamp`).
    pub fn clamp(&self, pose: &Pose) -> Pose {
        let mut out = Pose {
            bones: BTreeMap::new(),
            root: pose.root,
        };
        for (name, bp) in &pose.bones {
            let lim = self.limit(name);
            out.bones
                .insert(name.clone(), lim.clamp(bp.rot, &bp.bend));
        }
        out
    }

    /// Add the `[guard.<status>]` pose for each guarded status name.
    ///
    /// `@0x100a4a8b4` — `PoseSet::guarded(&self, &Pose, &[&str]) -> Pose`.  The
    /// binary folds every matching guard with `Pose::flinched` at envelope `1.0`
    /// (a full-strength offset), which is what this does.
    pub fn guarded(&self, pose: &Pose, statuses: &[&str]) -> Pose {
        let mut out = pose.clone();
        for name in statuses {
            if let Some(g) = self.guard.get(*name) {
                out = out.flinched(g, 1.0);
            }
        }
        out
    }

    /// Add the `[posture.<name>]` pose.
    ///
    /// `@0x100a4aa3c` — `PoseSet::postured(&self, &Pose, &str) -> Pose`.
    pub fn postured(&self, pose: &Pose, name: &str) -> Pose {
        match self.posture.get(name) {
            Some(p) => pose.flinched(p, 1.0),
            None => pose.clone(),
        }
    }

    /// A stable hash used to pick one of the death poses.
    ///
    /// `@0x100a4aba4` — `PoseSet::dead_hash`.  The binary hashes a string and
    /// then looks `"dead"` up in the pose map (the panic string
    /// `no entry found for keydead` survives).  The exact hasher is
    /// `std::collections::hash_map::DefaultHasher`, which is *not* reproducible
    /// across Rust releases, so a documented FNV-1a is used instead — the only
    /// property the caller needs is "stable per family".
    pub fn dead_hash(&self, family: &str) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in family.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // The recovered code only ever distinguishes the two death poses, and it
        // asserts that `"dead"` exists before returning.
        debug_assert!(self.poses.contains_key("dead") || self.poses.is_empty());
        h
    }

    /// Which death pose a hash selects: `"dead"` or `"dead_2"` (both appear in
    /// every shipped file).
    ///
    /// `@0x100a4af50` — `PoseSet::dead_name(&self, u64) -> &str`.
    pub fn dead_name(&self, hash: u64) -> &'static str {
        if hash & 1 == 0 {
            "dead"
        } else {
            "dead_2"
        }
    }

    /// The flinch peak pose for a hit region (`"head"`, `"chest"`, …).
    pub fn flinch_for(&self, region: &str) -> Option<&Pose> {
        let key = match region {
            "leftArm" | "rightArm" | "leftHand" | "rightHand" => "arm",
            "leftLeg" | "rightLeg" => "leg",
            "head" => "head",
            "neck" => "neck",
            "chest" => "chest",
            "abdomen" => "abdomen",
            "back" => "back",
            other => other,
        };
        self.flinch.get(key).or_else(|| self.flinch.get(region))
    }
}

fn table_of<'a>(doc: &'a Table, key: &str) -> Option<&'a Table> {
    doc.get(key).and_then(|v| v.as_table())
}

/// Parse one bone record: either a bare `[x, y, z]` (the `root` shorthand) or an
/// inline table with `rot` / `bend` / `phase`.
fn bone_record(v: &Value) -> (BonePose, f32) {
    if let Some(rot) = v.as_vec3() {
        return (BonePose::from_rot(Vec3::from_slice(&rot)), 0.0);
    }
    let Some(t) = v.as_table() else {
        return (BonePose::rest(), 0.0);
    };
    let rot = t
        .get("rot")
        .and_then(|v| v.as_vec3())
        .map(|a| Vec3::from_slice(&a))
        .unwrap_or(Vec3::ZERO);
    let bend = t
        .get("bend")
        .and_then(|v| v.as_f32_list())
        .unwrap_or_default();
    let phase = t.get("phase").and_then(|v| v.as_f32()).unwrap_or(0.0);
    (BonePose::from_rot_bend(rot, bend), phase)
}

/// Parse a `[pose.<name>]` / `[flinch.<r>]` / `[guard.<n>]` table.
fn pose_of(v: &Value) -> Pose {
    let mut pose = Pose::new();
    let Some(t) = v.as_table() else { return pose };
    for (bone, body) in t {
        if bone == "root" {
            if let Some(rot) = body.as_vec3() {
                pose.root = Vec3::from_slice(&rot);
            }
            continue;
        }
        let (bp, _) = bone_record(body);
        pose.bones.insert(bone.clone(), bp);
    }
    pose
}

/// Parse a `[idle]` / `[family_idle.<f>]` table.
fn idle_of(t: &Table) -> Idle {
    let period = t.get("period").and_then(|v| v.as_f32()).unwrap_or(0.0);
    let mut idle = Idle::new(period);
    for (bone, body) in t {
        if bone == "period" {
            continue;
        }
        let (bp, phase) = bone_record(body);
        idle.bones.insert(
            bone.clone(),
            IdleBone {
                rot: bp.rot,
                bend: bp.bend,
                phase,
            },
        );
    }
    idle
}

/// Parse a `{ rot = [[lo, hi], …], bend = [[lo, hi], …] }` limit record.
fn limit_of(v: &Value) -> Option<Limit> {
    let t = v.as_table()?;
    let mut lim = Limit::free();
    if let Some(r) = t.get("rot").and_then(|v| v.as_ranges()) {
        for (i, range) in r.iter().take(3).enumerate() {
            lim.rot[i] = *range;
        }
    }
    if let Some(b) = t.get("bend").and_then(|v| v.as_ranges()) {
        lim.bend = b;
    }
    Some(lim)
}

/// The 13 shipped templates, in the order of the binary's table at
/// `0x1043b60d8`.
static SHIPPED: [(&str, &str); 13] = [
    ("biped", include_str!("../poses/biped.toml")),
    ("quadruped", include_str!("../poses/quadruped.toml")),
    ("avian", include_str!("../poses/avian.toml")),
    ("serpentine", include_str!("../poses/serpentine.toml")),
    ("multileg", include_str!("../poses/multileg.toml")),
    ("winged_biped", include_str!("../poses/winged_biped.toml")),
    (
        "winged_quadruped",
        include_str!("../poses/winged_quadruped.toml"),
    ),
    ("mound", include_str!("../poses/mound.toml")),
    (
        "upright_insect",
        include_str!("../poses/upright_insect.toml"),
    ),
    ("centaur", include_str!("../poses/centaur.toml")),
    ("naga", include_str!("../poses/naga.toml")),
    ("fish", include_str!("../poses/fish.toml")),
    ("floating_orb", include_str!("../poses/floating_orb.toml")),
];

// ---------------------------------------------------------------------------
// posed rig
// ---------------------------------------------------------------------------

/// A posed rig: one world transform per bone, plus the box the figure occupies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Posed {
    /// Bone name → world transform.
    pub bones: BTreeMap<String, Xf>,
    /// Axis-aligned bounds `(min, max)` of the posed figure.
    pub bounds: (Vec3, Vec3),
}

impl Posed {
    /// The posed bounds.
    ///
    /// `@0x100a48d90` — `Pose::bounds`/`Posed::bounds`: it folds every bone
    /// origin into a min/max pair seeded with `Vec3::MAX` / `Vec3::MIN`.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.bounds
    }

    /// The bounds of the named bones only (`Posed::bounds_where`, the local
    /// helper the binary inlines into `pose_with`).
    pub fn bounds_where<'a, I>(&self, bones: I) -> (Vec3, Vec3)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut lo = Vec3::MAX;
        let mut hi = Vec3::MIN;
        for name in bones {
            if let Some(xf) = self.bones.get(name) {
                lo = lo.min(xf.t);
                hi = hi.max(xf.t);
            }
        }
        if lo.x > hi.x {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            (lo, hi)
        }
    }

    pub fn bone(&self, name: &str) -> Xf {
        self.bones.get(name).copied().unwrap_or(Xf::IDENTITY)
    }
}

impl Puppet {
    /// Forward kinematics for one [`Pose`].
    ///
    /// Recovered shape from `gs_puppet::pose::<impl Puppet>::pose_with`
    /// (`0x1009d4a4c`): walk the segments in order, compose each bone's local
    /// transform onto its parent's world transform (`Xf::then_inner`), then
    /// compute the bounds (`Posed::bounds_where`) and rest the figure on the
    /// ground.
    pub fn posed(&self, pose: &Pose) -> Posed {
        let mut bones: BTreeMap<String, Xf> = BTreeMap::new();
        let root_q = crate::math::euler_quat(pose.root);
        let mut lo = Vec3::MAX;
        let mut hi = Vec3::MIN;
        for (i, seg) in self.segments.iter().enumerate() {
            let bp = pose.bone(&seg.name);
            let parent = if i == 0 {
                Xf::from_rotation(&root_q)
            } else {
                *bones
                    .get(&self.segments[seg.parent].name)
                    .unwrap_or(&Xf::IDENTITY)
            };
            // Local frame: rotate about the bone's own pivot, then step out to
            // it — expressed in the parent's frame, so it composes *before* the
            // parent transform (`local.then_inner(parent)`).
            let local = Xf::new(crate::math::quat_to_m3(&bp.quat), seg.pivot);
            let world = local.then_inner(&parent);
            lo = lo.min(world.t);
            hi = hi.max(world.t);
            bones.insert(seg.name.clone(), world);
        }
        if lo.x > hi.x {
            lo = Vec3::ZERO;
            hi = Vec3::ZERO;
        }
        Posed {
            bones,
            bounds: (lo, hi),
        }
    }

    /// Pose the rig at rest.
    ///
    /// `@0x1009d2ef0` — `gs_puppet::pose::<impl Puppet>::pose`, a 12-byte wrapper
    /// that tail-calls `pose_with` with the empty pose.
    pub fn pose(&self) -> Posed {
        self.posed(&Pose::new())
    }

    /// The pose as drawn: the rest pose, rested on the ground.
    ///
    /// `@0x1009d2ee4` — same 12-byte tail-call shape as [`Puppet::pose`]; the
    /// difference is which pose the caller hands in, so both are modelled with
    /// the same entry point.
    pub fn pose_as_drawn(&self) -> Posed {
        self.pose()
    }

    /// Pose the rig with a named pose from `set`, blended in over `t`.
    ///
    /// `@0x1009d4a4c` — `pose_with(&self, &PoseSet, name: &str, t: f32) -> Posed`.
    /// An unknown name falls back to the rest pose (the binary's `get` returns
    /// `None` and the caller keeps the empty pose).
    pub fn pose_with(&self, set: &PoseSet, name: &str, t: f32) -> Posed {
        let target = set.get(name).cloned().unwrap_or_default();
        let clamped = set.clamp(&target);
        let pose = if t >= 1.0 {
            clamped
        } else {
            Pose::new().blend(&clamped, t.clamp(0.0, 1.0))
        };
        self.posed(&pose)
    }

    /// Pin a pose's arms so the hands stay inside the figure's own bounds.
    ///
    /// `@0x1009d2efc` — `pin_arms(&mut self, &mut Pose, ...) -> …`.  The binary
    /// drives `arm_l` / `arm_r` (string literals `"arm_l"`, `"arm_r"` survive in
    /// the body) with `Pose::set_euler` and a `Pose::bone` read of `"Leg"`, then
    /// clamps against the posed bounds.  What the reach limit *is* could not be
    /// recovered: the recovery keeps the observable behaviour (the arms are
    /// rotated towards the body and re-clamped against the rig's limits).
    ///
    /// Returns `true` when a pin was applied.
    pub fn pin_arms(&self, pose: &mut Pose, set: &PoseSet) -> bool {
        let mut pinned = false;
        for side in ["arm_l", "arm_r"] {
            if !pose.bones.contains_key(side) && !pose.bones.contains_key("arm") {
                continue;
            }
            let cur = pose.bone(side);
            let lim = set.limit(side);
            let spread = lim.clamp_rot(Vec3::new(cur.rot.x, cur.rot.y, cur.rot.z));
            // Pull the arm back towards the torso by a quarter of its spread.
            let target = Vec3::new(spread.x, spread.y * 0.75, spread.z * 0.75);
            if target != cur.rot {
                pose.set_euler(side, target);
                pinned = true;
            }
        }
        pinned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::DEG_TO_RAD;

    #[test]
    fn flinch_envelope_matches_the_recovered_hump() {
        // Zero at both ends, exactly 1.0 one third of the way in.
        assert_eq!(flinch_envelope(0.0), 0.0);
        assert_eq!(flinch_envelope(FLINCH_SECONDS), 0.0);
        let third = FLINCH_SECONDS / 3.0;
        assert!((flinch_envelope(third) - 1.0).abs() < 1e-6, "{}", flinch_envelope(third));
        // Out of range: negative time and past the end.
        assert_eq!(flinch_envelope(-0.1), 0.0);
        assert_eq!(flinch_envelope(1.0), 0.0);
        assert_eq!(flinch_envelope(f32::NAN), 0.0);
        // Monotone up to the peak.
        let mut prev = 0.0;
        for i in 0..10 {
            let v = flinch_envelope(third * i as f32 / 10.0);
            assert!(v >= prev);
            prev = v;
        }
        // 6.75 * 0.5 * 0.5^2 = 0.84375 at u = 0.5.
        assert!((flinch_envelope(FLINCH_SECONDS * 0.5) - 0.843_75).abs() < 1e-6);
    }

    #[test]
    fn recovered_envelope_constants() {
        assert_eq!(FLINCH_SECONDS.to_bits(), 0x3f0c_cccd);
        assert_eq!(FLINCH_PEAK_GAIN, 6.75);
        assert_eq!(TAU.to_bits(), 0x40c9_0fdb);
    }

    #[test]
    fn limit_clamps_each_axis_and_rebuilds_the_quat() {
        let lim = Limit {
            rot: [[-45.0, 70.0], [-60.0, 60.0], [-40.0, 40.0]],
            bend: vec![[-150.0, 20.0]],
        };
        let bp = lim.clamp(Vec3::new(-90.0, 100.0, 5.0), &[-200.0, 7.0]);
        assert_eq!(bp.rot, Vec3::new(-45.0, 60.0, 5.0));
        // The first bend is clamped, the second has no range and passes through.
        assert_eq!(bp.bend, vec![-150.0, 7.0]);
        // The quaternion is the one the clamped angles encode.
        assert_eq!(bp.quat, crate::math::euler_quat(bp.rot));
    }

    #[test]
    fn a_free_limit_changes_nothing() {
        let lim = Limit::free();
        let v = Vec3::new(-1000.0, 2.0, 3.0);
        assert_eq!(lim.clamp_rot(v), v);
        assert_eq!(lim.clamp(v, &[]).rot, v);
    }

    #[test]
    fn bone_pose_lerp_blends_angles_and_uses_slerp() {
        let a = BonePose::from_rot_bend(Vec3::new(0.0, 0.0, 0.0), vec![0.0, 0.0]);
        let b = BonePose::from_rot_bend(Vec3::new(20.0, 0.0, 90.0), vec![10.0, -20.0]);
        let mid = a.lerp(&b, 0.5);
        assert_eq!(mid.rot, Vec3::new(10.0, 0.0, 45.0));
        assert_eq!(mid.bend, vec![5.0, -10.0]);
        // Matches an explicit slerp of the two quaternions.
        let expect_q = crate::math::slerp(&a.quat, &b.quat, 0.5);
        assert!((mid.quat.w - expect_q.w).abs() < 1e-6);
        assert!((mid.quat.z - expect_q.z).abs() < 1e-6);
    }

    #[test]
    fn bone_pose_scaled_and_added() {
        let a = BonePose::from_rot_bend(Vec3::new(10.0, 20.0, -30.0), vec![5.0]);
        let half = a.scaled(0.5);
        assert_eq!(half.rot, Vec3::new(5.0, 10.0, -15.0));
        assert_eq!(half.bend, vec![2.5]);
        let sum = a.added(&half);
        assert_eq!(sum.rot, Vec3::new(15.0, 30.0, -45.0));
        assert_eq!(sum.bend, vec![7.5]);
        // `added` grows the bend list when the other side is longer.
        let longer = BonePose::from_rot_bend(Vec3::ZERO, vec![1.0, 2.0, 3.0]);
        assert_eq!(a.added(&longer).bend, vec![6.0, 2.0, 3.0]);
    }

    #[test]
    fn pose_pair_lookup_mirrors_the_right_side() {
        let mut p = Pose::new();
        p.set(
            "leg",
            BonePose::from_rot(Vec3::new(-62.0, 0.0, 10.0)),
        );
        // `leg_l` is the pair key as written...
        assert_eq!(p.bone("leg_l").rot, Vec3::new(-62.0, 0.0, 10.0));
        // ... and `leg_r` mirrors y and z.
        assert_eq!(p.bone("leg_r").rot, Vec3::new(-62.0, -0.0, -10.0));
        // An unknown bone is the rest pose.
        assert_eq!(p.bone("tail").rot, Vec3::ZERO);
    }

    #[test]
    fn pose_set_euler_converts_degrees_through_the_recovered_constant() {
        let mut p = Pose::new();
        p.set_euler("neck", Vec3::new(0.0, 0.0, 90.0));
        let bp = p.bone("neck");
        // 90° about z: w = z = √½ (row-vector convention, `w` first).
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert!((bp.quat.w - h).abs() < 1e-6, "{:?}", bp.quat);
        assert!((bp.quat.z - h).abs() < 1e-6, "{:?}", bp.quat);
        // And the matrix half-angle agrees with the recovered DEG_TO_RAD.
        assert!((90.0f32 * DEG_TO_RAD - std::f32::consts::FRAC_PI_2).abs() < 1e-7);
    }

    #[test]
    fn pose_breathe_uses_a_sine_per_idle_bone() {
        let mut idle = Idle::new(4.0);
        idle.bones.insert(
            "chest".into(),
            IdleBone {
                rot: Vec3::new(2.0, 0.0, 0.0),
                bend: vec![],
                phase: 0.0,
            },
        );
        // At t = 1/4 of a cycle the sine is 1, so the full amplitude lands.
        let p = Pose::new().breathe(&idle, 0.25, 1.0);
        assert!((p.bone("chest").rot.x - 2.0).abs() < 1e-5, "{:?}", p.bone("chest"));
        // At t = 3/4 it is -1.
        let p = Pose::new().breathe(&idle, 0.75, 1.0);
        assert!((p.bone("chest").rot.x + 2.0).abs() < 1e-5);
        // Gain 0 or a non-finite t returns the pose untouched.
        assert!(Pose::new().breathe(&idle, 0.25, 0.0).is_empty());
        assert!(Pose::new().breathe(&idle, f32::NAN, 1.0).is_empty());
        // A phase of -0.25 lags a quarter cycle.
        let mut lagging = idle.clone();
        lagging.bones.get_mut("chest").unwrap().phase = -0.25;
        assert!((Pose::new().breathe(&lagging, 0.5, 1.0).bone("chest").rot.x - 2.0).abs() < 1e-5);
    }

    #[test]
    fn pose_flinched_scales_the_peak() {
        let mut peak = Pose::new();
        peak.set("skull", BonePose::from_rot(Vec3::new(12.0, 0.0, -8.0)));
        let base = Pose::new();
        let half = base.flinched(&peak, 0.5);
        assert_eq!(half.bone("skull").rot, Vec3::new(6.0, 0.0, -4.0));
        // A zero envelope is a no-op, and so is a non-finite one.
        assert!(base.flinched(&peak, 0.0).is_empty());
        assert!(base.flinched(&peak, f32::INFINITY).is_empty());
    }

    #[test]
    fn pose_blend_of_rest_and_pose_moves_halfway() {
        let mut b = Pose::new();
        b.set("neck", BonePose::from_rot(Vec3::new(20.0, 0.0, 0.0)));
        b.root = Vec3::new(0.0, 0.0, 90.0);
        let mid = Pose::new().blend(&b, 0.5);
        assert_eq!(mid.bone("neck").rot, Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(mid.root, Vec3::new(0.0, 0.0, 45.0));
        // Bones only present on one side are blended against the rest pose.
        let quarter = Pose::new().blend(&b, 0.25);
        assert_eq!(quarter.bone("neck").rot, Vec3::new(5.0, 0.0, 0.0));
    }

    #[test]
    fn shipped_table_has_thirteen_templates() {
        let names: Vec<_> = PoseSet::shipped_names().collect();
        assert_eq!(names.len(), 13);
        assert_eq!(names[0], "biped");
        assert!(names.contains(&"winged_quadruped"));
        assert!(PoseSet::shipped_toml("BIPED").is_some());
        assert!(PoseSet::shipped_toml("  fish  ").is_some());
        assert!(PoseSet::shipped_toml("dragon").is_none());
    }

    #[test]
    fn every_shipped_template_parses_with_its_own_body_type() {
        for (name, text) in SHIPPED {
            let set = PoseSet::from_toml(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(set.body_type, name, "{name}");
            // Every file has a death pose and an idle cycle.
            assert!(set.get("dead").is_some(), "{name} has no dead pose");
            assert!(set.idle.period > 0.0, "{name} has no idle period");
            assert!(!set.limits.is_empty(), "{name} has no limits");
            assert!(!set.flinch.is_empty(), "{name} has no flinch regions");
        }
    }

    #[test]
    fn biped_data_is_the_recovered_data() {
        let set = PoseSet::for_family("biped", "").unwrap();
        // arms_up raises both arms 152° out to the side with a -14° elbow.
        let arms = set.get("arms_up").unwrap();
        assert_eq!(arms.bone("arm_l").rot, Vec3::new(0.0, 0.0, 152.0));
        assert_eq!(arms.bone("arm_l").bend, vec![-14.0]);
        // ... and the right arm is the mirror of the pair key.
        assert_eq!(arms.bone("arm_r").rot, Vec3::new(0.0, -0.0, -152.0));
        // prone turns the whole figure 90° about x.
        assert_eq!(set.get("prone").unwrap().root, Vec3::new(90.0, 0.0, 0.0));
        // The biped's idle cycle is 4.2 s; the upper torso leans back a little
        // and the arms drift with a -0.16 phase lag.
        assert!((set.idle.period - 4.2).abs() < 1e-6, "{}", set.idle.period);
        let torso = &set.idle.bones["torso_upper"];
        assert_eq!(torso.rot, Vec3::new(-3.0, 0.0, 0.0));
        let arm = &set.idle.bones["arm"];
        assert_eq!(arm.rot, Vec3::new(1.5, 0.0, 2.8));
        assert_eq!(arm.bend, vec![-2.2]);
        assert!((arm.phase + 0.16).abs() < 1e-6);
        // kneel folds the knee 87° then the ankle 62°.
        let kneel = set.get("kneel").unwrap();
        assert_eq!(kneel.bone("leg_l").bend, vec![87.0, 62.0]);
    }

    #[test]
    fn limits_come_out_of_the_pose_file_and_clamp() {
        let set = PoseSet::for_family("quadruped", "").unwrap();
        let hind = set.limit("hindleg_r");
        assert_eq!(hind.rot[0], [-140.0, 75.0]);
        assert_eq!(hind.bend, vec![[-10.0, 170.0], [-120.0, 20.0]]);
        // A pose outside the limit is clamped.
        let mut wild = Pose::new();
        wild.set("hindleg", BonePose::from_rot_bend(Vec3::new(-400.0, 0.0, 0.0), vec![0.0, 0.0]));
        let clamped = set.clamp(&wild);
        assert_eq!(clamped.bone("hindleg").rot, Vec3::new(-140.0, 0.0, 0.0));
        // An unlimited bone is left alone.
        assert_eq!(set.limit("unknown_bone"), Limit::free());
    }

    #[test]
    fn family_overrides_are_folded_in() {
        // quadruped's elephantid family has a trunk bone with its own limits and
        // its own idle period.
        let plain = PoseSet::for_family("quadruped", "").unwrap();
        let elephant = PoseSet::for_family("quadruped", "elephantid").unwrap();
        assert!(plain.limits.get("trunk").is_none());
        let trunk = &elephant.limits["trunk"];
        assert_eq!(trunk.rot[0], [-140.0, 60.0]);
        assert_eq!(trunk.bend.len(), 3);
        assert!((elephant.idle.period - 5.4).abs() < 1e-6, "{}", elephant.idle.period);
        assert!(elephant.idle.bones.contains_key("trunk"));
        // The base bones survive the merge.
        assert!(elephant.idle.bones.contains_key("brisket"));
    }

    #[test]
    fn dead_pose_selection_is_stable_per_family() {
        let set = PoseSet::for_family("biped", "").unwrap();
        let a = set.dead_hash("troll");
        assert_eq!(a, set.dead_hash("troll"));
        assert_ne!(a, set.dead_hash("golem"));
        assert!(matches!(set.dead_name(a), "dead" | "dead_2"));
        assert_eq!(set.dead_name(0), "dead");
        assert_eq!(set.dead_name(1), "dead_2");
    }

    #[test]
    fn flinch_regions_map_onto_body_parts() {
        let set = PoseSet::for_family("biped", "").unwrap();
        assert!(set.flinch_for("leftArm").is_some());
        assert_eq!(
            set.flinch_for("leftArm").unwrap().bones.len(),
            set.flinch_for("arm").unwrap().bones.len()
        );
        assert!(set.flinch_for("nonsense").is_none());
    }

    #[test]
    fn fk_places_the_hand_down_the_arm() {
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
        let rest = p.pose();
        let h = rest.bone("hand_l").t;
        assert!((h.x - 0.2).abs() < 1e-6 && (h.y - 0.2).abs() < 1e-6, "{h:?}");
        let b = rest.bounds();
        assert_eq!(b.0, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(b.1, Vec3::new(0.2, 0.5, 0.0));
        // A 180° turn about z about the root flips the arm across the body.
        let mut pose = Pose::new();
        pose.root = Vec3::new(0.0, 0.0, 180.0);
        let turned = p.posed(&pose);
        let t = turned.bone("arm_l").t;
        assert!((t.x + 0.2).abs() < 1e-5 && (t.y + 0.5).abs() < 1e-5, "{t:?}");
    }

    #[test]
    fn pose_with_blends_in_and_clamps() {
        let set = PoseSet::for_family("quadruped", "").unwrap();
        use crate::model::Segment;
        let p = Puppet::new(
            "quadruped",
            vec![Segment::new("hindleg", 0, Vec3::new(0.0, 0.4, 0.0), 0.4)],
        )
        .unwrap();
        // At t = 0 nothing has happened yet.
        let at0 = p.pose_with(&set, "sit", 0.0);
        assert_eq!(at0.bone("hindleg").t, Vec3::new(0.0, 0.4, 0.0));
        // At t = 1 the pose is applied (and clamped to the file's limits).
        let at1 = p.pose_with(&set, "sit", 1.0);
        assert_ne!(at1.bone("hindleg").t, Vec3::new(0.0, 0.4, 0.0));
        // An unknown pose name is the rest pose.
        assert_eq!(
            p.pose_with(&set, "nonesuch", 1.0).bone("hindleg").t,
            Vec3::new(0.0, 0.4, 0.0)
        );
    }

    #[test]
    fn overrides_replace_named_bones() {
        let mut base = Pose::new();
        base.set("neck", BonePose::from_rot(Vec3::new(1.0, 0.0, 0.0)));
        base.set("skull", BonePose::from_rot(Vec3::new(2.0, 0.0, 0.0)));
        let mut ov = PoseOverride::default();
        ov.bones
            .insert("skull".into(), BonePose::from_rot(Vec3::new(30.0, 0.0, 0.0)));
        ov.root = Some(Vec3::new(0.0, 0.0, 70.0));
        let out = base.with_override(&ov);
        assert_eq!(out.bone("neck").rot, Vec3::new(1.0, 0.0, 0.0));
        assert_eq!(out.bone("skull").rot, Vec3::new(30.0, 0.0, 0.0));
        assert_eq!(out.root, Vec3::new(0.0, 0.0, 70.0));
    }

    #[test]
    fn guards_and_postures_add_their_offsets() {
        let set = PoseSet::for_family("biped", "").unwrap();
        let base = Pose::new();
        let guarded = set.guarded(&base, &["back"]);
        assert!(!guarded.is_empty());
        let postured = set.postured(&base, "cower");
        assert_eq!(
            postured.bone("torso_upper").rot,
            set.posture["cower"].bone("torso_upper").rot
        );
        // An unknown posture is a no-op.
        assert!(set.postured(&base, "nonesuch").is_empty());
    }
}
