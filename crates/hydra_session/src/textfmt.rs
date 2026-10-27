//! The JSON / TOML document type the file stores share.
//!
//! The original crate used `serde_json` (its `__Visitor` impls are still in
//! the symbol table, e.g.
//! `<hydra_session::notes::_::<impl Deserialize for notes::File>::deserialize::__Visitor
//! as Visitor>::expecting @0x101a54fb4`) and `toml` / `toml_edit` for the
//! settings and stream rules.  **Phase 1** reconnected both:
//!
//! * [`parse_json`] / [`to_json`] are `serde_json::from_str` /
//!   `serde_json::to_string_pretty` over [`Value`], which implements
//!   [`serde::Serialize`] and [`serde::Deserialize`] itself so that the key
//!   *order* of a document survives a round trip (`serde_json::Value`'s map is
//!   a `BTreeMap` unless `preserve_order` is on, and turning that feature on
//!   would reorder every other crate's output);
//! * [`parse_toml`] is `toml::from_str` over `toml::Value`, converted into
//!   [`Value`]; [`to_toml`] renders a flat table (the shape the settings use)
//!   and `stream_rules` writes its `[[rules]]` documents with `toml_edit`.
//!
//! [`Value`] is the crate's own document type, not `serde_json::Value`: every
//! file store in this crate (`notes`, `alias`, `character_store`,
//! `settings_store`, `player_log::links`, `actor`) reads and writes it, and
//! several of them expose it in their public API (`SettingsFile::sections`).
//! Unknown keys are kept but never interpreted, which mirrors how the original
//! ignored fields it did not know about.  Departures are recorded in
//! `NOTES.md` §"Phase 1 reconnection".

use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// A parsed JSON or TOML value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// any number; JSON has one number type, TOML integers land here too
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// insertion ordered, so a rewrite keeps the original key order
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::Num(n) if *n >= 0.0 && n.fract() == 0.0 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    /// Look a key up in an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// A string member of an object, or `""`.
    pub fn str_of(&self, key: &str) -> String {
        self.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    /// A string array member of an object, or empty.
    pub fn strings_of(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A boolean member of an object, or `false`.
    pub fn bool_of(&self, key: &str) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    /// An integer member of an object, or `None`.
    pub fn u64_of(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(Value::as_u64)
    }
}


// ---------------------------------------------------------------------------
// JSON — `serde_json`, over this crate's own document type
// ---------------------------------------------------------------------------

/// Parse a JSON document with `serde_json`.
///
/// The error is serde's own message, which carries the line and column
/// (`expected `,` or `}` at line 3 column 7`) instead of the byte offset the
/// hand-written parser used to report.  Callers wrap it in their own
/// `"{} cannot be read: {}"` shape, as before.
pub fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Render a value as JSON with `serde_json`, pretty-printed with two spaces.
pub fn to_json(v: &Value) -> String {
    // `Value`'s `Serialize` impl cannot fail for a finite document, and a
    // non-finite float is written as `null`; an empty string keeps `to_json`
    // total, which is what every caller assumes.
    serde_json::to_string_pretty(v).unwrap_or_default()
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Null => s.serialize_unit(),
            Value::Bool(b) => s.serialize_bool(*b),
            Value::Num(n) => {
                // JSON has one number type; the original wrote whole numbers
                // without a fractional part (`"schema_version": 2`), which is
                // also what `serde_json` does for integers, so an integral
                // `f64` is emitted through the integer path.
                if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 {
                    s.serialize_i64(*n as i64)
                } else {
                    s.serialize_f64(*n)
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
                for (key, item) in pairs {
                    map.serialize_entry(key, item)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
        d.deserialize_any(Document)
    }
}

/// The visitor behind `Value::deserialize`: it keeps object members in the
/// order the document lists them, which `serde_json::Value` does not (its map
/// is a `BTreeMap` unless the `preserve_order` feature is on).
struct Document;

impl<'de> Visitor<'de> for Document {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
        Ok(Value::Bool(b))
    }

    fn visit_i64<E>(self, n: i64) -> Result<Value, E> {
        Ok(Value::Num(n as f64))
    }

    fn visit_u64<E>(self, n: u64) -> Result<Value, E> {
        Ok(Value::Num(n as f64))
    }

    fn visit_f64<E>(self, n: f64) -> Result<Value, E> {
        Ok(Value::Num(n))
    }

    fn visit_str<E>(self, s: &str) -> Result<Value, E> {
        Ok(Value::Str(s.to_string()))
    }

    fn visit_string<E>(self, s: String) -> Result<Value, E> {
        Ok(Value::Str(s))
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        Value::deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element::<Value>()? {
            items.push(item);
        }
        Ok(Value::Arr(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut pairs = Vec::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((key, item)) = map.next_entry::<String, Value>()? {
            pairs.push((key, item));
        }
        Ok(Value::Obj(pairs))
    }
}
/// Escape a string for JSON output.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// TOML — the `toml` crate for reading, `toml_edit` for writing
// ---------------------------------------------------------------------------

/// Parse a TOML document with the `toml` crate.
///
/// The document is converted into [`Value`] so that the file stores keep one
/// document type: tables become [`Value::Obj`], arrays [`Value::Arr`], integers
/// and floats [`Value::Num`], and a TOML datetime is kept as its rendered
/// [`Value::Str`] (no store in this crate reads a datetime back).
pub fn parse_toml(text: &str) -> Result<Value, String> {
    let doc: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(from_toml(doc))
}

fn from_toml(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::Str(s),
        toml::Value::Integer(i) => Value::Num(i as f64),
        toml::Value::Float(f) => Value::Num(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::Str(d.to_string()),
        toml::Value::Array(items) => Value::Arr(items.into_iter().map(from_toml).collect()),
        toml::Value::Table(table) => {
            Value::Obj(table.into_iter().map(|(k, v)| (k, from_toml(v))).collect())
        }
    }
}

/// Escape a string for a TOML basic string.
///
/// `toml_edit` escapes what it writes itself (see [`to_toml`]); this helper is
/// kept for callers that render a single TOML scalar by hand, and escapes the
/// same set of characters `serde_json` does.
pub fn toml_escape(s: &str) -> String {
    json_escape(s)
}

/// Render a flat table (scalars and arrays of scalars only) as TOML with
/// `toml_edit`.
///
/// Nested tables and arrays of tables are skipped, as before: they are the
/// shapes `stream_rules` renders itself, and the settings file is JSON.
pub fn to_toml(obj: &Value) -> String {
    let mut doc = toml_edit::DocumentMut::new();
    if let Value::Obj(pairs) = obj {
        for (k, v) in pairs {
            match v {
                Value::Obj(_) => continue,
                Value::Arr(items) => {
                    if is_table_array(v) {
                        continue;
                    }
                    let mut arr = toml_edit::Array::new();
                    for item in items {
                        arr.push(toml_value(item));
                    }
                    doc[k] = toml_edit::Item::Value(toml_edit::Value::Array(arr));
                }
                other => {
                    doc[k] = toml_edit::Item::Value(toml_value(other));
                }
            }
        }
    }
    doc.to_string()
}

fn is_table_array(v: &Value) -> bool {
    matches!(v, Value::Arr(items) if items.iter().any(|i| matches!(i, Value::Obj(_))))
}

/// One scalar as a `toml_edit` value.  An integral `f64` becomes a TOML
/// integer, which is how the crate has always written `window = 30`.
fn toml_value(v: &Value) -> toml_edit::Value {
    match v {
        Value::Null => toml_edit::Value::from(""),
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Num(n) if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15 => {
            toml_edit::Value::from(*n as i64)
        }
        Value::Num(n) => toml_edit::Value::from(*n),
        Value::Str(s) => toml_edit::Value::from(s.as_str()),
        Value::Arr(_) | Value::Obj(_) => toml_edit::Value::from(""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trips_a_notes_shaped_document() {
        let text = r#"{
          "schema": 2,
          "notes": [
            {"key": "0x1a2b", "text": "a cart\nmore", "when": "2024-05-01T10:00:00"}
          ]
        }"#;
        let v = parse_json(text).unwrap();
        assert_eq!(v.u64_of("schema"), Some(2));
        let notes = v.get("notes").unwrap().as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].str_of("key"), "0x1a2b");
        assert_eq!(notes[0].str_of("text"), "a cart\nmore");
        let again = parse_json(&to_json(&v)).unwrap();
        assert_eq!(again, v);
    }

    #[test]
    fn json_reports_a_bad_escape_with_serde_s_position() {
        // The hand-written parser reported a byte offset; `serde_json` reports
        // a line and column.  The stored message shape is unchanged.
        let e = parse_json(r#"{"a": "\q"}"#).unwrap_err();
        assert!(e.contains("invalid escape"), "{e}");
        assert!(e.contains("line 1 column 9"), "{e}");
        // A truncated document is refused with a position too, not a panic.
        let e = parse_json(r#"{"a": "b"#).unwrap_err();
        assert!(e.contains("line 1"), "{e}");
    }

    #[test]
    fn serde_keeps_member_order_through_a_round_trip() {
        // This is the property `serde_json::Value` does not have without the
        // `preserve_order` feature, and the settings file depends on it.
        let v = parse_json(r#"{"zeta": 1, "alpha": 2, "mu": 3}"#).unwrap();
        let keys: Vec<&str> = match &v {
            Value::Obj(pairs) => pairs.iter().map(|(k, _)| k.as_str()).collect(),
            other => panic!("{other:?}"),
        };
        assert_eq!(keys, ["zeta", "alpha", "mu"]);
        assert_eq!(parse_json(&to_json(&v)).unwrap(), v);
    }

    #[test]
    fn serde_json_parses_real_documents_the_store_writes() {
        // A whole settings-shaped document, through `serde_json` directly: the
        // wrapper is a paper-thin delegation now.
        let text = r#"{
          "schema_version": 1,
          "instance": "gemstone",
          "character": "Aelith",
          "sections": {"lich": {"path": "/usr/bin/ruby"}, "symbol": "."}
        }"#;
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(v.u64_of("schema_version"), Some(1));
        let sections = v.get("sections").unwrap();
        assert_eq!(sections.str_of("symbol"), ".");
        assert_eq!(sections.get("lich").unwrap().str_of("path"), "/usr/bin/ruby");
        // Whole numbers stay integral on the way out, as the original wrote
        // them, and nested tables are indented two spaces.
        let written = to_json(&v);
        assert!(written.contains("\"schema_version\": 1"), "{written}");
        assert!(written.contains("\n  \"sections\": {"), "{written}");
        assert_eq!(parse_json(&written).unwrap(), v);
    }

    #[test]
    fn json_numbers_and_unicode_escapes() {
        let v = parse_json(r#"[1, -2.5, true, null, "\u00e9", "\ud83d\ude00"]"#).unwrap();
        let a = v.as_array().unwrap();
        assert_eq!(a[0].as_u64(), Some(1));
        assert_eq!(a[1], Value::Num(-2.5));
        assert_eq!(a[2].as_bool(), Some(true));
        assert_eq!(a[3], Value::Null);
        assert_eq!(a[4].as_str(), Some("é"));
        assert_eq!(a[5].as_str(), Some("😀"));
    }

    #[test]
    fn toml_scalars_tables_and_arrays_of_tables() {
        let text = r#"
            # a comment
            symbol = "."
            keep_log = true
            window = 30

            [lich]
            path = "/usr/bin/ruby"
            args = ["-e", "x"]

            [[rules]]
            name = "say"
            regex = "^\\w+ says"

            [[rules]]
            name = "fight"
            regex = "attacks"
        "#;
        let v = parse_toml(text).unwrap();
        assert_eq!(v.str_of("symbol"), ".");
        assert!(v.bool_of("keep_log"));
        assert_eq!(v.u64_of("window"), Some(30));
        let lich = v.get("lich").unwrap();
        assert_eq!(lich.str_of("path"), "/usr/bin/ruby");
        assert_eq!(lich.strings_of("args"), vec!["-e", "x"]);
        let rules = v.get("rules").unwrap().as_array().unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].str_of("name"), "say");
        assert_eq!(rules[1].str_of("regex"), "attacks");
    }

    #[test]
    fn toml_ignores_comments_inside_strings() {
        let v = parse_toml("a = \"x # y\" # gone\n").unwrap();
        assert_eq!(v.str_of("a"), "x # y");
    }

    #[test]
    fn toml_writer_renders_scalars_and_arrays() {
        let v = Value::Obj(vec![
            ("sym".to_string(), Value::Str(".".to_string())),
            ("n".to_string(), Value::Num(3.0)),
            ("b".to_string(), Value::Bool(false)),
            (
                "arr".to_string(),
                Value::Arr(vec![Value::Str("a".to_string()), Value::Str("b".to_string())]),
            ),
        ]);
        assert_eq!(to_toml(&v), "sym = \".\"\nn = 3\nb = false\narr = [\"a\", \"b\"]\n");
        // and the toml crate reads back what toml_edit wrote.  `toml`'s table is
        // a `BTreeMap` (its `preserve_order` feature is off in the workspace
        // pins), so the re-read document is key-sorted: compare by key.
        let back = parse_toml(&to_toml(&v)).unwrap();
        assert_eq!(back.str_of("sym"), ".");
        assert_eq!(back.u64_of("n"), Some(3));
        assert!(!back.bool_of("b"));
        assert_eq!(back.strings_of("arr"), vec!["a", "b"]);
    }

    #[test]
    fn the_toml_crate_brings_the_rest_of_the_syntax() {
        // Multi-line strings, dotted keys, inline tables, dates and
        // underscores in integers — none of which the hand parser read.
        let text = "title = \"\"\"\nline one\nline two\n\"\"\"\na.b.c = 1\ninline = { x = 1, y = \"two\" }\nwhen = 2024-05-01T10:00:00Z\nbig = 1_000_000\n";
        let v = parse_toml(text).unwrap();
        assert_eq!(v.str_of("title"), "line one\nline two\n");
        assert_eq!(
            v.get("a").unwrap().get("b").unwrap().get("c").unwrap().as_u64(),
            Some(1)
        );
        assert_eq!(v.get("inline").unwrap().str_of("y"), "two");
        assert_eq!(v.str_of("when"), "2024-05-01T10:00:00Z");
        assert_eq!(v.u64_of("big"), Some(1_000_000));
        // A syntax error carries the crate's own line/column message.
        let e = parse_toml("a = \n").unwrap_err();
        assert!(e.contains("line 1"), "{e}");
    }
}
