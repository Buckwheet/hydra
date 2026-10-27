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

use crate::theme::Rgb;

/// One matched span of a line: what a firing rule recolours.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    /// The rule that fired (its pattern, as [`crate::story::Heard::rule`] keeps it).
    pub rule: String,
    /// Byte offset of the match, from [`matcher::Find::start`].
    pub start: usize,
    /// Byte offset just past the match.
    pub end: usize,
    /// The rule's `color`, when it sets one.
    pub color: Option<Rgb>,
}

/// `matcher::caught` @0x1019659e8 over a whole rule set: every span a line's
/// rules would recolour, in the matcher's order (longest pattern first).
///
/// This is the model-side half of highlighting; the painting half
/// ([`crate::story::Story::paint_with`]) only splits text at these spans and
/// never invents a match of its own.
pub fn spans(text: &str, matcher: &Matcher) -> Vec<Span> {
    let mut out = Vec::new();
    for compiled in matcher.rules() {
        if let Some(find) = compiled.pattern.find(text) {
            out.push(Span {
                rule: compiled.rule.pattern.clone(),
                start: find.start,
                end: find.end,
                color: compiled.rule.color,
            });
        }
    }
    out
}

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

    #[test]
    fn spans_are_the_matches_of_every_firing_rule() {
        let m = Matcher::new(&[
            Rule {
                pattern: "orc".into(),
                color: Some(crate::theme::rgb(0xff, 0x00, 0x00)),
                ..Rule::default()
            },
            // A longer pattern sorts first and is reported first.
            Rule {
                pattern: "a large orc".into(),
                color: None,
                ..Rule::default()
            },
            // This one does not fire.
            Rule {
                pattern: "elf".into(),
                ..Rule::default()
            },
        ]);
        let found = spans("a large orc arrives", &m);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].rule, "a large orc");
        assert_eq!((found[0].start, found[0].end), (0, 11));
        assert_eq!(found[0].color, None);
        assert_eq!(found[1].rule, "orc");
        assert_eq!((found[1].start, found[1].end), (8, 11));
        assert_eq!(found[1].color, Some(crate::theme::rgb(0xff, 0x00, 0x00)));
        // The spans really are the matched text.
        assert_eq!(&"a large orc arrives"[found[1].start..found[1].end], "orc");
        // Nothing matches, nothing is reported.
        assert!(spans("nothing here", &m).is_empty());
    }

    #[test]
    fn spans_respect_the_rule_flags() {
        let m = Matcher::new(&[Rule {
            pattern: "orc".into(),
            whole_word: true,
            ..Rule::default()
        }]);
        assert_eq!(spans("an orc", &m).len(), 1);
        assert!(spans("an orcish thing", &m).is_empty());
    }
}
