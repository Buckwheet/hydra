//! Creature status classification.
//!
//! `Classification::parse` (4184 B) matches the concatenated status-word run
//! from the string pool:
//! `webbed disoriented stunned rooted calmed prone sitting flying hidden blind
//! dispelled tangleweed natures_decay terrified dazed immobile diseased
//! bleeding confused frenzied kneeling poisoned silenced sleeping sunburst
//! weakened hovering crippled`.

/// One creature status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Classification {
    Webbed,
    Disoriented,
    Stunned,
    Rooted,
    Calmed,
    Prone,
    Sitting,
    Flying,
    Hidden,
    Blind,
    Dispelled,
    Tangleweed,
    NaturesDecay,
    Terrified,
    Dazed,
    Immobile,
    Diseased,
    Bleeding,
    Confused,
    Frenzied,
    Kneeling,
    Poisoned,
    Silenced,
    Sleeping,
    Sunburst,
    Weakened,
    Hovering,
    Crippled,
}

impl Classification {
    /// Every status word, in the recovered order.
    pub const ALL: [Classification; 28] = [
        Classification::Webbed,
        Classification::Disoriented,
        Classification::Stunned,
        Classification::Rooted,
        Classification::Calmed,
        Classification::Prone,
        Classification::Sitting,
        Classification::Flying,
        Classification::Hidden,
        Classification::Blind,
        Classification::Dispelled,
        Classification::Tangleweed,
        Classification::NaturesDecay,
        Classification::Terrified,
        Classification::Dazed,
        Classification::Immobile,
        Classification::Diseased,
        Classification::Bleeding,
        Classification::Confused,
        Classification::Frenzied,
        Classification::Kneeling,
        Classification::Poisoned,
        Classification::Silenced,
        Classification::Sleeping,
        Classification::Sunburst,
        Classification::Weakened,
        Classification::Hovering,
        Classification::Crippled,
    ];

    /// The storage spelling (the recovered literal).
    pub fn key(self) -> &'static str {
        match self {
            Classification::Webbed => "webbed",
            Classification::Disoriented => "disoriented",
            Classification::Stunned => "stunned",
            Classification::Rooted => "rooted",
            Classification::Calmed => "calmed",
            Classification::Prone => "prone",
            Classification::Sitting => "sitting",
            Classification::Flying => "flying",
            Classification::Hidden => "hidden",
            Classification::Blind => "blind",
            Classification::Dispelled => "dispelled",
            Classification::Tangleweed => "tangleweed",
            Classification::NaturesDecay => "natures_decay",
            Classification::Terrified => "terrified",
            Classification::Dazed => "dazed",
            Classification::Immobile => "immobile",
            Classification::Diseased => "diseased",
            Classification::Bleeding => "bleeding",
            Classification::Confused => "confused",
            Classification::Frenzied => "frenzied",
            Classification::Kneeling => "kneeling",
            Classification::Poisoned => "poisoned",
            Classification::Silenced => "silenced",
            Classification::Sleeping => "sleeping",
            Classification::Sunburst => "sunburst",
            Classification::Weakened => "weakened",
            Classification::Hovering => "hovering",
            Classification::Crippled => "crippled",
        }
    }

    /// @0x0000000101c0f3b8 (4184 B)
    pub fn parse(text: &str) -> Option<Classification> {
        let text = text.trim();
        Classification::ALL
            .iter()
            .copied()
            .find(|c| c.key().eq_ignore_ascii_case(text))
    }

    /// Every status mentioned in a line, in the recovered word order.
    pub fn parse_all(text: &str) -> Vec<Classification> {
        let lower = text.to_ascii_lowercase();
        Classification::ALL
            .iter()
            .copied()
            .filter(|c| lower.contains(c.key()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_word_round_trips() {
        for status in Classification::ALL {
            assert_eq!(Classification::parse(status.key()), Some(status));
        }
        assert_eq!(Classification::ALL.len(), 28);
    }

    #[test]
    fn recovered_order_starts_with_webbed() {
        assert_eq!(Classification::ALL[0].key(), "webbed");
        assert_eq!(Classification::ALL[12].key(), "natures_decay");
        assert_eq!(Classification::ALL[27].key(), "crippled");
    }

    #[test]
    fn parse_all_finds_every_mentioned_status() {
        let found = Classification::parse_all("The kobold is stunned and prone.");
        assert_eq!(found, vec![Classification::Stunned, Classification::Prone]);
        assert!(Classification::parse_all("The kobold is fine.").is_empty());
    }

    #[test]
    fn unknown_words_are_rejected() {
        assert_eq!(Classification::parse("sleepy"), None);
    }
}
