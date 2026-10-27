# `hydra_ledger` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled globals in `symbols_demangled.txt`, everything else from
`nm -an`). Every address below is an **unslid virtual address**. The crate is
Hydra's SQLite-backed ledger: pulses, snapshots, the `names` interning table, the
`login_rewards`/`store_visits` tables, the import bookkeeping, and a writer
thread that owns the database.

Status: `cargo check -p hydra_ledger` and `cargo test -p hydra_ledger` pass
(**40 tests**); `grep -rn 'todo!()\|unimplemented!()' crates/hydra_ledger/src` is
empty.

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow, literals, constants and row layout read out of the instruction stream; covered by tests with real values. |
| **partly** | entry contract, literals, SQL and the called helpers are recovered, but one structural detail (a batching rule, an offset) is not. |
| **unknown** | only the call shape is visible; the safest behaviour is implemented and marked `// UNKNOWN:` at the site. |

---

## 0. Tooling

Read-only, all scratch files outside the repository (`$TMPDIR/ledger_recon/`):

* `python3 ../recon.py {syms,sizes,disas,bytes,str,findstr}` — the workspace helper;
* `dump.py` — read a `(ptr, len)` pair out of `__DATA` and print the string it
  points at (this is how the three migration batches and the three column lists
  were read);
* `blobs.py` — decode a packed `format!` blob (`<len><bytes>`, `0xC0` = `{}`,
  `0x80 <u16 len>` = a long literal), used for the `SELECT {}` templates and the
  two error messages;
* `lits.py` — walk a function's `adrp`/`add` pairs to list its literal pool
  targets;
* `xrefs.py` — scan `__text` for every `bl` to a given address (how
  `Stamp::now`'s callers, `migrate`'s callers and `hunt_overlapping`'s caller
  were found).

Fallback disassembly: `/usr/bin/objdump -d --start-address=… --stop-address=…
--no-show-raw-insn hydra` (`--disassemble-symbols` does not work on this file).

### Report sizes are unreliable

A symbol ends where the next *named* symbol begins, and several
`hydra_ledger` symbols are followed by thousands of unnamed outlined functions
belonging to other crates. `name_id` reports 18,224 bytes but its body is 352;
`row::Stamp::now` reports 5,412 bytes but its body is 116. Every size quoted
below was checked against the function's own `ret`.

### Two ABI facts the row layouts depend on

1. A `String`/`Vec` in this build is `{ cap @0, ptr @8, len @0x10 }` — the
   `drop_in_place` shims pass `[obj+0]` as `__rust_dealloc`'s *size* and
   `[obj+8]` as its pointer — and `Option<String>` puts `isize::MIN` in the
   capacity word for `None`.
2. `Option<u32>` is `{ tag: u32, value: u32 }` (8 bytes) and `Option<i64>` is
   `{ tag: u64, value: i64 }` (16 bytes): no niche in either.

`Result<T, rusqlite::Error>` uses the sentinel `0x8000_0000_0000_0016` for `Ok`
in the first word (`cmp x8, #0x16; movk x8, #0x8000, lsl #48` before every error
branch); that is a codegen detail and is not reproduced.

---

## 1. Function table

Twenty-five external symbols were stubs; `nm -an` adds the local helpers the
demangled file omits, and those are the interesting ones.

| module | function | address | size | status |
|---|---|---|---|---|
| `lib` | `Ledger::connect` | `0x10168e614` | 776 | **full** |
| `lib` | `Ledger::path_in` | `0x10168e91c` | 16 | **full** |
| `lib` | `Ledger::writing` | `0x10168e92c` | 8 | **full** |
| `lib` | `Ledger::note_import` | `0x10168e2d8` | 532 | **full** |
| `lib` | `Ledger::prune_pulses` | `0x10168e4ec` | 296 | **full** |
| `lib` | `migrate` | `0x10168ed90` | 1228 | **full**, one unknown (§5.4) |
| `lib` | `name_id` | `0x10168f25c` | 352 | **full** |
| `lib` | `Ledger::open` (local, not a stub) | `0x101282b78`, `0x10011c8a4` | — | **full** (context for `migrate`) |
| `reader` | `Ledger::reader` | `0x10168e934` | 172 | **full** |
| `reader` | `Reader::characters` | `0x10168b4d8` | 256 | **full** |
| `reader` | `Reader::has_pulse` | `0x10168cc58` | 340 | **full** |
| `reader` | `Reader::has_reward_on` | `0x10168b880` | 316 | **full** |
| `reader` | `Reader::hunt_overlapping` | `0x10168bc28` | 1356 | **full** |
| `reader` | `Reader::imported` | `0x10168cb1c` | 316 | **full** |
| `reader` | `Reader::last_pulses` | `0x10168b5d8` | 680 | **full** |
| `reader` | `Reader::latest_rewards` | `0x10168b9bc` | 372 | **full** |
| `reader` | `Reader::latest_snapshots` | `0x10168c174` | 572 | **full** |
| `reader` | `Reader::latest_store_claims` | `0x10168c3b0` | 256 | **full** |
| `reader` | `Reader::latest_store_visits` | `0x10168c4b0` | 256 | **full** |
| `reader` | `Reader::pulses` | `0x10168c5b0` | 708 | **full** |
| `reader` | `Reader::pulses_between` | `0x10168bb30` | 248 | **full** (it is a `COUNT(*)`) |
| `reader` | `Reader::rewards` | `0x10168c874` | 680 | **full** |
| `reader` | `Reader::snapshots` | `0x10168cdac` | 5420 | **full** |
| `reader` | `pulse` (local mapper) | `0x10168aca8` | 2096 | **full** |
| `reader` | `reward` (local mapper) | `0x10168d0ec` | 1332 | **partly** (§5.2) |
| `reader` | `snapshot` (local mapper) | `0x10168d620` | 3256 | **full** |
| `reader` | `store_visit` (local mapper) | `0x10168a8d4` | 980 | **full** |
| `row` | `Source::parse` (local) | `0x10168a808` | 204 | **full** |
| `row` | `Who::new` (local) | `0x1006ce96c`, `0x10129e93c` | — | **full** |
| `row` | `Stamp::now` | `0x1016995a4` | 116 | **full** (field *names* unknown) |
| `writer` | `Ledger::writer` | `0x10168e9e0` | 944 | **full** for the setup |
| `writer` | `Writer::try_record` | `0x10169398c` | 3436 | **full** (returns `bool`) |
| `writer` | `Writer::record` / `Writer::flush` | `{{closure}}` only | — | **partly** (async, no stub) |
| `writer` | `writer::run` (local) | `0x101692860` | 4396 | **partly** (§5.5) |
| `writer` | `<Closed as Display>::fmt` | `0x1016946f8` | 20 | **full** |

---

## 2. The recovered schema (migrations)

`migrate` (`0x10168ed90`) reads `PRAGMA user_version`, refuses anything above
**3**, and then runs one transaction per step: `BEGIN …`, the batch,
`PRAGMA user_version = n + 1`, `COMMIT`. The three batches are `(ptr, len)` pairs
in `__DATA` at `0x104442760` (the walk stops at `0x104442790`, i.e. three
entries) and are reproduced byte-for-byte in `src/schema.rs`
(`MIGRATION_1` 8,190 bytes, `MIGRATION_2` 451, `MIGRATION_3` 362).

```sql
-- MIGRATION_1 (0x103983834, 8190 bytes): schema version 1
CREATE TABLE pulses (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        game_time INTEGER,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        exp INTEGER NOT NULL,
        axp INTEGER NOT NULL,
        exp_gained INTEGER NOT NULL DEFAULT 0,
        axp_gained INTEGER NOT NULL DEFAULT 0,
        fxp INTEGER,
        fxp_max INTEGER,
        until_next INTEGER,
        lumnis INTEGER,
        rpa TEXT,
        fashlonae INTEGER,
        mind_percent INTEGER,
        mind_text TEXT,
        source TEXT NOT NULL DEFAULT 'normal',
        instant INTEGER NOT NULL DEFAULT 0
    );
CREATE INDEX pulses_who_at ON pulses(instance, character, at);
CREATE TABLE snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        game_time INTEGER,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        kind TEXT NOT NULL,
        name TEXT,
        value INTEGER,
        weekly INTEGER,
        total INTEGER
    );
CREATE INDEX snapshots_who_kind_at ON snapshots(instance, character, kind, at);
CREATE TABLE names (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        number INTEGER,
        mnemonic TEXT
    );
CREATE UNIQUE INDEX names_kind_name ON names(kind, name);
CREATE INDEX names_mnemonic ON names(kind, mnemonic);
CREATE TABLE hunts (id, instance, character, started_at, ended_at)      -- + hunts_who
CREATE TABLE creatures (…)                                             -- + creatures_hunt, creatures_exist
CREATE TABLE attacks (… root_attack_id, parent_attack_id, …)            -- + attacks_{hunt,creature,name,root}
CREATE TABLE resolutions (…)                                           -- + resolutions_attack
CREATE TABLE flare_rows (…)                                            -- + flare_rows_{attack,name}
CREATE VIEW flares AS SELECT … FROM flare_rows f JOIN names n ON n.id = f.name_id;
CREATE TABLE hits (…)                                                  -- + hits_{attack,creature,creature_only,flare}
CREATE TABLE statuses (…)                                              -- + statuses_{creature,attack}
CREATE TABLE imports (… stamp, path, size, imported_at, first_at, last_at, hunts, pulses)
CREATE UNIQUE INDEX imports_session ON imports(instance, character, stamp);
CREATE TABLE gemstone_rows (…)                                         -- + gemstone_rows_{hunt,property}
CREATE VIEW hunt_gemstones AS SELECT … FROM gemstone_rows g JOIN names n ON n.id = g.property_id;
CREATE TABLE hunt_effects (…)                                          -- + hunt_effects_hunt
CREATE TABLE hunt_gear (…)                                             -- + hunt_gear_hunt
```

The full text of every statement is in `src/schema.rs`; the abbreviations above
are only for this table. The `flares`/`hunt_gemstones` views are what
`hydra_combat_log`'s `Store` calls the joined tables.

```sql
-- MIGRATION_2 (0x103985832, 451 bytes): schema version 2
CREATE TABLE login_rewards (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL, game_time INTEGER,
        instance TEXT NOT NULL, character TEXT NOT NULL, day TEXT NOT NULL,
        streak INTEGER, bridges INTEGER,
        reward TEXT NOT NULL, thanks TEXT NOT NULL, line TEXT NOT NULL
    );
CREATE UNIQUE INDEX login_rewards_day ON login_rewards(instance, character, day);

-- MIGRATION_3 (0x1039859f5, 362 bytes): schema version 3
CREATE TABLE store_visits (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL, account TEXT NOT NULL, game TEXT NOT NULL,
        claimed INTEGER NOT NULL, message TEXT NOT NULL, balance TEXT NOT NULL,
        next_at INTEGER
    );
CREATE INDEX store_visits_account ON store_visits(account, game, at);
```

`src/schema.rs`'s tests assert the three lengths, every table/view/index name and
that each of the eighteen `PULSE_COLUMNS` exists in the `pulses` table.

---

## 3. The recovered statements, one per reader/writer

Every query below is quoted in `src/schema.rs` with its address, and the length
in the `mov wN, #len` that feeds `prepare_with_flags`/`StatementCache::get` (or,
for a `SELECT {}` template, the format blob's address). The transcribed text of
each statement is pinned by a length assertion in `src/schema.rs`'s tests.

| reader | SQL (predicate / order / limit) | binds |
|---|---|---|
| `characters` `0x10168b4d8` | `SELECT instance, character FROM pulses UNION SELECT instance, character FROM hunts ORDER BY 1, 2` (113 B) | none |
| `last_pulses` `0x10168b5d8` | `SELECT {} FROM pulses WHERE instance = ?1 AND character = ?2 ORDER BY at DESC, id DESC LIMIT ?3` (blob `0x103e6e457`) | who, limit (clamped to `i64::MAX`) |
| `rewards` `0x10168c874` | `SELECT {} FROM login_rewards WHERE instance = ?1 AND character = ?2 ORDER BY at DESC, id DESC LIMIT ?3` (blob `0x103e6e553`) | who, limit |
| `pulses` `0x10168c5b0` | `SELECT {} FROM pulses WHERE instance = ?1 AND character = ?2 AND at >= ?3 AND at < ?4 ORDER BY at, id` (blob `0x103e6e4d2`) | who, from, to |
| `pulses_between` `0x10168bb30` | `SELECT COUNT(*) FROM pulses WHERE instance = ?1 AND character = ?2 AND at BETWEEN ?3 AND ?4` (91 B) | who, from, to |
| `has_pulse` `0x10168cc58` | `SELECT COUNT(*) FROM pulses WHERE instance = ?1 AND character = ?2 AND at = ?3 AND exp = ?4` (91 B) | who, at, exp |
| `snapshots` `0x10168cdac` | `SELECT {} FROM snapshots WHERE instance = ?1 AND character = ?2 AND kind = ?3 AND at >= ?4 AND at < ?5 ORDER BY at, id` (blob `0x1039835ca`) | who, kind **as text**, from, to |
| `latest_snapshots` `0x10168c174` | `SELECT {} FROM snapshots WHERE id IN (SELECT MAX(id) FROM snapshots WHERE instance = ?1 AND character = ?2 GROUP BY kind, name) ORDER BY kind, name` (blob `0x103983283`) | who |
| `latest_rewards` `0x10168b9bc` | `SELECT {} FROM login_rewards WHERE id IN (SELECT MAX(id) FROM login_rewards GROUP BY instance, character) ORDER BY instance, character` (blob `0x10398302d`) | **none** |
| `latest_store_claims` `0x10168c3b0` | `SELECT at, account, game, claimed, message, balance, next_at FROM store_visits WHERE id IN (SELECT MAX(id) FROM store_visits WHERE claimed = 1 GROUP BY account, game) ORDER BY account, game` (241 B) | none |
| `latest_store_visits` `0x10168c4b0` | the same without `claimed = 1` (197 B) | none |
| `has_reward_on` `0x10168b880` | `SELECT COUNT(*) FROM login_rewards WHERE instance = ?1 AND character = ?2 AND day = ?3` (86 B) | who, day |
| `imported` `0x10168cb1c` | `SELECT COUNT(*) FROM imports WHERE instance = ?1 AND character = ?2 AND stamp = ?3` (82 B) | who, stamp |
| `hunt_overlapping` `0x10168bc28` | `SELECT h.id FROM hunts h WHERE h.instance = ?1 AND h.character = ?2 AND h.started_at <= ?4 AND COALESCE(h.ended_at, (SELECT MAX(occurred_at) FROM attacks WHERE hunt_id = h.id), h.started_at) >= ?3 ORDER BY h.id LIMIT 1` (274 B) | who, `?3` = from, `?4` = to |

| writer / ledger | SQL | binds |
|---|---|---|
| `Ledger::note_import` `0x10168e2d8` | `INSERT INTO imports (instance, character, stamp, path, size, imported_at, first_at, last_at, hunts, pulses) VALUES (?1 … ?10)` (185 B) | who ×2, stamp, path, size, `imported_at` = `SystemTime::now`, first/last, hunts, pulses |
| `Ledger::prune_pulses(false)` `0x10168e4ec` | `DELETE FROM pulses WHERE instance = ?1 AND character = ?2 AND exp_gained = 0 AND axp_gained = 0` (108 B) | who |
| `Ledger::prune_pulses(true)` | `DELETE FROM pulses WHERE instance = ?1 AND character = ?2` (57 B) | who |
| `name_id` `0x10168f25c` | `INSERT OR IGNORE INTO names (kind, name, number, mnemonic) VALUES (?1, ?2, ?3, ?4)` (82 B) then `SELECT id FROM names WHERE kind = ?1 AND name = ?2` (50 B) | kind, name, `Option<i64>`, `Option<&str>` |
| `writer::run` | `INSERT INTO pulses (…) VALUES (?1 … ?18)` (303 B); `INSERT INTO snapshots (…) VALUES (?1 … ?9)` (157 B); `INSERT INTO store_visits (…) VALUES (?1 … ?7)` (125 B); the `login_rewards` upsert (518 B) | the row's columns, in the `SELECT` order |
| `migrate` | `PRAGMA user_version` (19 B), `BEGIN DEFERRED|IMMEDIATE|EXCLUSIVE`, `COMMIT`, the three batches | — |

The `login_rewards` write is an upsert, which is why a repeated login line does
not pile up rows:

```sql
INSERT INTO login_rewards (at, game_time, instance, character, day, streak,
    bridges, reward, thanks, line) VALUES (?1 … ?10)
 ON CONFLICT (instance, character, day) DO UPDATE SET
    at = excluded.at, game_time = excluded.game_time, streak = excluded.streak,
    bridges = excluded.bridges, reward = excluded.reward,
    thanks = excluded.thanks, line = excluded.line
 WHERE login_rewards.reward = '' AND excluded.reward <> ''
```

---

## 4. Row layouts

`instance`/`character` are part of every per-character row (the queries bind them
as `?1`/`?2` from a [`Who`] instead of `SELECT`ing them per row), which is why
each row type carries both.

### `Pulse` — 208 bytes (`0xd0`), `reader::pulse` `0x10168aca8`

| offset | field | column |
|---|---|---|
| `0x00` | `instance: String` | 2 |
| `0x18` | `character: String` | 3 |
| `0x30` | `rpa: Option<String>` | 12 (`rpa TEXT`) |
| `0x48` | `mind_text: Option<String>` | 15 |
| `0x60` | `game_time: Option<u32>` | 1 |
| `0x68` | `at: i64` | 0 |
| `0x70` | `fxp: Option<u32>` | 8 |
| `0x78` | `fxp_max: Option<u32>` | 9 |
| `0x80` | `until_next: Option<u32>` | 10 |
| `0x88` | `lumnis: Option<u32>` | 11 |
| `0x90` | `fashlonae: Option<u32>` | 13 |
| `0x98` | `mind_percent: Option<u32>` | 14 |
| `0xa0` | `exp: i64` | 4, clamped `max(v, 0)` |
| `0xa8` | `axp: i64` | 5, clamped `max(v, 0)` |
| `0xb0` | `exp_gained: i64` | 6 |
| `0xb8` | `axp_gained: i64` | 7 |
| `0xc0` | `instant: i64` | 17 |
| `0xc8` | `source: Source` (one byte) | 16, via `Source::parse` |

Evidence: the column order is the `SELECT` (160-byte list at `0x103982ee9`); the
`Row::get` monomorphizations split the eighteen columns into four types
(`{cap,ptr,len}` triples for the four Strings, an 8-byte tag+value for the seven
`Option<u32>`s, `i64` for `at`/`exp`/`axp`/`exp_gained`/`axp_gained`/`instant`);
and the tail stores at `0x10168b000`…`0x10168b0ac` give the offsets. The clamp is
`bic x10, x23, x23, asr #63` at `0x10168afec`.

The `exp`/`axp` clamp is applied where the row is produced (the mapper), so this
build applies it in `store::Connection::insert_pulse`.

### `Snapshot` — 144 bytes (`0x90`), `reader::snapshot` `0x10168d620`

`value: Option<i64>` `+0x00`, `weekly: Option<i64>` `+0x10`, `total: Option<i64>`
`+0x20`, `instance: String` `+0x30`, `character: String` `+0x48`,
`name: Option<String>` `+0x60`, `game_time: Option<u32>` `+0x78`, `at: i64`
`+0x80`, `kind: SnapshotKind` `+0x88`.

`kind` is TEXT in the database and a three-variant enum in memory: the mapper
compares `favor`/`resource`/`suffused` inline
(`movk w10, #0x6f76`… at `0x10168d758`, `#0x6f73,#0x7275,#0x6563` at
`0x10168d868`, `#0x6666,#0x7375,#0x6465` at `0x10168d884`).

### `Reward` — 176 bytes (`0xb0`), `reader::reward` `0x10168d0ec` — **partly**

Six string capacities at `0x20`, `0x38`, `0x50`, `0x68`, `0x80`, `0x98`
(`drop_in_place<Reward>`), so the four numbers fill `0x00`…`0x20`. Column types
from the per-column `Row::get` monomorphizations: `at` `i64`, `game_time`
`Option<u32>`, `streak`/`bridges` `Option<i64>`, and the six Strings
`instance`, `character`, `day`, `reward`, `thanks`, `line`. Which string sits at
which of the six offsets is **not** established (§5.2); the declaration in
`src/row.rs` follows the column order.

### `StoreVisit` — 128 bytes (`0x80`), `reader::store_visit` `0x10168a8d4`

`next_at: Option<i64>` `+0x00`, `account: String` `+0x10`, `game: String`
`+0x28`, `message: String` `+0x40`, `balance: String` `+0x58`, `at: i64` `+0x70`,
`claimed: bool` `+0x78` (`strb w22, [x23, #0x78]`).

### `Who` — 48 bytes (`0x30`)

`instance: String` `+0x00`, `character: String` `+0x18` — every reader binds
`&who` and `&who + 0x18` (`add x9, x20, #0x18`), and `hydra_exp`'s recovered
`command::read` reads the report's character name at `+0x20`/`+0x28`, which is
this `character` string's pointer and length.

### `Row` — 208 bytes (`0xd0`)

An enum over the four writeable rows (`Pulse`, `Snapshot`, `Reward`,
`StoreVisit`), established by the four `INSERT`s `writer::run` issues, by
`try_record`'s 0xd0-byte copy, and by `drop_in_place<Row>` (`0x1012bd28c`), whose
three other arms drop a `Snapshot` (strings at `0x38`/`0x50`/`0x68`), a `Reward`
(`0x28`…`0xa0`, six) and a `StoreVisit` (`0x18`…`0x60`, four) — each exactly `+8`
from the standalone layouts above, so the tag word sits at `+0` and the payloads
of those three variants at `+8`. Variant 0 is the `Pulse`, whose capacity word is
never negative, and the other three use `isize::MIN + n` sentinels in that same
word. This build uses a plain enum.

### `Stamp` — 16 bytes

`u32 @0`, `u32 @4`, `at: i64 @8`; `Stamp::now` stores its two word arguments
first (`stp w20, w19, [x21]`) and the clock second (`str x8, [x21, #8]`).

---

## 5. The stubs, one at a time

### 5.1 The ledger handle

`Ledger` is `{ dir: PathBuf }`. `connect` loads the path as a `&str` from
`+0x08`/`+0x10` (the `String` layout above), `path_in` `Path::_join`s the
eight-byte `hydra.db`.

`Ledger::connect(read_only: bool)` — the flags:
`cmp w24, #0; csel w2, w9(#0x8001), w8(#0x8006), ne`, so a **non-zero** argument
means `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` and zero means
`READ_WRITE | CREATE | NO_MUTEX`. Then `busy_timeout(Duration{5, 0})`, and — only
for a writable connection — `PRAGMA journal_mode = WAL` and
`PRAGMA synchronous = NORMAL`. The argument is therefore *read-only*, not
*writing*; `Ledger::writing` is the eight-byte `mov w1, #0; b connect`
(`0x10168e92c`) that names the common case.

`note_import`'s argument list was read off the register use: `x1 = &Who`,
`(x2,x3)`/`(x4,x5)` two `&str`, `x6` a `u64` clamped with
`cmp x22, #0x7fff…; csel`, `x7` a pointer to a 32-byte pair (`x20` and
`x20 + 0x10` share one `ToSql` vtable, i.e. the two `Option<i64>`s rustc passes
indirectly), and the two counters on the stack (`ldp x9, x8, [x29, #0x10]`). So
the signature is
`note_import(&self, who, stamp, path, size: u64, times: (Option<i64>, Option<i64>), hunts: u64, pulses: u64)`.

### 5.2 `reader::reward` (partly)

The column *types* are established (six Strings, one `i64`, one `Option<u32>`,
two `Option<i64>`); the mapping from the six strings to the six offsets is not —
the tail stores of `reader::reward` are interleaved with the error blocks and do
not resolve column-by-column the way `pulse`'s do. `src/row.rs` therefore
declares the strings in column order and marks the layout as partly recovered.
Nothing in the crate depends on `Reward`'s byte offsets: it is only built by the
mapper and written by the upsert.

### 5.3 `Stamp::now`'s two arguments

`Stamp::now(a: u32, b: u32) -> Stamp` is established (the two word arguments are
stored at `+0`/`+4`). *What* they are is not: one caller passes the adjacent
`u32`s a game-state struct keeps (`ldr w0, [x19, #0x2728]; ldr w1, [x19, #0x272c]`
at `0x1012d5c54`), another passes a literal `0` (`0x100072e4c`). The only
consumer in the binary reads `+0x08` alone (`hydra_rewards::watcher::Heard::at`),
so the fields are named `hour`/`minute` with `// UNKNOWN:` at the site.

### 5.4 `migrate`'s `BEGIN` variant (unknown)

`migrate` picks the `BEGIN` text with `ldrb w8, [x20, #0x68]` — one byte past the
0x68-byte connection value it was handed. `Ledger::open`'s call site writes the
`connect` result tag exactly there (`strb w8, [sp, #0x88]` with the connection at
`sp+0x20`) and only calls `migrate` when that tag is not the error value, so the
index is `0` in every recovered call path: `BEGIN DEFERRED`. The three-entry
table is kept in `src/schema.rs`, and `migrate` always uses `BEGIN DEFERRED`,
marked `// UNKNOWN:`.

### 5.5 `writer::run` (partly)

The four `INSERT`s, the three `BEGIN` texts and `COMMIT` are established; the
batching rule (how many rows per transaction), the `tokio::select!` structure and
the shutdown handshake are not. The reconstruction drains the queue into one
batch, writes it, commits, then answers the flushes that arrived with it.

`Writer::try_record` is **full** and has a surprising return: `cmp x19, #0x2;
cset w0, ne` returns `false` only for `TrySendError::Closed`, so a row dropped
because the 1024-slot queue was full still returns `true` — "the writer is alive"
rather than "the row was queued". This build keeps that rule and counts the drops
in `Writer::dropped()`.

---

## 6. `std`-only departures

| original | recovered use | what this build does |
|---|---|---|
| `rusqlite` (`Connection`, `Statement`, `StatementCache`, `query_row`, `query_map`, `execute`, `execute_batch`, `pragma_update`, `busy_timeout`, `Transaction`) | every read and write in the crate | [`store::Connection`]: the recovered schema as typed row vectors, every recovered statement as a typed operation, and the SQL text kept verbatim in `src/schema.rs` beside each operation. There is **no SQL engine** — statements are dispatched by identity (`Connection::execute_batch`), never parsed, and an unknown text is an `Error::Unsupported` rather than a silent no-op. |
| `rusqlite::Error` (`Display` into the caller's text) | `Ledger::connect`/`reader`/`note_import`/`prune_pulses`, `Writer` | [`Error`] with `Sqlite`/`Io`/`Message`/`ReadOnly`/`Unsupported`/`Constraint` arms; `Display` is what the original prints. |
| the SQLite file `hydra.db` | `path_in`, `connect` | **not reproduced.** No SQLite file format is read or written. `Connection::commit`/the auto-commit path write a documented line format (`store.rs`, `hydra-ledger <version>` header, `\t` separated with `\\`,`\t`,`\n`,`\r` escaped, empty field = `NULL`). A real `hydra.db` is *refused*, not mis-parsed: `Connection::open` rejects a file whose first record is not the header, and `ledger::is_this_builds_store` answers the question explicitly. A tiny SQLite reader/writer was judged out of scope. |
| SQLite's auto-commit | every `execute` | every mutating `store::Connection` method persists when the connection is file-backed, so a row written by one `Ledger::connect` is visible to the next — the behaviour the crate's own call pattern (open, write, drop) relies on. `commit()` is only the explicit end of a transaction (`migrate`, the writer's batch). |
| `tokio::sync::mpsc::channel(1024)` + `try_send` | `Ledger::writer`, `Writer::try_record` | `std::sync::mpsc::sync_channel(1024)` — same bound, same full/closed distinction. |
| `tokio::sync::oneshot` for the flush ack | `Sent`'s second variant | a zero-capacity `std::sync::mpsc::sync_channel`. |
| `async fn record` / `async fn flush` | `Writer::record`/`flush` | blocking. |
| `std::thread::Builder::spawn_unchecked` | `Ledger::writer` | `Builder::spawn` (same name, `hydra-ledger`). The `format!("ledger thread: {}")` error text is kept. |
| `hydration` of `PRAGMA journal_mode = WAL` / `synchronous = NORMAL` | `connect` | recorded in the docs; nothing to do without SQLite. |

### What this build cannot do

* open a real `hydra.db` (no SQLite file format);
* run SQL that is not one of the recovered statements;
* share one database between two live connections other than through the file
  (each `Connection` holds its own row vectors; writes are persisted, so a later
  connection sees them, but two *open* connections do not see each other's).

---

## 7. Evidence conflicts and things that could not be established

1. **`pulses_between` is a `COUNT(*)`.** `hydra_exp`'s recovery (`crates/hydra_exp/src/command.rs`)
   declares `Source::pulses_between(&self, from, to) -> Vec<Pulse>` and uses it
   for every report window. The binary's `Reader::pulses_between`
   (`0x10168bb30`, 248 bytes) prepares `SELECT COUNT(*) …` with four binds and
   returns `Result<i64>` — the row query is `Reader::pulses` (`0x10168c5b0`,
   `at >= ?3 AND at < ?4`). Both facts are kept: this crate implements the
   binary's two methods, and `hydra_exp` keeps its own stand-in (it may not be
   modified). A caller that wants rows must use `Reader::pulses`.
2. **`last_pulses` returns newest first.** The recovered text is
   `ORDER BY at DESC, id DESC`, so `Vec<Pulse>[0]` is the *newest* row;
   `hydra_exp`'s `Source::last_pulses` doc-comment says "oldest first" and
   `report::level` takes `pulses.last()` as the newest. That is a real
   disagreement between the sibling's stand-in and the binary; the SQL is
   quoted here and the tests pin the binary's order.
3. **`Reader::latest_rewards` has no character bind.** The SQL groups by
   `(instance, character)` and the function makes **zero**
   `bind_parameter` calls, so unlike `latest_snapshots` it is not filtered by
   `Who`. Implemented as recovered.
4. **`Reader::has_pulse`/`imported`/`has_reward_on` return `i64`, not `bool`.**
   The result is the two-word `niche + i64` shape and the mapper is
   `Row::get::<i64>(0)` over a `COUNT(*)`.
5. **`Option<u32>`/`Option<i64>` have no niche** in this build (tag word + value
   word), which is why `Pulse`'s seven optional `u32`s cost 8 bytes each.
6. **`Reward`'s string offsets** — see §5.2.
7. **`Stamp`'s first two fields' meaning** — see §5.3.
8. **`migrate`'s `BEGIN` byte** — see §5.4.
9. **`Pulse`'s `instant` vs `hydra_exp`'s `bonus`.** The column is
   `pulses.instant` and it is an integer amount; `hydra_exp`'s copy of the row
   calls it `bonus` because that is how `report::daily`/`hour` use it. Same
   field, same offset (`+0xc0`).
10. **`Snapshot`'s `name` vs `hydra_exp`'s `label`.** Same field (`+0x60`),
    different name.
11. **`Snapshot`'s `kind`** is TEXT in the schema and a three-variant enum in
    memory; `hydra_exp` carries it as a bare `u8`, which is the same byte.
12. **`hydra_exp`'s reading of the row puts `fashlonae` at `+0x90` and
    `game_time` nowhere.** Here `game_time` is at `+0x90`'s neighbour `+0x60`
    and `fashlonae` at `+0x90`; the mapper's tail stores agree with this
    `NOTES.md` and not with any other assignment.
