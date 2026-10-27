//! Command-line validation.
//!
//! `input::validate_line` @0x10195f798 is small: it trims the line and rejects
//! it when the trimmed form is empty.  `InputError`'s `Display` impl
//! @0x10195c1a4 spells out the two messages below.

use std::fmt;

/// Why a typed line will not be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputError {
    /// The line was empty (or only whitespace).
    Empty,
    /// The line was too long for the game's line limit.
    TooLong { len: usize, max: usize },
}

/// The longest line the client will send.
///
/// `// UNKNOWN:` the binary does not compare against a limit in
/// `validate_line`; the value is the conservative one Hydra's protocol layer
/// documents.
pub const MAX_LEN: usize = 4096;

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::Empty => f.write_str("there is nothing to send"),
            InputError::TooLong { len, max } => {
                write!(f, "that line is {len} characters; the limit is {max}")
            }
        }
    }
}

impl std::error::Error for InputError {}

/// `@0x10195f798` — check a line before it is sent.
pub fn validate_line(line: &str) -> Result<(), InputError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(InputError::Empty);
    }
    if trimmed.len() > MAX_LEN {
        return Err(InputError::TooLong {
            len: trimmed.len(),
            max: MAX_LEN,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_content() {
        assert_eq!(validate_line("look"), Ok(()));
        assert_eq!(validate_line("  .go north  "), Ok(()));
        assert_eq!(validate_line("\u{a0}x\u{a0}"), Ok(()));
    }

    #[test]
    fn rejects_blank() {
        assert_eq!(validate_line(""), Err(InputError::Empty));
        assert_eq!(validate_line("   "), Err(InputError::Empty));
        assert_eq!(validate_line("\t\n"), Err(InputError::Empty));
    }

    #[test]
    fn rejects_overlong() {
        let long = "x".repeat(MAX_LEN + 1);
        assert_eq!(
            validate_line(&long),
            Err(InputError::TooLong {
                len: MAX_LEN + 1,
                max: MAX_LEN
            })
        );
        // Exactly the limit still fits.
        assert_eq!(validate_line(&"x".repeat(MAX_LEN)), Ok(()));
    }

    #[test]
    fn messages_are_plain() {
        assert_eq!(InputError::Empty.to_string(), "there is nothing to send");
        assert_eq!(
            InputError::TooLong { len: 9, max: 4 }.to_string(),
            "that line is 9 characters; the limit is 4"
        );
    }
}
