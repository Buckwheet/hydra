//! One cast, as handed to the driver.
//!
//! `hydra_cast::request::request` (0x1019a1ef0, 19856 bytes) is the largest
//! function in the crate after `driver::start`.  What is established:
//!
//! * The argument is a byte slice: the prologue keeps `(x0, x1)` and computes
//!   `x26 = x0 + x1` once (0x1019a1f34), and the body never grows it — it is a
//!   scanner over raw bytes, not over `char`s.
//! * It is the thing that produces [`Request`]s: `driver::start` holds a
//!   `tokio::sync::mpsc::unbounded::UnboundedReceiver<hydra_cast::request::Request>`
//!   (drop glue at 0x10197f5b4) and `Request` owns heap data (drop glue at
//!   0x101a3c3c), so a request carries at least one `String`.
//! * Two of the texts it matches are whole and were recovered from the literal
//!   pool: `"keeps the spell from working."` (0x1039c4706) and
//!   `"keep the spell from working."` (0x1039c4723) — the game's way of saying
//!   the character's stance or verb blocked the spell.
//!
//! UNKNOWN: the remaining ~19 KB of the scanner.  The body is a chain of
//! length checks against game text with the words compared as immediates, and
//! without the game's message corpus only the two texts above can be named.
//! The parser below therefore covers the *crate's own command grammar* (which
//! is established from the formats `plan` builds) and reports "not a request"
//! for everything else.

use crate::settings::{Chosen, VERBS};

/// The game's "your stance blocked the spell" texts, recovered verbatim.
pub const BLOCKED_TEXTS: [&str; 2] = [
    "keeps the spell from working.",
    "keep the spell from working.",
];

/// One cast for the driver to carry out.
///
/// UNKNOWN: the original's field list.  What the driver needs — which spell, at
/// what, with which verb — is the minimum consistent with `plan::plan`, which
/// takes a spell number and formats a target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// The spell number.
    pub spell: u16,
    /// The word the cast is aimed at, if any.
    pub target: Option<String>,
    /// A verb that overrides the settings for this one cast.
    pub verb: Option<Chosen>,
}

impl Request {
    /// A request for one spell.
    pub fn new(spell: u16) -> Request {
        Request {
            spell,
            target: None,
            verb: None,
        }
    }
}

/// Read a line of the caster's own command grammar.
///
/// The grammar is the one `plan` writes back out: `prepare <n>`,
/// `cast <n> [target]`, `incant <n> [channel|evoke]`, and a bare
/// `<n> [target]` (the "typed spell" form `ec on` accepts).
///
/// ESTABLISHED: the four with the literals — `"prepare {}"` at 0x103e6c6d3,
/// `"cast {}"` at 0x103e6c6dd, `"cast{}"` at 0x103e7b596, `"incant {}{}"` at
/// 0x103e7b5b0 with the suffix table `["channel", "evoke", "channel evoke"]`
/// at 0x10449c628.
///
/// UNKNOWN: the original took a byte slice and matched game text; this takes
/// `&str` because the crate's own command path is the only part recovered.
pub fn request(line: &str) -> Option<Request> {
    let mut words = line.split_whitespace();
    let first = words.next()?;
    match first.to_ascii_lowercase().as_str() {
        "incant" => {
            let spell = words.next()?.parse::<u16>().ok()?;
            let verb = match words.next().map(|word| word.to_ascii_lowercase()).as_deref() {
                Some(word) if word == VERBS[1] => Some(Chosen::Channel),
                Some(word) if word == VERBS[2] => Some(Chosen::Evoke),
                Some(_) => return None,
                None => None,
            };
            Some(Request {
                spell,
                target: None,
                verb,
            })
        }
        "prepare" | "cast" => Some(Request {
            spell: words.next()?.parse::<u16>().ok()?,
            target: words.next().map(|word| word.to_string()),
            verb: None,
        }),
        // The typed form: "<n> [target]".
        _ => Some(Request {
            spell: first.parse::<u16>().ok()?,
            target: words.next().map(|word| word.to_string()),
            verb: None,
        }),
    }
}

/// Whether a line of game text says the spell was blocked by the character's
/// stance or verb.
pub fn blocked(line: &str) -> bool {
    BLOCKED_TEXTS.iter().any(|text| line.contains(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_prepare_and_cast() {
        assert_eq!(request("prepare 901"), Some(Request::new(901)));
        assert_eq!(
            request("cast 901 kobold"),
            Some(Request {
                spell: 901,
                target: Some("kobold".to_string()),
                verb: None,
            })
        );
    }

    #[test]
    fn reads_incant_suffixes() {
        assert_eq!(
            request("incant 901 channel"),
            Some(Request {
                spell: 901,
                target: None,
                verb: Some(Chosen::Channel),
            })
        );
        assert_eq!(
            request("incant 901 evoke"),
            Some(Request {
                spell: 901,
                target: None,
                verb: Some(Chosen::Evoke),
            })
        );
        assert_eq!(request("incant 901 shout"), None);
    }

    #[test]
    fn reads_the_typed_form_and_rejects_rubbish() {
        assert_eq!(
            request("901"),
            Some(Request {
                spell: 901,
                target: None,
                verb: None,
            })
        );
        assert_eq!(request("zap"), None);
        assert_eq!(request("prepare"), None);
    }

    #[test]
    fn reads_the_recovered_blocked_texts() {
        assert!(blocked("Your guarded stance keeps the spell from working."));
        assert!(blocked("Elemental spells keep the spell from working."));
        assert!(!blocked("You prepare 901."));
    }
}
