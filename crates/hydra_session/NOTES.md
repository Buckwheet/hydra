# `hydra_session` — reverse-engineering notes

Source: arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra`, symbols at
`symbols_demangled.txt`; every address below is an unslid VA.  This crate is a
**std-only** reconstruction of `hydra_session`: the original linked `tokio`,
`tokio_util`, `serde`/`serde_json`, `toml`/`toml_edit`/`toml_writer`, `regex`,
`flate2`, `zlib_rs`, `getrandom` and `hydra_model`/`hydra_net`/`hydra_protocol`.

> **Phase 1 (reconnection)**: `serde`/`serde_json`, `toml`/`toml_edit` (and
> `toml_writer` through `toml`'s `display` feature), `regex`, `flate2` with the
> `zlib-rs` backend, `tokio` and `tokio-util` are now the real crates at the
> workspace pins.  See §4 for the departures that are gone and the ones that
> stay, with the reason for each.

Legend:

* **ESTABLISHED** — directly visible in the disassembly, the string literals, or
  the field offsets.  Quoted addresses can be re-read with
  `python3 /tmp/hydra_disasm.py addr 0x…`.
* **INFERRED** — follows from the code shape or from a neighbouring recovered
  API, but the original type/name is not in the binary.
* **UNKNOWN** — not recoverable; the safest behaviour was chosen and is marked
  `// UNKNOWN:` in the source.

## 0. How literals were read

`__TEXT,__cstring` format strings are stored as length-prefixed pieces with a
single `0xC0` byte for every `{}` argument, e.g. at `0x103e7bf3a`:

```
01 '[' c0 02 '][' c0 02 '] ' c0 01 '\n'      ==  "[{}][{}] {}\n"
```

So `<len><text>` is a literal piece and `<c0>` is the next argument.  Every
format string quoted in this crate's comments was decoded that way from
`/tmp/hydra_cstrings.txt` / `/tmp/hydra_lits_hydra_session.txt`.

## 1. Function table

`full` = behaviour recovered from the disassembly; `partly` = the entry
semantics (arguments/literals/return shape) are recovered but the body is
inlined or enormous; `helper` = the stub was a thin tail call into another
module, so the reconstruction is a documented equivalent.

| module | function | address | status |
| --- | --- | --- | --- |
| `store` | `safe_component` | `0x101aa1d9c` | full |
| `store` | `character_path` | `0x101aa1bbc` | full |
| `store` | `save_bytes` | `0x101aa16dc` | full |
| `settings_store` | `settings_path` | `0x101a059c0` | full |
| `settings_store` | `SettingsFile::new` | `0x101a058ac` | full |
| `settings_store` | `load` | `0x101a05ae8` | partly |
| `settings_store` | `save` | `0x101a060d0` | partly |
| `character_store` | `store_path` | `0x101aac818` | full |
| `character_store` | `Persistence::mark` / `take` | `0x101aac828` / `0x101aac8a0` | full |
| `character_store` | `merge_and_save` / `load` / `save` | `0x101aac8fc` / `…d5c` / `…268` | partly |
| `menu_store` | `load` / `merge_and_save` / `underlay` | `0x101a4f16c` / `0x101a4e960` / `0x101a4f984` | partly |
| `queue` | `admit` | `0x101a0c52c` | full |
| `queue` | `clear_waiting` | `0x101a0c308` | full |
| `queue` | `open_window` / `written_at_once` | `0x101a0c0f0` / `0x101a0c48c` | full |
| `queue` | `close_window` / `offer` / `answer_all` | `0x101a0c1a4` / `0x101a0c5b8` / `0x101a0bb3c` | partly (audited: entry shapes recovered, `answer_all` also drains `waiting`) |
| `queue` | `drop_in_place<Owed>` / `drop_in_place<Envelope>` | `0x101a3ff2c` / `0x101a42220` | full (layout evidence) |
| `queue` | `take_next` | `0x101a0c764` | partly (60 KB; entry semantics) |
| `supervisor` | `jitter` | `0x101a0ba08` | full (audited: `f64` fraction) |
| `supervisor` | `backoff` | `0x101a0ba90` | full (audited: table `1,2,5,10,30`) |
| `supervisor` | `answer_between` | `0x101a0ad6c` | helper |
| `lich` | `lock` | `0x101a884e0` | full |
| `lich` | `Slot::{takes_typing, hand, wire, begin, close, waits}` | `0x101a88014`…`0x101a8846c` | full |
| `lich` | `Wire::{fell_behind, begin}` | `0x101a884bc` / `0x101a884d0` | full |
| `lich` | `Typing::waiting` | `0x101a88554` | full |
| `lich` | `LichDoor::{symbol, lich_symbol, say, session_over}` | `0x101a89030`…`0x101a88608` | full |
| `lich` | `LichDoor::claim` | `0x101a88960` | full (capacities 4096/64) |
| `lich` | `LichDoor::show` | `0x101a8860c` | partly (`lich::text` is not in the table) |
| `lich` | `SessionHandle::{hand_to_lich, to_lich, lich_door, lich_running, lich_takes_typing}` | `0x101a38d58`…`0x101a3923c` | full |
| `command` | `generation` / `set_symbol` / `symbol` / `lich_symbol` | `0x101a39308`…`0x101a3c89c` | full |
| `command` | `say` / `say_unlogged` / `chain_stopped` | `0x101a3c0ec` / `0x101a399f0` / `0x101a39a3c` | full |
| `command` | `clip` / `set_clip` / `keep_log` / `stopping` / `log_typed` | `0x101a3c1b8`…`0x101a3d51c` | full |
| `command` | `claim` / `set_desk` / `set_taker` / `set_aliases` / `set_lich_view` / `set_lich_symbol` / `lichs` | `0x101a3c2cc`…`0x101a3e144` | full (shape) |
| `command` | `new` | `0x101a3bc9c` | partly |
| `command` | `command_words` / `run_steps` | `0x101a39b08` / `0x101a3d710` | partly |
| `command` | `fold_in` | `0x101a3ca04` | full |
| `command` | `subscribe` | `0x101a3e2a0` | helper (22 KB of `select!`) |
| `alias` | `character` | `0x101aafcf8` | full |
| `alias` | `AliasFile::{of, put, remove, load, expand}` | `0x101aae0c8`…`0x101aaf7e4` | full / partly (`expand`) |
| `alias` | `Aliases::{for_character, expand, problem}` | `0x101aad5fc` / `0x101aad79c` / `0x101aadc60` | full |
| `steps` | `pieces` | `0x101ab0c08` | partly (audited: `,` and `\` recovered; `[`/`]` UNKNOWN) |
| `steps` | `steps` / `said` / `wait` | `0x101ab01e8` / `0x101aafedc` / `0x101aaffb8` | full (audited) |
| `steps` | `behaviour_said` | `0x101aafd40` | helper (not modelled) |
| `notes` | `notes_path` / `key_for` / `forget` / `remove` / `rewrite` / `add` / `add_lines` / `write` | `0x101a52988`…`0x101a54ed0` | full |
| `notes` | `load` / `Notes::{get, find, search}` | `0x101a53bbc` / `0x101a53f28` / `0x101a540b8` / `0x101a548b0` | full |
| `notes` | `Note::{first_line, clipped}` / `Clip::of_room` | `0x101a53a90` / `0x101a53b74` / `0x101a52e20` | full |
| `notes` | `stamp` | `0x101a5496c` | helper (`hydra_net::sink::config::moment`) |
| `claimant` | `usable` | `0x101a807ec` | full (`char::is_ascii_punctuation`) |
| `claimant` | `Desk::{new, with_words, set_lich_symbol, claim}` | `0x101a803bc`…`0x101a8066c` | full |
| `claimant` | `Settings::{symbol, lich_symbol}` | `0x101a80890` / `0x101a80818` | full |
| `lifecycle` | `GenerationCell::{get, advance}` | `0x101a809dc` / `0x101a809e8` | full |
| `notice` | `closes_a_hunt` / `help` / `linked` | `0x101a7a82c` / `0x101a7a8f4` / `0x101a7aa68` | full / partly (`help` text, `closes_a_hunt` wording) |
| `stream_rules` | `Compiled::{new, streams}` | `0x101a04e34` / `0x101a05084` | full |
| `stream_rules` | `Rule::{check, finds, fighting, like}` | `0x101a033d0`…`0x101a036a0` | full / partly (`like`) |
| `stream_rules` | `Rules::{load, parse, to_toml}` | `0x101a048c0` / `0x101a04b6c` / `0x101a04c94` | partly (member names UNKNOWN) |
| `stream_rules` | `StreamRules::{new, set, current}` | `0x101a01f30` / `0x101a02048` / `0x101a02218` | full |
| `stream_rules` | `fought` / `like` | `0x101a04dcc` / `0x101a03090` | full |
| `player_log` | `class` / `sender` | `0x101a58f54` / `0x101a5e230` | full / helper |
| `player_log::writer` | `file_key` / `file_name` / `day_path` / `piece` / `day_of` / `dir` / `days` | `0x101a52158`…`0x101a51c1c` | full |
| `player_log::writer` | `parse_line` / `scrubbed` / `adopt_unscoped` | `0x101a50dd4` / `0x101a522e0` / `0x101a514e8` | full |
| `player_log::writer` | `root` | `0x101a51d44` | helper (`log_dir`) |
| `player_log::writer` | `PlayerWriter::{write, flush}` | `0x101a50f3c` / `0x101a50f00` | full |
| `player_log::reader` | `days` / `read_day` / `read_day_from` / `tail` / `around` / `window` | `0x101a5ac18`…`0x101a5cf8c` | full / partly (`around`, `window` args) |
| `player_log::reader` | `search` / `search_days` / `tagged` / `export` / `export_path` / `sort_again` | `0x101a5c66c`…`0x101a58ffc` | full |
| `player_log::reader` | `Pattern::{literal, regex, matches}` / `Streams::admits` | `0x101a5d9d0`…`0x101a5dc84` | full |
| `player_log::links` | `add` / `once` / `read` / `written` / `Does::of` | `0x101a50418`…`0x101a50bc0` | partly (JSON member names UNKNOWN) |
| `player_log::archive` | `path` / `names` / `manifest` / `archives` / `usage` / `period` / `from_word` | `0x101a64140`…`0x101a671f0` | full / partly |
| `player_log::archive` | `sweep` | `0x101a649c4` | partly (Phase 1: real gzip via `flate2`) |
| `player_log::retention` | `doomed` / `preview` / `prune` | `0x101aa09f4` / `0x101aa1558` / `0x101aa06b8` | full |
| `player_log::channel` | `PlayerLog::{new, with_capacity, typed, notice, close, dropped, redact}` / `LogSink::{dropped, try_recv}` | `0x101a873c8`…`0x101a87f20` | full |
| `player_log::feed` | `Feed::{frame, command, set_rules, take_noticed, watch, write_to, close}` | `0x101a86790`…`0x101a84dec` | partly |
| `actor` | `is_exit_intent` | `0x101a675f4` | full |
| `actor` | `pushed_commands` | `0x101a677d0` | partly (payload shape) |
| `saves` | `Saves::heard` | `0x101a6a688` | partly (recognised texts INFERRED) |
| `bestiary` | `asked` / `answered` / `room_uid` | `0x101a7aaf8` / `0x101a7f850` / `0x101a800b4` | full texts / partly |
| `lookout_seed` | `seed` | `0x101aaa614` | partly |
| `password` | `env_name` / `prompt` / `is_prompt` / `kept` | `0x1011f76a6c` / `0x1011f7b68` (in `hydra_table`) | full |

New modules that have no stub of their own:

| module | why |
| --- | --- |
| `textfmt` | the document type the file stores share (`serde` drives the actual JSON/TOML) |
| `pattern` | the compiled-pattern facade over `regex` (`Pattern`'s API predates the reconnection) |
| `password` | the `HYDRA_PASSWORD_*` / `Password for account {}: ` helpers, recovered from `hydra_table::account` |

## 2. Recovered APIs

### 2.1 `store` — paths and durable writes (ESTABLISHED)

```rust
pub fn safe_component(s: &str) -> String;                 // ascii-alnum only, lower-cased
pub fn character_path(root: &Path, a: &str, b: &str, ext: &str) -> Option<PathBuf>;
pub fn temp_extension(path: &Path, pid: u32, hash: u64) -> String;
pub fn save_bytes(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
```

`safe_component` is `chars().filter(is_ascii_alphanumeric).map(to_ascii_lowercase)`
(read step by step from the shared `Map::fold` at `0x101a9f48c`: the `sub #0x30`
/ `sub #0x41` range tests and the `orr #0x20` + `csel` fold).  A component with
no alphanumerics becomes empty, and `character_path` then returns `None` (the
`Option` niche word `0x8000000000000000` at `0x101aa1cdc`).

`save_bytes` is `create_dir_all` → `with_extension("json.<pid>-<hash>.tmp")` →
`write_all` → `sync_data` → `rename` → directory `sync_data`, and it removes the
temporary file when the rename fails.

### 2.2 File formats

| file | format | recovered shape |
| --- | --- | --- |
| `<instance>_<character>.settings.json` | JSON | `schema_version`, `instance`, `character`, `sections` (in that order) |
| `<instance>_<character>.json` | JSON | character state (`schema_version`, `character`, `sections`) |
| `<instance>_<character>.json` | JSON | aliases (`schema`, `aliases`) |
| `notes.json` | JSON | `schema` (up to **2**) + `notes` |
| `links.json` | JSON | `schema` + `links` (`name`, `day`, `line`) |
| `menu.tsv` | text | `version<TAB>verb<TAB>noun<TAB>confirmed` per line |
| `<character>/<YYYY-MM-DD>_<character>.log` | text | `[HH:MM][stream] text` per line |
| `<period>.gz` + `<period>.manifest` | gzip + text | one gzip member per day file; `name<TAB>uncompressed-size` manifest lines (Phase 1; a pre-Phase-1 archive is a plain concatenation and is still read) |
| `rules.toml` | TOML | `[[rules]]` with `name`, `stream`, `class`, `regex`, `fighting` |

Only `notes.json`, `.settings.json`, `{}_{}.log`, `{}_to_{}`, `{}.{}.partial`,
`{}.gz`, `{}\t{}\n`, `week-{}`, `{}\t{}\t{}\t{}\n`, `HYDRA_PASSWORD_{}` and
`Password for account {}: ` are **recovered** names/format strings; the JSON
*member* names inside `notes.json`, `links.json`, `*.json` (aliases, character
state) and `rules.toml` are **UNKNOWN**, chosen to match the recovered
behaviour (see each module's doc comment).

### 2.3 `queue` — FIFO with owed windows (ESTABLISHED)

```rust
pub const WINDOW_OPEN: u32 = 1_000_000_000;   // 0x3b9aca00
pub const WINDOW_TAKEN: u32 = 1_000_000_001;  // 0x3b9aca01
pub const WINDOW_STALE: Duration = Duration::from_secs(30);

pub struct CommandQueue { waiting: VecDeque<Envelope>, owed: VecDeque<Owed> }
impl CommandQueue {
    pub fn admit(&mut self, envelope: Envelope);       // push_back, no cap
    pub fn clear_waiting(&mut self) -> usize;          // drains, sends `Outcome::Cleared` (tag 6)
    pub fn open_window(&mut self, key: u32) -> Receiver<Outcome>;
    pub fn written_at_once(&mut self, key: u32) -> Receiver<Outcome>;
    pub fn close_window(&mut self, answer: Answer) -> Option<Outcome>;
    pub fn offer(&mut self, answer: Answer) -> bool;
    pub fn answer_all(&mut self, answer: &Answer) -> usize;
    pub fn take_next(&mut self, key: u32) -> Option<Envelope>;
}
```

`CommandQueue` is exactly two `VecDeque`s (element sizes `0x28` and `0x20`), the
window tags are `1e9` / `1e9 + 1`, and a window older than 30 s
(`cmp x0, #0x1e` on `Instant::elapsed`) is dropped.

Re-checked in the audit pass (§6).  What the disassembly pins down:

* **`Envelope` (40 bytes)** — `drop_in_place` (`0x101a42220`) drops a `String` at
  `+0x00` (a `__rust_dealloc(ptr, cap, 1)` with cap `+0x00`) and an
  `Option<oneshot::Sender<Outcome>>` at `+0x18`; the last word `+0x20` has no
  drop glue.  `admit` copies exactly 40 bytes, `clear_waiting` sends
  discriminant **6** to the `+0x18` sender and returns the drained length.
* **`Owed` (32 bytes) is an enum, not a plain struct** —
  `drop_in_place<Owed>` (`0x101a3ff2c`, duplicated at `0x101a12944`) first reads
  a `u32` at `+0x18` and **returns early when it is `1e9`**, otherwise drops an
  `Arc`-based `Option<oneshot::Sender<Outcome>>` at `+0x00` and an
  `Option<Box<Frame>>` at `+0x08`.  `1e9`/`1e9 + 1` are not valid nanoseconds,
  which is why they work as variant tags in the same slot as the `Instant`
  nanoseconds.
* Its three states and their constructors:
  * `written_at_once` (`0x101a0c48c`) builds `{ at: Instant@0x00, tag = 1e9@0x18 }`
    (the `0x10` word is *poison* — nothing is stored there), i.e. "written, no
    answer owed to anyone";
  * `open_window` (`0x101a0c0f0`) builds `{ reply@0x00, frame = None@0x08,
    at: Instant@0x10, at.nanos@0x18 }` — so it takes the **reply sender as its
    argument** (`x1` is stored straight into `+0x00`) rather than creating the
    channel; the valid nanosecond value is what tags this variant;
  * `close_window` recognised the `1e9`/`1e9+1` tags by masking
    (`and w9, w9, #0x3ffffffe`) before deciding whether to send.
* `offer` (`0x101a0c5b8`) walks the owed queue, drops `1e9`-tagged entries whose
  `Instant::elapsed` is `>= 30` s, and stores a cloned `Box<Frame>` into
  `+0x08` of the newest entry that has none.
* `answer_all` (`0x101a0bb3c`) drains **both** deques: first the owed queue
  (sending the answer's frame to entries that are not already tagged), then the
  *waiting* queue, answering each envelope's `+0x18` sender.  The crate's
  reconstruction only touches the owed queue, so this is the remaining **gap**.

The reconstruction keeps the documented std-only model (`Owed` as a plain
struct, `open_window`/`written_at_once` creating the channel and taking a `key`)
because turning it into an enum and moving the sender to the caller would change
the crate's public API.  The constants, the size, the field offsets and the
30-second rule are exact; the *shape* of `Outcome` and of `answer_all` is not.

### 2.3a `supervisor` — retries (ESTABLISHED in the audit pass)

`jitter` (`0x101a0ba08`) takes **no argument** and returns an `f64`:

```
SystemTime::now().duration_since(UNIX_EPOCH)  -> Err => 0.0
micros = subsec_nanos / 1000                  (umull 0x10624dd3, lsr #38)
fraction = (micros % 1000) as f64 / 999.0     (umull 0x418938, msub, fdiv 0x408F380000000000)
```

`backoff` (`0x101a0ba90`) takes `(attempt: u32, jitter: f64)`:

```
i    = min(attempt, 4)                        (cmp w0,#4 ; csel)
base = TABLE[i] * 1000                        (ms; TABLE @0x1039c5c60)
j    = clamp(jitter, 0.0, 1.0)                (fcsel x2)
ms   = (u64)((0.8 + 0.4 * j) * base)          (0.8 @0x101a0baec, 0.4 @0x101a0bad8)
out  = Duration { secs: ms / 1000, nanos: (ms % 1000) * 1e6 }
```

The table is five `u64`s: **`1, 2, 5, 10, 30`** seconds — not a doubling, and the
cap is 30 s, not 60 s.  `SupervisedSession::wait_before_retry` calls
`jitter()` and passes `d0` straight into `backoff` (`0x1000089cc` / `0x1000089d4`),
then bumps the attempt counter with a saturating `+1`.

The reconstruction was **wrong here** before the audit (1 s doubling capped at
60 s plus up to 250 ms additive jitter — an INFERRED doubling from an
instruction count).  It now uses the recovered table and the multiplicative
jitter, keeping the one-argument `backoff(u32)` and the `Duration`-shaped
`jitter(up_to)` so the public API is unchanged; `JITTER_MS` survives only as an
unused, documented constant.

### 2.3b `steps` — pieces, waits and rendering (ESTABLISHED in the audit pass)

```
pieces 0x101ab0c08  char loop comparing ',', '[', ']', '\'   => ',' splits, '\' escapes
steps  0x101ab01e8  pieces(line) then wait(piece) for every piece; Err => send as-is
wait   0x101aaffb8  first char == 's' (case-folded) and rest parses as f32,
                    need 0.0 <= s < 60.0, else the literal at 0x103e7cc12:
                    "`{}` waits from a moment to {} seconds" (second arg 60)
said   0x101aafedc  map steps to text, join with 7 bytes @0x1039c770b = ", then "
```

Step renderings (the `Map::fold` at `0x101a9e900`, element stride `0x30`):

| tag | rendering |
| --- | --- |
| 0 | the line itself |
| 1 | `format!("wait {} s", secs + nanos/1e9)` (f32 math) |
| 2 | `"wait for roundtime"` (23 bytes @0x1039c751b) |
| 3 | `"wait for cast roundtime"` (18 bytes @0x1039c7509) |
| other | `steps::behaviour_said` (`0x101aafd40`) — **UNKNOWN** |

Corrections made in the audit pass: the separator is `,` (was `;`), the wait
spelling is `s<seconds>` (was only `wait <seconds>`, kept as a tolerated extra),
and a wait renders as `"wait N s"` (was `"wait N seconds"` / `"wait a moment"`).
Still **UNKNOWN**: what `[`/`]` group, the roundtime step kinds (they need game
state, so `Step` models only send/wait), and the `behaviour_said` arms.

### 2.4 `claimant` — the desk (ESTABLISHED)

`Desk::new(symbol: Option<char>, character)` uses `'.'` when the symbol is the
`Option<char>` niche and starts Lich's symbol at `';'`.  `usable(c)` is
`c.is_ascii_punctuation()` — the four `add.4s`/`cmhi.4s` ranges at
`0x102268000` are 33..47, 58..64, 91..96, 123..126.  `Desk::claim(line)` strips
the leading command symbol.

## 3. The Lich door contract

This is what `hydra_lich`'s relay expects (`crates/hydra_lich/NOTES.md` §2).

### 3.1 Types (ESTABLISHED layout, std synchronisation)

```
Slot     { hand: Option<Tap>, wire: Option<Tap>, session_over: bool, … }
Tap      { tx: Sender<String>, wire: Wire, typing: bool, waiting: bool }
Wire     { flag: Arc<AtomicBool> }
LichDoor { session: SessionHandle }        // forwards; the binary tail-calls
Typing   { pending: Vec<String> }
```

The recovered `Slot` is `+0x08` a `Tap` (0x20 bytes), `+0x28` a second `Tap`,
`+0x48` the `session_over` bool; each `Tap` is a nullable channel pointer at
`+0x00`, the sender at `+0x08` and an `Arc` whose `+0x10` byte is the
"fell behind" flag.

### 3.2 `LichDoor` (`claim`, `say`, `symbol`, `lich_symbol`, `show`, `session_over`)

```rust
let door = session.lich_door();
let claim = door.claim();          // 4096-slot line channel, 64-slot typed channel
// relay: child stdout ──> claim.lines ;  claim.typed ──> child stdin
door.show("a line of Lich output");// parse + fold the frames into the session
door.say(&Notice::line(2, "…"));   // the player sees it, it goes to the log
let c = door.symbol();             // '.' when the session has no symbol
door.session_over();               // true once the slot is closed
```

* `claim` creates the two channels the original allocates
  (`Semaphore::new(0x1000)` and `Semaphore::new(0x40)`) and installs the two
  taps; the *line* end is handed to the relay (it pushes Lich's output) and the
  *typed* end is pulled by the relay to write to the child's stdin.
* `symbol()` maps the `Option<char>` niche `0x110000` to `'.'`
  (`csel w8, #0x2e, w0, eq` at `0x101a89044`).
* `session_over()` reads the bool through the door's shared lock.
* `show()` is where `hydra_session::lich::text::LichText::read` used to run; that
  parser is not in this crate's symbol table (`crates/hydra-session/src/lich/text.rs`
  appears in `__TEXT`), so the line's own text is the frame.  **UNKNOWN**: the
  frame/mark encoding.

### 3.3 `SessionHandle` Lich methods

```rust
session.lich_running();          // a tap exists
session.lich_takes_typing();     // hand tap, or wire tap with the typing flag
session.hand_to_lich(line);      // says one of the two recovered notices on failure
session.to_lich(line);           // quiet
session.wire_to_lich(line);      // the mirrored game stream
```

Recovered failure notices (`hydra_session` literals):

```
Lich is not running for this character: not sent: {}                       level 2
Lich is not reading what is typed for it ({} lines wait): not handed: {}    level 1
```

### 3.4 Token handshake — OBSERVED (`docs/LICH_WIRE.md` §2)

`hydra_lich::new_token` (`0x101689d30`) fills 32 bytes from `getrandom` and
renders 64 lowercase hex characters.  Lich is started with
`--pipe --stormfront --sentinel -g 127.0.0.1:<port>` (`relay::spawn`).

**The direction is the opposite of what this file claimed until Phase 3.**
Hydra **listens** on `127.0.0.1:<ephemeral>` and Lich connects back to it:
Hydra plays the game server for Lich.  The token is a key *Lich* presents, and
the same two `\n`-terminated lines travel on two redundant channels:

```text
Hydra -> Lich stdin, once, right after the spawn:
    <token>\n
    /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n
Lich -> Hydra, first two lines of the accepted socket:
    <token>\n                                          (the key)
    /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n  (client identification)
```

`hydra_session::lich` carries the small pieces this crate needs (it cannot
depend on `hydra_lich`, so the constant and the two-line parser are
duplicated and must stay spelled identically):

* `accept_keyed(&mut stream, token)` — **server** side, validating the two
  lines: first line must equal the token, second must equal
  `CLIENT_IDENTIFICATION` (`PermissionDenied` / `InvalidData` otherwise; the
  caller drops the stream, which closes it).  The token is never logged.
* `client_handshake(&mut stream, token)` / `handshake_bytes(token)` — Lich's
  side: writes the two lines and flushes.  Used by fake Lich helpers and
  tests, **not** by Hydra (Hydra accepts).
* `connect_keyed(port, token)` / `connect_keyed_to(addr, token)` — connect to
  `127.0.0.1:<port>`, apply the recovered **60-second** deadline to
  connect/read/write and run `client_handshake`.
* `LichFrame` / `command_frame` — the `<c>` command versus bare line
  distinction on the Lich -> Hydra socket (`<c>exp` vs `look` vs `_injury 2`).
* `GAME_CHUNK_PREFIX` (0x1F) with `has_game_chunk_prefix` /
  `strip_game_chunk_prefix` — the per-chunk prefix on game XML Lich forwards
  on its stdout; Lich's own lines carry none.

The 60-second number is the `Ended::DidNotConnect { seconds: 60 }` literal in
`hydra_lich`; the capture is consistent with it (a healthy Lich connects at
once).

**Still UNKNOWN**: what Hydra shows the player on a token mismatch (no
mismatch happened in the capture, so no message is invented here), whether the
stdin lines are re-sent on a Lich restart, and behaviour on non-Windows builds
and other game instances.  The duplicate constants and parsers in the two
crates are a deliberate consequence of the crates not depending on each other.

### 3.5 Password flow — recovered, and it is *not* a Lich prompt

Tracing the two literals to their users (both in **`hydra_table::account`**,
not in `hydra_session`):

* `hydra_table::account::env_name` (`0x1011f76a6c`) builds
  `format!("HYDRA_PASSWORD_{}", …)` (`0x103e613d0`), where the account is
  trimmed and every character is mapped by the loop at `0x10128ed34`:
  `a`-`z` → upper case, `A`-`Z`/`0`-`9` kept, anything else → `_`.
* `hydra_table::account::kept` (`0x1011f7568`) asks the **OS keyring**
  (`keyring::Entry::get_password`, service name a 5-byte literal) after
  lower-casing the account.
* `hydra_table::account::password` (`0x1011f7b68`) is `kept` or, when that is
  `None`, `rpassword::prompt_password` on
  `format!("Password for account {}: ", account)` (`0x103e613e2`) — Hydra's own
  **terminal** prompt.

So the password never goes near Lich's command line or environment:
`hydra_lich::child::command` (`0x101683638`) removes **every** `HYDRA_PASSWORD_*`
variable from the inherited environment before spawning
(`child::is_password`, `0x101683420`: a 15-byte `eq_ignore_ascii_case` prefix
test), so the user-supplied Ruby cannot read it.  What Lich *does* receive is
whatever the relay writes to the child's stdin.

`hydra_session::password` exposes the recovered pieces:

```rust
password::env_name("Aelith")      // "HYDRA_PASSWORD_AELITH"
password::prompt("Aelith")        // "Password for account Aelith: "
password::is_prompt("[login] Password for account Aelith: ")  // Some("Aelith")
password::kept("Aelith")          // the env var, never logged
```

`kept` reads the environment variable rather than the keychain (std only, and a
test can set it); **no code path in this crate logs a password**.
**UNKNOWN**: the keychain service name, and whether the original ever typed a
stored password into Lich's stdin without being prompted.

## 4. std-only departures

### 4.1 Phase 1 reconnection — what is real again

Every crate below is the workspace pin, declared as `x = { workspace = true }`
in this crate's `Cargo.toml`, and used through `cargo --offline`.

| real crate (pin) | replaces | where | notes |
| --- | --- | --- | --- |
| `serde` 1.0.229 (derive) | the hand JSON shape readers | `textfmt`, `settings_store`, `player_log::links`, `stream_rules` | `Value` implements `Serialize`/`Deserialize` by hand so document key order survives (see 4.3) |
| `serde_json` 1.0.151 | `textfmt::{parse_json, to_json}` body | all JSON file stores | pretty printer is the same two-space form; errors now carry line/column instead of a byte offset |
| `toml` 1.1.6 | `textfmt::parse_toml`, `stream_rules::Rules::parse` | settings/stream rules | `Rules::parse` deserialises derived `RulesDoc`/`RuleDoc`; `toml_writer` 1.1.3 comes in through `toml`'s `display` feature |
| `toml_edit` 0.25.15 | `textfmt::to_toml`, `stream_rules::Rules::to_toml` | as above | one `[[rules]]` table per rule, nested rules as inline tables |
| `regex` 1.13.1 | `pattern::Pattern`'s hand engine | `pattern`, `stream_rules`, `player_log::reader` | `regex::escape` is the binary's own `escape` |
| `flate2` 1.1.10 + `zlib-rs` 0.6.8 backend | plain-concatenation archives | `player_log::archive` | real gzip, one gzip member per day file; the `zlib-rs` feature makes flate2 use the same backend version the binary linked |
| `tokio` 1.53.1 (`full`) | — | `command`, `queue`, `lich`, `supervisor` | `broadcast`, `oneshot`, `mpsc`, `time`, `#[tokio::test]` |
| `tokio-util` 0.7.19 | `AtomicBool` `stopping` | `command::SessionHandle::{stopping, stop, stop_token}` | a real `CancellationToken` |

### 4.2 Departures that remain

| original | reconstruction | why it stays |
| --- | --- | --- |
| `tokio::sync::broadcast::Sender<Event>` **as the only channel** | the real broadcast channel *plus* the retained `Arc<Mutex<EventLog>>` + `Listener` cursors | `Listener::try_recv`/`drain` are synchronous and are what the sibling crates use; `watch()` is the real channel for tokio callers |
| `tokio::sync::mpsc` for the queue reply | `std::sync::mpsc::Sender` reply **plus** the real `tokio::sync::oneshot` (`Oneshot`, `open_window_async`, `Envelope::with_oneshot`) | `Reply` is `Sender<Outcome>` in the public API and its `Receiver` is what every caller/test holds; `oneshot::Sender` is not `Clone`, which `Envelope`/`Owed` need |
| `tokio::sync::mpsc` in the Lich door and the player-log channel | `std::sync::mpsc` **plus** `LichDoor::claim_async()` with real `tokio::mpsc` channels (4096/64) | the synchronous `Claim` is the shape `hydra_lich`'s relay and the tests take; `player_log::channel` keeps `sync_channel(64)`/`SyncSender` (`dropped` counts a closed channel, not a full one) |
| `tokio::sync::Mutex`/`RwLock` | `std::sync::Mutex` (+ `AtomicBool`) | every public accessor returns a `MutexGuard` from `std`; a tokio mutex would make `session.queue()`/`session.aliases()`/`Slot::state()` `async` and change every sibling call site |
| `answer_between`'s `select!` body | `answer_between` (std) + `answer_between_async` (`tokio::time::timeout`) | the body is UNKNOWN; the async form is the tokio equivalent, the sync one is what the existing tests drive |
| `getrandom` | nothing: the token comes from `hydra_lich::new_token` | `getrandom` is not in the workspace pins; this crate never called it |
| OS keyring (`keyring` crate) | `HYDRA_PASSWORD_*` environment variable (`password::kept`) | the keychain call in the binary is `hydra_table::account::kept`, not this crate's; an OS keychain read is not testable here and would change `kept`'s documented env-var contract.  Note `keyring = "=3.6.3"` *is* in the workspace pins, so the parent may hand this to `hydra_table` |
| `hydra_net::eastern::{now, date, week_start, days_from_civil, civil_from_days, …}` | `notes::{days_from_civil, civil_from_days}`, `writer::civil`, `archive::week_start` | §4.4: the sibling exists and is acyclic, but it is **Eastern-time and Sunday-based**, while this crate's helpers are UTC and Monday-based; adopting it changes day keys, `when` stamps and archive period names (see below) |
| `hydra_net::sink::config::{log_dir, moment}` | `writer::root(base)`, `notes::stamp()` | same dependency; `log_dir()` returns a process-wide `$HOME/hydra/logs`, while every function here takes the root as an argument (that is what the sibling crates call) |
| `hydra_net::sink::redactions::Redactions::apply` | `writer::scrubbed(text, &[String])` | same dependency; `Redactions` is a *byte*-oriented sink type with its own masks, while the log writer scrubs `&str` with the caller's list |
| `hydra_model::{LearnedCommands, Stance, GameState, …}` | `menu_store::{Learned, MenuFile}` and plain strings | `hydra_model` is mid-Phase-1 in its own worktree; its `GameState` shape is not stable yet |
| `hydra_protocol::frame::vocabulary::Frame` (280 bytes) | `queue::Answer = Vec<String>`, `Event::Frames(Vec<String>)` | §4.4: `Answer` is in `Envelope`/`Owed`/`Outcome`/`Event::Frames`, which `hydra_board` consumes; changing it is an API change across crates that also depends on `hydra_protocol`'s Phase-1 snapshot |

### 4.3 The JSON document type

`textfmt::Value` stays the crate's document type (it is public through
`SettingsFile::sections`, and six stores read and write it).  It now implements
`serde::Serialize` and `serde::Deserialize` itself, because:

* `serde_json::Value`'s object is a `BTreeMap` unless the `preserve_order`
  feature is on, and turning that feature on is workspace-wide (it would
  reorder every other crate's JSON too).  The settings file must keep the four
  recovered keys in the recovered order (`schema_version`, `instance`,
  `character`, `sections`), so the writer preserves the parsed order instead.
* the visitor pushes map entries in encounter order, so a round trip is
  ordered in and ordered out;
* integers are written as integers (`"schema_version": 2`, not `2.0`), which is
  what the recovered writer did.

`toml`'s table *is* key-sorted without its `preserve_order` feature, so
`textfmt::parse_toml` returns a key-sorted document.  Nothing depends on TOML
order: `stream_rules` deserialises derived structs, and `to_toml` writes from
the struct's field order.

### 4.4 Why the sibling crates are still not linked

`hydra_net`, `hydra_protocol` and `hydra_model` are path-reachable and acyclic
(`hydra_net` and `hydra_protocol` have no dependencies at all), so a
`hydra_session -> hydra_net` edge would compile.  It is not taken in this pass
because the swap changes *behaviour*, not just the dependency:

* `hydra_net::eastern::week_start` is **Sunday**-based (recovered: day numbers
  floored to a multiple of seven in the shifted frame).  `archive::week_start`
  here steps back to **Monday**, and the archive tests pin that
  (`week-2024-04-29` for 2024-05-01).  Swapping in the sibling renames every
  week archive (`week-2024-04-28`), which changes on-disk file names for the
  archive, retention and `sweep` paths.
* `eastern::{now, date}` work in **Eastern** time (`EST`/`EDT` with a fixed DST
  rule), while `writer::civil::now`, `notes::stamp` and every log day key here
  are UTC.  Adopting them shifts day-file names and every `when` field by four
  or five hours for part of each day.
* `hydra_net::sink::config::log_dir()` is the *sink's* process-wide root;
  every function in this crate takes the root as an argument (that is how
  `hydra_board`/`hydra_table`/`hydra_web` call it), so it is a different shape.
* `hydra_protocol::Frame` would change `queue::Answer` and `Event::Frames`,
  which `hydra_board` matches on; both crates are still being reconnected in
  their own worktrees, so the parent should sequence that swap.

Everything marked `// UNKNOWN:` in the source expands the points above.

## 5. Tests

`cargo test -p hydra_session` runs **186** tests with ~750 assertions (it was
158 with 600 before Phase 1; the new tests are the ones marked *Phase 1* below):

* `store`: `safe_component` cases (`"Aelith's Cart"` → `aelithscart`, `".."` →
  `""`, non-ASCII dropped, case folded), both `character_path` outcomes, the
  temporary extension, and a real `save_bytes` round trip with no leftovers.
* `textfmt`: JSON round trip of a notes-shaped document, a bad escape reported
  with `serde_json`'s line/column (*Phase 1*), unicode escapes + surrogate
  pairs, member order through a round trip (*Phase 1*), a settings-shaped
  document through `serde_json` directly (*Phase 1*), TOML
  scalars/tables/arrays of tables/comments-in-strings, the TOML writer read
  back by `toml` (*Phase 1*), and the extra `toml` syntax — multi-line strings,
  dotted keys, inline tables, datetimes, underscored integers (*Phase 1*).
* `pattern`: literals, `^`+escape, classes, `.*`, alternation + groups, `* + ?`,
  `regex::escape`, three broken patterns, the crate's own error wording for
  those three (*Phase 1*), the syntax the hand engine lacked — `\b`, `{2,3}`,
  POSIX classes, `(?i)`, lazy quantifiers, captures and named groups (*Phase
  1*), and literal-is-escaped-and-ordered (*Phase 1*).
* `settings_store`: the recovered extension is 14 bytes, the four JSON keys in
  the recovered order, the "schema {} … reads up to {}" message, sections, a
  missing file, and (*Phase 1*) a hand-written file loaded through serde with
  unknown members ignored, saved back, and a newer schema refused on `load`.
* `character_store` / `menu_store`: merge-and-save round trips, the dirty flag,
  the four-column TSV line, `underlay`.
* `queue`: 1000-deep FIFO, `clear_waiting` clearing every caller, the exact
  window tags (`0x3b9aca00`), `answer_all`/`offer`/`close_window`, `take_next`
  recording what was written, and (*Phase 1*) the tokio `oneshot` paths —
  `open_window_async` → `close_window`, `clear_waiting`, and `answer_all` after
  `take_next`, plus the two ticket kinds on separate entries.
* `supervisor`: the recovered `[1, 2, 5, 10, 30]` s table with the 80 %..=120 %
  band per step, the clamp at entry 4, the constants themselves, the jitter
  fraction bounds, `answer_between` first-value and deadline, and (*Phase 1*)
  `answer_between_async` taking the first value, hitting the deadline, and
  returning at once on a closed channel.
* `alias`: `{}_{}` key, name/rest split, single-command expansion, the
  multi-command `\?` error verbatim, `put`/`remove`, JSON round trip.
* `steps`: comma splitting + `\,` escaping, `wait` in both spellings
  (`s2`/`S2.5`/`s60` and `wait`/`wait 5`/`WAIT 60`), the recovered out-of-range
  message, and `said` rendering `wait N s` joined by the confirmed 7-byte
  separator.
* `notes`: `notes.json`, key building, `first_line`/`clipped`, clip exits, get/
  find/search, JSON round trip + schema guard, civil-date arithmetic, and an
  on-disk add/append/forget cycle.
* `claimant`: `usable` for every punctuation range and the negatives, the default
  symbols, shared clones, `claim` stripping, `with_words`, `Settings`.
* `lifecycle`: `advance` returns the new value and clones share the cell.
* `notice`: quiet levels, `first_line`/`text`, the 12-line help, `linked`.
* `stream_rules`: class tags (`Fighting == 2`), `check` wording, anchored
  `finds`, `fought`, the 60-char `like` elision, TOML round trip, stream dedup,
  a failed `set` leaving the old rules in place, and (*Phase 1*) a nested +
  quoted-regex round trip that `toml` also accepts, a hand-written file with
  missing and unknown members, the bad-class and broken-TOML errors, and a
  round trip through a real `rules.toml` on disk.
* `player_log`: `class` on real log lines; writer day rollover and exact file
  contents; `piece`/`day_of`; reader `days`/`tail`/`around`/`window`/`search`/
  `tagged`/`export`; links (including *Phase 1* `serde` for `LogLink`/`Does`
  and the `schema`-then-`links` file shape); archives (week/month periods,
  manifest errors, sweep + read-back + deletion, and (*Phase 1*) real gzip: the
  magic bytes, a `MultiGzDecoder` round trip per member, a second sweep
  appending to the same archive, the plain-concatenation fallback and the
  empty-file case); retention `doomed`/`preview` exact sentence/`prune`;
  channel capacity, drops and rendering; feed gag/redirect/filters.
* `lich`: the door's `.` mapping, `say`→session, `claim` installing the taps,
  both recovered `hand_to_lich` messages, `session_over`, `show` folding frames,
  `Wire::fell_behind`, `begin`, the typing queue, and (*Phase 1*) `claim_async`
  over real tokio channels — typed lines in order, lines back to the door, the
  recovered 4096/64 capacities, and a full channel refusing a line.  (*Phase 3*)
  the observed handshake: the two exact lines from `handshake_bytes`/
  `client_handshake`, the server-side `accept_keyed` accepting a good token +
  client line (CRLF tolerated, nothing over-read), refusing a wrong token
  (`PermissionDenied`, token not in the message), a wrong second line and a
  truncated handshake; `connect_keyed` round-tripping against a real loopback
  listener; `<c>`/bare `LichFrame` framing and `command_frame`; and the 0x1F
  game-chunk prefix helpers.
* `command`: the basics, `say` vs `say_unlogged`, subscriber cursors,
  `chain_stopped` at level 1, `run_steps` through the taker, the broken-alias
  path, `command_words`, `fold_in`, `clip`, `stop`, and (*Phase 1*) the
  `CancellationToken` waking a spawned waiter, and every event arriving on the
  `broadcast` channel as well as on the cursor `Listener`.
* `actor`: the four exit words and the near misses; a pushed payload updating
  the menu.
* `bestiary`: level/band/name parsing with the recovered refusal, the answer
  headers, and a stable `room_uid`.
* `saves`: save recognition and path normalisation.
* `lookout_seed`: the empty directory, the newest day, the summary counts.
* `password`: the variable name mapping, the prompt text, prompt recognition,
  and `kept` reading a real environment variable.

Gates (all with `--offline`): `cargo check -p hydra_session`, `cargo test -p
hydra_session` (186 tests passing) and `cargo check --workspace` are clean, and
`grep -rn 'todo!\|unimplemented!' crates/hydra_session/src` is empty.

## 6. Audit pass (verification batch)

This crate had already been recovered and merged; this pass re-read the
disassembly against the reconstruction.  Gates re-run from the worktree root:
`cargo check -p hydra_session`, `cargo test -p hydra_session` (158 passing) and
`grep -Rn 'todo!()\|unimplemented!()' crates/hydra_session/src` (empty).

Checked against the binary: `queue::{admit, clear_waiting, open_window,
written_at_once, close_window, offer, answer_all}` plus both `drop_in_place`
glues, `supervisor::{jitter, backoff}` and their caller, `lich::{claim, symbol,
session_over}` and the two `hand_to_lich` literals, `steps::{pieces, steps, said,
wait}` (incl. the inlined `Map::fold` renderings), `stream_rules::fought`,
`bestiary`'s literal set, and `hydra_table::account::env_name`.

Fixed (behaviour was silently wrong):

1. `supervisor::backoff`/`jitter` — the crate claimed a doubling from 1 s capped
   at 60 s with up to 250 ms of additive jitter, marked INFERRED because the
   constants "cannot be seen without decompiling".  They can: the table at
   `0x1039c5c60`, the `0.4`/`0.8` doubles and the `fcvtzu`/`ms` split give
   `backoff(attempt) = [1,2,5,10,30][min(attempt,4)] s × (0.8 + 0.4 × jitter())`
   with millisecond truncation, and `jitter()` is a `f64` fraction from the wall
   clock's sub-millisecond field.  See §2.3a.  `BACKOFF_CAP` is now 30 s.
2. `steps::pieces` — the separator is `,` (with `\` escaping), not `;`.
3. `steps::wait` — the original's spelling is `s<seconds>` (f32, `0.0..60.0`);
   the crate now accepts it as well as its own `wait <seconds>`.
4. `steps::said`/`Step::text` — a wait renders as `wait N s`, not
   `wait N seconds`/`wait a moment`; `steps()` runs `wait` over every piece,
   which is what `0x101ab01e8` does.

Recorded but **not** fixed (would change the public API):

* `queue::Owed` is an enum in the binary (three states tagged through the
  `Instant` nanosecond slot, `1e9` / `1e9 + 1`; `open_window` takes the reply
  sender as an argument), and `queue::answer_all` also drains the *waiting*
  queue.  The reconstruction keeps a plain `Owed` struct with a `key` parameter
  and only walks the owed queue.  Constants, sizes and the 30-second rule match.
* `steps::Step` models two of the original's (at least) five kinds; the
  roundtime waits and `behaviour_said` are not modelled.
* `notice::help` and `notice::closes_a_hunt`'s wording, `saves`' prefixes,
  `menu.tsv`/`links.json` file names, and the `player_log::links` JSON members
  have no literal in `__TEXT` — they remain INFERRED, as §2.2 says.

## 7. Phase 1 reconnection pass

`cargo check -p hydra_session --offline`, `cargo test -p hydra_session
--offline` (186 passing) and `cargo check --workspace --offline` are clean after
the reconnection; `grep -Rn 'todo!()\|unimplemented!()' crates/hydra_session/src`
is still empty.

Reconnected (see §4.1 for the table): `serde`/`serde_json`, `toml`/`toml_edit`
(with `toml_writer` through `toml`'s `display` feature), `regex`, `flate2` with
its `zlib-rs` 0.6.8 backend, `tokio` and `tokio-util`.

The public API did not change shape: every function and type the sibling crates
call (`store::*`, `notes::*`, `settings_store::*`, `command::SessionHandle`,
`steps::Step`, `player_log::channel::PlayerLog`, `stream_rules::Rules`,
`claimant::Desk`, `password::*`) keeps its signature, and the tokio work is
additive (`stop_token`, `watch`, `broadcaster`, `open_window_async`,
`Envelope::with_oneshot`, `Owed::async_waiting`, `Oneshot`,
`LichDoor::claim_async`, `AsyncClaim`, `answer_between_async`).  Two public
structs gained a field (`Envelope::oneshot`, `Owed::oneshot`; `Tap::tx_async`
and `Tap::keep`) — that breaks `Struct { .. }` literals built outside the crate,
but not any constructor call, and no sibling crate builds them by hand.

Deliberately **not** changed: `supervisor::backoff`'s one-argument signature,
`queue::Reply`'s `std::sync::mpsc::Sender`, `LichDoor::claim`'s synchronous
`Claim`, `SessionHandle::subscribe`'s cursor `Listener`, `textfmt::Value` as the
document type, and the UTC/Monday date helpers (§4.4).

Behaviour that changed as a direct result of using the real crates:

1. JSON and TOML syntax errors now read like `serde_json`/`toml` errors — a
   line and column (`invalid escape at line 1 column 9`) instead of the
   hand-written `bad escape` / `at byte N`.  The surrounding message shapes
   (`"{} cannot be read: {}"`, `"notes.json cannot be read: {}"`, `"{}: {}"`)
   are unchanged.
2. `textfmt::parse_toml` returns a **key-sorted** document, because `toml`'s
   table is a `BTreeMap` without its `preserve_order` feature (see §4.3).
   Nothing depends on TOML order.
3. Archives are gzip.  Pre-Phase-1 archives (a plain concatenation) still read;
   `archive::is_gzip` says which kind a file is.  The manifest still records the
   *uncompressed* member size.
4. `Pattern::regex` accepts the whole `regex` language, so a rule that the hand
   engine used to accept as a literal (`a{2}`, `\b`) now behaves as the crate
   does.  The three broken-pattern cases the crate tests stayed broken.
