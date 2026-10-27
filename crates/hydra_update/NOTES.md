# `hydra_update` — reverse-engineering notes

Reconstructed from the arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled globals in `symbols_demangled.txt`, private symbols from
`nm -an` + `rustfilt`).  The crate had **7 `todo!()` stubs over 4 files**
(`lib.rs`, `release.rs`, `signed.rs`, `swap.rs`); this build has none —

* `grep -Rn 'todo!\|unimplemented!' crates/hydra_update/src` is empty,
* `cargo check -p hydra_update` — clean, no warnings,
* `cargo test -p hydra_update` — **61 tests pass**,
* `cargo check --workspace` — clean (no other crate was touched).

The binary contains the whole crate plus its consumers, so most of the calling
convention could be read from both sides (the stub and `hydra::update`).

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow, literals, offsets and data tables read out of the instruction stream; asserted by a test. |
| **partly** | the entry contract, literals and helper calls are recovered, but one step (the curve arithmetic, a runtime-only constant) could not be; the body implements the evidenced part and says so at the site. |
| **UNKNOWN** | the binary does not pin this down. The safest behaviour was chosen and written where it is used (`// UNKNOWN:` in the source). |

---

## 0. Function boundaries

`nm -an` is what separates the stubs; the shared `recon.py` sizes are wider than
the real bodies because they stop at the next *global* symbol:

| symbol | address | real size |
|---|---|---|
| `signed::Sums::check` | 0x1010a442c | 656 B (`.. 0x1010a46bc`) |
| `signed::Sums::checked` | 0x1010a46bc | **1164 B**, not 5884 — the closure that parses the list (0x1010a4b48) and four `std::io`/`minisign` helpers follow it |
| `signed::Sums`' parse closure | 0x1010a4b48 | 912 B |
| `swap::put_in_place` | 0x1010a5db8 | 1336 B (0x1010a62f0 is `beside`) |
| `swap::beside` | 0x1010a62f0 | 428 B (**private**, only visible in `nm`) |
| `swap::unpack` | 0x1010a649c | 1084 B |
| `swap::clear_old` | 0x1010a68d8 | **148 B**, not 30364 |
| `release::newest` | 0x1010adf74 | 3480 B |
| `release::newest::{{closure}}` | 0x1010ae95c | 944 B (private) |
| `Releases::new` | 0x1010af1bc | **208 B**, not 8348 |

Tooling (all scratch, nothing written into the repository): the shared
`.work/recon.py` (`syms`, `sizes`, `disas`, `bytes`, `str`, `findstr`) plus two
throwaway scripts — one driving `llvm-objdump --start-address/--stop-address`
(the Mach-O-capable one, since `objdump` ignores the range on this file) through
`recon.annotate`, one scanning `__text` for `bl` callers and `adrp`+`add`
literal references.  The format strings below were decoded with the sibling
crates' `<len><bytes>` / `0xC0` = `{}` scheme.

---

## 1. What the crate is

```
hydra_update
├── Releases          the updater handle: one word, the reqwest client
│   ├── new()         ClientBuilder::default().user_agent("hydra/0.1.0-beta.7").timeout(30s).build()
│   ├── look()        GET the release list → release::newest  (wrapped in timeout(60s))
│   └── fetch()       GET checksums + signature → Sums::checked → GET archive
│                       → Sums::check → swap::unpack → swap::put_in_place
│                        (wrapped in timeout(15min))
├── release::newest   JSON → Vec<Listed> → best Offer newer than the running version
├── signed::Sums      checksum list + its minisign signature
└── swap              unpack the archive member, swap the file, clean up
```

`Releases` is 8 bytes: `hydra::update::keep`'s state stores it at +0x108 and
passes `&self` to `look` (0x10009ed48).  `fetch` is generic —
`Releases::fetch<hydra::update::fetch::{{closure}}::{{closure}}>` in the symbol
table — because it hands the downloaded bytes to a closure.

---

## 2. `release` — the release list

### 2.1 The schema (full)

`Listed`/`ListedFile` field names, order and count come from the derive's string
table at `__const` 0x1023fd534 and the `expecting` globals at 0x1010aed0c /
0x1010aed20:

```
"Listed" "tag_name" "draft" "html_url" "assets" "struct Listed" "struct Listed with 4 elements"
"ListedFile" "name" "browser_download_url" "size" "struct ListedFile" "struct ListedFile with 3 elements"
```

Layout from `drop_in_place<Listed>` @0x1010b05a4 and `drop_in_place<ListedFile>`
@0x1010b0660 (Strings are `{cap@0, ptr@8, len@16}` here, `Vec` likewise):

| type | size | layout |
|---|---|---|
| `Listed` | 0x50 | `tag_name` @0x00, `html_url` @0x18, `assets: Vec<ListedFile>` @0x30, `draft: bool` @0x48 (rustc moves the `bool` last) |
| `ListedFile` | 0x38 | `name` @0x00, `browser_download_url` @0x18, `size: u64` @0x30 |
| `Offer` | 0xb8 | two Strings @0x00/@0x18, three `(String, u64)` @0x30+0x48, 0x50+0x68, 0x70+0x88, then the `semver::Version` @0x90 |

The `assets` element stride is 0x38 (`madd x20, x1, #0x88, …` walks the
IndexMap of the archive, but `lsl x9, x8, #6; sub x27, x9, x8, lsl #3` walks the
JSON vec at 0x56 per record).

### 2.2 `newest(json: &str, running: &Version, asset: &str) -> Result<Option<Offer>, String>` (full)

* `serde_json::from_str::<Vec<Listed>>` @0x1010adfc0; on error
  `format!("The releases could not be read: {}", e)` (0x103e5f02e) and the
  result word is `isize::MIN + 1` → the `Err` niche of
  `Result<Option<Offer>, String>`.
* The per-release closure @0x1010ae95c, in order:
  1. `tag_name`'s first byte must be `'v'` (`cmp w10, #0x76`, 0x1010ae994);
  2. `semver::Version::parse(&tag_name[1..])`, failure → skip;
  3. `draft` (`ldrb w8, [sp, #0x238]`, 0x1010ae0b0) → skip;
  4. three exact-name asset lookups: the caller's `asset`, `"SHA256SUMS.txt"`
     (14 bytes, built as `SHA256SU..` + `SUMS.txt` in two registers at
     0x1010aea8c) and `"SHA256SUMS.txt.minisig"` (22 bytes, 0x1010aeb48);
     any missing → skip;
  5. the result is an `Offer` of the release's `tag_name`, `html_url`, the three
     `(browser_download_url, size)` pairs and the parsed version.
* The outer loop keeps the greatest version (`semver::Version::partial_cmp`, the
  four inlined comparisons at 0x1010ae16c–0x1010ae1cc) and only keeps a
  candidate whose version compares **greater** than the running one (0x1010ae1d0
  keeps `Ordering::Greater` only).  Nothing found → `Ok(None)`.

### 2.3 `Version` (full)

`semver::Version` is an external crate, so its behaviour is reproduced: `parse`
(three numbers, optional `-pre`, optional `+build`, no leading zeros in numbers
or numeric pre-release identifiers), `Display`, and the ordering `major, minor,
patch, Prerelease, BuildMetadata` that the binary compares field by field.
`Prerelease`'s rules (empty sorts highest, numeric identifiers compare
numerically, numeric < alphanumeric, shorter prefix first) are implemented, and
`build` is compared last as text, as `semver` 1.x does.

---

## 3. `signed` — the checksums and their signature

### 3.1 `Sums` (full)

`Sums { entries: Vec<(String, String)> }` — 48-byte `(name, digest)` pairs.
`Sums::check` @0x1010a442c walks `self+0x8`/`self+0x10` by 0x30, compares
`entry.name` by length and bytes, and only then hashes:

* SHA-256 over `(ptr, len)` with `sha2::sha256::compress256`
  (0x1010b6b60) and the standard IV from `__const` 0x1023fd008;
* the digest formatted as 64 lowercase hex characters
  (`String::from_iter::<char>` at 0x1010a4644 over the 32-byte digest, after the
  `rev32.16b` that turns the state words into `{:x}` order);
* equal → `Ok(())`, else `"{} is not the file the release signed: its hash differs"`;
* no entry → `"The release's checksums do not list {}"`.

### 3.2 `Sums::checked(sums: &[u8], signature: &str, version: &str, key: &str)` (full, except the math)

The four arguments are fixed by `hydra::update`'s call site 0x10025e424:

```
x0,x1 = the downloaded SHA256SUMS.txt bytes   (ldp x0,x1,[x19,#0xe0])
x2,x3 = the downloaded .minisig text          (text() into x19+0x90…)
x4,x5 = the version string, ptr/len           (ldp x4,x5,[x8,#0x8], x8 = [x19+0xb8])
x6,x7 = the release key, 56 characters        (adrp/add __const 0x1020701b8, w7 = 0x38)
```

Order of operations (`Sums::checked` @0x1010a46bc):

| step | address | message |
|---|---|---|
| `PublicKey::from_base64` | 0x1010a4708 | `"The release key cannot be read: {}"` (0x103e5ecc7) |
| `Signature::decode` | 0x1010a47c8 | `"The checksums' signature cannot be read: {}"` (0x103e5ec70) |
| `PublicKey::verify(data, sig, allow_legacy = false)` | 0x1010a48d0 (`mov w4, #0x0`) | `"The checksums' signature does not hold: {}"` (0x103e5ec9b) |
| `Signature::trusted_comment()` vs `format!("Hydra {}", version)` | 0x1010a4900 / `format!` 0x1010a48ec | `"The checksums are signed for \"{}\", not {}"` (0x103e5ec46) |
| parse the list | 0x1010a4a5c | — |

The list parser (`str::lines()` — `\n` search, trailing `\r` stripped, exactly
like `SplitInternal` at 0x1010a82e8 — then the filter closure @0x1010a4b48):
`line.split_once(' ')`, skip one `' '` or `'*'` binary marker, name = the rest,
digest = the part before the space, **lowercased** with a vectorised ASCII fold
(0x1010a4dc0–0x1010a4ea0).  Lines without a space or marker are dropped.

### 3.3 The signature seam (partly)

`minisign-verify` 0.3.0 does: structural checks (algorithm `ED`, key id equal,
`allow_legacy = false`) and then Ed25519 over BLAKE2b-512 of the data plus the
global signature over `signature ‖ trusted comment`.  Reproducing BLAKE2b and
curve25519 is out of scope for a std-only rebuild, so:

* key/signature **parsing, base64, the algorithm check, the key-id check and the
  trusted-comment check are implemented** (and tested),
* the arithmetic is behind `signed::Verifier`; the only verifier that ships is
  `signed::Unavailable`, which **always fails**, and `Sums::checked` uses it.
  So an unverified release can never be installed: `fetch` fails with
  `"The checksums' signature does not hold: the signature cannot be checked…"`.
* `Sums::checked_with(&verifier, …)` is the seam a networked/complete build (or
  a test) uses.  The tests install a stub verifier and exercise everything after
  it: trusted comment, list parsing, checksum matching, unpack and swap.

`decode_signature` requires all four lines of the `.minisig` file (comment,
signature, trusted comment, global signature) where `minisign-verify` treats the
last two as optional — a file without a trusted comment could never pass step 4,
so it is refused at parse time instead.

The minisign error texts the binary carries (`"Invalid encoding in minisign
data"`, `"This signature algorithm is not supported by this implementation"`,
`"The signature was created with a different key than the one provided"`, …,
`__const` 0x102402a50) are reproduced verbatim.

---

## 4. `swap` — unpack, swap, clean up

### 4.1 `beside(place, tag) -> PathBuf` (full)

`Path::file_stem` (0x1010a6320) → `String::from_utf8_lossy`, then
`format!("{}.{}.{}", stem, tag, ext)` when `Path::extension` has something
(0x1010a6378, `__cstring` 0x103e5ed88 = `{}.{}.{}`) and `format!("{}.{}", …)`
otherwise (0x103e790f6), then `Path::with_file_name` (0x1010a63f8).  The two
tags the crate uses are 3-byte literals at `__const` 0x1023fd0a3: `"new"`
and `"old"`.

### 4.2 `unpack(bytes: &[u8], name: &str) -> Result<Vec<u8>, String>` (full)

`zip::read::zip_archive::ZipArchive::new` @0x1010ad178 with
`Cursor<&[u8]>` (`stur xzr` for the cursor position), then inlined
`by_name`: `IndexMap::get_index_of` (0x1010aa8ec), `find_content`
(0x1010ac7e4), `make_reader` (0x1010ac3b8), then `read_to_end`
(0x1010acd78) into a `Vec` with capacity = the member's uncompressed size
(0x1010a674c).  Messages: `"The release file cannot be opened: {}"`,
`"The release file has no {}: {}"`, `"{} cannot be read out of the release
file: {}"` (`__cstring` 0x103e5edbd/0x103e5ede3/0x103e5ed90).

### 4.3 `put_in_place(place: &str, bytes: &[u8]) -> Result<(), String>` (full)

1. `new = beside(place, "new")`; `OpenOptions` `write+create+truncate` with mode
   `0o666` (0x1010a5e04: `mov w8, #0x1b6`) → `"{} cannot be written: {}"`;
2. `write_all` + `sync_all` (0x1010acf1c, 0x1010e8f35c) → same message;
3. `metadata(place)`; if present, `set_permissions(new, mode)` →
   `"{} cannot be made runnable: {}"`.  A missing `place` is ignored
   (0x1010a5f30 drops the error);
4. `rename(new, place)` → done;
5. on failure: `old = beside(place, "old")`, `remove_file(old)` tolerating only
   `NotFound` → `"{} cannot be taken away: {}"`; `rename(place, old)` →
   `"{} cannot be moved aside: {}"`; `rename(new, place)` →
   `"{} cannot be put in place: {}"`;
6. the retry failing puts the original back (`rename(old, place)`), removes the
   leftover `*.new` (0x1010a61f4), and says so if the put-back fails too:
   `"{} cannot be put in place ({}), and {} could not be put back: rename it to
   {} by hand before Hydra's next start"`.

`// UNKNOWN:` none of the messages say *which* executable bit is set; the
mode comes from the file being replaced, so Hydra relies on the installed binary
already being executable.

### 4.4 `clear_old(place: &str)` (full)

`fs::remove_file(beside(place, "old"))`, result dropped.  Called from
`hydra::gui::run` (0x100111958) as Hydra starts.

### 4.5 ZIP and DEFLATE (full)

`zip` 9.0.0 and `flate2` are external, so:

* `src/zip.rs` — end-of-central-directory search (with the 64 KiB comment
  window), ZIP64 locator/record, central-directory walk (with the 0x0001 extra
  field), local-header offset, stored and deflated members, CRC-32 verification.
  Error texts follow `zip`'s (`"Could not find EOCD"`, `"Oversized central
  directory"`, `"Invalid checksum"`, …) — they are the ones the binary carries.
* `src/inflate.rs` — RFC 1951: stored, fixed-Huffman and dynamic-Huffman blocks.

The tests carry a real 242-byte archive built with Python's `zipfile` (one
stored, one deflated member) and raw DEFLATE streams produced with zlib.

---

## 5. Departures from the original

| original | here | why |
|---|---|---|
| `reqwest` + `tokio` async (`look`, `fetch`, `text`, `body` are `async fn`s) | synchronous functions over a one-method `Transport` trait; the shipped `Offline` transport refuses everything | no network, no external crates; the async shape is invisible to callers of a std-only crate |
| releases URL and platform asset name are compile-time constants inside `look` | parameters of `Releases::look` / `Releases::fetch` | **not recoverable**: the binary holds no releases endpoint among its 31 `https://` literals and no `.zip`/`macos`/`arm64` asset name anywhere in `__cstring`, `__const` or `__DATA`, yet `look` is called with `(&self, &Version)` only (0x10009ed48), so both values were constants it does not spell out.  `// UNKNOWN:` marked at the signature. |
| `serde`/`serde_json` | `src/json.rs` (a small full JSON reader) + hand-written field mapping | std only; the mapping keeps `serde`'s required-field behaviour and error wording |
| `semver::Version` | `release::Version` | std only; ordering rules reproduced and tested |
| `sha2` | `src/sha256.rs` | std only; NIST vectors + streaming equality tests |
| `minisign-verify` | parsing and every structural check, `Verifier` seam for Ed25519/BLAKE2b; the shipped verifier always fails | the curve is impractical here; the safe path is explicit, never a silent accept |
| `zip` + `flate2` | `src/zip.rs` + `src/inflate.rs` | std only; stored **and** deflate supported, CRC checked |
| `Releases::new()` can fail on `ClientBuilder::build` | cannot fail (`Ok`), the failure moves to the first request | there is no client to build |
| `MOST` size guard: `"{} is larger than any release of Hydra's"` | same message, guard on the received length | the limit itself is not a literal in the binary; possibly a `Content-Length` comparison.  `// UNKNOWN:`, set to 512 MiB |
| `Sums::checked` is the only entry point | plus `checked_with`, `Sums::parse`, `Sums::list`, `Releases::with`, `Releases::transport`, `Releases::fetch_with` | helpers for tests and for a build that supplies a transport/verifier |

Nothing else is invented: module paths (`release`, `signed`, `swap`), the
function names (`newest`, `check`, `checked`, `unpack`, `put_in_place`,
`clear_old`, `Releases::{new, look, fetch}`), the constants (`USER_AGENT`,
`KEY`, `VERSION`, the three timeouts) and every message come from the binary.

## 6. Conflicts and loose ends

* `recon.py`'s `sizes` for `Sums::checked`, `swap::clear_old` and
  `Releases::new` are 5–200× too large (they stop at the next *global* symbol).
  `nm -an` boundaries were used instead — see §0.
* `beside`'s argument is a `&str` in the source but the compiler passed only its
  pointer (`adrp/add` into `__const`, no length word) and materialised the length
  from the type inside the callee.  The signature is unaffected, and both call
  sites pass 3-byte tags.
* `Releases::look`'s generator is inlined into
  `Timeout<look>::poll` (0x1002603d4); the two `text` calls and the
  `release::newest` call are visible there (0x10026053c, 0x100260604), which is
  how `look`'s shape and `newest`'s three arguments were recovered.
* The asset name is compared **exactly** (length then `memcmp`), including in
  `Sums::check`; `SHA256SUMS.txt`'s own name is looked up the same way, so a
  release that renames its checksums file is skipped rather than guessed at.
