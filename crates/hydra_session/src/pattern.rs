//! Regular expressions.
//!
//! The original linked the `regex` crate: `hydra_session::stream_rules` calls
//! `regex_automata::meta::regex::Regex::search_half` (`0x101a061e0`) and
//! `regex::escape` (`0x101a02398`, `0x101a03f80`), and the player log's search
//! takes either a literal or a regex.  **Phase 1** reconnected the real crate:
//! [`Pattern`] wraps a `regex::Regex`, so the syntax the rules are written in
//! (`\w`, `\d`, `\s`, classes, `* + ?`, `^ $`, alternation, groups — and now
//! everything else `regex` understands: `\b`, `[[:alpha:]]`, `{2,3}`, `(?i)`,
//! lazy repetitions, `(?:…)`, `(?P<name>…)`) is the crate's own, and a broken
//! pattern is reported with the crate's error message instead of a hand-written
//! one.
//!
//! Two behaviours are deliberately *kept* from the reconstruction:
//!
//! * [`Pattern::literal`] is a substring test — it compiles `regex::escape` of
//!   the text, which is exactly `str::contains` for the same input;
//! * [`escape`] is `regex::escape`, the function the binary calls.
//!
//! The three broken-pattern behaviours the crate tests are the `regex` crate's:
//! an unclosed group (`^(a`), an unclosed class (`[abc`) and a repetition with
//! nothing to repeat (`*a`) are all `Err`, and each error carries the span and
//! the reason.

use regex::Regex;
use std::fmt;
use std::sync::Arc;

/// A compiled pattern.
///
/// Cloning is cheap: the compiled `Regex` is shared through an `Arc`, which is
/// what the rule set does when it hands a compiled pattern to `Compiled`.
#[derive(Clone)]
pub struct Pattern {
    /// the compiled expression
    inner: Arc<Regex>,
    /// the original source, kept for display and for `Debug`
    source: String,
    /// literal patterns are `regex::escape`d text (`Pattern::literal`)
    literal: bool,
}

impl Pattern {
    /// A pattern that matches `text` literally (no metacharacters at all).
    ///
    /// **ESTABLISHED** shape: the reader's literal search is a substring test,
    /// so the literal is escaped and compiled like any other pattern.
    pub fn literal(text: &str) -> Self {
        let inner = Regex::new(&regex::escape(text))
            .expect("`regex::escape` always produces a valid expression");
        Pattern {
            inner: Arc::new(inner),
            source: text.to_string(),
            literal: true,
        }
    }

    /// Compile a regular expression with the `regex` crate.
    ///
    /// **ESTABLISHED** (`0x101a02398`, `0x101a03f80`): the original compiled
    /// with `regex` and used `regex::escape` to build anchored patterns.
    pub fn regex(src: &str) -> Result<Self, String> {
        let inner = Regex::new(src).map_err(|e| e.to_string())?;
        Ok(Pattern {
            inner: Arc::new(inner),
            source: src.to_string(),
            literal: false,
        })
    }

    /// The source text this pattern was built from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Is this a plain literal (no regex syntax)?
    pub fn is_literal(&self) -> bool {
        self.literal
    }

    /// Does `text` contain a match?
    ///
    /// **ESTABLISHED** (`0x101a061e0`): `search_half` on the compiled
    /// expression, i.e. an unanchored search.
    pub fn is_match(&self, text: &str) -> bool {
        self.inner.is_match(text)
    }

    /// The compiled expression itself, for callers that need captures.
    pub fn compiled(&self) -> &Regex {
        &self.inner
    }
}

impl PartialEq for Pattern {
    /// Two patterns are the same when they were built from the same source the
    /// same way; the compiled `Regex` is not comparable.
    fn eq(&self, other: &Self) -> bool {
        self.literal == other.literal && self.source == other.source
    }
}

impl Eq for Pattern {}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pattern")
            .field("source", &self.source)
            .field("literal", &self.literal)
            .finish()
    }
}

/// `regex::escape`: put a backslash before every character that means
/// something to the regex engine.
pub fn escape(text: &str) -> String {
    regex::escape(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_is_a_substring_test() {
        let p = Pattern::literal("a cart");
        assert!(p.is_match("You see a cart here."));
        assert!(!p.is_match("You see a barrel here."));
        assert!(p.is_literal());
    }

    #[test]
    fn anchored_caret_plus_literal() {
        let p = Pattern::regex(&format!("^{}", escape("Aelith"))).unwrap();
        assert!(p.is_match("Aelith says, \"hello\""));
        assert!(!p.is_match("X Aelith says"));
    }

    #[test]
    fn dot_star_and_classes() {
        let p = Pattern::regex(r"^[A-Z][a-z]+ attacks").unwrap();
        assert!(p.is_match("Kobold attacks you!"));
        assert!(!p.is_match("kobold attacks you!"));
        let q = Pattern::regex(r"^a.c$").unwrap();
        assert!(q.is_match("abc"));
        assert!(!q.is_match("abbc"));
    }

    #[test]
    fn alternation_and_groups() {
        let p = Pattern::regex(r"^(kobold|orc) (attacks|moves)").unwrap();
        assert!(p.is_match("orc attacks"));
        assert!(p.is_match("kobold moves"));
        assert!(!p.is_match("kobold sleeps"));
    }

    #[test]
    fn optional_and_repetition() {
        let p = Pattern::regex(r"^ab*c$").unwrap();
        assert!(p.is_match("ac"));
        assert!(p.is_match("abbbc"));
        let q = Pattern::regex(r"^ab+c$").unwrap();
        assert!(!q.is_match("ac"));
        let r = Pattern::regex(r"^colou?r$").unwrap();
        assert!(r.is_match("color"));
        assert!(r.is_match("colour"));
    }

    #[test]
    fn escape_covers_the_metacharacters() {
        assert_eq!(escape("a.b*c"), r"a\.b\*c");
        assert_eq!(escape("plain words"), "plain words");
        assert!(Pattern::regex(&format!("^{}$", escape("(x)")))
            .unwrap()
            .is_match("(x)"));
    }

    #[test]
    fn broken_patterns_are_reported() {
        assert!(Pattern::regex("^(a").is_err());
        assert!(Pattern::regex("[abc").is_err());
        assert!(Pattern::regex("*a").is_err());
    }

    #[test]
    fn broken_patterns_carry_the_regex_crate_s_message() {
        // The hand engine reported `unclosed group` / `nothing to repeat` of its
        // own; the crate's error carries the span and the reason.
        let e = Pattern::regex("^(a").unwrap_err();
        assert!(e.contains("unclosed group"), "{e}");
        let e = Pattern::regex("[abc").unwrap_err();
        assert!(e.contains("unclosed character class"), "{e}");
        let e = Pattern::regex("*a").unwrap_err();
        assert!(e.contains("repetition operator missing expression"), "{e}");
        // An empty alternation in a repetition is refused by the crate.
        assert!(Pattern::regex("(?:a|)+").is_ok());
        assert!(Pattern::regex("(?P<1>x)").is_err(), "bad capture name");
    }

    #[test]
    fn the_syntax_the_hand_engine_lacked_now_works() {
        // word boundaries and explicit repetition counts
        let p = Pattern::regex(r"\bAelith\b").unwrap();
        assert!(p.is_match("then Aelith says"));
        assert!(!p.is_match("Aeliths says"));
        assert!(Pattern::regex(r"^\d{2,3} coins").unwrap().is_match("123 coins"));
        assert!(!Pattern::regex(r"^\d{2,3} coins")
            .unwrap()
            .is_match("1 coins"));
        // POSIX classes, which the hand engine explicitly did not support
        assert!(Pattern::regex("^[[:alpha:]]+ says")
            .unwrap()
            .is_match("Aelith says"));
        // inline flags
        assert!(Pattern::regex("(?i)^kobold").unwrap().is_match("KOBOLD attacks"));
        // lazy quantifiers are real now: the match still succeeds, but the
        // shortest form wins when a capture is read
        let lazy = Pattern::regex(r"^a.*?c").unwrap();
        assert!(lazy.is_match("abcbc"));
        let caps = Pattern::regex(r"^(\w+) attacks (\w+)").unwrap();
        let got = caps.compiled().captures("Kobold attacks you").unwrap();
        assert_eq!(&got[1], "Kobold");
        assert_eq!(&got[2], "you");
        // named groups, which the crate exposes through `compiled()`
        let named = Pattern::regex(r"^(?P<who>\w+) says").unwrap();
        let caps = named.compiled().captures("Aelith says hi").unwrap();
        assert_eq!(&caps["who"], "Aelith");
    }

    #[test]
    fn literal_patterns_are_escaped_not_interpreted() {
        // A literal never means syntax, and an empty literal matches anything —
        // both were true of the hand engine and stay true.
        let p = Pattern::literal("a.c");
        assert!(p.is_match("see a.c here"));
        assert!(!p.is_match("see abc here"));
        assert!(Pattern::literal("").is_match("anything"));
        assert_eq!(Pattern::literal("a.c").source(), "a.c");
        // `PartialEq` compares the source and the kind, not the compiled form.
        assert_eq!(Pattern::literal("a.c"), Pattern::literal("a.c"));
        assert_ne!(Pattern::literal("a.c"), Pattern::regex("a.c").unwrap());
    }
}
