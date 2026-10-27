//! `UCS` (combat-specialization) lines.
//!
//! Recovered literals: `"positioning against"`, `"vulnerable to a followup"`,
//! `"crimson mist"`, def family `"ucs"`, group `"tier"`, calls
//! `PositionTier::parse` (680 B), `UcsAttack::parse` (716 B) and
//! `target::any_link`. The `tier` group ([`crate::pattern::Groups`]) is fed to
//! `PositionTier::parse` before the plain-text phrases are tried.

use super::defs::{defs, Defs};

/// The UCS positioning tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PositionTier {
    /// `"positioning against"` — you have the advantage.
    Positioning,
    /// `"vulnerable to a followup"`.
    Vulnerable,
    /// `"crimson mist"` — the UCS proc state.
    CrimsonMist,
}

impl PositionTier {
    /// @0x0000000101be? (680 B)
    pub fn parse(text: &str) -> Option<PositionTier> {
        let text = text.trim();
        if text.contains("vulnerable to a followup") {
            Some(PositionTier::Vulnerable)
        } else if text.contains("crimson mist") {
            Some(PositionTier::CrimsonMist)
        } else if text.contains("positioning against") {
            Some(PositionTier::Positioning)
        } else {
            None
        }
    }
}

/// The UCS attack named in a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UcsAttack(pub String);

impl UcsAttack {
    /// @0x0000000101be? (716 B)
    pub fn parse(text: &str) -> Option<UcsAttack> {
        UcsAttack::parse_with_defs(defs(), text)
    }

    /// [`UcsAttack::parse`] against a caller-supplied table.
    pub fn parse_with_defs(table: &Defs, text: &str) -> Option<UcsAttack> {
        let def = table.first_match(text)?;
        if def.family != "ucs" {
            return None;
        }
        Some(UcsAttack(def.name.clone()))
    }
}

/// A classified UCS line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UcsLine {
    pub attack: Option<UcsAttack>,
    pub tier: Option<PositionTier>,
}

impl UcsLine {
    /// @0x0000000101be580c (1972 B)
    pub fn classify(text: &str) -> Option<UcsLine> {
        UcsLine::classify_with_defs(defs(), text)
    }

    /// [`UcsLine::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<UcsLine> {
        let mut attack = None;
        let mut tier = None;
        if let Some((def, groups)) = table.first_match_groups(text) {
            if def.family == "ucs" {
                attack = Some(UcsAttack(def.name.clone()));
                tier = groups
                    .get("tier")
                    .and_then(PositionTier::parse);
            }
        }
        let tier = tier.or_else(|| PositionTier::parse(text));
        if attack.is_none() && tier.is_none() {
            return None;
        }
        Some(UcsLine { attack, tier })
    }
}

#[cfg(test)]
mod tests {
    use super::super::defs::Def;
    use super::*;

    #[test]
    fn tiers_are_recognised_from_their_phrases() {
        assert_eq!(
            PositionTier::parse("You are positioning against the kobold."),
            Some(PositionTier::Positioning)
        );
        assert_eq!(
            PositionTier::parse("The kobold is vulnerable to a followup."),
            Some(PositionTier::Vulnerable)
        );
        assert_eq!(
            PositionTier::parse("A cloud of crimson mist surrounds you."),
            Some(PositionTier::CrimsonMist)
        );
        assert_eq!(PositionTier::parse("nothing"), None);
    }

    #[test]
    fn ucs_line_requires_either_half() {
        assert_eq!(UcsLine::classify("nothing"), None);
        assert!(UcsLine::classify("you are positioning against it").is_some());
    }

    #[test]
    fn the_tier_group_is_used_when_the_row_captures_one() {
        let table = Defs {
            defs: vec![Def::new(
                "ucs",
                "sunder",
                "damaging",
                1,
                "",
                "",
                "^Sunder \\[(?P<tier>vulnerable to a followup)\\]",
            )],
        };
        let line = UcsLine::classify_with_defs(&table, "Sunder [vulnerable to a followup]").unwrap();
        assert_eq!(line.attack, Some(UcsAttack("sunder".into())));
        assert_eq!(line.tier, Some(PositionTier::Vulnerable));
    }
}
