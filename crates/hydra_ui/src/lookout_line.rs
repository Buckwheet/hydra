//! Lookout lines: the `[Lookout]` war panel's text.
//!
//! `lookout_line::mark` @0x1018c80d4 is 11 KB — it inlines the whole lookout
//! grammar (hand counts, "you are", "the wind", invader waves).  The token it
//! paints with is [`Token::Lookout`] (0x103991e87).

use crate::theme::Token;

/// Words that make a line a lookout line.
pub const MARKS: [&str; 8] = [
    "Lookout",
    "lookout",
    "invaders",
    "an invader",
    "the warning",
    "watch fire",
    "banner",
    "Lookout's",
];

/// `@0x1018c80d4` — the token a lookout line paints with.
///
/// `// UNKNOWN:` the 11 KB body also updates the lookout state in the model;
/// only the style decision is reproduced here.
pub fn mark(line: &str) -> Option<Token> {
    let lower = line.to_lowercase();
    if MARKS.iter().any(|m| lower.contains(&m.to_lowercase())) {
        Some(Token::Lookout)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookout_lines_are_recognised() {
        assert_eq!(mark("[Lookout] 3 invaders"), Some(Token::Lookout));
        assert_eq!(mark("The warning horn sounds"), Some(Token::Lookout));
        assert_eq!(mark("You see a rock"), None);
    }
}
