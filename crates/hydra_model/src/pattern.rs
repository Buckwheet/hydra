//! Pattern compilation over the real `regex` crate.
//!
//! Recovered shape (arm64):
//!
//! * `ascii` (`0x101b89924`, 1972 B) parses the source with
//!   `regex_syntax::ast::parse::Parser`, visits the AST with
//!   `pattern::Edits` (a `regex_syntax::ast::visitor::Visitor`) collecting
//!   `(start, end, replacement)` edits, sorts them with `driftsort_main` and
//!   splices them into a copy of the source with `String::replace_range`. The
//!   two visitor entry points are
//!   `<Edits as Visitor>::visit_pre` (`0x101b8cc64`) and
//!   `::visit_class_set_item_pre` (`0x101b8cb40`); between them they emit
//!   exactly two kinds of edit (see [`ascii`]).
//! * `compile` (`0x101b8a0d8`, 124 B) is `ascii` + `regex::Regex::new`
//!   (`0x101d8c61c`) — no builder, so no size limit.
//! * `compile_gated` (`0x101b89800`, 292 B) is `ascii` +
//!   `RegexBuilder::new` (`0x101d8655c`) + `RegexBuilder::dfa_size_limit(0)`
//!   (`x1 == 0` at `0x101b89830`, and the crate documents `0` as *no* limit) +
//!   `build`, returning `Option`-shaped data (`None` when the build fails).
//!
//! This build therefore uses `regex` directly. `regex_syntax` is *not* a
//! workspace pin, so [`ascii`] performs the same two edits at the source level
//! instead of on a `regex_syntax` AST; see the note on that function.

use regex::{Captures, Regex, RegexBuilder};

/// A compiled pattern.
///
/// `source` is the post-[`ascii`] text handed to `regex`; `error` holds the
/// compiler's message when the source does not compile, and such a pattern
/// never matches (the safe behaviour when the engine cannot reproduce the
/// original semantics).
#[derive(Clone, Debug)]
pub struct Pattern {
    /// The (post-[`ascii`]) pattern source.
    pub source: String,
    /// `true` when the pattern was compiled with the `i` flag.
    pub case_insensitive: bool,
    /// `Some(reason)` when the source does not compile.
    pub error: Option<String>,
    regex: Option<Regex>,
}

impl PartialEq for Pattern {
    /// Two patterns are equal when they were compiled from the same source with
    /// the same flags. (`regex::Regex` itself is not comparable.)
    fn eq(&self, other: &Pattern) -> bool {
        self.source == other.source
            && self.case_insensitive == other.case_insensitive
            && self.error == other.error
    }
}

impl Default for Pattern {
    fn default() -> Pattern {
        compile("")
    }
}

impl Pattern {
    /// Does this pattern match anywhere in `hay`?
    pub fn is_match(&self, hay: &str) -> bool {
        self.regex.as_ref().is_some_and(|regex| regex.is_match(hay))
    }

    /// True when [`compile`] produced a usable matcher.
    pub fn is_ok(&self) -> bool {
        self.error.is_none()
    }

    /// The first match's capture set, when the pattern matched.
    ///
    /// This is the `regex_automata::util::captures::Captures` the recovered
    /// classifiers walk with `get_group_by_name`.
    pub fn captures<'h>(&self, hay: &'h str) -> Option<Captures<'h>> {
        self.regex.as_ref()?.captures(hay)
    }

    /// The value of the named group `name` in the first match.
    ///
    /// Returns `None` when the pattern did not match or when the group did not
    /// take part in the match (exactly `Captures::get_group_by_name(name)`).
    pub fn capture<'h>(&self, hay: &'h str, name: &str) -> Option<&'h str> {
        self.captures(hay)?.name(name).map(|m| m.as_str())
    }

    /// Every named group this pattern declares, in declaration order.
    pub fn group_names(&self) -> Vec<&str> {
        match &self.regex {
            Some(regex) => regex.capture_names().flatten().collect(),
            None => Vec::new(),
        }
    }

    /// Every named group that took part in the first match, with its text.
    pub fn groups(&self, hay: &str) -> Option<Groups> {
        let regex = self.regex.as_ref()?;
        let captures = regex.captures(hay)?;
        Some(Groups {
            values: regex
                .capture_names()
                .flatten()
                .filter_map(|name| {
                    captures
                        .name(name)
                        .map(|matched| (name.to_string(), matched.as_str().to_string()))
                })
                .collect(),
        })
    }
}

/// The named groups of one match — the `Captures::get_group_by_name` results
/// the recovered classifiers read out of a hit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    values: Vec<(String, String)>,
}

impl Groups {
    /// The value of one group, when it took part in the match.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(group, _)| group == name)
            .map(|(_, value)| value.as_str())
    }

    /// Did `name` take part in the match? (The recovered `bool` fields are
    /// group *presence*.)
    pub fn has(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// The same as [`Groups::get`], owned.
    pub fn text(&self, name: &str) -> Option<String> {
        self.get(name).map(str::to_string)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// `(name, value)` pairs in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values
            .iter()
            .map(|(group, value)| (group.as_str(), value.as_str()))
    }

    /// The group names that took part in the match.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values.iter().map(|(group, _)| group.as_str())
    }
}

/// Rewrite a pattern into its ASCII-only equivalent.
///
/// The two edits `Edits` emits, recovered instruction by instruction:
///
/// 1. **Perl classes** (`visit_pre` / `visit_class_set_item_pre`, node tag `6`):
///    the span is replaced by `format!("[{}]", class)` — or
///    `format!("[^{}]", class)` when the class is negated — where the class
///    text comes from two static tables:
///
///    | `ClassPerlKind` | class text |
///    |---|---|
///    | `Digit` (0) | `0-9` |
///    | `Space` (1) | `\t\n\x0B\x0C\r ` (15 bytes: tab, LF, VT, FF, CR, space) |
///    | `Word` (2) | `0-9A-Za-z_` |
///
///    So `\d` becomes `[0-9]`, `\W` becomes `[^0-9A-Za-z_]`, and a Perl class
///    *inside* a bracketed class is rewritten the same way (nested classes are
///    legal syntax). Both visitor entry points emit the same edit.
/// 2. **The word boundary assertion** (`visit_pre` node tag `4`, kind word `4`
///    = `AssertionKind::WordBoundary`): the span is replaced by the 8-byte
///    immediate `(?-u:\b)` (`movz #0x3f28` / `movk #0x752d` / `movk #0x5c3a` /
///    `movk #0x2962`), i.e. `\b` → `(?-u:\b)`. `\B` is *not* rewritten (the
///    kind test is `== 4`).
///
/// Nothing else is rewritten: in particular a `(?u)` flag group is left alone
/// (there is no `Ast::Flags` branch), which is why this function no longer
/// strips one.
///
/// The edits are applied from the highest offset down, which is what the
/// recovered `replace_range` loop does after its `driftsort`.
pub fn ascii(source: &str) -> String {
    // The `ClassPerlKind` table, in discriminant order.
    const CLASSES: [&str; 3] = ["0-9", r"\t\n\x0B\x0C\r ", "0-9A-Za-z_"];

    fn class(kind: char) -> Option<(&'static str, bool)> {
        Some(match kind {
            'd' => (CLASSES[0], false),
            's' => (CLASSES[1], false),
            'w' => (CLASSES[2], false),
            'D' => (CLASSES[0], true),
            'S' => (CLASSES[1], true),
            'W' => (CLASSES[2], true),
            _ => return None,
        })
    }

    let mut out = String::with_capacity(source.len() + 16);
    // Bracket depth, so a `\b` inside a class (which is not an assertion) is
    // left alone: the AST visitor only ever sees assertion nodes.
    let mut depth = 0usize;
    let mut chars = source.chars();
    while let Some(c) = chars.next() {
        match c {
            '[' => {
                depth += 1;
                out.push(c);
            }
            ']' => {
                depth = depth.saturating_sub(1);
                out.push(c);
            }
            '\\' => match chars.next() {
                Some(esc) => match class(esc) {
                    Some((class, negated)) => {
                        out.push('[');
                        if negated {
                            out.push('^');
                        }
                        out.push_str(class);
                        out.push(']');
                    }
                    None if esc == 'b' && depth == 0 => out.push_str(r"(?-u:\b)"),
                    None => {
                        out.push('\\');
                        out.push(esc);
                    }
                },
                None => out.push('\\'),
            },
            other => out.push(other),
        }
    }
    out
}

/// Compile a pattern (`regex::Regex::new` over [`ascii`]'s output).
///
/// Returns a [`Pattern`] whose `error` is set when the source does not compile;
/// such a pattern never matches.
pub fn compile(source: &str) -> Pattern {
    let rewritten = ascii(source);
    let case_insensitive = rewritten.starts_with("(?i)");
    match Regex::new(&rewritten) {
        Ok(regex) => Pattern {
            source: rewritten,
            case_insensitive,
            error: None,
            regex: Some(regex),
        },
        Err(error) => Pattern {
            source: rewritten,
            case_insensitive,
            error: Some(error.to_string()),
            regex: None,
        },
    }
}

/// Compile a pattern, returning `None` when it cannot be compiled at all.
///
/// The original wrapped `RegexBuilder::dfa_size_limit(0).build()` (a limit of
/// zero means "no limit" to `regex`) and forwarded the error; `MatchIndex`
/// treats `None` as "no gate".
pub fn compile_gated(source: &str) -> Option<Pattern> {
    let rewritten = ascii(source);
    let case_insensitive = rewritten.starts_with("(?i)");
    match RegexBuilder::new(&rewritten).dfa_size_limit(0).build() {
        Ok(regex) => Some(Pattern {
            source: rewritten,
            case_insensitive,
            error: None,
            regex: Some(regex),
        }),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_rewrites_perl_classes() {
        // the recovered class texts, byte for byte
        assert_eq!(ascii(r"^\d+\s"), r"^[0-9]+[\t\n\x0B\x0C\r ]");
        assert_eq!(ascii(r"\w"), "[0-9A-Za-z_]");
        assert_eq!(ascii(r"\D\S\W"), r"[^0-9][^\t\n\x0B\x0C\r ][^0-9A-Za-z_]");
        // inside a bracketed class the same replacement is used (nested class)
        assert_eq!(ascii(r"[\d]+"), "[[0-9]]+");
        // ... and `regex` accepts the nested class it produces
        let p = compile(r"^[\d]+$");
        assert!(p.is_ok(), "{:?}", p.error);
        assert!(p.is_match("42"));
    }

    #[test]
    fn ascii_rewrites_the_word_boundary_only() {
        assert_eq!(ascii(r"\bAS\b"), r"(?-u:\b)AS(?-u:\b)");
        // `\B` is not rewritten (the recovered kind test is `== 4`)
        assert_eq!(ascii(r"\B"), r"\B");
        // ... and neither is a `\b` inside a character class (not an assertion)
        assert_eq!(ascii(r"[\b]"), r"[\b]");
    }

    #[test]
    fn ascii_leaves_everything_else_alone() {
        // an escaped literal paren survives untouched
        assert_eq!(
            ascii(r"^Burn exposes the spine \(from the front\)"),
            r"^Burn exposes the spine \(from the front\)"
        );
        // a `(?u)` flag group is *not* stripped by the recovered edits
        assert_eq!(ascii("(?u)abc"), "(?u)abc");
        assert_eq!(ascii("^plain [a-z]{2}"), "^plain [a-z]{2}");
    }

    #[test]
    fn literal_prefix_patterns_match() {
        let p = compile("^Acid burn on the right ear.");
        assert!(p.is_ok());
        assert!(p.is_match("Acid burn on the right ear."));
        assert!(!p.is_match("Acid burn on the left ear."));
        // anchored
        assert!(!p.is_match("The Acid burn on the right ear."));
    }

    #[test]
    fn anchors_and_wildcards() {
        let p = compile("^Too bad .*? ducked.");
        assert!(p.is_match("Too bad you ducked."));
        assert!(p.is_match("Too bad it ducked."));
        assert!(!p.is_match("Too bad you ran."));
        let p = compile(".*removes skull.");
        assert!(p.is_match("The blast removes skull."));
        assert!(!p.is_match("nothing here"));
    }

    #[test]
    fn lazy_repeat_and_groups() {
        let p = compile("^A freezing and accurate strike renders .*? right leg a (?:fond )?memory.");
        assert!(p.is_match("A freezing and accurate strike renders your right leg a memory."));
        assert!(p.is_match("A freezing and accurate strike renders your right leg a fond memory."));
        assert!(!p.is_match("A freezing strike renders your right leg a memory."));
    }

    #[test]
    fn escaped_parens_are_literal() {
        let p = compile(r"^Burn exposes the spine \(from the front\).");
        assert!(p.is_match("Burn exposes the spine (from the front)."));
        assert!(!p.is_match("Burn exposes the spine ."));
    }

    #[test]
    fn word_boundaries_and_unicode_classes_now_compile() {
        // `\b` used to be a literal `b` in the std-only engine; the recovered
        // `ascii` rewrites it to the ASCII-only assertion
        let p = compile(r"\bAS:\s*(\d+)");
        assert!(p.is_ok(), "{:?}", p.error);
        assert!(p.is_match("AS: 431"));
        assert!(!p.is_match("xAS: 431"));
        // `\p{..}` is real Unicode-class syntax to `regex`
        let p = compile(r"^\p{L}+$");
        assert!(p.is_ok(), "{:?}", p.error);
        assert!(p.is_match("Kithar"));
    }

    #[test]
    fn invalid_syntax_reports_error_and_never_matches() {
        let p = compile("^(unclosed");
        assert!(!p.is_ok());
        assert!(!p.is_match("(unclosed"));
        assert!(compile_gated("^(unclosed").is_none());
        assert!(compile_gated("^plain").is_some());
        // look-around is not part of `regex`'s syntax; the crit table's one
        // look-ahead row is compiled by `MatchIndex` instead (see that module)
        let p = compile("^(?!.*removes skull.)Blow to head.");
        assert!(!p.is_ok());
        assert!(!p.is_match("Blow to head."));
    }

    #[test]
    fn character_classes_and_ranges() {
        let p = compile("^[Rr]ank [0-9]$");
        assert!(p.is_ok());
        assert!(p.is_match("Rank 3"));
        assert!(p.is_match("rank 9"));
        assert!(!p.is_match("Rank 33"));
        let p = compile(r"^\d+ points?$");
        assert!(p.is_match("12 points"));
        assert!(p.is_match("7 point"));
        assert!(!p.is_match("7 1"));
    }

    #[test]
    fn alternation() {
        let p = compile("^(?:Left|Shield) arm explodes at the shoulder.");
        assert!(p.is_ok());
        assert!(p.is_match("Left arm explodes at the shoulder."));
        assert!(p.is_match("Shield arm explodes at the shoulder."));
        assert!(!p.is_match("Right arm explodes at the shoulder."));
    }

    #[test]
    fn case_insensitive_flag() {
        let p = compile("(?i)^ACID BURN");
        assert!(p.case_insensitive);
        assert!(p.is_match("acid burn on the ear"));
        assert!(p.is_match("Acid Burn"));
        assert!(!compile("^ACID BURN").case_insensitive);
    }

    #[test]
    fn named_captures_are_exposed() {
        let p = compile(r"(?i)^(?P<attacker>.+?) swings at (?P<target>.+)!$");
        assert!(p.is_ok(), "{:?}", p.error);
        assert_eq!(p.group_names(), vec!["attacker", "target"]);
        assert_eq!(
            p.capture("The kobold swings at you!", "attacker"),
            Some("The kobold")
        );
        assert_eq!(p.capture("The kobold swings at you!", "target"), Some("you"));
        assert_eq!(p.capture("nothing here", "target"), None);
        // an optional group that did not take part is `None`
        let p = compile(r"^(?P<attack>You swing)(?: (?P<extra>hard))?$");
        assert_eq!(p.capture("You swing", "extra"), None);
        assert_eq!(p.capture("You swing hard", "extra"), Some("hard"));
        let groups = p.groups("You swing hard").expect("a match");
        assert_eq!(groups.get("attack"), Some("You swing"));
        assert_eq!(groups.get("extra"), Some("hard"));
        assert_eq!(groups.names().collect::<Vec<_>>(), vec!["attack", "extra"]);
        let groups = p.groups("You swing").expect("a match");
        assert!(!groups.has("extra"));
        assert!(groups.has("attack"));
    }
}
