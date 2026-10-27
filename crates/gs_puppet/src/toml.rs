//! A tiny TOML reader — just enough for the shipped pose files.
//!
//! The original crate used `toml` + `serde`; this crate is `std`-only, so the
//! subset the pose data actually needs is implemented here: `#` comments,
//! `[a.b.c]` table headers, `key = value` with `"strings"`, numbers, booleans,
//! nested `[ … ]` arrays and single-line `{ … }` inline tables.  Nothing else
//! appears in `poses/*.toml` (checked against all 13 shipped files), and
//! anything outside the subset is a hard parse error rather than a silent
//! misread.
//!
//! **std departure** (was `toml`/`serde`).

use std::collections::BTreeMap;

/// A parsed TOML value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    Arr(Vec<Value>),
    Table(Table),
}

/// An insertion-ordered-by-key table (the pose data is looked up by name, so
/// ordering never matters, but `BTreeMap` keeps output deterministic).
pub type Table = BTreeMap<String, Value>;

impl Value {
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|n| n as f32)
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_f64().filter(|n| *n >= 0.0).map(|n| n as usize)
    }

    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(a) => Some(a),
            _ => None,
        }
    }

    /// `[f32; 3]` — used for every `rot` / `root` triple.
    pub fn as_vec3(&self) -> Option<[f32; 3]> {
        let a = self.as_arr()?;
        if a.len() != 3 {
            return None;
        }
        Some([a[0].as_f32()?, a[1].as_f32()?, a[2].as_f32()?])
    }

    /// `[f32; N]` — used for `bend` (one value per inner segment).
    pub fn as_f32_list(&self) -> Option<Vec<f32>> {
        let a = self.as_arr()?;
        a.iter().map(|v| v.as_f32()).collect()
    }

    /// `[[f32; 2]; N]` — used for every limit range.
    pub fn as_ranges(&self) -> Option<Vec<[f32; 2]>> {
        let a = self.as_arr()?;
        let mut out = Vec::with_capacity(a.len());
        for v in a {
            let inner = v.as_arr()?;
            if inner.len() != 2 {
                return None;
            }
            out.push([inner[0].as_f32()?, inner[1].as_f32()?]);
        }
        Some(out)
    }
}

/// Parse a document.  Errors carry a line number.
pub fn parse(src: &str) -> Result<Table, String> {
    let mut root = Table::new();
    let mut path: Vec<String> = Vec::new();
    for (lineno, raw) in src.lines().enumerate() {
        let lineno = lineno + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("[[") {
            // Array of tables: `[[name.sub]]` appends a fresh table.
            let Some(inner) = rest.strip_suffix("]]") else {
                return Err(format!("line {lineno}: unterminated array header"));
            };
            let inner = inner.trim();
            if inner.is_empty() {
                return Err(format!("line {lineno}: empty array header"));
            }
            path = inner.split('.').map(|s| s.trim().to_string()).collect();
            let mut cur = &mut root;
            for seg in &path[..path.len() - 1] {
                let entry = cur
                    .entry(seg.clone())
                    .or_insert_with(|| Value::Table(Table::new()));
                if entry.as_table().is_none() {
                    return Err(format!("line {lineno}: `{seg}` is not a table"));
                }
                cur = match entry {
                    Value::Table(t) => t,
                    _ => unreachable!(),
                };
            }
            let last = path.last().unwrap().clone();
            let entry = cur
                .entry(last)
                .or_insert_with(|| Value::Arr(Vec::new()));
            let Value::Arr(items) = entry else {
                return Err(format!("line {lineno}: `{}` is not an array", path.last().unwrap()));
            };
            items.push(Value::Table(Table::new()));
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let Some(inner) = rest.strip_suffix(']') else {
                return Err(format!("line {lineno}: unterminated table header"));
            };
            let inner = inner.trim();
            if inner.is_empty() {
                return Err(format!("line {lineno}: empty table header"));
            }
            path = inner.split('.').map(|s| s.trim().to_string()).collect();
            if path.iter().any(|s| s.is_empty()) {
                return Err(format!("line {lineno}: empty key in header"));
            }
            // Materialise the table chain.
            let mut cur = &mut root;
            for seg in &path {
                let entry = cur
                    .entry(seg.clone())
                    .or_insert_with(|| Value::Table(Table::new()));
                if entry.as_table().is_none() {
                    return Err(format!("line {lineno}: `{seg}` is not a table"));
                }
                cur = match entry {
                    Value::Table(t) => t,
                    _ => unreachable!(),
                };
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {lineno}: expected `key = value`"));
        };
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {lineno}: empty key"));
        }
        let value = parse_value(value.trim(), lineno)?;
        let mut cur = &mut root;
        for seg in &path {
            cur = match cur
                .entry(seg.clone())
                .or_insert_with(|| Value::Table(Table::new()))
            {
                Value::Table(t) => t,
                // `[[arr]]` leaves an array here: the following keys belong to its
                // last element.
                Value::Arr(items) => match items.last_mut() {
                    Some(Value::Table(t)) => t,
                    _ => return Err(format!("line {lineno}: `{seg}` is not a table")),
                },
                _ => return Err(format!("line {lineno}: `{seg}` is not a table")),
            };
        }
        cur.insert(key.to_string(), value);
    }
    Ok(root)
}

/// Cut a `#` comment that is not inside a string.
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'#' if !in_str => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

fn parse_value(s: &str, lineno: usize) -> Result<Value, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        let Some(body) = rest.strip_suffix('"') else {
            return Err(format!("line {lineno}: unterminated string"));
        };
        // The pose data contains no escapes; keep it simple but reject them
        // loudly rather than silently mangling a value.
        if body.contains('\\') {
            return Err(format!("line {lineno}: escapes are not supported"));
        }
        return Ok(Value::Str(body.to_string()));
    }
    if let Some(rest) = s.strip_prefix('[') {
        let Some(body) = rest.strip_suffix(']') else {
            return Err(format!("line {lineno}: unterminated array"));
        };
        let mut out = Vec::new();
        for item in split_top_level(body) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            out.push(parse_value(item, lineno)?);
        }
        return Ok(Value::Arr(out));
    }
    if let Some(rest) = s.strip_prefix('{') {
        let Some(body) = rest.strip_suffix('}') else {
            return Err(format!("line {lineno}: unterminated inline table"));
        };
        let mut out = Table::new();
        for item in split_top_level(body) {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let Some((k, v)) = item.split_once('=') else {
                return Err(format!("line {lineno}: bad inline pair `{item}`"));
            };
            out.insert(k.trim().to_string(), parse_value(v.trim(), lineno)?);
        }
        return Ok(Value::Table(out));
    }
    match s {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    s.parse::<f64>()
        .map(Value::Num)
        .map_err(|_| format!("line {lineno}: bad value `{s}`"))
}

/// Split on commas that are not inside `[]`, `{}` or a string.
fn split_top_level(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'[' | b'{' if !in_str => depth += 1,
            b']' | b'}' if !in_str => depth -= 1,
            b',' if !in_str && depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headers_scalars_arrays_and_inline_tables() {
        let src = r#"
# a comment
body_type = "biped"

[pose.arms_up]
arm = { rot = [0.0, 0.0, 152.0], bend = [-14.0] }

[limits]
leg = { rot = [[-110.0, 90.0], [-30.0, 30.0], [-60.0, 35.0]] }

[idle]
period = 3.6
"#;
        let doc = parse(src).unwrap();
        assert_eq!(doc["body_type"].as_str(), Some("biped"));
        let pose = doc["pose"].as_table().unwrap();
        let arms = pose["arms_up"].as_table().unwrap();
        let arm = arms["arm"].as_table().unwrap();
        assert_eq!(arm["rot"].as_vec3(), Some([0.0, 0.0, 152.0]));
        assert_eq!(arm["bend"].as_f32_list(), Some(vec![-14.0]));
        let limits = doc["limits"].as_table().unwrap();
        let leg = limits["leg"].as_table().unwrap();
        assert_eq!(
            leg["rot"].as_ranges(),
            Some(vec![[-110.0, 90.0], [-30.0, 30.0], [-60.0, 35.0]])
        );
        assert_eq!(doc["idle"].as_table().unwrap()["period"].as_f32(), Some(3.6));
    }

    #[test]
    fn comments_inside_strings_are_kept() {
        let doc = parse("k = \"a # b\" # trailing\n").unwrap();
        assert_eq!(doc["k"].as_str(), Some("a # b"));
    }

    #[test]
    fn parses_arrays_of_tables() {
        let src = "[[segment]]\nname = \"root\"\nradius = 0.1\n[[segment]]\nname = \"neck\"\nradius = 0.2\n";
        let doc = parse(src).unwrap();
        let arr = doc["segment"].as_arr().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_table().unwrap()["name"].as_str(), Some("root"));
        assert_eq!(arr[1].as_table().unwrap()["radius"].as_f32(), Some(0.2));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("no equals sign\n").is_err());
        assert!(parse("k = [1, 2\n").is_err());
        assert!(parse("k = \"open\n").is_err());
    }
}
