//! `rewards.toml`, read and written by `serde` + `toml`.
//!
//! **Reconnected (Phase 1)**: the original's document is a serde derive and the
//! `toml` crate — the `struct Raw` and its three field names at
//! `0x10241aee8` … `0x10241af27`, the `visit_map` at `0x1014db690`, the
//! `toml::de::error::Error` `Display` that `Settings::read` formats at
//! `0x1014c8888` and the `toml::ser::to_string` that `Settings::to_toml` calls
//! at `0x1014c952c`.  This module is that derive, at the pinned crate versions
//! (`serde` 1.0.229, `toml` 1.1.6).
//!
//! The three keys, in the order the visitor declares them
//! (`0x10241aee8` … `0x10241aef5`):
//!
//! ```toml
//! time = "19:15"
//! skip = ["Aelith", "GS4:Bob"]
//! store = true
//! ```
//!
//! Behaviour the derive gives back, which the `std` stand-in had to fake:
//!
//! * unknown keys are **ignored** by the reader (serde's default for a
//!   derived struct: `visit_map` drops what it does not know) and are
//!   therefore not carried across a rewrite — the writer serialises a fresh
//!   [`Raw`], exactly as the recovered `to_toml` does;
//! * a missing `skip` or `store` reads as the empty list / `false`
//!   (`#[serde(default)]`), a missing `time` as `None` (serde's `Option` rule);
//! * a syntax error, or a value of the wrong type, is the `toml` crate's own
//!   error — a message with the line and the span, which `Settings::read`
//!   wraps as `{path}: {error}` (`0x103e21615`);
//! * `to_toml` cannot fail for this document; the recovered code's error arm
//!   answers an empty `String` (`0x1014c9538`), which is what [`to_toml`] does
//!   if the crate ever does fail.
//!
//! `skip` and `time` are left out when they are empty, and `store` is always
//! written, so the idle schedule is `store = false\n` — the shape the
//! recovered `to_toml` emits before `Settings::write` removes the file.

use serde::{Deserialize, Serialize};

/// The document `rewards.toml` holds: the serde `Raw` of `Settings::parse`.
///
/// **ESTABLISHED**: `struct Raw` at `0x10241af27` expects exactly these three
/// field names (`0x10241aee8` … `0x10241aef5`).  `time` is an `Option` (the
/// parser tests its tag at `0x1014c8908`), `skip` an array of strings and
/// `store` a bool (`ldrb w8, [x20, #0x20]` in `to_toml`, `0x1014c94fc`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Raw {
    /// `time = "19:15"` — when to claim, as the game's clock spells it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// `skip = ["Aelith", "GS4:Bob"]` — characters not to claim for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip: Vec<String>,
    /// `store = true` — claim the account's monthly SimuCoins too.
    #[serde(default)]
    pub store: bool,
}

/// Parse a `rewards.toml` into [`Raw`] with `toml::from_str`.
///
/// **ESTABLISHED**: `Settings::parse` (`0x1014c87c8`) deserialises the text
/// (`toml::de::Deserializer` for `Raw`) and answers the error's `Display`
/// unchanged, which is why a bad document produces the `toml` crate's own
/// message here.
pub fn parse(text: &str) -> Result<Raw, String> {
    ::toml::from_str::<Raw>(text).map_err(|e| e.to_string())
}

/// Render a [`Raw`] with `toml::to_string`.
///
/// **ESTABLISHED**: `Settings::to_toml` (`0x1014c946c`) builds the `Raw` and
/// serialises it (`0x1014c952c`); its error arm answers a `String` built from
/// an empty literal (`0x1014c9538`), which is what this answers when the crate
/// refuses — it cannot for this document.
pub fn to_toml(raw: &Raw) -> String {
    ::toml::to_string(raw).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reads_the_three_keys() {
        let raw = parse(
            "# the login reward\ntime = \"19:15\"\nskip = [\"Aelith\", \"GS4:Bob\"]\nstore = true\n",
        )
        .unwrap();
        assert_eq!(raw.time.as_deref(), Some("19:15"));
        assert_eq!(raw.skip, vec!["Aelith".to_string(), "GS4:Bob".to_string()]);
        assert!(raw.store);
    }

    #[test]
    fn a_missing_key_is_the_empty_value() {
        let raw = parse("time = \"09:05\"\n").unwrap();
        assert_eq!(raw.time.as_deref(), Some("09:05"));
        assert!(raw.skip.is_empty());
        assert!(!raw.store);
        // and a document with no keys at all is the default
        assert_eq!(parse("").unwrap(), Raw::default());
        assert_eq!(parse("# nothing but a comment\n").unwrap(), Raw::default());
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let raw = parse("\n# hi\n\nstore = false  # off\n").unwrap();
        assert!(!raw.store);
    }

    #[test]
    fn a_hash_inside_a_string_is_kept() {
        let raw = parse("time = \"19:15 # not a comment\"\n").unwrap();
        assert_eq!(raw.time.as_deref(), Some("19:15 # not a comment"));
    }

    #[test]
    fn an_unknown_key_is_read_past() {
        // serde's derived `visit_map` drops a field it does not know; the
        // recovered `Raw` had no `deny_unknown_fields` (its three names are
        // the whole visitor, `0x10241aee8` … `0x10241aef5`).
        let raw = parse("time = \"19:15\"\nalarm = 7\nstore = true\n").unwrap();
        assert_eq!(raw.time.as_deref(), Some("19:15"));
        assert!(raw.store);
        // a rewrite is a fresh `Raw`, so the unknown key is not carried over
        assert_eq!(to_toml(&raw), "time = \"19:15\"\nstore = true\n");
    }

    #[test]
    fn a_bad_document_is_an_error() {
        assert!(parse("time\n").is_err());
        assert!(parse("time = \n").is_err());
        // a value of the wrong type is the crate's message, not a silent default
        let err = parse("store = \"yes\"\n").unwrap_err();
        assert!(err.contains("invalid type"), "{err}");
    }

    #[test]
    fn it_writes_what_it_reads() {
        let text = "time = \"19:15\"\nskip = [\"Aelith\", \"GS4:Bob\"]\nstore = true\n";
        let raw = parse(text).unwrap();
        assert_eq!(to_toml(&raw), text);
        // the key order is the visitor's: time, skip, store
        assert_eq!(parse(&to_toml(&raw)).unwrap(), raw);
    }

    #[test]
    fn an_empty_document_is_one_line() {
        assert_eq!(to_toml(&Raw::default()), "store = false\n");
    }

    #[test]
    fn escapes_survive_a_round_trip() {
        let raw = Raw {
            time: None,
            skip: vec!["a \"quoted\" \\ back\nslash".to_string()],
            store: false,
        };
        assert_eq!(parse(&to_toml(&raw)).unwrap(), raw);
    }
}
