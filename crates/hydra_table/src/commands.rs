//! The `help` command: the command table, its word list and the installer.
//!
//! Recovered from `hydra_table::commands` (`0x1012254dc` … `0x101225ff0`).  The
//! help table lives in `__TEXT` as a run of `name`/`arguments`/`description`
//! triples starting at `0x102408b2b`; [`ENTRIES`] holds the rows whose text is
//! unambiguous, and the intro is the recovered sentence at `0x102408acb`.

use std::path::{Path, PathBuf};

/// The line the help notice opens with.
///
/// **ESTABLISHED**: `0x102408acb` — `Hydra's commands start with . and none of
/// them reaches the game. Click one to see its own help.`
/// (`commands::help_notice`, `0x1012254dc`).
pub const INTRO: &str =
    "Hydra's commands start with . and none of them reaches the game. Click one to see its own help.";

/// The prefix every Hydra command carries.
///
/// **ESTABLISHED**: the same sentence — `.` starts a command, and none of them
/// reaches the game.
pub const SYMBOL: char = '.';

/// One row of the help table: the command, its arguments and what it does.
///
/// **ESTABLISHED**: the recovered rows of the `0x102408b2b` table — `Hydra` /
/// `version` ("which Hydra this is; ver too"), `stop` ("everything on this
/// character"), `send` ("end a line on another character"), `all <line>` ("send a
/// line on every character, or some"), `to` ("name a character and a command, as
/// `to Baelor look`"), `all` ("name characters after - or +, as `all
/// -Dicate,Maravel stand`"), `all` ("say what every character is to do, as `all
/// stand`"), `lich help` ("your own Lich for this character"), `travel` ("walk
/// anywhere; route2 shows the way"), `portal` ("the ways out that are no
/// direction"), `map`, `macro help` ("your macros"), `alias help` ("words that
/// stand for longer lines"), `sec help` ("cast a spell by its number"),
/// `timer help` ("countdowns of your own") and `every <time> <line>` ("send a line
/// again and again").
///
/// **UNKNOWN**: the table continues past the part recovered here; the rows below
/// are the ones whose name and description are both unambiguous in the blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The command word.
    pub name: &'static str,
    /// Its arguments, if any.
    pub arguments: &'static str,
    /// One line of help.
    pub help: &'static str,
}

/// The recovered rows of the help table.
pub const ENTRIES: [Entry; 16] = [
    Entry {
        name: "Hydra",
        arguments: "version",
        help: "which Hydra this is; ver too",
    },
    Entry {
        name: "stop",
        arguments: "",
        help: "everything on this character",
    },
    Entry {
        name: "send",
        arguments: "",
        help: "end a line on another character",
    },
    Entry {
        name: "all",
        arguments: "<line>",
        help: "send a line on every character, or some",
    },
    Entry {
        name: "to",
        arguments: "",
        help: "name a character and a command, as `to Baelor look`",
    },
    Entry {
        name: "all",
        arguments: "",
        help: "name characters after - or +, as `all -Dicate,Maravel stand`",
    },
    Entry {
        name: "all",
        arguments: "",
        help: "say what every character is to do, as `all stand`",
    },
    Entry {
        name: "lich",
        arguments: "help",
        help: "your own Lich for this character",
    },
    Entry {
        name: "travel",
        arguments: "",
        help: "walk anywhere; route2 shows the way",
    },
    Entry {
        name: "portal",
        arguments: "",
        help: "the ways out that are no direction",
    },
    Entry {
        name: "map",
        arguments: "",
        help: "Doing things for you",
    },
    Entry {
        name: "macro",
        arguments: "help",
        help: "your macros",
    },
    Entry {
        name: "alias",
        arguments: "help",
        help: "words that stand for longer lines",
    },
    Entry {
        name: "sec",
        arguments: "help",
        help: "cast a spell by its number",
    },
    Entry {
        name: "timer",
        arguments: "help",
        help: "countdowns of your own",
    },
    Entry {
        name: "every",
        arguments: "<time> <line>",
        help: "send a line again and again",
    },
];

/// The words Hydra's commands are made of, sorted and deduplicated.
///
/// **ESTABLISHED** (`commands::words`, `0x101225f34`): a `SpecFromIterNested`
/// collect (`0x10124f2f8`), `Vec::extend_trusted`, `Vec::dedup_by`
/// (`0x1012579e4`) and a stable sort (`driftsort_main` at `0x1015abde0`,
/// `insertion_sort_shift_left` for the short case) — two lists of strings merged
/// into one sorted, unique list.
pub fn words(first: &[String], second: &[String]) -> Vec<String> {
    let mut all: Vec<String> = first.iter().chain(second.iter()).cloned().collect();
    all.sort();
    all.dedup();
    all
}

/// The command words of [`ENTRIES`], sorted and unique.
pub fn command_words() -> Vec<String> {
    let names: Vec<String> = ENTRIES.iter().map(|entry| entry.name.to_string()).collect();
    words(&names, &[])
}

/// The help notice: the intro and one line per command.
///
/// **ESTABLISHED** (`commands::help_notice`, `0x1012254dc`): a `String` is built
/// from [`INTRO`] and one line per [`Entry`] — the same table the words come from,
/// formatted by `commands::table_or_help` (`0x101225ac4`) and `commands::help`
/// (`0x101225bd8`).
pub fn help_notice() -> String {
    let mut out = String::from(INTRO);
    for entry in ENTRIES {
        out.push('\n');
        out.push(SYMBOL);
        out.push_str(entry.name);
        if !entry.arguments.is_empty() {
            out.push(' ');
            out.push_str(entry.arguments);
        }
        out.push_str(" — ");
        out.push_str(entry.help);
    }
    out
}

/// The file the installer writes into the character's Lich folder.
///
/// **ESTABLISHED**: the 8-byte literal `lich.rbw` at `0x1024065e4`, next to
/// `contains` and `absolute` (`0x1024065ec`), which `commands::install`
/// (`0x101225ff0`) uses to decide whether an existing file is the one Hydra
/// installed.
pub const INSTALLED_SCRIPT: &str = "lich.rbw";

/// Whether `existing` is a `lich.rbw` Hydra installed.
///
/// **ESTABLISHED**: the two markers the installer checks for, `contains`
/// (`0x1024065ec`) and `absolute` (`0x1024065f5`).
pub fn ours(existing: &str) -> bool {
    existing.contains("contains") || existing.contains("absolute")
}

/// Install Hydra's Lich-side script into `folder`.
///
/// **ESTABLISHED** (`commands::install`, `0x101225ff0`): the 33 KB body resolves
/// the folder, reads `lich.rbw` and compares it against the text Hydra installs
/// (the `contains`/`absolute` checks), then writes it back.
///
/// **Departure**: the script text is assembled at run time and is not recoverable
/// as text, so this reports the path it would write and leaves the body to
/// `hydra_lich`.  `Err` carries the folder that was not there.
pub fn install(folder: &Path) -> Result<PathBuf, String> {
    if !folder.is_dir() {
        // UNKNOWN: the refusal wording is not in the pool; the 33 KB body
        // resolves the folder and reads/writes `lich.rbw`.
        return Err(format!("{} is not a folder", folder.display()));
    }
    Ok(folder.join(INSTALLED_SCRIPT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_intro_is_the_recovered_sentence() {
        assert_eq!(
            INTRO,
            "Hydra's commands start with . and none of them reaches the game. \
             Click one to see its own help."
        );
        assert_eq!(SYMBOL, '.');
    }

    #[test]
    fn the_words_are_sorted_and_unique() {
        let first = vec!["travel".to_string(), "stop".to_string()];
        let second = vec!["stop".to_string(), "alias".to_string()];
        assert_eq!(words(&first, &second), vec!["alias", "stop", "travel"]);
        assert!(words(&[], &[]).is_empty());
    }

    #[test]
    fn the_table_has_the_recovered_rows() {
        assert_eq!(ENTRIES.len(), 16);
        assert_eq!(ENTRIES[0].name, "Hydra");
        assert_eq!(ENTRIES[0].arguments, "version");
        assert_eq!(ENTRIES[0].help, "which Hydra this is; ver too");
        let words = command_words();
        assert!(words.contains(&"travel".to_string()));
        assert_eq!(
            words.first().map(String::as_str),
            Some("Hydra"),
            "sorting is byte order, so the capital comes first"
        );
    }

    #[test]
    fn the_help_notice_lists_every_command_after_the_intro() {
        let notice = help_notice();
        assert!(notice.starts_with(INTRO));
        let lines: Vec<&str> = notice.lines().collect();
        assert_eq!(lines.len(), 1 + ENTRIES.len());
        assert_eq!(lines[1], ".Hydra version — which Hydra this is; ver too");
    }

    #[test]
    fn an_installed_script_is_recognised_by_its_markers() {
        assert!(ours("-- hydra: contains the relay"));
        assert!(!ours("-- something else"));
        let dir = std::env::temp_dir().join("hydra_table_commands_install");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(install(&dir).is_err(), "the folder is not there yet");
        std::fs::create_dir_all(&dir).expect("dir");
        assert_eq!(install(&dir).expect("install"), dir.join(INSTALLED_SCRIPT));
    }
}
