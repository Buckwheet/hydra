//! Body-part canonicalisation and labels.
//!
//! Recovered from `gs_base::body::part_label` @0x100a8b760 and
//! `gs_base::body::canonical_part` @0x100a8b7f4.
//!
//! Both functions lowercase ASCII in place (`c - 'A' < 26 ? c | 0x20 : c`, which
//! is exactly `eq_ignore_ascii_case`) and then scan a static table of
//! `(key, label)` pairs.

/// The 14 body parts, in the order the original table stores them.
///
/// The first field is the *key* the wire protocol uses; the second is the human
/// label.  Both tables in the binary (0x1043b9120 = 14 × 32-byte `(key, label)`
/// pairs, 0x1043b92e0 = 14 × 16-byte keys in a different order) are reproduced
/// below.
pub static BODY_PARTS: [(&str, &str); 14] = [
    ("head", "head"),
    ("leftEye", "left eye"),
    ("rightEye", "right eye"),
    ("neck", "neck"),
    ("chest", "chest"),
    ("abdomen", "abdomen"),
    ("back", "back"),
    ("leftArm", "left arm"),
    ("rightArm", "right arm"),
    ("leftHand", "left hand"),
    ("rightHand", "right hand"),
    ("leftLeg", "left leg"),
    ("rightLeg", "right leg"),
    ("nsys", "nervous system"),
];

/// Key order of the second table at 0x1043b92e0 (`canonical_part`'s lookup).
///
/// It is `BODY_PARTS` with the three eye entries moved to the end.
pub static CANONICAL_KEYS: [&str; 14] = [
    "head", "neck", "chest", "abdomen", "back", "leftArm", "rightArm", "leftHand", "rightHand",
    "leftLeg", "rightLeg", "leftEye", "rightEye", "nsys",
];

/// A body part.  The discriminants are the recovered ones: `part_label` returns
/// `index + 1` for the table index, i.e. `Head = 1 … Nsys = 14`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum BodyPart {
    Head = 1,
    LeftEye = 2,
    RightEye = 3,
    Neck = 4,
    Chest = 5,
    Abdomen = 6,
    Back = 7,
    LeftArm = 8,
    RightArm = 9,
    LeftHand = 10,
    RightHand = 11,
    LeftLeg = 12,
    RightLeg = 13,
    Nsys = 14,
}

impl BodyPart {
    /// Protocol key, e.g. `"leftEye"`.
    pub fn key(self) -> &'static str {
        BODY_PARTS[self as usize - 1].0
    }

    /// Human label, e.g. `"left eye"`.
    pub fn label(self) -> &'static str {
        BODY_PARTS[self as usize - 1].1
    }

    /// Inverse of [`BodyPart::key`]; case-insensitive.
    pub fn from_key(key: &str) -> Option<BodyPart> {
        part_label(key)
    }

    /// All parts in table order.
    pub fn all() -> impl Iterator<Item = BodyPart> {
        (1..=14u32).map(|i| BodyPart::from_index(i).unwrap())
    }

    fn from_index(i: u32) -> Option<BodyPart> {
        Some(match i {
            1 => BodyPart::Head,
            2 => BodyPart::LeftEye,
            3 => BodyPart::RightEye,
            4 => BodyPart::Neck,
            5 => BodyPart::Chest,
            6 => BodyPart::Abdomen,
            7 => BodyPart::Back,
            8 => BodyPart::LeftArm,
            9 => BodyPart::RightArm,
            10 => BodyPart::LeftHand,
            11 => BodyPart::RightHand,
            12 => BodyPart::LeftLeg,
            13 => BodyPart::RightLeg,
            14 => BodyPart::Nsys,
            _ => return None,
        })
    }
}

/// Look a body-part key up, ASCII-case-insensitively, and return the part.
///
/// `@0x100a8b760` — walks `BODY_PARTS` comparing the *key* (32-byte stride, so
/// field 0 of each `(key, label)` pair) and returns the 1-based index wrapped in
/// `Option` (the binary returns `{1, index + 1}` / `{0, _}`, which is
/// `Option<BodyPart>` for a `#[repr(u32)]` enum).
pub fn part_label(name: &str) -> Option<BodyPart> {
    BODY_PARTS
        .iter()
        .position(|(key, _)| key.eq_ignore_ascii_case(name))
        .and_then(|i| BodyPart::from_index(i as u32 + 1))
}

/// Map an arbitrary spelling onto a canonical body-part key.
///
/// `@0x100a8b7f4` — ASCII-lowercases the input into a scratch `Vec<u8>` (the
/// SIMD `add/cmhi/orr` sequence is `to_ascii_lowercase`), then:
///
/// * `"nerves"`   → `"nsys"`
/// * `"leftfoot"` → `"leftLeg"`
/// * `"rightfoot"`→ `"rightLeg"`
/// * otherwise the first entry of `CANONICAL_KEYS` that matches
///   case-insensitively (returned verbatim, i.e. with its canonical spelling).
/// * otherwise the empty string.
///
/// UNKNOWN: the no-match branch of the binary returns a null pointer with a
/// stale length (it never writes a real empty slice), which almost certainly
/// came from `unwrap_or_default()`.  Returning `""` is the equivalent safe
/// behaviour.
pub fn canonical_part(name: &str) -> &'static str {
    if name.eq_ignore_ascii_case("nerves") {
        return "nsys";
    }
    if name.eq_ignore_ascii_case("leftfoot") {
        return "leftLeg";
    }
    if name.eq_ignore_ascii_case("rightfoot") {
        return "rightLeg";
    }
    for key in CANONICAL_KEYS {
        if key.eq_ignore_ascii_case(name) {
            return key;
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_label_is_case_insensitive_and_one_based() {
        assert_eq!(part_label("head"), Some(BodyPart::Head));
        assert_eq!(part_label("HEAD"), Some(BodyPart::Head));
        assert_eq!(part_label("leftEye"), Some(BodyPart::LeftEye));
        assert_eq!(part_label("lefteye"), Some(BodyPart::LeftEye));
        assert_eq!(part_label("nsys"), Some(BodyPart::Nsys));
        assert_eq!(BodyPart::Head as u32, 1);
        assert_eq!(BodyPart::LeftEye as u32, 2);
        assert_eq!(BodyPart::Nsys as u32, 14);
        assert_eq!(part_label("left eye"), None);
        assert_eq!(part_label(""), None);
    }

    #[test]
    fn labels() {
        assert_eq!(BodyPart::LeftEye.label(), "left eye");
        assert_eq!(BodyPart::RightHand.label(), "right hand");
        assert_eq!(BodyPart::Nsys.label(), "nervous system");
        assert_eq!(BodyPart::LeftLeg.key(), "leftLeg");
        assert_eq!(BODY_PARTS.len(), 14);
        assert_eq!(BodyPart::all().count(), 14);
    }

    #[test]
    fn canonical_aliases() {
        assert_eq!(canonical_part("nerves"), "nsys");
        assert_eq!(canonical_part("NERVES"), "nsys");
        assert_eq!(canonical_part("leftfoot"), "leftLeg");
        assert_eq!(canonical_part("rightfoot"), "rightLeg");
        assert_eq!(canonical_part("LEFTHAND"), "leftHand");
        assert_eq!(canonical_part("nervous system"), "");
        assert_eq!(canonical_part("elbow"), "");
        assert_eq!(canonical_part(""), "");
    }
}
