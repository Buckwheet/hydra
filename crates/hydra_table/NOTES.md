# `hydra_table` — reverse-engineering notes

Source: the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF); every address below is an unslid VA.  `hydra_table` was Hydra's GUI
hub: the seat table, the roster/teams files, the account passwords, the travel
command and the settings pages.

Legend:

* **ESTABLISHED** — read straight out of the instruction stream, the string
  literals, or the field offsets.  Every claim quotes its address.
* **INFERRED** — follows from the code shape or from a neighbouring recovered
  API, but the original's type or name is not in the binary.
* **UNKNOWN** — not recoverable.  The safest behaviour was chosen and marked
  `// UNKNOWN:`/`**UNKNOWN**` at the site.

Status: `cargo check -p hydra_table` and `cargo test -p hydra_table` pass
(**83 tests**), and `grep -R "todo!()\|unimplemented!()" crates/hydra_table/src`
is empty.

---

## 1. Tooling

Read-only recon, all from the worktree:

* `python3 ../recon.py syms|sizes|disas|bytes|str|findstr` — the shared helper.
* A throwaway per-function dumper in `$TMPDIR` that walks a function range and
  annotates `adrp`/`add` literal references with the bytes they point at.  This
  is how the packed `__TEXT` blobs (every help text, every `{}` format) were read.
* `objdump -d` over the **whole** binary (480 MB of listing) to answer "who
  calls this?" — this is how the crate's private functions and closures were
  found, and it turned out the binary has far more symbols than
  `symbols_demangled.txt`: **302** `hydra_table` symbols survive locally
  (`roster::load`, `teams::save`, `pages::section`, `table::start`,
  `store_claim::run`, …), and they are what let the public stubs be tied back to
  their callers.

### Two facts that unlocked the type recovery

* **`Option<String>`/`Option<Vec<_>>` niche.**  Checked against the local
  toolchain: a `String` is `(cap@0, ptr@8, len@16)` and `None` is
  `cap == isize::MIN`.  `Result<T, io::Error>` on a `String`/`Vec` payload uses
  **two** niches: `isize::MIN` for one variant and `isize::MIN + 1` for the other.
  That is exactly what `roster::find` writes (`0x10129cc80` stores `isize::MIN + 1`
  for the error arm, `0x10129de44` stores `isize::MIN` for "no entry"), which is
  how `Result<Option<Entry>, io::Error>` was identified.
* **Format strings** are stored as `<len><text>`, with a single `0xC0` byte for
  every `{}`; a final `0xC8` ends the string.  Every message quoted below was
  decoded that way, e.g. `0x103e61ad0` = `c0 09 " is kept."` = `"{} is kept."`.

---

## 2. Function table

`full` = signature, control flow and constants recovered; `partly` = the
contract, the literals and the call graph recovered, the body simplified where a
dependency is not reconstructed; `seam` = the behaviour lives in a crate that is
**not** part of the reconstruction (`hydra_board`, `hydra_ledger`,
`hydra_rewards`, `hydra_travel`, `hydra_ui`, `cena_map`, `keyring`, `rpassword`,
`tokio`, `egui`), so the recovered message/state layer is implemented and the
missing side is documented.

| module | function | address | status |
| --- | --- | --- | --- |
| `account` | `data_dir` | `0x1011f79dc` | full |
| `account` | `env_name` | `0x1011f7a6c` | full (delegates to `hydra_session::password`) |
| `account` | `saved` | `0x1011f777c` | full |
| `account` | `kept` | `0x1011f7568` | full (keychain → in-process store) |
| `account` | `keep` | `0x1011f7370` | full (keychain → in-process store) |
| `account` | `forget` | `0x1011f77d0` | full (keychain → in-process store) |
| `account` | `password` | `0x1011f7b68` | full (rpassword → stdin line) |
| `account` | `from_roster` | `0x1011f70f0` | full |
| `ledger` | `shared` | `0x10122549c` | seam (`hydra_ledger`) |
| `ledger` | `open` | `0x101225424` | seam (`hydra_ledger`) |
| `roster` | `key` | `0x10129cb80` | full |
| `roster` | `load` | `0x10129cd1c` | full |
| `roster` | `save` | `0x10129cf40` | full |
| `roster` | `find_in` | `0x10129dc2c` | full |
| `roster` | `find` | `0x10129cc44` | full |
| `roster` | `all` | `0x10129cb04` | full |
| `roster` | `record` | `0x10129d814` | full |
| `roster` | `forget` | `0x10129d45c` | full |
| `roster` | `nickname` | `0x10129e148` | full |
| `roster` | `favourite` | `0x10129e584` | full |
| `roster` | `Entry::of` | `0x10129d288` | full |
| `roster` | `Entry::card` | `0x10129d344` | partly (`+0x61` byte unexplained) |
| `teams` | `load` / `save` | `0x101213b60` / `0x101213dc4` | full |
| `teams` | `all` | `0x101212e34` | full |
| `teams` | `find` | `0x101212eb0` | full (it *removes* from the in-memory map) |
| `teams` | `keep` | `0x101213008` | full |
| `teams` | `forget` | `0x10121410c` | full |
| `session` | `crit_tables` | `0x1011cba30` | full |
| `session` | `supervised_with` | `0x1011cbb90` | seam (logs/sink/tokio spawn) |
| `reward_claim` | `key` | `0x101292600` | full |
| `reward_claim` | `connected` | `0x1012928f0` | seam (`hydra_board`) |
| `reward_claim` | `busy_accounts` | `0x1012924dc` | seam (`hydra_board`) |
| `reward_claim` | `current` | `0x1012927f0` | seam (`hydra_rewards`) |
| `store_claim` | `accounts` | `0x1012768a0` | seam (`hydra_ledger`) |
| `commands` | `words` | `0x101225f34` | full |
| `commands` | `help_notice` | `0x1012254dc` | partly (table truncated) |
| `commands` | `install` | `0x101225ff0` | seam (script text is built at run time) |
| `relay` | `in_its_symbols` | `0x1012243c8` | full |
| `relay` | `alone` | `0x10122464c` | full |
| `relay::pick` | `pick` | `0x10127d530` | partly |
| `relay::pick` | `chosen` | `0x10127d9fc` | seam (`egui`) |
| `lich_command` | `Lichs::lock` | `0x1011f0ef8` | partly |
| `lich_command` | `Lichs::once_ready` | `0x1011f0cd8` | partly |
| `lich_command` | `family` | `0x1011f1944` | seam (help lines lost) |
| `inventory_command` | `Load::start` | `0x10123846c` | seam (55 KB state machine) |
| `inventory_command` | `Load::heard` | `0x101237e64` | partly |
| `travel_command` | `already` | `0x101222dac` | full |
| `travel_command` | `listed` | `0x101222cb4` | full |
| `travel_command` | `rooms_meant` | `0x10121e604` | full |
| `travel_command` | `ways_listed` | `0x10121e7a0` | partly |
| `travel_command` | `way_named` | `0x101224140` | full |
| `travel_command` | `side_unknown` | `0x10121e8b8` | full |
| `travel_command` | `helm` | `0x101221100` | full |
| `travel_command` | `in_a_line` | `0x1012239b0` | partly |
| `travel_command` | `forgotten` | `0x1012237a0` | partly |
| `travel_command` | `notes` | `0x101222940` | partly |
| `travel_command` | `several` | `0x101223184` | full |
| `travel_command` | `to_whom` | `0x101223368` | partly |
| `travel_command` | `reachable` | `0x101223b78` | seam (`cena_map`) |
| `travel_command` | `ended` | `0x101221b08` | full |
| `travel_command` | `set_out` | `0x101222e1c` | partly |
| `travel_command` | `set_sail` | `0x101223430` | full (claim handling) |
| `travel_command` | `stop_walk` | `0x101223c80` | full (claim handling) |
| `travel_command` | `Claim::take` | `0x101221164` | full (claim registry) |
| `travel_command` | `Gigas::said` | `0x10122180c` | seam (`hydra_travel`) |
| `travel_command::sea` | `bank_holding` | `0x10121ea94` | partly |
| `travel_command::sea` | `charges` | `0x10121fe08` | seam (fare table) |
| `travel_command::sea` | `dock_npc` | `0x101220edc` | partly |
| `travel_command::sea` | `own_ship_here` | `0x10121ecc8` | seam |
| `travel_command::sea` | `passage` | `0x1012200c8` | partly |
| `travel_command::sea` | `set_out` | `0x101220acc` | full (messages) |
| `travel_command::sea` | `told` | `0x10121f350` | seam |
| `travel_command::sea` | `way` | `0x10121ee5c` | partly |
| `travel_command::sea::swim` | `stroke_again` | `0x10120ea58` | full |
| `travel_command::sea::swim` | `crossing` | `0x10120ebf8` | seam (`hydra_travel`) |
| `pages` | `who` | `0x101264510` | full |
| `pages` | `travel_settings` | `0x1012638f8` | full |
| `pages` | `log_settings` | `0x1012608e8` | partly |
| `pages` | `apply` | `0x10126467c` | partly (no pending-edit model) |
| `pages` | `chosen` | `0x101268784` | partly |
| `pages` | `pages` | `0x1012647c0` | partly (titles only) |
| `pages` | `sorting` | `0x10126c04c` | partly |
| `pages` | `symbols` | `0x10126c344` | partly |
| `pages` | `targetid` | `0x10126d674` | partly |
| `table` | `restarting` | `0x10129326c` | full |
| `table` | `show` | `0x1012933d8` | full |
| `table` | `with_game_runtime` | `0x101293358` | partly |
| `table` | `seated` | `0x101293410` | partly |
| `table` | `running` | `0x1012937f8` | partly |
| `table` | `settings` | `0x101293e6c` | partly |
| `table` | `Resume::arg` | `0x101294630` | full |
| `table` | `Resume::read` | `0x101294690` | partly (file name unknown) |
| `table` | `remember` | `0x1012949c8` | full |
| `table` | `forget` | `0x10129483c` | full |
| `table` | `nickname_set` | `0x101292e34` | full |
| `table` | `favourite` | `0x101294c54` | full |
| `table` | `forget_password` | `0x1012930e8` | full |
| `table` | `give_typing` | `0x101292970` | seam (`sync::start`) |

---

## 3. Recovered layouts

### `roster::Entry` (size `0x68`)

| offset | field |
| --- | --- |
| `+0x00` | `character: String` |
| `+0x18` | `account: String` |
| `+0x30` | `game_code: String` |
| `+0x48` | `nickname: Option<String>` (`None` = `isize::MIN` in the cap) |
| `+0x60` | `favourite: bool` |

Evidence: `Entry::of` (`0x10129d288`) clones `+0`, `+0x18`, `+0x30`, `+0x48` and
writes `false` at `+0x60`; `Entry::serialize` (`0x10129c9b4`) writes the fields
in the order `character` (9), `account` (7), `game_code` (9), `favourite` (9),
`nickname` (8, only when `Some`) — the names are in `__TEXT` at `0x10240c800` …
`0x10240c829`.

**UNKNOWN**: `Entry::card` (`0x10129d344`) also stores `self.favourite` in the
byte at `+0x61`, which cannot be a field of a five-field `Entry` (the serde
visitor says "Entry with 5 elements").  It is probably a `#[serde(skip)]` field
or a second card-only flag; nothing here reads it.

### `roster::File` (size `0x28`)

`characters: BTreeMap<String, Entry>` at `+0x00`, `schema_version: u32` at
`+0x18`.  `roster::save` stamps it to `1` before writing
(`mov w8, #1; str w8, [_, #0x18]!`, `0x10129cf68`).

### `teams::Team` (size `0x38`) and `teams::File`

`name: String` at `+0x00`, `members: Vec<String>` at `+0x18` (pointer `+0x20`,
length `+0x28`), `favourite: bool` at `+0x30`.  Evidence: `teams::keep` reads
`[team, #0x8]` for the name and `[team, #0x20]`/`[team, #0x28]` for a
`Vec<String>` of members, and `teams::find` stores the bool at `+0x30`.

`File` is `{ schema_version, teams }` — the `expecting` strings at `0x102408340`
name both.

### `Table`

`Sessions` at `+0x28`, a `LazyLock<Mutex<Option<String>>>` at `+0x108` (whose
value sits at `+0x118`), the account name at `+0x160`.

### Keys

| what | key | evidence |
| --- | --- | --- |
| roster entry | `lower(trim(game_code) + ":" + trim(character))` | `roster::key`, `0x10129cb80`; `record` passes `game_code` then `character` |
| team | `lower(trim(name))` | `teams::find`, `0x101212f0c` |
| password | `lower(trim(account))` | the four keychain calls |
| rewards claim | `lower(character + ":" + account)` | `reward_claim::key`, `0x101292600` |
| walk claim | `lower(trim(name))` | `Claim::take`, `0x101221164` |

---

## 4. Recovered files and messages

`roster.json` and `teams.json` live in the directory named by `HYDRA_DATA_DIR`
(else `data`), and are `serde_json` documents:

```json
{"schema_version":1,"characters":{"gs4:aelith":{"character":"Aelith","account":"main","game_code":"GS4","favourite":false,"nickname":"Aely"}}}
{"schema_version":1,"teams":{"crew":{"name":"Crew","members":["alpha","beta"],"favourite":false}}}
```

`roster::load`/`teams::load` treat a missing file as an empty one, any other I/O
error as itself, and a JSON error as
`io::Error::new(InvalidData, format!("{}: {}", path, error))` (`0x10129ce14`
passes `0x15` = `InvalidData`).  A second entry for the same account is refused
with `io::Error::new(InvalidInput, format!("{} is on more than one game ({}); say
which, as GAME:", what, games.join(", ")))` (`0x14` = `InvalidInput` at
`0x10129e080`, the `", "` separator at `0x10240c835`).

Every user-visible message implemented here is quoted with its address in the
source; the full list is in `src/table.rs`, `src/teams.rs`, `src/travel_command*`
and `src/pages.rs`.  The test suite asserts 30+ of them verbatim, including their
byte lengths.

---

## 5. `std`-only departures

| original | what it did | this build |
| --- | --- | --- |
| `keyring` | `account::{keep,kept,forget}` stored the password in the OS keychain, service `hydra`, user `lower(trim(account))` | a process-wide `BTreeMap` plus the original's own `HYDRA_PASSWORD_<ACCOUNT>` environment fallback.  The **only** thing lost is persistence across runs; the key, the trimming and the empty-password rule are the original's. |
| `rpassword` | `account::password` prompted without echo | prints the recovered prompt to stderr and reads one line from stdin **with echo** |
| `serde`/`serde_json` | `roster`/`teams` files | `src/json.rs`, a 300-line reader/writer that reproduces the field order, the escaping and the `BTreeMap` key order |
| `tokio` | `session::supervised_with`, the claim tokens, every `*::run` | synchronous: the session's directories/crit tables are built and returned, the walk claims are a `Mutex<BTreeMap>`, and nothing is spawned |
| `egui` | `pages::*`, `relay::pick::chosen`, `table::*` drawing | the page list, its recovered titles/help and the settings reads, with the drawing left out |
| `hydra_board` | `Sessions`/`Seat` (glance, open_windows, keep_window) | `src/seat.rs`: the same calls over an in-memory live-session list |
| `hydra_ledger` | the reward/store claim ledger | `crate::ledger` returns "no ledger", which every recovered caller already handles (`cbz x0`) |
| `hydra_rewards` | `schedule::Settings::read` | `reward_claim::current` reports no schedule |
| `hydra_travel`, `cena_map`, `hydra_map` | routes, rooms, fares, itineraries | the travel module keeps the recovered *wording* and takes the resolved route data from its arguments |
| `hydra_ui` | `hub::TeamCard`, `hub::Saved`, … | `teams::keep` takes the recovered `Team` layout instead (identical field offsets) |

Path dependencies (allowed, all local): `hydra_protocol`, `hydra_session`,
`hydra_net`, `hydra_model`.  `hydra_session::password` is reused for
`account::env_name`/the prompt so the two crates cannot drift; `hydra_model::crit`
supplies the real `CritTables` behind `session::crit_tables`.

---

## 6. Evidence conflicts and open questions

1. **`Entry::card`'s second bool** (`+0x61`) — see above.  The reconstruction
   keeps five fields.
2. **`roster::find_in`'s scan predicate.**  The `:`-split arm is fully recovered
   (key lookup).  The no-colon arm filters the whole map, and the predicate itself
   is inside a `SpecFromIterNested` monomorphisation (`0x10124cb24`) whose closure
   has no symbol.  `teams::keep` inlines the same byte-wise ASCII fold against
   `Entry.account` (`0x10121322c` … `0x101213278`), so the filter is modelled as
   `account.eq_ignore_ascii_case(what)`.
3. **`teams::keep`'s second argument** is an `Option<&str>` (the `csel x2, xzr,
   x22, eq` on an `isize::MIN` length slot).  Nothing in the 4.3 KB body reads it;
   it is accepted and ignored.
4. **`teams::find` removes** the team from the loaded map (`BTreeMap::remove` at
   `0x1011dd44c`) and never saves, so the removal is unobservable.  Reproduced as
   `remove`, with a note.
5. **`ledger::open`'s handle shape.**  The refcount bumps are at `[inner]` and
   `[inner + 0x200]`, and `open` returns two registers, which does not match
   `Arc<T>`.  Without `hydra_ledger` the type cannot be pinned down; both entry
   points answer `None` here, which is what the recovered callers (`cbz x0`) are
   written for.
6. **`pages::who`'s error arm** names the whole input
   (`format!("{} is not a roster name (GAME:Name).")`), not just the game prefix.
7. **`pages` section names.**  `travel`, `scrollback`,
   `general`, `log`, `themes` are recovered; which key holds the *current page*
   (`0x101268784`'s `pages::switch`) is not — this build uses `page`.
8. **`travel_command::ways_listed`** reduces a `0x38`-stride slice to a maximum
   over the field at `+0x00`; calling that field "steps" is inferred from
   `way_named`'s `{} step{}` text.
9. **`relay::pick::chosen`** and `relay::pick::pick`'s exact matching rules: the
   roster is walked and the name compared, but the 84 KB window also offers
   account and nickname matches, so all three are accepted.
10. **`commands::ENTRIES`** holds the 16 help rows whose name *and* description
    survive contiguously; the table continues past them.
