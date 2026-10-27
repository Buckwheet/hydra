//! The `relay` command: what `.` and the Lich symbol mean, and who a line is for.
//!
//! Recovered from `hydra_table::relay` (`0x1012243c8` … `0x101224b20`).
//!
//! Two characters decide where a typed line goes: Hydra's own symbol (`.`) and
//! Lich's.  [`in_its_symbols`] is that test — the binary builds the UTF-8 form of
//! each character (`cmp w2, #0x110000` rejects a `char::MAX + 1` "no symbol"
//! marker, then the one- to four-byte encoders at `0x101224408` …) and compares it
//! against the head of the line.

pub mod pick;

/// The most bytes a UTF-8 character can take, and so the longest symbol.
pub const MAX_SYMBOL_LEN: usize = 4;

/// Whether `line` starts with one of Hydra's command symbols.
///
/// **ESTABLISHED** (`relay::in_its_symbols`, `0x1012243c8`): both characters are
/// rejected when they are `0x110000` (`char::MAX + 1`, the "no symbol" marker the
/// caller passes), the UTF-8 bytes of each are built on the stack, and the line is
/// compared against them — a line that is only the symbol does **not** count
/// (`subs x23, x20, x22` then `b.hs`: there must be at least one byte after it).
pub fn in_its_symbols(line: &str, first: Option<char>, second: Option<char>) -> bool {
    let mut symbols = [0u8; 2 * MAX_SYMBOL_LEN];
    let mut lengths = [0usize; 2];
    for (at, symbol) in [first, second].into_iter().enumerate() {
        if let Some(symbol) = symbol {
            let encoded = symbol.encode_utf8(&mut symbols[at * MAX_SYMBOL_LEN..]);
            lengths[at] = encoded.len();
        }
    }
    for at in 0..2 {
        let length = lengths[at];
        if length == 0 || line.len() <= length {
            continue;
        }
        let symbol = &symbols[at * MAX_SYMBOL_LEN..at * MAX_SYMBOL_LEN + length];
        if line.as_bytes().starts_with(symbol) {
            return true;
        }
    }
    false
}

/// The line that teaches `to`.
///
/// **ESTABLISHED**: 56 bytes at `0x102408a19` — `To: name a character and a
/// command, as `to Baelor look`.`
pub const TO_HELP: &str = "To: name a character and a command, as `to Baelor look`.";

/// The line that teaches `all` with names.
///
/// **ESTABLISHED**: 66 bytes at `0x102408a51` — `All: name characters after - or
/// +, as `all -Dicate,Maravel stand`.`
pub const ALL_NAMES_HELP: &str = "All: name characters after - or +, as `all -Dicate,Maravel stand`.";

/// The line that teaches `all` without names.
///
/// **ESTABLISHED**: 55 bytes at `0x102408a93` — `All: say what every character is
/// to do, as `all stand`.`
pub const ALL_HELP: &str = "All: say what every character is to do, as `all stand`.";

/// The `relay` command's own help.
///
/// **ESTABLISHED** (`relay::alone`, `0x10122464c`, and `relay::family`
/// `0x101224b20`): the three lines above, in this order — `To:`, then the two
/// `All:` forms.
pub fn alone() -> Vec<String> {
    vec![
        TO_HELP.to_string(),
        ALL_NAMES_HELP.to_string(),
        ALL_HELP.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_starts_with_a_symbol_is_a_command() {
        assert!(in_its_symbols(".help", Some('.'), None));
        assert!(in_its_symbols(";look", None, Some(';')));
        assert!(in_its_symbols("→look", Some('→'), None), "a multi-byte symbol works");
        assert!(!in_its_symbols("look", Some('.'), None));
        assert!(!in_its_symbols("", Some('.'), None));
        // A line that is only the symbol is not a command.
        assert!(!in_its_symbols(".", Some('.'), None));
        // No symbols at all means no match.
        assert!(!in_its_symbols(".help", None, None));
        // The "no symbol" marker is None here, not a character.
        assert!(!in_its_symbols("\u{10ffff}", None, None));
    }

    #[test]
    fn the_help_lines_are_the_recovered_ones() {
        let help = alone();
        assert_eq!(help.len(), 3);
        assert_eq!(help[0], TO_HELP);
        assert_eq!(help[0].len(), 56);
        assert_eq!(help[1].len(), 66);
        assert_eq!(help[2], ALL_HELP);
        assert_eq!(help[2].len(), 55);
    }
}
