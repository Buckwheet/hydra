# `hydra_import` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra` (no DWARF), symbols
from `symbols_demangled.txt` plus the local symbol table
(`nm -nm … | c++filt`).  Addresses are unslid VAs; a function ends where the next
`__TEXT,__text` symbol begins.  The original crate lives at
`crates/hydra-import/src/lib.rs` — that path survives in `__cstring`
(`0x103e217fe`), right next to the crate's report format strings.

The stub file named four functions, but the symbol table has six: two private
helpers (`attribute_of`, `gather`) and the `Session` record with its `Ord`
comparator survive as inlined code.  All six are reconstructed here.

**Full inventory (audit pass, both sources).** `symbols_demangled.txt` lists the
same 6 global symbols; `nm -a | rustfilt` gives 28 lines naming `hydra_import`,
which collapse to 6 crate functions, 1 record, and the drop glue / closure
types of three functions that are **out of this crate's stub set**:

| symbol | addr | size | status |
|---|---|---|---|
| `instance_of` (T) | `0x10026368c` | 384 | reconstructed |
| `attribute_of` (t) | `0x10026380c` | 1792 | reconstructed |
| `gather` (t) | `0x100263f0c` | 560 | reconstructed |
| `name_of` (T) | `0x10026413c` | 724 | reconstructed |
| `span_of` (T) | `0x100264410` | 760 | reconstructed |
| `sessions` (T) | `0x100264708` | 1344 | reconstructed |
| `drop_in_place<Session>` | `0x100264c80` + 7 ICF copies | 212/192 | gives the record layout (§2) |
| `drop_in_place<Vec<Session>>` | `0x100264e88`, `0x1002663dc` | 196 | confirms `Vec<Session>` |
| `import::{{closure}}` (drop glue) | 3 addresses (`0x10001c62c`, …) | 112 | out of scope |
| `replay::{{closure}}` (drop glue) | 3 addresses (`0x10001c69c`, …) | 332 | out of scope |
| `import_into::{{closure}}` | `0x1000058d8`, `0x100138b80`, `0x100162340` + 3 drop glues | 10456/13588/836 | out of scope |

The two 10–13 KB `import_into::{{closure}}` symbols are the drop glue of an
**`async` state machine** (a closure type of that size can only be captured
`.await` state), which is consistent with `import_into` being an `async fn`;
they are the only hint that the crate had async code at all, and no `tokio`
symbol names them, so no async runtime is claimed for this crate. The published
6-function surface is the whole of this crate's reconstructed API.

Legend:

* **ESTABLISHED** — read directly out of the instructions, the immediates or the
  literal pool.
* **INFERRED** — follows from the code shape, the call sites or the literals, but
  the original spelling is not recoverable.
* **UNKNOWN** — not recoverable from this binary; the safest behaviour is
  implemented and marked with a `// UNKNOWN:` comment in the source.

---

## 0. Phase 1: dependency reconnection — this crate is a **no-op** for it

`hydra_import` has no std stand-in to replace, and no registry dependency to
reconnect.  That is a checked finding, not a copy of §3: the call graph of all
six recovered functions was re-read out of the binary for this phase
(`recon.py disas 'hydra_import::<fn>$'`, every `bl` target listed).  Each target
is

* `core`/`alloc`/`std` — `str::trim_matches`, `str::pattern::StrSearcher::new`,
  `str::converts::from_utf8`, `str::split_once`, `slice::memchr::
  {memchr_aligned,memrchr}`, `slice::sort::{driftsort_main,insert_tail,
  sort4_stable,median3_rec}`, `fs::read_dir`, `DirEntry::path`, `Path::{is_dir,
  extension,file_stem,to_path_buf,components}`, `path::compare_components`,
  `raw_vec::{grow_one,try_allocate_in,handle_error}`, `__rust_alloc`,
  `__rust_alloc_zeroed`, `__rust_dealloc`, `panic*`/`slice_error_fail`,
* one of this crate's own six symbols, or
* `hydra_net::gemstone::endpoint::instance` (`0x101c44af8`) — the original's
  only sibling call.

No `tokio`, `serde`, `regex`, `jiff`, `reqwest`, `tracing` or `anyhow` symbol is
reachable, so no `x = { workspace = true }` line was added: `Cargo.toml` still
names `hydra_net` as the crate's only dependency, and pulling a pinned crate in
here would invent a dependency the original never had.  `crate_versions.txt`
lists no entry for this crate either (it lists registry crates only).

What this phase *did* fix is the one functional gap noted in §4 — the sibling
endpoint table — plus the coverage listed in §6.

---

## 1. Function table

| function | address | size | recovered signature |
| --- | --- | --- | --- |
| `instance_of` | `0x10026368c` | 384 B | `fn(&str) -> Option<String>` (24 B sret, niche `i64::MIN`) |
| `attribute_of` (private) | `0x10026380c` | 1792 B | `fn(&mut Option<String>, &str, &str, &str)` (`x0` sret, then text, tag, attribute as `(ptr,len)` pairs in `x1..x6`) |
| `gather` (private) | `0x100263f0c` | 560 B | `fn(&Path, &mut Vec<PathBuf>)` |
| `name_of` | `0x10026413c` | 724 B | `fn(&Path) -> Option<(String, Option<String>, String)>` (72 B sret) |
| `span_of` | `0x100264410` | 760 B | `fn(&str) -> (Option<i64>, Option<i64>)` (32 B sret) |
| `sessions` | `0x100264708` | 1344 B | `fn(&[PathBuf]) -> Vec<Session>` (24 B sret) |
| `core::ptr::drop_in_place<Session>` | `0x100264c80` | — | gives the record layout (§2) |
| `core::slice::sort…insert_tail<…h426f2e09…>` | `0x10026516c` | — | gives `Session`'s `Ord` (§3) |
| `core::str::<impl str>::split_once<…h29a2a330…>` | `0x100265080` | — | monomorphised for `char`: the delimiter is `'_'` |

The four public entry points are `external`; `attribute_of` and `gather` are
`non-external`, i.e. the original had them private.  The original crate also had
`import`, `import_into` and `replay` (their `{{closure}}` thunks survive:
`0x100058d8`, `0x100138b80`, `0x100162340`), which are **not** part of this
crate's stub set and are not reconstructed here.

### 1.1 `instance_of` — which game/instance a recording belongs to (ESTABLISHED)

```rust
pub fn instance_of(text: &str) -> Option<String> {
    if let Some(game) = attribute_of(text, "<app ", "game=") {
        if !game.is_empty() { return Some(game) }
    }
    let code = attribute_of(text, "<settingsInfo ", "instance=")?;
    let endpoint = hydra_net::gemstone::endpoint::instance(code.trim())?;
    Some(<display name>.to_string())
}
```

Evidence (`recon.py disas instance_of`, raw literal pool at `0x102070940`):

* The literal pool holds `"<app "` (`0x10207094c`, len 5), `"game="`
  (`0x102070951`, len 5), `"<settingsInfo "` (`0x102070956`, len 14) and
  `"instance="` (`0x102070964`, len 9) — exactly the `(ptr,len)` pairs in
  `x3/x4` and `x5/x6` of the two `attribute_of` calls.
* First call result at `sp+8`: word 0 (the *capacity*) is compared against
  `i64::MIN` (`0x1002636dc..0x1002636e4`) — that is `Option<String>::None` for
  this build's `String` layout.  On `Some`, the length word (`sp+0x18`) is
  tested: **non-zero → the string is moved into the sret unchanged**
  (`0x100263764..0x100263770`); zero → `__rust_dealloc` (`0x10026379c`) and fall
  through to the `settingsInfo` path.
* Second call: on `Some` the string is fed to
  `hydra_net::gemstone::endpoint::instance` (`x0/x1` = ptr/len at
  `0x10026371c..0x100263724`), the string is freed, and the `(ptr,len)` pair the
  endpoint function returns is copied into a fresh allocation
  (`__rust_alloc`+`memcpy`, `0x1002637b0..0x1002637e4`) — i.e. `.to_string()`;
  `x0 == 0` means `None` (`0x100263744`).

`hydra_net::gemstone::endpoint::instance` itself (`0x101c44af8`) trims its
argument and compares three bytes case-insensitively against `gs3`, `gsf`, `gst`,
returning the **second** `&str` of a static 0x20-byte record
(`ldp x0, x1, [x8, #0x10]`, `0x101c44c74`).  The three records at
`0x1044a8680`/`0x1044a86a0`/`0x1044a86c0` are `("GS3","Prime")`,
`("GSF","Shattered")`, `("GST","Test")`, so the original returned the *display
name*, not the code.

**Phase 1 re-verification** (same binary, `recon.py bytes 0x1044a8680 96` and
`recon.py str`):

* the three records hold `ptr/len` pairs `0x103bf962d/3`, `0x103bf9630/5`;
  `0x103bf9635/3`, `0x103bf9638/9`; `0x103bf9641/3`, `0x10213f868/4`, which
  read back as `"GS3" "Prime" "GSF" "Shattered" "GST"` (one contiguous run
  `GS3PrimeGSFShatteredGST` at `0x103bf962d`) and `"Test"` at `0x10213f868` —
  the three rows above are exact, stride `0x20`, and nothing else is in the
  table;
* the three compares are the only ones in the function, each guarded by
  `cmp x1, #0x3` and the `sub w, w, #0x41 / cmp #0x1a / csel w8, #0x20, wzr, lo`
  lowercasing idiom, and the fall-through is `mov x0, #0x0` (`0x101c44c7c`),
  i.e. `None`.  There is **no `GS4` row**: the original spelled `GS4`
  recordings through the `<app game="GS4">` path of `instance_of`, not through
  this table.

**Conflict with the sibling crate (unresolved by design).**  The workspace's
`crates/hydra_net/src/gemstone/endpoint.rs` reconstructs `instance` differently:
it returns `Option<&'static Endpoint>` from a *host/port* table and matches the
`game` field, so it answers for `GS4`/`GS3` and returns `None` for `GST`/`GSF`.
That cannot produce the display names and is not what `0x101c44af8` does.
`hydra_net` belongs to another phase and was **not** edited here; instead
`instance_of` applies the three recovered rows itself (`display_name`, private,
ESTABLISHED) and consults the sibling only for a code the table did not name —
see §4 row 1 for the one resulting superset (`GS4`).

### 1.2 `attribute_of` — pull one attribute out of one element (ESTABLISHED)

```rust
fn attribute_of(text: &str, tag: &str, attribute: &str) -> Option<String> {
    if tag.len() > text.len() { return None }
    let at = <first byte-wise match of `tag` in `text`>?;   // bcmp scan
    let rest = &text[at..];
    let end = rest.find('>')?;                              // byte scan for 0x3e
    let inner = &rest[..end];
    let after = inner.find(attribute)? + attribute.len();   // core::str::pattern::StrSearcher
    let quote = inner[after..].chars().next()?;             // one UTF-8 char, 1..4 bytes
    let value = &inner[after + quote.len_utf8()..];
    let close = value.find(quote)?;                         // CharSearcher: memchr(last byte)+memcmp
    Some(value[..close].to_string())
}
```

Evidence: `cmp x4, x2` guard (`0x10026382c`); the `bcmp` loop that advances the
text pointer one byte at a time (`0x100263850..0x100263878`); the `0x3e` scan
(`0x100263898..0x1002638b0`); `core::str::converts::from_utf8` over
`text[at..at+gt]` (`0x1002638cc`); `core::str::pattern::StrSearcher::new` for the
attribute (`0x10026391c`); the UTF-8 decode of the byte right after the attribute
(`0x100263c28..0x100263c8c`) which yields the quote char and its length; the
`memchr`+`bcmp` loop for the same char (`0x100263db0..0x100263e38`); and
`RawVecInner::try_allocate_in`+`memcpy` for the owned result
(`0x100263e3c..0x100263e80`).  The value is whatever sits between the quote char
and its next occurrence — no escaping, no `>` filtering, first element wins.

### 1.3 `gather` — every `.bytes` file below a path (ESTABLISHED)

```rust
fn gather(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_dir() {                                  // 0x101e8dd04
        if let Ok(entries) = std::fs::read_dir(path) {   // 0x101e9ff90
            for entry in entries.flatten() { gather(&entry.path(), files) }  // 0x101e92e34
        }
        return;
    }
    if path.extension() == Some("bytes") { files.push(path.to_path_buf()) }
}
```

The extension test is a length-5 check plus a byte compare against the
immediates `mov w10, #0x7962 / movk w10, #0x6574` plus `w9 == 's'`
(`0x100264008..0x10026401c`), i.e. `b'y' 't' 'e'` + `'s'` — case sensitive.
`read_dir` errors simply contribute nothing (`0x100263f5c`), and erroring
directory entries are skipped (`Flatten<ReadDir>` at `0x100263f7c`).

### 1.4 `name_of` — the character/instance import rule (ESTABLISHED)

The task brief guessed "last `-`, three-character suffix"; the exact rule is
stricter.  On the file stem (`Path::file_stem`, then `str::from_utf8`, then
`core::slice::memchr::memrchr(b'-')`):

1. `suffix = &stem[dash+1..]` must be **exactly 3 bytes, all ASCII digits**.  The
   three checks at `0x1002641f4..0x100264220` are
   `sub w, w, #0x3a; cmn w, #0xa; b.lo` (twice) and
   `sub w, w, #0x3a; cmn w, #0xb; b.ls` (once); both forms accept exactly
   `0x30..=0x39` (carry set only for `v-58+10 >= 2^32`, `Z=0` for `v-58+11`).
2. `dash >= 20` (`subs x20, x2, #0x14`, `0x100264224`) and
   `stem[dash-20] == b'-'` (`0x100264240`).
3. The 19 bytes `stem[dash-19..dash]` — the *stamp* — must have `stamp[10] == b'_'`
   (`ldrb w8, [x27, #0xa]` with `x27 = stem + dash - 19`, `0x100264250`).
   Slice start is char-boundary checked (`is_char_boundary` inlined at
   `0x100264234`).
4. `prefix = &stem[..dash-20]`, split with `str::split_once('_')`
   (`core::str::<impl str>::split_once` at `0x100265080` is the `char = '_'`
   instantiation).  `Some((a, b))` ⇒ name `= b`, prefix `= Some(a)`;
   `None` ⇒ name `= prefix`, prefix `= None` (`0x1002642f8..0x100264334`, where
   the `Option` is tagged by `mov x21, #i64::MIN`).
5. The 19-byte stamp is copied into its own `String`
   (`__rust_alloc(19,1)` + 16-byte `ldr q0`/`stur w8` copy, `0x10026433c`).

So the accepted shape is

```
<prefix>-<19 byte stamp, stamp[10] == '_'>-<3 ASCII digits>
```

with `<prefix>` = `<before the first '_'>_<after it>` when it contains an `_`.
The 19-byte stamp happens to be exactly the shape of
`hydra_net::sink::config::file_stamp` (`{:04}-{:02}-{:02}_{:02}-{:02}-{:02}`,
`0x101c50398`) — 10 characters, `'_'` at index 10, 8 characters — e.g.
`Vaalor_GS4-2024-03-10_10-04-59-000.bytes`.  UNKNOWN: which producer writes that
name.  `hydra_net::sink::writer` builds `<stem>-{:03}.bytes` from
`format!("{}-{}_{}", date, character, instance)` (`0x103e7f486`/`0x103e7f48e`) and
is called from `hydra::connect::run` (`0x100027060`) with a two-argument
`format!` whose pieces contain `'_'`; the field lengths that make that stem pass
this rule are not recoverable from the binary, and any file name that does not
match returns `None` and is skipped by `sessions`.

### 1.5 `span_of` — the recorded span, from the prompt timestamps (ESTABLISHED)

```rust
pub fn span_of(text: &str) -> (Option<i64>, Option<i64>)   // (first, last)
```

* The 14-byte needle is built from two immediates (`0x10026447c..0x100264498`):
  `x27 = 0x2074_706d_6f72_703c` = `"<prompt "` and `x28 = 0x223d_656d_6974_2074`
  = `"t time=\""`, compared at `[p]` and `[p+6]`, which overlays to
  **`<prompt time="`** (14 bytes = the `cmp x1, #0xe` guard at `0x100264430`).
* The match loop starts at `i` and tries offsets `0..=(len-i-14)`
  (`0x1002644ac..0x1002644d0`).
* From `p+14` it consumes a run of ASCII digits (same `sub #0x3a`/`cmn #0xa`
  idiom, `0x100264510..0x100264528`) and hands exactly that slice to an inlined
  `i64::from_str` (`sign`, `smulh` overflow checks: `0x1002645cc..0x1002646c0`,
  so the integer is **signed 64-bit**).
* Accumulation (`0x1002646c4..0x1002646d8`): `if first.is_none() { first = v }`
  (`csel x25, x25, x9, ne` on the `Option` tag in `x21`), then `last = v`.
  The return is two `Option<i64>` (32 bytes: tag/value/tag/value) — the early
  exit at `0x100264440` writes both tags as 0.
* The scan resumes from the end of the digit run, so the values come from
  successive prompts, and an empty run (e.g. `<prompt time="x">`) is skipped.

### 1.6 `sessions` — the whole import (ESTABLISHED)

```rust
pub fn sessions(paths: &[PathBuf]) -> Vec<Session> {
    let mut files = Vec::new();
    for path in paths { gather(path, &mut files) }        // 0x100264748..0x10026475c
    if files.len() >= 2 { files.sort() }                  // 0x100264760, stable sort
    let mut out = Vec::new();
    for file in files {                                   // into_iter over the gathered Vec
        let Some((name, prefix, stamp)) = name_of(&file) else { continue };
        match out.iter_mut().find(|s| s.name == name && s.stamp == stamp) {
            Some(s) => s.files.push(file),                 // 0x1002648d0..0x100264900
            None => out.push(Session { name, stamp, files: vec![file], prefix }),
        }
    }
    if out.len() >= 2 { out.sort() }                      // 0x100264a20
    out
}
```

Details that are visible in the disassembly:

* The `&[PathBuf]` element (stride `0x18`) is read as `ptr`/`len` at `+0x08`/`+0x10`
  and passed straight to `gather` (`0x100264748`), so the slice elements are used
  as paths (`&[PathBuf]` — INFERRED; `&[String]` would compile identically).
* The locally gathered list is consumed by value (`Vec::into_iter`, elements
  freed on the `None` arm at `0x100264838`, moved into a session on the `Some`
  arm at `0x1002648f4`), which is why the merge can stuff the path into
  `files` without cloning.
* The merge key is `(name, stamp)`: `0x100264890` compares the session's `+0x10`
  length and `+0x08` pointer with the fresh `name`, then `0x1002648b0` compares
  `+0x28`/`+0x20` with the fresh stamp.  `name_of`'s `prefix` is *not* part of
  the key: the first file in sorted order decides which prefix survives.
* Sorting is `slice::sort` (stable; `driftsort_main` for ≥ 21 elements,
  `insert_tail` otherwise) — for the file list `h2697e8c1641dcd2f`
  (`PathBuf`), for the sessions `h0962be01fb541a11` (`Session`).

### 1.7 `Session`'s `Ord` (ESTABLISHED)

The comparator inlined into `insert_tail<…h426f2e09599b406e>`
(`0x10026516c`, called with a `0x60` stride from `0x100264ac4`) compares

1. the `String` at `+0x18` (`ldp x19, x23, [x1, #0x20]` vs `[x1-0x40]`, i.e.
   `+0x20`/`+0x28`: ptr/len) — the *stamp*,
2. and only when that is `Equal`, the `String` at `+0x00` (`ldp x21, x24,
   [x22, #0x8]`) — the *name*,

as `str` comparisons (`bcmp` of the shorter length, then the length difference).
`files` and `prefix` are ignored, so this is a **hand-written** `Ord`, not
`#[derive]` (derived order would compare `+0x00` first).  `PartialEq`/`Eq` are
derived structurally in this crate — UNKNOWN whether the original agreed with
`Ord` there.

---

## 2. Recovered record layout

`core::ptr::drop_in_place<hydra_import::Session>` (`0x100264c80`) drops, in this
order:

| offset | drop | ⇒ field |
| --- | --- | --- |
| `+0x48` | `cmp [x0+0x48], i64::MIN` / `ccmp …, #0, #4, ne` ⇒ only when `!= i64::MIN` does it `dealloc([x0+0x50], size = [x0+0x48])` | **`Option<String>`** (None spells `cap = i64::MIN`) |
| `+0x00` | `ldr x1, [x0]; cbz` ⇒ `dealloc([x0+0x8], size = x1)` | plain `String` |
| `+0x18` | same shape at `+0x18`/`+0x20` | plain `String` |
| `+0x30` | iterate `[x19+0x38]` elements of 24 bytes, freeing each, then `dealloc([x19+0x38], size = 24*[x19+0x30])` | `Vec<PathBuf>` |

So `Session` = `{ String, String, Vec<PathBuf>, Option<String> }` = `0x60` bytes,
matching the `0x60` stride used by `Vec<Session>` in `sessions`
(`0x1002647b0`, `0x100264ae8`) and in the caller (`0x100005b70`).  In this build
`String`/`Vec` store **`cap` at `+0`, `ptr` at `+8`, `len` at `+16`** and use the
capacity word as the `Option` niche (`i64::MIN` = `None`) — consistent
everywhere in this crate (`__rust_dealloc(ptr, cap, 1)` in the drop glue).

Field meaning (INFERRED from how the values are produced and used):

| field | from | why |
| --- | --- | --- |
| `+0x00` `name` | `name_of`'s first string (after the first `_`) | merged on it, and printed first |
| `+0x18` `stamp` | `name_of`'s 19-byte tail | primary sort key; printed second |
| `+0x30` `files` | the merged `.bytes` paths | drop glue walks `PathBuf`s |
| `+0x48` `prefix` | `name_of`'s `Option<String>` (before the first `_`) | the only `Option` in the record |

A caller formats the label as `format!("{} {}", &session[0x00], &session[0x18])`
— the two `Display` args of the `"{} {}"` literal at `0x103e21840` are
`x19+0x80` and `x19+0x98`, i.e. `&session` and `&session[0x18]` (`0x100006034`,
`0x100006038`), and that label is what the report line
`{}: no game named; skipped.` (`0x103e218fd`) prints.  The other report literals
recovered from the same block are `{}: {} hunts, {} pulses, {} parts.`
(`0x103e2181d`), `{}: hunt #{} on file spans the same time, recorded as it was
played; skipped.` (`0x103e21845`), `{}: {} pulses on file in the same span, …`
(`0x103e21893`), `{}: imported before; skipped.` (`0x103e218df`) and
`{} sessions read, {} refused: {} hunts and {} pulses added; {} login rewards
found.` (`0x103e21919`) — those belong to `import_into`, outside this stub set.

---

## 3. Dependencies

`hydra_net = { path = "../hydra_net" }` is the only dependency.  `instance_of`
calls `hydra_net::gemstone::endpoint::instance`, exactly as the original did
(`bl 0x101c44af8` at `0x100263724`).  Everything else is `std`
(`std::path`, `std::fs`, `Vec`, `String`); the original used no async, no `serde`
and no regex in these six functions.

Phase 1 verified this from the call graph instead of trusting the note: every
`bl` target of the six functions is `core`/`alloc`/`std`, one of the crate's own
symbols, or that one sibling call — no registry crate is reachable (§0).  So the
dependency set is unchanged and **no** `x = { workspace = true }` line was added.

**Audit pass re-verification (third time, different method).** The whole `.text`
range `0x10026368c..0x10026516c+` was disassembled and every `bl` target
resolved against the full `nm -a` table and bucketed by crate prefix:
`core` (37, plus 2 `<core…` generic impls), `rustc` shims (35, i.e.
`__rust_alloc`/`__rust_dealloc`/`__rust_no_alloc_shim_is_unstable_v2`),
`<local/unnamed>` intra-crate tail calls (24), `alloc` (14, plus 5 `<alloc…`),
`std` (3, plus 9 `<std…`: `Path::{is_dir,extension,file_stem,to_path_buf,
components}`, `std::path::compare_components`, `std::fs::read_dir`,
`<std::fs::DirEntry>::path`), `hydra_import` itself (5), and exactly one
sibling call — `hydra_net::gemstone::endpoint::instance` at `0x100263724`.
Nothing else. Also searched the entire `nm -a` dump for any
symbol naming a registry crate *and* a `hydra_import` type (the tell-tale for a
trait impl or a monomorphisation) — there is none. Contrast `hydra_exp`, which
has 15 `tokio`-naming symbols: this crate has zero. **Verdict: no third-party
dependency, confirmed.**

---

## 4. Departures

| original | here |
| --- | --- |
| `endpoint::instance(&str) -> Option<&'static str>` returning the *display name* for exactly `gs3`/`gsf`/`gst` and `None` for anything else | **only the fallback is a departure now.**  `instance_of` applies the three recovered rows itself (`display_name`, ESTABLISHED, §1.1) and returns `"Prime"`/`"Shattered"`/`"Test"` exactly as the original did — this was the Phase 1 fix, `GST` no longer answers `None`.  For a code the table did not name it still calls the sibling `hydra_net::gemstone::endpoint::instance`, whose `game` is returned verbatim: `instance="GS4"` → `Some("GS4")` where the original answered `None` (the original spelled GS4 recordings through the `<app game="GS4">` path).  That superset is kept because the sibling table is the only recovered source for GS4, and it disappears if that table is ever corrected to the binary's shape; `hydra_net` was not edited here. |
| `name_of` slices `&stem[dash-19..dash]` unconditionally and panics via `core::str::slice_error_fail` on a non-char-boundary | `str::get` returns `None` instead — same result for every name whose bytes are ASCII at those offsets, no panic |
| `Attribute`/`Session` type and field names, `name_of`'s return type | UNKNOWN; `Session { name, stamp, files, prefix }` and the tuple `(name, prefix, stamp)` are descriptive |

---

## 5. UNKNOWNs

* The original names of `Session`'s four fields and of `name_of`'s return type.
  The record is known field-for-field by offset and drop behaviour, but no
  symbol or literal carries the source names.
* Which producer writes `<prefix>-<19 byte stamp>-<NNN>.bytes` and what the three
  pieces mean.  The predicate is exact (see §1.4); the semantics of `prefix`,
  `name` and `stamp` are inferred from use (sort key, merge key, report label).
* Whether `Session`'s `PartialEq`/`Eq` were structural (this crate derives them)
  or agreed with the hand-written `Ord`.
* The sign handling inside `span_of`'s inlined `i64::from_str` is unreachable
  from the digit-run scan, so whether the original meant `i64` or an unsigned
  integer cannot be observed; `i64` is what the inlined parser checks for
  (64-bit `smulh` overflow).
* `import`, `import_into`, `replay`, `Session`'s other callers: out of scope
  (only the four stubs plus their two helpers were requested).  Their literals
  are listed in §2 for context.

### 5.1 Cross-crate conflict, left open in Phase 1

Both this crate and `hydra_net` reconstruct the same symbol,
`hydra_net::gemstone::endpoint::instance` (`0x101c44af8`), and disagree:
`hydra_net/src/gemstone/endpoint.rs` matches a host/port table and returns
`Endpoint { host, port, game }` for `GS4`/`GS3`, while the disassembly in §1.1
shows three `&str` rows (`GS3`/`GSF`/`GST` → `Prime`/`Shattered`/`Test`) and
`None` otherwise.  This phase was told to leave `hydra_net` untouched, and
`hydra_import`'s behaviour no longer depends on the sibling for those three
codes.  Whoever next owns `hydra_net` should re-read `0x101c44af8` and, if they
agree with §1.1, restore the display-name signature; `hydra_import`'s
`display_name` helper (and the `GS4` superset in §4) can then be deleted.
`hydra_net`'s host/port knowledge is real, it just belongs to
`endpoint::other_spelling` (`0x101c447b4`), not to `instance`.

---

## 6. Tests

`cargo test --offline -p hydra_import` runs 26 unit tests, all asserting
recovered values:

* `attribute_of`: first `<app …>` wins and stops at the closing quote; missing
  tag / attribute / `>` / closing quote / empty text all give `None` (6 cases in
  2 tests); any character after the attribute name is the quote — `|`, `*` and
  a two-byte `§` all delimit a value (added in Phase 1, ESTABLISHED from the
  UTF-8 decode at `0x100263c28..0x100263c8c`).
* `instance_of`: `<app game="GS4" …>` → `"GS4"`; `game=""` falls through to
  `<settingsInfo instance="GS3">` → `"Prime"`; `" gs3 "` trims and resolves;
  unknown code, unknown tag and plain text → `None`.
* `instance_of` table (Phase 1): all three recovered rows resolve, case-folded
  and padded (`GS3`/`gs3`/`" gs3\t"` → `"Prime"`, `GSF`/`gsf` → `"Shattered"`,
  `GST`/`" GST "`/`gst` → `"Test"`); `display_name` is checked to know only the
  three *whole* three-byte codes (`""`, `GS`, `GS4`, `GSTX`, `GS3 `, ` GSF`,
  `gs4`, `Test` → `None`, so trimming stays `instance_of`'s job); and the one
  superset is locked in — `instance="GS4"` → `Some("GS4")` (§4 row 1).
* `name_of`: full split (`Vaalor_GS4-2024-03-10_10-04-59-000.bytes` →
  `("GS4", Some("Vaalor"), "2024-03-10_10-04-59")`); no `_` in the prefix;
  2-digit / 4-digit / alphabetic counters rejected; missing or misplaced `_` in
  the stamp rejected; missing second dash rejected; too-short stems and empty
  paths rejected; `.bytes` extension and directory prefix ignored.
* `span_of`: first/last `<prompt time="…">`; text shorter than 14 bytes and
  prompt-less text → `(None, None)`; a non-numeric time is skipped (42 then a
  larger stamp); a prompt without `time="` does not match.
* `gather`: a scratch tree with a `.bytes` file, a rotated `.bytes` file in a
  subdirectory, a `.log` and a `.txt` — only the two `.bytes` paths are kept.
* `sessions`: rotations `-000`/`-001` of one recording merge into one session
  whose `files` are in sorted order; different stamps stay separate and come
  back oldest-first; an unparsable name is skipped; merging ignores the prefix;
  a single file path and an empty slice and a missing path all behave; `Ord` is
  `(stamp, name)` and ignores `files`/`prefix`.

The scratch fixtures live in `std::env::temp_dir()` and are removed by a `Drop`
guard; no fixture is written into the repository.

---

## 7. What changed in audit-core (Phase 2)

The six functions and the record layout needed **no behavioural change**: they
were re-derived from the disassembly once more (see the coverage table in the
header and §3) and every recovered value in §1.1–§1.7 still holds. What changed
is the bookkeeping and one pinned consequence:

1. **Full symbol inventory added** (header table): 28 `nm -a` lines → 6
   crate functions, 1 record + 8 drop-glue instantiations, and the closure/drop
   types of the out-of-scope `import`/`import_into`/`replay`.
2. **New observation**: `import_into`'s closure type is 10.5 KB / 13.6 KB, i.e.
   an `async` state machine — recorded in the header table. It does not affect
   this crate's dependency verdict (no runtime symbol names it).
3. **Dependency verdict re-verified a third time** with a different method (see
   §3): still `hydra_net` only, still no `x = { workspace = true }`.
4. **One new test** (`order_and_equality_disagree_on_the_ignored_fields`) pins
   the `Ord`/`Eq` contract divergence §1.7 flagged as UNKNOWN: `Ord` compares only
   `(stamp, name)` while `Eq` is derived over all four fields, so a
   `BTreeSet<Session>` silently collapses records that differ only in
   `files`/`prefix`. That is what the binary's comparator does, and `sessions`
   merges on `(name, stamp)` so it cannot hit the case — but the hazard is now
   asserted rather than described.
5. **Tests 25 → 26.** No existing test changed.
