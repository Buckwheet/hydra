//! The little TOML `rewards.toml` needs: three keys, strings, booleans and an
//! array of strings.
//!
//! **Departure**: the original used `toml`/`serde` (`toml::de::error::Error`'s
//! `Display` at `0x1014c8888`, `toml::ser::to_string` at `0x1014c952c`, the
//! `Raw` `visit_map` at `0x1014db690`, and the `toml_datetime` panics for the
//! `time` value).  This crate is `std`-only, so the subset the file actually
//! uses is read and written here:
//!
//! * `key = <value>` lines, `[table]` headers (skipped), `#` comments;
//! * basic strings (`"…"` with the usual escapes), booleans and integers;
//! * single-line arrays of those scalars.
//!
//! Unknown keys are kept in the document and ignored by the reader, which is
//! what `serde`'s `Raw` did with them (its visitor's "unknown field
//! `{}`, there are no fields" arm at `0x103e6b8d0`).

/// A parsed value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    /// insertion ordered, so a rewrite keeps the file's order
    Obj(Vec<(String, Value)>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn bool_of(&self, key: &str) -> bool {
        matches!(self.get(key), Some(Value::Bool(true)))
    }
}

/// Parse the subset described above.  The error is a one-line message.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut doc: Vec<(String, Value)> = Vec::new();
    for (number, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected `key = value`", number + 1))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {}: empty key", number + 1));
        }
        let key = key
            .strip_prefix('"')
            .and_then(|k| k.strip_suffix('"'))
            .unwrap_or(key);
        let value = parse_value(value.trim())
            .ok_or_else(|| format!("line {}: cannot read the value", number + 1))?;
        doc.push((key.to_string(), value));
    }
    Ok(Value::Obj(doc))
}

/// Render a document.
pub fn to_toml(value: &Value) -> String {
    let mut out = String::new();
    if let Value::Obj(pairs) = value {
        for (key, value) in pairs {
            match value {
                Value::Arr(items) => {
                    let items: Vec<String> = items.iter().map(scalar).collect();
                    out.push_str(&format!("{key} = [{}]\n", items.join(", ")));
                }
                other => out.push_str(&format!("{key} = {}\n", scalar(other))),
            }
        }
    }
    out
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.fract() == 0.0 {
                format!("{}", *n as i64)
            } else {
                format!("{n}")
            }
        }
        Value::Str(s) => format!("\"{}\"", escape(s)),
        Value::Arr(_) | Value::Obj(_) => "\"\"".to_string(),
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Drop a `#` comment that is not inside a string.
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '#' => return &line[..i],
            _ => {}
        }
    }
    line
}

fn parse_value(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(inner) = text.strip_prefix('[') {
        let inner = inner.strip_suffix(']')?;
        let mut items = Vec::new();
        for item in split_items(inner) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            items.push(parse_value(item)?);
        }
        return Some(Value::Arr(items));
    }
    if text.starts_with('"') {
        return unquote(text).map(Value::Str);
    }
    match text {
        "true" => return Some(Value::Bool(true)),
        "false" => return Some(Value::Bool(false)),
        _ => {}
    }
    text.parse::<f64>().ok().map(Value::Num)
}

fn split_items(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in inner.chars() {
        if in_string {
            current.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                current.push(c);
            }
            ',' => out.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn unquote(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => return None,
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_reads_the_three_keys() {
        let doc = parse(
            "# the login reward\ntime = \"19:15\"\nskip = [\"Aelith\", \"GS4:Bob\"]\nstore = true\n",
        )
        .unwrap();
        assert_eq!(doc.get("time").unwrap().as_str(), Some("19:15"));
        assert_eq!(
            doc.get("skip").unwrap().as_array().unwrap().len(),
            2
        );
        assert!(doc.bool_of("store"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let doc = parse("\n# hi\n\nstore = false  # off\n").unwrap();
        assert!(!doc.bool_of("store"));
    }

    #[test]
    fn a_hash_inside_a_string_is_kept() {
        let doc = parse("time = \"19:15 # not a comment\"\n").unwrap();
        assert_eq!(
            doc.get("time").unwrap().as_str(),
            Some("19:15 # not a comment")
        );
    }

    #[test]
    fn a_bad_line_is_an_error() {
        assert!(parse("time\n").is_err());
        assert!(parse("time = \n").is_err());
    }

    #[test]
    fn it_writes_what_it_reads() {
        let text = "time = \"19:15\"\nskip = [\"Aelith\", \"GS4:Bob\"]\nstore = true\n";
        let doc = parse(text).unwrap();
        assert_eq!(to_toml(&doc), text);
        assert_eq!(parse(&to_toml(&doc)).unwrap(), doc);
    }

    #[test]
    fn escapes_survive_a_round_trip() {
        let doc = Value::Obj(vec![(
            "note".to_string(),
            Value::Str("a \"quoted\" \\ back\nslash".to_string()),
        )]);
        assert_eq!(parse(&to_toml(&doc)).unwrap(), doc);
    }
}
