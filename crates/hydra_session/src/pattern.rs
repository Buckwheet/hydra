//! A small regular-expression matcher.
//!
//! The original linked the `regex` crate: `hydra_session::stream_rules` calls
//! `regex_automata::meta::regex::Regex::search_half` (`0x101a061e0`) and
//! `regex::escape` (`0x101a02398`, `0x101a03f80`), and the player log's search
//! takes either a literal or a regex.  `hydra_session` is std-only here, so
//! this module implements the subset of `regex` syntax the rules use:
//!
//! * literals, and `\` escaping of any metacharacter (see [`escape`], which is
//!   `regex::escape`);
//! * `.` — any character except `\n`;
//! * `[abc]`, `[^abc]`, `[a-z]`, `[[:alpha:]]`-free POSIX classes are *not*
//!   supported;
//! * the class escapes \w, \d, \s and their negations;\n//! * the greedy postfix operators `*`, `+`, `?`, and the lazy forms `*?`, `+?`,
//!   `??` (lazy is accepted and treated as greedy-from-the-shortest);
//! * `^` and `$` anchors;
//! * alternation `|`, and non-capturing/unnamed groups `(...)`; captures are
//!   not exposed.
//!
//! Departures from `regex` are recorded in `NOTES.md`; the important one is
//! that matching is character-based and backtracks, so a pathological pattern
//! can be exponential (the recovered rules are short and anchored).

/// A compiled pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// alternatives, each a sequence of nodes
    alts: Vec<Vec<Node>>,
    /// the original source, kept for display and for `Debug`
    source: String,
    /// literal patterns skip the matcher entirely (`Pattern::literal`)
    literal: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Lit(char),
    Any,
    Class { ranges: Vec<(char, char)>, negated: bool },
    Start,
    End,
    Star(Box<Node>, bool),
    Plus(Box<Node>, bool),
    Opt(Box<Node>, bool),
    Group(Vec<Vec<Node>>),
}

impl Pattern {
    /// A pattern that matches `text` literally (no metacharacters at all).
    pub fn literal(text: &str) -> Self {
        Pattern {
            alts: vec![text.chars().map(Node::Lit).collect()],
            source: text.to_string(),
            literal: true,
        }
    }

    /// Compile a regular expression.
    pub fn regex(src: &str) -> Result<Self, String> {
        let chars: Vec<char> = src.chars().collect();
        let mut p = Comiler { c: &chars, i: 0 };
        let alts = p.alternatives()?;
        Ok(Pattern {
            alts,
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
    pub fn is_match(&self, text: &str) -> bool {
        if self.literal {
            // A literal pattern matches as a substring, like `str::contains`.
            return match self.alts.first() {
                Some(nodes) => {
                    let needle: String = nodes
                        .iter()
                        .map(|n| match n {
                            Node::Lit(c) => *c,
                            _ => '\u{0}',
                        })
                        .collect();
                    text.contains(&needle)
                }
                None => true,
            };
        }
        let chars: Vec<char> = text.chars().collect();
        // An unanchored regex is tried at every start offset.
        let anchored_start = self
            .alts
            .iter()
            .all(|alt| matches!(alt.first(), Some(Node::Start)));
        for start in 0..=chars.len() {
            for alt in &self.alts {
                if let Some(_end) = match_seq(alt, &chars, start) {
                    return true;
                }
            }
            if anchored_start {
                break;
            }
        }
        false
    }
}

struct Comiler<'a> {
    c: &'a [char],
    i: usize,
}

impl<'a> Comiler<'a> {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    fn alternatives(&mut self) -> Result<Vec<Vec<Node>>, String> {
        let mut alts = Vec::new();
        loop {
            alts.push(self.sequence()?);
            if self.peek() == Some('|') {
                self.i += 1;
                continue;
            }
            break;
        }
        if self.i < self.c.len() {
            return Err(format!(
                "unexpected `{}` at offset {}",
                self.c[self.i], self.i
            ));
        }
        Ok(alts)
    }

    fn sequence(&mut self) -> Result<Vec<Node>, String> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            let atom = self.atom()?;
            // postfix operators
            let mut node = atom;
            loop {
                match self.peek() {
                    Some('*') => {
                        self.i += 1;
                        let lazy = self.eat_lazy();
                        node = Node::Star(Box::new(node), lazy);
                    }
                    Some('+') => {
                        self.i += 1;
                        let lazy = self.eat_lazy();
                        node = Node::Plus(Box::new(node), lazy);
                    }
                    Some('?') => {
                        self.i += 1;
                        let lazy = self.eat_lazy();
                        node = Node::Opt(Box::new(node), lazy);
                    }
                    _ => break,
                }
            }
            out.push(node);
        }
        Ok(out)
    }

    fn eat_lazy(&mut self) -> bool {
        if self.peek() == Some('?') {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn atom(&mut self) -> Result<Node, String> {
        match self.next() {
            None => Err("unexpected end of pattern".to_string()),
            Some('\\') => match self.next() {
                Some(c @ ('w' | 'd' | 's' | 'W' | 'D' | 'S')) => {
                    let ranges: Vec<(char, char)> = match c.to_ascii_lowercase() {
                        'w' => vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')],
                        'd' => vec![('0', '9')],
                        _ => vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')],
                    };
                    Ok(Node::Class {
                        ranges,
                        negated: c.is_ascii_uppercase(),
                    })
                }
                Some(c) => Ok(Node::Lit(c)),
                None => Err("dangling escape".to_string()),
            },
            Some('.') => Ok(Node::Any),
            Some('^') => Ok(Node::Start),
            Some('$') => Ok(Node::End),
            Some('(') => {
                // `(?:` is accepted and ignored, as are capture groups.
                if self.peek() == Some('?') {
                    self.i += 1;
                    if self.peek() == Some(':') {
                        self.i += 1;
                    } else {
                        return Err("unsupported group flag".to_string());
                    }
                }
                let inner = self.alternatives_until_paren()?;
                Ok(Node::Group(inner))
            }
            Some('[') => self.class(),
            Some(c @ ('*' | '+' | '?') ) => Err(format!("nothing to repeat `{c}`")),
            Some(c) => Ok(Node::Lit(c)),
        }
    }

    fn alternatives_until_paren(&mut self) -> Result<Vec<Vec<Node>>, String> {
        let mut alts = Vec::new();
        loop {
            alts.push(self.sequence()?);
            match self.peek() {
                Some('|') => {
                    self.i += 1;
                }
                Some(')') => {
                    self.i += 1;
                    return Ok(alts);
                }
                _ => return Err("unclosed group".to_string()),
            }
        }
    }

    fn class(&mut self) -> Result<Node, String> {
        let mut negated = false;
        if self.peek() == Some('^') {
            self.i += 1;
            negated = true;
        }
        let mut ranges = Vec::new();
        let mut first = true;
        loop {
            let c = match self.next() {
                None => return Err("unclosed character class".to_string()),
                Some(c) => c,
            };
            if c == ']' && !first {
                return Ok(Node::Class { ranges, negated });
            }
            first = false;
            let lo = if c == '\\' {
                self.next().ok_or("dangling escape in class")?
            } else {
                c
            };
            if self.peek() == Some('-') && self.c.get(self.i + 1).copied() != Some(']') {
                self.i += 1;
                let hi = match self.next() {
                    Some('\\') => self.next().ok_or("dangling escape in class")?,
                    Some(h) => h,
                    None => return Err("unclosed character class".to_string()),
                };
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
        }
    }
}

fn match_seq(nodes: &[Node], text: &[char], pos: usize) -> Option<usize> {
    match nodes.split_first() {
        None => Some(pos),
        Some((node, rest)) => match node {
            Node::Start => {
                if pos == 0 {
                    match_seq(rest, text, pos)
                } else {
                    None
                }
            }
            Node::End => {
                if pos == text.len() {
                    match_seq(rest, text, pos)
                } else {
                    None
                }
            }
            Node::Lit(c) => {
                if text.get(pos) == Some(c) {
                    match_seq(rest, text, pos + 1)
                } else {
                    None
                }
            }
            Node::Any => match text.get(pos) {
                Some('\n') | None => None,
                Some(_) => match_seq(rest, text, pos + 1),
            },
            Node::Class { ranges, negated } => match text.get(pos) {
                None => None,
                Some(c) => {
                    let inside = ranges.iter().any(|(lo, hi)| lo <= c && c <= hi);
                    if inside != *negated {
                        match_seq(rest, text, pos + 1)
                    } else {
                        None
                    }
                }
            },
            Node::Group(alts) => {
                // Leftmost alternative first, then continue with the rest.
                for alt in alts {
                    if let Some(next) = match_seq(alt, text, pos) {
                        if let Some(done) = match_seq(rest, text, next) {
                            return Some(done);
                        }
                    }
                }
                None
            }
            Node::Opt(inner, _) => {
                if let Some(next) = match_seq(std::slice::from_ref(&**inner), text, pos) {
                    if let Some(done) = match_seq(rest, text, next) {
                        return Some(done);
                    }
                }
                match_seq(rest, text, pos)
            }
            Node::Star(inner, _) => {
                // Greedy: take one iteration, ask for as many more as possible
                // (the recursion below), and only then accept zero more.
                let mut more = vec![Node::Star(inner.clone(), false)];
                more.extend_from_slice(rest);
                if let Some(next) = match_seq(std::slice::from_ref(&**inner), text, pos) {
                    if next > pos {
                        if let Some(done) = match_seq(&more, text, next) {
                            return Some(done);
                        }
                    }
                }
                match_seq(rest, text, pos)
            }
            Node::Plus(inner, _) => {
                let mut more = vec![Node::Star(inner.clone(), false)];
                more.extend_from_slice(rest);
                if let Some(next) = match_seq(std::slice::from_ref(&**inner), text, pos) {
                    if next > pos {
                        return match_seq(&more, text, next);
                    }
                }
                None
            }
        },
    }
}

/// Match `nodes` at `pos` and return the end position.
fn match_one(node: &Node, text: &[char], pos: usize) -> Option<usize> {
    match_seq(std::slice::from_ref(node), text, pos)
}

/// `regex::escape`: put a backslash before every character that means
/// something to the regex engine.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
                | '#' | '&' | '-' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
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
}
