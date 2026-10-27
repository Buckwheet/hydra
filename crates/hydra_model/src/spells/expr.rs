//! Spell-cost expression evaluator.
//!
//! `evaluate` (40612 B at `0x101c03e8c`) is the biggest function in the crate.
//! Recovered evidence for its shape: `f64::from_str` on number literals,
//! `+ - * /` handling, parentheses, and a variable lookup — the callers
//! (`GameState::evaluate`, `GameState::spell_cost`) resolve names through
//! `SkillKind::key` / `SkillSet::ranks` / `Effects::remaining` before calling
//! it. The `Debug` impls in the disassembly (`debug_tuple`, `DebugTuple::field`)
//! show it evaluates over an owned token/`f64` tree.
//!
//! `// UNKNOWN:` the exact operator set beyond `+ - * /` and parentheses, and
//! the token grammar. The implementation below is a conventional recursive
//! descent evaluator for that grammar; every variable is resolved through the
//! caller's closure, so no game data is invented here.
//!
//! `*` and `/` bind tighter than `+` and `-`; a leading `-` is unary.

/// Evaluate `expression`, resolving identifiers with `lookup`.
///
/// Returns `None` for a malformed expression or an unknown name.
pub fn evaluate(expression: &str, lookup: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let tokens = tokenize(expression)?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        lookup,
    };
    let value = parser.expression()?;
    if parser.pos == parser.tokens.len() {
        Some(value)
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Number(f64),
    Name(String),
    Plus,
    Minus,
    Star,
    Slash,
    Open,
    Close,
}

fn tokenize(text: &str) -> Option<Vec<Token>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '/' => {
                out.push(Token::Slash);
                i += 1;
            }
            '(' => {
                out.push(Token::Open);
                i += 1;
            }
            ')' => {
                out.push(Token::Close);
                i += 1;
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                out.push(Token::Number(text.parse().ok()?));
            }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == ' ')
                    && !matches!(chars[i], '+' | '-' | '*' | '/' | '(' | ')')
                {
                    // a name may contain single spaces (skill names do); stop
                    // when the space is followed by an operator or digit
                    if chars[i] == ' ' {
                        match chars.get(i + 1) {
                            Some(next) if next.is_alphabetic() || *next == '_' => {}
                            _ => break,
                        }
                    }
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                out.push(Token::Name(text.trim().to_string()));
            }
            _ => return None,
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    lookup: &'a dyn Fn(&str) -> Option<f64>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn expression(&mut self) -> Option<f64> {
        let mut value = self.term()?;
        while let Some(token) = self.peek() {
            match token {
                Token::Plus => {
                    self.pos += 1;
                    value += self.term()?;
                }
                Token::Minus => {
                    self.pos += 1;
                    value -= self.term()?;
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn term(&mut self) -> Option<f64> {
        let mut value = self.factor()?;
        while let Some(token) = self.peek() {
            match token {
                Token::Star => {
                    self.pos += 1;
                    value *= self.factor()?;
                }
                Token::Slash => {
                    self.pos += 1;
                    let divisor = self.factor()?;
                    if divisor == 0.0 {
                        return None;
                    }
                    value /= divisor;
                }
                _ => break,
            }
        }
        Some(value)
    }

    fn factor(&mut self) -> Option<f64> {
        match self.peek()?.clone() {
            Token::Number(value) => {
                self.pos += 1;
                Some(value)
            }
            Token::Minus => {
                self.pos += 1;
                Some(-self.factor()?)
            }
            Token::Plus => {
                self.pos += 1;
                self.factor()
            }
            Token::Open => {
                self.pos += 1;
                let value = self.expression()?;
                match self.peek() {
                    Some(Token::Close) => {
                        self.pos += 1;
                        Some(value)
                    }
                    _ => None,
                }
            }
            Token::Name(name) => {
                self.pos += 1;
                (self.lookup)(&name)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<f64> {
        None
    }

    #[test]
    fn arithmetic_precedence() {
        assert_eq!(evaluate("2 + 3 * 4", &none), Some(14.0));
        assert_eq!(evaluate("(2 + 3) * 4", &none), Some(20.0));
        assert_eq!(evaluate("10 / 4", &none), Some(2.5));
        assert_eq!(evaluate("-5 + 2", &none), Some(-3.0));
        assert_eq!(evaluate("1.5 * 2", &none), Some(3.0));
    }

    #[test]
    fn identifiers_go_through_the_lookup() {
        let lookup = |name: &str| match name {
            "ranks" => Some(40.0),
            "bonus" => Some(2.5),
            _ => None,
        };
        assert_eq!(evaluate("ranks * bonus", &lookup), Some(100.0));
        assert_eq!(evaluate("unknown", &lookup), None);
    }

    #[test]
    fn names_may_contain_spaces() {
        let lookup = |name: &str| (name == "harness power").then_some(40.0);
        assert_eq!(evaluate("harness power + 2", &lookup), Some(42.0));
    }

    #[test]
    fn malformed_expressions_are_none() {
        assert_eq!(evaluate("", &none), None);
        assert_eq!(evaluate("2 +", &none), None);
        assert_eq!(evaluate("(2 + 3", &none), None);
        assert_eq!(evaluate("2 $ 3", &none), None);
        assert_eq!(evaluate("1 / 0", &none), None);
    }
}
