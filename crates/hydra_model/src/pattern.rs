//! Pattern compilation.
//!
//! # Departure from the original
//!
//! The original crate used the `regex` crate: `ascii` drove
//! `regex_syntax::ast::parse::Parser` and a `Visitor` implementation
//! (`pattern::Edits`, symbols `<hydra_model::pattern::Edits as
//! regex_syntax::ast::visitor::Visitor>::visit_pre` /
//! `::visit_class_set_item_pre`) to rewrite a pattern into its ASCII-only
//! equivalent, and `compile` / `compile_gated` then called
//! `regex::Regex::new` / `RegexBuilder::dfa_size_limit().build()`.
//!
//! This build has no network, so `regex` is not available. Instead this module
//! ships a small backtracking matcher for the subset of the syntax that the
//! *embedded* tables actually use — recovered from the 32204-byte crit table
//! at `0x103a3ff08` (see `crit/data/crit.tsv`): literals, `^`, `$`, `.`,
//! `*`, `+`, `?` and lazy variants, `\`-escapes, character classes with
//! ranges, `(?:...)`/`(...)` grouping, `|` alternation and a leading `(?i)`.
//! Negative/positive look-ahead (`(?!...)`, `(?=...)`) is supported as a
//! zero-width assertion: the embedded table prefixes some patterns with
//! `(?!.*removes skull.)`. Constructs outside the subset (`\p{...}`, `{n,m}`
//! repetitions) make [`compile`] return a [`Pattern`] with `error` set, and
//! such a pattern never matches: the safe behaviour when the engine cannot
//! reproduce the original semantics.
//!
//! `// UNKNOWN:` the exact set of edits `ascii` performs. The disassembly shows
//! an AST rewrite pass plus a re-sort and a re-compile, and that the result is
//! still a regex, so the implementation rewrites the wide Unicode shorthands
//! (`\d \w \s` and their negations) to explicit ASCII classes and drops a
//! leading `(?u)`.

/// A compiled pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    /// The (post-[`ascii`]) pattern source.
    pub source: String,
    /// `true` when the pattern was compiled with the `i` flag.
    pub case_insensitive: bool,
    /// `Some(reason)` when the engine cannot handle the source.
    pub error: Option<String>,
    root: Node,
}

impl Default for Pattern {
    fn default() -> Self {
        Pattern {
            source: String::new(),
            case_insensitive: false,
            error: None,
            root: Node::Seq(Vec::new()),
        }
    }
}

impl Pattern {
    /// Does this pattern match anywhere in `hay`?
    pub fn is_match(&self, hay: &str) -> bool {
        if self.error.is_some() {
            return false;
        }
        let text: Vec<char> = if self.case_insensitive {
            hay.chars().flat_map(|c| c.to_lowercase()).collect()
        } else {
            hay.chars().collect()
        };
        for start in 0..=text.len() {
            if !self.root.ends(&text, start).is_empty() {
                return true;
            }
        }
        false
    }

    /// True when [`compile`] produced a usable matcher.
    pub fn is_ok(&self) -> bool {
        self.error.is_none()
    }
}

/// The pattern syntax tree. Matching is set-based (`ends` returns every
/// position a node can end at), which handles groups and alternation without a
/// continuation machine.
#[derive(Clone, Debug, PartialEq)]
enum Node {
    /// Start of the haystack.
    Start,
    /// End of the haystack.
    End,
    Lit(char),
    Any,
    /// `(charset, negated)`.
    Class(String, bool),
    Seq(Vec<Node>),
    Alt(Vec<Node>),
    /// Zero-width negative (`(?!`) / positive (`(?=`) look-ahead.
    Look {
        node: Box<Node>,
        negative: bool,
    },
    Repeat {
        node: Box<Node>,
        min: usize,
        max: usize,
        /// `// UNKNOWN:` the engine treats lazy and greedy identically; the
        /// match *set* is the same, only the reported span differs and no
        /// caller in this crate inspects spans.
        lazy: bool,
    },
}

fn push_unique(out: &mut Vec<usize>, v: impl IntoIterator<Item = usize>) {
    for x in v {
        if !out.contains(&x) {
            out.push(x);
        }
    }
}

impl Node {
    /// Every position `self` can end at when it starts at `pos`.
    fn ends(&self, text: &[char], pos: usize) -> Vec<usize> {
        match self {
            Node::Start => {
                if pos == 0 {
                    vec![0]
                } else {
                    Vec::new()
                }
            }
            Node::End => {
                if pos == text.len() {
                    vec![pos]
                } else {
                    Vec::new()
                }
            }
            Node::Lit(c) => {
                if text.get(pos) == Some(c) {
                    vec![pos + 1]
                } else {
                    Vec::new()
                }
            }
            Node::Any => {
                if pos < text.len() {
                    vec![pos + 1]
                } else {
                    Vec::new()
                }
            }
            Node::Class(set, negated) => match text.get(pos) {
                Some(c) if *negated != set.contains(*c) => vec![pos + 1],
                _ => Vec::new(),
            },
            Node::Seq(nodes) => {
                let mut positions = vec![pos];
                for node in nodes {
                    let mut next = Vec::new();
                    for p in &positions {
                        push_unique(&mut next, node.ends(text, *p));
                    }
                    if next.is_empty() {
                        return Vec::new();
                    }
                    positions = next;
                }
                positions
            }
            Node::Look { node, negative } => {
                let matched = !node.ends(text, pos).is_empty();
                if matched == *negative {
                    Vec::new()
                } else {
                    vec![pos]
                }
            }
            Node::Alt(branches) => {
                let mut out = Vec::new();
                for branch in branches {
                    push_unique(&mut out, branch.ends(text, pos));
                }
                out
            }
            Node::Repeat {
                node,
                min,
                max,
                lazy: _,
            } => {
                let mut out = Vec::new();
                if *min == 0 {
                    out.push(pos);
                }
                let mut frontier = vec![pos];
                let mut count = 0usize;
                while count < *max && !frontier.is_empty() {
                    count += 1;
                    let mut next = Vec::new();
                    for p in &frontier {
                        push_unique(&mut next, node.ends(text, *p));
                    }
                    if next.is_empty() {
                        break;
                    }
                    if count >= *min {
                        push_unique(&mut out, next.iter().copied());
                    }
                    frontier = next;
                }
                out
            }
        }
    }
}

/// Rewrite a pattern into its ASCII-only equivalent.
///
/// Recovered structure: the original parsed the pattern into
/// `regex_syntax::ast::Ast`, visited it with a `Visitor` that collected
/// *edits* (a start/end byte range plus replacement text), sorted them by
/// offset (`driftsort_main`) and spliced the replacements into the source.
/// The observable effect on the embedded crit table is nil (its patterns are
/// already ASCII); the rewrite matters for `\d`/`\w`/`\s`, which in the
/// original matched Unicode. Those are rewritten here to ASCII classes.
pub fn ascii(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 8);
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // `// UNKNOWN:` the exact replacement text; ASCII ranges are the
            // smallest set that keeps the original ASCII intent.
            Some('d') => out.push_str("[0-9]"),
            Some('D') => out.push_str("[^0-9]"),
            Some('w') => out.push_str("[0-9A-Za-z_]"),
            Some('W') => out.push_str("[^0-9A-Za-z_]"),
            Some('s') => out.push_str("[ \\t\\n\\r\\x0B\\x0C]"),
            Some('S') => out.push_str("[^ \\t\\n\\r\\x0B\\x0C]"),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    // A leading `(?u)` enables Unicode semantics; drop it after the rewrite.
    out.strip_prefix("(?u)").map(str::to_string).unwrap_or(out)
}

/// Compile a pattern.
///
/// Returns a [`Pattern`] whose `error` is set when the source uses syntax this
/// engine does not implement; such a pattern never matches.
pub fn compile(source: &str) -> Pattern {
    let mut rewritten = ascii(source);
    let mut case_insensitive = false;
    if let Some(rest) = rewritten.strip_prefix("(?i)") {
        case_insensitive = true;
        rewritten = rest.to_string();
    }
    let chars: Vec<char> = if case_insensitive {
        rewritten.chars().flat_map(|c| c.to_lowercase()).collect()
    } else {
        rewritten.chars().collect()
    };
    let mut pos = 0usize;
    let (root, error) = match parse_alt(&chars, &mut pos) {
        Ok(node) if pos == chars.len() => (node, None),
        Ok(_) => (Node::Seq(Vec::new()), Some("unconsumed trailing input".to_string())),
        Err(err) => (Node::Seq(Vec::new()), Some(err)),
    };
    Pattern {
        source: rewritten,
        case_insensitive,
        error,
        root,
    }
}

/// Compile a pattern, returning `None` when it cannot be compiled at all.
///
/// The original wrapped `RegexBuilder::dfa_size_limit(..)` + `build()` and
/// forwarded the error; `MatchIndex::build` treats `None` as "no gate".
pub fn compile_gated(source: &str) -> Option<Pattern> {
    let compiled = compile(source);
    if compiled.is_ok() {
        Some(compiled)
    } else {
        None
    }
}

fn parse_alt(chars: &[char], pos: &mut usize) -> Result<Node, String> {
    let mut branches = vec![parse_seq(chars, pos)?];
    while chars.get(*pos) == Some(&'|') {
        *pos += 1;
        branches.push(parse_seq(chars, pos)?);
    }
    if branches.len() == 1 {
        Ok(branches.pop().expect("one branch"))
    } else {
        Ok(Node::Alt(branches))
    }
}

fn parse_seq(chars: &[char], pos: &mut usize) -> Result<Node, String> {
    let mut nodes: Vec<Node> = Vec::new();
    while let Some(&c) = chars.get(*pos) {
        match c {
            ')' | '|' => break,
            '^' => {
                nodes.push(Node::Start);
                *pos += 1;
            }
            '$' => {
                nodes.push(Node::End);
                *pos += 1;
            }
            '.' => {
                nodes.push(Node::Any);
                *pos += 1;
            }
            '(' => {
                let mut skip = 1;
                let mut look: Option<bool> = None;
                if chars.get(*pos + 1) == Some(&'?') {
                    match chars.get(*pos + 2) {
                        Some(':') => skip = 3,
                        Some('!') => {
                            look = Some(true);
                            skip = 3;
                        }
                        Some('=') => {
                            look = Some(false);
                            skip = 3;
                        }
                        _ => return Err(format!("unsupported group at {}", *pos)),
                    }
                }
                *pos += skip;
                let inner = parse_alt(chars, pos)?;
                if chars.get(*pos) != Some(&')') {
                    return Err("unbalanced '('".to_string());
                }
                *pos += 1;
                match look {
                    Some(negative) => nodes.push(Node::Look {
                        node: Box::new(inner),
                        negative,
                    }),
                    None => nodes.push(inner),
                }
            }
            '*' | '+' | '?' => {
                let (min, max) = match c {
                    '*' => (0, usize::MAX),
                    '+' => (1, usize::MAX),
                    _ => (0, 1),
                };
                let Some(last) = nodes.pop() else {
                    return Err(format!("nothing to repeat at {}", *pos));
                };
                let lazy = chars.get(*pos + 1) == Some(&'?');
                nodes.push(Node::Repeat {
                    node: Box::new(last),
                    min,
                    max,
                    lazy,
                });
                *pos += if lazy { 2 } else { 1 };
            }
            '[' => {
                let mut j = *pos + 1;
                let mut negated = false;
                if chars.get(j) == Some(&'^') {
                    negated = true;
                    j += 1;
                }
                let mut set = String::new();
                let mut first = true;
                while j < chars.len() && chars[j] != ']' {
                    let mut ch = chars[j];
                    if ch == '\\' {
                        j += 1;
                        let Some(esc) = chars.get(j) else {
                            return Err("trailing backslash in class".to_string());
                        };
                        ch = match esc {
                            'n' => '\n',
                            't' => '\t',
                            'r' => '\r',
                            other => *other,
                        };
                    }
                    // `a-z` range?
                    if chars.get(j + 1) == Some(&'-')
                        && chars.get(j + 2).is_some()
                        && chars[j + 2] != ']'
                    {
                        let hi = chars[j + 2];
                        let mut lo = ch as u32;
                        while lo <= hi as u32 {
                            if let Some(c) = char::from_u32(lo) {
                                set.push(c);
                            }
                            lo += 1;
                        }
                        j += 3;
                        first = false;
                        continue;
                    }
                    set.push(ch);
                    j += 1;
                    first = false;
                }
                let _ = first;
                if j >= chars.len() {
                    return Err(format!("unterminated class at {}", *pos));
                }
                nodes.push(Node::Class(set, negated));
                *pos = j + 1;
            }
            '\\' => {
                let Some(esc) = chars.get(*pos + 1) else {
                    return Err("trailing backslash".to_string());
                };
                let lit = match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => *other,
                };
                nodes.push(Node::Lit(lit));
                *pos += 2;
            }
            '{' => return Err("repetition counts are not supported".to_string()),
            other => {
                nodes.push(Node::Lit(other));
                *pos += 1;
            }
        }
    }
    Ok(Node::Seq(nodes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_rewrites_unicode_shorthands() {
        assert_eq!(ascii(r"^\d+\s"), r"^[0-9]+[ \t\n\r\x0B\x0C]");
        assert_eq!(ascii(r"\w"), "[0-9A-Za-z_]");
        // an escaped literal paren survives untouched
        assert_eq!(
            ascii(r"^Burn exposes the spine \(from the front\)"),
            r"^Burn exposes the spine \(from the front\)"
        );
        assert_eq!(ascii("(?u)abc"), "abc");
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
    fn negative_lookahead_gate() {
        // exactly the prefix used by the embedded crit table
        let p = compile("^(?!.*removes skull.)Blow to head.");
        assert!(p.is_ok(), "{:?}", p.error);
        assert!(p.is_match("Blow to head."));
        assert!(!p.is_match("The blow removes skull."));
        let positive = compile("^(?=.*head)Blow to");
        assert!(positive.is_ok());
        assert!(positive.is_match("Blow to head"));
        assert!(!positive.is_match("Blow to leg"));
    }

    #[test]
    fn unsupported_syntax_reports_error_and_never_matches() {
        let p = compile(r"^\p{L}+$");
        assert!(!p.is_ok());
        assert!(!p.is_match("anything"));
        assert!(compile_gated(r"^\p{L}+$").is_none());
        assert!(compile_gated("^plain").is_some());
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
    }
}
