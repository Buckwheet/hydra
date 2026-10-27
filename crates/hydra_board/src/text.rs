//! A small JSON / TOML reader, standing in for `serde_json` and `toml`.
//!
//! The original crate used `serde`, `serde_json` and `toml` with derived
//! `Deserialize` impls.  This build has no external crates, so the file formats
//! are read by hand into a [`Value`] tree and the typed structs (chips, targets,
//! wheels, …) pick fields out of it.  The binary's own literals (`c001_`,
//! `c005.json`, `controller_wheels`, `struct Chip with 11 elements`, …) show
//! both formats were in use: config files named `*.toml` are TOML, `*.json` are
//! JSON.
//!
//! This module is a **departure**: `hydra_session` has an equivalent reader but
//! it is private (`hydra_session::textfmt`), and this crate may not edit its
//! siblings, so the subset lives here.
//!
//! The TOML subset: `[table]`, `[[array.of.tables]]`, `key = value` with dotted
//! keys, basic/literal strings, integers, floats, booleans, arrays (which may
//! span lines) and inline `{ .. }` tables, plus `#` comments.

use std::collections::BTreeMap;

/// A parsed document node.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Table(BTreeMap<String, Value>),
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

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.as_i64().map(|i| i.max(0) as u64)
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::Float(f) => Some(*f as f32),
            Value::Int(i) => Some(*i as f32),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    /// The member `key` of a table.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_table().and_then(|t| t.get(key))
    }

    /// The member `key` of a table, following dotted paths.
    pub fn path(&self, keys: &[&str]) -> Option<&Value> {
        let mut node = self;
        for key in keys {
            node = node.get(key)?;
        }
        Some(node)
    }

    pub fn str_of(&self, key: &str) -> String {
        self.get(key).and_then(Value::as_str).unwrap_or("").to_string()
    }

    pub fn string_or(&self, key: &str, fallback: &str) -> String {
        match self.get(key).and_then(Value::as_str) {
            Some(s) => s.to_string(),
            None => fallback.to_string(),
        }
    }

    pub fn bool_of(&self, key: &str) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn u64_of(&self, key: &str) -> Option<u64> {
        self.get(key).and_then(Value::as_u64)
    }

    pub fn f32_of(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(Value::as_f32)
    }

    pub fn strings_of(&self, key: &str) -> Vec<String> {
        self.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

/// Parse a JSON document.  Objects become tables, so `Value::get` works the
/// same on either format.
pub fn parse_json(text: &str) -> Result<Value, String> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        at: 0,
        allow_literal_strings: false,
        allow_bare_keys: false,
        allow_comments: false,
        key_sep: b':',
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.at < p.bytes.len() {
        return Err(format!("trailing text at byte {}", p.at));
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// TOML
// ---------------------------------------------------------------------------

/// Parse the TOML subset described in the module comment.
pub fn parse_toml(text: &str) -> Result<Value, String> {
    let mut root: Value = Value::Table(BTreeMap::new());
    // The path of the table that plain `key = value` lines land in.
    let mut section: Vec<String> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("[[") {
            let Some(inner) = rest.strip_suffix("]]") else {
                return Err(format!("bad array-of-tables header: {line}"));
            };
            let path = split_key(inner)?;
            push_array_table(&mut root, &path)?;
            section = path;
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let Some(inner) = rest.strip_suffix(']') else {
                return Err(format!("bad table header: {line}"));
            };
            section = split_key(inner)?;
            ensure_table(&mut root, &section)?;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("cannot parse line '{line}'"));
        };
        let mut key_path = split_key(key.trim())?;
        let mut text = value.trim().to_string();
        // An array or inline table may run over several lines.
        while needs_more(&text) {
            match lines.next() {
                Some(next) => {
                    text.push('\n');
                    text.push_str(strip_comment(next).trim());
                }
                None => return Err(format!("unterminated value for '{}'", key.trim())),
            }
        }
        let mut p = Parser {
            bytes: text.as_bytes(),
            at: 0,
            allow_literal_strings: true,
            allow_bare_keys: true,
            allow_comments: false,
            key_sep: b'=',
        };
        p.ws();
        let v = p.value()?;
        p.ws();
        if p.at < p.bytes.len() {
            return Err(format!("trailing text after '{}'", key.trim()));
        }
        let mut path = section.clone();
        path.append(&mut key_path);
        insert(&mut root, &path, v)?;
    }
    Ok(root)
}

fn needs_more(text: &str) -> bool {
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for &b in text.as_bytes() {
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'[' | b'{' => depth += 1,
                b']' | b'}' => depth -= 1,
                _ => {}
            },
        }
    }
    depth > 0 || quote.is_some()
}

fn strip_comment(line: &str) -> &str {
    let mut quote: Option<u8> = None;
    for (i, &b) in line.as_bytes().iter().enumerate() {
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => {
                if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'#' {
                    return &line[..i];
                }
            }
        }
    }
    line
}

fn split_key(key: &str) -> Result<Vec<String>, String> {
    let parts: Vec<String> = key
        .split('.')
        .map(|p| p.trim().trim_matches('"').trim_matches('\'').to_string())
        .collect();
    if parts.iter().any(String::is_empty) {
        return Err(format!("empty key in '{key}'"));
    }
    Ok(parts)
}

fn ensure_table<'a>(root: &'a mut Value, path: &[String]) -> Result<&'a mut Value, String> {
    let mut node = root;
    for (i, key) in path.iter().enumerate() {
        if !matches!(node, Value::Table(_)) && i > 0 {
            return Err(format!("'{key}' is not a table"));
        }
        if let Value::Table(map) = node {
            // `[[x]]` leaves an array behind: descend into its last element.
            node = map
                .entry(key.clone())
                .or_insert_with(|| Value::Table(BTreeMap::new()));
            if let Value::Array(items) = node {
                node = items
                    .last_mut()
                    .ok_or_else(|| format!("empty array-of-tables '{key}'"))?;
            }
        } else {
            return Err(format!("'{key}' is not a table"));
        }
    }
    Ok(node)
}

fn push_array_table(root: &mut Value, path: &[String]) -> Result<(), String> {
    let (last, parents) = path.split_last().ok_or("empty table path")?;
    let parent = ensure_table(root, parents)?;
    let Value::Table(map) = parent else {
        return Err("array-of-tables header outside a table".into());
    };
    match map
        .entry(last.clone())
        .or_insert_with(|| Value::Array(Vec::new()))
    {
        Value::Array(items) => {
            items.push(Value::Table(BTreeMap::new()));
            Ok(())
        }
        _ => Err(format!("'{last}' is not an array of tables")),
    }
}

fn insert(root: &mut Value, path: &[String], value: Value) -> Result<(), String> {
    let (last, parents) = path.split_last().ok_or("empty key")?;
    let parent = ensure_table(root, parents)?;
    match parent {
        Value::Table(map) => {
            map.insert(last.clone(), value);
            Ok(())
        }
        _ => Err(format!("'{last}' written outside a table")),
    }
}

// ---------------------------------------------------------------------------
// The value grammar shared by both formats
// ---------------------------------------------------------------------------

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    allow_literal_strings: bool,
    allow_bare_keys: bool,
    allow_comments: bool,
    /// What separates a key from its value: `:` in JSON, `=` in TOML.
    key_sep: u8,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => self.at += 1,
                Some(b'#') if self.allow_comments => {
                    while let Some(b) = self.peek() {
                        self.at += 1;
                        if b == b'\n' {
                            break;
                        }
                    }
                }
                _ => return,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.table(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.basic_string()?)),
            Some(b'\'') if self.allow_literal_strings => Ok(Value::Str(self.literal_string()?)),
            Some(_) => self.word(),
            None => Err("value expected".into()),
        }
    }

    fn table(&mut self) -> Result<Value, String> {
        self.at += 1; // '{'
        let mut map = BTreeMap::new();
        loop {
            self.ws();
            match self.peek() {
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Table(map));
                }
                None => return Err("unterminated inline table".into()),
                _ => {}
            }
            let key = self.key()?;
            self.ws();
            if self.peek() != Some(self.key_sep) {
                return Err(format!(
                    "'{}' expected in inline table",
                    self.key_sep as char
                ));
            }
            self.at += 1;
            let value = self.value()?;
            map.insert(key, value);
            self.ws();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {}
                _ => return Err("',' or '}' expected in inline table".into()),
            }
        }
    }

    fn key(&mut self) -> Result<String, String> {
        self.ws();
        match self.peek() {
            Some(b'"') => self.basic_string(),
            Some(b'\'') if self.allow_literal_strings => self.literal_string(),
            _ => {
                let start = self.at;
                while let Some(b) = self.peek() {
                    if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' {
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                if start == self.at {
                    return Err(format!("key expected at byte {}", self.at));
                }
                Ok(String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned())
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.at += 1; // '['
        let mut items = Vec::new();
        loop {
            self.ws();
            match self.peek() {
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                None => return Err("unterminated array".into()),
                _ => {}
            }
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {}
                _ => return Err("',' or ']' expected in array".into()),
            }
        }
    }

    fn basic_string(&mut self) -> Result<String, String> {
        self.at += 1; // '"'
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err("unterminated string".into()),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let Some(esc) = self.peek() else {
                        return Err("unterminated escape".into());
                    };
                    self.at += 1;
                    match esc {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'u' => out.push(self.unicode_escape(4)?),
                        b'U' => out.push(self.unicode_escape(8)?),
                        other => return Err(format!("bad escape '\\{}'", other as char)),
                    }
                }
                Some(_) => {
                    let start = self.at;
                    while let Some(b) = self.peek() {
                        if b == b'"' || b == b'\\' {
                            break;
                        }
                        self.at += 1;
                    }
                    out.push_str(&String::from_utf8_lossy(&self.bytes[start..self.at]));
                }
            }
        }
    }

    fn unicode_escape(&mut self, digits: usize) -> Result<char, String> {
        if self.at + digits > self.bytes.len() {
            return Err("short unicode escape".into());
        }
        let hex = std::str::from_utf8(&self.bytes[self.at..self.at + digits])
            .map_err(|_| "bad unicode escape".to_string())?;
        let code = u32::from_str_radix(hex, 16).map_err(|_| "bad unicode escape".to_string())?;
        self.at += digits;
        char::from_u32(code).ok_or_else(|| "bad code point".to_string())
    }

    fn literal_string(&mut self) -> Result<String, String> {
        self.at += 1; // '\''
        let start = self.at;
        while let Some(b) = self.peek() {
            if b == b'\'' {
                let s = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
                self.at += 1;
                return Ok(s);
            }
            self.at += 1;
        }
        Err("unterminated literal string".into())
    }

    fn word(&mut self) -> Result<Value, String> {
        let start = self.at;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.' | b'_' | b':' | b'T' | b'Z')
            {
                self.at += 1;
            } else {
                break;
            }
        }
        // A bare key in an inline table is a bare value here only if we are
        // parsing a value; the caller knows which it wants.
        if start == self.at {
            return Err(format!("value expected at byte {}", self.at));
        }
        let word = String::from_utf8_lossy(&self.bytes[start..self.at]).into_owned();
        Ok(match word.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            _ => {
                let cleaned: String = word.chars().filter(|c| *c != '_').collect();
                if let Ok(i) = cleaned.parse::<i64>() {
                    Value::Int(i)
                } else if let Ok(f) = cleaned.parse::<f64>() {
                    Value::Float(f)
                } else if self.allow_bare_keys {
                    Value::Str(word)
                } else {
                    return Err(format!("unexpected word '{word}'"));
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Escape `s` for a JSON string body.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Render a document as JSON with sorted keys.
pub fn to_json(value: &Value) -> String {
    let mut out = String::new();
    write_json(value, &mut out);
    out
}

fn write_json(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => out.push_str(&i.to_string()),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                out.push_str(&format!("{f:.1}"));
            } else {
                out.push_str(&f.to_string());
            }
        }
        Value::Str(s) => {
            out.push('"');
            out.push_str(&json_escape(s));
            out.push('"');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json(item, out);
            }
            out.push(']');
        }
        Value::Table(map) => {
            out.push('{');
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(&json_escape(key));
                out.push_str("\":");
                write_json(item, out);
            }
            out.push('}');
        }
    }
}

/// Render a document as the TOML subset above.
pub fn to_toml(value: &Value) -> String {
    let mut out = String::new();
    write_toml(value, &mut out, &[]);
    out
}

fn write_toml(value: &Value, out: &mut String, path: &[String]) {
    let Some(map) = value.as_table() else {
        return;
    };
    for (key, item) in map {
        if item.as_table().is_some() || matches!(item, Value::Array(_)) {
            continue;
        }
        out.push_str(key);
        out.push_str(" = ");
        out.push_str(&inline_value(item));
        out.push('\n');
    }
    for (key, item) in map {
        let mut here = path.to_vec();
        here.push(key.clone());
        match item {
            Value::Table(_) => {
                out.push_str(&format!("\n[{}]\n", here.join(".")));
                write_toml(item, out, &here);
            }
            Value::Array(items) if items.iter().all(|i| i.as_table().is_some()) => {
                for entry in items {
                    out.push_str(&format!("\n[[{}]]\n", here.join(".")));
                    write_toml(entry, out, &here);
                }
            }
            _ => {}
        }
    }
}

fn inline_value(value: &Value) -> String {
    match value {
        Value::Null => "\"\"".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Str(s) => format!("\"{}\"", json_escape(s)),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(inline_value).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Table(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{} = {}", k, inline_value(v)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
    }
}

/// Read a file and parse it by extension (`*.toml` → TOML, else JSON).
pub fn read_file(path: &std::path::Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
        parse_toml(&text)
    } else if text.trim().is_empty() {
        Ok(Value::Table(BTreeMap::new()))
    } else {
        parse_json(&text).or_else(|_| parse_toml(&text))
    }
}

/// Write a document to `path`, making the folder if it is missing.
pub fn write_file(path: &std::path::Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let text = if path.extension().and_then(|e| e.to_str()) == Some("toml") {
        to_toml(value)
    } else {
        to_json(value)
    };
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let v = parse_json(r#"{"a":1,"b":[true,null,"x\ty"],"c":{"d":2.5}}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_i64(), Some(1));
        assert_eq!(v.get("b").unwrap().as_array().unwrap().len(), 3);
        assert_eq!(v.path(&["c", "d"]).unwrap().as_f32(), Some(2.5));
        assert_eq!(to_json(&v), r#"{"a":1,"b":[true,null,"x\ty"],"c":{"d":2.5}}"#);
    }

    #[test]
    fn json_escape_is_reversible() {
        let v = parse_json(r#""a\"b\\c\n""#).unwrap();
        assert_eq!(v.as_str(), Some("a\"b\\c\n"));
        assert_eq!(json_escape("a\"b"), "a\\\"b");
    }

    #[test]
    fn toml_tables_arrays_and_dotted_keys() {
        let text = "\
# the wheels
default = [\"keep\", \"drop\"]

[controller_wheels.main]
label = 'main wheel'
span = 30.5

[[controller_wheels.rings]]
label = \"one\"

[[controller_wheels.rings]]
label = \"two\"
";
        let v = parse_toml(text).unwrap();
        assert_eq!(v.strings_of("default"), vec!["keep", "drop"]);
        assert_eq!(
            v.path(&["controller_wheels", "main"]).unwrap().str_of("label"),
            "main wheel"
        );
        assert_eq!(
            v.path(&["controller_wheels", "main"]).unwrap().f32_of("span"),
            Some(30.5)
        );
        let rings = v
            .path(&["controller_wheels", "rings"])
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(rings.len(), 2);
        assert_eq!(rings[1].str_of("label"), "two");
    }

    #[test]
    fn toml_multiline_array_and_inline_table() {
        let v = parse_toml("a = [\n 1,\n 2,\n]\nb = { x = \"y\", n = 3 }\n").unwrap();
        assert_eq!(v.get("a").unwrap().as_array().unwrap().len(), 2);
        assert_eq!(v.get("b").unwrap().str_of("x"), "y");
        assert_eq!(v.get("b").unwrap().u64_of("n"), Some(3));
    }

    #[test]
    fn toml_serialises_back() {
        let v = parse_toml("a = 1\nb = \"two\"\n[t]\nc = true\n").unwrap();
        let text = to_toml(&v);
        assert!(text.contains("a = 1"), "{text}");
        assert!(text.contains("b = \"two\""), "{text}");
        assert!(text.contains("[t]"), "{text}");
        assert!(text.contains("c = true"), "{text}");
        assert_eq!(parse_toml(&text).unwrap(), v);
    }
}
