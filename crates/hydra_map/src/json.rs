//! A very small JSON reader/writer.
//!
//! **`std`-only departure.** The original `scene.rs` and `overrides.rs` used
//! `serde` + `serde_json` (`serde_json::de::from_trait`, `serde_json::ser::
//! format_escaped_str`, `serde_json::ser::Compound::end` are all still in the
//! binary). Since external crates are not available here, this module carries
//! the subset those two files need: objects, arrays, strings (with the six
//! mandatory escapes plus `\uXXXX`), integers, floats, `true`/`false`/`null`.
//!
//! The *field names and types* of the wire format are the recovered ones; only
//! the derive machinery is replaced. Numbers keep their source text so a
//! round-trip of an unmodified document is byte-identical (serde_json does the
//! same for integers).

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// The original text of the number, so `1`, `1.0` and `1e3` survive.
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) => n.parse().ok(),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => n.parse().ok(),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
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
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?.get(key)
    }
}

/// The error text follows serde_json's wording, which is what
/// `scene.rs`/`overrides.rs` surfaced to the user through `Display`.
#[derive(Clone, Debug, PartialEq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

type Result<T> = std::result::Result<T, Error>;

pub fn parse(text: &str) -> Result<Value> {
    let bytes = text.as_bytes();
    let mut p = Parser { b: bytes, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(Error("trailing characters".into()));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Value> {
        match self.peek() {
            None => Err(Error("EOF while parsing a value".into())),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => Err(Error("expected value".into())),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(Error("expected value".into()))
        }
    }

    fn number(&mut self) -> Result<Value> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i]).unwrap_or("");
        if text.is_empty() || text == "-" {
            return Err(Error("expected value".into()));
        }
        Ok(Value::Number(text.to_string()))
    }

    fn string(&mut self) -> Result<String> {
        if !self.eat(b'"') {
            return Err(Error("expected string".into()));
        }
        let mut out = String::new();
        loop {
            let Some(c) = self.peek() else {
                return Err(Error("EOF while parsing a string".into()));
            };
            self.i += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let Some(e) = self.peek() else {
                        return Err(Error("EOF while parsing a string".into()));
                    };
                    self.i += 1;
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
                            let ch = if (0xD800..0xDC00).contains(&hi) {
                                if !(self.eat(b'\\') && self.eat(b'u')) {
                                    return Err(Error("unexpected end of hex escape".into()));
                                }
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(Error("lone leading surrogate in hex escape".into()));
                                }
                                let cp = 0x1_0000 + (((hi - 0xD800) as u32) << 10)
                                    + (lo - 0xDC00) as u32;
                                char::from_u32(cp)
                                    .ok_or_else(|| Error("invalid unicode code point".into()))?
                            } else {
                                char::from_u32(hi as u32)
                                    .ok_or_else(|| Error("invalid unicode code point".into()))?
                            };
                            out.push(ch);
                        }
                        _ => return Err(Error("invalid escape".into())),
                    }
                }
                _ => {
                    // Collect a whole UTF-8 sequence.
                    let start = self.i - 1;
                    let extra = match c {
                        0x00..=0x7f => 0,
                        0xc0..=0xdf => 1,
                        0xe0..=0xef => 2,
                        0xf0..=0xff => 3,
                        _ => return Err(Error("invalid unicode code point".into())),
                    };
                    let end = start + 1 + extra;
                    if end > self.b.len() {
                        return Err(Error("EOF while parsing a string".into()));
                    }
                    let s = std::str::from_utf8(&self.b[start..end])
                        .map_err(|_| Error("invalid unicode code point".into()))?;
                    out.push_str(s);
                    self.i = end;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u16> {
        if self.i + 4 > self.b.len() {
            return Err(Error("EOF while parsing a string".into()));
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + 4])
            .map_err(|_| Error("invalid escape".into()))?;
        let v = u16::from_str_radix(s, 16).map_err(|_| Error("invalid escape".into()))?;
        self.i += 4;
        Ok(v)
    }

    fn array(&mut self) -> Result<Value> {
        if !self.eat(b'[') {
            return Err(Error("expected `[`".into()));
        }
        let mut out = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Value::Array(out));
        }
        loop {
            self.ws();
            out.push(self.value()?);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b']') {
                return Ok(Value::Array(out));
            }
            return Err(Error("expected `,` or `]`".into()));
        }
    }

    fn object(&mut self) -> Result<Value> {
        if !self.eat(b'{') {
            return Err(Error("expected `{`".into()));
        }
        let mut out = BTreeMap::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Value::Object(out));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            if !self.eat(b':') {
                return Err(Error("expected `:`".into()));
            }
            self.ws();
            let value = self.value()?;
            out.insert(key, value);
            self.ws();
            if self.eat(b',') {
                continue;
            }
            if self.eat(b'}') {
                return Ok(Value::Object(out));
            }
            return Err(Error("expected `,` or `}`".into()));
        }
    }
}

/// Write `s` as a JSON string, escaped exactly the way
/// `serde_json::ser::format_escaped_str` does (only the mandatory escapes; a
/// `/` is not escaped, and non-ASCII is passed through as UTF-8).
pub fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A tiny JSON writer: the object/array scaffolding serde_json's `Serializer`
/// produces (no spaces, `{}` for an empty object).
#[derive(Default)]
pub struct Writer {
    out: String,
    /// One entry per open container: whether it already has a value.
    stack: Vec<bool>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer::default()
    }

    pub fn into_string(self) -> String {
        self.out
    }

    fn comma(&mut self) {
        if let Some(seen) = self.stack.last_mut() {
            if *seen {
                self.out.push(',');
            }
            *seen = true;
        }
    }

    pub fn begin_object(&mut self) {
        self.comma();
        self.out.push('{');
        self.stack.push(false);
    }

    pub fn end_object(&mut self) {
        self.stack.pop();
        self.out.push('}');
    }

    pub fn begin_array(&mut self) {
        self.comma();
        self.out.push('[');
        self.stack.push(false);
    }

    pub fn end_array(&mut self) {
        self.stack.pop();
        self.out.push(']');
    }

    /// A key, as `serialize_entry`/`serialize_field` writes it.
    pub fn key(&mut self, k: &str) {
        self.comma();
        // The key itself must not count as the container's "value" for the next
        // comma, so roll the flag back.
        if let Some(seen) = self.stack.last_mut() {
            *seen = false;
        }
        write_str(&mut self.out, k);
        self.out.push(':');
    }

    pub fn raw(&mut self, text: &str) {
        self.comma();
        self.out.push_str(text);
    }

    pub fn str(&mut self, s: &str) {
        self.comma();
        write_str(&mut self.out, s);
    }

    pub fn u32(&mut self, v: u32) {
        self.comma();
        let _ = write!(self.out, "{v}");
    }

    pub fn i32(&mut self, v: i32) {
        self.comma();
        let _ = write!(self.out, "{v}");
    }

    pub fn u64(&mut self, v: u64) {
        self.comma();
        let _ = write!(self.out, "{v}");
    }

    pub fn f32(&mut self, v: f32) {
        self.comma();
        // serde_json writes integers without a fractional part and everything
        // else through the shortest round-trip form.
        if v == v.trunc() && v.abs() < 1e16 {
            let _ = write!(self.out, "{}", v as i64);
            self.out.push_str(".0");
        } else {
            let _ = write!(self.out, "{v}");
        }
    }

    pub fn bool(&mut self, v: bool) {
        self.comma();
        self.out.push_str(if v { "true" } else { "false" });
    }

    pub fn null(&mut self) {
        self.comma();
        self.out.push_str("null");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_objects_arrays_and_escapes() {
        let v = parse(r#"{"a":[1,2.5,-3e2],"b":"x\ny\"z","c":true,"d":null,"e":"\u00e9\ud83d\ude00"}"#)
            .unwrap();
        assert_eq!(v.get("a").unwrap().as_array().unwrap().len(), 3);
        assert_eq!(v.get("a").unwrap().as_array().unwrap()[1].as_f64(), Some(2.5));
        assert_eq!(v.get("b").unwrap().as_str(), Some("x\ny\"z"));
        assert_eq!(v.get("c").unwrap().as_bool(), Some(true));
        assert_eq!(v.get("d"), Some(&Value::Null));
        assert_eq!(v.get("e").unwrap().as_str(), Some("é😀"));
    }

    #[test]
    fn rejects_trailing_and_broken_input() {
        assert_eq!(parse("{} x").unwrap_err().0, "trailing characters");
        assert_eq!(parse("[1,]").unwrap_err().0, "expected value");
        assert_eq!(parse("\"abc").unwrap_err().0, "EOF while parsing a string");
        assert_eq!(parse("").unwrap_err().0, "EOF while parsing a value");
    }

    #[test]
    fn numbers_keep_their_text() {
        assert_eq!(parse("1.0").unwrap(), Value::Number("1.0".into()));
        assert_eq!(parse("-0").unwrap(), Value::Number("-0".into()));
    }

    #[test]
    fn writer_matches_serde_json_shape() {
        let mut w = Writer::new();
        w.begin_object();
        w.key("area");
        w.str("North Blue");
        w.key("rooms");
        w.begin_array();
        w.u32(7);
        w.f32(1.5);
        w.end_array();
        w.key("empty");
        w.begin_object();
        w.end_object();
        w.end_object();
        assert_eq!(
            w.into_string(),
            r#"{"area":"North Blue","rooms":[7,1.5],"empty":{}}"#
        );
    }

    #[test]
    fn write_str_escapes_only_mandatory_characters() {
        let mut s = String::new();
        write_str(&mut s, "a/b\"c\\d\u{1}");
        assert_eq!(s, "\"a/b\\\"c\\\\d\\u0001\"");
    }
}
