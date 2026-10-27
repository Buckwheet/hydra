//! Highlighting: the rule file, the matcher and the importers.
//!
//! A line arriving from the game is matched against the rule set; a rule can
//! recolour it, squelch it, substitute text, start a timer, redirect it to a
//! stream or sound an alarm.  See [`file::Rule`] for the field list and
//! [`matcher::Matcher`] for the matching.

pub mod edit;
pub mod file;
pub mod matcher;
pub mod sound;
pub mod vellum;
pub mod wrayth;

pub use file::{general, HighlightFile, Mark, Rule};
pub use matcher::{caught, Compiled, Matcher, Pattern};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::Heard;

    #[test]
    fn the_general_rules_fire_through_the_matcher() {
        let m = Matcher::new(&general());
        let heard: Vec<Heard> = m.check("You also see a rock.");
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].rule, "You also see");
        assert!(m.check("nothing to see").is_empty());
    }

    #[test]
    fn a_compiled_rule_exposes_its_source() {
        let c = Compiled::new(Rule {
            pattern: "orc".into(),
            ..Rule::default()
        });
        assert_eq!(c.matcher().pattern, "orc");
        assert!(c.pattern.is_match("an Orc"));
    }
}
