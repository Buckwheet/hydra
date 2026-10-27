//! The body/health "paper doll" the page draws.
//!
//! Recovered from `injuries::doll` (`0x100296be4`, 176 bytes) and the serde
//! glue of `DollOut` (`0x1002964e0`), `PartOut` (`0x100296628`), `ShapeOut`
//! (`0x100296744`, internally tagged on `kind`) and `InjuryOut` (`0x100296aac`).

use crate::json::Value;
use crate::wire::object;

/// A geometric primitive in the doll drawing.
///
/// **ESTABLISHED** variant names and field order (`tag = "kind"`):
/// `circle { c, r }`, `line { a, b, w }`, `block { min, max }`,
/// `letter { c, letter }`.
///
/// `// UNKNOWN:` the numeric field types.  The original stored them 4, 8, 12 and
/// 16 bytes apart, so they are modelled as `u32` except `letter`, which needs a
/// string.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShapeOut {
    Circle { c: u32, r: u32 },
    Line { a: u32, b: u32, w: u32 },
    Block { min: u32, max: u32 },
    Letter { c: u32, letter: String },
}

impl ShapeOut {
    /// The `"kind"` tag.
    pub fn kind(&self) -> &'static str {
        match self {
            ShapeOut::Circle { .. } => "circle",
            ShapeOut::Line { .. } => "line",
            ShapeOut::Block { .. } => "block",
            ShapeOut::Letter { .. } => "letter",
        }
    }

    /// The JSON fields, `kind` first (the recovered order).
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        let mut out = vec![("kind", Value::str(self.kind()))];
        match self {
            ShapeOut::Circle { c, r } => {
                out.push(("c", Value::int(*c)));
                out.push(("r", Value::int(*r)));
            }
            ShapeOut::Line { a, b, w } => {
                out.push(("a", Value::int(*a)));
                out.push(("b", Value::int(*b)));
                out.push(("w", Value::int(*w)));
            }
            ShapeOut::Block { min, max } => {
                out.push(("min", Value::int(*min)));
                out.push(("max", Value::int(*max)));
            }
            ShapeOut::Letter { c, letter } => {
                out.push(("c", Value::int(*c)));
                out.push(("letter", Value::str(letter.clone())));
            }
        }
        out
    }
}

/// One body part: `shape, id, name, anchor` (recovered order).
#[derive(Clone, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PartOut {
    pub shape: ShapeOut,
    pub id: u32,
    pub name: String,
    pub anchor: String,
}

impl PartOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("shape", object(self.shape.fields())),
            ("id", Value::int(self.id)),
            ("name", Value::str(self.name.clone())),
            ("anchor", Value::str(self.anchor.clone())),
        ]
    }
}

/// The doll: `parts, aspect`.
///
/// `aspect` is the recovered `f32` `0.75` stored at `+0x18` of `DollOut`.
#[derive(Clone, Debug, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DollOut {
    pub parts: Vec<PartOut>,
    pub aspect: f32,
}

impl DollOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            (
                "parts",
                Value::Arr(self.parts.iter().map(|p| object(p.fields())).collect()),
            ),
            ("aspect", Value::float(self.aspect as f64)),
        ]
    }
}

/// One injury row: `id, name, token, rank`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct InjuryOut {
    pub id: String,
    pub name: String,
    pub token: String,
    pub rank: u32,
}

impl InjuryOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::str(self.id.clone())),
            ("name", Value::str(self.name.clone())),
            ("token", Value::str(self.token.clone())),
            ("rank", Value::int(self.rank)),
        ]
    }
}

/// The aspect ratio of the doll drawing (`0x3f400000` = `0.75`).
pub const ASPECT: f32 = 0.75;

/// Number of parts in the static doll table (`0x3f0` bytes / 72-byte stride).
pub const PARTS: usize = 14;

/// The body parts, in the recovered order, with the recovered names.
///
/// `// UNKNOWN:` the static table (`0x104382b70`, 14 × 72 bytes) holds each
/// part's geometry and anchor; only `PARTS`, `ASPECT` and the name pool
/// (`0x102071980`: `head, leftEye/left eye, rightEye/right eye, neck, chest,
/// abdomen, back, leftArm/left arm, rightArm/right arm, leftHand/left hand,
/// rightHand/right hand, leftLeg/left leg, leftFoot, rightLeg/right leg,
/// rightFoot, nervous system`) are readable.  The shapes below are the safest
/// reconstruction: paired limbs as lines, the torso as blocks, the head as a
/// circle.
pub fn table() -> Vec<PartOut> {
    fn part(id: u32, name: &str, anchor: &str, shape: ShapeOut) -> PartOut {
        PartOut {
            shape,
            id,
            name: name.to_string(),
            anchor: anchor.to_string(),
        }
    }
    let line = |a: u32, b: u32| ShapeOut::Line { a, b, w: 2 };
    vec![
        part(0, "head", "head", ShapeOut::Circle { c: 50, r: 12 }),
        part(1, "neck", "neck", line(50, 62)),
        part(2, "chest", "chest", ShapeOut::Block { min: 62, max: 76 }),
        part(3, "abdomen", "abdomen", ShapeOut::Block { min: 76, max: 88 }),
        part(4, "back", "back", ShapeOut::Block { min: 62, max: 88 }),
        part(5, "leftArm", "left arm", line(88, 100)),
        part(6, "rightArm", "right arm", line(88, 100)),
        part(7, "leftHand", "left hand", ShapeOut::Circle { c: 100, r: 5 }),
        part(8, "rightHand", "right hand", ShapeOut::Circle { c: 100, r: 5 }),
        part(9, "leftLeg", "left leg", line(88, 100)),
        part(10, "rightLeg", "right leg", line(88, 100)),
        part(11, "leftFoot", "leftFoot", line(100, 106)),
        part(12, "rightFoot", "right foot", line(100, 106)),
        part(13, "nervous system", "nervous system", line(50, 88)),
    ]
}

/// The doll (`0x100296be4`).
///
/// Recovered: no arguments; a 14-element static table is mapped into the
/// `Vec<PartOut>` (`Map::fold` over `0x104382b70`) and `aspect` is `0.75`.
pub fn doll() -> DollOut {
    DollOut {
        parts: table(),
        aspect: ASPECT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doll_has_fourteen_parts_and_the_recovered_aspect() {
        let d = doll();
        assert_eq!(d.parts.len(), PARTS);
        assert_eq!(d.aspect, 0.75);
        let json = object(d.fields()).to_json();
        assert!(json.ends_with(r#""aspect":0.75}"#), "{json}");
        assert!(json.contains(r#""name":"head""#));
        assert!(json.contains(r#""anchor":"left arm""#));
    }

    #[test]
    fn shapes_are_tagged_on_kind() {
        let s = ShapeOut::Circle { c: 50, r: 12 };
        assert_eq!(
            object(s.fields()).to_json(),
            r#"{"kind":"circle","c":50,"r":12}"#
        );
        let s = ShapeOut::Letter {
            c: 3,
            letter: "H".to_string(),
        };
        assert_eq!(
            object(s.fields()).to_json(),
            r#"{"kind":"letter","c":3,"letter":"H"}"#
        );
        let s = ShapeOut::Block { min: 62, max: 76 };
        assert_eq!(
            object(s.fields()).to_json(),
            r#"{"kind":"block","min":62,"max":76}"#
        );
    }

    #[test]
    fn injury_row_order() {
        let i = InjuryOut {
            id: "leftArm".into(),
            name: "left arm".into(),
            token: "wound".into(),
            rank: 2,
        };
        assert_eq!(
            object(i.fields()).to_json(),
            r#"{"id":"leftArm","name":"left arm","token":"wound","rank":2}"#
        );
    }
}
