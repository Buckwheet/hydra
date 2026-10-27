//! JSON documents: the recovered field order plus a `serde_json`-backed
//! writer and parser.
//!
//! The original crate used `serde` + `serde_json`; that is what the wire types
//! (`wire::Out`, `wire::In`, every leaf payload) now derive against.  This
//! module keeps the small ordering-preserving [`Value`] the crate's `fields()`
//! helpers build, so the *recovered serialization order* can be spelled out and
//! asserted next to the serde derives.  Nothing here parses or escapes JSON by
//! hand any more: [`parse`] is `serde_json::from_str`, [`write_str`] and
//! [`Value::to_json`]/[`Value::write`] go through `serde_json`'s writer (which
//! is why the escaping is exactly the escaping the original emitted).
//!
//! `Value` keeps object keys **in document/insertion order** — `serde_json`'s
//! own `Map` is a `BTreeMap` without the `preserve_order` feature, which would
//! sort the recovered field order away.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
use std::fmt;

/// A JSON value, with object keys in insertion order.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Numbers are kept as text so integers round-trip without float loss
    /// (`serde_json::Number` keeps its own representation for the same reason).
    Num(String),
    Str(String),
    Arr(Vec<Value>),
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// `Value::Num` from an integer.
    pub fn int<N: std::fmt::Display>(n: N) -> Value {
        Value::Num(n.to_string())
    }

    /// A number from an `f64`, written the way `serde_json` writes it (no
    /// trailing `.0` on whole values).
    pub fn float(f: f64) -> Value {
        if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
            Value::Num(format!("{}", f as i64))
        } else if f.is_finite() {
            Value::Num(format!("{f}"))
        } else {
            Value::Null
        }
    }

    /// `Value::Str` from anything string-like.
    pub fn str(s: impl Into<String>) -> Value {
        Value::Str(s.into())
    }

    /// Build an object from `(key, value)` pairs.
    pub fn obj<const N: usize>(pairs: [(&str, Value); N]) -> Value {
        Value::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }

    /// An object field.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// A string field.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A boolean field.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// An integer field (accepts anything the number text can parse).
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Num(t) => t.parse().ok(),
            _ => None,
        }
    }

    /// A float field.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(t) => t.parse().ok(),
            _ => None,
        }
    }

    /// An array field.
    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(xs) => Some(xs),
            _ => None,
        }
    }

    /// Render to JSON text (`serde_json`'s writer).
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a json::Value always serializes")
    }

    /// Append the JSON text to `out`.
    pub fn write(&self, out: &mut String) {
        out.push_str(&self.to_json());
    }
}

/// Write a JSON string literal (`serde_json`'s escape set: `"` `\` and the
/// control characters, `\uXXXX` for the rest below `0x20`).
pub fn write_str(s: &str, out: &mut String) {
    out.push_str(&serde_json::to_string(s).expect("a string always serializes"));
}

/// Parse JSON text (order preserving, via `serde_json`).
pub fn parse(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Null => s.serialize_unit(),
            Value::Bool(b) => s.serialize_bool(*b),
            // The text is re-read as a number so `serde_json` formats it; a
            // whole number stays an integer, anything else becomes a float.
            Value::Num(text) => {
                if let Ok(n) = text.parse::<i64>() {
                    s.serialize_i64(n)
                } else if let Ok(n) = text.parse::<u64>() {
                    s.serialize_u64(n)
                } else if let Ok(n) = text.parse::<f64>() {
                    s.serialize_f64(n)
                } else {
                    s.serialize_unit()
                }
            }
            Value::Str(text) => s.serialize_str(text),
            Value::Arr(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Value::Obj(pairs) => {
                let mut map = s.serialize_map(Some(pairs.len()))?;
                for (key, value) in pairs {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
        d.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Num(v.to_string()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Num(v.to_string()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Ok(Value::float(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::Str(v.to_string()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::Str(v))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut out = Vec::new();
        while let Some(item) = seq.next_element()? {
            out.push(item);
        }
        Ok(Value::Arr(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        // `MapAccess` yields the keys in document order, so the recovered
        // field order survives the round trip.
        let mut out = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            out.push((key, value));
        }
        Ok(Value::Obj(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_match_serde_json() {
        let mut out = String::new();
        write_str("a\"b\\c\nd\te\u{1}", &mut out);
        assert_eq!(out, r#""a\"b\\c\nd\te\u0001""#);
    }

    #[test]
    fn round_trips_an_object() {
        let v = Value::obj([
            ("t", Value::str("welcome")),
            ("login", Value::str("Ima")),
            ("bold", Value::Bool(true)),
            ("n", Value::int(20u32)),
        ]);
        let text = v.to_json();
        assert_eq!(text, r#"{"t":"welcome","login":"Ima","bold":true,"n":20}"#);
        assert_eq!(parse(&text).unwrap(), v);
    }

    #[test]
    fn parses_nested_and_escaped() {
        let v = parse(r#" { "a" : [1, 2.5, -3e2], "b":{"c":"\u00e9\ud83d\ude00"} } "#).unwrap();
        assert_eq!(v.get("a").unwrap().as_arr().unwrap().len(), 3);
        assert_eq!(v.get("b").unwrap().get("c").unwrap().as_str(), Some("é😀"));
        assert_eq!(v.get("a").unwrap().as_arr().unwrap()[2].as_f64(), Some(-300.0));
    }

    #[test]
    fn rejects_junk() {
        assert!(parse("{}x").is_err());
        assert!(parse("[1,").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn floats_are_written_like_serde() {
        assert_eq!(Value::float(0.75).to_json(), "0.75");
        assert_eq!(Value::float(2.0).to_json(), "2");
        assert_eq!(Value::float(f64::NAN).to_json(), "null");
    }

    #[test]
    fn object_order_survives_the_round_trip() {
        // `serde_json`'s own `Map` would sort these three keys.
        let text = r#"{"zebra":1,"apple":2,"t":3}"#;
        let v = parse(text).unwrap();
        let Value::Obj(pairs) = &v else {
            panic!("not an object");
        };
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["zebra", "apple", "t"]);
        assert_eq!(v.to_json(), text);
    }
}
