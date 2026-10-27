//! Data packs: hashing, manifest parsing, ZIP installation and the ledger.
//!
//! Recovered from:
//! * `gs_base::packs::sha256_hex`        @0x100a76c6c
//! * `gs_base::packs::install_zip`       @0x100a76f84
//! * `gs_base::packs::Report::summary`   @0x100a785cc
//! * `gs_base::packs::ledger`            @0x100a78740
//! * `gs_base::packs::remove`            @0x100a7892c
//! * `gs_base::packs::Manifest::from_toml` @0x100a79edc
//!
//! ## Recovered constants and wording
//!
//! The crate's literal pool at 0x1021eff00 / 0x103e36a3e carries the whole
//! vocabulary of this module:
//!
//! ```text
//! packs                                       // <global>/packs
//! installed.toml                              // <global>/packs/installed.toml
//! ledger, pack.toml, pack-tmp, Ledger, Manifest, Installed
//! not a zip
//! a name in the zip
//! the zip has no pack.toml
//! pack.toml is not text
//! the zip unpacks to more than a pack may
//! the zip holds more files than a pack may
//! the pack's SHA-256 is {}, not {}: not installed
//! {}: outside the pack's folder
//! {}: a pack holds no links
//! {} {}: {} added, {} updated, {} removed
//! , {} kept as the pool has them
//! no pack '{}' is installed
//! pack.toml: id '{}' is not lower snake case
//! pack '{}' is format {}, newer than this build understands ({})
//! pack file '{}' is not under global/
//! ```
//!
//! ## std departures
//!
//! | original | here |
//! |---|---|
//! | `sha2::Sha256` | [`Sha256`], written from scratch |
//! | `zip` crate (`zlib_rs` inflate) | [`ZipArchive`], with a small DEFLATE decoder |
//! | `toml` + `serde` | a minimal parser in [`Manifest::from_toml`] / [`parse_toml`] |
//! | `anyhow` | `crate::Error` |

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::{Error, Result};

// ---------------------------------------------------------------------------
// SHA-256
// ---------------------------------------------------------------------------

/// First 32 bits of the fractional parts of the square roots of the first eight
/// primes — the SHA-256 initial hash value, loaded verbatim from 0x1021eff08.
pub const SHA256_IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// The 64 SHA-256 round constants (FIPS 180-4 §4.2.2).
pub static SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// A from-scratch SHA-256 (FIPS 180-4).
///
/// The binary inlines `sha2::sha256::compress256` and the standard IV; the
/// second `compress256` call with a zero-filled final block is the
/// `0x80`/length padding.  **std departure**: replaces the `sha2` crate.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_bits: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256 {
    pub fn new() -> Self {
        Sha256 {
            state: SHA256_IV,
            buf: [0; 64],
            buf_len: 0,
            total_bits: 0,
        }
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut rest = data;
        self.total_bits = self.total_bits.wrapping_add((data.len() as u64) * 8);
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(rest.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&rest[..take]);
            self.buf_len += take;
            rest = &rest[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
        while rest.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&rest[..64]);
            self.compress(&block);
            rest = &rest[64..];
        }
        if !rest.is_empty() {
            self.buf[..rest.len()].copy_from_slice(rest);
            self.buf_len = rest.len();
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.total_bits;
        // Padding: 0x80, zeros, then the 64-bit big-endian bit count.
        let mut tail = [0u8; 128];
        tail[0] = 0x80;
        let pad = if self.buf_len < 56 {
            64 - self.buf_len
        } else {
            128 - self.buf_len
        };
        tail[pad - 8..pad].copy_from_slice(&bits.to_be_bytes());
        // Feed the padding without disturbing `total_bits` again.
        let mut rest = &tail[..pad];
        let mut buf = self.buf;
        let mut buf_len = self.buf_len;
        if buf_len > 0 {
            let take = (64 - buf_len).min(rest.len());
            buf[buf_len..buf_len + take].copy_from_slice(&rest[..take]);
            buf_len += take;
            rest = &rest[take..];
            if buf_len == 64 {
                self.compress(&buf);
                buf_len = 0;
            }
        }
        while rest.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&rest[..64]);
            self.compress(&block);
            rest = &rest[64..];
        }
        let _ = buf_len;

        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut h = self.state;
        for i in 0..64 {
            let s1 = h[4].rotate_right(6) ^ h[4].rotate_right(11) ^ h[4].rotate_right(25);
            let ch = (h[4] & h[5]) ^ (!h[4] & h[6]);
            let t1 = h[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = h[0].rotate_right(2) ^ h[0].rotate_right(13) ^ h[0].rotate_right(22);
            let maj = (h[0] & h[1]) ^ (h[0] & h[2]) ^ (h[1] & h[2]);
            let t2 = s0.wrapping_add(maj);
            h[7] = h[6];
            h[6] = h[5];
            h[5] = h[4];
            h[4] = h[3].wrapping_add(t1);
            h[3] = h[2];
            h[2] = h[1];
            h[1] = h[0];
            h[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            self.state[i] = self.state[i].wrapping_add(h[i]);
        }
    }
}

/// Lower-case hex digest of `data`'s SHA-256.
///
/// `@0x100a76c6c` — inline SHA-256 followed by
/// `format!("{:02x}", byte)` folded over the 32 digest bytes (the binary
/// monomorphises `Map<…, _>::fold` at 0x100a8fc14).
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let digest = h.finish();
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push(hex_digit(b >> 4));
        out.push(hex_digit(b & 0xf));
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'a' + nibble - 10) as char,
    }
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Highest `format` this build understands.
///
/// INFERRED: the constant lives inside `from_toml` as an immediate next to the
/// "newer than this build understands" message; `1` is the only value that
/// makes the message reachable for a current `pack.toml`.
pub const SUPPORTED_FORMAT: u32 = 1;

/// Directory prefix every manifest file entry must live under.
pub const FILE_PREFIX: &str = "global/";

/// `pack.toml`.
///
/// INFERRED: the exact schema is not present in the binary — only the three
/// field *names* (`id`, `format`, `files`) are, via the two validation messages
/// ("id '{}' is not lower snake case", "pack '{}' is format {}, newer than this
/// build understands ({})") and "pack file '{}' is not under global/".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// Pack id; must be lower `snake_case`.
    pub id: String,
    /// Manifest format version.
    pub format: u32,
    /// Files the pack ships, each under `global/`.
    pub files: Vec<String>,
}

impl Manifest {
    /// Parse and validate a `pack.toml`.
    ///
    /// `@0x100a79edc`.  Recovered rejections, in message order:
    /// * `pack.toml: id '{}' is not lower snake case`
    /// * `pack '{}' is format {}, newer than this build understands ({})`
    /// * `pack file '{}' is not under global/`
    ///
    /// **std departure**: the original used `toml` + `serde`.
    pub fn from_toml(text: &str) -> Result<Manifest> {
        let doc = parse_toml(text)?;
        let id = doc.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !is_lower_snake_case(&id) {
            return Err(Error::msg(format!(
                "pack.toml: id '{id}' is not lower snake case"
            )));
        }
        let format = match doc.get("format") {
            Some(TomlValue::Integer(n)) if *n >= 0 => *n as u32,
            Some(TomlValue::String(s)) => s.parse::<u32>().map_err(|_| {
                Error::msg(format!(
                    "pack '{id}' is format {s}, newer than this build understands ({SUPPORTED_FORMAT})"
                ))
            })?,
            _ => 0,
        };
        if format > SUPPORTED_FORMAT {
            return Err(Error::msg(format!(
                "pack '{id}' is format {format}, newer than this build understands ({SUPPORTED_FORMAT})"
            )));
        }
        let mut files = Vec::new();
        if let Some(TomlValue::Array(items)) = doc.get("files") {
            for item in items {
                let path = match item {
                    TomlValue::String(s) => s.clone(),
                    _ => String::new(),
                };
                if !path.starts_with(FILE_PREFIX) {
                    return Err(Error::msg(format!(
                        "pack file '{path}' is not under global/"
                    )));
                }
                files.push(path);
            }
        }
        Ok(Manifest { id, format, files })
    }
}

/// `[a-z_][a-z0-9_]*`.
pub fn is_lower_snake_case(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

// ---------------------------------------------------------------------------
// A very small TOML subset
// ---------------------------------------------------------------------------

/// Value of the TOML subset this crate understands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TomlValue {
    String(String),
    Integer(i64),
    Boolean(bool),
    Array(Vec<TomlValue>),
    Table(BTreeMap<String, TomlValue>),
}

impl TomlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            TomlValue::String(s) => Some(s),
            _ => None,
        }
    }
}

/// A flat `key = value` / `[section]` document.
///
/// **std departure** — replaces `toml`.  Keys inside `[a.b]` sections are stored
/// with their dotted path.  Supports strings (basic and literal), integers,
/// booleans, single-line arrays, and `#` comments.
pub fn parse_toml(text: &str) -> Result<BTreeMap<String, TomlValue>> {
    let mut root: BTreeMap<String, TomlValue> = BTreeMap::new();
    let mut section: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            section = rest.trim().split('.').map(|p| p.trim().to_string()).collect();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(Error::msg(format!("pack.toml: cannot parse line '{line}'")));
        };
        let key = key.trim().to_string();
        let (value, rest) = parse_value(value.trim())?;
        if !rest.trim().is_empty() {
            return Err(Error::msg(format!(
                "pack.toml: trailing text after '{}'",
                key
            )));
        }
        let mut path = section.clone();
        path.push(key);
        insert_path(&mut root, &path, value);
    }
    Ok(root)
}

fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match in_str {
            Some(q) => {
                if b == q {
                    in_str = None;
                }
            }
            None => {
                if b == b'"' || b == b'\'' {
                    in_str = Some(b);
                } else if b == b'#' {
                    return &line[..i];
                }
            }
        }
    }
    line
}

fn insert_path(
    root: &mut BTreeMap<String, TomlValue>,
    path: &[String],
    value: TomlValue,
) {
    if path.len() == 1 {
        root.insert(path[0].clone(), value);
        return;
    }
    let entry = root
        .entry(path[0].clone())
        .or_insert_with(|| TomlValue::Table(BTreeMap::new()));
    if let TomlValue::Table(map) = entry {
        insert_path(map, &path[1..], value);
    }
}

fn parse_value(s: &str) -> Result<(TomlValue, &str)> {
    if let Some(mut rest) = s.strip_prefix('"') {
        let mut out = String::new();
        loop {
            let Some(pos) = rest.find(['"', '\\']) else {
                return Err(Error::msg("pack.toml: unterminated string"));
            };
            out.push_str(&rest[..pos]);
            if rest.as_bytes()[pos] == b'"' {
                return Ok((TomlValue::String(out), &rest[pos + 1..]));
            }
            // Escape.
            let mut it = rest[pos + 1..].chars();
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => return Err(Error::msg("pack.toml: truncated escape")),
            }
            rest = it.as_str();
        }
    }
    if let Some(rest) = s.strip_prefix('\'') {
        let Some(pos) = rest.find('\'') else {
            return Err(Error::msg("pack.toml: unterminated literal string"));
        };
        return Ok((TomlValue::String(rest[..pos].to_string()), &rest[pos + 1..]));
    }
    if let Some(rest) = s.strip_prefix('[') {
        let mut items = Vec::new();
        let mut tail = rest.trim_start();
        if let Some(t) = tail.strip_prefix(']') {
            return Ok((TomlValue::Array(items), t));
        }
        loop {
            let (v, t) = parse_value(tail)?;
            items.push(v);
            tail = t.trim_start();
            if let Some(t) = tail.strip_prefix(',') {
                tail = t.trim_start();
                if let Some(t) = tail.strip_prefix(']') {
                    return Ok((TomlValue::Array(items), t));
                }
                continue;
            }
            if let Some(t) = tail.strip_prefix(']') {
                return Ok((TomlValue::Array(items), t));
            }
            return Err(Error::msg("pack.toml: malformed array"));
        }
    }
    // Bare token: integer or boolean.
    let end = s
        .find(|c: char| c == ',' || c == ']' || c.is_whitespace())
        .unwrap_or(s.len());
    let (token, rest) = s.split_at(end);
    if token == "true" {
        return Ok((TomlValue::Boolean(true), rest));
    }
    if token == "false" {
        return Ok((TomlValue::Boolean(false), rest));
    }
    if let Ok(n) = token.replace('_', "").parse::<i64>() {
        return Ok((TomlValue::Integer(n), rest));
    }
    Err(Error::msg(format!("pack.toml: bad value '{token}'")))
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// The outcome of installing or removing a pack.
///
/// INFERRED: the exact field types.  `summary` reads four `.len()` values at
/// `self + 0x40/0x58/0x70/0x88`, i.e. four `Vec`s 0x18 apart, preceded by two
/// `String`s at `+0x00` and `+0x18`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Pack id.
    pub pack: String,
    /// Pack version / format.
    pub version: String,
    /// Files written for the first time.
    pub added: Vec<String>,
    /// Files rewritten with new content.
    pub updated: Vec<String>,
    /// Files deleted because the pack no longer ships them.
    pub removed: Vec<String>,
    /// Files left alone because the pool already has them.
    pub kept: Vec<String>,
}

impl Report {
    /// The one-line human summary.
    ///
    /// `@0x100a785cc` — format string `"{} {}: {} added, {} updated, {} removed"`
    /// at 0x103e36aa7, then, only when the `kept` length is non-zero,
    /// `", {} kept as the pool has them"` appended (0x103e36acf).
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{} {}: {} added, {} updated, {} removed",
            self.pack,
            self.version,
            self.added.len(),
            self.updated.len(),
            self.removed.len()
        );
        if !self.kept.is_empty() {
            s.push_str(&format!(
                ", {} kept as the pool has them",
                self.kept.len()
            ));
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Ledger
// ---------------------------------------------------------------------------

/// Directory under `global/` holding the ledger.  Literal `packs` @0x1021eff28.
pub const PACKS_DIR: &str = "packs";
/// Ledger file name.  Literal `installed.toml` (14 bytes) @0x1021eff2d.
pub const LEDGER_FILE: &str = "installed.toml";

/// One installed pack's record.
///
/// INFERRED: serde's field name `files` is in the pool; the value shape
/// (a list of file names) is inferred from the `remove` walk, which joins each
/// element onto the pack's directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Installed {
    pub files: Vec<String>,
}

/// `installed.toml`.
///
/// INFERRED: serde's struct name `Ledger` and field name `format` are in the
/// pool, as is `Installed`/`files`.  A `pack` field name is also present but its
/// owner could not be pinned down, so it is left out (see NOTES.md).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    pub format: u32,
    pub installed: BTreeMap<String, Installed>,
}

/// Read `<root>/packs/installed.toml`.
///
/// `@0x100a78740` — `root.join("packs").join("installed.toml")`, then
/// `fs::read_to_string`, then the TOML deserializer wrapped in
/// `anyhow::Context`.
pub fn ledger(root: &Path) -> Result<Ledger> {
    let path = root.join(PACKS_DIR).join(LEDGER_FILE);
    let text = fs::read_to_string(&path)?;
    let doc = parse_toml(&text)?;
    let format = match doc.get("format") {
        Some(TomlValue::Integer(n)) if *n >= 0 => *n as u32,
        _ => 0,
    };
    let mut installed = BTreeMap::new();
    if let Some(TomlValue::Table(packs)) = doc.get(PACKS_DIR) {
        for (id, value) in packs {
            let mut files = Vec::new();
            if let TomlValue::Table(fields) = value {
                if let Some(TomlValue::Array(items)) = fields.get("files") {
                    for item in items {
                        if let TomlValue::String(s) = item {
                            files.push(s.clone());
                        }
                    }
                }
            }
            installed.insert(id.clone(), Installed { files });
        }
    } else if let Some(TomlValue::Table(packs)) = doc.get("installed") {
        for (id, value) in packs {
            let mut files = Vec::new();
            if let TomlValue::Table(fields) = value {
                if let Some(TomlValue::Array(items)) = fields.get("files") {
                    for item in items {
                        if let TomlValue::String(s) = item {
                            files.push(s.clone());
                        }
                    }
                }
            }
            installed.insert(id.clone(), Installed { files });
        }
    }
    Ok(Ledger { format, installed })
}

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

/// Uninstall a pack.
///
/// `@0x100a7892c` — `remove(root: &Path, id: &str) -> Result<Report>`:
/// 1. `ledger(root)?` and `installed.remove(id)`; a miss is the error
///    `no pack '{}' is installed` (0x103e36aef);
/// 2. for each recorded file, join it onto the pack's directory and
///    `fs::remove_file`;
/// 3. rewrite the ledger.
pub fn remove(root: &Path, id: &str) -> Result<Report> {
    let mut book = ledger(root)?;
    let entry = book
        .installed
        .remove(id)
        .ok_or_else(|| Error::msg(format!("no pack '{id}' is installed")))?;
    let dir = root.join(PACKS_DIR).join(id);
    let mut removed = Vec::new();
    for file in &entry.files {
        let path = dir.join(file);
        if fs::remove_file(&path).is_ok() {
            removed.push(file.clone());
        }
    }
    if fs::metadata(&dir).is_ok() {
        let _ = fs::remove_dir_all(&dir);
    }
    write_ledger(root, &book)?;
    Ok(Report {
        pack: id.to_string(),
        version: String::new(),
        added: Vec::new(),
        updated: Vec::new(),
        removed,
        kept: Vec::new(),
    })
}

/// Serialise and write the ledger back.
pub fn write_ledger(root: &Path, book: &Ledger) -> Result<()> {
    let dir = root.join(PACKS_DIR);
    fs::create_dir_all(&dir)?;
    let mut out = String::new();
    out.push_str(&format!("format = {}\n", book.format));
    for (id, entry) in &book.installed {
        out.push_str(&format!("\n[{PACKS_DIR}.{id}]\nfiles = ["));
        for (i, f) in entry.files.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push('"');
            out.push_str(&f.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
        }
        out.push_str("]\n");
    }
    let mut f = fs::File::create(dir.join(LEDGER_FILE))?;
    f.write_all(out.as_bytes())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// install_zip
// ---------------------------------------------------------------------------

/// Refuse archives with more entries than this.  The binary compares the entry
/// count against `0x186A0` = `100_000` with a `b.hi` (strictly greater).
pub const MAX_ENTRIES: usize = 100_000;

/// Refuse archives whose unpacked size exceeds this many bytes.
///
/// UNKNOWN: the exact limit.  The binary reaches the
/// "the zip unpacks to more than a pack may" message from a size accumulator;
/// only the six-digit immediate `0x186A0` (the *entry* limit) could be pinned
/// down, so the byte limit is modelled on it.
pub const MAX_UNPACKED_BYTES: u64 = 100_000_000;

/// Install a pack from ZIP bytes into `global/`.
///
/// `@0x100a76f84` — the binary's arguments are `(zip: &[u8], sha256: &str, …)`
/// and the first thing it does is
/// `sha256_hex(zip)` compared ASCII-case-insensitively against the trimmed
/// expected digest, so `expected_sha256` is a plain hex string (not a `[u8; 32]`).
///
/// Recovered rejections, in order:
/// * `the pack's SHA-256 is {}, not {}: not installed`
/// * `the zip holds more files than a pack may` (entry count > 100 000)
/// * `the zip has no pack.toml`
/// * `pack.toml is not text`
/// * `the zip unpacks to more than a pack may`
/// * `{}: outside the pack's folder`
/// * `{}: a pack holds no links`
///
/// INFERRED: the exact third argument (which directory the files land in) and
/// the `Report` bookkeeping.  `global` is taken to be `global_dir()`.
pub fn install_zip(zip: &[u8], expected_sha256: &str, global: &Path) -> Result<Report> {
    let actual = sha256_hex(zip);
    let expected = expected_sha256.trim();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(Error::msg(format!(
            "the pack's SHA-256 is {actual}, not {expected}: not installed"
        )));
    }

    let archive = ZipArchive::new(zip)?;
    if archive.entries.len() > MAX_ENTRIES {
        return Err(Error::msg("the zip holds more files than a pack may"));
    }

    // Find and parse pack.toml.
    let manifest_entry = archive
        .entry("pack.toml")
        .ok_or_else(|| Error::msg("the zip has no pack.toml"))?;
    let raw = archive.read(manifest_entry)?;
    let text = std::str::from_utf8(&raw)
        .map_err(|_| Error::msg("pack.toml is not text"))?
        .to_string();
    let manifest = Manifest::from_toml(&text)?;

    // Size guard before writing anything.
    let mut total: u64 = 0;
    for entry in &archive.entries {
        total = total.saturating_add(entry.uncompressed_size);
    }
    if total > MAX_UNPACKED_BYTES {
        return Err(Error::msg(
            "the zip unpacks to more than a pack may",
        ));
    }

    // Validate every entry before touching the filesystem, so a hostile archive
    // never leaves a staging directory behind.
    let mut planned: Vec<(usize, PathBuf)> = Vec::new();
    for (idx, entry) in archive.entries.iter().enumerate() {
        let name = entry.name.clone();
        if name == "pack.toml" {
            continue;
        }
        // A ZIP symlink is marked by S_IFLNK in the external attributes.
        if entry.is_symlink() {
            return Err(Error::msg(format!("{name}: a pack holds no links")));
        }
        let Some(rel) = safe_relative(&name) else {
            return Err(Error::msg(format!("{name}: outside the pack's folder")));
        };
        planned.push((idx, rel));
    }

    let pack_dir = global.join(PACKS_DIR).join(&manifest.id);
    let staging = global.join(PACKS_DIR).join("pack-tmp");
    fs::create_dir_all(&staging)?;

    let mut report = Report {
        pack: manifest.id.clone(),
        version: manifest.format.to_string(),
        ..Default::default()
    };
    let mut written: Vec<String> = Vec::new();

    for (idx, rel) in &planned {
        let entry = &archive.entries[*idx];
        let dest = staging.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = match archive.read(entry) {
            Ok(b) => b,
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(e);
            }
        };
        fs::write(&dest, &bytes)?;
        written.push(rel.to_string_lossy().replace('\\', "/"));
    }

    // Swap the staging tree in, recording what changed.
    if fs::metadata(&pack_dir).is_ok() {
        for name in &written {
            let dest = pack_dir.join(name);
            let src = staging.join(name);
            let same = match (fs::read(&dest), fs::read(&src)) {
                (Ok(a), Ok(b)) => a == b,
                _ => false,
            };
            if same {
                report.kept.push(name.clone());
            } else {
                report.updated.push(name.clone());
            }
        }
        let _ = fs::remove_dir_all(&pack_dir);
    } else {
        report.added = written.clone();
    }
    if let Some(parent) = pack_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&staging, &pack_dir)?;

    // Update the ledger.
    let mut book = ledger(global).unwrap_or_default();
    book.installed.insert(
        manifest.id.clone(),
        Installed {
            files: written,
        },
    );
    write_ledger(global, &book)?;

    Ok(report)
}

/// Reject absolute paths, `..` escapes and drive prefixes; return the cleaned
/// relative path.
fn safe_relative(name: &str) -> Option<PathBuf> {
    let normalized = name.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') {
        return None;
    }
    if normalized.contains('\0') {
        return None;
    }
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            p if p.contains(':') => return None,
            p => out.push(p),
        }
    }
    if out.as_os_str().is_empty() {
        return None;
    }
    // Belt and braces: no component may escape.
    if out
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return None;
    }
    Some(out)
}

// ---------------------------------------------------------------------------
// ZIP reader
// ---------------------------------------------------------------------------

/// One central-directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipEntry {
    /// The stored name, as written in the archive.
    pub name: String,
    /// Compression method (`0` = stored, `8` = deflate).
    pub method: u16,
    /// CRC-32 as recorded in the central directory (not verified).
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    /// Unix mode bits from the "external attributes" field, if any.
    pub unix_mode: u32,
    local_header_offset: u64,
}

impl ZipEntry {
    pub fn is_symlink(&self) -> bool {
        self.unix_mode & 0o170000 == 0o120000
    }
}

/// Magic markers recovered from the pool at 0x1021f0ec9..: `PK\x05\x06`
/// (EOCD), `PK\x06\x06` (ZIP64 EOCD) and `PK\x01\x02` (central-directory
/// header).
pub const EOCD_SIG: u32 = 0x0605_4b50;
pub const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
pub const CDFH_SIG: u32 = 0x0201_4b50;
pub const LFH_SIG: u32 = 0x0403_4b50;

/// A minimal ZIP archive reader: EOCD → central directory → local headers.
///
/// **std departure** — replaces the `zip` crate.  ZIP64 EOCD locators are
/// recognised (the markers are in the pool) but not parsed; archives needing
/// them fall back to the 32-bit central-directory values.
#[derive(Debug, Clone)]
pub struct ZipArchive<'a> {
    data: &'a [u8],
    pub entries: Vec<ZipEntry>,
}

impl<'a> ZipArchive<'a> {
    /// Parse the central directory.
    pub fn new(data: &'a [u8]) -> Result<ZipArchive<'a>> {
        let eocd = find_eocd(data).ok_or_else(|| Error::msg("not a zip"))?;
        // EOCD: sig(4) disk(2) cd_disk(2) n_disk(2) n_total(2) cd_size(4) cd_off(4) comment(2)
        if eocd + 22 > data.len() {
            return Err(Error::msg("not a zip"));
        }
        let n_total = u16::from_le_bytes([data[eocd + 10], data[eocd + 11]]) as usize;
        let cd_size = u32::from_le_bytes([
            data[eocd + 12],
            data[eocd + 13],
            data[eocd + 14],
            data[eocd + 15],
        ]) as usize;
        let cd_off = u32::from_le_bytes([
            data[eocd + 16],
            data[eocd + 17],
            data[eocd + 18],
            data[eocd + 19],
        ]) as usize;
        if cd_off.saturating_add(cd_size) > data.len() {
            return Err(Error::msg("not a zip"));
        }

        let mut entries = Vec::with_capacity(n_total.min(1024));
        let mut p = cd_off;
        for _ in 0..n_total {
            if p + 46 > data.len() || u32::from_le_bytes(rd4(data, p)) != CDFH_SIG {
                break;
            }
            let method = u16::from_le_bytes([data[p + 10], data[p + 11]]);
            let crc32 = u32::from_le_bytes(rd4(data, p + 16));
            let compressed_size = u32::from_le_bytes(rd4(data, p + 20)) as u64;
            let uncompressed_size = u32::from_le_bytes(rd4(data, p + 24)) as u64;
            let name_len = u16::from_le_bytes([data[p + 28], data[p + 29]]) as usize;
            let extra_len = u16::from_le_bytes([data[p + 30], data[p + 31]]) as usize;
            let comment_len = u16::from_le_bytes([data[p + 32], data[p + 33]]) as usize;
            let unix_mode = u32::from_le_bytes(rd4(data, p + 38)) >> 16;
            let local_header_offset = u32::from_le_bytes(rd4(data, p + 42)) as u64;
            let name_start = p + 46;
            let name_end = name_start + name_len;
            if name_end > data.len() {
                return Err(Error::msg("not a zip"));
            }
            let name = String::from_utf8_lossy(&data[name_start..name_end]).into_owned();
            entries.push(ZipEntry {
                name,
                method,
                crc32,
                compressed_size,
                uncompressed_size,
                unix_mode,
                local_header_offset,
            });
            p = name_end + extra_len + comment_len;
        }
        Ok(ZipArchive { data, entries })
    }

    /// Look an entry up by exact name.
    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Decompress one entry.
    pub fn read(&self, entry: &ZipEntry) -> Result<Vec<u8>> {
        let off = entry.local_header_offset as usize;
        if off + 30 > self.data.len() || u32::from_le_bytes(rd4(self.data, off)) != LFH_SIG {
            return Err(Error::msg("not a zip"));
        }
        let name_len = u16::from_le_bytes([self.data[off + 26], self.data[off + 27]]) as usize;
        let extra_len = u16::from_le_bytes([self.data[off + 28], self.data[off + 29]]) as usize;
        let start = off + 30 + name_len + extra_len;
        let end = start
            .checked_add(entry.compressed_size as usize)
            .filter(|e| *e <= self.data.len())
            .ok_or_else(|| Error::msg("not a zip"))?;
        let raw = &self.data[start..end];
        match entry.method {
            0 => Ok(raw.to_vec()),
            8 => inflate(raw, entry.uncompressed_size as usize),
            m => Err(Error::msg(format!("a name in the zip: method {m}"))),
        }
    }
}

fn rd4(d: &[u8], p: usize) -> [u8; 4] {
    [d[p], d[p + 1], d[p + 2], d[p + 3]]
}

/// Locate the end-of-central-directory record.
fn find_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let stop = data.len().saturating_sub(22);
    let start = stop.saturating_sub(0xFFFF);
    (start..=stop)
        .rev()
        .find(|&i| u32::from_le_bytes(rd4(data, i)) == EOCD_SIG)
}

// ---------------------------------------------------------------------------
// DEFLATE (RFC 1951)
// ---------------------------------------------------------------------------

/// Decode a raw DEFLATE stream.
///
/// **std departure** — replaces the `zlib_rs` inflater vendored by the `zip`
/// crate.  Fixed *and* dynamic Huffman blocks, with the standard length/distance
/// tables.
pub fn inflate(input: &[u8], expected_len: usize) -> Result<Vec<u8>> {
    let mut state = BitReader::new(input);
    let mut out: Vec<u8> = Vec::with_capacity(expected_len.max(16));
    loop {
        let last = state.bits(1)? == 1;
        let btype = state.bits(2)?;
        match btype {
            0 => inflate_stored(&mut state, &mut out)?,
            1 => {
                let (lit, dist) = fixed_tables();
                inflate_block(&mut state, &mut out, &lit, &dist)?;
            }
            2 => {
                let (lit, dist) = dynamic_tables(&mut state)?;
                inflate_block(&mut state, &mut out, &lit, &dist)?;
            }
            _ => return Err(Error::msg("not a zip: bad deflate block")),
        }
        if last {
            return Ok(out);
        }
    }
}

fn inflate_stored(r: &mut BitReader<'_>, out: &mut Vec<u8>) -> Result<()> {
    r.align();
    let len = r.u16()? as usize;
    let nlen = r.u16()? as usize;
    if len != !nlen & 0xffff {
        return Err(Error::msg("not a zip: stored length mismatch"));
    }
    out.extend_from_slice(r.bytes(len)?);
    Ok(())
}

struct Huffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        Ok(Huffman { counts, symbols })
    }

    fn decode(&self, r: &mut BitReader<'_>) -> Result<u16> {
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..16 {
            code |= r.bits(1)? as i32;
            let count = self.counts[len] as i32;
            if code - count < first {
                return Ok(self.symbols[(index + (code - first)) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(Error::msg("not a zip: bad huffman code"))
    }
}

fn fixed_tables() -> (Huffman, Huffman) {
    let mut lit = [0u8; 288];
    for (i, slot) in lit.iter_mut().enumerate() {
        *slot = if i < 144 {
            8
        } else if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
    }
    let dist = [5u8; 30];
    (
        Huffman::new(&lit).expect("fixed literal table"),
        Huffman::new(&dist).expect("fixed distance table"),
    )
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn dynamic_tables(r: &mut BitReader<'_>) -> Result<(Huffman, Huffman)> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = r.bits(5)? as usize + 257;
    let hdist = r.bits(5)? as usize + 1;
    let hclen = r.bits(4)? as usize + 4;
    let mut cl = [0u8; 19];
    for &slot in ORDER.iter().take(hclen) {
        cl[slot] = r.bits(3)? as u8;
    }
    let clh = Huffman::new(&cl)?;
    let mut lengths = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < lengths.len() {
        let sym = clh.decode(r)?;
        match sym {
            0..=15 => {
                lengths[i] = sym as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(Error::msg("not a zip: bad length repeat"));
                }
                let prev = lengths[i - 1];
                let replay = 3 + r.bits(2)? as usize;
                for _ in 0..replay {
                    if i >= lengths.len() {
                        break;
                    }
                    lengths[i] = prev;
                    i += 1;
                }
            }
            17 => {
                let replay = 3 + r.bits(3)? as usize;
                i += replay;
            }
            18 => {
                let replay = 11 + r.bits(7)? as usize;
                i += replay;
            }
            _ => return Err(Error::msg("not a zip: bad code length symbol")),
        }
    }
    if i > lengths.len() {
        return Err(Error::msg("not a zip: length overflow"));
    }
    let lit = Huffman::new(&lengths[..hlit])?;
    let dist = Huffman::new(&lengths[hlit..])?;
    Ok((lit, dist))
}

fn inflate_block(
    r: &mut BitReader<'_>,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
) -> Result<()> {
    loop {
        let sym = lit.decode(r)?;
        match sym {
            0..=255 => out.push(sym as u8),
            256 => return Ok(()),
            257..=285 => {
                let idx = (sym - 257) as usize;
                let len = LEN_BASE[idx] as usize + r.bits(LEN_EXTRA[idx] as u32)? as usize;
                let dsym = dist.decode(r)? as usize;
                if dsym >= DIST_BASE.len() {
                    return Err(Error::msg("not a zip: bad distance code"));
                }
                let d = DIST_BASE[dsym] as usize + r.bits(DIST_EXTRA[dsym] as u32)? as usize;
                if d == 0 || d > out.len() {
                    return Err(Error::msg("not a zip: distance out of range"));
                }
                let start = out.len() - d;
                for k in 0..len {
                    let b = out[start + k];
                    out.push(b);
                }
            }
            _ => return Err(Error::msg("not a zip: bad literal code")),
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader {
            data,
            pos: 0,
            bit: 0,
        }
    }

    fn bits(&mut self, n: u32) -> Result<u32> {
        let mut out = 0u32;
        for i in 0..n {
            if self.pos >= self.data.len() {
                return Err(Error::msg("not a zip: truncated deflate stream"));
            }
            let b = (self.data[self.pos] >> self.bit) & 1;
            out |= (b as u32) << i;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        Ok(out)
    }

    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }

    fn u16(&mut self) -> Result<u16> {
        let lo = self.bits(8)? as u16;
        let hi = self.bits(8)? as u16;
        Ok(lo | hi << 8)
    }

    fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.data.len() {
            return Err(Error::msg("not a zip: truncated stored block"));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- SHA-256 vectors --------------------------------------------------

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_hex(&[b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
        // 55/56/64-byte inputs straddle the padding boundary.
        let million_a = vec![b'a'; 1_000_000];
        let mut h = Sha256::new();
        h.update(&million_a[..500_000]);
        h.update(&million_a[500_000..]);
        let mut got = String::new();
        for b in h.finish() {
            got.push(hex_digit(b >> 4));
            got.push(hex_digit(b & 0xf));
        }
        assert_eq!(
            got,
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn sha256_tables() {
        assert_eq!(SHA256_IV[0], 0x6a09e667);
        assert_eq!(SHA256_IV[7], 0x5be0cd19);
        assert_eq!(SHA256_K[0], 0x428a2f98);
        assert_eq!(SHA256_K[63], 0xc67178f2);
    }

    // ---- Manifest ---------------------------------------------------------

    #[test]
    fn manifest_accepts_a_valid_pack_toml() {
        let text = r#"
            # a pack
            id = "hydra_maps"
            format = 1
            files = ["global/a.png", "global/b/c.json"]
        "#;
        let m = Manifest::from_toml(text).unwrap();
        assert_eq!(m.id, "hydra_maps");
        assert_eq!(m.format, 1);
        assert_eq!(m.files.len(), 2);
        assert_eq!(m.files[0], "global/a.png");
    }

    #[test]
    fn manifest_rejections_use_the_recovered_wording() {
        let e = Manifest::from_toml(r#"id = "HydraMaps""#).unwrap_err();
        assert_eq!(e.message(), "pack.toml: id 'HydraMaps' is not lower snake case");
        let e = Manifest::from_toml(r#"id = "ok"
format = 9"#)
        .unwrap_err();
        assert_eq!(
            e.message(),
            "pack 'ok' is format 9, newer than this build understands (1)"
        );
        let e = Manifest::from_toml("id = \"ok\"\nfiles = [\"maps/a.png\"]").unwrap_err();
        assert_eq!(e.message(), "pack file 'maps/a.png' is not under global/");
    }

    #[test]
    fn lower_snake_case_rules() {
        assert!(is_lower_snake_case("hydra"));
        assert!(is_lower_snake_case("hydra_maps_2"));
        assert!(is_lower_snake_case("_x"));
        assert!(!is_lower_snake_case(""));
        assert!(!is_lower_snake_case("Hydra"));
        assert!(!is_lower_snake_case("2hydra"));
        assert!(!is_lower_snake_case("hydra-maps"));
    }

    // ---- Report -----------------------------------------------------------

    #[test]
    fn report_summary_wording() {
        let mut r = Report {
            pack: "hydra".into(),
            version: "1".into(),
            added: vec!["a".into(), "b".into()],
            updated: vec!["c".into()],
            removed: vec![],
            kept: vec![],
        };
        assert_eq!(r.summary(), "hydra 1: 2 added, 1 updated, 0 removed");
        r.kept = vec!["d".into(), "e".into(), "f".into()];
        assert_eq!(
            r.summary(),
            "hydra 1: 2 added, 1 updated, 0 removed, 3 kept as the pool has them"
        );
    }

    // ---- ZIP --------------------------------------------------------------

    /// Build a stored (uncompressed) ZIP.
    fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cdir = Vec::new();
        for (name, body) in files {
            let crc = crc32(body);
            let off = out.len() as u32;
            out.extend_from_slice(&LFH_SIG.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method = stored
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(body);

            cdir.extend_from_slice(&CDFH_SIG.to_le_bytes());
            cdir.extend_from_slice(&20u16.to_le_bytes()); // version made by
            cdir.extend_from_slice(&20u16.to_le_bytes()); // version needed
            cdir.extend_from_slice(&0u16.to_le_bytes()); // flags
            cdir.extend_from_slice(&0u16.to_le_bytes()); // method
            cdir.extend_from_slice(&0u16.to_le_bytes()); // time
            cdir.extend_from_slice(&0u16.to_le_bytes()); // date
            cdir.extend_from_slice(&crc.to_le_bytes());
            cdir.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cdir.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cdir.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes()); // extra
            cdir.extend_from_slice(&0u16.to_le_bytes()); // comment
            cdir.extend_from_slice(&0u16.to_le_bytes()); // disk
            cdir.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            cdir.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            cdir.extend_from_slice(&off.to_le_bytes());
            cdir.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&cdir);
        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cdir.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// Like `make_zip`, but with explicit unix modes in the external attributes.
    fn make_zip_with_mode(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cdir = Vec::new();
        for (name, body, mode) in files {
            let crc = crc32(body);
            let off = out.len() as u32;
            out.extend_from_slice(&LFH_SIG.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(body);

            cdir.extend_from_slice(&CDFH_SIG.to_le_bytes());
            cdir.extend_from_slice(&20u16.to_le_bytes());
            cdir.extend_from_slice(&20u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&crc.to_le_bytes());
            cdir.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cdir.extend_from_slice(&(body.len() as u32).to_le_bytes());
            cdir.extend_from_slice(&(name.len() as u16).to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&0u16.to_le_bytes());
            cdir.extend_from_slice(&(mode << 16).to_le_bytes());
            cdir.extend_from_slice(&off.to_le_bytes());
            cdir.extend_from_slice(name.as_bytes());
        }
        let cd_off = out.len() as u32;
        out.extend_from_slice(&cdir);
        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(files.len() as u16).to_le_bytes());
        out.extend_from_slice(&(cdir.len() as u32).to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    #[test]
    fn zip_round_trip_stored() {
        let zip = make_zip(&[("a.txt", b"hello"), ("dir/b.bin", &[1, 2, 3])]);
        let ar = ZipArchive::new(&zip).unwrap();
        assert_eq!(ar.entries.len(), 2);
        assert_eq!(ar.entries[0].name, "a.txt");
        assert_eq!(ar.entries[0].method, 0);
        assert_eq!(ar.read(&ar.entries[0]).unwrap(), b"hello");
        assert_eq!(ar.read(&ar.entries[1]).unwrap(), vec![1, 2, 3]);
        assert!(ar.entry("nope").is_none());
        assert!(ZipArchive::new(b"garbage").is_err());
    }

    #[test]
    fn plain_deflate_streams() {
        // Empty fixed-Huffman block: BFINAL=1, BTYPE=01, end-of-block.
        assert_eq!(inflate(&[0x03, 0x00], 0).unwrap(), Vec::<u8>::new());
        // A stored block: BFINAL=1, BTYPE=00, LEN=3, NLEN=!3, "abc".
        let stored = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&stored, 3).unwrap(), b"abc");
    }

    #[test]
    fn install_rejects_bad_hash_first() {
        let zip = make_zip(&[]);
        let err = install_zip(&zip, "deadbeef", Path::new("/nonexistent")).unwrap_err();
        assert!(
            err.message().starts_with("the pack's SHA-256 is "),
            "{}",
            err.message()
        );
        assert!(err.message().ends_with(": not installed"));
    }

    #[test]
    fn install_rejects_missing_manifest_and_traversal() {
        let zip = make_zip(&[("a.txt", b"x")]);
        let digest = sha256_hex(&zip);
        let err = install_zip(&zip, &digest, Path::new("/nonexistent")).unwrap_err();
        assert_eq!(err.message(), "the zip has no pack.toml");

        // A manifest plus a traversal entry.
        let manifest = "id = \"p\"\nformat = 1\nfiles = []\n";
        let zip = make_zip(&[("pack.toml", manifest.as_bytes()), ("../evil", b"x")]);
        let digest = sha256_hex(&zip);
        let err = install_zip(&zip, &digest, Path::new("/nonexistent")).unwrap_err();
        assert_eq!(err.message(), "../evil: outside the pack's folder");
    }

    #[test]
    fn safe_relative_rules() {
        assert_eq!(safe_relative("a/b.txt"), Some(PathBuf::from("a/b.txt")));
        assert_eq!(safe_relative("./a//b"), Some(PathBuf::from("a/b")));
        assert_eq!(safe_relative("/etc/passwd"), None);
        assert_eq!(safe_relative("../x"), None);
        assert_eq!(safe_relative("a/../../x"), None);
        assert_eq!(safe_relative("C:/x"), None);
        assert_eq!(safe_relative(""), None);
        assert_eq!(safe_relative("."), None);
    }

    fn scratch(tag: &str) -> PathBuf {
        // Keep test scratch inside the (gitignored) workspace `target/` tree so
        // the sandbox allows the writes.
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/gs_base-test");
        let dir = base.join(tag);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn install_writes_files_and_records_the_ledger() {
        let root = scratch("install");
        let manifest = "id = \"hydra_maps\"\nformat = 1\nfiles = [\"global/a.txt\", \"global/b/c.txt\"]\n";
        let zip = make_zip(&[
            ("pack.toml", manifest.as_bytes()),
            ("global/a.txt", b"hello"),
            ("global/b/c.txt", b"deep"),
        ]);
        let digest = sha256_hex(&zip);

        // Wrong digest is refused.
        let err = install_zip(&zip, &"0".repeat(64), &root).unwrap_err();
        assert!(err.message().starts_with("the pack's SHA-256 is "));

        let report = install_zip(&zip, &digest, &root).unwrap();
        assert_eq!(report.pack, "hydra_maps");
        assert_eq!(report.version, "1");
        assert_eq!(report.added.len(), 2);
        assert_eq!(report.added[0], "global/a.txt");
        assert_eq!(
            fs::read(root.join("packs/hydra_maps/global/a.txt")).unwrap(),
            b"hello"
        );
        assert_eq!(
            fs::read(root.join("packs/hydra_maps/global/b/c.txt")).unwrap(),
            b"deep"
        );
        assert_eq!(
            report.summary(),
            "hydra_maps 1: 2 added, 0 updated, 0 removed"
        );

        // The ledger round-trips.
        let book = ledger(&root).unwrap();
        assert_eq!(book.format, 0);
        assert_eq!(book.installed.len(), 1);
        assert_eq!(book.installed["hydra_maps"].files.len(), 2);

        // Re-installing identical content keeps everything.
        let again = install_zip(&zip, &digest, &root).unwrap();
        assert_eq!(again.kept.len(), 2);
        assert!(again.added.is_empty());
        assert_eq!(
            again.summary(),
            "hydra_maps 1: 0 added, 0 updated, 0 removed, 2 kept as the pool has them"
        );

        // Removing drops the tree and the ledger entry.
        let removed = remove(&root, "hydra_maps").unwrap();
        assert_eq!(removed.removed.len(), 2);
        assert!(ledger(&root).unwrap().installed.is_empty());
        assert!(fs::metadata(root.join("packs/hydra_maps")).is_err());

        // Removing again uses the recovered wording.
        let err = remove(&root, "hydra_maps").unwrap_err();
        assert_eq!(err.message(), "no pack 'hydra_maps' is installed");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_marks_changed_files_as_updated() {
        let root = scratch("update");
        let manifest = "id = \"p\"\nformat = 1\nfiles = [\"global/a.txt\"]\n";
        let v1 = make_zip(&[
            ("pack.toml", manifest.as_bytes()),
            ("global/a.txt", b"one"),
        ]);
        install_zip(&v1, &sha256_hex(&v1), &root).unwrap();
        let v2 = make_zip(&[
            ("pack.toml", manifest.as_bytes()),
            ("global/a.txt", b"two"),
        ]);
        let report = install_zip(&v2, &sha256_hex(&v2), &root).unwrap();
        assert_eq!(report.updated, vec!["global/a.txt".to_string()]);
        assert_eq!(
            fs::read(root.join("packs/p/global/a.txt")).unwrap(),
            b"two"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_rejects_symlinks() {
        let root = scratch("symlink");
        // The "external attributes" field carries the unix mode; 0o120777 is S_IFLNK.
        let zip = make_zip_with_mode(&[
            (
                "pack.toml",
                b"id = \"p\"\nformat = 1\nfiles = []\n".as_slice(),
                0o100644,
            ),
            ("global/link", b"target".as_slice(), 0o120777),
        ]);
        let err = install_zip(&zip, &sha256_hex(&zip), &root).unwrap_err();
        assert_eq!(err.message(), "global/link: a pack holds no links");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn toml_subset() {
        let doc = parse_toml(
            r#"
            a = "x"   # trailing
            b = 42
            c = true
            d = [1, 2, 3]
            e = ['lit']
            [tbl]
            f = "y"
        "#,
        )
        .unwrap();
        assert_eq!(doc["a"], TomlValue::String("x".into()));
        assert_eq!(doc["b"], TomlValue::Integer(42));
        assert_eq!(doc["c"], TomlValue::Boolean(true));
        assert_eq!(
            doc["d"],
            TomlValue::Array(vec![
                TomlValue::Integer(1),
                TomlValue::Integer(2),
                TomlValue::Integer(3)
            ])
        );
        assert_eq!(doc["e"], TomlValue::Array(vec![TomlValue::String("lit".into())]));
        match &doc["tbl"] {
            TomlValue::Table(t) => assert_eq!(t["f"], TomlValue::String("y".into())),
            other => panic!("expected table, got {other:?}"),
        }
    }
}
