//! Crit-table value types.
//!
//! Both parsers are byte-for-byte length dispatch tables in the binary
//! (`WoundLocation::parse` 512 B, `Position::parse` 216 B): a `cmp` on the
//! string length, then 4/8-byte big-endian comparisons of the literal. The
//! accepted spellings and their variant numbers were read out of those
//! comparisons.

/// Where a critical hit landed.
///
/// Recovered values (`mov w0, #n` per accepted spelling):
///
/// | input       | value |
/// |-------------|-------|
/// | `head`      | 0     |
/// | `neck`      | 1     |
/// | `chest`     | 2     |
/// | `abdomen`   | 3     |
/// | `back`      | 4     |
/// | `nerves`    | 5     |
/// | `right_leg` | 6     |
/// | `both_eyes` | 7     |
/// | anything    | 8     |
///
/// `// UNKNOWN:` the variant *names*; the numbers and spellings are recovered.
/// Note that `left_leg`, `left_arm`, `right_arm`, `left_hand`, `right_hand`,
/// `left_eye`, `right_eye` and `unspecified` appear in the string pool but are
/// **not** accepted by `parse` — they fall into `Unknown` (8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WoundLocation {
    Head,
    Neck,
    Chest,
    Abdomen,
    Back,
    Nerves,
    RightLeg,
    BothEyes,
    #[default]
    Unknown,
}

impl WoundLocation {
    pub const ALL: [WoundLocation; 9] = [
        WoundLocation::Head,
        WoundLocation::Neck,
        WoundLocation::Chest,
        WoundLocation::Abdomen,
        WoundLocation::Back,
        WoundLocation::Nerves,
        WoundLocation::RightLeg,
        WoundLocation::BothEyes,
        WoundLocation::Unknown,
    ];

    pub fn parse(text: &str) -> WoundLocation {
        match text {
            "head" => WoundLocation::Head,
            "neck" => WoundLocation::Neck,
            "chest" => WoundLocation::Chest,
            "abdomen" => WoundLocation::Abdomen,
            "back" => WoundLocation::Back,
            "nerves" => WoundLocation::Nerves,
            "right_leg" => WoundLocation::RightLeg,
            "both_eyes" => WoundLocation::BothEyes,
            _ => WoundLocation::Unknown,
        }
    }

    pub fn as_index(self) -> u8 {
        match self {
            WoundLocation::Head => 0,
            WoundLocation::Neck => 1,
            WoundLocation::Chest => 2,
            WoundLocation::Abdomen => 3,
            WoundLocation::Back => 4,
            WoundLocation::Nerves => 5,
            WoundLocation::RightLeg => 6,
            WoundLocation::BothEyes => 7,
            WoundLocation::Unknown => 8,
        }
    }

    /// The storage spelling (the one `parse` accepts).
    pub fn as_str(self) -> &'static str {
        match self {
            WoundLocation::Head => "head",
            WoundLocation::Neck => "neck",
            WoundLocation::Chest => "chest",
            WoundLocation::Abdomen => "abdomen",
            WoundLocation::Back => "back",
            WoundLocation::Nerves => "nerves",
            WoundLocation::RightLeg => "right_leg",
            WoundLocation::BothEyes => "both_eyes",
            WoundLocation::Unknown => "unspecified",
        }
    }
}

/// Body position while the critical hit landed.
///
/// Recovered values: `kneeling` = 0, `prone` = 1, `sitting` = 2, everything
/// else (including the empty column and `standing`) = 3.
///
/// `// UNKNOWN:` the name of variant 3 — the crit TSV uses an empty column for
/// "upright", and `standing` is *not* an accepted spelling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Position {
    Kneeling,
    Prone,
    Sitting,
    #[default]
    Standing,
}

impl Position {
    pub fn parse(text: &str) -> Position {
        match text {
            "kneeling" => Position::Kneeling,
            "prone" => Position::Prone,
            "sitting" => Position::Sitting,
            _ => Position::Standing,
        }
    }

    pub fn as_index(self) -> u8 {
        match self {
            Position::Kneeling => 0,
            Position::Prone => 1,
            Position::Sitting => 2,
            Position::Standing => 3,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Position::Kneeling => "kneeling",
            Position::Prone => "prone",
            Position::Sitting => "sitting",
            Position::Standing => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wound_location_parse_covers_every_accepted_spelling() {
        assert_eq!(WoundLocation::parse("head"), WoundLocation::Head);
        assert_eq!(WoundLocation::parse("neck"), WoundLocation::Neck);
        assert_eq!(WoundLocation::parse("chest"), WoundLocation::Chest);
        assert_eq!(WoundLocation::parse("abdomen"), WoundLocation::Abdomen);
        assert_eq!(WoundLocation::parse("back"), WoundLocation::Back);
        assert_eq!(WoundLocation::parse("nerves"), WoundLocation::Nerves);
        assert_eq!(WoundLocation::parse("right_leg"), WoundLocation::RightLeg);
        assert_eq!(WoundLocation::parse("both_eyes"), WoundLocation::BothEyes);
    }

    #[test]
    fn wound_location_rejects_the_other_limbs() {
        for absent in [
            "left_leg",
            "left_arm",
            "right_arm",
            "left_hand",
            "right_hand",
            "left_eye",
            "right_eye",
            "unspecified",
            "",
            "HEAD",
        ] {
            assert_eq!(WoundLocation::parse(absent), WoundLocation::Unknown, "{absent}");
        }
    }

    #[test]
    fn wound_location_indices_round_trip() {
        for loc in WoundLocation::ALL {
            assert_eq!(WoundLocation::parse(loc.as_str()), loc);
        }
        assert_eq!(WoundLocation::RightLeg.as_index(), 6);
        assert_eq!(WoundLocation::Unknown.as_index(), 8);
    }

    #[test]
    fn position_parse_matches_length_table() {
        assert_eq!(Position::parse("kneeling"), Position::Kneeling);
        assert_eq!(Position::parse("prone"), Position::Prone);
        assert_eq!(Position::parse("sitting"), Position::Sitting);
        // `standing` is *not* accepted; the empty TSV column lands here too.
        assert_eq!(Position::parse("standing"), Position::Standing);
        assert_eq!(Position::parse(""), Position::Standing);
        assert_eq!(Position::Standing.as_index(), 3);
    }
}
