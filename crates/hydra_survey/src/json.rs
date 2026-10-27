//! `serde_json`, under the name the recovered code used.
//!
//! **Phase 1 reconnection.** This module used to be a hand-written JSON reader,
//! because the reconstruction had no external crates. The original crate did
//! not have one: it handed every log line straight to `serde_json`
//! (`serde_json::de::from_trait::<Record>` at `0x1013c6f50`, inlined into
//! `log::read`), and the flattened body is read back out of the `Content`
//! buffer that `serde_json::Error::custom` (`0x101f3b6d0`) builds.
//!
//! So the module stays as the name the crate already spoke, but everything in
//! it is now `serde_json`'s: `crate::json::Value` **is** `serde_json::Value`.
//! `log` no longer parses by hand at all (it is `serde_json::from_str` over the
//! derived schema), so no second parser is in use.
//!
//! Two items of the old module could not be preserved, because both existed
//! only to serve the hand-written reader:
//!
//! * `json::Value::kind()` (the serde-style name of a value's kind) and
//!   `json::wrong_type()` — `serde` produces those messages itself now;
//! * `json::parse` returns `serde_json::Error` instead of `String`.
//!
//! One item changed shape rather than disappearing: the old `Value::Object` was
//! a `Vec<(String, Value)>` in *source* order, so `as_object()`/`get()` handed
//! back a slice in document order. `serde_json::Value` keeps its object in a
//! `Map` — a `BTreeMap`, since the workspace pin does not enable
//! `preserve_order` — so keys come back sorted and `as_object()` yields
//! `&Map<String, Value>`. Nothing in this crate reads a line as JSON any more
//! (`log::parse` goes through the derived schema), so the order is not
//! observable here, but a caller that wanted document order would notice.
//! `Value`'s `as_str`, `as_array`, `get` and `Display` are otherwise
//! `serde_json`'s own and unchanged.

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
    fn object_members_come_back_by_name() {
        let value = parse(r#"{"t":"1","run":"a b","pos":[12,-4,0],"ok":true,"no":null}"#).unwrap();
        assert_eq!(value.get("run").and_then(Value::as_str), Some("a b"));
        assert!(value.get("missing").is_none());
        let pos: Vec<i64> = value
            .get("pos")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|item| item.as_i64().unwrap())
            .collect();
        assert_eq!(pos, [12, -4, 0]);
        assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
        assert!(value.get("no").map(Value::is_null).unwrap_or(false));
    }

    #[test]
    fn integers_stay_exact() {
        // The hand-written `Value` kept every number as source text; a `u64`
        // past 2^53 survives here unchanged.
        let value = parse(r#"{"silvers":9007199254740993}"#).unwrap();
        assert_eq!(
            value.get("silvers").unwrap().as_u64(),
            Some(9007199254740993)
        );
        assert_eq!(value.to_string(), r#"{"silvers":9007199254740993}"#);
    }

    #[test]
    fn rejects_garbage() {
        // `serde_json` is stricter than the hand-written reader was: it also
        // refuses a trailing comma and an unterminated literal.
        assert!(parse("{").is_err());
        assert!(parse("{\"a\":}").is_err());
        assert!(parse("{} extra").is_err());
        assert!(parse("").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("nul").is_err());
    }
}
