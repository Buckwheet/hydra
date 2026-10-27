# `hydra_exp` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled symbols in `symbols_demangled.txt`). Every address below is
an **unslid virtual address**. The crate is Hydra's experience tracker: it parses
what the game says about experience, keeps per-creature/per-hunt rows, computes
levels and rates, and prints the `exp …` reports.

Status: `cargo check -p hydra_exp --offline` and `cargo test -p hydra_exp
--offline` pass (**47 tests**); `grep -rn 'todo!()\|unimplemented!()'
crates/hydra_exp/src` is empty. Phase 1 (Layer 2) reconnected `hydra_ledger`
behind the crate's row/report seam — see “the `hydra_ledger` seam” below. Phase 2
(audit-core) reconnected **`tokio`** — the crate's second real dependency, which
Phase 1 missed — see [Dependency
verdict](#dependency-verdict-tokio--a-real-missed-dependency).

---

## Legend

| tag | meaning |
|---|---|
| **ESTABLISHED** | in the instruction stream, in a string literal, or in a field offset — re-derivable from the addresses quoted. |
| **INFERRED** | follows from the code shape or from a recovered sibling crate, but the original type/name is not in the binary. |
| **UNKNOWN** | not recoverable; the safest behaviour was chosen and is marked `// UNKNOWN:` at the site. |

---

## Tooling

Read-only; nothing was written outside the worktree.

* `python3 ../recon.py {syms,sizes,disas,bytes,str,findstr}` (the shared helper).
* A throwaway driver in `$TMPDIR/recon/` built on the same section/symbol loader:
  * `dis.py` — disassemble with `adrp`/`add` folded, literals decoded, every
    `bl` resolved to `symbol+offset`;
  * `fmts.py` — find every `bl alloc::fmt::format::format_inner` site, recover
    the `x0` format blob and **decode it** (see “format blobs” below);
  * `fninfo.py` — per function: literal pool targets + call histogram;
  * `offs.py` — histogram of `[xN, #imm]` field offsets (used for the row
    layouts);
  * `fend.py` — find the real end of a function (prologue/epilogue match),
    needed because **a symbol ends where the next *named* symbol begins**, and
    several hydra_exp symbols are followed by thousands of unnamed outlined
    functions of other crates (hydra_gui, hydra_combat_log, rusqlite, …);
  * `findtab.py`/`findtab2.py`/`findtab3.py` — recover `&str` tables and
    `(label, sub, help, kind)` struct tables from `__const`/`__cstring`;
  * `callers.py` — scan `__text` for every `bl` to a given address.
* Fallback disassembly: `/usr/bin/objdump -d --start-address=… --stop-address=…
  --no-show-raw-insn hydra` (`--disassemble-symbols` does not work on this file).

### Format blobs (the key to every report string)

`format!` in this build does not pass `&[&str]` pieces. `alloc::fmt::format::
format_inner(x0 = blob, x1 = tagged args pointer)` receives a **compact,
NUL-terminated blob**:

```
[len][text bytes] …         a literal piece (len < 0x80)
0x80 [u16 len][text]        a literal piece longer than 127 bytes
0xc0                        "{}" — the next argument, default spec
0xNN (>= 0x81, != 0xc0)     an argument with a spec, payload per bit:
                              bit0: u32 spec word follows   (fill + flags + align)
                              bit1: u16 follows             (width)
                              bit2: u16 follows             (precision)
                              bit3: u16 follows             (argument index)
                              bit4/bit5: width/precision come from another argument
0x00                        end of the blob
```

`core::fmt::write` (`0x101ebd9ec`) is the authoritative decoder; `format_inner`
(`0x101eb6920`) walks the same blob to size the `String`. The spec word is the
`Formatter`'s 8-byte option block (`core::fmt::write` builds it at `sp+0x18`:
u32 at `+0x10`, width u16 at `+0x14`, precision u16 at `+0x16` relative to the
`Formatter`), whose bits were pinned from `Formatter::{pad, pad_integral}`:

| bits | meaning |
|---|---|
| 0…20 | fill character (`' '` = `0x20` in every site in this crate) |
| 21 / 22 | `+` / `-` sign flags |
| 23 | sign-aware zero pad (`pad_integral`, `tbnz w26, #0x17`) |
| 24 | set by `{:<02}`-style integer specs (the clock `{:02}` sites) |
| 27 / 28 | width / precision present (`Formatter::pad`, `tst w22, #0x18000000`) |
| 29…30 | alignment: 0 = left, 1 = right, 2 = centre, 3 = unknown (`ubfx w8, w22, #29, #2`) |

So e.g. the table headers' spec words decode as `0x08000020` + width = `{:<10}`,
`0x28000020` + width = `{:>12}`, `0x70000020` + precision 1 = `{:.1}` and
`0x69000020` + width 2 = `{:02}` (clock fields).

---

## `hydra_exp` in the binary

27 symbols, but the *reported* sizes are misleading: `catalog::Entry::matches`
(61 KB), `command::answer` (33 KB), `tracker::Tracker::knows` (54 KB),
`recorder::start` (45 KB) and `report::snapshots` (82 KB) are followed by
thousands of unnamed outlined functions belonging to other crates (their literal
pools contain `hydra_combat_log` SQL, `image-0.25.10/src/images/buffer.rs`,
`JoinHandle polled after completion`, `__TEXT`, …). The real bodies were found by
locating the entry's epilogue: `matches` is `0x1012ec37c .. 0x1012ecdf4`
(2,680 B), `answer` is `0x1012cc990 .. 0x1012cce7c` (1,260 B, with a cold block
after its `ret`) and `knows`' `ret` is at `0x1012de020`.

Surviving symbol names are the only ones in the crate — there are no
`drop_in_place<hydra_exp::…>` symbols, so the field sets below come from the
offset arithmetic, not from drop glue.

---

## Symbol inventory (BOTH sources: `symbols_demangled.txt` + `nm -a | rustfilt`)

`symbols_demangled.txt` lists **27** global symbols for this crate. `nm -a |
rustfilt` over the whole binary yields **41** lines naming `hydra_exp`, which
collapse to 30 distinct `hydra_exp::`-prefixed names — the 27 globals plus
**three symbols the global-only table cannot see**:

* `hydra_exp::tracker::Tracker::take` at `0x1012dda80` (1152 B) — a private
  method that the previous pass's function table **missed entirely**;
* `hydra_exp::command::read::{{closure}}` (2 addresses, `0x1012cbc70`,
  `0x1012cbd3c`);
* `hydra_exp::command::parse::{{closure}}` (`0x1012cc930`).

Five further types exist only as *type arguments* of `core`/`tokio` symbols, so
they have no `hydra_exp::` name of their own:

| type | where it appears |
|---|---|
| `hydra_exp::recorder::start::{{closure}}` | `tokio::task::spawn::spawn_inner<Pin<Box<…>>>`, `tokio::runtime::task::core::{Cell, Stage, CoreStage}<…>`, `with_current<…, JoinHandle<()>>` (3 `drop_in_place` sites) |
| `hydra_exp::recorder::run::{{closure}}` | `drop_in_place<hydra_exp::recorder::run::{{closure}}>` — the future type of an `async fn run` |
| `hydra_exp::tracker::Tracker` | 12 `drop_in_place<Tracker>` (`BTreeMap`/`Ring`/`HashMap` fields) |
| `hydra_exp::row::{Pulse, Snapshot, Who, Source}` | no glue of its own — the rows are `Copy`-ish and typed only through `hydra_ledger` |

That the crate has an `async fn run` and an async block inside `start`, and that
the block is `Box::pin`ned and handed to `tokio::task::spawn`, is the whole of
the dependency evidence below.

---

## Coverage table (all 30 `hydra_exp::` symbols)

Sizes are exact extents from `nm -a` (distance to the next symbol of any kind).
Where this file previously recorded a smaller "real body" size, both are given:
the difference is cold blocks and outlined generic code that `nm -a` attributes
to the same symbol.

| symbol | addr | size | status | notes |
|---|---|---|---|---|
| `levels::label` | `0x1012ec07c` | 272 | implemented | `[u64; 101]` table at `0x10240d7c0` re-read this pass: `0,2500,…,7460000,7572500`, strictly increasing; cap literal `0x10240db06` = `"level 100 (cap)"` |
| `catalog::Entry::command` | `0x1012ec18c` | 496 | implemented | |
| `catalog::Entry::matches` | `0x1012ec37c` | 2872 | implemented | body ends at `0x1012ecdf4` (2680 B); the rest is outlined code |
| `report::hour` | `0x1012b0280` | 592 | implemented | |
| `report::last` | `0x1012b04d0` | 1256 | implemented | |
| `report::daily` | `0x1012b09b8` | 2372 | implemented | |
| `report::level` | `0x1012b12fc` | 1448 | implemented | |
| `report::range` | `0x1012b18a4` | 8644 | partial | the two "fullness" lines are still not printed — the row offsets of the `0..=99` columns are not readable; body is `0x1012b18a4..0x1012b34ec` (7240 B) |
| `report::stamp` | `0x1012b3a68` | 396 | implemented | |
| `report::today` | `0x1012b3bf4` | 428 | implemented | |
| `report::Totals::said` | `0x1012b3da0` | 1104 | implemented | |
| `report::timing` | `0x1012b41f0` | 1564 | implemented | `TIMING_GAP_LIMIT` re-read this pass: the `i64` at `0x10240cf28` is exactly `90` |
| `report::bonuses` | `0x1012b480c` | 724 | implemented | |
| `report::session` | `0x1012b4ae0` | 692 | implemented | |
| `report::snapshots` | `0x1012b4d94` | 1852 | implemented | |
| `command::parse_time` | `0x1012ca8f4` | 2060 | partial | three spellings recovered |
| `command::run` | `0x1012cb100` | 812 | partial | prefix parse full, dispatch partly |
| `command::read` | `0x1012cb42c` | 2116 | partial | jump table + windows recovered, `Parsed` enum partly |
| `command::read::{{closure}}` | `0x1012cbc70` | 204 | unknown | **not in the previous table**; 2 instantiations |
| `command::parse` | `0x1012cbe60` | 2768 | partial | word split + quoted argument recovered |
| `command::parse::{{closure}}` | `0x1012cc930` | 96 | unknown | **not in the previous table** |
| `command::answer` | `0x1012cc990` | 2300 | implemented | body `0x1012cc990..0x1012cce7c` (1260 B) + cold block |
| `tracker::Tracker::new` | `0x1012dcf60` | 380 | partial | field offsets established, labels not |
| `tracker::Tracker::look` | `0x1012dd0dc` | 1676 | partial | |
| `tracker::Tracker::seed` | `0x1012dd768` | 792 | partial | |
| `tracker::Tracker::take` | `0x1012dda80` | 1152 | unknown | **newly found this pass.** Signature shape: `fn take(&mut self, name: &str, kind: u8, label: &Option<String>) -> …` — `(x1,x2)` is a `&str`, `and w22, w26, #0xff` makes `x3` the kind byte, `ldr x24,[x4]` vs `isize::MIN` makes `x4` the `Option<String>` niche. It hashes into the `HashMap` at `Tracker+0x1e8` (`RawTable` probe over `+0x208`, `dup.8b`/`cmeq.8b` control-byte scan) and moves a 0x30-byte value out. Not implemented: `crate::tracker` has no map-taking method and no caller needs one. |
| `tracker::Tracker::heard` | `0x1012ddf00` | 100 | implemented | |
| `tracker::Tracker::knows` | `0x1012ddf64` | 192 | implemented | `ret` at `0x1012de020` |
| `recorder::names_a_snapshot` | `0x1012fb41c` | 152 | implemented | |
| `recorder::start` | `0x1012fb4b4` | 992 | partial | see the tokio section; the spawn half is now implemented as `recorder::spawn` |

30 rows: **16 implemented, 10 partial, 4 unknown**.

### `report::range`'s fullness lines — still not recoverable

The task asked for these "only if the two row offsets can be read". They cannot,
this pass either. What *is* established is unchanged: two `{:.1}` percentages,
the first over an `f64` mean of an integer column filtered by `cmp x10, #0x63`
(`< 100`), the second a count divided by a count (`ucvtf`/`fdiv` at
`0x1012b2b48`). The pulses row is 208 bytes and the reports read `+0x30`,
`+0x68`, `+0x80`, `+0x88`, `+0x90`, `+0xa0`, `+0xa8`, `+0xb0`, `+0xb8`,
`+0xc0`, `+0xc8`; the two remaining `u8` columns are somewhere in
`+0x00..+0x30`, `+0x38..+0x68`, `+0x70..+0x80`, `+0x98..+0xa0` or
`+0xc9..0xd0`, and nothing in the 7.2 KB straight-line block names them. Printing
a number we cannot back is worse than printing neither line, so this build
prints neither. The two literals stay registered in [`crate::report`]'s docs.

---

## Dependency verdict: **`tokio` — a real, missed dependency**

This corrects the previous pass's "no `tokio` use in `crates/hydra_exp/src`".
The evidence is in `recorder::start` itself (`0x1012fb4b4`), not in a type name:

```text
1012fb7e8  bl  0x1012dccf0   tokio::task::spawn::spawn
1012fb7ec  mov x19, x0       ; the JoinHandle
1012fb7f0  bl  0x101e69b90   tokio::runtime::task::state::State::drop_join_handle_fast
1012fb7f8  bl  0x101e6b85c   tokio::runtime::task::raw::RawTask::drop_join_handle_slow
```

and the monomorphised type names confirm the future comes from this crate:

```text
tokio::task::spawn::spawn_inner<Pin<Box<hydra_exp::recorder::start::{{closure}}>>>
tokio::runtime::task::core::Cell<Pin<Box<hydra_exp::recorder::start::{{closure}}>>, Arc<…Handle>>
core::ptr::drop_in_place<hydra_exp::recorder::run::{{closure}}>
```

So the original's `recorder::start` **spawns** the recording loop on the tokio
runtime and drops the handle: fire-and-forget, nothing is awaited. (The same
shape appears in `hydra_rewards::recorder::start`, `0x1014e1ea8`, which is
another crate's business but corroborates the pattern.)

**Reconnected**: `crates/hydra_exp/Cargo.toml` now has
`tokio = { workspace = true }` (workspace pin `=1.53.1`, `features = ["full"]`,
already in the root pins and pre-fetched — the build runs with `--offline`).
`recorder::spawn` is the recovered half:

```rust
pub fn spawn(sink: Box<dyn Sink + Send>, who: Who, at: i64) {
    let handle = tokio::spawn(async move { run(sink, who, at).await });
    drop(handle);
}
```

with `async fn run(..)` standing in for `hydra_exp::recorder::run::{{closure}}`.
`recorder::start` keeps its existing synchronous `&mut dyn Sink` signature — it
is the extracted core, and changing it would break the sibling-facing API — so
the observable difference is the new additive `spawn`. A test drives a
`current_thread` runtime and asserts the spawned pass actually runs and records
the one snapshot `names_a_snapshot` accepts.

| candidate | verdict | evidence |
|---|---|---|
| **`tokio`** (workspace pin `1.53.1`) | **used — reconnected** | 15 `tokio`-naming symbols over `hydra_exp` future types, plus the `bl` sites above |
| `rusqlite` (workspace pin `0.37`) | **not a direct dependency** | `drop_in_place<rusqlite::error::Error>` is instantiated inside `recorder::start` (`0x1012fa470`), but only because the crate drops the `Err` arm of `hydra_ledger::Ledger::reader()` (`0x10168e934`); no `rusqlite` type is named anywhere in this crate and no `Connection`/`Statement`/`Row` glue exists for it. Reached through `hydra_ledger`, as before. |
| `serde`, `serde_json`, `regex`, `jiff`, `toml`, `chrono` | **not used** | no symbol naming one of them references a `hydra_exp` type, and no such call target appears in the crate's `.text` |
| `hydra_ledger`, `hydra_net`, `hydra_model` | **path deps** | `drop_in_place<hydra_ledger::{reader::Reader, writer::Writer, row::Snapshot}>` in `recorder::start`; `hydra_net::eastern`; `hydra_model::…::Experience` |

---

## Function table (from the previous pass — superseded by the coverage table above)

| module | function | address | status |
|---|---|---|---|
| `levels` | `label` | `0x1012ec07c` | **full** |
| `catalog` | `Entry::command` | `0x1012ec18c` | **full** |
| `catalog` | `Entry::matches` | `0x1012ec37c` | **full** |
| `report` | `hour` | `0x1012b0280` | **full** |
| `report` | `last` | `0x1012b04d0` | **full** |
| `report` | `daily` | `0x1012b09b8` | **full** |
| `report` | `level` | `0x1012b12fc` | **full** |
| `report` | `range` | `0x1012b18a4` | partly (every line but the two "fullness" ones is recovered) |
| `report` | `Totals::said` | `0x1012b3da0` | **full** |
| `report` | `stamp` | `0x1012b3a68` | **full** |
| `report` | `today` | `0x1012b3bf4` | **full** |
| `report` | `timing` | `0x1012b41f0` | **full** |
| `report` | `bonuses` | `0x1012b480c` | **full** |
| `report` | `session` | `0x1012b4ae0` | **full** |
| `report` | `snapshots` | `0x1012b4d94` | **full** |
| `command` | `parse_time` | `0x1012ca8f4` | partly |
| `command` | `run` | `0x1012cb100` | full (prefix parse) / partly (dispatch) |
| `command` | `read` | `0x1012cb42c` | partly (jump table + windows recovered, `Parsed` enum partly) |
| `command` | `parse` | `0x1012cbe60` | partly (word split + quoted argument recovered) |
| `command` | `answer` | `0x1012cc990` | **full** |
| `tracker` | `Tracker::heard` | `0x1012ddf00` | **full** (control flow) |
| `tracker` | `Tracker::new` | `0x1012dcf60` | partly |
| `tracker` | `Tracker::look` | `0x1012dd0dc` | partly |
| `tracker` | `Tracker::seed` | `0x1012dd768` | partly |
| `tracker` | `Tracker::knows` | `0x1012ddf64` | partly (loop + map recovered) |
| `recorder` | `names_a_snapshot` | `0x1012fb41c` | **full** |
| `recorder` | `start` | `0x1012fb4b4` | partly |

---

## Recovered data

### Level table (`levels::label`)

`[u64; 101]` at `0x10240d7c0`, copied verbatim into `levels::LEVELS`:
`0, 2500, 5000, 10000, 17500, 27500, 40000, 55000, 72500, 92500, 115000, …,
7460000, 7572500`. The scan starts at index 100, walks down while
`exp < LEVELS[i]`, and prints
`level {i} ({:.1}% through)` with `(exp - LEVELS[i]) * 100.0 / (LEVELS[i+1] -
LEVELS[i])`. Index 100 prints the literal `level 100 (cap)` (`0x10240db06`,
15 B) because there is no `LEVELS[101]`.

### Pulse rows (208 bytes, stride `0xd0`)

Established offsets (all read by the reports):

| offset | field | evidence |
|---|---|---|
| `+0x30` | `rpa`: `Option<String>` | `report::bonuses` niche test `cmp x9, isize::MIN`, `<String as Display>::fmt` at `0x1012ebb44` |
| `+0x68` | `at`: `i64` | every `stamp`/`clock` caller |
| `+0x80` / `+0x84` | `until_next`: `Option<u32>` | `report::level` (tag + `<u32 as Display>`) |
| `+0x88` / `+0x8c` | `lumnis`: `Option<u32>` | `report::bonuses` |
| `+0x90` / `+0x94` | `fashlonae` (tag `1` + value `2` prints the word) | `report::bonuses` |
| `+0xa0` | `exp`: `i64` | totals, `levels::label` input |
| `+0xa8` | `axp`: `i64` | |
| `+0xb0` | `exp_gained`: `i64` | |
| `+0xb8` | `axp_gained`: `i64` | |
| `+0xc0` | `bonus`: `i64` (the single amount a brooch/instant row holds) | `report::daily`/`hour` |
| `+0xc8` | `source`: `u8` → 0 `normal`, 1 `bounty`, 2 `brooch`, 3 `instant`, 4 `manual` | names table `0x10442dae0`/`0x10240cee8` |

`Source` names, verbatim: `normal`, `bounty`, `brooch`, `instant`, `manual`
(lengths 6/6/6/7/6). `Totals` buckets: source 1 adds `exp_gained + axp_gained`,
sources 2 and 3 add `+0xc0`.

### Snapshot rows (144 bytes, stride `0x90`)

Established from `report::snapshots` (`0x1012b4d94`) and its two closures
(`0x1012bcf64` filters on the kind byte, `0x1012bcd1c` matches kind + label):

| offset | field |
|---|---|
| `+0x00` tag (`u32`, bit 0 set = some) / `+0x08` | `value` — the reading (Voln favor, suffused experience) |
| `+0x10` tag (== 1) / `+0x18` | `weekly` |
| `+0x20` tag (== 1) / `+0x28` | `total` |
| `+0x60` capacity (`isize::MIN` = `None`), `+0x68` ptr, `+0x70` len | `label`: `Option<String>` |
| `+0x80` | `at` — the unix timestamp handed to `report::stamp` |
| `+0x88` | `kind` — the byte `report::snapshots`' `which` argument is compared with |

`report::snapshots(which, snaps, today)` prints **two** lines per row: the
"as of" line and `  {} movement today: {:+}`, where the movement is
`max - min` over *today's* rows of the same kind and label. A row without a
label prints `"Resource"` (`0x10213c7c8`) for `resource` and `"resource"`
(`0x10206b390`) for `suffused`; the movement label is the five-byte literal
`"Favor"` for `favor` and the row's own label otherwise.

### Strings and formats (all quoted text is verbatim)

```
levels   "level 100 (cap)"
report   "Last hour for {}"                      "  Rate:        {} exp/hour"
         "No experience data in the last hour for {}."
         "Today's totals for {} (since midnight)" "No experience data today for {}."
         "Session report for {} (since {})"       "No session data yet for {}."
         "No pulses recorded yet for {}."         "{}: {}"
         "  Total exp: {} (axp {})"               "  To next level: {}"
         "  ETA at today's rate: ~{:.1}h ({} exp/hour)"   "  Last pulse: {}"
         "Last {} {}pulses:"                      "No {}entries found for {}."
         "{:<10} {:>12} {:>8} {:>8} {:>9} Bonuses"  -> after join: " Bonuses"
         "Time" "Total Exp" "Gained" "Asc" "Type"
         "Date" "Total" "Normal" "Asc" "Bounty" "Brooch" "Instant" "Pulses"
         "No experience data found for the last 10 days."  "* = Today"
         "Pulse timing analysis (last {} normal pulses):"
         "  Min time between pulses: {} seconds"  "  Max time between pulses: {} seconds"
         "  Average time: {:.1} seconds"          "  Median time: {} seconds"
         "  Pulses analyzed: {}"                  "  (Excluded: bounty, brooch, instant, manual, gaps >{}s)"
         "Not enough pulse data found for timing analysis."
         "Not enough consecutive pulses found for timing analysis."
         "lumnis {}x"  "rpa {}x"  "fashlonae"  "none"  " + "
         "  {}: {}"  "  Total gained: {}"  "    Normal:     {}"  "    Ascension:  {}"
         "  Of which:"  "    Bounty:     {}"  "    Brooch:     {}"  "    Instant:    {}"
         "  Pulses:       {}"  "  Current exp: {} (axp {})"
         "Period: {} to {}"  "Experience Report for {}"  "Starting Exp: {} (axp {})"
         "Ending Exp:   {} (axp {}) - {}"  "Levels Gained: {} ({} -> {})"
         "Total Gained: {} exp, {} ascension"  "  Bounty:   {} ({:.2}%)"
         "  Brooch:   {} ({:.2}%)"  "  Instant:  {} ({:.2}%)"
         "  {:<24} {:>6} pulses ({:>5.1}%)  {:>9} exp"
         "  Min interval: {} seconds"  "  Max interval: {} seconds"
         "  Avg interval: {:.1} seconds"
         "  Avg mind fullness: {:.1}%  ({:.1}% of pulses at must-rest)"
         "  Avg pool fullness: {:.1}%  ({:.1}% of pulses pool-capped)"
         "  Active Time:  {}h {}m (gaps >{}s excluded)"
         "  Rate:         {} exp/hour (active)"  "  Total Pulses: {}"
         "  Next Level:   {} exp to go (~{:.1}h at this rate)"
         "Voln Favor: {} (as of {})"  "{}: {}/50,000 weekly, {}/200,000 total (as of {})"
         "Suffused {}: {} (as of {})"  "  {} movement today: {:+}"  "No {} data captured yet."
         "{:02}:{:02}:{:02}"  "{} {}"   (stamp = "date clock")
command  "exp" prefix, "{} is not an experience report."
recorder "Voln Favor:"  "(Weekly)Suffused"  "Suffused" + ' '
catalog  the eleven report entries and the twelve help lines (see below)
```

Timing gap limit: the static `i64` at `0x10240cf28` is **90** (`0x5a`); the gap
filter keeps `diff < 91`, and `report::range` uses the *same* constant for its
`(gaps >{}s excluded)` placeholder, so both reports share
[`report::TIMING_GAP_LIMIT`].

`command` vocabulary (help table at `0x10442dc00`, 12 × `&str`):
`session`, `today`, `hour`, `daily`, `last`, `instant`, `timing`, `level`,
`favor`, `resource`, `suffused`, `report <from> <to>`, `addgain <n>`, `clean`,
`reset` plus the closing note *“Nothing is ever sent to the game: the bars the
game sends on its own are what is recorded.”*

Report catalogue (11 × 56-byte `Entry` at `0x104391108`, this crate's slice of
hydra_gui's catalogue): `Today/today`, `Hour/hour`, `Daily/daily`,
`Last pulses/last`, `Instant/instant`, `Timing/timing`, `Level/level`,
`Favor/favor`, `Resource/resource`, `Suffused/suffused`,
`Between times/report` (`kind = 1`, every other entry `kind = 0`).

### Report windows (`command::read`, from the jump-table targets)

`session` = `pulses_between(now - 86_400, MAX)`; `today` =
`pulses_between(midnight, MAX)`; `hour` = `pulses_between(now - 3_600, MAX)`;
`daily` = `pulses_between(now - 864_000, MAX)`; `last` = `last_pulses(10)`;
`instant` = `last_pulses(1_000)` + keep non-normal sources + keep ten; `timing` =
`last_pulses(1_000)`; `level` = `last_pulses(1)` (the `Option<&Pulse>` is the
`csel x24, xzr, x9, eq` null when empty) plus today's pulses for the ETA;
`report` = `pulses_between(from, to + 1)`.

---

### `report::range` in detail

The body is `0x1012b18a4 .. 0x1012b34ec` (7,240 B, all straight-line `format!`
pushes), so the line order is the address order: `Period` → `Experience Report
for` → `Starting Exp` → `Ending Exp` → `Levels Gained` → `Total Gained` →
`Bounty`/`Brooch`/`Instant` → one row per source present → `Min`/`Max`/`Avg
interval` → (two "fullness" lines) → `Active Time` → `Rate` → `Total Pulses` →
`Next Level`.

* `Ending Exp: {} (axp {}) - {}` — the third argument is `levels::label` of the
  last pulse's absorbed experience (`bl levels::label` at `0x1012b1da0` with
  `[sp+0xc0]`, the ending exp, and the result passed as a `String`).
* `Levels Gained: {} ({} -> {})` — all three placeholders are integers
  (`<u64 as Display>::fmt` at `0x1012b1ef0`), i.e. the bare level numbers and
  their difference — which is why [`levels::number`] exists next to
  [`levels::label`].
* `Active Time: {}h {}m` adds up the surviving gaps, and `Rate` is
  `total_exp * 3600 / active_seconds`; `Next Level` divides `until_next` by that
  rate.
* `UNKNOWN:` the two "Avg mind fullness" / "Avg pool fullness" lines. Their
  shape is established — an `f64` mean of a `0..=99` integer column (the closure
  filters with `cmp x10, #0x63`) plus a second percentage that divides one count
  by another (`ucvtf`/`fdiv` at `0x1012b2b48`) — but the two row offsets those
  columns live at are not, so this build prints neither line rather than a
  number it cannot back.

## `std`-only departures and cross-crate seams

| original | recovered use | what this build does |
|---|---|---|
| `hydra_ledger::row::Pulse` (208 B), `row::Snapshot`, `row::Who`, `reader::Reader`, the writer, `Ledger::prune_pulses` | every report takes `&[Pulse]`; `read`/`answer` open `Ledger::reader()` and call `Reader::{last_pulses, pulses, latest_snapshots, snapshots}`; `answer` writes through `Writer::try_record` and `Ledger::prune_pulses`; `recorder::start` walks `latest_snapshots` and writes snapshots | **reconnected** — `hydra_ledger = { path = "../hydra_ledger" }`, and [`crate::ledger::{LedgerSource, LedgerSink}`] implement the [`crate::command::Source`] / [`crate::recorder::Sink`] traits over the real `Ledger`/`Reader`/`Writer`. [`crate::row`] still declares its own copies, because those are this crate's public API and they are *not* the ledger's types — see “the `hydra_ledger` seam” below. |
| `rusqlite::error::Error` → `Display` into the reply | `read`/`answer` tag `1` replies | `hydra_exp` never named `rusqlite` itself; the text reached it through `hydra_ledger`. The `Source`/`Sink` methods return `Result<_, String>` and [`crate::ledger`] fills that text from `hydra_ledger::Error`'s `Display` (and `writer::Closed`'s, i.e. `hydra_ledger::CLOSED_TEXT`), unchanged. `rusqlite` remains `hydra_ledger`'s own pin; the ledger still runs on its std store, so a real `hydra.db` does not open yet. |
| `alloc::fmt::format` blobs | all report text | ordinary `format!` with the recovered pieces, column widths and precisions. |
| `HashMap`/`RandomState` in `Tracker` | `knows` inserts `(kind byte, Option<String>) → 0x30-byte value` into the map at `+0x1e8` | `std::collections::HashMap` with the same key/value shape. |
| `hydra_net::eastern` (path dep) | `date`, `offset`, `midnight`, `format`, `parse` | used as-is. |
| `hydra_model::…::Experience` (path dep) | `Tracker::heard` calls `apply_bar` | used as-is (see the conflict below). |

### The `hydra_ledger` seam, reconnected

`hydra_ledger` is no longer a stub: it carries the full `row`/`reader`/`writer`
reconstruction (208-byte `Pulse`, 144-byte `Snapshot`, `Who { instance,
character }`, the `Reader` `SELECT`s, the writer thread, `prune_pulses`), and it
is a path dependency of this crate. [`crate::ledger`] is the adapter; `read`,
`answer` and `recorder::start` are unchanged and still go through the
`Source`/`Sink` traits, so every sibling-facing signature and every existing test
is untouched. `LedgerSource`/`LedgerSink` are bound to one ledger directory and
one `hydra_ledger::row::Who` (instance **and** character) because that is what
every `Reader` query filters on, while [`crate::row::Who`] carries the name
alone; `LedgerSource::as_who()` builds the reports' `Who` from the same pair.

Why [`crate::row`] keeps its own copies instead of re-exporting the ledger's:
they are the public API (`report::*` takes `&[Pulse]`), and the two families of
names/types differ. The same 208-byte row, read off the reports on this side and
off the `pulses` columns on the ledger's:

| `hydra_exp` | `hydra_ledger` |
|---|---|
| `bonus` (`+0xc0`, the amount a brooch/instant row holds) | `instant` (the column name) |
| `fashlonae: Option<u8>` | `fashlonae: Option<u32>` |
| `source: crate::row::Source` | `source: hydra_ledger::row::Source` (same five bytes) |
| — (the reports read none of these) | `instance`, `character`, `mind_text`, `game_time`, `fxp`, `fxp_max`, `mind_percent` |

and for the 144-byte snapshot, `label` vs the ledger's `name`, and the `u8` kind
byte vs the ledger's typed `SnapshotKind` (`favor`/`resource`/`suffused`).
`fashlonae` is narrowed **clamping** rather than dropping, because
`report::bonuses` only asks whether the tag is `1` and the word is `2`: presence
is the meaning. The fields this crate has no source for are filled from the bound
`Who` on the way in and ignored on the way out.

Two deliberate differences from the binary, both in [`crate::ledger`]'s docs:

1. **One writer per recorded row.** The binary opens `Ledger::writer()` once and
   `try_record` only `try_send`s. The ledger's `Connection` in this workspace is
   a whole-file store, so a long-lived writer keeps the database as it was when
   its thread started and each `COMMIT` rewrites the file from that copy — it
   would resurrect rows a `prune_pulses` (its own connection) deleted. Each write
   therefore uses its own `Writer` and flushes it: `try_record` is blocking and
   durable here (`Ok(())` = on disk, and a later read sees it). With a real
   SQLite connection the long-lived writer becomes safe *and* faithful again.
2. **Closed windows.** `Reader::pulses` and `Reader::snapshots` are half-open
   (`at >= from AND at < to`), while `command::Source` documents closed
   intervals (as the recovered windows are handed in), so the adapter passes
   `to + 1`, saturating.

**Integration risk for the parent:** `hydra_ledger` is being rewritten in its own
Layer-2 worktree. The adapter is kept thin on purpose — it names only
`Ledger::{open, reader, writer, prune_pulses}`, `Reader::{last_pulses, pulses,
latest_snapshots, snapshots}`, `Writer::{try_record, flush}`,
`hydra_ledger::CLOSED_TEXT` and `row::{Pulse, Snapshot, Row, Source,
SnapshotKind, Who}` (plus whatever `Error`'s `Display` says). If that crate's
`Reader` returns different row types or renames those methods, `src/ledger.rs`
must be re-pointed, and it is the only file in this crate that would need it.
Note also that the ledger keys pulses *and* snapshots by (instance, character),
so a caller must know the instance; the reports only ever needed the name, which
is why the adapter takes the pair at construction.

**Corrected in the audit pass:** this paragraph used to claim there was no
`tokio` use either. That was wrong — see [Dependency
verdict](#dependency-verdict-tokio--a-real-missed-dependency) above: `tokio` is
a real dependency of this crate and has been reconnected. `rusqlite` is still
reached only through `hydra_ledger`'s `Display` text and is never named here;
`serde`, `regex`, `toml`, `jiff` and `chrono` appear nowhere.

### Evidence conflicts / things that could not be established

1. **`Experience::apply_bar` signature.** The binary's `apply_bar`
   (`0x101bc3900`) takes `(&mut self, reading)` where the reading's first field is
   a `String` (`+0x08` pointer, `+0x10` length) and immediately compares nine
   bytes with `mindState`. The already-committed `hydra_model` reconstruction has
   `apply_bar(&mut self, current: u64, maximum: u64)`. `hydra_model` is out of
   bounds for this task, so `Tracker::heard` forwards the reading's
   `current`/`maximum` and keeps the recovered return rule
   (`changed && text == "mindState"`); the mismatch is recorded at the call site.
2. **Literal text minted from C strings.** Three messages lose a leading
   character because the format blob starts with the C string's first byte used
   as a piece length: `"No experience data in the last hour for {}."` (from
   `"(No experience …"`, the `(` **is** the length byte 40) and the
   `"total exp"/"gained"` header words are separate pooled literals. The output
   above is what the binary prints, not a transcription slip.
3. **`report::bonuses` prints the raw column names** — `lumnis 2x`, `rpa 1.25x`
   (the `rpa` value is a `String`!), `fashlonae` — not prettified words, and it
   joins them with `" + "`, so a row's bonus column reads
   `lumnis 2x + rpa 1.25x + fashlonae`.
4. **`report::timing`'s "Pulses analyzed"** prints the number of **gaps**, not
   pulses (`sp+0x68` is the gap vector length), and the median is
   `gaps[len / 2]` (the upper middle element, no averaging). Both are kept.
5. **`report::snapshots` takes three slices, not a window.** The kind filter and
   the "movement today" both come from the snapshot rows themselves: the third
   argument is *today's* snapshots (`report::snapshots(which, &latest,
   &today)`). `command::read` therefore fetches `latest_snapshots()` **and**
   `snapshots(which, midnight, i64::MAX)`.
6. **`command`'s `Parsed` enum.** The discriminant mapping is only partly
   established: `read`'s ten-entry jump table at `0x10240cff0` gives its windows,
   `answer`'s compare chain gives `0xa`…`0xd`, but which of `0`…`9` is which
   command is read off the jump targets, not off the `parse` body (which is a
   chain of `str` compares). `parse` therefore maps command words to variants
   directly; only the *set* of commands is certain (it is the help table).
7. **`command::answer` needs `&mut`.** Its four arms write: `addgain` builds a
   `source = manual` row on top of the last absorbed figures and calls
   `Writer::try_record`, `clean`/`confirm_reset` call `Ledger::prune_pulses(0/1)`.
   `read` takes `&dyn Source`, so a caller that wants the mutating commands must
   go through `answer` (as the binary does) — `read` only prints their usage line.
8. **`Tracker::{new,look,seed,knows}`** — field *offsets* are established
   (`+0x0`, `+0x180`, `+0x198`, `+0x1a8`, `+0x1b8`, `+0x1d0`, `+0x1e8`,
   `+0x208`/`+0x210` RandomState keys, `+0x218`, `+0x220`/`+0x224`), the snapshot
   labels the three `seed` calls use are not.
9. **`recorder::start`** — the write path is not isolatable (the 45 KB span holds
   the `INSERT` statements of `attacks`/`hits`/`flares`/`creatures`, i.e.
   hydra_combat_log's). The recovered part is the `latest_snapshots` walk plus
   the `Option<String>`/kind-byte extraction; the recording filter reuses
   `names_a_snapshot`.

---

## Layout of this crate

```
src/lib.rs        modules: catalog, command, ledger, levels, recorder, report, row, tracker
src/levels.rs     LEVELS table + label()
src/row.rs        Pulse / Snapshot / Who / Source  (this crate's view of the ledger's rows)
src/report.rs     Totals + the twelve reports
src/catalog.rs    Entry, command(), matches(), REPORTS
src/command.rs    Source trait, Parsed, Reply, HELP, parse/parse_time/run/read/answer
src/tracker.rs    Tracker, Bar, Watch, Known
src/recorder.rs   names_a_snapshot, Sink trait, start, spawn / run (tokio)
src/ledger.rs     LedgerSource / LedgerSink — the real hydra_ledger behind the two traits
```

`Edit`d by hand against the disassembly; the tests assert literal recovered
values (level thresholds and percentages, the ten `said` lines, the five-column
`last` table with its widths, the 90 s timing limit, the Eastern-date grouping
of `daily`, `parse_time`'s three spellings, and `names_a_snapshot`'s three
tests). The ten `ledger` tests round-trip every mapped field through a throwaway
ledger directory under `std::env::temp_dir()` (removed by each test), and cover
the oldest-first ordering, the closed windows, the instance/character filter,
`prune_pulses`, `command::answer`'s manual gain, `recorder::start` over a real
sink, and the `Display` text of a failed connection.

---

## What changed in audit-core (Phase 2)

1. **`tokio` reconnected.** `crates/hydra_exp/Cargo.toml` gained
   `tokio = { workspace = true }`; `recorder::spawn` + `async fn run` reproduce
   the binary's `tokio::task::spawn` + immediate `JoinHandle` drop. This was the
   only dependency the previous pass got wrong: it recorded "there is nothing to
   reconnect" and the binary disagrees. `recorder::start` keeps its signature.
2. **Symbol inventory redone from `nm -a`, not just the global list.** One
   previously-missed function (`tracker::Tracker::take`, 1152 B) and two closure
   symbols (`command::read::{{closure}}` ×2, `command::parse::{{closure}}`) are
   now in the inventory; the coverage table lists all 30.
3. **Four symbols re-measured to exact `nm -a` extents** — `report::range`
   (8644, not 7240), `command::answer` (2300, not 1260), `catalog::Entry::matches`
   (2872, not 2680) and `tracker::Tracker::knows` (192). The smaller numbers were
   the real body up to the first `ret`; the difference is cold blocks and
   outlined generic code that `nm -a` attributes to the same symbol.
4. **Two data tables re-read** from `__const` and confirmed verbatim in this
   crate: the 101-entry `LEVELS` table at `0x10240d7c0` (`0, 2500, 5000, …,
   7460000, 7572500`, strictly increasing) and the timing gap limit `90` at
   `0x10240cf28`. Neither needed a change.
5. **`report::range`'s fullness lines were re-attempted and are still out of
   reach** — see [that section](#reportranges-fullness-lines--still-not-recoverable).
   Nothing was invented to close them.
6. **Tests 46 → 47**: one new test
   (`recorder::tests::spawn_hands_the_pass_to_the_runtime_and_drops_the_handle`)
   which drives a `current_thread` tokio runtime and asserts the spawned pass
   runs and records exactly the snapshot `names_a_snapshot` accepts.

Nothing in this crate's public API was removed or renamed; `recorder::spawn`
and `recorder::run` are additive, and `recorder::start` is unchanged.
