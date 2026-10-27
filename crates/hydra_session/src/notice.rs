//! Notices — the short texts Hydra shows the player.
//!
//! Recovered from `hydra_session::notice::{closes_a_hunt, help, linked}` at
//! `0x101a7a82c`, `0x101a7a8f4`, `0x101a7aa68`, plus the `Notice` layout used
//! by `command::SessionHandle::say` (`0x101a3c0ec`).
//!
//! ESTABLISHED layout facts (from `SessionHandle::say`):
//! * a `Notice` is a 64-byte value;
//! * the first word is a discriminant;
//! * `+0x10`/`+0x18` hold the line collection (`ptr`, `len`, stride 24 =
//!   `String`), so the lines are a slice of owned strings;
//! * `+0x38` and `+0x39` are two flag/level bytes (the second is compared
//!   against `3` before the notice is handed to the player log).
//!
//! UNKNOWN: the discriminant numbering of the original enum.  We keep a
//! `Kind` enum with the three recovered constructors and give `line` the
//! plain "text" kind.

/// How bad a notice is.  `Ended::notice` (in `hydra_lich`) uses the same
/// scale: `0` is a failure, `1` a warning, `2` information.
pub const LEVEL_FAIL: u8 = 0;
pub const LEVEL_WARN: u8 = 1;
pub const LEVEL_INFO: u8 = 2;
/// The level the binary tests against before logging (`cmp w9, #3`).
pub const LEVEL_QUIET: u8 = 3;

/// What kind of notice this is; the discriminant of the original value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    /// plain text lines (`Notice::line`)
    #[default]
    Text,
    /// the help block (`Notice::help`)
    Help,
    /// a link between a note and a log line (`Notice::linked`)
    Linked,
    /// the closing line of a hunt (`Notice::closes_a_hunt`)
    ClosesAHunt,
}

/// A short multi-line message for the player.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Notice {
    /// discriminant (`+0x00`)
    pub kind: Kind,
    /// severity, `0..=3` (`+0x39`)
    pub level: u8,
    /// the text, one entry per line (`+0x10`/`+0x18`)
    pub lines: Vec<String>,
    /// `+0x38` — set by the constructors that must not be logged
    pub quiet: bool,
}

impl Notice {
    /// A plain notice at `level` with one line.  This is the constructor
    /// `hydra_lich` uses (`Notice::line(2, "...")`).
    pub fn line(level: u8, text: impl Into<String>) -> Self {
        Notice {
            kind: Kind::Text,
            level,
            lines: vec![text.into()],
            quiet: level >= LEVEL_QUIET,
        }
    }

    /// A plain notice with several lines.
    pub fn lines(level: u8, lines: impl IntoIterator<Item = String>) -> Self {
        Notice {
            kind: Kind::Text,
            level,
            lines: lines.into_iter().collect(),
            quiet: level >= LEVEL_QUIET,
        }
    }

    /// Is this notice supposed to be kept out of the player log?
    pub fn is_quiet(&self) -> bool {
        self.quiet || self.level >= LEVEL_QUIET
    }

    /// The whole notice as one string, lines joined with `\n`.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The first line, if any.
    pub fn first_line(&self) -> &str {
        self.lines.first().map(String::as_str).unwrap_or("")
    }

    /// Why a hunt ended, shown when a kill closes it.  **ESTABLISHED** shape:
    /// `closes_a_hunt` takes no arguments and builds exactly one formatted
    /// line at level 2.
    pub fn closes_a_hunt(creature: &str) -> Self {
        Notice::line(
            LEVEL_INFO,
            format!("The hunt for {creature} is closed: nothing else in the room is fighting."),
        )
    }

    /// The `/help` block.  **INFERRED**: `notice::help` counts the characters
    /// of the block (two `char_count` calls, so it builds one string and then
    /// measures it) and returns it as a `Notice`.  The exact list of commands
    /// is not recoverable, only that it exists and is measured, so the block
    /// below documents the commands the rest of this crate implements.
    pub fn help() -> Self {
        const LINES: [&str; 12] = [
            "Hydra commands start with a symbol, . by default.",
            ".go <direction>      walk one room",
            ".queue               show what is waiting to be sent",
            ".alias <name>=<cmds> add or replace an alias",
            ".aliases             list the aliases for this character",
            ".note <text>         keep a note about this room",
            ".notes               list the notes for this room",
            ".clip <text>         copy a line out of the last note",
            ".log <text>          search today's player log",
            ".lich <text>         hand a line straight to Lich",
            ".exit                end the session",
            "Alt+Enter opens the prompt; Enter sends what you typed.",
        ];
        Notice {
            kind: Kind::Help,
            level: LEVEL_INFO,
            lines: LINES.iter().map(|s| (*s).to_string()).collect(),
            quiet: true,
        }
    }

    /// The notice produced when a note is tied to a log line.
    /// **ESTABLISHED** shape: `notice::linked` allocates one `String` of three
    /// parts (see `0x103e7ca1e`, `"{} ({})"`, and the two adjacent argument
    /// markers at `0x103e7bf5b`) and returns it at level 2, not quiet.
    pub fn linked(first_line: &str, day: &str, line: u64) -> Self {
        Notice {
            kind: Kind::Linked,
            level: LEVEL_INFO,
            lines: vec![format!("{first_line} ({day}:{line})")],
            quiet: false,
        }
    }

    /// The notice `hydra_lich` sends when Lich has not said it knows the
    /// character yet (level 2, from `relay::Waits::typed`).
    pub fn lich_waits() -> Self {
        Notice::line(
            LEVEL_INFO,
            "Lich is still logging in; what you typed follows once it knows the character.",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_sets_quiet_only_for_the_quiet_level() {
        let n = Notice::line(LEVEL_INFO, "hi");
        assert_eq!(n.level, 2);
        assert!(!n.is_quiet());
        assert!(Notice::line(LEVEL_QUIET, "hush").is_quiet());
    }

    #[test]
    fn first_line_is_the_first_entry() {
        let n = Notice::lines(LEVEL_WARN, vec!["one".to_string(), "two".to_string()]);
        assert_eq!(n.first_line(), "one");
        assert_eq!(n.text(), "one\ntwo");
        assert_eq!(n.level, 1);
        assert_eq!(Notice::default().first_line(), "");
    }

    #[test]
    fn help_is_twelve_lines_and_quiet() {
        let h = Notice::help();
        assert_eq!(h.kind, Kind::Help);
        assert_eq!(h.lines.len(), 12);
        assert!(h.is_quiet());
        assert_eq!(
            h.first_line(),
            "Hydra commands start with a symbol, . by default."
        );
    }

    #[test]
    fn linked_uses_the_parenthesised_day_and_line() {
        let n = Notice::linked("a cart", "2024-05-01", 42);
        assert_eq!(n.kind, Kind::Linked);
        assert_eq!(n.text(), "a cart (2024-05-01:42)");
    }
}
