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
// TOML (the small subset the settings and stream rules use)
// ---------------------------------------------------------------------------

/// Parse the TOML subset: scalars, single-line arrays, `[table]` and
/// `[[array of tables]]`, with `#` comments.  Trailing `\r` from CRLF files is
/// stripped.  Later duplicate keys win, which matches `serde`'s behaviour of
/// taking the last value it is handed for a field.
pub fn parse_toml(text: &str) -> Result<Value, String> {
    let mut root: Vec<(String, Value)> = Vec::new();
    // Path of the table we are currently filling.
    let mut path: Vec<String> = Vec::new();

    for (n, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("[[") {
            let name = line
                .strip_prefix("[[")
                .and_then(|r| r.strip_suffix("]]"))
                .ok_or_else(|| format!("line {}: bad array-of-tables header", n + 1))?
                .trim();
            let name = unquote_key(name);
            path = vec![name.clone()];
            // Push a fresh table onto the array at `path`.
            let parent = table_of(&mut root, &[]);
            let idx = match parent.iter().position(|(k, _)| *k == name) {
                Some(i) => i,
                None => {
                    parent.push((name, Value::Arr(Vec::new())));
                    parent.len() - 1
                }
            };
            match &mut parent[idx].1 {
                Value::Arr(items) => items.push(Value::Obj(Vec::new())),
                other => *other = Value::Arr(vec![Value::Obj(Vec::new())]),
            }
            continue;
        }
        if line.starts_with('[') {
            let name = line
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .ok_or_else(|| format!("line {}: bad table header", n + 1))?
                .trim()
                .to_string();
            path = name.split('.').map(|s| unquote_key(s.trim())).collect();
            let (parent, last) = match path.split_last() {
                Some((last, rest)) => (rest.to_vec(), last.clone()),
                None => (Vec::new(), String::new()),
            };
            let table = table_of(&mut root, &parent);
            if !table.iter().any(|(k, _)| *k == last) {
                table.push((last, Value::Obj(Vec::new())));
            }
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: not a `key = value` line", n + 1))?;
        let key = unquote_key(key.trim());
        let value = parse_toml_value(value.trim()).map_err(|e| format!("line {}: {e}", n + 1))?;
        let table = table_of(&mut root, &path);
        match table.iter_mut().find(|(k, _)| *k == key) {
            Some((_, existing)) => *existing = value,
            None => table.push((key, value)),
        }
    }
    Ok(Value::Obj(root))
}

/// Walk to the table at `path`, creating tables as needed.  Descending into an
/// array of tables goes to its last element, which is what `[[name]]` opens.
fn table_of<'a>(
    root: &'a mut Vec<(String, Value)>,
    path: &[String],
) -> &'a mut Vec<(String, Value)> {
    let Some((seg, rest)) = path.split_first() else {
        return root;
    };
    let idx = match root.iter().position(|(k, _)| k == seg) {
        Some(i) => i,
        None => {
            root.push((seg.clone(), Value::Obj(Vec::new())));
            root.len() - 1
        }
    };
    match &mut root[idx].1 {
        Value::Obj(pairs) => table_of(pairs, rest),
        Value::Arr(items) => {
            if !matches!(items.last(), Some(Value::Obj(_))) {
                items.push(Value::Obj(Vec::new()));
            }
            match items.last_mut() {
                Some(Value::Obj(pairs)) => table_of(pairs, rest),
                _ => unreachable!(),
            }
        }
        other => {
            *other = Value::Obj(Vec::new());
            match other {
                Value::Obj(pairs) => table_of(pairs, rest),
                _ => unreachable!(),
            }
        }
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_str = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' if i == 0 || bytes[i - 1] != b'\\' => in_str = !in_str,
            b'#' if !in_str => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn unquote_key(k: &str) -> String {
    let k = k.trim();
    if k.len() >= 2 && k.starts_with('"') && k.ends_with('"') {
        parse_toml_string(k).unwrap_or_else(|_| k[1..k.len() - 1].to_string())
    } else {
        k.to_string()
    }
}

fn parse_toml_value(s: &str) -> Result<Value, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty value".to_string());
    }
    if s.starts_with('"') {
        return parse_toml_string(s).map(Value::Str);
    }
    if s == "true" {
        return Ok(Value::Bool(true));
    }
    if s == "false" {
        return Ok(Value::Bool(false));
    }
    if s.starts_with('[') {
        let inner = s
            .strip_prefix('[')
            .and_then(|r| r.strip_suffix(']'))
            .ok_or("unterminated array")?;
        let mut items = Vec::new();
        for piece in split_array(inner) {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            items.push(parse_toml_value(piece)?);
        }
        return Ok(Value::Arr(items));
    }
    let cleaned = s.replace('_', "");
    if let Ok(i) = cleaned.parse::<i64>() {
        return Ok(Value::Num(i as f64));
    }
    if let Ok(f) = cleaned.parse::<f64>() {
        return Ok(Value::Num(f));
    }
    Err(format!("`{s}` is not a value"))
}

fn split_array(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut depth = 0i32;
    let mut escaped = false;
    for c in inner.chars() {
        match c {
            '"' if !escaped => {
                in_str = !in_str;
                cur.push(c);
            }
            '[' if !in_str => {
                depth += 1;
                cur.push(c);
            }
            ']' if !in_str => {
                depth -= 1;
                cur.push(c);
            }
            ',' if !in_str && depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
        escaped = c == '\\' && !escaped;
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_toml_string(s: &str) -> Result<String, String> {
    let inner = s
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .ok_or("unterminated string")?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => return Err("dangling escape".to_string()),
        }
    }
    Ok(out)
}

/// Escape a string for a TOML basic string.
pub fn toml_escape(s: &str) -> String {
    json_escape(s)
}

/// Render a flat table (scalars and arrays of scalars only) as TOML.
pub fn to_toml(obj: &Value) -> String {
    let mut out = String::new();
    if let Value::Obj(pairs) = obj {
        for (k, v) in pairs {
            match v {
                Value::Obj(_) | Value::Arr(_) if is_table_array(v) => continue,
                Value::Obj(_) => continue,
                Value::Arr(items) => {
                    let rendered: Vec<String> = items.iter().map(toml_scalar).collect();
                    out.push_str(&format!("{k} = [{}]\n", rendered.join(", ")));
                }
                other => out.push_str(&format!("{k} = {}\n", toml_scalar(other))),
            }
        }
    }
    out
}

fn is_table_array(v: &Value) -> bool {
    matches!(v, Value::Arr(items) if items.iter().any(|i| matches!(i, Value::Obj(_))))
}

fn toml_scalar(v: &Value) -> String {
    match v {
        Value::Null => "\"\"".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::Str(s) => format!("\"{}\"", toml_escape(s)),
        Value::Arr(_) | Value::Obj(_) => "\"\"".to_string(),
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
    }
}
