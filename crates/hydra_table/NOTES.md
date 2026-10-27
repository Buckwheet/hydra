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

Status: `cargo check -p hydra_table --offline` and
`cargo test -p hydra_table --offline` pass (**93 tests**), and
`grep -R "todo!()\|unimplemented!()" crates/hydra_table/src` is empty.  Phase 1
reconnected `serde`/`serde_json`, `keyring`, `rpassword`, `tokio` and
`tokio-util`; see §5 and §8.

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
`hydra_rewards`, `hydra_travel`, `hydra_ui`, `cena_map`, `egui`), so the recovered
message/state layer is implemented and the missing side is documented.
(`keyring`, `rpassword`, `tokio` and `tokio-util` were seams too and are real
again after Phase 1 — §5.)

| module | function | address | status |
| --- | --- | --- | --- |
| `account` | `data_dir` | `0x1011f79dc` | full |
| `account` | `env_name` | `0x1011f7a6c` | full (delegates to `hydra_session::password`) |
| `account` | `saved` | `0x1011f777c` | full |
| `account` | `kept` | `0x1011f7568` | full (real `keyring` + the env fallback) |
| `account` | `keep` | `0x1011f7370` | full (real `keyring`) |
| `account` | `forget` | `0x1011f77d0` | full (real `keyring`) |
| `account` | `password` | `0x1011f7b68` | full (real `rpassword`) |
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
| `session` | `supervised_with` | `0x1011cbb90` | partly (real `tokio` spawn; sink/`Live` not reconstructed) |
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
| `travel_command` | `claim_token` / `stopped` | `0x101d378b4` | full (real `tokio_util::sync::CancellationToken`) |
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
passes `0x15` = `InvalidData`; `error` is `serde_json`'s, which is what the
original passed through as well — §5.1).  A second entry for the same account is refused
with `io::Error::new(InvalidInput, format!("{} is on more than one game ({}); say
which, as GAME:", what, games.join(", ")))` (`0x14` = `InvalidInput` at
`0x10129e080`, the `", "` separator at `0x10240c835`).

Every user-visible message implemented here is quoted with its address in the
source; the full list is in `src/table.rs`, `src/teams.rs`, `src/travel_command*`
and `src/pages.rs`.  The test suite asserts 30+ of them verbatim, including their
byte lengths.

---

## 5. Phase 1 reconnection — what is real again

Every crate below is the workspace pin, declared as `x = { workspace = true }` in
this crate's `Cargo.toml` and used through `cargo --offline`.

| real crate (pin) | replaced | where | notes |
| --- | --- | --- | --- |
| `serde` 1.0.229 (derive) | `src/json.rs`'s hand-written readers/writers | `roster::{Entry, File}`, `teams::{Team, File}` | the derives are the four `serde_core::de::Visitor::expecting` symbols the binary still carries (see `src/lib.rs`); declaration order is the recovered `serialize_entry` order, so the files come out byte for byte as before |
| `serde_json` 1.0.151 | `json::{parse, write_text, write_file}` | `roster::{load, save, to_json}`, `teams::{load, save, to_json}` | `{"schema_version":N,"<map>":{…}}`, `BTreeMap` order, `serde_json`'s string escapes; a JSON error is still `io::Error::new(InvalidData, format!("{}: {}", path, error))` |
| `keyring` 3.6.3, feature `apple-native` | the process-wide `BTreeMap` | `account::{kept, keep, forget}` | the real OS keychain: service `hydra`, user `account.trim().to_lowercase()`.  `apple-native` is the store the arm64 Mach-O binary linked; without it `keyring` 3.6.3 falls back to its `mock` store, which keeps nothing between two `Entry::new` calls |
| `rpassword` 7.5.4 | the echoed `stdin` line | `account::password` | `rpassword::prompt_password(recovered prompt)`: raw mode, prompt on the TTY, no echo |
| `tokio` 1.53.1 (`full`) | "nothing is spawned" | `session::{spawn_supervised_with, supervised_with_async}` | the recovered step-5 `tokio` spawn, with the synchronous `supervised_with` kept for callers that have no runtime |
| `tokio-util` 0.7.19 | the `cancelled: bool` flag as the only cancellation | `travel_command::{claim_token, stopped}`, `Claim::stop_walk` | a real `CancellationToken` per walk; `stop_walk` cancels it, so a walk awaiting `token.cancelled()` actually stops |

`src/json.rs` is **deleted** and `pub mod json` with it: nothing outside this
crate ever used `hydra_table::json`, and the two files are the only JSON this
crate owns.  `roster::to_json`/`teams::to_json` keep their signatures and are now
`serde_json::to_string`.

### 5.1 Behaviour that changed as a direct result of using the real crates

1. **JSON error text.**  A broken file still becomes
   `InvalidData(format!("{}: {}", path, error))`, but the second half is now
   `serde_json`'s (`expected value at line 1 column 3`) instead of the old
   "… at byte N".  The message wrap itself is the recovered one
   (`0x10129ce14`), and the tests assert the path-first shape.
2. **Duplicate JSON keys.**  `src/json.rs` rejected them; `serde_json` keeps the
   last one, which is what the original (a `serde_json` reader) did.
3. **Missing keys.**  `schema_version` reads as `0`, `favourite` as `false`,
   `nickname` as `None`, `characters`/`teams`/`members` as empty — the same
   defaults the hand-written reader applied, now written as `#[serde(default)]`.
   Unknown keys are ignored, as before.
4. **`account::forget` of an account with no password** reports the keyring's own
   `NoEntry` text ("No matching entry found in secure storage") through
   `Err(String)`; the previous "nothing is kept for …" sentence was a placeholder
   marked `UNKNOWN` and is gone.  `table::forget_password` therefore prints
   `HYDRA_PASSWORD_X is not forgotten: No matching entry found in secure
   storage`, which is still the inferred wording of §7 with the reason filled in
   by the real crate.
5. **`account::password` with nothing kept now needs a terminal.**  It is
   `rpassword::prompt_password`, exactly as recovered; a headless caller gets the
   `io::Error` from rpassword instead of an echoed `stdin` line.
6. **A walk is stopped by a token, not by a flag.**  `travel_command::stop_walk`
   cancels the walk's `CancellationToken` before marking the `Claim`; the `Claim`
   value shape (`who`, `cancelled`, `token`) is unchanged, and
   `claim_token(who)` hands the walk loop the token to `select!` on.  A token
   that is cancelled is replaced the next time it is asked for, so a fresh walk
   for the same character starts live.

### 5.2 Departures that remain

| original | reconstruction | why it stays |
| --- | --- | --- |
| `egui` | `pages::*`, `relay::pick::chosen`, `table::*` drawing | **not in the workspace pins at all** (Phase 4 in `PLAN.md`: the binary's egui is probably a git dependency).  The page list, its recovered titles/help and the settings reads are kept; the drawing is left out |
| `hydra_board` | `src/seat.rs` (`Sessions`, `Seat`, `Live`): glance, open_windows, keep_window over an in-memory live-session list | a local seam, not a pinned external crate; it is Phase 1 L3b and mid-rewrite |
| `hydra_ledger` | `crate::ledger` returns "no ledger", which every recovered caller handles (`cbz x0`) | ditto — **rewritten in parallel**, so no path adapter was added: `ledger::open`'s handle shape (§6.5) cannot be pinned against a moving crate |
| `hydra_rewards` | `reward_claim::current` reports no schedule | ditto — rewritten in parallel |
| `hydra_travel`, `cena_map`, `hydra_map` | the travel module keeps the recovered *wording* and takes the resolved route data from its arguments | local seams / L3a, not the crates under reconnection here |
| `hydra_ui` | `teams::keep` takes the recovered `Team` layout instead (identical field offsets) | local seam |
| `hydra_host` | `Table::with_game_runtime` has no runtime handle | L3b; `hydra_host::Handle` is a local seam type, so a `tokio` handle is no longer the missing piece — the host crate is (§5.3) |

Path dependencies (allowed, all local): `hydra_protocol`, `hydra_session`,
`hydra_net`, `hydra_model`.  `hydra_session::password` is reused for
`account::env_name`/the prompt so the two crates cannot drift; `hydra_model::crit`
supplies the real `CritTables` behind `session::crit_tables`.

### 5.3 What `tokio` is now real for, and what is still synchronous

* **Real spawn.**  `session::spawn_supervised_with(account, character) ->
  JoinHandle<Supervised>` is the recovered step-5 spawn (on the blocking pool,
  because the reconstructed body is synchronous), and
  `session::supervised_with_async` awaits it.  `session::supervised_with` keeps
  its signature and stays synchronous — it is the entry point the other crates
  call and it does not require a runtime.
* **What the spawned body still does not do**: `hydra_net::sink::writer::
  SessionSink::create`, `hydra_session::player_log::channel::PlayerLog::new`,
  `hydra_net::behind::Live::new` and the relay.  Those siblings have no runnable
  form yet, so the task returns [`session::Supervised`] rather than running a
  session.
* **Real cancellation.**  `travel_command::claim_token(who)` is the walk's
  `tokio_util::sync::CancellationToken`; `Claim::stop_walk` cancels it and
  `travel_command::stopped(who)` reports it.  The registry itself is still a
  `Mutex<BTreeMap<_, _>>` rather than `tokio::sync::Mutex` + `HashMap`, because
  `Claim::take`/`set_out`/`stop_walk` are synchronous public functions with the
  recovered signature.
* **Still synchronous**: the `*::run` runners (`store_claim`, `reward_claim`,
  `inventory_command::Load`) — they were never reconstructed here at all, so
  there is nothing to spawn; `table::give_typing` (`sync::start`) likewise.

### 5.4 How the keychain is tested without touching the user's Keychain

`keyring` builds entries with whichever credential store its features select, so
a test against the default builder would write real credentials into the login
Keychain.  `account::test_keychain` (test-only, `#[cfg(test)]`) therefore
installs an **in-memory `CredentialBuilderApi`/`CredentialApi` pair** as
keyring's default builder, once, before any entry exists, and hands back a guard
that serialises the password tests and empties the store.  Entries are keyed by
`(service, user)`, exactly like the real store, so the tests still assert the
recovered service name `hydra` and the `lower(trim(account))` user.
`table::forget_password` uses the same helper.  Consequences:

* nothing is written to the real keychain by `cargo test`;
* `account::password`'s **interactive** path (rpassword with a TTY) is not
  exercised: the tests only drive the "already kept" branch, and separately drive
  rpassword's own configurable entry point (`input_data` + an output file) with
  the recovered prompt text, which proves the prompt without needing a terminal;
* no test can block on stdin.

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
11. **`keyring`'s platform store.**  The binary is an arm64 macOS program, so
    `apple-native` (the login Keychain) is the store it used; the crate's
    `linux-*`/`windows-native` stores would be the equivalents on those targets
    and are not enabled here.  What the recovered code does *not* settle is
    whether the original also used a keyring *target* string — every one of the
    four calls passes only `(service, user)`, so no target is set.

## 7. Inferred wording

Four seams print sentences whose exact text is not in the binary's literal pool;
each is marked `// UNKNOWN:` at the site and is not presented as evidence:

| site | inferred text | recovered around it |
| --- | --- | --- |
| `commands::install` | `… is not a folder` | the folder resolution and `lich.rbw` markers |
| `table::forget_password` | `… is not forgotten: …` | `… is forgotten.` and `env_name` |
| `pages::{load_file,apply}` | `… has no settings path` | `who`, `settings_path`, `load` |
| `pages::TARGETID_KEYS` | `creature_tags` | `targetid` and the "creature tags" messages |

`account::forget`'s no-entry message is no longer invented: with the real
`keyring` back it is the crate's own `NoEntry` text, and the `UNKNOWN` marker is
gone from `src/account.rs` (the sentence in §7's `table::forget_password` row now
has the keyring's reason after it).

---

## 8. The Phase 1 reconnection pass

`cargo check -p hydra_table --offline`, `cargo test -p hydra_table --offline`
(**93 passing, up from 83**) and `cargo check --workspace --offline` are clean
after the pass, and `grep -Rn "todo!()\|unimplemented!()" crates/hydra_table/src`
is still empty.

Reconnected: `serde`/`serde_json` (both files), `keyring` (with `apple-native`),
`rpassword`, `tokio` (the session spawn) and `tokio-util` (the walk tokens).  Not
reconnected, deliberately: `egui` (**not in the workspace pins** — Phase 4),
`hydra_board`, `hydra_ledger`, `hydra_rewards`, `hydra_travel`/`cena_map`/
`hydra_map`, `hydra_ui` and `hydra_host` (local seams, none of them a pinned
external crate; `hydra_ledger` and `hydra_rewards` are being rewritten in
parallel, so no path adapter was added).

### Not applicable to this crate

Two of the L2 examples do not exist here: this crate owns **no TOML manifest**
(the character settings file is `hydra_session::settings_store`'s, reconnected in
that crate), and there is **no async relay** to reconnect — `crate::relay` is the
name-picking seam (`relay::pick::chosen` is an `egui` seam) and the real relay
lives in `hydra_lich`/`hydra_net`, both already reconnected in L1.

### Public API

Nothing was removed except the private-in-practice `hydra_table::json` module
(and `src/json.rs`):

* `roster::{Entry, File}` and `teams::{Team, File}` gained `Serialize`/
  `Deserialize` derives; the fields, their order and their types are unchanged,
  and `#[serde(default, skip_serializing_if = …)]` reproduces the old leniency.
* `account::{keep, kept, saved, forget, password}` keep their signatures and
  error types (`Result<(), String>`, `Option<String>`, `io::Result<String>`).
* `session::spawn_supervised_with` and `session::supervised_with_async` are new;
  `session::supervised_with` is unchanged.
* `travel_command::{claim_token, stopped}` are new; `Claim`, `Claim::take`,
  `Claim::set_out`, `Claim::stop_walk`, `set_sail` and `stop_walk` keep their
  signatures and values.  `Claim::stop_walk` now also cancels the real token.
* `roster::to_json` / `teams::to_json` keep their signatures (bodies are now
  `serde_json::to_string`).

### Integration risks for the parent

1. **`Cargo.lock`.**  Adding `keyring`/`rpassword` (+ `security-framework`,
   `rtoolbox`, `zeroize`, …) re-resolved a few Windows-only edges
   (`windows-sys 0.52.0` → `0.61.2` for `rustix`/`tempfile`/`nix`); no pinned
   version was changed by hand and nothing that compiles on this target moved.
   Sibling worktrees that regenerate the lock will produce the same.
2. **`keyring`'s feature is crate-local.**  `apple-native` is declared in
   `hydra_table`'s dependency; if another crate later depends on `keyring`
   without it, feature unification keeps `apple-native` on (features are
   additive), so the store will not silently fall back to `mock`.
3. **`account::password` needs a TTY** when nothing is kept.  A headless
   bootstrap must call `kept`/`HYDRA_PASSWORD_*` first, as the original did.
4. **`hydra_table::json` is gone.**  No crate in this workspace used it; if the
   Phase 2 binary wants a document type, take `serde_json` directly.
5. **`session::Supervised`** still describes a session without running one: the
   spawn is real but the sink/`Live`/relay body belongs to `hydra_net` and
   `hydra_session`, so the Phase 2 binary has to run those itself.

