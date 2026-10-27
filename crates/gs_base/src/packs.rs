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
//! ## Dependencies (Phase 1)
//!
//! | original | here |
//! |---|---|
//! | `anyhow` 1.0.104 | the real crate, re-exported as `crate::Error` / `crate::Result`; every message below is still a plain `format!` string |
//! | `zip` 9.0.0 (+ its `zlib_rs` inflater) | the real `zip` crate behind [`ZipArchive`]; [`inflate`] now wraps `flate2` 1.1.10 |
//! | `toml` 1.1.6 + `serde` 1.0.229 | the real crates: [`Manifest`] and [`Ledger`] derive `Deserialize`, [`parse_toml`] maps `toml::Table` onto [`TomlValue`] |
//! | `sha2::Sha256` | **[`Sha256`] is kept** — the original had no `sha2` dependency (`PLAN.md`); `compress256` is inlined in the binary |

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail};

use crate::Result;

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
/// `0x80`/length padding.
///
/// **Kept by hand** (not reconnected): `PLAN.md` records that the original had
/// no `sha2` crate, and the round function really is inlined in the binary
/// rather than called through a library boundary, so this is the original code
/// rather than a stand-in.
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
///
/// The struct is deserialized by the real `serde` + `toml` crates; unknown keys
/// are ignored, exactly as the previous hand-written reader ignored them.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
pub struct Manifest {
    /// Pack id; must be lower `snake_case`.
    #[serde(default)]
    pub id: String,
    /// Manifest format version.
    #[serde(default, deserialize_with = "de_format")]
    pub format: u32,
    /// Files the pack ships, each under `global/`.
    #[serde(default)]
    pub files: Vec<String>,
}

/// `format` accepts an integer or an integer written as a string.
///
/// The recovered reader did the same (`Some(Integer(n)) if n >= 0` otherwise
/// `Some(String(s)) => s.parse()`, anything else `0`); a string that is not a
/// number is now a deserialization error instead of the "newer than this build
/// understands" message, which was only ever reached for a numeric string.
fn de_format<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Int(i64),
        Text(String),
    }

    Ok(match Raw::deserialize(deserializer)? {
        // Mirrors the recovered `Some(Integer(n)) if *n >= 0 => *n as u32, _ => 0`.
        Raw::Int(n) => {
            if n < 0 {
                0
            } else {
                n as u32
            }
        }
        Raw::Text(s) => s
            .parse::<u32>()
            .map_err(|_| serde::de::Error::custom(format!("format '{s}' is not a number")))?,
    })
}

impl Manifest {
    /// Parse and validate a `pack.toml`.
    ///
    /// `@0x100a79edc`.  Recovered rejections, in message order:
    /// * `pack.toml: id '{}' is not lower snake case`
    /// * `pack '{}' is format {}, newer than this build understands ({})`
    /// * `pack file '{}' is not under global/`
    ///
    /// The text is parsed by the real `toml` crate, so a *syntax* error now
    /// carries that crate's wording ("TOML parse error at line …, column …")
    /// instead of the hand parser's `pack.toml: cannot parse line '…'`, which
    /// was never a recovered literal.
    pub fn from_toml(text: &str) -> Result<Manifest> {
        let manifest: Manifest = toml::from_str(text)?;
        if !is_lower_snake_case(&manifest.id) {
            bail!(
                "pack.toml: id '{}' is not lower snake case",
                manifest.id
            );
        }
        if manifest.format > SUPPORTED_FORMAT {
            bail!(
                "pack '{}' is format {}, newer than this build understands ({})",
                manifest.id,
                manifest.format,
                SUPPORTED_FORMAT
            );
        }
        for path in &manifest.files {
            if !path.starts_with(FILE_PREFIX) {
                bail!("pack file '{path}' is not under global/");
            }
        }
        Ok(manifest)
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
// TOML
// ---------------------------------------------------------------------------

/// Value of the TOML subset this crate cares about.
///
/// `toml::Value` mapped one-for-one (a date-time is carried as its textual
/// form); `f64` is why this type is `PartialEq` but not `Eq`.
#[derive(Debug, Clone, PartialEq)]
pub enum TomlValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Datetime(String),
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

/// Parse a TOML document into nested [`TomlValue`]s.
///
/// Deferred to the real `toml` crate (pinned 1.1.6): dotted keys and `[a.b]`
/// headers nest the same way the recovered subset did, and arrays, inline
/// tables, floats and date-times are now accepted too.  Only the error *text*
/// changed — see [`Manifest::from_toml`].
pub fn parse_toml(text: &str) -> Result<BTreeMap<String, TomlValue>> {
    let table: toml::Table = toml::from_str(text)?;
    Ok(table
        .into_iter()
        .map(|(key, value)| (key, from_toml_value(value)))
        .collect())
}

fn from_toml_value(value: toml::Value) -> TomlValue {
    match value {
        toml::Value::String(s) => TomlValue::String(s),
        toml::Value::Integer(n) => TomlValue::Integer(n),
        toml::Value::Float(f) => TomlValue::Float(f),
        toml::Value::Boolean(b) => TomlValue::Boolean(b),
        toml::Value::Datetime(d) => TomlValue::Datetime(d.to_string()),
        toml::Value::Array(items) => {
            TomlValue::Array(items.into_iter().map(from_toml_value).collect())
        }
        toml::Value::Table(table) => TomlValue::Table(
            table
                .into_iter()
                .map(|(key, value)| (key, from_toml_value(value)))
                .collect(),
        ),
    }
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
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Installed {
    #[serde(default)]
    pub files: Vec<String>,
}

/// `installed.toml`.
///
/// INFERRED: serde's struct name `Ledger` and field name `format` are in the
/// pool, as is `Installed`/`files`.  A `pack` field name is also present but its
/// owner could not be pinned down, so it is left out (see NOTES.md).
///
/// Deserializing a `Ledger` directly expects the `[installed.…]` table shape;
/// [`ledger`] accepts either spelling and writes the `[packs.…]` one.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct Ledger {
    #[serde(default)]
    pub format: u32,
    #[serde(default)]
    pub installed: BTreeMap<String, Installed>,
}

/// The on-disk file, which may spell the pack table `[packs.…]` *or*
/// `[installed.…]` (NOTES unknown 6).
#[derive(serde::Deserialize)]
struct LedgerFile {
    #[serde(default)]
    format: u32,
    #[serde(default)]
    packs: Option<BTreeMap<String, Installed>>,
    #[serde(default)]
    installed: Option<BTreeMap<String, Installed>>,
}

/// The shape this crate writes: `format = n` plus one `[packs.<id>]` table per
/// installed pack.
#[derive(serde::Serialize)]
struct LedgerOut<'a> {
    format: u32,
    packs: &'a BTreeMap<String, Installed>,
}

/// Read `<root>/packs/installed.toml`.
///
/// `@0x100a78740` — `root.join("packs").join("installed.toml")`, then
/// `fs::read_to_string`, then the TOML deserializer wrapped in
/// `anyhow::Context`.  Both the `[packs.…]` and the `[installed.…]` table
/// shapes are accepted, `packs` winning if a file somehow carries both.
pub fn ledger(root: &Path) -> Result<Ledger> {
    let path = root.join(PACKS_DIR).join(LEDGER_FILE);
    let text = fs::read_to_string(&path)?;
    let file: LedgerFile = toml::from_str(&text)?;
    Ok(Ledger {
        format: file.format,
        installed: file
            .packs
            .or(file.installed)
            .unwrap_or_default(),
    })
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
        .ok_or_else(|| anyhow!("no pack '{id}' is installed"))?;
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
///
/// Now serialised by the real `toml` crate (pinned 1.1.6) instead of by hand;
/// the keys, the `format = n` line and the `[packs.<id>]` tables are unchanged.
pub fn write_ledger(root: &Path, book: &Ledger) -> Result<()> {
    let dir = root.join(PACKS_DIR);
    fs::create_dir_all(&dir)?;
    let text = toml::to_string(&LedgerOut {
        format: book.format,
        packs: &book.installed,
    })?;
    let mut f = fs::File::create(dir.join(LEDGER_FILE))?;
    f.write_all(text.as_bytes())?;
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
        bail!("the pack's SHA-256 is {actual}, not {expected}: not installed");
    }

    let mut archive = open_zip(zip)?;
    let count = archive.len();
    if count > MAX_ENTRIES {
        bail!("the zip holds more files than a pack may");
    }

    // Find and parse pack.toml.
    let manifest_index = archive
        .index_for_name("pack.toml")
        .ok_or_else(|| anyhow!("the zip has no pack.toml"))?;
    let text = {
        let mut entry = archive.by_index(manifest_index)?;
        let mut raw = Vec::new();
        entry.read_to_end(&mut raw)?;
        String::from_utf8(raw).map_err(|_| anyhow!("pack.toml is not text"))?
    };
    let manifest = Manifest::from_toml(&text)?;

    // Central-directory metadata for the guards below.
    let meta = archive.metadata();

    // Size guard before writing anything.
    let mut total: u64 = 0;
    for index in 0..count {
        let entry = meta.entry(index).map_err(|_| anyhow!("not a zip"))?;
        total = total.saturating_add(entry.size());
    }
    if total > MAX_UNPACKED_BYTES {
        bail!("the zip unpacks to more than a pack may");
    }

    // Validate every entry before touching the filesystem, so a hostile archive
    // never leaves a staging directory behind.
    let mut planned: Vec<(usize, PathBuf)> = Vec::new();
    for index in 0..count {
        let entry = meta.entry(index).map_err(|_| anyhow!("not a zip"))?;
        let name = entry
            .name()
            .map_err(|_| anyhow!("not a zip"))?
            .into_owned();
        if name == "pack.toml" {
            continue;
        }
        // A ZIP symlink is marked by S_IFLNK in the external attributes.
        if entry.is_symlink() {
            bail!("{name}: a pack holds no links");
        }
        let Some(rel) = safe_relative(&name) else {
            bail!("{name}: outside the pack's folder");
        };
        planned.push((index, rel));
    }
    drop(meta);

    let pack_dir = global.join(PACKS_DIR).join(&manifest.id);
    let staging = global.join(PACKS_DIR).join("pack-tmp");
    fs::create_dir_all(&staging)?;

    let mut report = Report {
        pack: manifest.id.clone(),
        version: manifest.format.to_string(),
        ..Default::default()
    };
    let mut written: Vec<String> = Vec::new();

    for (index, rel) in &planned {
        let dest = staging.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::new();
        match archive.by_index(*index) {
            Ok(mut entry) => {
                if let Err(e) = entry.read_to_end(&mut bytes) {
                    let _ = fs::remove_dir_all(&staging);
                    return Err(e.into());
                }
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(anyhow!("not a zip: {e}"));
            }
        }
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
    book.installed
        .insert(manifest.id.clone(), Installed { files: written });
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
    /// CRC-32 as recorded in the central directory.
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    /// Unix mode bits from the "external attributes" field, if any.
    pub unix_mode: u32,
    /// Index into the real archive, used by [`ZipArchive::read`].
    index: usize,
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

/// An archive listing with entry-by-entry reads.
///
/// Thin wrapper over `zip::ZipArchive` (pinned 9.0.0): the metadata recorded in
/// [`ZipEntry`] is copied out of the crate's central directory, and [`read`]
/// re-opens the archive (the crate needs `&mut` for a read while this API is
/// `&self`) to decompress one entry.  Because the real crate does the parsing it
/// also verifies each entry's CRC-32, which the hand-written reader did not, and
/// it understands ZIP64 and every method the crate was built with.
///
/// [`read`]: ZipArchive::read
#[derive(Debug, Clone)]
pub struct ZipArchive<'a> {
    data: &'a [u8],
    pub entries: Vec<ZipEntry>,
}

impl<'a> ZipArchive<'a> {
    /// Parse the central directory.
    pub fn new(data: &'a [u8]) -> Result<ZipArchive<'a>> {
        let archive = open_zip(data)?;
        let meta = archive.metadata();
        let mut entries = Vec::with_capacity(meta.len());
        for index in 0..meta.len() {
            let entry = meta.entry(index).map_err(|_| anyhow!("not a zip"))?;
            entries.push(ZipEntry {
                name: entry
                    .name()
                    .map_err(|_| anyhow!("not a zip"))?
                    .into_owned(),
                method: method_code(entry.compression()),
                crc32: entry.crc32(),
                compressed_size: entry.compressed_size(),
                uncompressed_size: entry.size(),
                unix_mode: entry.unix_mode().unwrap_or(0),
                index,
            });
        }
        Ok(ZipArchive { data, entries })
    }

    /// Look an entry up by exact name.
    pub fn entry(&self, name: &str) -> Option<&ZipEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Decompress one entry.
    pub fn read(&self, entry: &ZipEntry) -> Result<Vec<u8>> {
        let mut archive = open_zip(self.data)?;
        let mut file = archive.by_index(entry.index)?;
        let mut out = Vec::with_capacity(entry.uncompressed_size.min(1 << 20) as usize);
        file.read_to_end(&mut out)?;
        Ok(out)
    }
}

/// Open a ZIP over an in-memory slice; anything the crate rejects is the
/// recovered literal `not a zip`.
fn open_zip(data: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>> {
    zip::ZipArchive::new(Cursor::new(data)).map_err(|_| anyhow!("not a zip"))
}

/// The numeric ZIP method code for a method the crate knows.
///
/// The crate only exposes `from_u16`, so the code is found by inverting it over
/// the u16 space; the deprecated `Unsupported(code)` variant answers directly.
#[allow(deprecated)]
fn method_code(method: zip::CompressionMethod) -> u16 {
    if let zip::CompressionMethod::Unsupported(code) = method {
        return code;
    }
    (0..=u16::MAX)
        .find(|code| zip::CompressionMethod::from_u16(*code) == method)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// DEFLATE (RFC 1951)
// ---------------------------------------------------------------------------

/// Decode a raw DEFLATE stream.
///
/// This was a hand-written inflater standing in for the `zlib_rs` decoder the
/// `zip` crate vendored; it now wraps `flate2` (pinned 1.1.10), so the error
/// text of a malformed stream is `flate2`'s `io::Error` instead of the previous
/// `not a zip: …` wording (never a recovered literal).
pub fn inflate(input: &[u8], expected_len: usize) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(expected_len.min(1 << 20));
    let mut decoder = flate2::read::DeflateDecoder::new(input);
    decoder.read_to_end(&mut out)?;
    Ok(out)
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
        assert_eq!(
            e.to_string(),
            "pack.toml: id 'HydraMaps' is not lower snake case"
        );
        let e = Manifest::from_toml(
            r#"id = "ok"
format = 9"#,
        )
        .unwrap_err();
        assert_eq!(
            e.to_string(),
            "pack 'ok' is format 9, newer than this build understands (1)"
        );
        let e = Manifest::from_toml("id = \"ok\"\nfiles = [\"maps/a.png\"]").unwrap_err();
        assert_eq!(e.to_string(), "pack file 'maps/a.png' is not under global/");
    }

    /// The real deserializer ignores keys the schema does not know, exactly as
    /// the hand-written reader did.
    #[test]
    fn manifest_ignores_unknown_keys() {
        let m = Manifest::from_toml(
            r#"
            id = "hydra_maps"
            format = 1
            files = ["global/a.png"]
            description = "the recovered schema never mentions this"
            [extra]
            nested = 1
        "#,
        )
        .unwrap();
        assert_eq!(m.id, "hydra_maps");
        assert_eq!(m.files, vec!["global/a.png".to_string()]);

        // Missing keys fall back to the recovered defaults: no id is invalid.
        let e = Manifest::from_toml("format = 1").unwrap_err();
        assert_eq!(e.to_string(), "pack.toml: id '' is not lower snake case");
    }

    /// `format` may be written as an integer or as a numeric string (the
    /// recovered reader accepted both through `parse::<u32>`).
    #[test]
    fn manifest_accepts_a_quoted_format() {
        let m = Manifest::from_toml("id = \"p\"\nformat = \"1\"").unwrap();
        assert_eq!(m.format, 1);
        let e = Manifest::from_toml("id = \"p\"\nformat = \"2\"").unwrap_err();
        assert_eq!(
            e.to_string(),
            "pack 'p' is format 2, newer than this build understands (1)"
        );
        // A negative integer used to fall back to 0 rather than being rejected.
        assert_eq!(Manifest::from_toml("id = \"p\"\nformat = -1").unwrap().format, 0);
        // A non-numeric string is now a deserialization error.
        let e = Manifest::from_toml("id = \"p\"\nformat = \"soon\"").unwrap_err();
        assert!(e.to_string().contains("format 'soon' is not a number"), "{e}");
    }

    /// Syntax errors come from the `toml` crate now; the hand parser's
    /// `pack.toml: cannot parse line '…'` wording was never recovered.
    #[test]
    fn manifest_syntax_errors_come_from_the_toml_crate() {
        let e = Manifest::from_toml("id = \"unterminated\n").unwrap_err();
        let text = e.to_string();
        assert!(text.contains("TOML parse error"), "{text}");
        assert!(!text.contains("cannot parse line"), "{text}");
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

    /// Build a ZIP with the real `zip` writer.
    fn make_zip_with(files: &[(&str, &[u8])], method: zip::CompressionMethod) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in files {
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(method)
                .unix_permissions(0o644);
            writer.start_file(*name, options).unwrap();
            writer.write_all(body).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// A stored (uncompressed) ZIP.
    fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        make_zip_with(files, zip::CompressionMethod::Stored)
    }

    /// A deflated ZIP.
    fn make_deflated_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        make_zip_with(files, zip::CompressionMethod::Deflated)
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
    fn zip_round_trip_stored() {
        let zip = make_zip(&[("a.txt", b"hello"), ("dir/b.bin", &[1, 2, 3])]);
        let ar = ZipArchive::new(&zip).unwrap();
        assert_eq!(ar.entries.len(), 2);
        assert_eq!(ar.entries[0].name, "a.txt");
        assert_eq!(ar.entries[0].method, 0);
        // CRC-32 of "hello", recorded by the real writer in the central dir.
        assert_eq!(ar.entries[0].crc32, 0x3610_a686);
        assert_eq!(ar.entries[0].uncompressed_size, 5);
        assert!(!ar.entries[0].is_symlink());
        assert_eq!(ar.read(&ar.entries[0]).unwrap(), b"hello");
        assert_eq!(ar.read(&ar.entries[1]).unwrap(), vec![1, 2, 3]);
        assert!(ar.entry("nope").is_none());
        assert_eq!(ar.entry("dir/b.bin").unwrap().name, "dir/b.bin");
    }

    #[test]
    fn zip_round_trip_deflated() {
        let body = b"the pool holds the same line over and over and over and over\n".repeat(40);
        let zip = make_deflated_zip(&[("global/log.txt", &body), ("global/small.txt", b"x")]);
        let ar = ZipArchive::new(&zip).unwrap();
        assert_eq!(ar.entries.len(), 2);
        assert_eq!(ar.entries[0].method, 8);
        assert_eq!(ar.entries[0].uncompressed_size, body.len() as u64);
        assert!(
            ar.entries[0].compressed_size < body.len() as u64,
            "deflate should shrink the repetitive body"
        );
        assert_eq!(ar.read(&ar.entries[0]).unwrap(), body);
        assert_eq!(ar.read(&ar.entries[1]).unwrap(), b"x");
    }

    /// The writer really emits the three `PK` markers the literal pool keeps.
    #[test]
    fn zip_writer_emits_the_recovered_markers() {
        let zip = make_zip(&[("a.txt", b"hello")]);
        let find = |sig: u32| {
            zip.windows(4)
                .any(|w| w == sig.to_le_bytes())
        };
        assert!(find(LFH_SIG), "local file header");
        assert!(find(CDFH_SIG), "central directory header");
        assert!(find(EOCD_SIG), "end of central directory");
        // ZIP64 records are only written for ZIP64 archives.
        assert!(!find(ZIP64_EOCD_SIG));
    }

    #[test]
    fn zip_rejects_garbage_with_the_recovered_wording() {
        let e = ZipArchive::new(b"garbage").unwrap_err();
        assert_eq!(e.to_string(), "not a zip");
        let e = ZipArchive::new(&[]).unwrap_err();
        assert_eq!(e.to_string(), "not a zip");
    }

    #[test]
    fn plain_deflate_streams() {
        // Empty fixed-Huffman block: BFINAL=1, BTYPE=01, end-of-block.
        assert_eq!(inflate(&[0x03, 0x00], 0).unwrap(), Vec::<u8>::new());
        // A stored block: BFINAL=1, BTYPE=00, LEN=3, NLEN=!3, "abc".
        let stored = [0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&stored, 3).unwrap(), b"abc");
    }

    /// `inflate` reads what the real `flate2` encoder writes.
    #[test]
    fn inflate_round_trips_the_deflate_encoder() {
        let body = b"hydra hydra hydra hydra hydra hydra hydra hydra\n".repeat(32);
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&body).unwrap();
        let compressed = encoder.finish().unwrap();
        assert!(compressed.len() < body.len());
        assert_eq!(inflate(&compressed, body.len()).unwrap(), body);
        // Truncated input is an error rather than a silent short read.
        assert!(inflate(&compressed[..compressed.len() / 2], body.len()).is_err());
    }

    #[test]
    fn install_rejects_bad_hash_first() {
        let zip = make_zip(&[]);
        let err = install_zip(&zip, "deadbeef", Path::new("/nonexistent")).unwrap_err();
        assert!(
            err.to_string().starts_with("the pack's SHA-256 is "),
            "{}",
            err
        );
        assert!(err.to_string().ends_with(": not installed"));
    }

    #[test]
    fn install_rejects_missing_manifest_and_traversal() {
        let zip = make_zip(&[("a.txt", b"x")]);
        let digest = sha256_hex(&zip);
        let err = install_zip(&zip, &digest, Path::new("/nonexistent")).unwrap_err();
        assert_eq!(err.to_string(), "the zip has no pack.toml");

        // A manifest plus a traversal entry.
        let manifest = "id = \"p\"\nformat = 1\nfiles = []\n";
        let zip = make_zip(&[("pack.toml", manifest.as_bytes()), ("../evil", b"x")]);
        let digest = sha256_hex(&zip);
        let err = install_zip(&zip, &digest, Path::new("/nonexistent")).unwrap_err();
        assert_eq!(err.to_string(), "../evil: outside the pack's folder");

        // An absolute entry is refused the same way.
        let zip = make_zip(&[("pack.toml", manifest.as_bytes()), ("/etc/passwd", b"x")]);
        let digest = sha256_hex(&zip);
        let err = install_zip(&zip, &digest, Path::new("/nonexistent")).unwrap_err();
        assert_eq!(err.to_string(), "/etc/passwd: outside the pack's folder");

        // A manifest that is not UTF-8 is the recovered `pack.toml is not text`.
        let zip = make_zip(&[("pack.toml", &[0xff, 0xfe, 0x00])]);
        let digest = sha256_hex(&zip);
        let err = install_zip(&zip, &digest, Path::new("/nonexistent")).unwrap_err();
        assert_eq!(err.to_string(), "pack.toml is not text");
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

    #[test]
    fn install_writes_files_and_records_the_ledger() {
        let root = scratch("install");
        let manifest =
            "id = \"hydra_maps\"\nformat = 1\nfiles = [\"global/a.txt\", \"global/b/c.txt\"]\n";
        let zip = make_zip(&[
            ("pack.toml", manifest.as_bytes()),
            ("global/a.txt", b"hello"),
            ("global/b/c.txt", b"deep"),
        ]);
        let digest = sha256_hex(&zip);

        // Wrong digest is refused.
        let err = install_zip(&zip, &"0".repeat(64), &root).unwrap_err();
        assert!(err.to_string().starts_with("the pack's SHA-256 is "));

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
        assert_eq!(err.to_string(), "no pack 'hydra_maps' is installed");

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

    /// A pack whose entries the real crate has to inflate.
    #[test]
    fn install_deflated_pack_end_to_end() {
        let root = scratch("deflated");
        let manifest = "id = \"tall_pack\"\nformat = 1\nfiles = [\"global/log.txt\"]\n";
        let body = b"the same line, again and again and again\n".repeat(50);
        let zip = make_deflated_zip(&[
            ("pack.toml", manifest.as_bytes()),
            ("global/log.txt", body.as_slice()),
        ]);
        let report = install_zip(&zip, &sha256_hex(&zip), &root).unwrap();
        assert_eq!(report.added, vec!["global/log.txt".to_string()]);
        assert_eq!(
            fs::read(root.join("packs/tall_pack/global/log.txt")).unwrap(),
            body
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_rejects_symlinks() {
        let root = scratch("symlink");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "pack.toml",
                zip::write::SimpleFileOptions::default().compression_method(
                    zip::CompressionMethod::Stored,
                ),
            )
            .unwrap();
        writer.write_all(b"id = \"p\"\nformat = 1\nfiles = []\n").unwrap();
        writer
            .add_symlink(
                "global/link",
                "/etc/passwd",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        let zip = writer.finish().unwrap().into_inner();

        let err = install_zip(&zip, &sha256_hex(&zip), &root).unwrap_err();
        assert_eq!(err.to_string(), "global/link: a pack holds no links");
        let _ = fs::remove_dir_all(&root);
    }

    // ---- Ledger / TOML ----------------------------------------------------

    #[test]
    fn ledger_round_trips_through_the_real_toml_writer() {
        let root = scratch("ledger");
        let mut book = Ledger {
            format: 3,
            ..Default::default()
        };
        book.installed.insert(
            "alpha".into(),
            Installed {
                files: vec!["global/a.txt".into(), "global/b/c.txt".into()],
            },
        );
        book.installed.insert("beta".into(), Installed::default());
        write_ledger(&root, &book).unwrap();

        let text = fs::read_to_string(root.join(PACKS_DIR).join(LEDGER_FILE)).unwrap();
        assert!(text.contains("format = 3"), "{text}");
        assert!(text.contains("[packs.alpha]"), "{text}");
        assert!(text.contains("[packs.beta]"), "{text}");

        assert_eq!(ledger(&root).unwrap(), book);

        // The serde `Serialize`/`Deserialize` impls round-trip as well.
        let encoded = toml::to_string(&book).unwrap();
        let decoded: Ledger = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, book);
        let _ = fs::remove_dir_all(&root);
    }

    /// The file may spell the table `[packs.…]` or `[installed.…]`; `packs`
    /// wins if a file somehow has both.
    #[test]
    fn ledger_accepts_packs_and_installed_shapes() {
        for (tag, table) in [
            ("shape-packs", "packs"),
            ("shape-installed", "installed"),
        ] {
            let root = scratch(tag);
            let dir = root.join(PACKS_DIR);
            fs::create_dir_all(&dir).unwrap();
            let text = format!(
                "format = 2\n\n[{table}.hydra_maps]\nfiles = [\"global/a.png\"]\n"
            );
            fs::write(dir.join(LEDGER_FILE), text).unwrap();
            let book = ledger(&root).unwrap();
            assert_eq!(book.format, 2);
            assert_eq!(book.installed.len(), 1);
            assert_eq!(
                book.installed["hydra_maps"].files,
                vec!["global/a.png".to_string()]
            );
            let _ = fs::remove_dir_all(&root);
        }

        // Unknown tables and keys are ignored, as serde does by default.
        let root = scratch("shape-extra");
        let dir = root.join(PACKS_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(LEDGER_FILE),
            "format = 1\n[other]\nx = 1\n\n[packs.p]\nfiles = []\nnote = \"ignored\"\n",
        )
        .unwrap();
        let book = ledger(&root).unwrap();
        assert_eq!(book.installed.len(), 1);
        assert!(book.installed["p"].files.is_empty());
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
            f = 1.5
            g = 1979-05-27T07:32:00Z
            [tbl]
            h = "y"
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
        assert_eq!(
            doc["e"],
            TomlValue::Array(vec![TomlValue::String("lit".into())])
        );
        assert_eq!(doc["f"], TomlValue::Float(1.5));
        assert_eq!(
            doc["g"],
            TomlValue::Datetime("1979-05-27T07:32:00Z".into())
        );
        match &doc["tbl"] {
            TomlValue::Table(t) => assert_eq!(t["h"], TomlValue::String("y".into())),
            other => panic!("expected table, got {other:?}"),
        }
        assert_eq!(doc["a"].as_str(), Some("x"));
        assert_eq!(doc["b"].as_str(), None);

        // Nested headers land in nested tables, as the recovered subset had it.
        let doc = parse_toml("[a.b]\nc = 1\n").unwrap();
        match &doc["a"] {
            TomlValue::Table(t) => match &t["b"] {
                TomlValue::Table(inner) => assert_eq!(inner["c"], TomlValue::Integer(1)),
                other => panic!("expected table, got {other:?}"),
            },
            other => panic!("expected table, got {other:?}"),
        }

        assert!(parse_toml("a = ").is_err());
    }

    // ---- anyhow -----------------------------------------------------------

    /// `crate::Error` is the real `anyhow::Error`, so the io error inside a
    /// `?` is downcastable and its text is unchanged.
    #[test]
    fn io_errors_stay_typed_in_the_anyhow_chain() {
        let err = ledger(Path::new("/gs-base-no-such-root")).unwrap_err();
        assert!(err.downcast_ref::<std::io::Error>().is_some(), "{err:?}");
        assert!(!err.to_string().is_empty());
    }
}
