//! `UCS` (combat-specialization) lines.
//!
//! Recovered literals: `"positioning against"`, `"vulnerable to a followup"`,
//! `"crimson mist"`, def family `"ucs"`, group `"tier"`, calls
//! `PositionTier::parse` (680 B), `UcsAttack::parse` (716 B) and
//! `target::any_link`.

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
        let def = super::defs::defs().first_match(text)?;
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
        let attack = UcsAttack::parse(text);
        let tier = PositionTier::parse(text);
        if attack.is_none() && tier.is_none() {
            return None;
        }
        Some(UcsLine { attack, tier })
    }
}

#[cfg(test)]
mod tests {
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
}
