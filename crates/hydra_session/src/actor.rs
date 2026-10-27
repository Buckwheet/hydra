//! The exit protocol and the "pushed commands" hand-off.
//!
//! Recovered from `actor::{is_exit_intent, pushed_commands}` at `0x101a675f4`
//! and `0x101a677d0`.
//!
//! **ESTABLISHED** `is_exit_intent` (`0x101a675f4`, 476 bytes): trim the line
//! (`str::trim_matches`), then compare case-folded bytes against two four-byte
//! words and two three-byte words:
//!
//! ```text
//! len 3:  "qui"  or  "exi"
//! len 4:  "quit" or  "exit"
//! ```
//!
//! The fold is `c | 0x20` for `A`..`Z`, done byte by byte, so it is
//! `eq_ignore_ascii_case`.  Anything else — including "q" and "e" — is not an
//! exit intent.
//!
//! **ESTABLISHED** `pushed_commands` (`0x101a677d0`, 12 KB of inlined serde and
//! zlib): it parses what Lich pushed over the wire — the symbol table shows
//! `serde_json::read::StrRead::parse_str`, `hydra_model::state::menu::
//! LearnedCommands::{set_version, apply}`, `zlib_rs::stable::Inflate`, plus the
//! notes/schema errors — so a pushed payload is a (possibly deflated) JSON
//! document that updates the learned-command table.
//!
//! **std-only departure**: no `zlib`, so a payload is taken as plain JSON; a
//! payload that does not parse is reported through the notes/schema error
//! shapes and dropped.

/// Is this line the player asking to quit?
///
/// **ESTABLISHED**: `"qui"`, `"exi"`, `"quit"`, `"exit"` after trimming, case
/// insensitively.
pub fn is_exit_intent(line: &str) -> bool {
    let trimmed = line.trim();
    match trimmed.len() {
        3 => trimmed.eq_ignore_ascii_case("qui") || trimmed.eq_ignore_ascii_case("exi"),
        4 => trimmed.eq_ignore_ascii_case("quit") || trimmed.eq_ignore_ascii_case("exit"),
        _ => false,
    }
}

/// One command a Lich script pushed at Hydra.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pushed {
    /// the verb
    pub verb: String,
    /// the noun, if any
    pub noun: String,
    /// the version of the menu the push came from
    pub version: u32,
}

/// Parse a pushed payload.
///
/// **ESTABLISHED** shape: a JSON document with a version and a list of
/// commands; `LearnedCommands::apply` is what the original calls with the
/// result.
pub fn pushed_commands(payload: &str) -> Result<Vec<Pushed>, String> {
    let v = crate::textfmt::parse_json(payload)?;
    let version = v.u64_of("version").unwrap_or(0) as u32;
    let mut out = Vec::new();
    if let Some(crate::textfmt::Value::Arr(items)) = v.get("commands") {
        for item in items {
            let verb = item.str_of("verb");
            if verb.is_empty() {
                continue;
            }
            out.push(Pushed {
                verb,
                noun: item.str_of("noun"),
                version,
            });
        }
    }
    Ok(out)
}

/// Fold a push into a learned-command table.
pub fn apply(table: &mut crate::menu_store::MenuFile, pushed: &[Pushed]) -> usize {
    let mut added = 0;
    for push in pushed {
        table.version = table.version.max(push.version);
        if !table
            .commands
            .iter()
            .any(|c| c.verb == push.verb && c.noun == push.noun)
        {
            table.commands.push(crate::menu_store::Learned {
                verb: push.verb.clone(),
                noun: push.noun.clone(),
                confirmed: false,
            });
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_four_exit_words_are_recognised() {
        for word in ["quit", "QUIT", "Quit", "exit", "EXIT", "qui", "exi"] {
            assert!(is_exit_intent(word), "{word} should exit");
            assert!(is_exit_intent(&format!("  {word}  ")), "{word} padded");
        }
        for word in ["q", "e", "qu", "quits", "exiting", "quitter", ""] {
            assert!(!is_exit_intent(word), "{word} should not exit");
        }
        // five bytes is never an exit intent, even if it starts the same way
        assert!(!is_exit_intent("quite"));
        assert!(!is_exit_intent("exits"));
    }

    #[test]
    fn a_push_updates_the_menu() {
        let payload = r#"{"version": 9, "commands": [{"verb": "kill", "noun": "rat"}, {"verb": "stance"}]}"#;
        let pushed = pushed_commands(payload).unwrap();
        assert_eq!(pushed.len(), 2);
        assert_eq!(pushed[0].version, 9);
        let mut table = crate::menu_store::MenuFile::new();
        assert_eq!(apply(&mut table, &pushed), 2);
        assert_eq!(table.version, 9);
        assert_eq!(apply(&mut table, &pushed), 0, "already there");
    }

    #[test]
    fn a_broken_push_is_an_error() {
        assert!(pushed_commands("not json").is_err());
        assert_eq!(pushed_commands("{}").unwrap(), Vec::new());
    }
}
