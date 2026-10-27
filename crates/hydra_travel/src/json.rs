//! `serde_json`, under the name the recovered code used.
//!
//! **Phase 1 reconnection.** This module used to be a hand-written JSON
//! reader/writer, because the reconstruction had no external crates. The
//! original crate did not have one: it handed `travel.json` straight to
//! `serde_json` (`serde_json::de::from_trait` at `0x101537760`,
//! `serde_json::ser::Serializer` inlined in `file::Shared::serialize`) and
//! stored the free-form remembered values as `serde_json::Value` — the drop
//! glue of `Spot` (`0x10150a8c4`) drops a `BTreeMap<String, serde_json::Value>`,
//! and `walk::drive::routine_name` (`0x10153831c`) reads a
//! `serde_json::Value::String`'s `ptr`/`len` at `+0x10`/`+0x18`.
//!
//! So the module stays as the name the crate already spoke, but everything in
//! it is now `serde_json`'s: `crate::json::Value` **is**
//! `serde_json::Value`, which is the type the binary's `Spot::memories` and
//! `Shared::memories` maps hold. Nothing outside this crate used
//! `hydra_travel::json`, so the only thing that had to survive is the paths
//! (`json::Value`, `json::parse`) — both keep their shape.

pub use serde_json::{Error, Map, Number, Value};

/// Parse a JSON document.
///
/// Was the hand-written parser; is now `serde_json::from_str`, the recovered
/// `serde_json::de::from_trait`.
pub fn parse(text: &str) -> Result<Value, Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_is_serde_json_value() {
        // The alias is real: the very same type, with `serde_json`'s writers.
        let value: Value = Value::from("a\"b\\c\nd\te\u{1}");
        assert_eq!(value.to_string(), r#""a\"b\\c\nd\te\u0001""#);
        assert_eq!(parse(&value.to_string()).unwrap(), value);
    }

    #[test]
    fn integers_stay_exact() {
        // The hand-written `Value` kept every number as `f64`, which rounds
        // anything past 2^53; `serde_json` keeps the integer, so a big silver
        // count survives a trip through `memories` unchanged.
        let value = parse(r#"{"silvers":9007199254740993}"#).unwrap();
        assert_eq!(value.get("silvers").unwrap().as_u64(), Some(9007199254740993));
        assert_eq!(value.to_string(), r#"{"silvers":9007199254740993}"#);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("nul").is_err());
    }
}
