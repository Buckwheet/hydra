# `gs_base` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(demangled symbols in `/Users/cpmbpro/Code/hydra-inspect/symbols_demangled.txt`).
The crate is the vendored `gs_studio` leaf crate
`gs_studio-…/crates/gs_base/src/*.rs`; 24 exported symbols survive, and this
document records what each one does, which constants were read out of the
disassembly, and what could not be established.

The crate has no unimplemented stubs. It was recovered `std`-only; after Phase 1
it depends on `anyhow`, `serde`, `toml`, `zip` and `flate2` at the pinned
workspace versions (see **Phase 1 reconnection**).

> **Phase 1 update.** Four of the six departures below are gone: `anyhow`, `zip`
> (+`flate2`), `toml` and `serde` are reconnected at the pinned workspace
> versions. The SHA-256 stays hand-written (the original had **no** `sha2`
> crate — `PLAN.md` line 12), `dirs` stays a std stand-in (not pinned) and
> `vellum_light::phase_keyframes` stays recovered (vendored, not on the
> registry). See **Phase 1 reconnection** at the end of this file.

---

## Legend

| tag | meaning |
|---|---|
| **established** | read directly out of the instruction stream / literal pool; the value or control flow is not in doubt. |
| **INFERRED** | the behaviour is constrained by evidence (message text, struct sizes, call shapes) but a source-level detail (field type, name, argument order) had to be chosen. |
| **UNKNOWN** | the binary does not pin this down. A safe default was chosen and the choice is written where it is used. |

Recovered-constant helpers: `/tmp/hydra_disasm.py` (objdump driver),
`/tmp/hydra_dis/hydra_gs_base.all.txt` (pre-extracted per-function disassembly),
`/tmp/hydra_lits_gs_base.txt` (referenced literals).

---

## Function table

| symbol | address | status | recovered signature |
|---|---|---|---|
| `body::part_label` | `0x100a8b760` | established | `fn part_label(name: &str) -> Option<BodyPart>` |
| `body::canonical_part` | `0x100a8b7f4` | established + 1 UNKNOWN | `fn canonical_part(name: &str) -> &'static str` |
| `data_dirs::base_dir` | `0x100a8f7c8` | established | `fn base_dir() -> Result<PathBuf>` |
| `data_dirs::base_generation` | `0x100a8f7bc` | established | `fn base_generation() -> u64` |
| `data_dirs::bodies_dir` | `0x100a8f490` | established | `fn bodies_dir() -> Result<PathBuf>` |
| `data_dirs::global_dir` | `0x100a8f5b0` | established | `fn global_dir() -> Result<PathBuf>` |
| `data_dirs::set_base_dir` | `0x100a8f66c` | established | `fn set_base_dir(dir: PathBuf)` |
| `elanthian_time::eastern_date` | `0x100a93f10` | established | `fn eastern_date(t: i64) -> (i64, u32, u32)` |
| `elanthian_time::elanthian_hour_f` | `0x100a9443c` | established | `fn elanthian_hour_f(t: f64) -> f32` |
| `elanthian_time::eastern_is_dst` (local) | `0x100a940b8` | established | `fn eastern_is_dst(t: i64) -> bool` |
| `packs::sha256_hex` | `0x100a76c6c` | established | `fn sha256_hex(data: &[u8]) -> String` |
| `packs::install_zip` | `0x100a76f84` | partly INFERRED | `fn install_zip(zip: &[u8], expected_sha256: &str, global: &Path) -> Result<Report>` |
| `packs::ledger` | `0x100a78740` | established + INFERRED schema | `fn ledger(root: &Path) -> Result<Ledger>` |
| `packs::remove` | `0x100a7892c` | established | `fn remove(root: &Path, id: &str) -> Result<Report>` |
| `packs::Manifest::from_toml` | `0x100a79edc` | established messages, INFERRED schema | `fn from_toml(text: &str) -> Result<Manifest>` |
| `packs::Report::summary` | `0x100a785cc` | established | `fn summary(&self) -> String` |
| `png_meta::read_text_bytes` | `0x100a8156c` | established | `fn read_text_bytes(data: &[u8], keyword: &str) -> Option<String>` |
| `png_meta::chunks` (local) | `0x100a817a4` | established | `fn chunks(data: &[u8]) -> Option<Vec<Chunk<'_>>>` |
| `sky::GameSky::sky` | `0x100a75e78` | established | `fn sky(&self, month: u32, day: u32, time: f64, hour: f32) -> Sky` |
| `sky_light::moon_light` | `0x100a893d0` | established | `fn moon_light(sky: &Sky, hour: f32) -> f32` |
| `sky_light::lornon_light` | `0x100a893dc` | established | `fn lornon_light(sky: &Sky, hour: f32) -> f32` |
| `sky_light::moon_light_of` (local) | `0x100a89538` | established | `fn moon_light_of(kind, window_kind, [f32;2], cloud, hour, phase, transit) -> f32` |
| `sky_light::moon_sky_arc` | `0x100a893e4` | established | `fn moon_sky_arc(kind: u32, hour: f32, transit: f32, offset: f32) -> Option<MoonArc>` |
| `sky_light::night_share_in` | `0x100a89710` | established | `fn night_share_in(times: &[f32;4], hour: f32) -> f32` |
| `status::Status::from_name` | `0x100a8bac4` | established (name table) | `fn from_name(name: &str) -> Option<Status>` |
| `status::Statuses::has_name` | `0x100a8c8a4` | established | `fn has_name(&self, name: &str) -> bool` |
| `status::Statuses::resolved` | `0x100a8c8e0` | established | `fn resolved(self) -> Statuses` |

(25 rows because two local helper symbols — `eastern_is_dst`, `chunks`,
`moon_light_of` — are recovered alongside the 24 exported ones.)

---

## Recovered constants

### `sky` / `sky_light`

| constant | evidence | value |
|---|---|---|
| `C_YEAR` | double bits `0x417E187E00000000` | 31 557 600 s = 365.25 d |
| `C_SLOW` | double bits `0x414539C000000000` | 2 782 080 s = 32.2 d |
| `C_FAST` | double bits `0x41233F8000000000` | 630 720 s = 7.3 d |
| `FOLD_HI` | `0x00E1B6D0` | 14 792 400 s |
| `FOLD_PEAK` | `0x00E23560` | 14 824 800 s |
| `FOLD_LO` | `0x01D2F950` | 30 603 600 s |
| `FOLD_END` | `0x02C3BD40` | 46 382 400 s |
| `SEASON_MAGIC` | `0x166DB073`, `lsr #39` | 376 287 347 = ⌈2³⁹/1461⌉ |
| `DAWN_BASE` | immediate `#0x3f48` | 16 200 = 4.5 h × 3600 |
| `DUSK_SUM` | float `0x47891C00` | 70 200.0 |
| month table | 12 × `u32` @ `0x1021f0964` | `[0,31,59,90,120,151,181,212,243,273,304,334]` |
| keyframe weights | 4 × `f32` @ `0x1021f0560` | `[0.5, 0.0, 0.5, 1.0]` |
| flat day window | 4 × `f32` @ `0x1021b45e0` | `[6.0, 8.0, 18.0, 20.0]` |
| moon elevation | immediate `0x425C0000` | 55.0° |
| moon light divisor | immediate `0x45610000` | 3600.0 |
| moon-light clamp | `0xBF666666` | −0.9 (`1 - 0.9·cloud`) |
| arc sweep | `0x428C0000` | 70.0° |
| transit offset | immediate `#13` | +13 h |
| circle constant | `0x40490FDB` / `0x40C90FDB` | π / 2π as `f32` |

### `elanthian_time`

| constant | evidence | value |
|---|---|---|
| EST offset | `mov x8, #-0x4650` | −18 000 s (−5 h) |
| EDT offset | `mov x9, #-0x3840` | −14 400 s (−4 h) |
| seconds/day | `movk w10, #0x1 ; #0x5180` | 86 400 |
| days→civil epoch | `#0xaf000 + #0xa6c` | 719 468 |
| DST start | `#0x1c20` | 7 200 s (02:00 EST = 07:00 UTC) |
| DST end | `#0xe10` | 3 600 s (01:00 EST = 06:00 UTC) |
| sanity range | `#0x8000`/`0xA528`/`0x746`, `16e12` | ±8 000 000 000 000 s |
| `civil_from_days`-in-DST shift | `#0xaf000 + #0xa68` | 719 464 (`719 468 − 4`) |

The DST helper uses `719 464` rather than `719 468`; combined with
`(7 − days % 7) % 7` that makes the result `first_sunday − 1`, which is exactly
what the March (`| 8` ⇒ second Sunday) and November (`+ 1` ⇒ transition day)
comparisons need. Verified against 2020 and 2024 transitions.

### `packs`

| constant | evidence | value |
|---|---|---|
| SHA-256 IV | 32 bytes @ `0x1021eff08` | FIPS 180-4 initial hash value |
| SHA-256 `K` | inlined `compress256` | FIPS 180-4 round constants |
| max archive entries | `mov w9, #0x86a0 ; movk #1` | 100 000 (strictly-greater test) |
| packs dir | literal @ `0x1021eff28` | `packs` |
| ledger file | literal @ `0x1021eff2d` | `installed.toml` |
| staging dir | literal @ `0x1021effa4` | `pack-tmp` |
| manifest file | literal @ `0x1021eff9d` | `pack.toml` |
| required prefix | `FILE_PREFIX` | `global/` |
| ZIP markers | @ `0x1021f0ec9…` | `PK\x05\x06`, `PK\x06\x06`, `PK\x01\x02` |

SHA-256 verified against the four FIPS/NIST vectors (`""`, `"abc"`, the 56-byte
message, and one million `'a'`s).

### `status`

33 discriminants (`0..=32`); `Option<Status>` uses `0x21` (33) as its `None`
byte. `resolved()` is `if self.0 & (1<<26) == 0 { self.0 &= !(1<<21) }`, i.e.
bit 26 (`Staggered`) keeps bit 21 (`Calm`).

### `body`

Two tables, both recovered verbatim:

* `0x1043b9120`, 14 × 32-byte `(key, label)` pairs — `head`, `leftEye`,
  `rightEye`, `neck`, `chest`, `abdomen`, `back`, `leftArm`, `rightArm`,
  `leftHand`, `rightHand`, `leftLeg`, `rightLeg`, `nsys`;
* `0x1043b92e0`, the same keys re-ordered for `canonical_part`.
* Aliases: `nerves → nsys`, `leftfoot → leftLeg`, `rightfoot → rightLeg`.

### `data_dirs`

Environment variable `VELLUM_FE_DIR` (13 bytes @ `0x1021f0ea0`), fallback
directory `.vellum-fe` (10 bytes @ `0x1021f0eca`), `global` (6 @ `0x1021f0e9a`),
`bodies` (6 @ `0x1021f0e94`), and the error text
`Could not find home directory` (29 @ `0x1021f0ead`).

---

## std departures

Rows struck out are the ones Phase 1 removed; the section at the end of the file
records what replaced them.

| original dependency | replacement | status |
|---|---|---|
| ~~`anyhow`~~ | ~~`gs_base::Error` (boxed `String`) + `Result<T>` alias~~ | **gone** — real `anyhow` 1.0.104 (`pub use anyhow::Error` + `pub type Result<T>`). The observable message text is preserved exactly; only `Error::message()` became `Error::to_string()` in the assertions, because `anyhow::Error` is neither `Clone` nor `PartialEq`. |
| `sha2::Sha256` | **`packs::Sha256` (kept)** | Not a departure to undo: `PLAN.md` records that the original had **no** `sha2` crate, and `compress256` is inlined in the binary. Written from scratch from the recovered IV and `K`. |
| ~~`zip` crate (`zlib_rs` inflate)~~ | ~~`packs::ZipArchive` + `packs::inflate`~~ | **gone** — real `zip` 9.0.0 behind `packs::ZipArchive`, and `packs::inflate` now wraps `flate2` 1.1.10. |
| ~~`toml` + `serde`~~ | ~~`packs::parse_toml`~~ | **gone** — real `toml` 1.1.6 + `serde` 1.0.229; `Manifest`/`Ledger`/`Installed` derive `Deserialize`, `parse_toml` maps `toml::Table` onto `TomlValue`. |
| `dirs::home_dir` | **`data_dirs::home_dir` (kept)** | `dirs` is not in the root `[workspace.dependencies]` pins, so the `$HOME`/`$USERPROFILE` stand-in stays. |
| `vellum_light::phase_keyframes` (vendored crate) | **`sky_light::phase_keyframes` (kept)** | `vellum_light` is a vendored crate, not a registry crate, so there is no version to pin. Fully recovered from `0x100ac3dcc`; returns `Vec<Keyframe>` with the same 8-byte elements. |

---

## Phase 1 reconnection

Registry crates are pinned in the root `Cargo.toml` `[workspace.dependencies]`;
this crate asks for them with `x = { workspace = true }`. Versions are the ones
recorded in `~/Code/hydra-inspect/crate_versions.txt`.

| real crate | pinned version | what it replaced |
|---|---|---|
| `anyhow` | `=1.0.104` | `gs_base::Error` (boxed `String`) and the hand-rolled `Result<T>`; now `pub use anyhow::Error` + `pub type Result<T> = anyhow::Result<T>` |
| `zip` | `=9.0.0` | the hand-written ZIP reader (EOCD → CDFH → local header) behind `packs::ZipArchive` / `packs::ZipEntry` |
| `flate2` | `=1.1.10` | the hand-written DEFLATE decoder behind `packs::inflate` (fixed *and* dynamic Huffman blocks) |
| `toml` | `=1.1.6` | the hand-written TOML subset in `packs::parse_toml` / `Manifest::from_toml` / `ledger` |
| `serde` | `=1.0.229` | the hand-written field-by-field readers for `Manifest`, `Ledger`, `Installed` (all three now derive `Deserialize`; `Installed`/`Ledger` also `Serialize`) |

Feature notes: `zip` takes the workspace default feature set (all compression
methods). Its `default-features` cannot be trimmed from a member while the root
pin leaves it unspecified, and the full set is what the original binary was
built with, so nothing is disabled here. `flate2` is taken with its defaults
(`miniz_oxide` backend); `zip` additionally turns on `flate2/zlib-rs`, which is
also pure Rust.

Behaviour that the real crates changed (all *not* recovered literals):

* **`toml` error text.** A syntax error in `pack.toml` / `installed.toml` now
  reads `TOML parse error at line …, column …` (with a caret snippet) instead of
  the hand parser's `pack.toml: cannot parse line '…'` /
  `pack.toml: unterminated string`. The *validation* messages
  (`pack.toml: id '{}' is not lower snake case`,
  `pack '{}' is format {}, newer than this build understands ({})`,
  `pack file '{}' is not under global/`, `no pack '{}' is installed`) are
  unchanged to the character.
* **`format` may be written as a string.** The recovered reader accepted both
  `format = 1` and `format = "1"` (and fell back to `0` for a negative value),
  so `Manifest::format` keeps a small `deserialize_with` that accepts an integer
  or a numeric string. A non-numeric string is now a deserialization error
  (`format 'soon' is not a number`) rather than the "newer than this build
  understands" message.
* **CRC-32 is now verified** on entry reads (the `zip` crate checks it; the
  hand-written reader did not). Malformed archives therefore fail earlier.
* **`inflate` error text** is `flate2`'s `io::Error`; the old
  `not a zip: bad deflate block` wording was never recovered.
* **`packs::ZipArchive::read(&self, …)`** still takes `&self`: the wrapper
  re-opens the archive (the `zip` crate wants `&mut` for a read) so the public
  API stays as it was. `install_zip` uses the crate directly and reads entries
  from the single open archive, so installation is not quadratic.
* **`TomlValue` gained `Float(f64)` and `Datetime(String)`** (the real parser
  produces both), so the enum is `PartialEq` but no longer `Eq`.

Kept by hand, with the reason:

| stand-in | reason |
|---|---|
| `packs::Sha256` / `sha256_hex` | `PLAN.md` line 12 is authoritative: the original had **no** `sha2` crate, and `compress256` is inlined in the binary. The earlier claim in this file that the original used `sha2` was wrong and is corrected above. |
| `data_dirs::home_dir` | `dirs` is not in `[workspace.dependencies]` (not pinned, not pre-fetched). |
| `sky_light::phase_keyframes` | `vellum_light` is a vendored crate, not on the registry. |
| `packs::safe_relative`, the entry/size guards, `MAX_ENTRIES` / `MAX_UNPACKED_BYTES` | recovered Hydra policy, not something a crate provides. |

---

## Significant unknowns

1. **`GameSky`'s full layout.** `sky()` reads only `self + 0x10` (`u32`) and
   `self + 0x14` (`f32`); the first 16 bytes are never touched by this function.
   Modelled as a two-field struct. UNKNOWN.
2. **`Sky::x08`, `Sky::x10`, `Sky::cloud`.** `sky()` writes `1`, `1`, `0`
   unconditionally. `x10 == 1` selects the seasonal dawn/dusk window and `cloud`
   scales `1 − 0.9x` inside `moon_light_of`, so the *effects* are established but
   their source-level meaning is INFERRED.
3. **`Sky::festival` / the month-12 night floor.** The arithmetic
   (`clamp((day − 20 + hour/24)/11, 0, 1)` for `month == 12 && day > 19`, and
   `1.0` on `month == 12 && day == 31`) is exact; what game event it models is
   INFERRED.
4. **`canonical_part`'s no-match return.** The binary returns a null pointer with
   a stale length (never a real empty slice) — most likely
   `unwrap_or_default()`. This returns `""`.
5. **`Manifest` schema.** Only three field names survive (`id`, `format`,
   `files`) via the validation messages; `files` is modelled as a list of paths
   under `global/`. Value types (`u32` for `format`) are INFERRED.
6. **`Ledger` / `Installed` schema.** serde's static pool has `Ledger`, `format`,
   `Installed`, `files`; a `pack` field name is also present but its owner could
   not be pinned down, so it is omitted. `ledger()` accepts both an
   `[installed.…]` and a `[packs.…]` table shape.
7. **`install_zip`'s third/fourth arguments.** The binary's `x2/x3` is the
   expected SHA-256 string; the destination is one of `x4`/`x5` (both single
   words). Modelled as a `&Path` `global` directory.
8. **`MAX_UNPACKED_BYTES`.** The "unpacks to more than a pack may" guard exists
   but its immediate could not be isolated; modelled on the entry limit
   (`100 000 000` bytes).
9. **Status name table provenance.** Every comparison constant in
   `Status::from_name` was decoded and grouped by the length the compiler tested.
   Seven 5-character entries (`kneel`, `prone`, `sleep`, `bleed`, `bound`,
   `blind`, `dazed`) had their 4-byte heads reconstructed through `add`/`orr`
   chains rather than literals — see the doc comment on `STATUS_NAMES`.
10. **`Report`'s field types.** `summary` reads four `.len()` values 0x18 apart
    plus two `String`s, i.e. four `Vec<String>`s; that shape is INFERRED.

---

## Tests

`cargo test -p gs_base --offline` — **63 tests**, all passing. Coverage of the
high-value areas:

* **SHA-256** — four NIST vectors (`""`, `"abc"`, the 56-byte message, one
  million `'a'`s), including a chunked update across the 64-byte boundary, plus
  the IV/`K` table spot-checks.
* **`elanthian_time`** — `civil_from_days`/`days_from_civil` round-trips over
  eight magnitudes and five exact dates; three `eastern_date` cases (EST and EDT);
  the 2020/2024 DST transitions to the second; whole-month DST rules;
  `first_sunday_offset` for four month/year pairs; five `elanthian_hour_f` values
  including the out-of-range guard.
* **`sky` / `sky_light`** — every double constant checked against its bit
  pattern; day fraction, dawn/dusk, festival and night-floor arithmetic; the
  triangle-wave fold; the keyframe table at both ends and midpoints; night-share
  values at seven hours; illumination curve; arc angles/elevations including the
  `None` cases; and moon-light peak/cloud-dim values.
* **`body`** — case-insensitive parsing, the one-based discriminants, all 14
  labels, and the three aliases plus their negatives.
* **`data_dirs`** — the exact joins, the env override, the home fallback, the
  missing-home error text, and `set_base_dir`'s generation bump.
* **`packs`** — manifest accept/reject with the recovered message wording, the
  `lower_snake_case` rules, `Report::summary` with and without the "kept" tail,
  the TOML subset (now via the real crate, including floats, date-times and
  nested headers), a ZIP round-trip written by the real `zip` writer for both
  stored and deflated entries (with the recovered `PK` markers checked on its
  output, and CRC-32 read back from the central directory), an `inflate`
  round-trip against the real `flate2` encoder, the path-traversal and symlink
  rejections, an end-to-end install → re-install (kept) → remove cycle against a
  real directory, a deflated pack installed end to end, ledger round-trips
  through the real `toml` writer, both ledger table shapes (`[packs.…]` and
  `[installed.…]`), and the typed `anyhow` chain on an io error.
* **`png_meta`** — hand-built PNG byte vectors covering `tEXt` extraction, other
  chunk types, keyword length mismatch, truncated input, missing NUL, non-UTF-8
  payloads and the chunk walk.
* **`status`** — discriminants and the `0x21` sentinel, name parsing with
  whitespace/case folding, the full 33-entry round-trip, `has_name`, and all four
  `resolved()` cases.

Verification commands (all clean in this worktree):

```
cargo check -p gs_base --offline
cargo test  -p gs_base --offline
cargo check --workspace --offline
grep -rn 'unimplemented' crates/gs_base   # empty
```
