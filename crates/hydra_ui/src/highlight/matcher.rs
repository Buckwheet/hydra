//! The highlight matcher.
//!
//! Recovered: `Matcher::{new, check, check_rule, respond, fired_alone,
//! has_stream, timer_default}`, `Compiled::{new, matcher}`, `caught`, plus the
//! helper set `build`, `words_of`, `named_groups`, `pattern_of`, `stream_of`,
//! `seconds`, `pools`, `whose`, `mark`, `finding`.  The drop glue names
//! `Fired`, `Pool`, `Built`, `Word`, `Timing`, `Timer`, `Find` and `Response`.
//!
//! `regex` is not available here, so [`Pattern`] is a small, documented subset:
//! a literal substring with optional case folding, optional whole-word
//! anchoring and optional `^`/`$` line anchors.  Everything the recovered rule
//! table uses (`pattern`, `case_sensitive`, `whole_word`) is expressible in it.

use crate::highlight::file::Rule;
use crate::story::Heard;

/// One matched span of a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Find {
    /// Byte offset of the match.
    pub start: usize,
    /// Byte offset just past the match.
    pub end: usize,
}

/// A compiled pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    /// The needle.
    pub needle: String,
    /// Fold case?
    pub case_sensitive: bool,
    /// Require word boundaries?
    pub whole_word: bool,
    /// Anchor at the start of the line?
    pub anchor_start: bool,
    /// Anchor at the end of the line?
    pub anchor_end: bool,
}

impl Pattern {
    /// `@0x10196adec` — compile a rule's pattern.
    pub fn build(rule: &Rule) -> Pattern {
        let mut needle = rule.pattern.clone();
        let mut anchor_start = false;
        let mut anchor_end = false;
        if let Some(rest) = needle.strip_prefix('^') {
            anchor_start = true;
            needle = rest.to_string();
        }
        if let Some(rest) = needle.strip_suffix('$') {
            anchor_end = true;
            needle = rest.to_string();
        }
        Pattern {
            needle,
            case_sensitive: rule.case_sensitive,
            whole_word: rule.whole_word,
            anchor_start,
            anchor_end,
        }
    }

    /// Find the first match of this pattern in `hay`.
    pub fn find(&self, hay: &str) -> Option<Find> {
        if self.needle.is_empty() {
            return None;
        }
        let (hay_c, needle_c) = if self.case_sensitive {
            (hay.to_string(), self.needle.clone())
        } else {
            (hay.to_lowercase(), self.needle.to_lowercase())
        };
        if self.anchor_start && self.anchor_end {
            return (hay_c == needle_c).then_some(Find {
                start: 0,
                end: hay.len(),
            });
        }
        let mut from = 0usize;
        while let Some(at) = hay_c[from..].find(&needle_c) {
            let start = from + at;
            let end = start + needle_c.len();
            let start_ok = if self.anchor_start {
                start == 0
            } else if self.whole_word {
                !hay_c[..start]
                    .chars()
                    .last()
                    .map(|c| c.is_alphanumeric())
                    .unwrap_or(false)
            } else {
                true
            };
            let end_ok = if self.anchor_end {
                end == hay_c.len()
            } else if self.whole_word {
                !hay_c[end..]
                    .chars()
                    .next()
                    .map(|c| c.is_alphanumeric())
                    .unwrap_or(false)
            } else {
                true
            };
            if start_ok && end_ok {
                return Some(Find { start, end });
            }
            from = start + hay_c[start..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            if from >= hay_c.len() {
                break;
            }
        }
        None
    }

    /// Does the pattern match at all?
    pub fn is_match(&self, hay: &str) -> bool {
        self.find(hay).is_some()
    }
}

/// A rule with its pattern compiled.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    pub rule: Rule,
    pub pattern: Pattern,
}

impl Compiled {
    /// `@0x10196adec` — compile a rule.
    pub fn new(rule: Rule) -> Compiled {
        let pattern = Pattern::build(&rule);
        Compiled { rule, pattern }
    }

    /// `@0x10196af88` — the rule this was compiled from.
    pub fn matcher(&self) -> &Rule {
        &self.rule
    }
}

/// A response a fired rule asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The rule name.
    pub rule: String,
    /// What it wants done: a `sound`, a `redirect` target or a `substitute`.
    pub action: String,
}

/// The compiled rule set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Matcher {
    rules: Vec<Compiled>,
}

impl Matcher {
    /// `@0x101969de4` — compile a rule set.
    pub fn new(rules: &[Rule]) -> Matcher {
        let mut compiled: Vec<Compiled> = rules.iter().cloned().map(Compiled::new).collect();
        // Longest pattern first, so a specific rule wins over a generic one.
        compiled.sort_by(|a, b| b.pattern.needle.len().cmp(&a.pattern.needle.len()));
        Matcher { rules: compiled }
    }

    /// The compiled rules, in match order.
    pub fn rules(&self) -> &[Compiled] {
        &self.rules
    }

    /// `@0x10196a154` — the responses for a line.
    pub fn respond(&self, text: &str) -> Vec<Response> {
        self.rules
            .iter()
            .filter(|r| r.pattern.is_match(text))
            .map(|r| Response {
                rule: r.rule.pattern.clone(),
                action: r
                    .rule
                    .substitute
                    .clone()
                    .or_else(|| r.rule.origin.clone())
                    .or_else(|| r.rule.sound.clone())
                    .unwrap_or_default(),
            })
            .collect()
    }

    /// `@0x10196a10c` — every rule that fires on a line.
    pub fn check(&self, text: &str) -> Vec<Heard> {
        self.rules
            .iter()
            .filter(|r| r.pattern.is_match(text))
            .map(|r| Heard {
                rule: r.rule.pattern.clone(),
                text: text.to_string(),
                sound: r.rule.sound.clone(),
            })
            .collect()
    }

    /// `@0x101965c04` — does the rule at `at` fire on `text`?
    pub fn check_rule(&self, at: usize, text: &str) -> bool {
        self.rules
            .get(at)
            .map(|r| r.pattern.is_match(text))
            .unwrap_or(false)
    }

    /// `@0x101965da0` — `true` when a rule fires on this line and nothing else
    /// on the line is highlighted.
    pub fn fired_alone(&self, text: &str) -> bool {
        let fired = self.rules.iter().filter(|r| r.pattern.is_match(text)).count();
        fired == 1
    }

    /// `@0x101965c84` — is any rule attached to this stream?
    pub fn has_stream(&self, stream: &str) -> bool {
        self.rules
            .iter()
            .any(|r| r.rule.stream.as_deref() == Some(stream))
    }

    /// `@0x101969d10` — the default timer length for a `timer` rule.
    ///
    /// The binary's default is one second (`mov w0, #1`).
    pub fn timer_default() -> u32 {
        1
    }
}

/// `@0x1019659e8` — the `(start, end)` spans on a line for one rule.
pub fn caught(text: &str, rule: &Rule) -> Option<Find> {
    Pattern::build(rule).find(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str) -> Rule {
        Rule {
            pattern: pattern.to_string(),
            case_sensitive: false,
            ..Rule::default()
        }
    }

    #[test]
    fn patterns_match_substrings() {
        let p = Pattern::build(&rule("orc"));
        assert!(p.is_match("an Orc arrives"));
        let f = p.find("an Orc arrives").unwrap();
        assert_eq!(&"an Orc arrives"[f.start..f.end], "Orc");
    }

    #[test]
    fn case_sensitivity() {
        let mut r = rule("orc");
        r.case_sensitive = true;
        assert!(!Pattern::build(&r).is_match("an Orc arrives"));
        assert!(Pattern::build(&r).is_match("an orc arrives"));
    }

    #[test]
    fn whole_words() {
        let mut r = rule("orc");
        r.whole_word = true;
        let p = Pattern::build(&r);
        assert!(p.is_match("an orc arrives"));
        assert!(!p.is_match("an orcish arrives"));
        assert!(!p.is_match("a porcupine"));
    }

    #[test]
    fn anchors() {
        let p = Pattern::build(&rule("^Obvious"));
        assert!(p.is_match("Obvious exits: north"));
        assert!(!p.is_match("The Obvious choice"));
        let p = Pattern::build(&rule("north$"));
        assert!(p.is_match("Obvious exits: north"));
        assert!(!p.is_match("north, south"));
    }

    #[test]
    fn matcher_reports_every_firing_rule() {
        let m = Matcher::new(&[rule("orc"), rule("arrives"), rule("elf")]);
        let heard = m.check("an orc arrives");
        assert_eq!(heard.len(), 2);
        assert!(heard.iter().any(|h| h.rule == "orc"));
        assert!(heard.iter().any(|h| h.rule == "arrives"));
    }

    #[test]
    fn specific_rules_come_first() {
        let m = Matcher::new(&[rule("a"), rule("a long pattern")]);
        assert_eq!(m.rules()[0].matcher().pattern, "a long pattern");
    }

    #[test]
    fn check_rule_and_fired_alone() {
        let m = Matcher::new(&[rule("orc")]);
        assert!(m.check_rule(0, "an orc"));
        assert!(!m.check_rule(9, "an orc"));
        assert!(m.fired_alone("an orc"));
        let m = Matcher::new(&[rule("orc"), rule("an")]);
        assert!(!m.fired_alone("an orc"));
    }

    #[test]
    fn streams_and_timers() {
        let mut r = rule("orc");
        r.stream = Some("thoughts".into());
        let m = Matcher::new(&[r]);
        assert!(m.has_stream("thoughts"));
        assert!(!m.has_stream("combat"));
        assert_eq!(Matcher::timer_default(), 1);
    }

    #[test]
    fn respond_prefers_the_substitute() {
        let mut r = rule("orc");
        r.sound = Some("orc.wav".into());
        let m = Matcher::new(&[r.clone()]);
        assert_eq!(m.respond("an orc")[0].action, "orc.wav");
        r.substitute = Some("goblin".into());
        let m = Matcher::new(&[r]);
        assert_eq!(m.respond("an orc")[0].action, "goblin");
    }

    #[test]
    fn caught_is_the_span() {
        let f = caught("an orc arrives", &rule("orc")).unwrap();
        assert_eq!(f.start, 3);
        assert_eq!(f.end, 6);
        assert!(caught("nothing", &rule("orc")).is_none());
    }
}
