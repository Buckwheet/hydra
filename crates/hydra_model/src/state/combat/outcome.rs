//! Attack outcomes (`Outcome`).
//!
//! Recovered from `Outcome::classify` (2016 B): the def family is `"outcome"`
//! and the names are the concatenated literal
//! `hitevadeblockparrywardedward_failedinterceptunaffectedfumblehindrancealready_deadoutcome`
//! — i.e. `hit`, `evade`, `block`, `parry`, `warded`, `ward_failed`,
//! `intercept`, `unaffected`, `fumble`, `hindrance`, `already_dead`, plus
//! `miss` (a bare literal) and `resisted`, `confused`, `frenzied`.

/// A resolved attack outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Outcome {
    Hit,
    Evade,
    Block,
    Parry,
    Warded,
    WardFailed,
    Intercept,
    Unaffected,
    Fumble,
    Hindrance,
    AlreadyDead,
    Miss,
    Resisted,
    Confused,
    Frenzied,
}

impl Outcome {
    /// The recovered `name` column value.
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Hit => "hit",
            Outcome::Evade => "evade",
            Outcome::Block => "block",
            Outcome::Parry => "parry",
            Outcome::Warded => "warded",
            Outcome::WardFailed => "ward_failed",
            Outcome::Intercept => "intercept",
            Outcome::Unaffected => "unaffected",
            Outcome::Fumble => "fumble",
            Outcome::Hindrance => "hindrance",
            Outcome::AlreadyDead => "already_dead",
            Outcome::Miss => "miss",
            Outcome::Resisted => "resisted",
            Outcome::Confused => "confused",
            Outcome::Frenzied => "frenzied",
        }
    }

    pub fn parse(text: &str) -> Option<Outcome> {
        let text = text.trim();
        [
            Outcome::Hit,
            Outcome::Evade,
            Outcome::Block,
            Outcome::Parry,
            Outcome::Warded,
            Outcome::WardFailed,
            Outcome::Intercept,
            Outcome::Unaffected,
            Outcome::Fumble,
            Outcome::Hindrance,
            Outcome::AlreadyDead,
            Outcome::Miss,
            Outcome::Resisted,
            Outcome::Confused,
            Outcome::Frenzied,
        ]
        .into_iter()
        .find(|outcome| outcome.name() == text)
    }

    /// @0x0000000101bffc98 (2016 B) — classify a whole line.
    ///
    /// `// UNKNOWN:` needs the def table (not embedded); a direct name match is
    /// the only thing that can be decided without it.
    pub fn classify(text: &str) -> Option<Outcome> {
        let def = super::defs::defs().first_match(text)?;
        if def.family != "outcome" {
            return None;
        }
        Outcome::parse(&def.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_names_round_trip() {
        for outcome in [
            Outcome::Hit,
            Outcome::Evade,
            Outcome::Block,
            Outcome::Parry,
            Outcome::Warded,
            Outcome::WardFailed,
            Outcome::Intercept,
            Outcome::Unaffected,
            Outcome::Fumble,
            Outcome::Hindrance,
            Outcome::AlreadyDead,
            Outcome::Miss,
            Outcome::Resisted,
            Outcome::Confused,
            Outcome::Frenzied,
        ] {
            assert_eq!(Outcome::parse(outcome.name()), Some(outcome));
        }
        assert_eq!(Outcome::parse("dodged"), None);
    }

    #[test]
    fn classify_needs_definitions() {
        assert_eq!(Outcome::classify("You evade the attack!"), None);
    }
}
