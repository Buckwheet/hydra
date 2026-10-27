//! Assault / sequence brackets.
//!
//! Recovered: `assault_start` (824 B) / `assault_end` (320 B) use the def family
//! `"assault"` with `Defs::first_match_with_role` and group `"target"`;
//! `sequence_start` (1144 B) / `sequence_end` (632 B) the family
//! `"sequence"`; `AssaultName::parse` (372 B) turns the def `name` into the
//! enum.
//!
//! `// UNKNOWN:` the `AssaultName` variant list (only the parse dispatch is
//! visible); the names below are the ones the string pool shows next to the
//! bracket families.

/// A named assault.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AssaultName(pub String);

impl AssaultName {
    /// @0x0000000101b? (372 B)
    pub fn parse(text: &str) -> Option<AssaultName> {
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(AssaultName(text.to_string()))
        }
    }
}

/// An open assault bracket.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Assault {
    pub name: AssaultName,
    pub target: Option<String>,
}

/// An open sequence bracket.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sequence {
    pub target: Option<String>,
}

/// @0x0000000101be67b8 (824 B)
pub fn assault_start(text: &str) -> Option<Assault> {
    let def = super::defs::defs().first_match_with_role(text, "assault")?;
    Some(Assault {
        name: AssaultName::parse(&def.name)?,
        target: None,
    })
}

/// @0x0000000101be6400 (320 B)
pub fn assault_end(text: &str) -> Option<AssaultName> {
    let def = super::defs::defs().first_match_with_role(text, "assault")?;
    AssaultName::parse(&def.name)
}

/// @0x0000000101be6af0 (1144 B)
pub fn sequence_start(text: &str) -> Option<Sequence> {
    let _def = super::defs::defs().first_match_with_role(text, "sequence")?;
    Some(Sequence { target: None })
}

/// @0x0000000101be6540 (632 B) — the sequence end marker (no payload).
pub fn sequence_end(text: &str) -> bool {
    super::defs::defs()
        .first_match_with_role(text, "sequence")
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_definitions_no_bracket_opens() {
        assert!(assault_start("You begin your assault").is_none());
        assert!(assault_end("Your assault ends").is_none());
        assert!(sequence_start("You begin a sequence").is_none());
        assert!(!sequence_end("Your sequence ends"));
    }

    #[test]
    fn assault_name_needs_text() {
        assert!(AssaultName::parse("  ").is_none());
        assert_eq!(AssaultName::parse("fury"), Some(AssaultName("fury".into())));
    }
}
