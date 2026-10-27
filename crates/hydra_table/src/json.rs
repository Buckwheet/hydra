//! A tiny JSON reader/writer for the roster/teams files.
//!
//! The original serialised `roster::File` / `teams::File` with `serde_json`
//! (`serde_core::ser::SerializeMap::serialize_entry` calls at `0x10129d000` and
//! `0x10129c9b4`).  This reconstruction is `std`-only, so the wire format is
//! reproduced by hand:
//!
//! * a file is `{"schema_version":1,"<map>":{ ... }}` (field order from the
//!   recovered `Serialize` impls, keys in `BTreeMap` order),
//! * strings are escaped with the `serde_json` rules (`\"`, `\\`, `\n`, `\r`,
//!   `\t`, `\u00XX` for other control characters),
//! * `Option<String>` writes `null` when absent (`nickname` is *omitted*
//!   entirely, see [`crate::roster::Entry`]).
//!
//! Only the subset the two files need is parsed: objects, arrays, strings,
//! unsigned numbers, booleans and `null`.  **UNKNOWN**: whether the original
//! tolerated duplicate keys or trailing data — this parser rejects both.

use std::collections::BTreeMap;

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }

    /// The `u32` this value holds, if it is an integer that fits.
    pub fn u32(&self) -> Option<u32> {
        match self {
            Value::Number(n) if *n >= 0.0 && n.fract() == 0.0 && *n <= u32::MAX as f64 => {
                Some(*n as u32)
            }
            _ => None,
        }
    }

    pub fn bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// Parse one JSON document.
pub fn parse(source: &str) -> Result<Value, String> {
    let bytes = source.as_bytes();
    let mut at = 0usize;
    let value = value(bytes, &mut at)?;
    skip_space(bytes, &mut at);
    if at != bytes.len() {
        return Err(format!("trailing data at byte {at}"));
    }
    Ok(value)
}

fn skip_space(b: &[u8], at: &mut usize) {
    while *at < b.len() && matches!(b[*at], b' ' | b'\t' | b'\n' | b'\r') {
        *at += 1;
    }
}

fn expect(b: &[u8], at: &mut usize, want: u8) -> Result<(), String> {
    skip_space(b, at);
    if b.get(*at) == Some(&want) {
        *at += 1;
        Ok(())
    } else {
        Err(format!("expected {:?} at byte {}", want as char, *at))
    }
}

fn value(b: &[u8], at: &mut usize) -> Result<Value, String> {
    skip_space(b, at);
    match b.get(*at) {
        None => Err("unexpected end of input".to_string()),
        Some(b'{') => {
            *at += 1;
            let mut map = BTreeMap::new();
            skip_space(b, at);
            if b.get(*at) == Some(&b'}') {
                *at += 1;
                return Ok(Value::Object(map));
            }
            loop {
                skip_space(b, at);
                let key = string(b, at)?;
                expect(b, at, b':')?;
                let v = value(b, at)?;
                if map.insert(key.clone(), v).is_some() {
                    return Err(format!("duplicate key {key:?}"));
                }
                skip_space(b, at);
                match b.get(*at) {
                    Some(b',') => *at += 1,
                    Some(b'}') => {
                        *at += 1;
                        return Ok(Value::Object(map));
                    }
                    _ => return Err(format!("expected ',' or '}}' at byte {}", *at)),
                }
            }
        }
        Some(b'[') => {
            *at += 1;
            let mut items = Vec::new();
            skip_space(b, at);
            if b.get(*at) == Some(&b']') {
                *at += 1;
                return Ok(Value::Array(items));
            }
            loop {
                items.push(value(b, at)?);
                skip_space(b, at);
                match b.get(*at) {
                    Some(b',') => *at += 1,
                    Some(b']') => {
                        *at += 1;
                        return Ok(Value::Array(items));
                    }
                    _ => return Err(format!("expected ',' or ']' at byte {}", *at)),
                }
            }
        }
        Some(b'"') => Ok(Value::Text(string(b, at)?)),
        Some(b't') if b[*at..].starts_with(b"true") => {
            *at += 4;
            Ok(Value::Bool(true))
        }
        Some(b'f') if b[*at..].starts_with(b"false") => {
            *at += 5;
            Ok(Value::Bool(false))
        }
        Some(b'n') if b[*at..].starts_with(b"null") => {
            *at += 4;
            Ok(Value::Null)
        }
        Some(_) => number(b, at),
    }
}

fn number(b: &[u8], at: &mut usize) -> Result<Value, String> {
    let start = *at;
    if b.get(*at) == Some(&b'-') {
        *at += 1;
    }
    while *at < b.len() && (b[*at].is_ascii_digit() || matches!(b[*at], b'.' | b'e' | b'E' | b'+' | b'-')) {
        *at += 1;
    }
    let text = std::str::from_utf8(&b[start..*at]).map_err(|e| e.to_string())?;
    text.parse::<f64>()
        .map(Value::Number)
        .map_err(|_| format!("bad number {text:?} at byte {start}"))
}

fn string(b: &[u8], at: &mut usize) -> Result<String, String> {
    expect(b, at, b'"')?;
    let mut out = String::new();
    loop {
        match b.get(*at) {
            None => return Err("unterminated string".to_string()),
            Some(b'"') => {
                *at += 1;
                return Ok(out);
            }
            Some(b'\\') => {
                *at += 1;
                match b.get(*at) {
                    Some(b'"') => out.push('"'),
                    Some(b'\\') => out.push('\\'),
                    Some(b'/') => out.push('/'),
                    Some(b'b') => out.push('\u{8}'),
                    Some(b'f') => out.push('\u{c}'),
                    Some(b'n') => out.push('\n'),
                    Some(b'r') => out.push('\r'),
                    Some(b't') => out.push('\t'),
                    Some(b'u') => {
                        let hex = b
                            .get(*at + 1..*at + 5)
                            .ok_or_else(|| "short \\u escape".to_string())?;
                        let hex = std::str::from_utf8(hex).map_err(|e| e.to_string())?;
                        let code = u32::from_str_radix(hex, 16)
                            .map_err(|_| format!("bad \\u escape {hex:?}"))?;
                        out.push(char::from_u32(code).ok_or_else(|| format!("bad code {code:#x}"))?);
                        *at += 4;
                    }
                    other => return Err(format!("bad escape {other:?}")),
                }
                *at += 1;
            }
            Some(_) => {
                let rest = std::str::from_utf8(&b[*at..]).map_err(|e| e.to_string())?;
                let ch = rest.chars().next().ok_or_else(|| "empty string".to_string())?;
                out.push(ch);
                *at += ch.len_utf8();
            }
        }
    }
}

/// Append `text` as a JSON string, quotes included.
pub fn write_text(out: &mut String, text: &str) {
    out.push('"');
    for ch in text.chars() {
        match ch {
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
    out.push('"');
}

/// Write `{"schema_version":N,"<key>":{ ... }}`.
///
/// `body` receives the output buffer and must write the object body without the
/// surrounding braces.
pub fn write_file<N, F>(out: &mut String, schema_version: N, key: &str, body: F)
where
    N: std::fmt::Display,
    F: FnOnce(&mut String),
{
    out.push('{');
    out.push_str("\"schema_version\":");
    out.push_str(&schema_version.to_string());
    out.push(',');
    write_text(out, key);
    out.push(':');
    out.push('{');
    body(out);
    out.push('}');
    out.push('}');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_recovered_file_shape() {
        let text = r#"{"schema_version":1,"characters":{"gs4:aelith":{"character":"Aelith","account":"main","game_code":"GS4","favourite":true,"nickname":"Aely"}}}"#;
        let v = parse(text).expect("valid json");
        let top = v.object().expect("object");
        assert_eq!(top["schema_version"].u32(), Some(1));
        let entries = top["characters"].object().expect("map");
        let entry = &entries["gs4:aelith"];
        let entry = entry.object().expect("entry");
        assert_eq!(entry["character"].text(), Some("Aelith"));
        assert_eq!(entry["account"].text(), Some("main"));
        assert_eq!(entry["game_code"].text(), Some("GS4"));
        assert_eq!(entry["favourite"].bool(), Some(true));
        assert_eq!(entry["nickname"].text(), Some("Aely"));
    }

    #[test]
    fn rejects_trailing_data_and_duplicate_keys() {
        assert!(parse("{} junk").is_err());
        assert!(parse(r#"{"a":1,"a":2}"#).is_err());
    }

    #[test]
    fn writes_serde_json_escapes() {
        let mut out = String::new();
        write_text(&mut out, "tab\there \"quoted\" back\\slash\n");
        assert_eq!(out, r#""tab\there \"quoted\" back\\slash\n""#);
    }

    #[test]
    fn round_trips_a_file_header() {
        let mut out = String::new();
        write_file(&mut out, 1u32, "characters", |_| {});
        assert_eq!(out, r#"{"schema_version":1,"characters":{}}"#);
    }
}
