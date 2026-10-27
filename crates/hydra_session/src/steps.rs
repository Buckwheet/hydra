//! Command "steps": one typed line can be several commands, with waits.
//!
//! Recovered from `steps::{pieces, steps, said, wait}` at `0x101ab0c08`,
//! `0x101ab01e8`, `0x101aafedc`, `0x101aaffb8`.
//!
//! **ESTABLISHED**:
//! * `said` (`0x101aafedc`) maps a `&[Step]` (element stride `0x30` = 48 bytes,
//!   `madd x1, x1, 48, x0`) to strings and joins them with a **7-byte**
//!   separator (`join_generic_copy` with length 7).  The only 7-byte
//!   separator that renders a list of steps as a sentence is `", then "`.
//! * `wait` (`0x101aaffb8`) looks at the first character and formats the
//!   literal at `0x103e7cc12`, which decodes (length-prefixed pieces) to
//!   `` `{}` waits from a moment to {} seconds ``.  So a `wait` word is
//!   either bare ("a moment") or carries a whole number of seconds, and there
//!   is an upper bound that produces that message.
//! * `Aliases::expand` (`0x101aaead4`) calls `steps::pieces`, and
//!   `AliasFile::put` (`0x101aae1ac`) calls `steps::steps`: the alias layer
//!   splits a line into pieces and parses each piece into a step.
//!
//! **UNKNOWN:** the piece separator.  The binary inlines the split, so we
//! choose `;` (and accept `\n`), which is the separator Lich and Stormfront use
//! for multi-command lines.  The upper bound on a wait is not recoverable
//! either; 60 seconds is our choice and is reported by [`MAX_WAIT`].

use std::time::Duration;

/// The longest single wait a step may ask for.
pub const MAX_WAIT: u64 = 60;
/// The word that introduces a wait step.
pub const WAIT_WORD: &str = "wait";
/// What [`said`] puts between two steps (**ESTABLISHED**: 7 bytes).
pub const SEPARATOR: &str = ", then ";

/// One thing to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// send this line to the game (or to Lich)
    Send(String),
    /// pause for this many seconds before the next step
    Wait(u64),
}

impl Step {
    /// How this step reads in [`said`].
    pub fn text(&self) -> String {
        match self {
            Step::Send(line) => line.clone(),
            Step::Wait(0) => "wait a moment".to_string(),
            Step::Wait(1) => "wait 1 second".to_string(),
            Step::Wait(n) => format!("wait {n} seconds"),
        }
    }

    /// The pause this step asks for, if any.
    pub fn duration(&self) -> Option<Duration> {
        match self {
            Step::Send(_) => None,
            Step::Wait(n) => Some(Duration::from_secs(*n)),
        }
    }

    /// The line this step sends, if any.
    pub fn line(&self) -> Option<&str> {
        match self {
            Step::Send(line) => Some(line),
            Step::Wait(_) => None,
        }
    }
}

/// Split a typed line into its pieces.
///
/// **UNKNOWN** separator: see the module note.  Blank pieces are dropped, so
/// `"a;;b;"` is two pieces.
pub fn pieces(line: &str) -> Vec<String> {
    line.split([';', '\n'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse one `wait` word: `wait` alone is "a moment", `wait 5` is five
/// seconds, and anything else is an error with the recovered message.
///
/// **ESTABLISHED** message: `` `{word}` waits from a moment to {MAX_WAIT}
/// seconds ``.
pub fn wait(piece: &str) -> Result<Step, String> {
    let mut parts = piece.split_whitespace();
    let word = parts.next().unwrap_or("");
    if !word.eq_ignore_ascii_case(WAIT_WORD) {
        return Err(format!(
            "`{word}` is not a wait: `wait` or `wait <seconds>`"
        ));
    }
    match parts.next() {
        None => Ok(Step::Wait(0)),
        Some(number) => match number.parse::<u64>() {
            Ok(n) if n <= MAX_WAIT => Ok(Step::Wait(n)),
            _ => Err(if number.parse::<u64>().is_ok() {
                format!("`{piece}` waits from a moment to {MAX_WAIT} seconds")
            } else {
                format!("`{piece}` is not a number of seconds")
            }),
        },
    }
}

/// Parse a whole typed line into steps.
///
/// A piece that starts with the wait word becomes [`Step::Wait`]; everything
/// else is sent as-is.
pub fn steps(line: &str) -> Vec<Step> {
    let mut out = Vec::new();
    for piece in pieces(line) {
        let first = piece.split_whitespace().next().unwrap_or("");
        if first.eq_ignore_ascii_case(WAIT_WORD) {
            match wait(&piece) {
                Ok(step) => out.push(step),
                // A malformed wait is sent through unchanged: the game will
                // reject it, which is better than dropping the line.
                Err(_) => out.push(Step::Send(piece)),
            }
        } else {
            out.push(Step::Send(piece));
        }
    }
    out
}

/// Render steps for a notice: `said(&steps)`.
///
/// **ESTABLISHED** shape: map every step to a string and join with
/// [`SEPARATOR`].
pub fn said(steps: &[Step]) -> String {
    let texts: Vec<String> = steps.iter().map(Step::text).collect();
    texts.join(SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pieces_drops_blanks() {
        assert_eq!(pieces("a;b"), vec!["a", "b"]);
        assert_eq!(pieces(" a ;; b ; "), vec!["a", "b"]);
        assert_eq!(pieces(""), Vec::<String>::new());
        assert_eq!(pieces("go north"), vec!["go north"]);
    }

    #[test]
    fn wait_words() {
        assert_eq!(wait("wait"), Ok(Step::Wait(0)));
        assert_eq!(wait("wait 5"), Ok(Step::Wait(5)));
        assert_eq!(wait("WAIT 60"), Ok(Step::Wait(60)));
        let e = wait("wait 61").unwrap_err();
        assert_eq!(e, "`wait 61` waits from a moment to 60 seconds");
        assert!(wait("wait soon").is_err());
        assert!(wait("run").unwrap_err().contains("is not a wait"));
    }

    #[test]
    fn steps_split_waits_from_sends() {
        let s = steps("go north; wait 2; kill rat");
        assert_eq!(
            s,
            vec![
                Step::Send("go north".into()),
                Step::Wait(2),
                Step::Send("kill rat".into())
            ]
        );
        assert_eq!(Step::Wait(2).duration(), Some(Duration::from_secs(2)));
        assert_eq!(Step::Send("x".into()).duration(), None);
        assert_eq!(Step::Send("x".into()).line(), Some("x"));
    }

    #[test]
    fn said_joins_with_the_seven_byte_separator() {
        assert_eq!(SEPARATOR.len(), 7);
        assert_eq!(
            said(&[Step::Send("go north".into()), Step::Wait(2), Step::Wait(0)]),
            "go north, then wait 2 seconds, then wait a moment"
        );
        assert_eq!(said(&[]), "");
        assert_eq!(said(&[Step::Wait(1)]), "wait 1 second");
    }
}
