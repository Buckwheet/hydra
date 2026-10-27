# `hydra_lookout` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`, no DWARF.  Every
address below is an **unslid VA** in `__TEXT,__text`; function bounds came from
`nm -an` (the stubs' `/// @0x…` VAs are the entry points, and the *last* stub in
a module is bounded by the next private symbol, so a few of the generated stub
sizes in `symbols_demangled.txt` are too large — real sizes are noted where that
matters).

Legend: **ESTABLISHED** = visible in the disassembly, in a literal, or in a
field offset.  **INFERRED** = follows from the code shape or from a recovered
neighbour, but no symbol/literal names it.  **UNKNOWN** = not recoverable; the
safest behaviour was chosen and is marked `// UNKNOWN:` in the source.

## 1. How the literals were read

`__TEXT,__cstring` format strings are stored as length-prefixed pieces with a
single `0xC0` byte per `{}` argument, e.g. `0x103e7cd02`:

```
11 "[hydra] lookout: " c0 1f "; its choices are the defaults\n" 00
```

Every literal quoted below was decoded that way.  The three state-file errors
are plain NUL-terminated strings that ld64 packed together at `0x1039c7947`
(`cut short` = 9, `more after the set than the set` = 31, `not a lookout file of
this version` = 34 bytes) — the decoder allocates exactly those lengths, which
is how the boundaries are known.

Private helpers were found with `nm -an | rustfilt`: `write_newest`,
`opened`, `Lookout::{find,count,snapshot}`, `file::encode`,
`settings::Settings::{read,write,is_off,ignore_list,set_reads}`, and the statics
`OPEN` (0x10458a880), `WRITTEN` (0x10458a8a8) and `file::write::WRITES`
(0x10458ee50).

## 2. Function table

| function | address | status |
| --- | --- | --- |
| `lock` | `0x101ab32f4` | **full** |
| `now_seconds` | `0x101ab2fdc` | **full** |
| `open` | `0x101ab3368` | **full** (lifecycle) |
| `opened` (helper) | `0x101ab3ad4` | **full** |
| `counts_all` | `0x101ab2ea8` | **full** |
| `reload` | `0x101ab3c40` | **full** |
| `save_all` | `0x101ab6e7c` | **full** |
| `Lookout::new` | `0x101ab4064` | **full** (layout) |
| `Lookout::set` | `0x101ab4754` | **full** |
| `Lookout::reads` | `0x101ab6424` | **full** |
| `Lookout::own` | `0x101ab4434` | **full** |
| `Lookout::find` (helper) | `0x101ab4acc` | **full** (shape) |
| `Lookout::count` (helper) | `0x101ab4d34` | **full** |
| `Lookout::clear` | `0x101ab4c34` | **full** |
| `Lookout::always` | `0x101ab65f8` | **partly** (the `Shown` field it pins is INFERRED) |
| `Lookout::forget` | `0x101ab69a4` | **partly** (same) |
| `Lookout::judge` | `0x101ab51d8` | **partly** (see §4) |
| `Lookout::snapshot` (helper) | `0x101ab6c78` | **full** |
| `Lookout::write` | `0x101ab6450` | **full** |
| `Lookout::save_now_and_then` | `0x101ab3e6c` | **full** |
| `write_newest` (helper) | `0x101ab3044` | **full** |
| `file::path` | `0x101abce3c` | **full** |
| `file::read` | `0x101abcf58` | **full** (shape) |
| `file::write` | `0x101abd154` | **full** |
| `file::decode` | `0x101abd4f8` | **full** (format) |
| `file::encode` (helper) | `0x101abd8d4` | **full** (format) |
| `key::object` | `0x101ace5b8` | **full** (one filter predicate partly) |
| `key::sentence` | `0x101ace768` | **partly** (see §5) |
| `Settings::ignore_list` | `0x101ac89ac` | **full** |
| `Settings::is_off` | `0x101ac8e68` | **full** |
| `Settings::read` | `0x101ac89d0` | **full** (shape) |
| `Settings::write` | `0x101ac8c94` | **full** (shape) |
| `Settings::set_reads` | `0x101ac9054` | **full** (448 bytes, not 13 KB) |

## 3. The state

### 3.1 `Lookout` (560 bytes, `0x230`)

Recovered from `Lookout::new` (`0x101ab4220`–`0x101ab4330`), `Lookout::set`
(`0x101ab4838`…), `Lookout::snapshot` and `Lookout::count`:

| offset | field | evidence |
| --- | --- | --- |
| `+0x000` | `dir: PathBuf` | `new` copies `Path::to_path_buf` here; `snapshot` calls `file::path` with `[+0x8]`/`[+0x10]`/`[+0x20]`/`[+0x28]` |
| `+0x018` | `character: String` | `new` copies the name; `counts_all` clones `[inner+0x38]` |
| `+0x030` | `settings: Settings` (128) | `reads` does `add x0, x0, #0x30`, `set` drops `+0x30` |
| `+0x0b0` | `own: Vec<String>` | `own` pushes 24-byte elements at `+0xb8`/`+0xc0` |
| `+0x0c8` | `shown: VecDeque<Shown>` (32) | `find` walks 96-byte elements, `clear` truncates, `forget` retains |
| `+0x0e8` | `instant: Instant` | `save_now_and_then` stores `Instant::now` at `+0xe8`/`+0xf0` |
| `+0x0f8` | `always: HashSet<u64>` (48) | `set` drops `[+0xf8]`, deallocates 9-byte buckets, stores the new one |
| `+0x128` | `known: HashSet<u64>` (48) | `count` `contains_key`, `forget`/`always` insert, `clear` resets |
| `+0x158` | `counting: HashMap<u64, Counting>` (48) | `count` inserts `(day, occasions)`, `snapshot` prunes by day, `encode` writes it |
| `+0x188` | `recent: HashMap<u64, Recent>` (48) | `count` inserts `(f64, u32, u8)`, prunes above `0x1000` |
| `+0x1b8` | `ignore` (`Option<RegexSet>` → `Vec<Pattern>`) | `set` drops/stores a 32-byte value |
| `+0x1d8` | `stats` (`Option<Regex>`) | `new` compiles `STAT_PATTERN` into it; `judge` uses it |
| `+0x1f8` | `owns` (`Option<Regex>`) | `own` drops/stores a 32-byte value |
| `+0x218` | `written: u64` | `write`/`save_now_and_then` store the generation; `counts_all` copies it |
| `+0x220` | `snaps: u64` | `snapshot` increments it and returns it |
| `+0x228` | `dirty: bool` | `count`, `clear`, `forget` set it; `write` clears it |
| `+0x229` | `shut: bool` | `open` sets it when the state file could not be read; `write` refuses |

A `HashMap` in this binary is 48 bytes and Rust put the table *before* the
hasher: control bytes at `+0` (e.g. `0x128`), bucket mask at `+8` (`0x130`),
growth left at `+0x10`, items at `+0x18` (`0x140`), `RandomState` at `+0x20`
(`0x148`).  `clear` (`0x101ab4c44`) writes exactly that: `memset(ctrl, 0xff,
bucket_mask + 9)`, `items = 0`.  `counts_all`'s closure reads the `items` field
of the maps at `0x128`/`0x158`, which is how `Counts` was pinned.

The two statics:

```text
OPEN    0x10458a880  LazyLock<Mutex<Vec<(PathBuf, Arc<Mutex<Lookout>>)>>>
WRITTEN 0x10458a8a8  LazyLock<Mutex<Vec<(PathBuf, u64)>>>
```

`open` walks `OPEN` with `Path::components().eq_by(..)` and `Arc::clone`s the
matching entry (`0x101ab3490`); otherwise it pushes a 32-byte `(PathBuf, Arc)`.
`WRITTEN` is `write_newest`'s "generation last written per path" table.

### 3.2 The state file (`HYDRALK1`)

Not JSON: `file::decode` (`0x101abd4f8`) and `encode` (`0x101abd8d4`) hand-roll
a little-endian layout and both check it exactly:

```text
0        8        12                                   12+8n
+--------+--------+-------------------------------------+
|HYDRALK1| u32 n  | n x u64 keys            (the set)   |
+--------+--------+-------------------------------------+------+--------------+
| u32 m                                                       | m x 13 bytes |
+-------------------------------------------------------------+--------------+
each entry: u64 key, u8 occasions, u32 day   (unaligned, 13 bytes)
```

* magic `HYDRALK1` (`0x101abd528`: `movz 0x5948`… = "HY" "DR" "AL" "K1").
* `decode` rejects `< 8` bytes (`cut short`), a wrong magic (`not a lookout file
  of this version`), a truncated section (`cut short`) and trailing bytes
  (`more after the set than the set`).
* the counts saturate at `u32::MAX` (`0x101abd9ec`).
* the iteration order is hash order — the original does not sort, and neither do
  we, so the bytes are only stable within one process.
* `file::read` returns `len` alongside the two maps (`0x101abd00c` stores the
  byte length at payload `+0x68`), which `open` puts into `Lookout::written`.

`file::write` is a write-then-rename: create the parent directory (`0o777`,
recursive), write `<path>.<pid>-<n>.tmp` (`".{}-{}.tmp"` at `0x103e7ce80`, with
`process::id()` and the `WRITES` counter), rename, and remove the temporary file
if the rename fails.  Both failure messages are `"<path>: <io error>"`.

### 3.3 The settings file (`lookout.toml`)

Twelve fields, from the `Deserialize` field visitor (`0x101ac9214`) and the
unknown-field `&str` table at `0x1044a02c0`.  Declaration order = serialisation
order (`0x101ac867c`):

| # | name | offset | type | default |
| --- | --- | --- | --- | --- |
| 1 | `occasions` | `+0x7b` | `u8` | 5 |
| 2 | `window` | `+0x48` | `f64` | 2.0 |
| 3 | `stat_floor` | `+0x50` | `u64` | 1500 |
| 4 | `forget_days` | `+0x70` | `u32` | 30 |
| 5 | `quiet_secs` | `+0x74` | `u32` | 60 |
| 6 | `mark` | `+0x78` | `u8` | 0 |
| 7 | `paused` | `+0x79` | `bool` | false |
| 8 | `off` | `+0x00` | `Vec<String>` | `[]` (skipped when empty) |
| 9 | `ignore` | `+0x30` | `Option<Vec<String>>` | `None` (skipped) |
| 10 | `always` | `+0x18` | `Vec<String>` | `[]` (skipped when empty) |
| 11 | `trigger_added` | `+0x7a` | `bool` | false |
| 12 | `seeded` | `+0x58` | `Vec<?>` | `[]` (skipped when empty) |

The defaults are the 0x80-byte block `Settings::read`, `Lookout::new` and
`reload` all inline (`0x101ac8afc`, `0x101ab40e8`, `0x101ab3c74`): `window`
2.0, `stat_floor` 1500, `forget_days` 30 (`0x10213cb50` first word),
`quiet_secs` 60 (second word), and one `str` store of `0x05000000` for the four
packed bytes `mark=0, paused=0, trigger_added=0, occasions=5`.

`Settings::ignore_list` (`0x101ac89ac`) clones `ignore` when it is `Some` and
otherwise collects the 28 built-in patterns in the `&str` table at
`0x1044a0380` (28 × 16 bytes = 0x1c0, exactly the `add x1, x0, #0x1c0`).  All 28
are reproduced verbatim in `IGNORED_BY_DEFAULT`.

`Settings::is_off` (`0x101ac8e68`) finds the **first** `:` (`memchr`), keeps
everything after it as the tag (untrimmed, `0x101ac8f38`), and matches every
`off` entry against both the tag and the whole line, trimming the entry
(`char::is_whitespace`) and comparing ASCII-case-insensitively (`0x101ac8fa0`
lowercases both sides byte by byte).  `Settings::set_reads` (`0x101ac9054`)
splits the same way, removes the tag from `off` and pushes it back when
`reads == false`.

## 4. What `Lookout::judge` does (partly recovered)

Established from `0x101ab51d8`:

* early out when `shut` (`+0x229`) or `settings.paused` (`+0xa9`);
* early out when `settings.is_off(<the said text>)` (`0x101ab522c`);
* the line's text is `trim_end_matches(..)` then `trim()`ed (`0x101ab5234`,
  `0x101ab5244`) and an empty line is ignored;
* if the line holds a `:` **and** the `stats` regex is present (`+0x1d8`), the
  regex is searched over the line with `regex_automata`'s captures
  (`0x101ab52f8` `Regex::create_captures`), the number in the capture is parsed
  as a signed integer (`0x101ab5754`, saturating) and compared with
  `settings.stat_floor`; below the floor the line is not a judgement
  (`0x101ab5894` `cmp x20, x8; b.ge`);
* surviving lines go through `Lookout::count` (`0x101ab5dfc`, `0x101ab5e84`).

**UNKNOWN**: which keys one line produces.  The key extraction is inlined
`regex_automata` capture handling across ~4 KB and the pattern it uses is not in
`__cstring`.  We count the sentence key plus one object key per phrase, which is
the shape `key::object`/`key::sentence` and `Lookout::forget`
(`objects[0]` = sentence tag, `objects[1..]` = object tag) support.

`Lookout::count` itself is fully recovered, and it is the interesting part:

```text
if known.contains(key)                                  → false
if let Some(recent) = recent[key]:
    if recent.tag == tag                                → count (no window)
    else if 0 <= now - recent.at < settings.window      → false   (quiet)
recent[key] = { at: now, tag, occasions }
if recent.len() > 0x1000: drop entries with now - at >= window
counting[key] = { day, occasions: min(occasions + 1, 255) }
if occasions >= settings.occasions:
    counting.remove(key); known.insert(key); recent.remove(key)
dirty = true                                            → true
```

`Lookout::snapshot` first prunes `counting` entries whose day is older than
`settings.forget_days` (day = `(now_seconds() / 86400.0) as u32`, `0x101ab6cf8`
holds `86400.0`), then `snaps += 1` and returns `(path, encode(known, counting),
snaps)`.

`Lookout::save_now_and_then` is throttled: it writes only when
`instant.elapsed() >= 300 s` (`cmp x0, #0x12c`), and the write happens on a
thread named `hydra-lookout` (`0x1039c77e6`).

## 5. `key::sentence` (partly recovered)

`0x101ace768` takes `(text: &str, matches: &[Match; N])` where `Match` is a
40-byte record whose `+0x18`/`+0x20` are the span's start/end
(`0x101ace844`: `ldr x27, [x19, #0x18]`, `ldr x24, [x19, #0x20]`).  It builds a
`String` with `text.len()` capacity and copies `text[start..end]` for every
match, separating them with `'‹'` (bytes `e2 80 b9` at `0x101ace8e0`), checking
character boundaries first.  Since the `Match` type (a `regex_automata` captures
handle) is not recoverable, this crate exposes the same join as
`key::spans(text, &[(from, to)])` plus the word-level normalisation
(`key::sentence`) that the keys end up comparing.

## 6. Std-only departures

| original | here | why |
| --- | --- | --- |
| `regex::RegexSet` for the ignore set | `Vec<hydra_model::pattern::Pattern>`, compiled one pattern at a time | no `regex`; `hydra_model::pattern` is the workspace's std-only matcher and the original already linked `hydra_model` (`0x101ab4184` calls `hydra_model::pattern::compile`) |
| `regex::Regex` for `STAT_PATTERN` | `hydra_model::pattern::Pattern`, compiled **without** the leading `\b` | `hydra_model::pattern` has no word-boundary assertion (verified: `\bAS` never matches), so the recovered `\b` prefix is stripped — `without_boundary` |
| `serde` + `toml` for `lookout.toml` | a 60-line `key = value` reader/writer for exactly these twelve keys | no external crates; the writer emits `toml::to_string_pretty`'s shape (declaration order, same skips) |
| `serde` for the state file | not needed: the file is a hand-rolled binary format | `file::decode`/`encode` are recovered in full |
| `tokio`-free already | `std::thread` for the autosave | the original also spawns a plain `std` thread |
| `Regex`/`Option<Regex>` fields | `Pattern` / `Vec<Pattern>` | see above |
| `hashbrown` maps with a pass-through hasher for the FNV keys | `HashSet<u64>`/`HashMap<u64, _>` with the default hasher | the key is already an FNV hash; the extra hash is harmless and keeps the type std |

Unresolved details, marked `// UNKNOWN:` in the source: the `Shown` field names
(three `String`s + `Vec<String>`, established; roles inferred), `Object`'s two
trailing words (40-byte elements with one `String`), `Settings::seeded`'s
element type, the meaning of `judge`'s first struct field (`Line::mark`), and
whether the settings file requires every key (we keep defaults for missing
keys).

## 7. Tests

40 tests, all asserting values read out of the binary:

* `key`: the article list, the punctuation-only-word filter, the exact
  lower-casing and space collapsing, the `'‹'` span separator.
* `file`: the `HYDRALK1` layout byte for byte (header sizes, 13-byte entries),
  all four error strings, the missing-file case, the atomic write leaving no
  `.tmp` behind, the lower-cased `lookout_<name>.bin` path.
* `settings`: the twelve defaults (and that `Settings` is 128 bytes), the 28
  default patterns, `is_off`'s first-colon/ASCII-case-insensitive matching,
  `set_reads` add/remove, a TOML round trip, unknown-key rejection.
* `lib`: `key_hash` against a hand-computed FNV-1a, the five-occasion threshold,
  the quiet window (same tag counts, a changed tag inside `window` does not),
  `own`'s de-duplication, judge/forget marking keys known, `clear` keeping the
  settings, and `open` returning the same `Arc` for the same directory.
