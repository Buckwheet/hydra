# `hydra_exp` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled symbols in `symbols_demangled.txt`). Every address below is
an **unslid virtual address**. The crate is Hydra's experience tracker: it parses
what the game says about experience, keeps per-creature/per-hunt rows, computes
levels and rates, and prints the `exp …` reports.

Status: `cargo check -p hydra_exp` and `cargo test -p hydra_exp` pass (**30
tests**); `grep -rn 'todo!()\|unimplemented!()' crates/hydra_exp/src` is empty.

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

## Function table

| module | function | address | status |
|---|---|---|---|
| `levels` | `label` | `0x1012ec07c` | **full** |
| `catalog` | `Entry::command` | `0x1012ec18c` | **full** |
| `catalog` | `Entry::matches` | `0x1012ec37c` | **full** |
| `report` | `hour` | `0x1012b0280` | **full** |
| `report` | `last` | `0x1012b04d0` | **full** |
| `report` | `daily` | `0x1012b09b8` | **full** |
| `report` | `level` | `0x1012b12fc` | **full** |
| `report` | `range` | `0x1012b18a4` | partly (formats + aggregates; two "fullness" lines and one placeholder UNKNOWN) |
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
filter keeps `diff < 91`.

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

## `std`-only departures and cross-crate seams

| original | recovered use | what this build does |
|---|---|---|
| `hydra_ledger::row::Pulse` (208 B), `row::Who`, `reader::Reader`, the writer | every report takes `&[Pulse]`; `read`/`answer` open `Ledger::reader()` and call `Reader::{last_pulses, pulses_between, latest_snapshots, snapshots}` | `hydra_ledger` is still a stub in this workspace (`row.rs` only declares `Stamp`), and only files under `crates/hydra_exp/` may change. The row shapes are therefore declared in [`crate::row`] at their recovered offsets, and the reader/writer half is the [`crate::command::Source`] / [`crate::recorder::Sink`] trait pair. |
| `rusqlite::error::Error` → `Display` into the reply | `read`/`answer` tag `1` replies | the `Source`/`Sink` methods return `Result<_, String>`; the error text is passed through unchanged. |
| `alloc::fmt::format` blobs | all report text | ordinary `format!` with the recovered pieces, column widths and precisions. |
| `HashMap`/`RandomState` in `Tracker` | `knows` inserts `(kind byte, Option<String>) → 0x30-byte value` into the map at `+0x1e8` | `std::collections::HashMap` with the same key/value shape. |
| `hydra_net::eastern` (path dep) | `date`, `offset`, `midnight`, `format`, `parse` | used as-is. |
| `hydra_model::…::Experience` (path dep) | `Tracker::heard` calls `apply_bar` | used as-is (see the conflict below). |

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
src/lib.rs        modules: catalog, command, levels, recorder, report, row, tracker
src/levels.rs     LEVELS table + label()
src/row.rs        Pulse / Snapshot / Who / Source  (stand-ins for hydra_ledger::row)
src/report.rs     Totals + the twelve reports
src/catalog.rs    Entry, command(), matches(), REPORTS
src/command.rs    Source trait, Parsed, Reply, HELP, parse/parse_time/run/read/answer
src/tracker.rs    Tracker, Bar, Watch, Known
src/recorder.rs   names_a_snapshot, Sink trait, start
```

`Edit`d by hand against the disassembly; the tests assert literal recovered
values (level thresholds and percentages, the ten `said` lines, the five-column
`last` table with its widths, the 90 s timing limit, the Eastern-date grouping
of `daily`, `parse_time`'s three spellings, and `names_a_snapshot`'s three
tests).
