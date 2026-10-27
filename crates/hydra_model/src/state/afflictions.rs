//! Affliction messages.
//!
//! `classify` (4836 B) matches the recovered affliction prose, e.g.
//! `"You shake off the immobilization that was restricting your movements"`,
//! `"You begin to feel a strange fatigue, spreading throughout your body."`,
//! `"The horrible pain in your vocal cords subsides as you spit o…"`,
//! `"No longer able to fight this odd paralysis, you collapse to …"`,
//! `"With a shaky gasp and trembling muscles, you regain at least…"`,
//! `"Fine coordination is difficult, but at least you can move at…"`.
//! It receives a `&TextFrame` and calls `as_run` on it.

/// An affliction event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Affliction {
    Immobilized,
    Fatigue,
    VocalCords,
    Paralysis,
    Recovered,
    Shaky,
}

impl Affliction {
    pub fn key(self) -> &'static str {
        match self {
            Affliction::Immobilized => "immobilized",
            Affliction::Fatigue => "fatigue",
            Affliction::VocalCords => "vocal_cords",
            Affliction::Paralysis => "paralysis",
            Affliction::Recovered => "recovered",
            Affliction::Shaky => "shaky",
        }
    }
}

/// @0x0000000101bfd0c0 (4836 B)
pub fn classify(text: &str) -> Option<Affliction> {
    const TABLE: &[(&str, Affliction)] = &[
        (
            "You shake off the immobilization that was restricting your movements",
            Affliction::Immobilized,
        ),
        (
            "You begin to feel a strange fatigue, spreading throughout your body.",
            Affliction::Fatigue,
        ),
        (
            "The horrible pain in your vocal cords subsides",
            Affliction::VocalCords,
        ),
        (
            "No longer able to fight this odd paralysis, you collapse",
            Affliction::Paralysis,
        ),
        (
            "With a shaky gasp and trembling muscles, you regain at least",
            Affliction::Recovered,
        ),
        (
            "Fine coordination is difficult, but at least you can move",
            Affliction::Shaky,
        ),
    ];
    TABLE
        .iter()
        .find(|(needle, _)| text.contains(needle))
        .map(|(_, event)| *event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_messages_are_recognised() {
        assert_eq!(
            classify("You shake off the immobilization that was restricting your movements!"),
            Some(Affliction::Immobilized)
        );
        assert_eq!(
            classify("You begin to feel a strange fatigue, spreading throughout your body."),
            Some(Affliction::Fatigue)
        );
        assert_eq!(
            classify("No longer able to fight this odd paralysis, you collapse to the ground."),
            Some(Affliction::Paralysis)
        );
    }

    #[test]
    fn unrelated_messages_are_none() {
        assert_eq!(classify("You feel fine."), None);
    }

    #[test]
    fn keys_are_stable() {
        assert_eq!(Affliction::Immobilized.key(), "immobilized");
        assert_eq!(Affliction::VocalCords.key(), "vocal_cords");
    }
}
