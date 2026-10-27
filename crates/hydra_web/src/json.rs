//! A very small `std`-only JSON value, writer and parser.
//!
//! The original crate used `serde` + `serde_json`.  This module is the
//! replacement: [`Value`] plus a deterministic writer (object keys are emitted
//! in insertion order, exactly like `serde_json`'s `Map` does for the small
//! structs this crate emits) and a strict-enough RFC 8259 parser.
//!
//! Everything the web page exchanges is built through this module so the wire
//! contract recovered from the binary (`{"t":"...", ...}`) can be asserted in
//! tests without pulling in a dependency.

/// A JSON value.
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

    /// Render to JSON text.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// Append the JSON text to `out`.
    pub fn write(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Num(t) => out.push_str(t),
            Value::Str(s) => write_str(s, out),
            Value::Arr(xs) => {
                out.push('[');
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    x.write(out);
                }
                out.push(']');
            }
            Value::Obj(pairs) => {
                out.push('{');
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_str(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// Write a JSON string literal (`serde_json`'s escape set: `"` `\` and the
/// control characters, `\uXXXX` for the rest below `0x20`).
pub fn write_str(s: &str, out: &mut String) {
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
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Parse JSON text.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        text,
        at: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.at != p.bytes.len() {
        return Err(format!("trailing characters at byte {}", p.at));
    }
    Ok(v)
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl<'a> Parser<'a> {
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

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.peek() == Some(b) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", b as char, self.at))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            None => Err("unexpected end of input".to_string()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(_) => self.number(),
        }
    }

    fn literal(&mut self, word: &str, v: Value) -> Result<Value, String> {
        if self.text[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(v)
        } else {
            Err(format!("bad literal at byte {}", self.at))
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while let Some(b) = self.peek() {
            if b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-' {
                self.at += 1;
            } else {
                break;
            }
        }
        let text = &self.text[start..self.at];
        if text.is_empty() || text == "-" {
            return Err(format!("bad number at byte {start}"));
        }
        Ok(Value::Num(text.to_string()))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let b = self.peek().ok_or("unterminated string")?;
            self.at += 1;
            match b {
                b'"' => return Ok(out),
                b'\\' => {
                    let e = self.peek().ok_or("unterminated escape")?;
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
                            let cp = self.hex4()?;
                            // Surrogate pairs.
                            if (0xd800..0xdc00).contains(&cp) {
                                if self.text[self.at..].starts_with("\\u") {
                                    self.at += 2;
                                    let lo = self.hex4()?;
                                    let c = 0x10000
                                        + (((cp - 0xd800) as u32) << 10)
                                        + (lo - 0xdc00) as u32;
                                    out.push(char::from_u32(c).unwrap_or('\u{fffd}'));
                                } else {
                                    out.push('\u{fffd}');
                                }
                            } else {
                                out.push(char::from_u32(cp.into()).unwrap_or('\u{fffd}'));
                            }
                        }
                        other => return Err(format!("bad escape `\\{}`", other as char)),
                    }
                }
                // Multi-byte UTF-8: copy the whole scalar.
                b if b < 0x80 => out.push(b as char),
                _ => {
                    let start = self.at - 1;
                    let len = match b {
                        0xc0..=0xdf => 2,
                        0xe0..=0xef => 3,
                        _ => 4,
                    };
                    if start + len > self.bytes.len() {
                        return Err("truncated utf-8".to_string());
                    }
                    out.push_str(&self.text[start..start + len]);
                    self.at = start + len;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u16, String> {
        if self.at + 4 > self.bytes.len() {
            return Err("truncated \\u escape".to_string());
        }
        let s = &self.text[self.at..self.at + 4];
        self.at += 4;
        u16::from_str_radix(s, 16).map_err(|_| format!("bad \\u escape `{s}`"))
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::Arr(out));
        }
        loop {
            self.ws();
            out.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                }
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Arr(out));
                }
                _ => return Err(format!("expected `,` or `]` at byte {}", self.at)),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut out = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Obj(out));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.expect(b':')?;
            self.ws();
            let v = self.value()?;
            out.push((key, v));
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.at += 1;
                }
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Obj(out));
                }
                _ => return Err(format!("expected `,` or `}}` at byte {}", self.at)),
            }
        }
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
}
