//! Baked puppets: a pre-computed rig serialised to TOML.
//!
//! Recovered schema (`gs_puppet::baked`, from the serde static pool at
//! `0x1021dc7d0`):
//!
//! * `struct BakedPuppet with 11 elements` — the field names that survive are
//!   `version`, `height`, `groups`, `made_splats`, `arms_hang`, `body_type` and
//!   `family`.
//! * `struct BakedSegment with 14 elements` — `segment`, `pos`, `normal`, `radius`,
//!   `group`, `axis_w`, `axis_d`, `hinge`.
//! * `struct BakedEye with 4 elements` — `BakedEye` is named, its four fields are
//!   modelled as `pos`, `normal`, `radius` and `open`.
//! * `BakedPuppet::from_toml` (`@0x1009b09cc`) is a thin serde entry point
//!   (`toml::from_str`) — only 476 bytes, because all the work is in the derived
//!   `Deserialize`.
//! * `Puppet::from_baked` (`@0x1009d5bac`) turns the baked record back into a rig
//!   (it calls `hang::hangs`, i.e. it re-derives which bones hang) and
//!   `Puppet::from_baked_toml` (`@0x1009d6658`, 220 bytes) is
//!   `BakedPuppet::from_toml` + `Puppet::from_baked`.
//!
//! The remaining fields of the 11/14-element records are **UNKNOWN**; they are
//! modelled with the minimum `Puppet::from_baked` needs, and the TOML reader
//! ignores keys it does not know.

use crate::math::Vec3;
use crate::model::{Puppet, PuppetError, Segment};
/// Re-export: the recovered `baked` functions are `impl`s on
/// [`crate::model::Puppet`].
#[allow(unused_imports)]
pub use crate::model::Puppet as BakedPuppetRig;
use crate::toml::{self, Value};

/// One baked bone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedSegment {
    /// Bone name (`segment` in the file).
    pub segment: String,
    /// Pivot / head position.
    pub pos: Vec3,
    /// Direction the bone lies along when it is baked.
    pub normal: Vec3,
    /// Bone radius.
    pub radius: f32,
    /// Group index (`group`) — bones in one group share a splat body.
    pub group: usize,
    /// Width-axis frame (`axis_w`).
    pub axis_w: Vec3,
    /// Depth-axis frame (`axis_d`).
    pub axis_d: Vec3,
    /// Hinge position (`hinge`).
    pub hinge: Vec3,
    /// Length of the bone; derived from `pos`/`hinge` when the file omits it.
    pub length: f32,
    /// Parent index; `None` means "infer from the file order".
    pub parent: Option<usize>,
}

/// One baked eye.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedEye {
    pub pos: Vec3,
    pub normal: Vec3,
    pub radius: f32,
    /// Whether the eye is open (the recovered 4th field).
    pub open: bool,
}

/// A whole baked puppet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedPuppet {
    /// Format version (`version`).
    pub version: u32,
    /// Overall height in model units.
    pub height: f32,
    /// Body template name (`body_type`), e.g. `"biped"`.
    pub body_type: String,
    /// Creature family (`family`).
    pub family: String,
    /// Group names, indexed by [`BakedSegment::group`].
    pub groups: Vec<String>,
    /// The bones.
    pub segments: Vec<BakedSegment>,
    /// The eyes.
    pub eyes: Vec<BakedEye>,
    /// `arms_hang`: the rest pose already has the arms hanging.
    pub arms_hang: bool,
    /// `made_splats`: the file also carries a splat cloud beside the rig.
    pub made_splats: bool,
}

impl BakedPuppet {
    /// Parse a baked puppet from TOML.
    ///
    /// `@0x1009b09cc` — `gs_puppet::baked::BakedPuppet::from_toml(&str) ->
    /// Result<BakedPuppet, _>`.  Segments may be given either as an array of tables
    /// (`[[segment]]`) or as a table keyed by bone name.
    pub fn from_toml(text: &str) -> Result<BakedPuppet, PuppetError> {
        let doc = toml::parse(text).map_err(PuppetError::BadBaked)?;
        let mut out = BakedPuppet {
            version: doc
                .get("version")
                .and_then(|v| v.as_usize())
                .unwrap_or(1) as u32,
            height: doc
                .get("height")
                .and_then(|v| v.as_f32())
                .unwrap_or(1.0),
            body_type: doc
                .get("body_type")
                .and_then(|v| v.as_str())
                .unwrap_or("biped")
                .to_string(),
            family: doc
                .get("family")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            arms_hang: doc
                .get("arms_hang")
                .map(|v| matches!(v, Value::Bool(true)))
                .unwrap_or(false),
            made_splats: doc
                .get("made_splats")
                .map(|v| matches!(v, Value::Bool(true)))
                .unwrap_or(false),
            ..Default::default()
        };
        if let Some(g) = doc.get("groups") {
            if let Some(arr) = g.as_arr() {
                out.groups = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
            }
        }
        match doc.get("segment").or_else(|| doc.get("segments")) {
            Some(Value::Arr(items)) => {
                for item in items {
                    out.segments.push(segment_of(item, out.segments.len())?);
                }
            }
            Some(Value::Table(t)) => {
                for (name, item) in t {
                    let named = item
                        .as_table()
                        .map(|t| t.contains_key("segment") || t.contains_key("name"))
                        .unwrap_or(false);
                    let mut seg = segment_of(item, out.segments.len())?;
                    if !named {
                        seg.segment = name.clone();
                    }
                    out.segments.push(seg);
                }
            }
            Some(_) => return Err(PuppetError::BadBaked("`segment` is not a list".into())),
            None => {}
        }
        if let Some(Value::Arr(items)) = doc.get("eye").or_else(|| doc.get("eyes")) {
            for item in items {
                out.eyes.push(eye_of(item)?);
            }
        }
        if out.segments.is_empty() {
            return Err(PuppetError::BadBaked("no segments".into()));
        }
        Ok(out)
    }
}

fn vec3_of(t: &crate::toml::Table, key: &str) -> Option<Vec3> {
    t.get(key).and_then(|v| v.as_vec3()).map(|a| Vec3::from_slice(&a))
}

fn segment_of(v: &Value, order: usize) -> Result<BakedSegment, PuppetError> {
    let Some(t) = v.as_table() else {
        return Err(PuppetError::BadBaked("segment is not a table".into()));
    };
    let seg = BakedSegment {
        segment: t
            .get("segment")
            .or_else(|| t.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        pos: vec3_of(t, "pos").unwrap_or(Vec3::ZERO),
        normal: vec3_of(t, "normal").unwrap_or(Vec3::new(0.0, 1.0, 0.0)),
        radius: t.get("radius").and_then(|v| v.as_f32()).unwrap_or(0.05),
        group: t.get("group").and_then(|v| v.as_usize()).unwrap_or(0),
        axis_w: vec3_of(t, "axis_w").unwrap_or(Vec3::new(1.0, 0.0, 0.0)),
        axis_d: vec3_of(t, "axis_d").unwrap_or(Vec3::new(0.0, 0.0, 1.0)),
        hinge: vec3_of(t, "hinge").unwrap_or(Vec3::ZERO),
        length: t.get("length").and_then(|v| v.as_f32()).unwrap_or(0.0),
        parent: t.get("parent").and_then(|v| v.as_usize()),
    };
    let mut seg = seg;
    if seg.parent.is_none() {
        // Parents come first, so the default parent is the previous bone.
        seg.parent = Some(order.saturating_sub(1));
    }
    if seg.length == 0.0 {
        seg.length = (seg.hinge - seg.pos).length();
    }
    if seg.segment.is_empty() {
        seg.segment = format!("bone{order}");
    }
    Ok(seg)
}

fn eye_of(v: &Value) -> Result<BakedEye, PuppetError> {
    let Some(t) = v.as_table() else {
        return Err(PuppetError::BadBaked("eye is not a table".into()));
    };
    Ok(BakedEye {
        pos: vec3_of(t, "pos").unwrap_or(Vec3::ZERO),
        normal: vec3_of(t, "normal").unwrap_or(Vec3::new(0.0, 0.0, 1.0)),
        radius: t.get("radius").and_then(|v| v.as_f32()).unwrap_or(0.05),
        open: t
            .get("open")
            .map(|v| matches!(v, Value::Bool(true)))
            .unwrap_or(true),
    })
}

impl Puppet {
    /// Rebuild a rig from a baked puppet.
    ///
    /// `@0x1009d5bac` — `gs_puppet::baked::<impl model::Puppet>::from_baked(&BakedPuppet)
    /// -> Puppet`.  It maps the baked bones onto [`Segment`]s, carries the body type
    /// and family over, sets the height, and then re-derives the hanging bones
    /// (`hang::hangs`).
    pub fn from_baked(baked: &BakedPuppet) -> Result<Puppet, PuppetError> {
        let mut segments = Vec::with_capacity(baked.segments.len());
        for (i, b) in baked.segments.iter().enumerate() {
            let parent = b.parent.unwrap_or(0).min(i);
            segments.push(
                Segment {
                    follower: baked.arms_hang && b.segment.starts_with("arm"),
                    parent,
                    name: b.segment.clone(),
                    pivot: b.pos,
                    length: if b.length > 0.0 { b.length } else { 0.1 },
                    radius: b.radius,
                },
            );
        }
        let mut puppet = Puppet::new(baked.body_type.clone(), segments)?;
        puppet.family = baked.family.clone();
        puppet.height = if baked.height > 0.0 { baked.height } else { 1.0 };
        // `from_baked` re-derives the hang set, which is what `Puppet::hangs` does on
        // demand here; touching it now keeps the mistake visible at load time.
        for i in 0..puppet.len() {
            let _ = puppet.hangs(i);
        }
        Ok(puppet)
    }

    /// Parse and rebuild in one step.
    ///
    /// `@0x1009d6658` — `gs_puppet::baked::<impl model::Puppet>::from_baked_toml(&str)
    /// -> Result<Puppet, _>`, a 220-byte wrapper: `BakedPuppet::from_toml` then
    /// `Puppet::from_baked`, dropping the intermediate.
    pub fn from_baked_toml(text: &str) -> Result<Puppet, PuppetError> {
        let baked = BakedPuppet::from_toml(text)?;
        Puppet::from_baked(&baked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAKED: &str = r#"
version = 2
height = 1.82
body_type = "biped"
family = "troll"
arms_hang = true
made_splats = true
groups = ["torso", "limb"]

[[segment]]
segment = "root"
pos = [0.0, 0.0, 0.0]
normal = [0.0, 1.0, 0.0]
radius = 0.12
group = 0
hinge = [0.0, 0.9, 0.0]

[[segment]]
segment = "arm_l"
pos = [0.2, 0.8, 0.0]
normal = [0.0, -1.0, 0.0]
radius = 0.06
group = 1
parent = 0
hinge = [0.2, 0.5, 0.0]

[[eye]]
pos = [0.05, 0.95, 0.1]
normal = [0.0, 0.0, 1.0]
radius = 0.02
open = false
"#;

    #[test]
    fn parses_the_recovered_schema() {
        let baked = BakedPuppet::from_toml(BAKED).unwrap();
        assert_eq!(baked.version, 2);
        assert!((baked.height - 1.82).abs() < 1e-6);
        assert_eq!(baked.body_type, "biped");
        assert_eq!(baked.family, "troll");
        assert!(baked.arms_hang);
        assert!(baked.made_splats);
        assert_eq!(baked.groups, vec!["torso", "limb"]);
        assert_eq!(baked.segments.len(), 2);
        assert_eq!(baked.segments[0].segment, "root");
        assert_eq!(baked.segments[0].radius, 0.12);
        assert_eq!(baked.segments[0].group, 0);
        // The length is derived from pos -> hinge when the file omits it.
        assert!((baked.segments[0].length - 0.9).abs() < 1e-6);
        assert_eq!(baked.segments[1].parent, Some(0));
        assert_eq!(baked.eyes.len(), 1);
        assert!(!baked.eyes[0].open);
        assert_eq!(baked.eyes[0].radius, 0.02);
    }

    #[test]
    fn builds_a_rig_from_the_baked_record() {
        let baked = BakedPuppet::from_toml(BAKED).unwrap();
        let puppet = Puppet::from_baked(&baked).unwrap();
        assert_eq!(puppet.body_type, "biped");
        assert_eq!(puppet.family, "troll");
        assert!((puppet.height - 1.82).abs() < 1e-6);
        assert_eq!(puppet.len(), 2);
        assert_eq!(puppet.segments[1].name, "arm_l");
        // `arms_hang` marks the arm as a follower.
        assert!(puppet.segments[1].follower);
        assert!(!puppet.segments[0].follower);
    }

    #[test]
    fn from_baked_toml_is_the_two_step_wrapper() {
        let puppet = Puppet::from_baked_toml(BAKED).unwrap();
        assert_eq!(puppet.len(), 2);
        assert_eq!(puppet.segments[0].length, 0.9);
    }

    #[test]
    fn segments_may_be_a_named_table() {
        let text = r#"
body_type = "mound"
[segment.root]
pos = [0.0, 0.0, 0.0]
radius = 0.3
[segment.skull]
pos = [0.0, 0.4, 0.0]
radius = 0.1
"#;
        let baked = BakedPuppet::from_toml(text).unwrap();
        assert_eq!(baked.segments.len(), 2);
        assert_eq!(baked.segments[0].segment, "root");
        assert_eq!(baked.segments[1].segment, "skull");
        assert_eq!(baked.segments[0].radius, 0.3);
    }

    #[test]
    fn empty_or_invalid_files_are_rejected() {
        assert!(matches!(
            BakedPuppet::from_toml("version = 1\n"),
            Err(PuppetError::BadBaked(_))
        ));
        assert!(BakedPuppet::from_toml("not toml at all\n").is_err());
    }
}
