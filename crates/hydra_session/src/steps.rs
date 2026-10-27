//! Command "steps": one typed line can be several commands, with waits.
//!
//! Recovered from `steps::{pieces, steps, said, wait}` at `0x101ab0c08`,
//! `0x101ab01e8`, `0x101aafedc`, `0x101aaffb8`.
//!
//! **ESTABLISHED**:
//! * `said` (`0x101aafedc`) maps a `&[Step]` (element stride `0x30` = 48 bytes,
//!   `madd x1, x1, 48, x0`) to strings and joins them with a **7-byte**
//!   separator (`join_generic_copy` with length 7) whose bytes are at
//!   `0x1039c770b`: `", then "`.
//! * The step renderings (the `Map::fold` inlined at `0x101a9e900`):
//!   discriminant `0` is the line itself; discriminant `1` is a real-time wait,
//!   formatted from the step's `Duration` as `format!("wait {} s", secs_f64)`
//!   (the `f32` divisor at `0x101a9e960` is `1e9`); discriminant `2` is the
//!   23-byte literal `"wait for roundtime"` (`0x1039c751b`) and discriminant `3`
//!   the 18-byte `"wait for cast roundtime"` (`0x1039c7509`); every other
//!   discriminant goes through `steps::behaviour_said` (`0x101aafd40`).
//! * `wait` (`0x101aaffb8`) decodes the **first character** of the piece and
//!   compares it case-insensitively against `'s'` (`and w9, w9, #0xffffffdf` /
//!   `cmp w9, #0x53`), parses the rest as an **`f32`** (`<f32 as FromStr>`), and
//!   requires `0.0 <= s < 60.0` (`cmp x8, #0x3c`); anything else formats the
//!   literal at `0x103e7cc12`, which decodes (length-prefixed pieces) to
//!   `` `{}` waits from a moment to {} seconds `` with `60` as the second
//!   argument.  So the original's wait spelling is `s<seconds>`, not
//!   `wait <seconds>`.
//! * `steps` (`0x101ab01e8`) calls `pieces` (`0x101ab0c08`) and then `wait` on
//!   each piece, sending the piece unchanged when `wait` refuses it.
//! * `Aliases::expand` (`0x101aaead4`) calls `steps::pieces`, and
//!   `AliasFile::put` (`0x101aae1ac`) calls `steps::steps`: the alias layer
//!   splits a line into pieces and parses each piece into a step.
//! * The piece separator: `pieces` (2 KB of hand-written char handling)
//!   compares each character against `','` (`0x2c`), `'['` (`0x5b`), `']'` and
//!   `'\\'` (`0x5c`), so **a comma separates steps** and a backslash escapes the
//!   next character.  [`pieces`] splits on `,` and honours `\,`; `;` and `\n`
//!   are kept as tolerated extras the original does not have.
//!
//! **UNKNOWN**: what `[`/`]` group in the original, and what the other step
//! kinds are (`steps::behaviour_said` is a thin tail call), so [`Step`] models
//! only "send a line" and "wait N seconds".

use std::time::Duration;

/// The longest single wait a step may ask for (**ESTABLISHED**: `cmp x8, #0x3c`
/// in `steps::wait`).
pub const MAX_WAIT: u64 = 60;
/// The word that introduces a wait step in this crate's spelling.
pub const WAIT_WORD: &str = "wait";
/// What [`said`] puts between two steps (**ESTABLISHED**: the 7 bytes at
/// `0x1039c770b`).
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
    ///
    /// **ESTABLISHED**: a wait renders as `format!("wait {} s", secs_f64)`.
    pub fn text(&self) -> String {
        match self {
            Step::Send(line) => line.clone(),
            Step::Wait(n) => format!("wait {n} s"),
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
/// **ESTABLISHED**: a comma separates steps and a backslash escapes the next
/// character (`0x101ab0c08` compares against `,`, `[`, `]` and `\`).  Blank
/// pieces are dropped, so `"a,,b,"` is two pieces.  `;` and `\n` are tolerated
/// extras the original does not have; what `[`/`]` group is **UNKNOWN**.
pub fn pieces(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(escaped) => current.push(escaped),
                None => current.push('\\'),
            },
            ',' | ';' | '\n' => {
                let piece = current.trim();
                if !piece.is_empty() {
                    out.push(piece.to_string());
                }
                current.clear();
            }
            _ => current.push(c),
        }
    }
    let piece = current.trim();
    if !piece.is_empty() {
        out.push(piece.to_string());
    }
    out
}

/// The recovered refusal for a wait that is not a number of seconds
/// (**ESTABLISHED**: `` `{}` waits from a moment to 60 seconds ``).
fn not_seconds(piece: &str) -> String {
    format!("`{piece}` waits from a moment to {MAX_WAIT} seconds")
}

/// Parse one wait word.
///
/// **ESTABLISHED** spelling (`0x101aaffb8`): `s<seconds>` — the first
/// character is compared case-insensitively against `'s'` and the rest parses
/// as an `f32` in `0.0..60.0`.  The original's step holds the `Duration`, so a
/// fractional wait keeps its fraction there; [`Step::Wait`] is whole seconds, so
/// `s2.5` is truncated to two seconds here.
///
/// The crate also accepts the more explicit `wait <seconds>`, and bare `wait`
/// for a zero-second wait; the original has no such spelling.
pub fn wait(piece: &str) -> Result<Step, String> {
    let piece = piece.trim();
    let mut chars = piece.chars();
    if let Some(first) = chars.next() {
        if first.eq_ignore_ascii_case(&'s') {
            return match chars.as_str().parse::<f32>() {
                Ok(seconds)
                    if seconds.is_finite() && seconds >= 0.0 && seconds < MAX_WAIT as f32 =>
                {
                    Ok(Step::Wait(seconds as u64))
                }
                _ => Err(not_seconds(piece)),
            };
        }
    }
    let mut parts = piece.split_whitespace();
    let word = parts.next().unwrap_or("");
    if !word.eq_ignore_ascii_case(WAIT_WORD) {
        return Err(format!("`{word}` is not a wait: `wait` or `s<seconds>`"));
    }
    match parts.next() {
        None => Ok(Step::Wait(0)),
        Some(number) => match number.parse::<u64>() {
            Ok(n) if n <= MAX_WAIT => Ok(Step::Wait(n)),
            _ => Err(not_seconds(piece)),
        },
    }
}

/// Parse a whole typed line into steps.
///
/// **ESTABLISHED** (`0x101ab01e8`): split the line with [`pieces`] and run
/// [`wait`] over **every** piece; a piece `wait` refuses is sent as-is.
pub fn steps(line: &str) -> Vec<Step> {
    let mut out = Vec::new();
    for piece in pieces(line) {
        match wait(&piece) {
            Ok(step) => out.push(step),
            // A piece that is not a wait is sent through unchanged.
            Err(_) => out.push(Step::Send(piece)),
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
    fn pieces_splits_on_commas_and_drops_blanks() {
        // The recovered separator is ',' (0x101ab0c08).
        assert_eq!(pieces("go north, kill rat"), vec!["go north", "kill rat"]);
        assert_eq!(pieces("a,,b,"), vec!["a", "b"]);
        assert_eq!(pieces(""), Vec::<String>::new());
        assert_eq!(pieces("go north"), vec!["go north"]);
        // ';' and '\n' are this crate's tolerated extras.
        assert_eq!(pieces("a;b"), vec!["a", "b"]);
        assert_eq!(pieces(" a ;; b ; "), vec!["a", "b"]);
        // A backslash escapes the next character.
        assert_eq!(pieces(r"say hi\, friend"), vec!["say hi, friend"]);
    }

    #[test]
    fn wait_words() {
        assert_eq!(wait("wait"), Ok(Step::Wait(0)));
        assert_eq!(wait("wait 5"), Ok(Step::Wait(5)));
        assert_eq!(wait("WAIT 60"), Ok(Step::Wait(60)));
        let e = wait("wait 61").unwrap_err();
        assert_eq!(e, "`wait 61` waits from a moment to 60 seconds");
        assert!(wait("run").unwrap_err().contains("is not a wait"));
        // The recovered spelling: 's' (any case) then an f32 in 0.0..60.0.
        assert_eq!(wait("s2"), Ok(Step::Wait(2)));
        assert_eq!(wait("S2.5"), Ok(Step::Wait(2)));
        assert_eq!(wait("s0"), Ok(Step::Wait(0)));
        assert_eq!(
            wait("s60").unwrap_err(),
            "`s60` waits from a moment to 60 seconds"
        );
        assert_eq!(
            wait("soon").unwrap_err(),
            "`soon` waits from a moment to 60 seconds"
        );
        assert!(wait("s-1").is_err());
    }

    #[test]
    fn steps_split_waits_from_sends() {
        let s = steps("go north, wait 2, kill rat");
        assert_eq!(
            s,
            vec![
                Step::Send("go north".into()),
                Step::Wait(2),
                Step::Send("kill rat".into())
            ]
        );
        // The recovered spelling goes through the same path.
        assert_eq!(
            steps("go north, s2"),
            vec![Step::Send("go north".into()), Step::Wait(2)]
        );
        assert_eq!(Step::Wait(2).duration(), Some(Duration::from_secs(2)));
        assert_eq!(Step::Send("x".into()).duration(), None);
        assert_eq!(Step::Send("x".into()).line(), Some("x"));
    }

    #[test]
    fn said_joins_with_the_seven_byte_separator() {
        assert_eq!(SEPARATOR.len(), 7);
        assert_eq!(SEPARATOR, ", then ");
        assert_eq!(
            said(&[Step::Send("go north".into()), Step::Wait(2), Step::Wait(0)]),
            "go north, then wait 2 s, then wait 0 s"
        );
        assert_eq!(said(&[]), "");
        assert_eq!(said(&[Step::Wait(1)]), "wait 1 s");
    }
}
