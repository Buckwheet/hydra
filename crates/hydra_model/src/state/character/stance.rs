//! Combat stance.
//!
//! `Stance::parse` (11540 B at `0x101b7ad74`) matches the six names recovered
//! from the literal at `0x1039cb...`: `offensive`, `advance`, `forward`,
//! `neutral`, `guarded`, `defensive`.
//!
//! `Stance::from_percent` (68 B, recovered exactly) maps a 0..=100+ percentage
//! onto the same enum:
//!
//! ```text
//!  0        -> 0
//!  1..=20   -> 1
//! 21..=40   -> 2
//! 41..=60   -> 3
//! 61..=80   -> 4
//! 81..=100  -> 5
//! > 100     -> 6
//! ```
//!
//! `Stance::parse_bar_text` (396 B) trims, splits on `" ("`, requires the text
//! to end in `"%)"` (`ldurh w10, [x10, #-2]` compared against `0x2925`) and
//! then parses the percentage.
//!
//! `// UNKNOWN:` the name of variant 6 — it is only reachable through a
//! percentage above 100.

/// A combat stance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stance {
    #[default]
    Offensive,
    Advance,
    Forward,
    Neutral,
    Guarded,
    Defensive,
    /// `// UNKNOWN:` reachable only from `from_percent(p)` with `p > 100`.
    Unknown,
}

impl Stance {
    /// @0x0000000101b7ad74 (11540 B) — the six recovered spellings.
    pub fn parse(text: &str) -> Option<Stance> {
        let text = text.trim().to_ascii_lowercase();
        match text.as_str() {
            "offensive" => Some(Stance::Offensive),
            "advance" => Some(Stance::Advance),
            "forward" => Some(Stance::Forward),
            "neutral" => Some(Stance::Neutral),
            "guarded" => Some(Stance::Guarded),
            "defensive" => Some(Stance::Defensive),
            _ => None,
        }
    }

    /// @0x0000000101b7aba4 (68 B) — recovered thresholds.
    pub fn from_percent(percent: u32) -> Stance {
        match percent {
            0 => Stance::Offensive,
            1..=20 => Stance::Advance,
            21..=40 => Stance::Forward,
            41..=60 => Stance::Neutral,
            61..=80 => Stance::Guarded,
            81..=100 => Stance::Defensive,
            _ => Stance::Unknown,
        }
    }

    /// 0..=6, the recovered discriminants.
    pub fn index(self) -> u8 {
        match self {
            Stance::Offensive => 0,
            Stance::Advance => 1,
            Stance::Forward => 2,
            Stance::Neutral => 3,
            Stance::Guarded => 4,
            Stance::Defensive => 5,
            Stance::Unknown => 6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Stance::Offensive => "offensive",
            Stance::Advance => "advance",
            Stance::Forward => "forward",
            Stance::Neutral => "neutral",
            Stance::Guarded => "guarded",
            Stance::Defensive => "defensive",
            Stance::Unknown => "",
        }
    }

    /// @0x0000000101b7abe8 (396 B) — `"Defensive (81%)"` style bar text.
    pub fn parse_bar_text(text: &str) -> Option<Stance> {
        let text = text.trim();
        if let Some((name, rest)) = text.split_once(" (") {
            if let Some(number) = rest.strip_suffix("%)") {
                if let Ok(percent) = number.trim().parse::<u32>() {
                    return Some(Stance::from_percent(percent));
                }
            }
            return Stance::parse(name);
        }
        Stance::parse(text)
    }
}

impl crate::state::character::Character {
    /// Set the stance from a bar/text reading.
    pub fn set_stance_text(&mut self, text: &str) -> bool {
        match Stance::parse_bar_text(text) {
            Some(stance) if stance != self.stance => {
                self.stance = stance;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_percent_thresholds_are_recovered() {
        assert_eq!(Stance::from_percent(0), Stance::Offensive);
        assert_eq!(Stance::from_percent(1), Stance::Advance);
        assert_eq!(Stance::from_percent(20), Stance::Advance);
        assert_eq!(Stance::from_percent(21), Stance::Forward);
        assert_eq!(Stance::from_percent(40), Stance::Forward);
        assert_eq!(Stance::from_percent(41), Stance::Neutral);
        assert_eq!(Stance::from_percent(60), Stance::Neutral);
        assert_eq!(Stance::from_percent(61), Stance::Guarded);
        assert_eq!(Stance::from_percent(80), Stance::Guarded);
        assert_eq!(Stance::from_percent(81), Stance::Defensive);
        assert_eq!(Stance::from_percent(100), Stance::Defensive);
        assert_eq!(Stance::from_percent(101), Stance::Unknown);
    }

    #[test]
    fn parse_covers_the_six_names() {
        for stance in [
            Stance::Offensive,
            Stance::Advance,
            Stance::Forward,
            Stance::Neutral,
            Stance::Guarded,
            Stance::Defensive,
        ] {
            assert_eq!(Stance::parse(stance.name()), Some(stance));
            assert_eq!(Stance::parse(&stance.name().to_uppercase()), Some(stance));
        }
        assert_eq!(Stance::parse("defencive"), None);
    }

    #[test]
    fn bar_text_uses_the_percentage() {
        assert_eq!(Stance::parse_bar_text("Defensive (100%)"), Some(Stance::Defensive));
        assert_eq!(Stance::parse_bar_text("  Forward (30%) "), Some(Stance::Forward));
        assert_eq!(Stance::parse_bar_text("Neutral"), Some(Stance::Neutral));
        assert_eq!(Stance::parse_bar_text("nonsense"), None);
    }

    #[test]
    fn indices_match_the_discriminants() {
        assert_eq!(Stance::Offensive.index(), 0);
        assert_eq!(Stance::Defensive.index(), 5);
        assert_eq!(Stance::Unknown.index(), 6);
    }
}
