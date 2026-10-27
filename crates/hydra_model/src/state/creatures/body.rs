//! Body parts and wound mapping.
//!
//! `BodyPart::from_wound` (2520 B at `0x101b88e28`) maps the crit-table wound
//! spellings onto [`gs_base::body::BodyPart`]. The literals it uses are the
//! same set the crit table stores (`head`, `neck`, `chest`, `abdomen`, `back`,
//! `nerves`, `right_leg`, `both_eyes`, `left_arm`, …) plus `unspecified`.

pub use gs_base::body::BodyPart;

/// Map a wound location string onto a body part.
///
/// @0x0000000101b88e28 (2520 B)
pub fn from_wound(wound: &str) -> Option<BodyPart> {
    Some(match wound.trim() {
        "head" => BodyPart::Head,
        "neck" => BodyPart::Neck,
        "chest" => BodyPart::Chest,
        "abdomen" => BodyPart::Abdomen,
        "back" => BodyPart::Back,
        "nerves" => BodyPart::Nsys,
        "left_arm" => BodyPart::LeftArm,
        "right_arm" => BodyPart::RightArm,
        "left_hand" => BodyPart::LeftHand,
        "right_hand" => BodyPart::RightHand,
        "left_leg" => BodyPart::LeftLeg,
        "right_leg" => BodyPart::RightLeg,
        "left_eye" => BodyPart::LeftEye,
        "right_eye" => BodyPart::RightEye,
        // `// UNKNOWN:` the 2520-byte function folds the paired eye location
        // onto a single part; `gs_base` has no "both eyes", so the head is the
        // documented choice.
        "both_eyes" => BodyPart::Head,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crit_locations_map_onto_body_parts() {
        assert_eq!(from_wound("head"), Some(BodyPart::Head));
        assert_eq!(from_wound("neck"), Some(BodyPart::Neck));
        assert_eq!(from_wound("chest"), Some(BodyPart::Chest));
        assert_eq!(from_wound("abdomen"), Some(BodyPart::Abdomen));
        assert_eq!(from_wound("left_arm"), Some(BodyPart::LeftArm));
        assert_eq!(from_wound("right_leg"), Some(BodyPart::RightLeg));
    }

    #[test]
    fn nerves_maps_to_the_nervous_system() {
        assert_eq!(from_wound("nerves"), Some(BodyPart::Nsys));
    }

    #[test]
    fn both_eyes_folds_into_the_head() {
        assert_eq!(from_wound("both_eyes"), Some(BodyPart::Head));
    }

    #[test]
    fn unspecified_has_no_part() {
        assert_eq!(from_wound("unspecified"), None);
        assert_eq!(from_wound(""), None);
    }
}
