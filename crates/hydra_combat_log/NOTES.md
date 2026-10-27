# `hydra_combat_log` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(demangled symbols in `/Users/cpmbpro/Code/hydra-inspect/symbols_demangled.txt`,
plus the **local** `t` symbols from `nm -an`, which the demangled file omits).
`hydra_combat_log` has 48 external text symbols — the stub inventory — and 98 more
local ones (helpers, closures, `drop_in_place` shims, duplicated `Recorder`
destructors for the crates that inline it).

Status: `cargo check -p hydra_combat_log --offline` and
`cargo test -p hydra_combat_log --offline` pass (**67 tests**). No
`todo!()`/`unimplemented!()` remains.

**Phase 1 (dependency reconnection):** `rusqlite` 0.37.0 (`bundled`) and
`tokio` 1.53.1 (`full`) are back, so the tables really are a SQLite database and
the recorder loop really is async; `hydra_ledger`, `hydra_model` and
`hydra_session` are the real path dependencies, since all three are implemented
in this workspace now. §1 is the corrected dependency table, §1.1 the record of
what each reconnection changed.

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow, literals and constants read out of the instruction stream; covered by tests with real values. |
| **partly** | entry contract, literals and the called helpers are recovered, but a data table, a regex capture or a predicate is missing, so the body implements the evidenced subset only. |
| **unknown** | only the call shape is visible. The safest behaviour is implemented and marked `// UNKNOWN:` at the site. |

---

## 0. How the literals were read

`__TEXT,__const` format strings are stored as **length-prefixed pieces** with
tagged placeholder bytes — not NUL-separated strings.  The tag byte layout is
visible in `alloc::fmt::format::format_inner` (0x101eb6920) and
`core::fmt::write` (0x101ebd9ec):

```
<len><bytes>      a literal piece (`len` < 0x80, in bytes)
0x80 <u16 len><bytes>   a long literal piece
0xC0              a plain `{}`
0x81..=0xFF       a `{}` with a spec: bit0 -> 4-byte spec word, bit1 -> u16,
                  bit2 -> u16, bit3 -> u16 (argument position)
0x00              end of the arguments
```

So `c0 01 73 00` is `{}s`, `c3 20 00 00 69 02 00 01 3d 00` is `{:02}` (`0xC3` =
bit0+bit1: the spec word `0x6900_0020` plus width 2), and
`01 '[' c0 02 '][' c0 02 '] ' c0 01 '\n'` is `[{}][{}] {}\n`.  Validation: the
recovered format of `hydra_net::eastern::format` decodes to `{:04}-{:02}-{:02}`,
byte-for-byte the string the sibling `hydra_net` recovery documents.

SQL statements are plain NUL-free text in `__const`; their length comes from the
`mov wN, #len` that feeds `Connection::prepare`/`StatementCache::get`.

Tooling (throwaway, outside the repository):

* a Mach-O/`__const` reader plus an `objdump`-driven disassembler with `adrp`/
  `add` literal resolution (the same driver `../recon.py` uses),
* a packed-format-string decoder for the layout above,
* a `bl`-target xref scanner over `__text` (used to find the call sites of
  `command::run`, `catalog::Entry::command`, `Recorder::open`, …),
* a word-constant extractor that rebuilds `mov`/`movk` pairs into the ASCII words
  a `match` compares against.

---

## 1. Dependency reality

The original crate depends on `rusqlite`, `tokio`, `hydra_ledger`,
`hydra_model`, `hydra_net` and `hydra_session`. Every one of them is a real
dependency of this build again:

| original | recovered use | what this build does now |
|---|---|---|
| `rusqlite` | 21 `StatementCache::get`, 86 `Connection::prepare_with_flags`, 41 `query_row`, 14 `query_map` call sites | **reconnected, `rusqlite` 0.37.0 `bundled`.** [`store::Store`] holds a `hydra_ledger::Connection` — a real `rusqlite::Connection` — and every recovered statement is executed through `prepare_cached` (rusqlite's `StatementCache`). The recovered SQL text is kept as a `const` beside each method. |
| `rusqlite::Error` | the `Result` payload niche (`0x8000_0000_0000_0016` for `Ok`) | **reconnected as a conversion.** [`Error`] is still the text the original printed, but `rusqlite::Error` and `hydra_ledger::Error` now convert into it (`From`), so a failed statement lands there instead of unwrapping or panicking. |
| `tokio` | `task::start` builds a runtime, `listen` is an async closure over a `broadcast::Receiver`, `Sent` travels over `mpsc` + `batch_semaphore` | **reconnected, `tokio` 1.53.1 `full`.** [`task::listen`] is the async loop over `tokio::sync::mpsc::Receiver<Sent>`; [`task::relay`] is the closure's other half, following a real `broadcast::Receiver<hydra_session::command::Event>`; [`task::start`]/[`task::run`] keep their synchronous signatures by building a current-thread runtime and writing to the recovered thread name. |
| `hydra_ledger` | `Ledger::writing` (`Recorder::open`), `Ledger::reader` (`command::answer`), `name_id` (`sync_gear`) | **reconnected as a path dependency.** `hydra_ledger` is no longer stubs: `Store::open` is `Ledger::open` + `Ledger::writing`, `Store::open_read_only` is `Ledger::connect(true)` (`Ledger::reader`'s own connection), the schema is `hydra_ledger::migrate`, and `Store::name_id`/`sync_gear` call `hydra_ledger::name_id`. §1.1 records the places the APIs do not line up. |
| `hydra_net` | `eastern::{offset, date, format, now}` in `report::clock`/`report::stamp`/`task::relay` | **path dependency, unchanged**; its recovered formats match, so the timestamps are byte-identical. |
| `hydra_model` | `state::combat::defs::defs`, `character::gemstones::Gemstones::equipped_properties` inside `analytics::gear`/`flare` | **partly reconnected.** `Recorder::record_gemstones` takes the real `hydra_model::…::gemstones::Gemstones` and writes one `hunt_gemstones` row per equipped property. `defs()` stays unused: `hydra_model`'s table is *empty* in this rebuild (the generated data file is not embedded), so wiring it into `analytics::gear` would change nothing — the seam stays and is documented at the site. |
| `hydra_session` | `event::Event` drop shims inside `task::start`'s closure | **reconnected.** The live enum is `hydra_session::command::Event` (there is no `event` module); [`task::relay`] consumes its `broadcast` channel and maps `Event::Frames` → [`task::Sent::Line`], `Event::Over` → [`task::Sent::Ended`], dropping the rest. |

The one recovered *data* table that is embedded verbatim is the help block
(§4.3).

### 1.1 Phase 1 reconnection: what changed

Dependencies actually added (`crates/hydra_combat_log/Cargo.toml`):

```toml
rusqlite = { workspace = true }            # 0.37.0, `bundled`
tokio = { workspace = true }               # 1.53.1, features = ["full"]
hydra_ledger = { path = "../hydra_ledger" }
hydra_model = { path = "../hydra_model" }
hydra_net = { path = "../hydra_net" }      # already there
hydra_session = { path = "../hydra_session" }
```

`Cargo.lock` only gains those five edges for `hydra_combat_log`.

**What is now backed by a real crate**

* **The database.** `Store` is a `hydra_ledger::Connection` over the recovered
  schema (`hunts`, `attacks`, `hits`, `creatures`, `flare_rows`+`flares` view,
  `statuses`, `hunt_effects`, `hunt_gear`, `gemstone_rows`+`hunt_gemstones`
  view, `names`). `Store::new` is `:memory:` + `hydra_ledger::migrate`;
  `Store::open(dir)` is the file-backed ledger `Ledger::open`+`Ledger::writing`;
  `Store::open_read_only(dir)` is the read-only reader, and it refuses writes
  with SQLite's own `attempt to write a readonly database`. The inserts, the
  updates and the selects quoted throughout `store.rs` are the recovered
  statements, prepared through the statement cache. `Store` is no longer
  `Clone` (a `Connection` is not); `Store::connection` hands the handle out and
  `Store::open` reopens the same file.
* **The row types are unchanged.** `Attack`, `Hit`, `Creature`, `Flare`,
  `Status`, `Effect`, `GearRow`, `Gemstone`, `Hunt`, `Name` keep every field and
  type; the readers return owned rows instead of `&Row` (a `Vec<Attack>` rather
  than a `Vec<&Attack>`), and the writers return `Result<i64, Error>` because a
  statement can now fail. Three schema columns disagree with the recovered rows
  and are mapped at the boundary, documented on the field: `hunts.instance` is
  `TEXT` (the row carries the `i64` the session query binds), `attacks.
  attacker_exist_id`/`creatures.exist_id` are `INTEGER` (the rows carry text),
  and `attacks.weapon`, `attacks.redirected_from`, `flare_rows.weapon` are `TEXT`
  (the rows carry name ids).
* **The recorder loop.** `task::listen` is `async` over `tokio::sync::mpsc`,
  `task::relay` consumes `hydra_session`'s `broadcast::Receiver<Event>` and
  stamps each frame with `hydra_net::eastern::now()`; `task::start`/`task::run`
  are the synchronous adapters on a current-thread runtime, so the
  `hydra-combat-log` thread name and the in-order message handling are
  unchanged.
* **`hydra_ledger::name_id`.** `Store::name_id` interns through the ledger's own
  function against `names_kind_name`; `names.kind` is `name` (`GEAR_KIND`/`PROPERTY_KIND`
  for the two tables that name one) — the kind argument is not visible at the
  recovered call sites, marked `// UNKNOWN:`.
* **`hydra_model`.** `Recorder::record_gemstones(&Gemstones)` writes the real
  gemstone report into `hunt_gemstones`; `report::gemstones_line` prints it.

**What could not be reconnected (and stays as it is)**

* **`analytics::gear`/`flare`'s gemstone property words** — `hydra_model::state::
  combat::defs::defs()` returns an *empty* table in this rebuild (its TSV data
  file is not embedded), so wiring it in would change no output. The seam stays
  and the sites keep their `// UNKNOWN:` markers.
* **`command::answer`'s reader.** The recovered call shape is
  `Ledger::reader() -> Reader`, but `hydra_ledger::reader::Reader` keeps its
  connection private (`Reader::connection()` hands out a `&Connection`), so a
  `Store` cannot take it over. `Store::open_read_only` calls `Ledger::connect
  (true)`, which is the exact connection `Ledger::reader` wraps.
* **`Recorder::sync_effects`' transaction.** The original closes by dropping the
  connection and issuing `COMMIT` (0x102413ab9); every statement here runs in
  SQLite's auto-commit mode, so the commit is implicit (`hydra_ledger::
  Connection::commit` is a documented no-op for the same reason).
* **The `event::Event` module name.** The original imported
  `hydra_session::event::Event`; the implemented module is
  `hydra_session::command::Event`. The type is the one the session actually
  broadcasts, so the relay uses it and this is the only difference.

---

## 2. Function table

`full` = behaviour recovered from the disassembly; `partly` = entry contract,
literals and called helpers recovered, body implements the evidenced subset;
`unknown` = only the call shape is visible.

### `analytics.rs` (10 stubs + 10 helpers)

| fn | address | status | notes |
|---|---|---|---|
| `hp` | 0x10130bbf8 | partly | heads `ID Our DMG All DMG`, `HP`, `Average: `; damage select 0x10240f8ae |
| `gear` | 0x10130d148 | partly | `Worn over the hunts:`, `Held as a swing was made:`, `No swing has it hands on record yet.`; gemstone property words unavailable (`// UNKNOWN:`) |
| `flare` | 0x10130e028 | partly | `Proc rate by attack:`, `Procs`, `targets`, `  No damage recorded (non-damaging flare).`, `[fires alongside]`/`[intercepts]`; the barrier table's predicates are recovered, the rank/gemstone sub-tables are not |
| `effects` | 0x101313d74 | partly | predicate `EXISTS (SELECT 1 FROM flares f WHERE f.attack_id = a.id AND f.ours = 1 AND f.name = ?1)`; heads `Effect`, `Down: procs/swings` |
| `barkskin` | 0x10131444c | partly | heading + columns recovered; the "gap past the cooldown starts another" grouping constant is not |
| `alongside` | 0x1013152c8 | partly | `[fires alongside]` / `[intercepts]` markers recovered |
| `compare` | 0x101312070 | partly | heading + the four hunt statements recovered |
| `defense` | 0x101313204 | partly | `Creatures (click one for DS/TD/UDF):`, `Min Avg Median Max`, `DS TD UDF`; the reading rows come from `statuses`, which the recorder only stores per attack |
| `intercepts` | 0x1013066dc | partly | heading + barrier predicate recovered |
| `analytics` | 0x101315a38 | partly | 0x3d08 bytes, six reports in one inlined match; headings recovered (`By attacker (…)`, `By round:`, `By the weapon in hand:`, `(hands not known)`, aim table, `Status`/`Events`/`Creature-coverage`) |
| helpers | 0x101303a5c… | — | `bark_casts`, `efficiency`, `gear_lines`, `said_median`, `gemstone_lines`, `group_location`, `reactive_flare`, `while_up_lines`, `pct`, `median` |

### `detail.rs` (4 stubs + 10 helpers)

| fn | address | status | notes |
|---|---|---|---|
| `attack_detail` | 0x1013435dc | partly | title/labels/hit line recovered exactly (`=== ATTACK REPORT #{} ==={}`, `Type: {}{}{}`, `Outcome: {}`, `Sequence: {}   Session: {}`, `Occurred: {}`, `Target: {}`, `Attacker: {}`, `Flags: {}`, `RESOLUTION: NONE\tPenalty: `, `STATUS EFFECTS: NONE  (window)`, `FLARES: NONE`, `CRIT`, ` AMPUTATED`, `[FATAL]`); the flags join and the status window filter are inferred |
| `attack_by_name` | 0x101346eec | full | name list statement 0x103e66ae2, heading `Attacks recorded (cstats attack <name> for one):`, `Index {} is out of range for {} ({} seen).` |
| `tree` | 0x10134a6a8 | full | children statement 0x102410c4a, ` -> {}` arrows, `SEQUENCE: single attack (no spawn tree); hit  (this attack)` |
| `creature` | 0x10134aff8 | partly | headings/errors recovered (`Unknown creature '{}'. The nouns recorded:`, `No creature with id {}.`, ` Name: {}`, ` Noun: {:18} ID: {}   Session: {}`, `First: {:18} Last: {}   Duration: {}`, `  DMG: {:18} Hits: {:6} Alive: {}`, `{} (root, on {})`, `  (no attacks recorded)`); the body-map art is UNKNOWN |
| helpers | — | — | `attack_rows`, `attack_names`, `creature_name`, `attack_statuses`, `resolution_line`, `compact_creature`, `resolution_lines`, `creature_exist_name`, `hits_of`, `crit_str`, `wound_map` |

### `report.rs` (19 stubs + 7 helpers)

| fn | address | status | notes |
|---|---|---|---|
| `clock` | 0x101352814 | full | `{:02}:{:02}:{:02}` over `at + eastern::offset(at)` |
| `stamp` | 0x1013528f8 | full | `{} {}` of `eastern::format(date)` and `clock` |
| `duration` | 0x101354e88 | full | `open` / `{}s` / `{}m{:02}s` |
| `fraction` | 0x101354f60 | full | `{}/{} ({:.1}%)` / `{}/{} (-)` |
| `grid` | 0x101351d38 | full | `+{}+` / `|{}|`, `+` and `|` joins |
| `section` | 0x1013539bc | full | `vec![title]` + `grid` |
| `Hunts::sql_in` | 0x101352698 | full | `IN ({})`, nine-byte `IN (NULL)` when empty |
| `Inbound::said` | 0x101353020 | full | `{} ({} landed, {} missed, {} intercepted)` |
| `resolve` | 0x10135306c | full | session id statement 0x102412509; the five scope labels |
| `inbound_of` | 0x101350470 | partly | statement 0x103e67557's two trailing predicates are caller-supplied and not visible |
| `hunt_row` | 0x101354ff8 | partly | four columns read via `Row::get`; the column *order* is inferred from the neighbouring heading constants |
| `sessions` | 0x101355504 | full | `Hunts - {} (cstats <id> for one):` + `section` |
| `session` | 0x101353b18 | partly | all headings recovered; creature/`full` sub-table layout inferred |
| `totals` | 0x101352a90 | full | the four statements (attacks/sequences, kills, dealt, taken) |
| `abilities` | 0x101355d60 | full | three tables, heads recovered (`Ability Landed Dmg Avg Crits Fatal`, `Flare Procs`, `Attack N Missed Intercepted`) |
| `aggregate` | 0x101356848 | partly | headings recovered; `Per hour` formatting is `{:.1}` (spec byte partly decoded) |
| `effects_line` | 0x101350954 | full | `  Up: {}` + `  {} more came up during the hunt {}` |
| `gemstones_line` | 0x101351534 | full | `  Gemstones: {}` and `{} {}/{}` rows |
| `worn_line` | 0x1013581a8 | full | `  Worn: {}` + `  Worn list changed at {}` |
| `kit_lines` | 0x101358010 | full | gemstones, worn, effects in that order |
| `quick_actions` | 0x101350ee8 | full | ten words, `  {:}  {:} …` |
| `linked` | 0x101352a84 | unknown | 12 bytes: a tail call into `<Vec<T> as SpecFromIter>::from_iter`; re-collects the argument |

### `listing.rs` (5 stubs + 1 helper)

| fn | address | status | notes |
|---|---|---|---|
| `flare_names` | 0x101380ef4 | full | `SELECT DISTINCT f.name … WHERE a.hunt_id {} AND f.ours = 1 ORDER BY f.name` |
| `paged` | 0x1013811dc | full | split on `(page `, then on `' '`/`','`, then `usize::from_str` |
| `Listing::words` | 0x101381a9c | full | `recent {}`, `page {}`, `per {}` |
| `Listing::from_words` | 0x10138154c | partly | only the three option words are consumed; `scope_from` supplies the scope |
| `attacks` | 0x101381d94 | full | titles, paging line, grid heads, `No attacks recorded.` |
| `creatures` | 0x101382a94 | full | title, paging line, heads, `No creatures recorded.`, both order words |

### `catalog.rs` (2 stubs)

| fn | address | status | notes |
|---|---|---|---|
| `Entry::command` | 0x101367dec | full | `cstats` + `' '` + `+0x10` field + `' '` + trimmed argument |
| `Entry::matches` | 0x101367fe0 | full | three fields joined and lower-cased; every query word must be a substring |

### `command.rs` (3 stubs + 2 helpers)

| fn | address | status | notes |
|---|---|---|---|
| `parse` | 0x1013924bc | partly | word vocabulary and the jump-table *lengths* recovered; the mapping from every word to its variant is inferred from the handler each comparison lands in |
| `run` | 0x101392108 | full | trim, skip `.`/`;`, six-byte case-insensitive `cstats`, else `{} is not a combat report.` |
| `answer` | 0x101393dc0 | full | 17-entry jump table at 0x10241455e (base 0x101393f00) + the `String`-payload variant `0x15` |
| `scope_from` | 0x101391784 | full | `between <a> <b>`, `last [<n>]`, `all`, a lone id |
| `help` | — | full | the 21-row help table at 0x102414701 plus the `SCOPE:` footer |

### `recorder.rs` (3 stubs + 7 helpers)

| fn | address | status | notes |
|---|---|---|---|
| `Recorder::open` | 0x10137580c | partly | `hydra_ledger::Ledger::writing`, an empty 0x1c8-byte recorder; the second argument is a two-`String` value (`Who`) |
| `Recorder::record` | 0x101375c6c | unknown | 10 188 bytes of line parsing; the *vocabulary* (outcomes, body parts, crit kinds, verbs, slots) and all six `INSERT`s are recovered, the grammar is not |
| `Recorder::close` | 0x101375a18 | partly | stamps `ended_at`, drops the connection (`COMMIT`, 0x102413ab9) |
| helpers | — | — | `ChunkWriter::{resolution, status_row, hit, kill, creature}`, `Recorder::{sync_effects, write_gemstones, sync_gear}` |

### `task.rs` (1 stub + 2 helpers)

| fn | address | status | notes |
|---|---|---|---|
| `start` | 0x101390df4 | partly | thread name `hydra-combat-log`, `RUST_MIN_STACK`, `combat log thread: {}`; `start`/`run` build a current-thread runtime and drive the async loop |
| `listen` | — | partly | the async loop: `tokio::sync::mpsc::Receiver<Sent>` in, `Recorder::record` per line, `combat log thread: {}` on failure |
| `relay` | — | partly | `broadcast::Receiver<hydra_session::command::Event>` → `Sent` (`Frames` → `Line`, `Over` → `Ended`) |
| helpers | — | — | `start::{{closure}}`, `listen::{{closure}}`, `task::Sent` |

---

## 3. Recovered schema

Column lists come from the `INSERT`/`UPDATE` statements; the `SELECT`s that read
them are quoted on each `store` type.

```sql
hunts(id, instance, character, started_at, ended_at)                    -- ids: 0x102412509
attacks(id, hunt_id, seq, occurred_at, name, parent, parent_weapon,     -- 0x102413413
        parent_confidence, via, creature_id, target_kind, attacker,
        attacker_exist_id, weapon, outcome, outcomes_all, aimed, ambush,
        attack_kind, inbound, orphan, foreign_caster, unowned, ours,
        chunk_seq, redirected_from, round, right_item, right_gear,
        left_item, left_gear)                                           -- (+root_attack_id,
                                                                        --  parent_attack_id 0x10241339a)
hits(id, attack_id, flare_id, hunt_id, creature_id, seq, damage,        -- 0x10241308e
     location, body_part, crit_type, crit_rank, wound_rank, fatal,
     amputated, secondary_location, secondary_rank, line_seq)
creatures(id, hunt_id, exist_id, noun, name, first_seen, last_seen,     -- 0x102413702
          killed_at, killed_by_attack_id, kill_credit)                  -- (+ 0x10241321c)
flare_rows(id, attack_id, seq, name_id, damaging, creature_id, weapon,  -- 0x10241328b
           outcome, ours, eligible, source)
statuses(id, attack_id, creature_id, flare_id, kind, status, action,    -- 0x102411004
         value, subject, source, spell_name, cause)
hunt_effects(id, hunt_id, name_id, began_at, ended_at)                  -- 0x10241378e
hunt_gear(id, hunt_id, gear_id, exist_id, slot, began_at, ended_at)     -- 0x1024139df, 0x102413a52
hunt_gemstones(id, hunt_id, number, description, name, rank, ranks)     -- 0x10241227f
names(id, name)                                                         -- hydra_ledger::name_id
```

`flares` in the reports is `flare_rows JOIN names`; `Store::flares` keeps the
joined `name` so the scans read like the SQL.

---

## 4. Recovered constants

### 4.1 Format strings (all decoded from the packed pieces)

```
{:02}:{:02}:{:02}                                 0x10241247b  report::clock
{} {}                                             0x103e67927  report::stamp
open | {}s | {}m{:02}s                            0x103e67cd1, 0x1024125cc  report::duration
{}/{} ({:.1}%) | {}/{} (-)                        0x1024125d9, 0x103e67cd5  report::fraction
+{}+ | |{}|                                       0x103e678e3, 0x103e678e9  report::grid
IN ({}) | IN (NULL)                               0x103e6791e, 0x102412471  Hunts::sql_in
{} ({} landed, {} missed, {} intercepted)         0x103e67ac7  Inbound::said
SESSION #{}                                       0x103e67b02
ALL {} SESSIONS                                   0x103e67af1
LAST {} SESSIONS (#{}-#{})                        0x103e67b0e
SESSIONS #{}-#{} ({})                             0x103e67b2a
last {} | between {} {}                           0x103e2accf, 0x103e2acd7
=== SESSION REPORT #{} ===                        0x103e67b41
Character: {}                                     0x103e67b5d
Started: {}                                       0x103e67b6b
Last Event: {}                                    0x103e67b77
Ended: {} ({})                                    0x103e67b86
  Total Attacks: {}    Sequences: {}    Inbound: {}            0x103e67b96
  Kills: {}    Assists: {}    Damage Dealt: {}    Damage Taken: {}  0x103e67bca
CREATURES ENCOUNTERED ({}):                       0x103e67c53
CREATURES ENCOUNTERED ({}) - {} lists every instance:  0x103e67c1c
{} ({})                                           0x103e67c70
#{}                                               0x103e2b314
  Up: {}                                          0x103e6776d
  {} more came up during the hunt {}              0x103e67788
  Gemstones: {}                                   0x103e67838
{} {}/{}                                          0x103e67848
  Worn: {}                                        0x103e6823a
  Worn list changed at {}                         0x103e68245
cstats {}                                         0x103e61cca
=== AGGREGATE REPORT - {} ===                     0x103e680a1
Hunts: {}    First: {}    Last: {}                0x103e680c0
Hunt time: {} (avg {} per hunt)                   0x103e680e3
  Per hour: {} kills    {} damage    {} attacks    {} taken   0x103e68104
{} hunts; {} lists them.                          0x103e68154
=== ABILITIES - {} ===                            0x103e67d02
cstats sessions {}                                0x103e68141
page {} of {}, {} a page, {} rows                 0x103e68bf7
Attacks, newest first - {} ({})                   0x103e68928
Attacks by damage - {} ({})                       0x103e68bc1
the last {}                                       0x103e68778
Creatures, {} - {} ({})                           0x103e68e10
recent {} | page {} | per {}                      0x103e68bde, 0x103e68be8, 0x103e68bf0
cstats attack #{}                                 0x103e66a70
=== ATTACK REPORT #{} ==={}                       0x103e66b64
Type: {:}{:}{:}                                   0x103e66b80
Outcome: {}                                       0x103e66b8b
Sequence: {}   Session: {}                        0x103e66b97
Occurred: {}                                      0x103e66bb2
{} (#{})                                          0x103e66bd9
{} (attacker: {})                                 0x103e66bbf
Target: {}                                        0x103e66be2
Attacker: {}                                      0x103e66bed
Flags: {}                                         0x103e66bfa
 (via {})                                         0x103e66dca
  Weapon: {}                                      0x103e66de0
  ({:} {:})                                       0x103e66dd5
R{}                                               0x103e674df
[{:}]                                             0x103e21ed5
Unknown creature '{}'. The nouns recorded:        0x103e66f4d
No creature with id {}.                           0x103e66f79
Index {} is out of range for {} ({} seen).        0x103e66f21
 Name: {}                                         0x103e672f4
 Noun: {:18} ID: {}   Session: {}                 0x102411939
First: {:18} Last: {}   Duration: {}             0x10241195e
, by {} #{:}                                      0x103e672fe
False ({:}{:})                                    0x103e6730a
  DMG: {:18} Hits: {:6} Alive: {}                 0x102411986
{} (root, on {})                                  0x103e67317
  (no attacks recorded)                           0x102411b26
  - {}                                            0x103e63a80
No hunt #{}.                                       0x103e67cc3
`{}` is not a cstats word; cstats help lists them. 0x103e69161
`{}` is not an attack id.                         0x103e69195
 say the root: cstats tree <id>.                  0x1024146c2
 say how many: cstats last <n>.                   0x1024146e2
{} is not a combat report.                        0x103e69146
combat log thread: {}                             0x103e69109
```

### 4.2 Fixed strings

```
No hunts recorded yet.          0x10240f13a   No matching hunt.        0x102410a75
No attacks recorded.            0x102413df0   No creatures recorded.   0x102413e44
no outcome recorded             0x102410a86   no target                0x102411bc7
newest first / by damage        0x102413ede   ALIVE / DEAD / DEAD (kill) / DEAD (assist)   0x102412599
INBOUND FOREIGN CASTER UNOWNED ORPHAN AIMED AMBUSH    0x102410f05
RESOLUTION: NONE\tPenalty:      0x102410b90   STATUS EFFECTS: NONE  (window)  0x1024110c8
FLARES: NONE [damaging]         0x1024111c3   SEQUENCE: single attack (no spawn tree)  0x102411240
CRIT / AMPUTATED / [FATAL]      0x102410aac / 0x10241127b / 0x102410bb0
By barrier (a block's share is of every attack; an absorb's, of what would have hit):  0x10240ec2f
(the game's intercept, barrier unnamed)                0x10240ebcd
blocks before the roll / Barrier / How / Attacks / stopped / Share / Endings  0x10240ebf4
Creatures (click one for DS/TD/UDF):                   0x102410647
Min Avg Median Max / UDF TD DS                         0x102410634
Nothing was seen up over these hunts.                  0x102410723
No barkskin absorbs recorded.                          0x10241074e
Barkskin, each bark cast (grouped by its absorbs: a crumble ends one, a gap past the cooldown starts another)  0x102410799
Swings and would-hit count every inbound attack between a bark's first and last absorb.                       0x102410842
ran out                                                0x102410899
With and without what was up for part of the swings:   0x10240f6e9
Effect / Down: procs/swings                            0x10240f6e9
No damage recorded for those creatures.                0x10240f870
By attacker (the most attacks first; …):               0x10240f0a1
By round: /   By the weapon in hand: 0x10240e3fa / (hands not known) 0x10240e1e2
Attempts / Outcomes                                    0x10240ddb8
Median-attempts / Median-seconds / P90-seconds / Median-damage / Aimed-kills / Unaided-kills  0x10240dea6
Fewer median attempts with aiming (positive favors aiming)  0x10240df14
solo observed / aimed / unaimed                        0x10240df4e
Hunt comparison (creature mix affects results)         0x102410201
cstats compare: say two hunt ids.                      0x1024103a9
Blind coverage by creature kind                        0x10241058a
Worn over the hunts: / Held as a swing was made:       0x10240fdf9 / 0x10240fcd3
Flares seen on it / No swing has its hands on record yet.  0x10240fcd3 (also 0x10240fcd9)
Proc rate by attack: / Procs / targets                 0x10240ffea
  No damage recorded (non-damaging flare).             0x102410003
[casts]cstats barkskin                                 0x10240f5c4
```

### 4.3 The help table (verbatim, 0x102414701)

The 21 rows plus the footer are in [`command::HELP`] and
[`command::HELP_FOOTER`]; the word column is padded to 28 characters.  Extracted
as one printable run from `0x102414701` to `0x102414cff`.

### 4.4 Vocabulary

```
outcomes    hit evade block parry warded ward_failed intercept unaffected fumble hindrance already_dead
body parts  right_eye chest abdomen right_arm left_hand right_hand right_leg nerves unspecified
            left_arm left_eye left_leg right_hand both_eyes        (with the game's "right eye" spellings)
crit kinds  puncture cold kick acid crush slash impact fire vacuum plasma steam grapple
swing verbs ambush waylay reverse_strike jab punch tier upon smite offucs bracket count
status      add remove / status dispel death spell_loss decent excellent last_own_hit resisted gemstone
slots       worn held gloves
barriers    barkskin mirror_image_def wthorns640_block sward319 edefenseiii414_lorebenefit
```

---

## 5. Evidence conflicts and things that could not be established

1. **`parent_confidence`** is written from `?7`; the column type is not visible in
   any recovered statement. It is carried as text.
2. **`Hunts`' field layout.** `sql_in` reads its id pointer at `+0x8` and its length
   at `+0x10`, while `resolve`'s `Ok` payload is 0x48 bytes with the result niche in
   the first word. The two readings cannot both be exact, so `Hunts { label, ids }`
   is the shape that satisfies `sql_in` and the labels; the extra words in
   `resolve`'s payload are not accounted for.
3. **`hunt_row`'s column order** is inferred from the heading constants that sit
   next to the format (`ID Status Attacks Damage`, `Ended At Kills/hr`) rather than
   from the four `Row::get` indices, which are only visible as ordinals.
4. **Format specs.** The spec word inside a `0xC3`-style placeholder decodes for
   some strings (`0x6900_0020` = zero fill, width in the first `u16`, which is how
   `{:02}:{:02}:{:02}`, `{:04}-{:02}-{:02}` and `{}m{:02}s` were established) but
   not for others, where the decoder reports a width like `{:00.9472}` — those are
   reported as plain `{}` and marked at the site.
5. **`catalog::Entry::matches`' conjunction.** The recovered code is a resumable
   `str::contains` search whose result is folded into a running flag; "every word
   must appear" is the reading that matches the flag's initialisation (a word that
   is not found exits `false`, running out of words exits `true`). A single failed
   word still exits the loop with `false`, which is why `.all()` is used.
6. **`Recorder::record`'s grammar.** Only the vocabulary and the six `INSERT`s are
   recovered; the parser accepts a documented subset and ignores everything else,
   which is what the original does with lines it does not recognise.
7. **`report::linked`** is a 12-byte tail call; its element type is inferred from
   the caller's `Vec<String>`.
8. **Row types vs the schema.** Three columns of `hydra_ledger`'s recovered
   migration do not agree with the rows this crate's statements imply, and the
   migration wins because it is what SQLite will accept: `hunts.instance` is
   `TEXT NOT NULL` (the ledger's own `Hunt` row says `instance: String`) while
   the recovered `SELECT id FROM hunts WHERE instance = ?1 …` (0x102412509)
   binds an integer, so [`store::Hunt::instance`] stays an `i64` and is stored as
   its decimal text; `attacks.attacker_exist_id` and `creatures.exist_id` are
   `INTEGER` while the recovered rows carry the line's text, so a non-numeric
   value becomes `NULL`; `attacks.weapon`, `attacks.redirected_from` and
   `flare_rows.weapon` are `TEXT` while the recovered rows carry name ids, so the
   id's decimal text is what is stored. Nothing else in the two descriptions
   conflicts, and all six round-trip for the values the recorder writes.
9. **`statuses`' two required columns.** The recovered [`store::Status`] row has
   neither `hunt_id` nor `occurred_at`, but both are `NOT NULL` in the schema, so
   `Store::status(hunt_id, at, row)` takes them from the caller.
10. **`command::parse`'s variant numbers above 16** (`0x11`, `0x14`, …) appear as
   immediates in the handler tails; `answer`'s jump table covers `0..=16` plus the
   `String` variant `0x15`. The mapping of `17..20` to `barkskin`/`intercepts`/
   `effects`/`gear` is inferred from the report each tail calls.

---

## Inferred wording

The recovered SQL, counts, column sets and the literal pool give the numbers and
most headings. A handful of surrounding report strings do **not** occur in the
binary and are therefore marked `// UNKNOWN:` at their site:

| function | invented fragment | what *is* recovered |
| --- | --- | --- |
| `analytics::flare` | `Flare: ` (named-flare heading) | `Proc rate by attack:`, the `Procs`/`targets` heads, the SQL |
| `analytics::gear` | `(none recorded)` | `Worn over the hunts:`, `Held as a swing was made:` |
| `analytics::compare` | `Difference: `, `Aim` | `Fewer median attempts …`, `Hunt comparison (creature mix affects results)` |
| `analytics::defense` | `Def - ` | the DS/TD/UDF grid heads and the observations |
| `analytics::intercepts` | `Barrier - ` | `By barrier (…)`, `blocks before the roll` |
| `detail::tree` | ` (root)` | `SEQUENCE: single attack (no spawn tree); hit  (this attack)` |
| `detail::creature`/`attack_detail` | `Target: no target` | `Target: ` and the missing-target branch |

Everything else quoted in the sources was checked against the binary's
`__cstring`/`__const` (format blobs were decoded with the piece-length scheme in
`hydra_exp/NOTES.md`).
