//! A small JSON reader, standing in for the `serde_json` the original used.
//!
//! The survey log is one JSON object per line, so all that is needed is a
//! parser that keeps object keys in order and remembers where a value sits.
//! Numbers are kept as their source text so a field can be read as an integer
//! or a float without lossy round trips.
//!
//! Recovered evidence for the format and the schema is in `NOTES.md`.

use std::fmt;

/// A parsed JSON value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// The number's source text, e.g. `-12` or `1.5e3`.
    Number(String),
    String(String),
    Array(Vec<Value>),
    /// Key/value pairs, in the order the source wrote them.
    Object(Vec<(String, Value)>),
}

impl Value {
    /// The name of this value's kind, as serde's error messages would say it.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "a boolean",
            Value::Number(_) => "a number",
            Value::String(_) => "a string",
            Value::Array(_) => "a sequence",
            Value::Object(_) => "a map",
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(entries) => Some(entries),
            _ => None,
        }
    }

    /// The member `name` of an object.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.as_object()?
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => f.write_str("null"),
            Value::Bool(true) => f.write_str("true"),
            Value::Bool(false) => f.write_str("false"),
            Value::Number(text) => f.write_str(text),
            Value::String(text) => write!(f, "{:?}", text),
            Value::Array(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{}", item)?;
                }
                f.write_str("]")
            }
            Value::Object(entries) => {
                f.write_str("{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{:?}:{}", key, value)?;
                }
                f.write_str("}")
            }
        }
    }
}

/// Parse one JSON document (a log line).
pub fn parse(text: &str) -> Result<Value, String> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.space();
    let value = reader.value()?;
    reader.space();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("trailing characters"));
    }
    Ok(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn error(&self, what: &str) -> String {
        format!("{} at byte {}", what, self.at)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::String(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error("expected a literal"))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.eat(b'{')?;
        let mut entries = Vec::new();
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Object(entries));
        }
        loop {
            self.space();
            let key = self.string()?;
            self.space();
            self.eat(b':')?;
            self.space();
            let value = self.value()?;
            entries.push((key, value));
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(entries));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.space();
            items.push(self.value()?);
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = self.peek().ok_or_else(|| self.error("unterminated string"))?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            // A surrogate pair is written as two \u escapes.
                            if (0xd800..0xdc00).contains(&code)
                                && self.bytes[self.at..].starts_with(b"\\u")
                            {
                                self.at += 2;
                                let low = self.hex4()?;
                                code = 0x1_0000
                                    + ((code - 0xd800) << 10)
                                    + (low.wrapping_sub(0xdc00) & 0x3ff);
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => return Err(self.error(&format!("bad escape `\\{}`", other as char))),
                    }
                }
                Some(_) => {
                    // Copy the whole UTF-8 sequence, so multi-byte text survives.
                    let start = self.at;
                    let end = self.char_end().ok_or_else(|| self.error("bad UTF-8"))?;
                    self.at = end;
                    match std::str::from_utf8(&self.bytes[start..end]) {
                        Ok(text) => out.push_str(text),
                        Err(_) => return Err(self.error("bad UTF-8")),
                    }
                }
            }
        }
    }

    /// The byte just past the UTF-8 sequence starting at `self.at`.
    fn char_end(&self) -> Option<usize> {
        let first = self.peek()?;
        let extra = match first {
            0x00..=0x7f => 0,
            0xc0..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf7 => 3,
            _ => return None,
        };
        Some(self.at + 1 + extra)
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut code = 0u32;
        for _ in 0..4 {
            let digit = self.peek().ok_or_else(|| self.error("bad \\u escape"))?;
            let value = match digit {
                b'0'..=b'9' => (digit - b'0') as u32,
                b'a'..=b'f' => (digit - b'a') as u32 + 10,
                b'A'..=b'F' => (digit - b'A') as u32 + 10,
                _ => return Err(self.error("bad \\u escape")),
            };
            code = code * 16 + value;
            self.at += 1;
        }
        Ok(code)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.at += 1;
            }
        }
        if self.at == start {
            return Err(self.error("expected a number"));
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.error("bad number"))?;
        Ok(Value::Number(text.to_owned()))
    }
}

/// `serde`-style "invalid type" wording, used by the field readers.
pub fn wrong_type(value: &Value, expected: &str) -> String {
    format!("invalid type: {}, expected {}", value.kind(), expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_object_in_order() {
        let value = parse(r#"{"t":"1","run":"a b","n":-12.5e2,"ok":true,"no":null}"#).unwrap();
        assert_eq!(
            value,
            Value::Object(vec![
                ("t".into(), Value::String("1".into())),
                ("run".into(), Value::String("a b".into())),
                ("n".into(), Value::Number("-12.5e2".into())),
                ("ok".into(), Value::Bool(true)),
                ("no".into(), Value::Null),
            ])
        );
        assert_eq!(value.get("run").and_then(Value::as_str), Some("a b"));
        assert!(value.get("missing").is_none());
    }

    #[test]
    fn parses_arrays_and_escapes() {
        let value = parse(r#"{"lines":["a\"b","c\\d","\u00e9"]}"#).unwrap();
        let lines: Vec<&str> = value
            .get("lines")
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        assert_eq!(lines, vec!["a\"b", "c\\d", "\u{e9}"]);
    }

    #[test]
    fn rejects_broken_documents() {
        assert!(parse("{").is_err());
        assert!(parse("{\"a\":}").is_err());
        assert!(parse("{} extra").is_err());
        assert!(parse("").is_err());
    }
}
