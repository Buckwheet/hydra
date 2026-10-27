//! Combat lines: crits, deaths and creature preparation.
//!
//! Recovered tokens next to this module (0x10399312a): `flare`, `status_you`,
//! `fatal_crit`, and the `creature_death` / `creature_prep` /
//! `creature_trigger` / `creature_recover` family.

use crate::theme::Token;

/// The markers a combat line is recognised by.
pub const MARKS: [(&str, Token); 6] = [
    ("You reveal", Token::Flare),
    ("flare", Token::Flare),
    ("a burst of flame", Token::Flare),
    ("fatal", Token::FatalCrit),
    ("is dead", Token::CreatureDeath),
    ("Death cry:", Token::CreatureDeath),
];

/// `@0x1018719c0` — the token a combat line paints with.
pub fn mark(line: &str) -> Option<Token> {
    MARKS
        .iter()
        .find(|(needle, _)| line.contains(needle))
        .map(|(_, t)| *t)
}

/// `@0x101871aac` — the creature names a combat line mentions.
///
/// The binary walks the creatures the model knows and keeps the ones whose name
/// appears in the line.
pub fn names<'a>(line: &'a str, known: &'a [String]) -> Vec<&'a str> {
    known
        .iter()
        .map(|s| s.as_str())
        .filter(|name| !name.is_empty() && line.contains(name))
        .collect()
}

/// `@0x101871b68` — the preset a combat line should be styled with.
pub fn preset(token: Token) -> &'static str {
    token.name()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_are_found() {
        assert_eq!(mark("You reveal a rune"), Some(Token::Flare));
        assert_eq!(mark("The troll is dead"), Some(Token::CreatureDeath));
        assert_eq!(mark("Death cry: the troll"), Some(Token::CreatureDeath));
        assert_eq!(mark("You feel yourself flare"), Some(Token::Flare));
        assert_eq!(mark("the wind blows"), None);
        assert_eq!(mark("You feel yourself grow"), None);
    }

    #[test]
    fn names_lists_the_known_creatures_present() {
        let known = vec!["troll".to_string(), "goblin".to_string()];
        assert_eq!(names("the troll is dead", &known), vec!["troll"]);
        assert!(names("nothing", &known).is_empty());
        let empty = vec![String::new()];
        assert!(names("anything", &empty).is_empty());
    }

    #[test]
    fn presets_are_token_names() {
        assert_eq!(preset(Token::FatalCrit), "fatal_crit");
        assert_eq!(preset(Token::CreatureDeath), "creature_death");
    }
}
