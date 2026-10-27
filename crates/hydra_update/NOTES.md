# `hydra_update` — reverse-engineering notes

Reconstructed from the arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled globals in `symbols_demangled.txt`, private symbols from
`nm -an` + `rustfilt`).  The crate had **7 `todo!()` stubs over 4 files**
(`lib.rs`, `release.rs`, `signed.rs`, `swap.rs`); this build has none —

* `grep -Rn 'todo!\|unimplemented!' crates/hydra_update/src` is empty,
* `cargo check --offline -p hydra_update` — clean, no warnings,
* `cargo test --offline -p hydra_update` — **63 tests pass**,
* `cargo check --offline --workspace` — clean (no other crate was touched).

The binary contains the whole crate plus its consumers, so most of the calling
convention could be read from both sides (the stub and `hydra::update`).

**Phase 1 reconnected the real crates** — `reqwest`/`tokio` for the network,
`serde`/`serde_json` for the releases document, `semver` for release tags and
`zip`/`flate2` for the archive — so §5's departures are gone bar two,
`src/zip.rs` and `src/inflate.rs` are deleted, and `src/json.rs` is a
`serde_json::Value` re-export.  The
versions, the behavioural differences that came with them, what is still a seam
and what moved out of the deleted white-box tests are all in
[§7 "Phase 1 reconnection"](#7-phase-1-reconnection).

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

`release::Version` **is** `semver::Version` 1.0.28 again, in a newtype
(`release::Version(semver::Version)`, `Deref` to the inner value, `From` both
ways) so the recovered path and the recovered `as_str` stay: `parse` is
`semver::Version::parse`, `Display` is the crate's, and the ordering the binary
compares field by field — `major`, `minor`, `patch`, `Prerelease`, then
`BuildMetadata` — is `semver::Version`'s derived `Ord` (the derive lists the
fields in exactly that order, and `Prerelease`/`BuildMetadata` carry the
identifier rules: empty sorts highest, numeric identifiers compare numerically,
numeric < alphanumeric, shorter prefix first at 0x1010ae16c–0x1010ae1cc).

One correction to what the stand-in had inferred: `BuildMetadata`'s comparison
is *not* plain text — dot-separated numeric identifiers compare numerically, so
`1.0.0+build.9` < `1.0.0+build.10`.  See §7.

### 2.4 The JSON schema, by `serde`

`release::Listed` and `release::ListedFile` are `serde` derives again
(`Serialize` + `Deserialize`, no `#[serde(default)]`, so every field is
required), which is where the string table in §2.1 comes from in the first
place; `release::listed` is `serde_json::from_str::<Vec<Listed>>`, and the
`{{Visitor}}::expecting` globals at 0x1010aed0c / 0x1010aed20 are the derive's.

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
global signature over `signature ‖ trusted comment`.  `minisign-verify` is not
a workspace pin (Phase 1, §7.3) and reproducing BLAKE2b and curve25519 stays out
of scope, so:

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

`unpack` calls `zip` 9.0.0 again — `ZipArchive::new(Cursor::new(bytes))`
(0x1010ad178), `by_name`, `read_to_end` — so the end-of-central-directory
search (64 KiB comment window), the ZIP64 locator/record, the central-directory
walk with the 0x0001 extra field, the local-header offset, stored/deflated
members and the CRC-32 verification are the crate's, and `flate2` 1.1.10 (the
pinned DEFLATE backend `zip` was built with) does the inflating.  (`zip` uses
the declared uncompressed size as the decoder's buffer size, not as an equality
check — the hand-written reader also compared it.  Integrity still rests on the
CRC-32, which covers every byte returned.)  The error texts the binary carries are `zip`'s own, which
is why they read the way they do: `"invalid Zip archive: Could not find EOCD"`,
`"invalid Zip archive: Oversized central directory"`, `"specified file not found
in archive"`, and — from the CRC reader — `"Invalid checksum"`.

`src/zip.rs` and `src/inflate.rs` are **deleted**; §7.5 lists what their tests
asserted and where that is asserted now.  The tests still carry a real 242-byte
archive built once with Python's `zipfile` (one stored, one deflated member) in
`src/testdata.rs`, plus archives written by `zip`'s own writer (stored,
deflated, per-entry ZIP64, and a full ZIP64 end-of-central-directory section).

---

## 5. Departures from the original

Reconnected entries are struck through; what is left is listed in §7.2/§7.3.

| original | here | why |
|---|---|---|
| ~~`reqwest` + `tokio` async (`look`, `fetch`, `text`, `body` are `async fn`s)~~ | ~~synchronous functions over a one-method `Transport` trait; the shipped `Offline` transport refuses everything~~ **reconnected** | `look`/`fetch`/`fetch_with`/`text`/`body` are `async fn`s again and the shipped transport is the real `reqwest::Client`.  See §7.1 |
| releases URL and platform asset name are compile-time constants inside `look` | parameters of `Releases::look` / `Releases::fetch` | **not recoverable**: the binary holds no releases endpoint among its 31 `https://` literals and no `.zip`/`macos`/`arm64` asset name anywhere in `__cstring`, `__const` or `__DATA`, yet `look` is called with `(&self, &Version)` only (0x10009ed48), so both values were constants it does not spell out.  `// UNKNOWN:` marked at the signature. |
| ~~`serde`/`serde_json`~~ | ~~`src/json.rs` (a small full JSON reader) + hand-written field mapping~~ **reconnected**: `serde` derives on `Listed`/`ListedFile`, `json` is a `serde_json::Value` re-export | §7.2 |
| ~~`semver::Version`~~ | ~~`release::Version`, ordering rules reproduced~~ **reconnected**: `release::Version(semver::Version)` | §7.3 |
| `sha2` | `src/sha256.rs` (hand-rolled FIPS 180-4) | **not a workspace pin**, so it stays; §7.3 records the PLAN/NOTES conflict about whether the original even had `sha2` |
| `minisign-verify` | parsing and every structural check, `Verifier` seam for Ed25519/BLAKE2b; the shipped verifier always fails | **not a workspace pin**, and the curve is impractical to reproduce; the safe path is explicit, never a silent accept (§7.3) |
| ~~`zip` + `flate2`~~ | ~~`src/zip.rs` + `src/inflate.rs`~~ **reconnected**: calls `zip` 9.0.0 directly | §7.4 |
| ~~`Releases::new()` can fail on `ClientBuilder::build`~~ | ~~cannot fail (`Ok`), the failure moves to the first request~~ **reconnected**: fails with `"Hydra cannot reach the releases: {}"` | §7.1 |
| `MOST` size guard: `"{} is larger than any release of Hydra's"` | same message, guard on the received length | the limit itself is not a literal in the binary; possibly a `Content-Length` comparison.  `// UNKNOWN:`, set to 512 MiB.  (`reqwest` could now report `Content-Length` up front, but the guard still runs on the bytes actually received, which a missing or lying header cannot get around.) |
| `Sums::checked` is the only entry point | plus `checked_with`, `Sums::parse`, `Sums::list`, `Releases::with`, `Releases::transport`, `Releases::fetch_with`, `client_builder`, `Http`, `Offline`, `BoxFuture`, `Transport::reqwest` | helpers for tests and for a build that supplies a transport/verifier.  `Releases::with` takes an `Arc<dyn Transport>` (the binary's `Releases` is one `Arc`-sized word); `Transport::reqwest` exists so a test can see that the default really is the reqwest client (§7.1) |

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
* **`sha2` — the two records disagree.**  `PLAN.md` says "The original had no
  sha2 crate (gs_base keeps its own SHA-256)"; §3.1 says the binary calls
  `sha2::sha256::compress256` @0x1010b6b60 with the standard IV.  Both cannot be
  right, and `sha2` is not a workspace pin either way, so the crate keeps
  `src/sha256.rs`.  See §7.3.

---

## 7. Phase 1 reconnection

Every crate this one linked that is a workspace pin is a dependency again, at
the pinned version, through `x = { workspace = true }`:

| original crate | pin | used for |
|---|---|---|
| `reqwest` | 0.12.28 | `Releases::new`'s client and the shipped `Http` transport (`json`/`native-tls` features from the workspace; **not** `blocking`) |
| `tokio` | 1.53.1 | dev-dependency only: `#[tokio::test]`.  The crate never starts a runtime or spawns — its `async fn`s are awaited by `hydra::update::keep`, which owns the `tokio::time::timeout` wrappers behind `LOOK_TIMEOUT`/`FETCH_TIMEOUT` |
| `serde` / `serde_json` | 1.0.229 / 1.0.151 | `Listed`/`ListedFile` derives and the whole document (`release::listed`, `json`) |
| `semver` | 1.0.28 | `release::Version` |
| `zip` | 9.0.0 | `swap::unpack` |
| `flate2` | 1.1.10 | dev-dependency only: an independent DEFLATE decoder in the tests (it is `zip`'s own backend, which is the point) |

`Cargo.lock` gained `semver` and the crate's dependency list; no other crate
changed.  Nothing here reaches the network, at build time or in the tests.

### 7.1 The network is real again (`reqwest` + `tokio`)

| recovered | now |
|---|---|
| `Releases::new` = `ClientBuilder::default().user_agent("hydra/0.1.0-beta.7").timeout(30s)…build()` (0x1010af1bc, 208 B) | `client_builder()` does exactly that (`user_agent` 0x1010af1e0, `timeout` 0x1010af1f4, `connect_timeout` 0x1010af200) and `Releases::new` installs it — **and can fail again**, with `"Hydra cannot reach the releases: {}"` |
| `look`/`fetch`/`text`/`body` are `async fn`s | `async fn`s again; `fetch`/`fetch_with` keep the same order of operations |
| `text`'s generator: `RequestBuilder::new` → `send` → `error_for_status` (0x101cc4ac4/0x101cb08b0/0x101ce60dc) | inside `Http::get`: `request(url)` → `send().await` → `error_for_status()` → `bytes().await`.  Status handling belongs to the transport because only it can see a status; the alternative (a status-carrying `Vec<u8>`) would have changed the seam's contract for nothing |
| `Releases` is one word — the `reqwest::Client` `Arc` | still one word: `Arc<dyn Transport>` |
| the `timeout(60s)` / `timeout(15min)` wrappers | untouched: they live in `hydra::update::keep`, and `LOOK_TIMEOUT`/`FETCH_TIMEOUT` are still the constants it uses |

The seam is kept, in the shape that survives object safety in edition 2021:

```rust
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Transport: Send + Sync {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>>;
    fn reqwest(&self) -> Option<&reqwest::Client> { None }   // diagnostics/tests
}
```

* What ships is `Http` (the real client).  `Offline` is still there and
  still refuses everything, but it is no longer what `Releases::new` installs —
  a caller that must stay off the network says `Releases::with(Arc::new(Offline))`.
  `Offline`'s message (`"no transport is installed, so {} was not fetched"`) is
  a reconstruction, not a recovered literal, and is unchanged.
* `Releases` no longer implements `Default`: the original has no default client,
  and the only honest one (the real client) can fail.  That is the one public
  API item dropped in Phase 1; nothing in the workspace used it.
* reqwest has **no getters** for client configuration, and it copies the
  builder's default headers and total timeout onto a request only inside
  `execute_request`.  So `Http::request` sets the same two values — one
  `user-agent: hydra/0.1.0-beta.7` header, one 30 s total timeout — on the
  request itself as well, which is what makes them assertable without a network
  round trip (`tests::the_default_updater_is_the_recorded_reqwest_client`).
  Behaviour is unchanged: those are the values the client sends either way.  The
  connect timeout is only observable from inside reqwest, so it is covered by
  the constant assertion (`tests::carries_the_recorded_client_settings`) and by
  the builder call itself.
* `Transport::reqwest` exists so that test can prove the *default* updater holds
  a real `reqwest::Client` (it reads `None` for every other transport).

### 7.2 `serde`/`serde_json` 1.0.229/1.0.151 — the document again

`Listed`/`ListedFile` are `Serialize + Deserialize` derives with the recovered
field names and declaration order, no `#[serde(default)]` (so all of them are
required), and no rename attributes — the GitHub names *are* the field names.
`release::listed` is `serde_json::from_str::<Vec<Listed>>`, the recovered
`from_trait` @0x1010a0f70.  `src/json.rs` keeps its name and its `parse`, but
`Json` is now `serde_json::Value`; the hand-written parser (and its
`Json::kind()` helper) is gone, so there is no second parser in the tree.

Behavioural differences, all of them `serde`'s more precise wording:
`"missing field `draft` at line 1 column 49"` instead of the hand-written
`"missing field `draft`"`, `"invalid type: string "no", expected a boolean at
line 1 column 34"` instead of `"invalid type: expected a boolean for `draft`"`.
`newest` still wraps them in `"The releases could not be read: {}"`.

New tests: `release::tests::the_schema_is_the_recovered_one` (serialized field
order and names, round trip, every required field of both structs, a mistyped
field) and `json::tests::is_serde_json` (the value type and its escaping).

### 7.3 `semver` 1.0.28, `sha2`, `minisign-verify`

**`semver` is back.**  `release::Version` is a newtype over `semver::Version`
with `Deref` (so `version.major`/`.pre`/`.build` still read), `From` in both
directions, `Display`, `Default` (`0.0.0`) and the recovered `as_str`.  The
ordering is the crate's derived `Ord` — `major`, `minor`, `patch`,
`Prerelease`, `BuildMetadata` — which is the chain the binary inlines at
0x1010ae16c–0x1010ae1cc, and `newest`'s "only if greater than the running
version" test is unchanged.

Two honest corrections came out of it:

1. **`BuildMetadata` is compared numerically per identifier, not as text.**  The
   stand-in compared the whole string, so its test asserted
   `1.0.0+build.10 < 1.0.0+build.9`.  With the real crate it is the other way
   round (`build.9 < build.10`); the test now says so and
   `version_ordering_agrees_with_semver` checks the newtype's ordering against
   `semver::Version`'s on every pair of a ten-version ladder, so the two can
   never drift again.
2. **The parse error is `semver`'s** (`"unexpected end of input while parsing
   minor version number"`, `"unexpected character 'v' while parsing major
   version number"`) instead of the stand-in's
   `"unexpected character in version "1.0": unexpected fourth number"`-style
   text.  Every rejection the old tests asserted is still rejected; only the
   wording assertions changed (`rejects_versions_semver_rejects`,
   `unreadable_documents_are_reported`).

**`sha2` is not a workspace pin, so `src/sha256.rs` stays.**  This is where the
two records conflict: `PLAN.md` says the original had no `sha2` crate, §3.1 says
the binary calls `sha2::sha256::compress256` @0x1010b6b60 with the standard IV
from `__const` 0x1023fd008.  Either way it is not in the pins, adding it at a
crate-local version would go around the pin policy, and the stand-in is
verified against FIPS 180-4's own vectors (empty, `abc`, the 448-bit two-block
message, a million `a`s) plus streaming/one-shot equality across the padding
boundaries — so the checksum comparison in `Sums::check` is still checked
byte-for-byte.  `sha2` 0.10.9/0.11.1 happen to be in the offline registry; that
is not an invitation to bypass the pins.

**`minisign-verify` is not a workspace pin, so the seam stays.**  Key and
signature parsing, base64, the algorithm and key-id checks and the trusted
comment are implemented and tested exactly as before; the Ed25519/BLAKE2b
arithmetic is still behind `signed::Verifier`, and the only verifier that ships
is `signed::Unavailable`, which fails closed.  No curve verification was
invented.  `fetch` therefore still cannot install anything on trust — that is
the intended behaviour, not a regression: a release whose signature cannot be
checked is a release that is not installed.

### 7.4 `zip` 9.0.0 + `flate2` 1.1.10 — the archive again

`swap::unpack(bytes, name) -> Result<Vec<u8>, String>` keeps its recovered
signature, message shapes and order of checks; the body is now

```rust
let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;   // "The release file cannot be opened: {}"
let mut entry = archive.by_name(name)?;                        // "The release file has no {}: {}"
entry.read_to_end(&mut out)?;                                  // "{} cannot be read out of the release file: {}"
```

so ZIP64, both compression methods (and every other method `zip` was built
with) and the CRC-32 check are the crate's.  One check moved shape: the
hand-written reader compared the returned length against the declared
uncompressed size and called a mismatch `"Invalid checksum"`; `zip` passes that
size to its decoder as a buffer hint and relies on the CRC-32 for integrity, so
an archive whose declared size is wrong but whose bytes and CRC agree now reads
instead of failing.  That is the real crate's behaviour (which is what the
binary had), and no integrity check is lost — a wrong size means different
bytes, and different bytes fail the CRC.

The recovered wrapper messages are untouched; the payloads are `zip`'s `Display`
texts, which are the strings the binary itself carries
(`"invalid Zip archive: Could not find EOCD"`, `"specified file not found in
archive"`, `"Invalid checksum"`), so the previously `// UNKNOWN:` "file not
found" guess is now the real wording in the tests.

One small difference from the binary's body: the buffer's capacity hint is
`entry.size().min(64 MiB)` instead of the member's declared size.  The binary
(`Vec::with_capacity` at 0x1010a674c) would abort on a corrupt central directory
that declares 2^60 bytes; `read_to_end` still returns the whole member and `zip`
still verifies its CRC-32.  No check is weakened — one unbounded allocation is
removed.

### 7.5 What the deleted white-box tests covered, and where it lives now

`src/zip.rs` (6 tests) and `src/inflate.rs` (no tests of its own; its decoder
was covered through `zip.rs`) are gone.  The coverage moved to black-box tests
through `unpack` and through `zip` itself, in `swap::tests`:

| deleted test | now |
|---|---|
| `zip::tests::lists_both_members` (names, methods 0/8, sizes 15/21/23) | `swap::tests::reads_real_stored_and_deflated_members` — same assertions, read off the real `ZipArchive` metadata |
| `zip::tests::reads_a_stored_member` | `swap::tests::unpacks_the_member_it_is_named` (+ the stored member's bytes are sliced out of the archive and compared verbatim) |
| `zip::tests::reads_a_deflated_member` | `swap::tests::unpacks_the_member_it_is_named` (+ the deflated member's raw bytes are decoded by `flate2` independently) |
| `zip::tests::a_missing_member_is_none` | `swap::tests::unpack_reports_what_went_wrong` (exact, and exact-match is still required) |
| `zip::tests::refuses_broken_archives` (empty, truncated, CRC) | `swap::tests::refuses_a_truncated_archive`, `refuses_a_member_whose_checksum_does_not_match`, `refuses_a_member_whose_deflate_stream_is_damaged` |
| `zip::tests::crc32_matches_the_reference` (the reference values `zipfile` wrote) | gone as a unit test: CRC-32 is `zip`'s `crc32fast` now, and `refuses_a_member_whose_checksum_does_not_match` proves the check is live |

Added with the real crate: `swap::tests::unpacks_what_the_real_writer_wrote`
(stored, deflated, per-entry ZIP64 with its 0xFFFFFFFF sentinels and 0x0001
extra field, and a full ZIP64 end-of-central-directory section — the last two
had no test before, because the hand-written reader's ZIP64 path was untested
here), `swap::tests::a_wrong_declared_size_is_not_an_error_of_its_own` (records
the one check that changed shape, see above) and
`swap::tests::overwrites_a_longer_leftover_new_file`.

### 7.6 Gate

```
cargo check --offline -p hydra_update   # clean, no warnings
cargo test  --offline -p hydra_update   # 63 passed
cargo check --offline --workspace       # clean (no other crate touched)
```

Nothing in this crate reaches the network during either command: the client is
built and its request is inspected, never sent.
