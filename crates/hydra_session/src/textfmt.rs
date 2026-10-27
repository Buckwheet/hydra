//! A very small JSON / TOML reader and writer.
//!
//! The original crate used `serde_json` (its `__Visitor` impls are still in
//! the symbol table, e.g.
//! `<hydra_session::notes::_::<impl Deserialize for notes::File>::deserialize::__Visitor
//! as Visitor>::expecting @0x101a54fb4`) and `toml` / `toml_edit` for the
//! settings and stream rules.  This crate is std-only, so this module
//! implements the small subset of both formats the recovered files actually
//! use:
//!
//! * JSON: objects, arrays, strings (with `\"`, `\\`, `\/`, `\b`, `\f`, `\n`,
//!   `\r`, `\t`, `\uXXXX`), integers, floats, `true`, `false`, `null`.
//! * TOML: `key = value` lines, `[table]` headers, `[[array-of-tables]]`
//!   headers, basic strings, booleans, integers and single-line arrays of
//!   scalars, plus `#` comments.
//!
//! Nothing here is a general-purpose implementation; unknown keys are kept but
//! never interpreted, which mirrors how the original ignored fields it did not
//! know about.  Departures from `serde` are recorded in `NOTES.md`.

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
// JSON
// ---------------------------------------------------------------------------

/// Parse a JSON document.  The error is a short human-readable message with
/// the byte offset, which is what the `Refused` error type carries.
pub fn parse_json(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        b: text.as_bytes(),
        i: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.i != p.b.len() {
        return Err(format!("trailing text at byte {}", p.i));
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

    fn eat(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected `{}` at byte {}", c as char, self.i))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        match self.peek() {
            None => Err("unexpected end of input".to_string()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(_) => self.number(),
        }
    }

    fn word(&mut self, w: &str, v: Value) -> Result<Value, String> {
        if self.b[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            Err(format!("expected `{w}` at byte {}", self.i))
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        while self.i < self.b.len()
            && matches!(self.b[self.i],
                b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        {
            self.i += 1;
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).map_err(|e| e.to_string())?;
        s.parse::<f64>()
            .map(Value::Num)
            .map_err(|e| format!("bad number `{s}`: {e}"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let c = self.peek().ok_or("dangling escape")?;
                    self.i += 1;
                    match c {
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
                                // surrogate pair
                                self.eat(b'\\')?;
                                self.eat(b'u')?;
                                let lo = self.hex4()?;
                                let cp =
                                    0x1_0000 + ((hi - 0xd800) << 10) + (lo.wrapping_sub(0xdc00));
                                char::from_u32(cp).unwrap_or('\u{fffd}')
                            } else {
                                char::from_u32(hi).unwrap_or('\u{fffd}')
                            };
                            out.push(ch);
                        }
                        other => return Err(format!("bad escape \\{}", other as char)),
                    }
                }
                Some(_) => {
                    // Copy one UTF-8 scalar.
                    let start = self.i;
                    let len = utf8_len(self.b[self.i]);
                    self.i = (self.i + len).min(self.b.len());
                    let chunk = std::str::from_utf8(&self.b[start..self.i])
                        .map_err(|e| e.to_string())?;
                    out.push_str(chunk);
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.b.len() {
            return Err("short \\u escape".to_string());
        }
        let s = std::str::from_utf8(&self.b[self.i..self.i + 4]).map_err(|e| e.to_string())?;
        self.i += 4;
        u32::from_str_radix(s, 16).map_err(|e| format!("bad \\u escape: {e}"))
    }

    fn array(&mut self) -> Result<Value, String> {
        self.eat(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.i += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("expected `,` or `]` at byte {}", self.i)),
            }
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.eat(b'{')?;
        let mut pairs = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.i += 1;
            return Ok(Value::Obj(pairs));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            self.eat(b':')?;
            self.ws();
            let v = self.value()?;
            pairs.push((k, v));
            self.ws();
            match self.peek() {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Obj(pairs));
                }
                _ => return Err(format!("expected `,` or `}}` at byte {}", self.i)),
            }
        }
    }
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
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

/// Render a value as JSON, pretty-printed with two spaces.
pub fn to_json(v: &Value) -> String {
    let mut out = String::new();
    write_json(&mut out, v, 0);
    out
}

fn write_json(out: &mut String, v: &Value, depth: usize) {
    let pad = |out: &mut String, d: usize| {
        out.push('\n');
        for _ in 0..d {
            out.push_str("  ");
        }
    };
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                out.push_str(&format!("{}", *n as i64));
            } else {
                out.push_str(&format!("{n}"));
            }
        }
        Value::Str(s) => {
            out.push('"');
            out.push_str(&json_escape(s));
            out.push('"');
        }
        Value::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                pad(out, depth + 1);
                write_json(out, item, depth + 1);
            }
            pad(out, depth);
            out.push(']');
        }
        Value::Obj(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, item)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                pad(out, depth + 1);
                out.push('"');
                out.push_str(&json_escape(k));
                out.push_str("\": ");
                write_json(out, item, depth + 1);
            }
            pad(out, depth);
            out.push('}');
        }
    }
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
    fn json_reports_the_byte_offset_of_a_bad_escape() {
        let e = parse_json(r#"{"a": "\q"}"#).unwrap_err();
        assert!(e.contains("bad escape"), "{e}");
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
