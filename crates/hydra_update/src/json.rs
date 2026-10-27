//! A small JSON reader for the one document this crate parses: GitHub's
//! `GET /repos/{owner}/{repo}/releases` array.
//!
//! The original used `serde_json::from_str::<Vec<release::Listed>>` — the
//! monomorphization is visible as `serde_json::de::from_trait`
//! (0x1010a0f70, called from `hydra_update::release::newest` @0x1010adfc0) and
//! the two `__Visitor::expecting` globals at 0x1010aed0c / 0x1010aed20.
//! `serde_json` is an external crate, so the reader is hand-rolled here.  It
//! parses the whole grammar (serde_json would too) and keeps unknown keys in
//! the tree; the mapping to `Listed`/`ListedFile` happens in `release`.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    /// Numbers are kept as the source text; the release schema only needs
    /// `size` (a `u64`) and ignores the rest.
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(text) => text.parse::<u64>().ok(),
            _ => None,
        }
    }

    /// What `Json` is, for a serde-shaped error message.
    pub fn kind(&self) -> &'static str {
        match self {
            Json::Null => "null",
            Json::Bool(_) => "a boolean",
            Json::Number(_) => "a number",
            Json::String(_) => "a string",
            Json::Array(_) => "a sequence",
            Json::Object(_) => "a map",
        }
    }
}

/// Parse one JSON document.  The error text follows the shape of
/// `serde_json::Error`'s `Display` ("expected value at line 1 column 1"),
/// which `newest` wraps in `"The releases could not be read: {}"`.
pub fn parse(text: &str) -> Result<Json, String> {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    let value = value(bytes, &mut at)?;
    whitespace(bytes, &mut at);
    if at != bytes.len() {
        return Err(error_at(bytes, at, "trailing characters"));
    }
    Ok(value)
}

fn error_at(bytes: &[u8], at: usize, what: &str) -> String {
    let mut line = 1usize;
    let mut column = 1usize;
    for (i, b) in bytes.iter().enumerate() {
        if i >= at {
            break;
        }
        if *b == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    format!("{what} at line {line} column {column}")
}

fn whitespace(bytes: &[u8], at: &mut usize) {
    while *at < bytes.len() && matches!(bytes[*at], b' ' | b'\t' | b'\n' | b'\r') {
        *at += 1;
    }
}

fn value(bytes: &[u8], at: &mut usize) -> Result<Json, String> {
    whitespace(bytes, at);
    let here = *at;
    match bytes.get(*at) {
        None => Err(error_at(bytes, here, "EOF while parsing a value")),
        Some(b'{') => object(bytes, at),
        Some(b'[') => array(bytes, at),
        Some(b'"') => Ok(Json::String(string(bytes, at)?)),
        Some(b't') => literal(bytes, at, "true", Json::Bool(true)),
        Some(b'f') => literal(bytes, at, "false", Json::Bool(false)),
        Some(b'n') => literal(bytes, at, "null", Json::Null),
        Some(b'-') | Some(b'0'..=b'9') => number(bytes, at),
        Some(_) => Err(error_at(bytes, here, "expected value")),
    }
}

fn literal(bytes: &[u8], at: &mut usize, word: &str, value: Json) -> Result<Json, String> {
    if bytes[*at..].starts_with(word.as_bytes()) {
        *at += word.len();
        Ok(value)
    } else {
        Err(error_at(bytes, *at, "expected value"))
    }
}

fn object(bytes: &[u8], at: &mut usize) -> Result<Json, String> {
    *at += 1; // '{'
    let mut map = BTreeMap::new();
    whitespace(bytes, at);
    if bytes.get(*at) == Some(&b'}') {
        *at += 1;
        return Ok(Json::Object(map));
    }
    loop {
        whitespace(bytes, at);
        if bytes.get(*at) != Some(&b'"') {
            return Err(error_at(bytes, *at, "key must be a string"));
        }
        let key = string(bytes, at)?;
        whitespace(bytes, at);
        if bytes.get(*at) != Some(&b':') {
            return Err(error_at(bytes, *at, "expected `:`"));
        }
        *at += 1;
        let item = value(bytes, at)?;
        map.insert(key, item);
        whitespace(bytes, at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b'}') => {
                *at += 1;
                return Ok(Json::Object(map));
            }
            _ => return Err(error_at(bytes, *at, "expected `,` or `}`")),
        }
    }
}

fn array(bytes: &[u8], at: &mut usize) -> Result<Json, String> {
    *at += 1; // '['
    let mut items = Vec::new();
    whitespace(bytes, at);
    if bytes.get(*at) == Some(&b']') {
        *at += 1;
        return Ok(Json::Array(items));
    }
    loop {
        items.push(value(bytes, at)?);
        whitespace(bytes, at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b']') => {
                *at += 1;
                return Ok(Json::Array(items));
            }
            _ => return Err(error_at(bytes, *at, "expected `,` or `]`")),
        }
    }
}

fn number(bytes: &[u8], at: &mut usize) -> Result<Json, String> {
    let start = *at;
    if bytes.get(*at) == Some(&b'-') {
        *at += 1;
    }
    while matches!(bytes.get(*at), Some(b'0'..=b'9') | Some(b'.') | Some(b'e') | Some(b'E')
        | Some(b'+') | Some(b'-'))
    {
        *at += 1;
    }
    let text = std::str::from_utf8(&bytes[start..*at]).map_err(|_| error_at(bytes, start, "invalid number"))?;
    if text.is_empty() || text == "-" {
        return Err(error_at(bytes, start, "expected value"));
    }
    Ok(Json::Number(text.to_string()))
}

fn string(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    *at += 1; // opening quote
    let mut out = String::new();
    loop {
        let here = *at;
        match bytes.get(*at) {
            None => return Err(error_at(bytes, here, "EOF while parsing a string")),
            Some(b'"') => {
                *at += 1;
                return Ok(out);
            }
            Some(b'\\') => {
                *at += 1;
                let esc = *bytes
                    .get(*at)
                    .ok_or_else(|| error_at(bytes, here, "EOF while parsing a string"))?;
                *at += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{8}'),
                    b'f' => out.push('\u{c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let first = hex4(bytes, at)?;
                        let ch = if (0xD800..0xDC00).contains(&first) {
                            // A surrogate pair: "\uD83D\uDE00".
                            if bytes.get(*at) != Some(&b'\\') || bytes.get(*at + 1) != Some(&b'u') {
                                return Err(error_at(bytes, here, "unexpected end of hex escape"));
                            }
                            *at += 2;
                            let second = hex4(bytes, at)?;
                            if !(0xDC00..0xE000).contains(&second) {
                                return Err(error_at(bytes, here, "lone leading surrogate in hex escape"));
                            }
                            let combined = 0x10000
                                + (((first - 0xD800) as u32) << 10)
                                + (second - 0xDC00) as u32;
                            char::from_u32(combined)
                                .ok_or_else(|| error_at(bytes, here, "invalid unicode code point"))?
                        } else {
                            char::from_u32(first as u32)
                                .ok_or_else(|| error_at(bytes, here, "invalid unicode code point"))?
                        };
                        out.push(ch);
                    }
                    _ => return Err(error_at(bytes, here, "invalid escape")),
                }
            }
            Some(b) if *b < 0x20 => {
                return Err(error_at(bytes, here, "control character while parsing a string"))
            }
            Some(_) => {
                // Copy the whole UTF-8 character.
                let rest = &bytes[*at..];
                let width = match rest[0] {
                    0x00..=0x7f => 1,
                    0xc0..=0xdf => 2,
                    0xe0..=0xef => 3,
                    _ => 4,
                };
                let end = (*at + width).min(bytes.len());
                let piece = std::str::from_utf8(&bytes[*at..end])
                    .map_err(|_| error_at(bytes, here, "invalid unicode code point"))?;
                out.push_str(piece);
                *at = end;
            }
        }
    }
}

fn hex4(bytes: &[u8], at: &mut usize) -> Result<u16, String> {
    let mut value = 0u16;
    for _ in 0..4 {
        let b = *bytes
            .get(*at)
            .ok_or_else(|| error_at(bytes, *at, "EOF while parsing a string"))?;
        let digit = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return Err(error_at(bytes, *at, "invalid escape")),
        };
        value = (value << 4) | digit as u16;
        *at += 1;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_release_shape() {
        let text = r#"[
            {"tag_name": "v0.1.0-beta.8", "draft": false,
             "html_url": "https://example.invalid/hydra/v0.1.0-beta.8",
             "assets": [
                {"name": "hydra-arm64", "browser_download_url": "https://example.invalid/a", "size": 12345}
             ]},
            {"tag_name": "v0.1.0-beta.7", "draft": true, "html_url": "", "assets": []}
        ]"#;
        let root = parse(text).expect("parses");
        let items = root.as_array().expect("an array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("tag_name").and_then(Json::as_str), Some("v0.1.0-beta.8"));
        assert_eq!(items[0].get("draft").and_then(Json::as_bool), Some(false));
        assert_eq!(items[1].get("draft").and_then(Json::as_bool), Some(true));
        let assets = items[0].get("assets").and_then(Json::as_array).unwrap();
        assert_eq!(assets[0].get("size").and_then(Json::as_u64), Some(12345));
    }

    #[test]
    fn reads_escapes_and_unknown_keys() {
        let root = parse(r#"{"a":"x\u00e9\n\"y\"","b":[null,true,-2.5e3],"c":{"d":1}}"#).unwrap();
        assert_eq!(root.get("a").and_then(Json::as_str), Some("xé\n\"y\""));
        assert_eq!(root.get("b").and_then(Json::as_array).map(<[Json]>::len), Some(3));
        assert_eq!(
            root.get("c").and_then(|c| c.get("d")).and_then(Json::as_u64),
            Some(1)
        );
    }

    #[test]
    fn surrogate_pairs() {
        let root = parse(r#""\ud83d\ude00""#).unwrap();
        assert_eq!(root.as_str(), Some("😀"));
    }

    #[test]
    fn rejects_broken_documents() {
        for bad in ["", "[", "{\"a\":}", "{\"a\" 1}", "nul", "[1,]", "\"unterminated", "{} x"] {
            assert!(parse(bad).is_err(), "{bad:?} should not parse");
        }
        let err = parse("[1,]").unwrap_err();
        assert!(err.contains("line 1 column"), "{err}");
    }
}
