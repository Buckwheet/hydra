//! `AttackCategory`, `MessageKind` and `Treasure`.
//!
//! `AttackCategory::parse` (532 B) and `MessageKind::parse` (888 B) are used by
//! `bounty::classify`; `Treasure::is_empty` (80 B) is a predicate.
//!
//! `// UNKNOWN:` the accepted spellings of both enumerations — only the call
//! sites (`bounty::classify`) are visible, and the literals are shared with the
//! creature-message tables.

/// How a creature attacks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AttackCategory {
    #[default]
    Unknown,
    Melee,
    Ranged,
    Bolt,
    Spell,
    Breath,
    Special,
}

impl AttackCategory {
    pub fn parse(text: &str) -> AttackCategory {
        match text.trim().to_ascii_lowercase().as_str() {
            "melee" => AttackCategory::Melee,
            "ranged" => AttackCategory::Ranged,
            "bolt" => AttackCategory::Bolt,
            "spell" | "cs" => AttackCategory::Spell,
            "breath" => AttackCategory::Breath,
            "special" | "maneuver" => AttackCategory::Special,
            _ => AttackCategory::Unknown,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            AttackCategory::Unknown => "unknown",
            AttackCategory::Melee => "melee",
            AttackCategory::Ranged => "ranged",
            AttackCategory::Bolt => "bolt",
            AttackCategory::Spell => "spell",
            AttackCategory::Breath => "breath",
            AttackCategory::Special => "special",
        }
    }
}

/// What kind of message a bounty/creature line is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageKind {
    #[default]
    Unknown,
    Speech,
    Whisper,
    SpellLoss,
    Target,
}

impl MessageKind {
    pub fn parse(text: &str) -> MessageKind {
        match text.trim().to_ascii_lowercase().as_str() {
            "speech" => MessageKind::Speech,
            "whisper" | "whispers" => MessageKind::Whisper,
            "spell_loss" => MessageKind::SpellLoss,
            "target" => MessageKind::Target,
            _ => MessageKind::Unknown,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            MessageKind::Unknown => "unknown",
            MessageKind::Speech => "speech",
            MessageKind::Whisper => "whisper",
            MessageKind::SpellLoss => "spell_loss",
            MessageKind::Target => "target",
        }
    }
}

/// The treasure flags a creature carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Treasure {
    pub coins: bool,
    pub gems: bool,
    pub boxes: bool,
    pub skin: bool,
    pub magic: bool,
}

impl Treasure {
    /// @0x0000000101b? (80 B)
    pub fn is_empty(&self) -> bool {
        !(self.coins || self.gems || self.boxes || self.skin || self.magic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_categories_parse_their_keys() {
        for category in [
            AttackCategory::Melee,
            AttackCategory::Ranged,
            AttackCategory::Bolt,
            AttackCategory::Spell,
            AttackCategory::Breath,
            AttackCategory::Special,
        ] {
            assert_eq!(AttackCategory::parse(category.key()), category);
        }
        assert_eq!(AttackCategory::parse("nonsense"), AttackCategory::Unknown);
    }

    #[test]
    fn message_kinds_parse_their_keys() {
        assert_eq!(MessageKind::parse("speech"), MessageKind::Speech);
        assert_eq!(MessageKind::parse("whisper"), MessageKind::Whisper);
        assert_eq!(MessageKind::parse("spell_loss"), MessageKind::SpellLoss);
        assert_eq!(MessageKind::parse("target"), MessageKind::Target);
        assert_eq!(MessageKind::parse("?"), MessageKind::Unknown);
    }

    #[test]
    fn empty_treasure() {
        assert!(Treasure::default().is_empty());
        assert!(!Treasure {
            coins: true,
            ..Treasure::default()
        }
        .is_empty());
    }
}
