//! A minimal JSON reader/writer.
//!
//! **`std`-only departure.** The original crate handed `travel.json` to
//! `serde_json` (`serde_json::de::from_trait` at `0x101537760`,
//! `serde_json::ser::Serializer` inlined in `file::Shared::serialize`).
//! No external crates are allowed in this workspace, so `hydra_travel` carries
//! this small JSON subset instead. It accepts exactly what the recovered file
//! format uses: objects, arrays, strings, numbers, `true`/`false`/`null`, with
//! whitespace between tokens. Everything the serializer needs to emit
//! (`{}`, `[]`, quoted strings with the six standard escapes, integers and
//! floats) round-trips.
//!
//! The value type mirrors `serde_json::Value`'s shape closely enough that
//! [`crate::file::Spot::memories`] can hold arbitrary remembered data — the
//! original stored those as `BTreeMap<String, serde_json::Value>`
//! (established from `drop_in_place<hydra_travel::file::Spot>`, which drops a
//! `BTreeMap<String, serde_json::Value>` and a `BTreeMap<String, Vec<u32>>`).

use std::collections::BTreeMap;
use std::fmt;

/// A JSON value.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    /// `null`; also the `Default`, matching `serde_json::Value::Null`.
    #[default]
    Null,
    Bool(bool),
    /// Numbers are kept as `f64`. `serde_json` keeps integers exactly; the
    /// travel file only stores small room numbers and seconds, which `f64`
    /// represents exactly.
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn string(s: impl Into<String>) -> Value {
        Value::String(s.into())
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Value::Number(n) if *n >= 0.0 && *n <= u32::MAX as f64 && n.fract() == 0.0 => {
                Some(*n as u32)
            }
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) if n.fract() == 0.0 => Some(*n as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// `value[key]`, like `serde_json::Value`'s `Index` impl.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object().and_then(|o| o.get(key))
    }

    /// The JSON text.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_to(&mut out);
        out
    }

    fn write_to(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    out.push_str(&format!("{}", *n as i64));
                } else {
                    out.push_str(&format!("{}", n));
                }
            }
            Value::String(s) => write_str(s, out),
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_to(out);
                }
                out.push(']');
            }
            Value::Object(map) => {
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(k, out);
                    out.push(':');
                    v.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

/// Write `s` as a JSON string literal with `serde_json`'s escape set
/// (`\\ \" \n \r \t` and `\u00XX` for the remaining control characters).
pub fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
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

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.write())
    }
}

/// A parse failure.
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub message: String,
    /// Byte offset into the input.
    pub at: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.at)
    }
}

impl std::error::Error for Error {}

/// Parse a JSON document.
pub fn parse(text: &str) -> Result<Value, Error> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        at: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.at != p.bytes.len() {
        return Err(p.err("trailing characters"));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, message: &str) -> Error {
        Error {
            message: message.to_string(),
            at: self.at,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn ws(&mut self) {
        while let Some(b) = self.peek() {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.at += 1;
            } else {
                break;
            }
        }
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value, Error> {
        match self.peek() {
            None => Err(self.err("eof while parsing a value")),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(_) => self.number(),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, Error> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.err("expected a literal"))
        }
    }

    fn object(&mut self) -> Result<Value, Error> {
        self.eat(b'{');
        let mut map = BTreeMap::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Value::Object(map));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if !self.eat(b':') {
                return Err(self.err("expected `:`"));
            }
            self.ws();
            let value = self.value()?;
            map.insert(key, value);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value::Object(map));
            }
            return Err(self.err("expected `,` or `}`"));
        }
    }

    fn array(&mut self) -> Result<Value, Error> {
        self.eat(b'[');
        let mut items = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Value::Array(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value::Array(items));
            }
            return Err(self.err("expected `,` or `]`"));
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        if !self.eat(b'"') {
            return Err(self.err("expected a string"));
        }
        let mut out = String::new();
        loop {
            let Some(b) = self.peek() else {
                return Err(self.err("eof inside a string"));
            };
            self.at += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err(self.err("eof inside an escape"));
                    };
                    self.at += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let ch = if (0xd800..0xdc00).contains(&hi) {
                                // Surrogate pair.
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(self.err("unpaired surrogate"));
                                }
                                let lo = self.hex4()?;
                                0x10000 + ((hi - 0xd800) << 10) + (lo.wrapping_sub(0xdc00))
                            } else {
                                hi
                            };
                            out.push(char::from_u32(ch).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(self.err("unknown escape")),
                    }
                }
                _ => {
                    // Re-decode UTF-8 from the original bytes.
                    let start = self.at - 1;
                    let len = utf8_len(b);
                    let end = start + len;
                    if end > self.bytes.len() {
                        return Err(self.err("truncated utf-8"));
                    }
                    let s = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|_| self.err("bad utf-8"))?;
                    out.push_str(s);
                    self.at = end;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        if self.at + 4 > self.bytes.len() {
            return Err(self.err("truncated \\u escape"));
        }
        let mut n = 0u32;
        for _ in 0..4 {
            let b = self.bytes[self.at];
            let d = match b {
                b'0'..=b'9' => (b - b'0') as u32,
                b'a'..=b'f' => (b - b'a' + 10) as u32,
                b'A'..=b'F' => (b - b'A' + 10) as u32,
                _ => return Err(self.err("bad hex digit")),
            };
            n = n * 16 + d;
            self.at += 1;
        }
        Ok(n)
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-'
            {
                self.at += 1;
            } else {
                break;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.err("bad number"))?;
        text.parse::<f64>()
            .map(Value::Number)
            .map_err(|_| self.err("bad number"))
    }
}

fn utf8_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_recovered_travel_shape() {
        let text = r#"{"schema_version":3,"characters":{"Aelith":{"last_room":10862,"memories":{},"targets":{"den":[284,300]}}},"instabilities":{},"memories":{}}"#;
        let value = parse(text).unwrap();
        assert_eq!(value.get("schema_version").unwrap().as_u32(), Some(3));
        let spot = value
            .get("characters")
            .and_then(|c| c.get("Aelith"))
            .unwrap();
        assert_eq!(spot.get("last_room").unwrap().as_u32(), Some(10862));
        let den = spot.get("targets").and_then(|t| t.get("den")).unwrap();
        let rooms: Vec<u32> = den
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u32().unwrap())
            .collect();
        assert_eq!(rooms, vec![284, 300]);
        assert_eq!(parse(&value.write()).unwrap(), value);
    }

    #[test]
    fn escapes_round_trip() {
        let value = Value::string("a\"b\\c\nd\te\u{1}");
        assert_eq!(value.write(), r#""a\"b\\c\nd\te\u0001""#);
        assert_eq!(parse(&value.write()).unwrap(), value);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("nul").is_err());
    }
}
