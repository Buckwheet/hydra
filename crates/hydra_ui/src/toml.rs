//! A hand-written, `std`-only subset of TOML.
//!
//! The original crate used `toml`/`toml_edit`/`serde` for its settings, theme,
//! highlight, hotbar and macro files.  Those dependencies are not available
//! here, so this module reimplements the part of the format the recovered code
//! actually touches:
//!
//! * `[table]` headers, nested via dotted keys (`[a.b]`),
//! * `[[array of tables]]`,
//! * bare keys, quoted keys, dotted keys,
//! * basic `"strings"` with the usual escapes, literal `'strings'`,
//! * multi-line `"""`/`'''` strings (trimmed the way TOML trims them),
//! * integers (with `_` separators), floats, booleans,
//! * arrays `[..]` (including trailing commas and newlines) and inline tables
//!   `{ a = 1, b = "x" }`,
//! * `#` comments.
//!
//! Dates/times are *not* interpreted: an RFC 3339 looking value is kept as a
//! string.  That departs from `toml` (which has a `Datetime` type, visible in
//! the binary as `$__toml_private_Datetime`) but no recovered Hydra field is
//! ever written as a date, so the difference is unobservable.
//!
//! Writing is deliberately ordered: [`Doc`] renders tables in insertion order,
//! which is what `toml_edit` does for a document it has just built.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A parsed TOML value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<Value>),
    Table(Table),
    /// `[[name]]` blocks.
    Tables(Vec<Table>),
}

/// A TOML table (ordered, so a round-trip keeps the file's order).
pub type Table = BTreeMap<String, Value>;

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }
    pub fn as_tables(&self) -> Option<&[Table]> {
        match self {
            Value::Tables(t) => Some(t),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            Value::Tables(t) => None,
            _ => None,
        }
    }
    /// Every element of an array or array-of-tables as a `Value`.
    pub fn elements(&self) -> Vec<&Value> {
        match self {
            Value::Array(a) => a.iter().collect(),
            Value::Tables(t) => t.iter().map(|_| &Value::Bool(false)).collect(),
            _ => Vec::new(),
        }
    }
}

/// Why a TOML document could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

type Res<T> = Result<T, Error>;

fn err<T>(line: usize, message: impl Into<String>) -> Res<T> {
    Err(Error {
        line,
        message: message.into(),
    })
}

// ---------------------------------------------------------------- parsing

struct Parser<'a> {
    src: &'a [u8],
    at: usize,
    line: usize,
}

/// Parse a TOML document into a table.
pub fn parse(src: &str) -> Res<Table> {
    let mut p = Parser {
        src: src.as_bytes(),
        at: 0,
        line: 1,
    };
    let mut root = Table::new();
    // Path of the table new key/value pairs land in.
    let mut cur: Vec<String> = Vec::new();
    loop {
        p.skip_blank();
        if p.at >= p.src.len() {
            break;
        }
        if p.peek() == Some(b'[') {
            let double = p.src.get(p.at + 1) == Some(&b'[');
            p.at += if double { 2 } else { 1 };
            p.skip_spaces();
            let path = p.key_path()?;
            p.skip_spaces();
            if double {
                if !p.eat(b']') || !p.eat(b']') {
                    return err(p.line, "expected `]]`");
                }
                p.skip_to_eol();
                insert_array_table(&mut root, &path, p.line)?;
            } else {
                if !p.eat(b']') {
                    return err(p.line, "expected `]`");
                }
                p.skip_to_eol();
                ensure_table(&mut root, &path, p.line)?;
            }
            cur = path;
            continue;
        }
        // key = value
        let path = p.key_path()?;
        p.skip_spaces();
        if !p.eat(b'=') {
            return err(p.line, "expected `=`");
        }
        p.skip_spaces();
        let value = p.value()?;
        p.skip_to_eol();
        let table = table_at(&mut root, &cur, p.line)?;
        insert(table, &path, value, p.line)?;
    }
    Ok(root)
}

fn table_at<'t>(root: &'t mut Table, path: &[String], line: usize) -> Res<&'t mut Table> {
    let mut t = root;
    for seg in path {
        let entry = t
            .entry(seg.clone())
            .or_insert_with(|| Value::Table(Table::new()));
        t = match entry {
            Value::Table(inner) => inner,
            Value::Tables(list) => match list.last_mut() {
                Some(last) => last,
                None => return err(line, "empty array of tables"),
            },
            _ => return err(line, format!("`{seg}` is not a table")),
        };
    }
    Ok(t)
}

fn ensure_table(root: &mut Table, path: &[String], line: usize) -> Res<()> {
    table_at(root, path, line).map(|_| ())
}

fn insert_array_table(root: &mut Table, path: &[String], line: usize) -> Res<()> {
    let (last, parent_path) = match path.split_last() {
        Some(v) => v,
        None => return err(line, "empty table header"),
    };
    let parent = table_at(root, parent_path, line)?;
    match parent
        .entry(last.clone())
        .or_insert_with(|| Value::Tables(Vec::new()))
    {
        Value::Tables(list) => {
            list.push(Table::new());
            Ok(())
        }
        _ => err(line, format!("`{last}` is not an array of tables")),
    }
}

fn insert(table: &mut Table, path: &[String], value: Value, line: usize) -> Res<()> {
    let (last, parents) = match path.split_last() {
        Some(v) => v,
        None => return err(line, "empty key"),
    };
    let t = table_at(table, parents, line)?;
    t.insert(last.clone(), value);
    Ok(())
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.at).copied()
    }
    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\t')) {
            self.at += 1;
        }
    }
    /// Skip spaces, comments and newlines.
    fn skip_blank(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') => self.at += 1,
                Some(b'\n') => {
                    self.line += 1;
                    self.at += 1;
                }
                Some(b'#') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.at += 1;
                    }
                }
                _ => return,
            }
        }
    }
    /// Skip the rest of the line (spaces + optional comment + newline).
    fn skip_to_eol(&mut self) {
        self.skip_spaces();
        if self.peek() == Some(b'#') {
            while let Some(c) = self.peek() {
                if c == b'\n' {
                    break;
                }
                self.at += 1;
            }
        }
        if self.peek() == Some(b'\r') {
            self.at += 1;
        }
        if self.peek() == Some(b'\n') {
            self.at += 1;
            self.line += 1;
        }
    }
    fn key_path(&mut self) -> Res<Vec<String>> {
        let mut out = Vec::new();
        loop {
            self.skip_spaces();
            let key = self.key()?;
            out.push(key);
            self.skip_spaces();
            if !self.eat(b'.') {
                return Ok(out);
            }
        }
    }
    fn key(&mut self) -> Res<String> {
        match self.peek() {
            Some(b'"') => self.basic_string(),
            Some(b'\'') => self.literal_string(),
            _ => {
                let start = self.at;
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                        self.at += 1;
                    } else {
                        break;
                    }
                }
                if self.at == start {
                    return err(self.line, "expected a key");
                }
                Ok(String::from_utf8_lossy(&self.src[start..self.at]).into_owned())
            }
        }
    }
    fn basic_string(&mut self) -> Res<String> {
        let multi = self.src[self.at..].starts_with(b"\"\"\"");
        self.at += if multi { 3 } else { 1 };
        if multi && self.peek() == Some(b'\n') {
            self.at += 1;
            self.line += 1;
        }
        let mut out = String::new();
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return err(self.line, "unterminated string"),
            };
            if multi {
                if self.src[self.at..].starts_with(b"\"\"\"") {
                    self.at += 3;
                    return Ok(out);
                }
            } else if c == b'"' {
                self.at += 1;
                return Ok(out);
            } else if c == b'\n' {
                return err(self.line, "newline in a basic string");
            }
            self.at += 1;
            match c {
                b'\\' => {
                    let e = match self.peek() {
                        Some(e) => e,
                        None => return err(self.line, "trailing backslash"),
                    };
                    self.at += 1;
                    match e {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'\n' => {
                            self.line += 1;
                            // line-ending backslash: trim following whitespace
                            while matches!(self.peek(), Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r'))
                            {
                                if self.peek() == Some(b'\n') {
                                    self.line += 1;
                                }
                                self.at += 1;
                            }
                        }
                        b'u' | b'U' => {
                            let n = if e == b'u' { 4 } else { 8 };
                            let hex = self.src.get(self.at..self.at + n).ok_or(Error {
                                line: self.line,
                                message: "short \\u escape".into(),
                            })?;
                            let s = std::str::from_utf8(hex).unwrap_or("0");
                            let cp = u32::from_str_radix(s, 16)
                                .map_err(|_| Error {
                                    line: self.line,
                                    message: "bad hex escape".into(),
                                })?;
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                            self.at += n;
                        }
                        other => {
                            out.push('\\');
                            out.push(other as char);
                        }
                    }
                }
                b'\n' => {
                    self.line += 1;
                    out.push('\n');
                }
                _ => out.push(c as char),
            }
        }
    }
    fn literal_string(&mut self) -> Res<String> {
        let multi = self.src[self.at..].starts_with(b"'''");
        self.at += if multi { 3 } else { 1 };
        if multi && self.peek() == Some(b'\n') {
            self.at += 1;
            self.line += 1;
        }
        let start = self.at;
        loop {
            let c = match self.peek() {
                Some(c) => c,
                None => return err(self.line, "unterminated literal string"),
            };
            if multi {
                if self.src[self.at..].starts_with(b"'''") {
                    let s = String::from_utf8_lossy(&self.src[start..self.at]).into_owned();
                    self.at += 3;
                    return Ok(s);
                }
            } else if c == b'\'' {
                let s = String::from_utf8_lossy(&self.src[start..self.at]).into_owned();
                self.at += 1;
                return Ok(s);
            }
            if c == b'\n' {
                self.line += 1;
            }
            self.at += 1;
        }
    }
    fn value(&mut self) -> Res<Value> {
        match self.peek() {
            Some(b'"') => Ok(Value::Str(self.basic_string()?)),
            Some(b'\'') => Ok(Value::Str(self.literal_string()?)),
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_blank();
                    if self.eat(b']') {
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value()?);
                    self.skip_blank();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(Value::Array(items));
                    }
                    return err(self.line, "expected `,` or `]` in an array");
                }
            }
            Some(b'{') => {
                self.at += 1;
                let mut t = Table::new();
                loop {
                    self.skip_blank();
                    if self.eat(b'}') {
                        return Ok(Value::Table(t));
                    }
                    let path = self.key_path()?;
                    self.skip_spaces();
                    if !self.eat(b'=') {
                        return err(self.line, "expected `=` in an inline table");
                    }
                    self.skip_spaces();
                    let v = self.value()?;
                    insert(&mut t, &path, v, self.line)?;
                    self.skip_blank();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(Value::Table(t));
                    }
                    return err(self.line, "expected `,` or `}` in an inline table");
                }
            }
            _ => self.scalar(),
        }
    }
    fn scalar(&mut self) -> Res<Value> {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c == b',' || c == b']' || c == b'}' || c == b'\n' || c == b'#' || c == b'\r' {
                break;
            }
            self.at += 1;
        }
        let raw = String::from_utf8_lossy(&self.src[start..self.at])
            .trim()
            .to_string();
        if raw.is_empty() {
            return err(self.line, "expected a value");
        }
        match raw.as_str() {
            "true" => return Ok(Value::Bool(true)),
            "false" => return Ok(Value::Bool(false)),
            _ => {}
        }
        let clean: String = raw.chars().filter(|c| *c != '_').collect();
        if let Ok(i) = clean.parse::<i64>() {
            return Ok(Value::Int(i));
        }
        if let Some(hex) = clean.strip_prefix("0x") {
            if let Ok(i) = i64::from_str_radix(hex, 16) {
                return Ok(Value::Int(i));
            }
        }
        if let Ok(f) = clean.parse::<f64>() {
            return Ok(Value::Float(f));
        }
        if clean == "inf" || clean == "+inf" {
            return Ok(Value::Float(f64::INFINITY));
        }
        if clean == "-inf" {
            return Ok(Value::Float(f64::NEG_INFINITY));
        }
        if clean == "nan" {
            return Ok(Value::Float(f64::NAN));
        }
        // `// UNKNOWN:` dates and times are kept verbatim as strings.
        Ok(Value::Str(raw))
    }
}

// ---------------------------------------------------------------- writing

/// Renders a [`Table`] back to TOML text.
///
/// Scalars go first in key order, then sub-tables, then arrays of tables —
/// the order `toml::to_string` produces.
pub fn to_string(table: &Table) -> String {
    let mut out = String::new();
    write_table(&mut out, table, &[]);
    out
}

fn write_table(out: &mut String, table: &Table, path: &[String]) {
    for (k, v) in table {
        if is_scalar(v) {
            let _ = write!(out, "{} = {}\n", quote_key(k), write_value(v));
        }
    }
    for (k, v) in table {
        if let Value::Table(inner) = v {
            let mut sub = path.to_vec();
            sub.push(k.clone());
            out.push('\n');
            let _ = writeln!(out, "[{}]", sub.iter().map(|s| quote_key(s)).collect::<Vec<_>>().join("."));
            write_table(out, inner, &sub);
        }
    }
    for (k, v) in table {
        if let Value::Tables(list) = v {
            let mut sub = path.to_vec();
            sub.push(k.clone());
            for t in list {
                out.push('\n');
                let _ = writeln!(
                    out,
                    "[[{}]]",
                    sub.iter().map(|s| quote_key(s)).collect::<Vec<_>>().join(".")
                );
                write_table(out, t, &sub);
            }
        }
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Table(_) | Value::Tables(_))
}

fn quote_key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_string()
    } else {
        let mut s = String::from("\"");
        for c in k.chars() {
            match c {
                '"' => s.push_str("\\\""),
                '\\' => s.push_str("\\\\"),
                '\n' => s.push_str("\\n"),
                _ => s.push(c),
            }
        }
        s.push('"');
        s
    }
}

fn write_value(v: &Value) -> String {
    match v {
        Value::Str(s) => {
            let mut out = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    _ => out.push(c),
                }
            }
            out.push('"');
            out
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.fract() == 0.0 && f.is_finite() {
                format!("{f:.1}")
            } else {
                format!("{f}")
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(write_value).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Table(t) => {
            let inner: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{} = {}", quote_key(k), write_value(v)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
        Value::Tables(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tables_and_scalars() {
        let doc = parse(
            r##"
# a comment
name = "Mist"
count = 3
ratio = 0.5
on = true
list = [1, 2, 3]

[names]
text = "#b4b4b4"

[shape.corners]
stroke = "flat"
"##,
        )
        .unwrap();
        assert_eq!(doc["name"].as_str(), Some("Mist"));
        assert_eq!(doc["count"].as_i64(), Some(3));
        assert_eq!(doc["ratio"].as_f64(), Some(0.5));
        assert_eq!(doc["on"].as_bool(), Some(true));
        assert_eq!(doc["list"].as_array().unwrap().len(), 3);
        assert_eq!(
            doc["names"].as_table().unwrap()["text"].as_str(),
            Some("#b4b4b4")
        );
        assert_eq!(
            doc["shape"].as_table().unwrap()["corners"]
                .as_table()
                .unwrap()["stroke"]
                .as_str(),
            Some("flat")
        );
    }

    #[test]
    fn reads_array_of_tables_and_inline_tables() {
        let doc = parse(
            r##"
[[rewards]]
name = "silver"
weight = 1

[[rewards]]
name = "gems"

[hole]
inline = { x = 1, y = 2 }
"##,
        )
        .unwrap();
        let rewards = doc["rewards"].as_tables().unwrap();
        assert_eq!(rewards.len(), 2);
        assert_eq!(rewards[0]["name"].as_str(), Some("silver"));
        assert_eq!(rewards[1]["name"].as_str(), Some("gems"));
        let inline = doc["hole"].as_table().unwrap()["inline"].as_table().unwrap();
        assert_eq!(inline["y"].as_i64(), Some(2));
    }

    #[test]
    fn reads_literal_and_multiline_strings() {
        let doc = parse(
            "a = 'C:\\path'\nb = \"\"\"\nline1\nline2\"\"\"\n",
        )
        .unwrap();
        assert_eq!(doc["a"].as_str(), Some("C:\\path"));
        assert_eq!(doc["b"].as_str(), Some("line1\nline2"));
    }

    #[test]
    fn round_trips_scalars() {
        let mut t = Table::new();
        t.insert("name".into(), Value::Str("Light".into()));
        t.insert("n".into(), Value::Int(7));
        t.insert("x".into(), Value::Float(1.5));
        let text = to_string(&t);
        let back = parse(&text).unwrap();
        assert_eq!(back["name"].as_str(), Some("Light"));
        assert_eq!(back["n"].as_i64(), Some(7));
        assert_eq!(back["x"].as_f64(), Some(1.5));
    }
}
