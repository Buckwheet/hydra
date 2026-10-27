//! The travel chat commands: `.go2`, `.route2`, `.sail`, `.portal`, `.exp`.
//!
//! Recovered from `hydra_travel::command::parse` (`0x101557e78`, 3664 bytes)
//! and its private helper `command::named` (`0x101557c20`).
//!
//! `parse` is a hand-inlined `char_indices` scan over the UTF-8 bytes (the
//! four-byte decode ladder at `0x101557f44`..`0x101557fa4` is the standard
//! `chars()` iterator) that:
//!
//! 1. takes the first whitespace-delimited word as the verb, compared
//!    case-insensitively against the pooled verbs `go2`, `route2`, `sail`,
//!    `portal` and `exp` (`0x10241f552`, `0x10241f558`, `0x10241f55e`,
//!    `0x10241f596`, `0x1024091d4`);
//! 2. collects the rest of the words with [`named`] (joined with one space,
//!    then split on `=`);
//! 3. pulls the flags `--global` (`0x10241f1da`) and `--brief`
//!    (`0x10241f1e2`) out of the collected words;
//! 4. answers with the recovered "where?" messages when the name is empty.

/// What a travel chat command asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `.go2 <name>` — walk to a remembered target.
    Go2(Ask),
    /// `.route2 <name>` — show a route without walking it.
    Route2(Ask),
    /// `.sail <port>` — sail to a port.
    Sail(Ask),
    /// `.portal <name>` — use a portal.
    Portal(Ask),
    /// `.exp <what>` — the experience helpers.
    Exp(Ask),
    /// `.go2 save <name>` with no name.
    NeedsName(&'static str),
    /// Nothing usable was typed; the payload is the line to print.
    Where(&'static str),
    /// The verb is not a travel command.
    Unknown,
}

/// The words after the verb.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ask {
    /// The target's name (or the whole rest of the line).
    pub name: String,
    /// `--global` was given.
    pub global: bool,
    /// `--brief` was given.
    pub brief: bool,
    /// `key=value` on the command line, when one was given.
    pub value: Option<String>,
}

impl Ask {
    pub fn new(name: impl Into<String>) -> Ask {
        Ask {
            name: name.into(),
            ..Ask::default()
        }
    }
}

/* The recovered "where?" lines (lengths from the `add` immediates). */
const GO2_WHERE: &str = "go2 where? .go2 bank, or .go2 targets for a list";
const ROUTE2_WHERE: &str = "route2 where? .route2 bank";
const SAIL_WHERE: &str = "sail where? .sail kraken, or .sail help for the ports";
const COURSE_WHERE: &str = "the course where? .sail route kraken";
const SAVE_NEEDS_NAME: &str = "it needs a name: .go2 save den";

/// Join the words after the verb.
///
/// `command::named` (`0x101557c20`) is
/// `join_generic_copy(words, " ")` followed by a scan for the first `=`
/// (byte `0x3d` at `0x101557c90`) which splits the result into a name and a
/// value; a `=` in the first 16 bytes is still split, the 16-byte window is
/// only the vectorised fast path.
pub fn named(words: &[&str]) -> Ask {
    let joined = words.join(" ");
    match joined.split_once('=') {
        Some((name, value)) => Ask {
            name: name.trim().to_string(),
            value: Some(value.trim().to_string()),
            ..Ask::default()
        },
        None => Ask {
            name: joined.trim().to_string(),
            ..Ask::default()
        },
    }
}

/// Parse one chat line.
pub fn parse(line: &str) -> Command {
    let mut words = line.split_whitespace();
    let Some(verb) = words.next() else {
        return Command::Unknown;
    };
    let verb = verb.to_ascii_lowercase();
    let rest: Vec<&str> = words.collect();
    let mut global = false;
    let mut brief = false;
    let mut kept: Vec<&str> = Vec::new();
    for word in &rest {
        match *word {
            "--global" => global = true,
            "--brief" => brief = true,
            other => kept.push(other),
        }
    }
    let mut ask = named(&kept);
    ask.global = global;
    ask.brief = brief;

    match verb.as_str() {
        "go2" => {
            if ask.name.is_empty() {
                return Command::Where(GO2_WHERE);
            }
            if ask.name.eq_ignore_ascii_case("save") || ask.name.eq_ignore_ascii_case("save ") {
                return Command::NeedsName(SAVE_NEEDS_NAME);
            }
            Command::Go2(ask)
        }
        "route2" => {
            if ask.name.is_empty() {
                return Command::Where(ROUTE2_WHERE);
            }
            Command::Route2(ask)
        }
        "sail" => {
            if ask.name.is_empty() {
                return Command::Where(SAIL_WHERE);
            }
            // `.sail route <port>`: the course itself was asked for, with no
            // port named.
            if ask.name.eq_ignore_ascii_case("route") {
                return Command::Where(COURSE_WHERE);
            }
            Command::Sail(ask)
        }
        "portal" => {
            if ask.name.is_empty() {
                return Command::Where(GO2_WHERE);
            }
            Command::Portal(ask)
        }
        "exp" => Command::Exp(ask),
        _ => Command::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recovered_where_lines_are_exact() {
        assert_eq!(
            GO2_WHERE,
            "go2 where? .go2 bank, or .go2 targets for a list"
        );
        assert_eq!(ROUTE2_WHERE, "route2 where? .route2 bank");
        assert_eq!(
            SAIL_WHERE,
            "sail where? .sail kraken, or .sail help for the ports"
        );
        assert_eq!(COURSE_WHERE, "the course where? .sail route kraken");
        assert_eq!(SAVE_NEEDS_NAME, "it needs a name: .go2 save den");
    }

    #[test]
    fn verbs_and_names() {
        assert_eq!(
            parse("go2 Wehnimer's Landing"),
            Command::Go2(Ask::new("Wehnimer's Landing"))
        );
        assert_eq!(
            parse("GO2 bank --brief"),
            Command::Go2(Ask {
                name: "bank".into(),
                global: false,
                brief: true,
                value: None,
            })
        );
        assert_eq!(
            parse("route2 --global den"),
            Command::Route2(Ask {
                name: "den".into(),
                global: true,
                brief: false,
                value: None,
            })
        );
        assert_eq!(parse("sail kraken"), Command::Sail(Ask::new("kraken")));
        assert_eq!(parse("portal rift"), Command::Portal(Ask::new("rift")));
        assert_eq!(parse("exp"), Command::Exp(Ask::default()));
        assert_eq!(parse("look"), Command::Unknown);
        assert_eq!(parse(""), Command::Unknown);
    }

    #[test]
    fn empty_names_answer_with_the_where_line() {
        assert_eq!(parse("go2"), Command::Where(GO2_WHERE));
        assert_eq!(parse("route2"), Command::Where(ROUTE2_WHERE));
        assert_eq!(parse("sail"), Command::Where(SAIL_WHERE));
        assert_eq!(parse("sail route"), Command::Where(COURSE_WHERE));
        assert_eq!(parse("portal"), Command::Where(GO2_WHERE));
        assert_eq!(parse("go2 save"), Command::NeedsName(SAVE_NEEDS_NAME));
    }

    #[test]
    fn named_splits_on_the_first_equals() {
        assert_eq!(named(&["bank"]).name, "bank");
        let ask = named(&["den=284"]);
        assert_eq!(ask.name, "den");
        assert_eq!(ask.value.as_deref(), Some("284"));
    }
}
