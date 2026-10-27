# `hydra_rewards` — recovery notes

Source: the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled symbols in `symbols_demangled.txt`).  Every address below
is an **unslid virtual address**.  The crate is Hydra's daily-login-reward
tracker: it watches the lines a session says, keeps the reward, replays old
logs, prints the `rewards` reports and owns `rewards.toml`.

Status: `cargo check -p hydra_rewards` and `cargo test -p hydra_rewards` pass
(**60 tests**), and `grep -rn 'todo!()\|unimplemented!()' crates/hydra_rewards/src`
is empty.

Legend:

| tag | meaning |
|---|---|
| **ESTABLISHED** | in the instruction stream, in a string literal, or in a field offset — re-derivable from the address quoted. |
| **INFERRED** | follows from the code shape or from a recovered sibling, but the original's name/type is not in the binary. |
| **UNKNOWN** | not recoverable; the safest behaviour was chosen and is marked at the site. |

---

## 1. Tooling

Read-only, all from the worktree; nothing was written outside it.  Scratch
drivers lived in `$TMPDIR/rr/` (not committed):

* `python3 ../recon.py {syms,sizes,disas,bytes,str,findstr}` — the shared
  helper.
* `dis.py` — disassemble a range with `adrp`/`add` folded, every literal
  decoded (as `&str`, as a length-prefixed format blob, or as raw bytes) and
  every `bl`/`b` resolved to a symbol.  This is how the format blobs in §4
  were read.
* `ends.py` — list the `ret` sites in a window, needed because **a symbol ends
  where the next *named* symbol begins**, and several `hydra_rewards` symbols
  are followed by tens of kilobytes of unnamed outlined code from other crates
  (`replay::rewards` reports 17,620 bytes but ends at `0x1014ca910`;
  `catalog::Entry::matches` reports 68,004 bytes but ends at `0x1014d1f78`;
  `recorder::start` reports 49,808 bytes and is mostly `rusqlite`/`tokio`).
* `calls.py`, `lits.py`, `xrefs.py`, `tab.py`, `rec.py` — call histograms,
  literal collection, call-site cross references, and `__const` table
  decoding (this is how the three catalogue rows at `0x104391370` were found).

`objdump --disassemble-symbols` does not work on this file; `/usr/bin/objdump
-d --start-address=… --stop-address=… --no-show-raw-insn` does.

### Format blobs

Every `format!` in this build passes a compact blob to
`alloc::fmt::format::format_inner` (`0x101eb6920`): `<len><bytes>` for a
literal piece, `0xC0` for `{}`, and for a piece with a spec a byte `c` whose
low bits say which payload words follow (`bit0` a `u32` spec word, `bit1` a
`u16` width, `bit2` a `u16` precision, `bit3` an argument index), terminated by
`0x00`.  The spec word is the `Formatter`'s option block, so `0x6900_0020` +
width 2 is `{:02}` and `0x0800_0020` + width 14 is `{:<14}`.  Every message in
§4 was decoded this way.

---

## 2. Function table

`full` = signature, control flow, constants and every message recovered;
`partly` = the contract, the literals and the call graph recovered, with a
dependency replaced by a seam; `unknown` = not recoverable.

| module | function | address | status |
|---|---|---|---|
| `catalog` | `Entry::command` | `0x1014d1470` | **full** |
| `catalog` | `Entry::matches` | `0x1014d1504` | **full** |
| `parse` | `reward` | `0x1014ee138` | **full** (body ends `0x1014ee30c`; one dead branch, see §6.1) |
| `parse` | `number_before` (private) | `0x1014edc00` | partly (first-vs-last occurrence unknown, §6.2) |
| `watcher` | `given_before` | `0x1014efde4` | **full** |
| `watcher` | `Heard::at` | `0x1014efe2c` | **full** |
| `watcher` | `Watcher::hear` | `0x1014eff64` | **full** (the frame argument is a `Chunk`, §6.3) |
| `watcher` | `Watcher::hear_line` | `0x1014f006c` | **full** (body ends `0x1014f0270`; one impossible test, §6.4) |
| `report` | `own` | `0x1014ce818` | **full** |
| `report` | `named` (private) | `0x1014cf05c` | **full** |
| `report` | `streak` (private) | `0x1014d0110` | **full** |
| `report` | `run` | `0x1014cecec` | **full** |
| `report` | `answer` | `0x1014cf570` | **full** for the dispatch, the messages and the ledger calls; the word list is partly inferred (§6.5) |
| `report` | `store` | `0x1014cf140` | **full** for the wording and the matching; the row layouts are partly (§6.6) |
| `report` | `everyone` | `0x1014d0260` | **partly** — the header and the pieces are recovered, the row layout is not (§6.7) |
| `store` | `kept` | `0x1014d1204` | **partly** — the date arithmetic and the row are recovered, the page enum is not (§6.8) |
| `replay` | `rewards` | `0x1014ca344` | **partly** — the search, the frame replay and the element type are recovered; the text is replayed line by line instead of through `hydra_protocol` (§6.9) |
| `recorder` | `start` | `0x1014e1ea8` | **partly** — a `tokio` task and a `rusqlite` write in the original; the loop and the grammar are recovered, the write is the `Sink` seam (§6.10) |
| `schedule` | `Settings::read` | `0x1014c857c` | **full** |
| `schedule` | `Settings::parse` | `0x1014c87c8` | **full** for the keys and the `time` grammar; the `toml`/`serde` layer is replaced (§6.11) |
| `schedule` | `Settings::write` | `0x1014c92b8` | **full** |
| `schedule` | `Settings::to_toml` | `0x1014c946c` | **full** (the error arm answers an empty string, §6.12) |
| `schedule` | `Settings::today` | `0x1014c8d08` | **partly** — the configured time resolved to an instant, but through `jiff`'s zone database in the original (§6.13) |
| `schedule` | `Settings::today_now` | `0x1014c9648` | **partly** — `jiff::now::sys::system_time` replaced by `SystemTime::now` |
| `schedule` | `Character::skipped` | `0x1014c9c24` | **full** |
| `schedule` | `next` (private, referenced by `hydra_table`) | `0x1010011cb08` window | **partly** (§6.14) |

Functions the binary has that the stub list did not: `report::named`,
`report::streak`, `parse::number_before`, `schedule::next`,
`hydra_rewards::claim::*` (not in the stub crate at all — the store-claim
path lives in `hydra_table::reward_claim` and `hydra_net::store`), and the
closures `recorder::{start,run}::{{closure}}`.  The three private helpers are
implemented and documented here because the public stubs call them.

---

## 3. Recovered data

### 3.1 The catalogue (`0x104391370`, stride `0x30`)

Three rows of three `&str`s — **no `kind` byte** (unlike `hydra_exp`'s
0x38-byte rows): the row after "Store SimuCoins" starts exactly 0x30 later.

| name | sub | help |
|---|---|---|
| `Login reward` | `""` | `Whether today's login reward is in, and the last ten` |
| `Login rewards, everyone` | `all` | `Every character's latest login reward, and whether today's is in` |
| `Store SimuCoins` | `store` | `Each account's monthly store SimuCoins: the last claimed, the balance, and when the next is` |

`Entry::command` builds `rewards` or `rewards {sub}` (`rewards {}` blob at
`0x103e6b922`); `Entry::matches` folds `format!("{} {} {}", name, sub, help)`
to lower case once and requires every whitespace word of the query to be
`contains`-ed in it (two `to_lowercase` calls — `0x1014d1564` and the word
fold — plus `memchr`/`StrSearcher`; the same shape as `hydra_exp::catalog`).

### 3.2 `Heard` — the row (0xc0 bytes)

| offset | field | evidence |
|---|---|---|
| `+0x00`/`+0x08` | `day: Option<u32>` | `report::streak` reads the tag (`0x1014d012c`) and the number, then prints `", day {}"` |
| `+0x10`/`+0x18` | `bridges: Option<u32>` | `0x1014d0164`, `0x1014d018c` → `", {} bridge{s}"` |
| `+0x20` | `instance: String` | cloned from `who+0x00` (`0x1014efe50`); the DB column is `instance` |
| `+0x38` | `character: String` | cloned from `who+0x18` (`0x1014efe60`); `report::own` compares it with the character (`0x1014ce858`) |
| `+0x50` | `day_text: String` | `eastern::format(eastern::date(stamp.game_time))` (`0x1014efe88`, `0x1014efe9c`) |
| `+0x68` | `reward: String` | the `{}` of `own`'s `…Eastern: {}{}.` (`0x1014ceaf0`); empty ⇒ "given before Hydra saw it" (`0x1014cea3c`) |
| `+0x80` | `line: String` | copied from the state (`0x1014efea0`) |
| `+0x98` | `said: String` | copied from the state (`0x1014efec0`); never read by a report — **UNKNOWN** which column it is |
| `+0xb0` | `stamp: Stamp` | copied verbatim (`0x1014efed8`) |

Column list, from `CREATE TABLE login_rewards` (`0x1039857f3`) and the
`INSERT` (`0x103985e59`): `at, game_time, instance, character, day, streak,
bridges, reward, thanks, line`.

### 3.3 The watcher's memory (`Watcher+0x00`, 0x38 bytes)

`{ day: Option<u32>, bridges: Option<u32>, line: String }` — written by the
streak path (`0x1014f02d8` … `0x1014f02e4`), reset to the tag `2` by a reward
line (`0x1014f0164`).  The `Vec<u8>` line buffer is at `+0x38` (cap `+0x38`,
ptr `+0x40`, len `+0x48`).

### 3.4 `Who` and `Stamp`

`Who` = two `String`s, `instance` at `+0x00`, `character` at `+0x18` (0x30
bytes).  `Stamp` = 16 bytes; `+0x08` is the unix second `eastern::date` is
called on (`0x1014efe88`), the `+0x00` word is **UNKNOWN** (it is only ever
copied).  `hydra_ledger`'s `row::Who`/`row::Stamp` are not in the symbol
table, so both are declared here.

### 3.5 `rewards.toml`

`<data dir>/rewards.toml`; keys `time`, `skip`, `store` (the serde `Raw`
visitor's field names at `0x10241aee8` … `0x10241aef5`, `struct Raw` at
`0x10241af27`):

```toml
time = "19:15"
skip = ["Aelith", "GS4:Bob"]
store = true
```

`time` is rendered `{:02}:{:02}` from minutes-from-midnight / 60 and % 60
(`0x1014c94a0` divides with the `0x88888889` magic).  `skip` is the list of
characters not to claim for, matched case-insensitively against the character
name or `GAME:Name` (`0x1014c9cd8`).  `store` turns the store claim on.

Writing an **idle** schedule (`time` is `None` and `store` is false) removes
the file instead of writing it (`0x1014c92f8` … `0x1014c9364`).

### 3.6 Line grammars

* `You have earned the following reward: ` — 38 bytes, compared as five
  8-byte chunks (`0x1014ee168` … `0x1014ee1c8`).
* `Thank you for logging into ` — 27 bytes (`0x10241b722`), then a number
  before `" consecutive day"` (16 bytes, `0x10241b73d`) and a number before
  `" login bridge"` (13 bytes, `0x10241b74d`).
* `replay::rewards` searches any log for `earned the following reward`
  (27 bytes, built from four immediates at `0x1014ca380`).

---

## 4. Every message, verbatim

| text | blob | built by |
|---|---|---|
| `rewards` | `0x1014d14c4`/`0x1014d14d0` (immediates) | `Entry::command` |
| `rewards {}` | `0x103e6b922` | `Entry::command` |
| `{} {} {}` | `0x103e2db91` | `Entry::matches` |
| `{}: today's login reward ({}) is not in yet.` | `0x103e6b790` | `report::own` |
| `{}: today's login reward ({}) is in: {}.` | `0x103e6b7bd` | `report::own` |
| `{}: today's login reward ({}) is in, at {} Eastern: {}{}.` | `0x103e6b7e6` | `report::own` |
| `{} is not a rewards report.` | `0x103e6b81f` | `report::run` |
| `` `{}` is not a rewards word; rewards help lists them. `` | `0x103e6b83b` | `report::answer` |
| `, {} bridge{}` | `0x103e6b871` | `report::streak` |
| `, day {}` | `0x103e6b87f` | `report::streak` |
| `Login rewards, today is {}:` | `0x103e6b887` | `report::everyone` |
| `{}  {}` | `0x103e2b3fb` | `report::own`'s list, `report::named` |
| `{}{}` | `0x103e2252a` | `report::named` |
| `{:02}:{:02}` | `0x10241af01` | `report::clock`, `Settings::to_toml` |
| `{:<14} {}  balance {}  last claimed {}  next {}` | `0x10241b0ab` | `report::store` |
| `Store SimuCoins by account:` | `0x10241b08f` | `report::store` |
| `No store visit is on file.` | `0x10241b075` | `report::store` |
| `No login reward is on file.` | `0x10241b046` | `report::own` |
| `given before Hydra saw it` | `0x104437120` (25) | `report::{named, own}` |
| `never seen` | `0x10241b0dc` (10) | `report::store` |
| `not told` | `0x1014cf30c` (8, immediates) | `report::store` |
| `-` | `0x10241b0aa` (1) | `report::store`'s missing balance |
| `s` | `0x10241b26e` (1) | `report::streak`'s plural |
| `today: in` / `today: not yet` | `0x10241b27d` / `0x10241b26f` | `report::everyone` |
| `` `{}` is not a time of day: say it as 19:05. `` | `0x103e6b629` | `Settings::parse` |
| `{}: {}` | `0x103e21615` | `Settings::read`/`write` errors |
| `rewards.toml` | `0x10241aef5` (12) | `Settings::{read,write}` |
| `Login rewards are claimed at ` / `Login rewards are not claimed.` | `0x10240b130` / `0x10240b160` | the GUI's settings page (not this crate) |

The chat help (`0x10241b0ec` … `0x10241e2xx`) is `report::HELP`, padded to a
17-character word column, with the footer `The game's day turns at midnight
Eastern. Nothing is ever sent to the game: the reward is read off the lines it
says at login.`  The shell's help index lists it as `rewards help` / `the daily
login reward` (`0x102408d52`).

---

## 5. `std`-only departures

| original | what it did | this build |
|---|---|---|
| `serde` + `toml` | `settings::Raw`'s derive, `toml::ser::to_string`, `toml::de::Error` | [`crate::toml`] — ~250 lines that read and write the three keys, strings, booleans and string arrays, keeping unknown keys and the file's key order. The error messages differ from `serde`'s (`invalid type: {}`, `unknown field \`{}\`` at `0x103e6b8a7`… are not reproduced). |
| `jiff` | `Settings::today` resolved the configured wall-clock time through the zone database (including the ambiguous-hour retries at `0x1014c8e0c`), `today_now` took the system clock | [`hydra_net::eastern`] — the game's clock, which is the clock every report quotes: `midnight(date(now)) + minutes * 60`. Ambiguous and non-existent local times are not modelled. |
| `tokio` | `recorder::start` was a spawned task driven by the session's frame stream | [`crate::recorder::start`] is a plain loop over [`Chunk`]s; nothing is spawned and no channel exists. |
| `rusqlite` / `hydra_ledger` | `Reader::{rewards, latest_store_visits, latest_store_claims}`, `Writer::record`, `Ledger::reader` | [`crate::report::Source`] and [`crate::recorder::Sink`] traits — the same seam `hydra_exp` uses. The error type is `String` where the original had `rusqlite::Error`, and `report::{answer,run}` answer `Result<Vec<String>, String>`. |
| `hydra_protocol` | `Watcher::hear` took `&hydra_protocol::frame::vocabulary::Frame` (a 0x118-byte tagged frame: a `&[u8]` at `+0x08`/`+0x10` and a "line ends here" byte at `+0x110`), and `replay::rewards` ran the text through `Parser::push_bytes`/`finish` | [`crate::watcher::Chunk`] carries exactly those two facts, and `replay::rewards` splits the text on `\n`. Recovered `hydra_protocol::frame::Frame` is a *simplified* model of the same enum (its own `NOTES.md` says so), so a `plain()`-based adapter would lose the offset-0x110 flag. |
| `hydra_session::store::save_bytes` | used, as-is | used, as-is (path dependency) — `Settings::write` keeps the durable write and the `remove_file` fallback. |

Path dependencies added to `crates/hydra_rewards/Cargo.toml`: `hydra_net`
(`eastern`, `store::page`) and `hydra_session` (`store::save_bytes`).  Both are
the crates the original called at the same sites (`0x101c586c0`,
`0x101c5cd1c`, `0x101aa16dc`).

---

## 6. Evidence conflicts and things that could not be established

1. **`parse::reward`'s `'!'` scan.**  `0x1014ee200` … `0x1014ee2a4` is a loop
   over `memchr(b'!')` that ends in
   `csel x1, x20, x8, eq` — i.e. `if let Some(i) = find('!') { &rest[..i] } else
   { rest }`.  But every entry into that `csel` needs `x22 > x20` (`b.ls` at
   `0x1014ee298`, `b.hs` at `0x1014ee288`), and `x22` is `idx + 1` with
   `idx < len`, so the branch is **dead** and the emitted code always keeps the
   whole remainder.  The reconstruction keeps the whole remainder (so a reward
   that ends in `!` keeps it, and `own`'s trailing `.` doubles the
   punctuation).  The alternative reading — "cut at the last bang" — is what
   the source almost certainly said; the two can be told apart only by a test
   the binary no longer has.
2. **`parse::number_before`'s occurrence.**  The search is a
   `StrSearcher` walk over the haystack and the digits before the match are
   folded into an integer, but which match wins (first or last) is not visible
   in the arithmetic.  `rfind` is used; every recovered line has the phrase
   once.
3. **`Watcher::hear`'s argument.**  The frame struct is not reconstructed
   (§5), so [`Chunk`] is a two-field stand-in whose field offsets (`&[u8]` at
   `+0x08`, flag at `+0x110`) are the recovered ones.
4. **`Watcher::hear_line`'s day test.**  The emitted `cmp x21, #0x2; b.ne`
   (`0x1014f0120`) tests the first word of `Option<u32>`, whose tag is 0 or 1,
   so it can never be equal; the only reading that keeps the two grammars
   apart is "a day count was found", which is implemented.  The `strict`
   alternative (never a streak line) would break the `Thank you for logging
   into` grammar.
5. **`report::answer`'s word list.**  The dispatch is a numbered `match`:
   `isize::MIN` = no word, and the arms `3`, `4`, `5`.  Only `5` is pinned —
   `0x1014cfa3c` compares the word with `stor` + `e`.  `3`/`4` are read as
   `all`/`everyone` because those are the catalogue's sub-command and name;
   the reconstructed `answer` matches the *words*, not the numbers, so a
   fourth alias would be accepted here and refused by the original.
6. **`report::store`'s row layouts.**  The two 0x80-byte slices' field offsets
   are recovered (`Store`: next `+0x00`/`+0x08`, account `+0x10`, character
   `+0x28`, balance `+0x60`/`+0x68`; `Visit`: account `+0x00`/`+0x08`,
   character `+0x18`/`+0x20`, instant `+0x38`), but the types behind them are
   `hydra_ledger::row::StoreClaim`/`StoreVisit`, which are not in the symbol
   table.  The first slice's `+0x28` and `+0x70` words are unexplained.
7. **`report::everyone`'s row.**  Only `Login rewards, today is {}:`, `{}` and
   `{}{}` are in the recovered range; the row is built here as
   `character  named  today: in|not yet` from the two `today:` literals that
   sit next to the header.  A different column order is entirely possible.
8. **`store::kept`'s page enum.**  The third argument holds a flag at `+0x48`
   and two `&str` pairs (`+0x08`/`+0x10`, `+0x38`/`+0x40`) — a two-variant
   enum whose meaning is not recoverable.  [`Page`] keeps the text and the
   flag.  The `2` written at the row's `+0x28` is unexplained and is dropped.
9. **`replay::rewards`' element type and conversion.**  The internal `Vec` is
   pushed with a `0x68` stride (the [`Said`](crate::watcher::Said] value),
   while the caller at `0x1001396bc` walks the answer with a `0xc0` stride
   (the [`Heard`](crate::watcher::Heard) row).  The conversion between the two
   is not in the recovered bodies, so this crate returns the `Said` values and
   offers [`replay::rows`] to make the rows.  The original's second argument
   (kept in the iterator state at `0x1014ca768`) is not identified and has no
   counterpart here.
10. **`recorder::start`.**  49,808 bytes of `tokio` task machinery, a
    `rusqlite` writer and the session's frame source.  The recovered parts are
    the watcher loop, the grammar and the `login_rewards` `INSERT`; the write
    itself is the `Sink` seam, and the original's de-duplication (the
    `INSERT … WHERE login_rewards.reward = '' AND excluded.reward <> ''`
    upgrade at `0x103986049`) is not modelled — every hearing is written.
11. **`Settings::parse`'s `time` grammar.**  The `:` split, the sign handling
    and the minutes bound of 60 are recovered; the bound `cmp x10, #0xf` at
    `0x1014c8bd0` (fifteen of *something*) is not explained — this
    reconstruction accepts two-digit hours `0..=23` and minutes `0..=59`.
    The GUI's help says the time is "hours from midnight, such as 19.25",
    which is a different spelling of the same setting and is not accepted by
    the TOML reader.
12. **`Settings::to_toml`'s return type.**  The emitted body answers a
    `Result`-shaped value whose error arm writes an empty `String`
    (`0x1014c9538`), and `Settings::write` uses the text without ever looking
    at an error.  This reconstruction returns `String` and cannot fail.
13. **`Settings::today`.**  The original resolved the time through `jiff`'s
    zone database with retries for ambiguous and skipped local times
    (`0x1014c8e0c`, `0x1014c8f04`).  This reconstruction resolves it on the
    game's Eastern clock at `midnight + minutes * 60`, which is the clock the
    reports quote; the difference only shows for settings whose time falls in
    a DST gap.
14. **`schedule::next`.**  The symbol survives (`0x1010011cb08`) but its body
    is a `jiff` comparison this reconstruction cannot read faithfully; "today's
    instant when it is still to come, tomorrow's otherwise" is the reading the
    name, the `hydra_table::reward_claim` call site and the settings help
    support.
15. **The `Said` variant order.**  The binary tags "nothing" as `2`; the
    payload variants' order and names are not recoverable.  `Said::Nothing`
    is declared last so the tag matches, and the three field sets are the
    recovered ones.
