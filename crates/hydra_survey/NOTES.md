# `hydra_survey` — recovery notes

Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled globals in `symbols_demangled.txt`). The crate had seven
`todo!()` stubs over three files; this build has none
(`grep -Rn 'todo!\|unimplemented!' crates/hydra_survey/src` is empty) and

* `cargo check --offline -p hydra_survey` — clean, no warnings
* `cargo test --offline -p hydra_survey` — **50 tests pass**
* `cargo check --workspace` — clean (no other crate was touched)

**Phase 1 is done for this crate:** `serde` 1.0.229 and `serde_json` 1.0.151 are
real dependencies again (workspace pins, `--offline`), the log schema is the
derived one the binary carries, and the hand-written `json` reader is gone. See
["Phase 1 reconnection"](#phase-1-reconnection) for what came back, what changed
and the two places where the recovered *API* and the recovered *schema* had to
be reconciled explicitly.

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow, literals, tables and offsets read out of the instruction stream; asserted by a test. |
| **partly** | the entry contract, literals and the shape of the data are recovered, but one step (a predicate, a field's name, or a call into another crate) could not be; the body implements the evidenced part and says so at the site. |
| **UNKNOWN** | the binary does not pin this down. The safest behaviour was chosen and the choice is written where it is used. |

Every uncertain decision also carries a `// UNKNOWN:` comment at the point of
use; this document collects them.

---

## Tooling

Read-only, all scratch under `$TMPDIR` (nothing written into the repo):

* `../recon.py` — the shared driver (`syms`, `disas`, `bytes`, `str`,
  `findstr`). `disas` was not enough here: the serde glue is a mass of local
  symbols, so a small wrapper (`objdump -d --start-address=… --stop-address=…`
  plus `recon.annotate`) was used to disassemble *ranges* rather than
  "one symbol to the next".
* **`(ptr, len)` array recovery.** serde's `FIELDS`/`VARIANTS` tables are plain
  arrays of `&'static str` in `__const`. Searching the whole file for a run of
  `(ptr, len)` pairs whose pointers land in the string pool rebuilds them
  exactly. This is how the schema was pinned (below), instead of matching bytes
  in `visit_str` by hand.
* **`adrp`/`add` cross-references.** A little simulator over `__text` finds every
  place a given data address is materialised, which located the `expecting`
  strings and the `deserialize_struct` calls that use each `FIELDS` table.
* **`drop_in_place` shims as a type oracle.** `nm -an | grep drop_in_place`
  gives one shim per instantiated type; its body is a field-by-field map of the
  struct: `cbz cap; dealloc(ptr)` is a `String`, a `cmp isize::MIN` guard is an
  `Option<String>`, a 24-byte-stride element loop is a `Vec<String>`, and so on.
  `RoomRecord`'s 17 fields, `Known`, `Seen`, `Graph`, `Record` and `Via` were all
  read this way.
* **`madd`/immediate analysis** for the `BTreeMap` node layout (`keys` at `+0x10`
  with 24-byte `String`s, values at `+0x118` with the value type's stride, the
  child array at `+0xb10`, the node length just before it).

---

## ABI facts this crate re-confirms

* `String`/`Vec` are `{cap @0, ptr @8, len @16}`, and `Option<String>`'s `None`
  is `isize::MIN` in the capacity slot (`cmp x1, #0x8000000000000000` in every
  `Option<String>` drop).
* A `sret` value comes back in `x8`; `Result<T, E>` with a 24-byte payload (a
  `String` or a `Vec`) puts the discriminant in the first word (`0` = `Ok`,
  `1` = `Err`) and then the payload, which is how `log::read`'s
  `Result<Vec<Record>, String>` was recognised.
* `BTreeMap<K, V>` is 24 bytes: `root.node @+0`, `root.height @+8`,
  `length @+0x10`. `BTreeMap::get`, `::insert` and `Drop` all touch those three.
* A `HashMap` is 48 bytes with `RandomState` last; `RandomState::new` is the
  `mov w8,#1; ... ldr x8,[GLOBAL]; blr x8` thread-local dance plus
  `std::sys::random::hashmap_random_keys` at `0x101ea02fc`.
* `Option<(i32, i32, i32)>` is 16 bytes with the tag in the first word (the
  `RoomRecord`/`Known` field at `0x180`/`0xc0`).

---

## The log schema (the heart of the crate)

`serde` derives are gone from the binary, but their *data* is not. Three
independent sources agree, which is why the names below are asserted in tests:

1. the `FIELDS`/`VARIANTS` `&'static str` arrays in `__const`;
2. the `__FieldVisitor::visit_str` byte comparisons (which map a name to its
   field index — the indices matched the array order in every case checked:
   `exits` → 6, `sensed_exit` → 14);
3. `drop_in_place<T>` for every field's type and the `visit_map` arms for its
   deserializer.

### `FIELDS` / `VARIANTS` tables (recovered addresses)

| table | address | contents |
|---|---|---|
| `Happening` `VARIANTS` | `0x104433708` | `died`, `healed`, `resumed`, `stopped` |
| `EdgeRecord` `FIELDS` | `0x104433748` | `from`, `cmd`, `to`, `rt`, `dir`, `result`, `lines` |
| `EdgeResult` `VARIANTS` | `0x1044337b8` | `arrived`, `failed`, `stayed` |
| `RoomRecord` `FIELDS` | `0x1044337e8` | `sid`, `uid`, `title`, `desc`, `paths`, `compass`, `exits`, `roommeta`, `via`, `pos`, `climate`, `terrain`, `animals`, `overhead`, `sensed_exit`, `tracked`, `location` |
| `VerbRecord` `FIELDS` | `0x1044338f8` | `sid`, `verb`, `lines`, `rt`, `cast_rt`, `prompt_closed` |
| `EventRecord` `FIELDS` | `0x104433958` | `sid`, `what` |

The `expecting` strings sit next to them (`0x104433588` = "struct Via with 2
elements", `0x104433598` = "struct EdgeRecord with 7 elements", `…5a8` = "struct
RoomRecord with 17 elements", `…5b8` = "struct VerbRecord with 6 elements",
`…5c8` = "struct EventRecord with 2 elements"), and match the table lengths.

### `Record` — the outer line (full)

`Record::__FieldVisitor::visit_str` at `0x1013c2ff0` compares four names:
`t` (1 byte, `cmp w8,#0x74`), `run` (3), `char` (4), `server` (6); every *other*
key falls into the "other" arm at `0x1013c30c0`, which copies the key bytes into
a `Content` (`strb w8=12` = `Content::Str`) — that is `#[serde(flatten)]`.

`drop_in_place<Record>` at `0x1013b4d0c` drops four `String`s (`+0x00`, `+0x18`,
`+0x30`, `+0x48`) and then `Body` at `+0x60`; the whole record is 232 bytes.

`serde_json::de::from_trait::<Record>` (`0x1013c6f50`) calls
`Deserializer::deserialize_map`, i.e. the line is one JSON **object**, not a
struct-shaped access — again what a flattened struct does.

### `Body` — the flattened, internally tagged enum (full)

`Deserialize for Body` at `0x1013c2674` builds
`TaggedContentVisitor::new(tag = "kind", name = "internally tagged enum Body")`
and then dispatches on the collected tag to `deserialize_struct`:

| tag index | tag string | payload |
|---|---|---|
| 0 | `room` | `Box<RoomRecord>` (`Box<T>::deserialize` at `0x1013cc85c`, `rust_alloc(0x1a8)`) |
| 1 | `verb` | `VerbRecord` (`deserialize_struct("VerbRecord", 6, …)` at `0x1013bb2a0`) |
| 2 | `edge` | `EdgeRecord` (`deserialize_struct("EdgeRecord", 7, …)` at `0x1013bd384`) |
| 3 | `event` | `EventRecord` (`deserialize_struct("EventRecord", 2, …)` at `0x1013ba8b0`) |

So a line looks like

```json
{"t":"…","run":"…","char":"…","server":"…","kind":"edge","from":"…","cmd":"north",
 "to":"…","rt":"0.2","dir":"n","result":"arrived","lines":["You walk north."]}
```

— the body's own keys live in the same object as the header. Tests assert that
shape for all four kinds.

### `RoomRecord` — 17 fields (full types, one UNKNOWN-free gap)

The `visit_map` is inlined into
`ContentDeserializer::deserialize_struct::<RoomRecord>` at `0x1013b785c`; it
dispatches on the field index through the 17-entry `u16` jump table at
`0x1024157bc` (base `0x1013b7ae0`, `add x9, x9, w10, lsl #2`). Each arm calls one
value deserializer, which gives the field's type:

| # | field | arm | value deserializer | type |
|---|---|---|---|---|
| 0 | `sid` | `0x1013b7ae0` | `deserialize_string` `0x1013b7708` | `String` |
| 1 | `uid` | `0x1013b80c4` | `deserialize_option` `0x1013b74e4` | `Option<String>` |
| 2 | `title` | `0x1013b7e98` | `deserialize_string` | `String` |
| 3 | `desc` | `0x1013b7fb0` | `deserialize_string` | `String` |
| 4 | `paths` | `0x1013b7d04` | `deserialize_string` | `String` (text, **not** a list) |
| 5 | `compass` | `0x1013b8184` | `deserialize_seq` `0x1013b68d4` | `Vec<String>` |
| 6 | `exits` | `0x1013b828c` | `deserialize_seq` | `Vec<String>` |
| 7 | `roommeta` | `0x1013b8038` | `deserialize_map` `0x1013b6600` (two `deserialize_string`s, one per side) | `BTreeMap<String, String>` |
| 8 | `via` | `0x1013b8378` | `deserialize_option` `0x1013b7328` | `Option<Via>` |
| 9 | `pos` | `0x1013b7e1c` | `deserialize_option` `0x1013b75f4` → `0x1013b706c` (three `next_element`s, `invalid_length(1)`/`(2)`) | `Option<(i32, i32, i32)>` |
| 10 | `climate` | `0x1013b8304` | `deserialize_option` | `Option<String>` |
| 11 | `terrain` | `0x1013b7c7c` | `deserialize_option` | `Option<String>` |
| 12 | `animals` | `0x1013b7d8c` | `deserialize_seq` | `Vec<String>` |
| 13 | `overhead` | `0x1013b8204` | `deserialize_option` | `Option<String>` |
| 14 | `sensed_exit` | `0x1013b7bf4` | `deserialize_option` | `Option<String>` |
| 15 | `tracked` | `0x1013b7f20` | `deserialize_seq` | `Vec<String>` |
| 16 | `location` | `0x1013b7b68` | `deserialize_option` | `Option<String>` |

`drop_in_place<RoomRecord>` (`0x1013bf188`) agrees with every type: four plain
`String`s (`+0x00`, `+0x18`, `+0x30`, `+0x48`), four `Vec<String>` (`+0x60`,
`+0x78`, `+0x90`, `+0xa8`), six `Option<String>` (`+0xc0`, `+0x108`, `+0x120`,
`+0x138`, `+0x150`, `+0x168`), `Option<Via>` at `+0xd8` (its second `String` at
`+0xf0`), the 16-byte `Option<(i32, i32, i32)>` at `+0x180`, the `BTreeMap` at
`+0x190`; 424 bytes, which is the `rust_alloc(0x1a8)` above. (The drop order is
*not* the declaration order — rustc grouped the 24-byte fields first — which is
why the field↔offset map above comes from the deserializers, not from offsets.)

### `Via` — `{ from, cmd }` (full)

`ContentDeserializer::deserialize_struct::<Via>` at `0x1013bc8f0` compares keys
`from` (len 4, tag 0, `0x1013bcb90`) and `cmd` (len 3, tag 1, `0x1013bccbc`);
`drop_in_place<Via>` at `0x1013bf03c` drops two `String`s (32- and 48-byte
offsets ⇒ 0x30 bytes). *Not* `to`/`rt`: those strings exist for `EdgeRecord`.

### `VerbRecord`, `EdgeRecord`, `EventRecord`

* `VerbRecord` — 96 bytes: `String@+0x00` (`sid`), `String@+0x18` (`verb`),
  `Vec<String>@+0x30` (`lines`), then the two roundtimes as 8-byte slots at
  `+0x48` (`rt`) and `+0x50` (`cast_rt`) and `prompt_closed` as the byte at
  `+0x58` — the visitor's epilogue copies the whole record with
  `ldp q0,q1,[x24]` / `ldp q0,q1,[x24,#0x20]` / `ldp q2,q3,[x24,#0x40]` and
  stores the tail with `stp d8, d9, [x24, #0x48]` and `strb w8, [x24, #0x58]`
  (`0x1013bc604`, `0x1013bc608`). The field arms call
  `deserialize_string` ×4, `deserialize_seq` ×2 (`lines`) and — decisive for
  the tail — `Option::<T>::deserialize` twice (`0x1013cc7d4`) plus
  `bool::deserialize` once (`0x1013c9acc`). Which of the four string arms is
  `sid`, which `verb` and which an `Option`'s inner `visit_some` cannot be told
  apart in the inlined body; that the two roundtimes are optional can.
  **UNKNOWN:** the element type of the two `Option`s (a float, judging by the
  8-byte slots; our public fields are `f64`). See [`log::VerbRecord`] and
  "Phase 1 reconnection".
* `EdgeRecord` (fits in `Body`'s 0x80-byte payload): two `String`s
  (`from@+0x00`, `cmd@+0x18`), a `Vec<String>` (`lines@+0x30`), two
  `Option<String>` (`to@+0x48`, `dir@+0x60`) and the drop-free `rt`/`result`.
  `result` is the byte at `+0x78` (`from_records` loads it from record offset
  `0xe0` = `Body+0x80`) and is compared against `0`/`1`/`2`, which is
  `arrived`/`failed`/`stayed` in `VARIANTS` order. The 7th field `rt` is
  drop-free: read as `Option<String>` (the round time as the game printed it).
  **UNKNOWN:** which of `to`/`rt`/`dir` is the second `Option<String>` in the
  layout; the names are certain (from `FIELDS`), the offsets are not.
* `EventRecord`: `sid: String`, `what: Happening`.
* `Happening`/`EdgeResult` are unit-only enums deserialised through
  `deserialize_enum` (`0x1013c2c94` / `0x1013c2e54` + the shared
  `ContentDeserializer::deserialize_enum` at `0x1013b6b34`), i.e. the JSON is the
  bare variant string: `"what":"healed"`, `"result":"arrived"`.

`Body::Room` really is boxed: `Box<RoomRecord>::deserialize` allocates 424 bytes
and the tag-0 branch of `Graph::from_records` reads a pointer at record
`+0x68`, whereas the other variants' payloads sit inline there.

---

## Function table

### `log`

| item | addr | status | notes |
|---|---|---|---|
| `read` | `0x1013c2878` | **full** | `fn read(&str) -> Result<Vec<Record>, String>`. `OpenOptions::_open` → on error `format!("{}: {}", path, error)` (`0x103e697be` holds the pieces `0xC0 0x02 ": " 0xC0`) → `rust_alloc(8192)` for a `BufReader` → `Lines::next` (`0x1013cc6e8`) and `SpecFromIterNested::from_iter` (`0x1013b4324`) into a `Vec<Record>`; only the open failure writes the `Err` discriminant, so line and parse errors are dropped. |
| `parse` (added) | — | **new** | The original's per-line step was `serde_json::from_str` (`serde_json::de::from_trait::<Record>` at `0x1013c6f50`), inlined into `read`. Exposed for tests and for callers that keep their own I/O. Since Phase 1 it *is* that call; failures are serde's message (`… at line 1 column N`). |
| `json` module (added) | — | **new** | Was the hand-rolled replacement for `serde_json`; since Phase 1 a thin alias over it (`pub use serde_json::{Error, Map, Number, Value}` + `parse`). See "Departures". |

### `graph`

| item | addr | status | notes |
|---|---|---|---|
| `step` | `0x1013cafb8` | **full** | `fn step(&str) -> Option<Step>`; `Step` is `{dx, dy, dz}` read from the sret layout (`some` at `+0`, `(dx, dy)` at `+4`, `dz` at `+0xc`). Match on byte length only: 1 = `n s e w`, 2 = `ne se sw nw up`, 4 = `down`, everything else `None`. Deltas: `n (0,-1)`, `s (0,1)`, `e (1,0)`, `w (-1,0)`, `ne (1,-1)`, `se (1,1)`, `sw (-1,1)`, `nw (-1,-1)`, `up/down (0,0,±1)` — tables at `0x1020734a8`, `0x1020784a0`, `0x102181c30`, `0x102157030`, the rest as `movi` immediates. |
| `Graph::from_records` | `0x1013cb194` | **partly** | `fn from_records(&[Record]) -> Graph`, one pass over 232-byte records switching on the body tag at `+0x60`. Recovered per branch: the `sid` number parse (`trim_start_matches('s')` + decimal, `0x1013cb2e8`/`0x1013cb468`), the running maximum of it (`0x1013cb5b0`), the `pos` → `HashMap<(i32, i32), i32>` insert (`0x1013cb5d8`, key = the 8 bytes at `+0x184`, value = the `i32` at `+0x18c`), the `Known` build and its `BTreeMap::insert` under the `sid` (`0x1013cb790`), the `movement::classify` gate on verb lines (`0x1013cb510`), and the edge path's room lookup by `from` (`0x1013cb370`) with the matched command and the result byte (`0x1013cb45c`). **UNKNOWN:** what the failed/stayed edge paths store (this build records the attempt in `Known::tried` and files an edge only on `arrived`), and what the verb path writes into the room (this build appends the lines to `Seen::lines` and remembers the verb in `Seen::last`). |
| `Graph::room` | `0x1013cba80` | **full** | `fn room(&self, &str) -> Option<&Known>`; inlined `BTreeMap::get` (root at `+0x48`, height at `+0x50`, keys 24-byte `String`s from `node+0x10`, children from `node+0xb10`, `memcmp` at `0x101f849dc`). |
| `Graph::untried` | `0x1013cbb5c` | **full** | `fn untried(&self) -> usize`: in-order walk of the room map, and for each room counts the exits its `BTreeSet` of tried names does not hold (empty graph and empty map both return `0`). |

`Graph`'s fields, from `drop_in_place<Graph>` (`0x1013cbf2c`) and the final store
at `0x1013cb8c4`: `edges: Vec<(String, String, String)> @+0x00` (72-byte
elements), `levels: HashMap<(i32, i32), i32> @+0x18`, `rooms: BTreeMap<String,
Known> @+0x48`, `highest: u32 @+0x60`.

`Known` (232 bytes, `drop_in_place` `0x1013cc9dc`): `sid: String @+0x00`,
`seen: Seen @+0x18`, `exits: Vec<String> @+0x78`, `via: Option<Via> @+0x90`,
`pos: Option<(i32, i32, i32)> @+0xc0`, `tried: BTreeSet<String> @+0xd0`.

`Seen` (0x60 bytes, `drop_in_place` `0x1013cbe58`): `Vec` of a one-byte
drop-free element `@+0x00`, another `@+0x18`, `Vec<String> @+0x30`,
`Option<String> @+0x48`.

### `probes`

| item | addr | status | notes |
|---|---|---|---|
| `magic` | `0x1013c1d8c` | **full** | `fn magic(&[String]) -> Answer`. Three passes over the lines: any of `Your magic fizzles ineffectually` (`0x102415960`, 32 B), `forces the magic words back down your throat` (`0x102415980`, 44 B — compared whole at that length) or `motions for you to stop` (`0x1024159ac`, 23 B) → `No`; any `lapse in your concentration` (`0x1024159c3`, 27 B) → `Unknown`; any line *equal to* `You gesture` or `You channel` (11 B, first 8 bytes + the 8 at `+3`) → `Yes`. |
| `splash` | `0x1013c1fe0` | **full** | `fn splash(&[String]) -> Answer`, one pass, three checks per line: `You just splashed yourself!` (`0x1024159de`, 27 B) → `Yes`; `How do you plan to do that here?` (`0x1024159fe`, 32 B) → `No`; `You splash about in the` (23 B) *and* `water` (`0x1024159f9`, 5 B) → `Yes`. Otherwise `Unknown`. |
| `Answer` (added name) | — | **full values** | The discriminants are the returned `w0` (`0`/`1`/`2`); `hydra_gui::surveyor::Picture::from_records` (`0x1007ea690`, `0x1007ea6e0`) compares `w0 & 0xff` with `2` (skip the room), then branches on non-zero to the "blocked"/"no" label and on zero to "works"/"yes". The *type's* name is not in the binary. |

Callers: the only code in the binary that uses this crate is
`hydra_gui::surveyor` (`Picture::from_records` `0x1007ea194`, `Surveyor::show`
`0x1007ec074`); `nm` shows no other `hydra_survey` symbols and no named callers.

---

## Departures from the original

1. ~~**`serde`/`serde_json` replaced by `std`.**~~ **Reconnected in Phase 1** —
   see the next section. The hand-written `json` reader and the hand-written
   field readers in [`log`] are gone; the nine types derive `Deserialize` over
   the recovered names, order and types again. Two of the lenient behaviours it
   had were *the reconstruction's*, not the original's, and went with it
   (numbers accepted where a `String` is expected; a missing `result` meaning
   `arrived`) — both are recorded in "Phase 1 reconnection".
2. **No `serde_json::Error` in the public API.** `read` still returns
   `Result<Vec<Record>, String>` (the stub's caller shape) and still drops
   per-line failures, exactly as the binary does; the only error path in `read`
   is the file open, and [`log::parse`] renders serde's error with
   `to_string()`.
3. **`probes::Answer`** is named here; the binary only carries the discriminants
   and the `hydra_gui` comparisons.
4. **`step::Step`** is named here; the binary only carries the three `i32`s.
5. **`movement::classify` is not used.** The original gates verb records on
   `hydra_model::movement::classify`, which returns a one-byte enum
   (`and x0, #0xff`, matched against `0x17` and `8`/`9`) — the recovered
   `hydra_model` in this workspace returns `Option<Movement>` instead, so the
   gate is re-implemented from the compass words. See `// UNKNOWN:` at
   `graph::reads_as_movement`.
6. ~~No new dependencies were added; `Cargo.toml` is unchanged.~~ `serde` and
   `serde_json` are workspace pins again (Phase 1).
7. **`Serialize` is derived, though the binary has none.** The nine types carry
   `Deserialize` only in the binary; `Serialize` is added here so the schema can
   be round-trip-tested and so a writer can be built on it. It changes no
   recovered behaviour.
8. **`Known`/`Seen` field names are ours.** `drop_in_place` gives the shapes,
   not the names (`Seen::marks`, `flags`, `lines`, `last`).

---

## Phase 1 reconnection

Both dependencies are workspace pins, declared as `x = { workspace = true }` in
this crate's `Cargo.toml` and used through `cargo --offline`. `hydra_survey` has
no siblings yet — nothing else in the workspace depends on it
(`hydra_gui::surveyor`, the only caller in the binary, is not reconstructed) — so
the compatibility surface is this crate's own public API.

| real crate (pin) | replaced | where |
|---|---|---|
| `serde` **1.0.229** (feature `derive`) | the hand-written field readers in `src/log.rs` | `#[derive(Deserialize)]` on `Record`, `Body`, `RoomRecord`, `Via`, `VerbRecord`, `EdgeRecord`, `EventRecord`, `EdgeResult`, `Happening` |
| `serde_json` **1.0.151** | `src/json.rs`'s ~380-line reader | `log::parse` (`serde_json::from_str`, the recovered `de::from_trait`), `crate::json` (the alias over `serde_json`) |

`cargo` added exactly four lines to the root `Cargo.lock` (the `serde` /
`serde_json` edges of the `hydra_survey` package); no version moved.

### The schema that came back

Everything below is asserted by a test in `src/log.rs`:

* `Record` — `t`, `run`, `char`, `server` (`String`, required) and
  `#[serde(flatten)] body: Body`. The body's keys live in the *same* object, so
  `serde_json` hands the unknown ones to `Content` and `Body` reads its tag back
  out of it — the `Content` arm at `0x1013c30c0`.
* `Body` — `#[serde(tag = "kind", rename_all = "lowercase")]` with
  `Room(Box<RoomRecord>)`, `Verb(VerbRecord)`, `Edge(EdgeRecord)`,
  `Event(EventRecord)`: the `TaggedContentVisitor::new("kind", …)` shape at
  `0x1013c2674`, tags `room`/`verb`/`edge`/`event` in `VARIANTS` order.
* `RoomRecord` — the 17 `FIELDS` at `0x1044337e8`, in that order and with the
  per-arm types: `sid`/`title`/`desc`/`paths` are `String` (**`paths` is the
  room's text, not a list**), `uid`/`climate`/`terrain`/`overhead`/`sensed_exit`/
  `location` are `Option<String>`, `compass`/`exits`/`animals`/`tracked` are
  `Vec<String>`, `roommeta` is `BTreeMap<String, String>`, `via` is `Option<Via>`
  and `pos` is `Option<(i32, i32, i32)>`. The serialized default record pins all
  seventeen names and their order in one string literal.
* `Via` — `from`, `cmd` (two `String`s, `0x1013bc8f0`).
* `VerbRecord` — `sid`, `verb`, `lines`, `rt`, `cast_rt`, `prompt_closed`.
* `EdgeRecord` — `from`, `cmd`, `to`, `rt`, `dir`, `result`, `lines`, with
  `result: EdgeResult` and `rt: Option<String>` (the roundtime as printed).
* `EventRecord` — `sid`, `what: Happening`.
* `EdgeResult` (`arrived`/`failed`/`stayed`) and `Happening`
  (`died`/`healed`/`resumed`/`stopped`) — unit enums, `rename_all =
  "lowercase"`, read as the bare variant string (`"result":"arrived"`), which is
  what `ContentDeserializer::deserialize_enum` (`0x1013b6b34`) expects.

`Serialize` is derived too. The binary has **no** `Serialize` side for these
types (only `__Visitor::expecting`/`__FieldVisitor` symbols and the
`Content`-based deserialisers exist), so serialization is an addition, not a
recovery; it exists so the round trip can be tested and so a writer has a
schema to target.

### Behaviour that changed

1. **Error wording is serde's**, position suffix included. `log::parse` renders
   `serde_json::Error` with `to_string()`, so a missing room `sid` is
   `missing field \`sid\` at line 1 column 155` (was `missing field \`sid\``),
   an unknown `kind` is
   ``unknown variant `roomy`, expected one of `room`, `verb`, `edge`, `event` at
   line 1 column 69``. The *set* of messages is the same one the hand reader
   aimed at, because `serde`'s defaults are what it was imitating. `read` still
   drops every line it cannot parse, so only the exact text a caller sees
   changed.
2. **A bare number is no longer accepted where a `String` is expected.** The old
   reader took `{"sid":1710}` because "the log writes some ids bare". The
   recovered arms call `deserialize_string` (`0x1013b7ae0` for `sid`, and
   `deserialize_option` over a string for the optional ones), so the original
   rejected it: `invalid type: integer \`1710\`, expected a string`. No
   `deserialize_with` shim was added for this — the evidence points the other
   way — and the test `a_bare_number_is_not_a_string` pins the strictness.
   If Phase 3's captured logs turn out to carry bare ids, this is the one place
   to revisit (a `string-or-number` helper would be added *there*, and
   documented as a deliberate divergence from the recovered schema).
3. **`roommeta` values must be strings.** The old reader also took numbers,
   bools and `null` and stringified them; the arm is `deserialize_map` with a
   `String` on both sides (`0x1013b8038`).
4. **Every non-`Option` field is required.** The old reader defaulted a missing
   `result` to `arrived` and a missing `Vec`/map to empty; the recovered arms
   deserialise an enum (`0x1013b6b34`), a sequence (`0x1013b68d4`) and a map
   (`0x1013b8038`), so an absent key is ``missing field `result` `` now — same
   for `sid`, `verb`, `what`, `title`, `desc`, `paths`, `from`, `cmd`, `lines`,
   `compass`, `exits`, `animals`, `tracked`, `roommeta` and the four header
   fields.
5. **`Option<_>` fields still take `null` *and* absence** — `serde`'s
   `missing_field` helper turns an absent `Option` into `None` — asserted for
   `uid`, `via`, `pos`, `climate`, `terrain`, `overhead`, `sensed_exit`,
   `location`, `to`, `rt`, `dir`.
6. **The two verb roundtimes keep the old tolerance, deliberately.** The
   `VerbRecord` visitor deserialises `rt` and `cast_rt` through
   `Option::<T>::deserialize` (`0x1013cc7d4`, twice), i.e. the original accepted
   an absent or `null` roundtime, but this crate's public fields are `f64`.
   `log::roundtime` (a `deserialize_with`) folds "absent" and `null` into `0.0`,
   which is what the `std` stand-in did; a non-number is still an error.
   `prompt_closed` carries `#[serde(default)]` (absent → `false`) as an
   **explicit divergence** from the recovered schema: the `bool::deserialize`
   arm (`0x1013c9acc`) says the original required the key, but the recovered
   type of that slot is `bool` with no evidenced default, this crate's field has
   always been `bool`, and `log::read` drops a whole line it cannot parse — so a
   missing flag would lose a record instead of defaulting a flag. Both shims are
   tested in `a_verb_roundtime_may_be_written_or_left_out`.
7. **`crate::json` changed shape.** It is now
   `pub use serde_json::{Error, Map, Number, Value}` plus
   `parse(&str) -> Result<Value, Error>` and nothing else. Three parts of the old
   module could not be preserved and are documented breaks — nobody outside the
   crate used any of them:
   * `Value::kind()` and `wrong_type()` — serde produces the messages itself;
   * the `String` error type of `json::parse` — now `serde_json::Error`;
   * `Value::as_object()` / `Value::get()` now hand back `serde_json::Map` and
     keep the *document's* order no longer: the old `Value::Object` was a
     `Vec<(String, Value)>` in source order, `serde_json::Value` sorts object
     keys (`BTreeMap`, no `preserve_order` feature, which the pin does not
     enable). A log line is one object and nothing in the crate reads it as
     JSON any more (`log::parse` goes through the derived schema), so the order
     is not observable here — but a caller that wanted document order would
     notice.
     `Value`'s `as_str`, `as_array`, `get` and `Display` are otherwise
     `serde_json`'s own and unchanged.

### What remains a departure

* `probes::Answer`, `Step`, `Known::{seen,exits,via,pos,tried}` and
  `Seen::{marks,flags,lines,last}` are named here (items 3, 4 and 8 above).
* `movement::classify` is still re-implemented from compass words (item 5).
* `Serialize` is derived where the original had none (item 7).
* `VerbRecord`'s `rt`/`cast_rt`/`prompt_closed` accept an absent key (item 6) —
  a deliberate divergence, not a recovery.
* `log::parse` and the `json` module are our additions to the crate's API.
* Fields whose *types* were only ever inferred (`VerbRecord`'s tail) carry
  `// UNKNOWN:` at the declaration.
* Nothing is left of the `std` stand-ins: no hand-written parser, no
  hand-written field reader.

---

## Conflicts with the sibling notes

* `hydra_map/NOTES.md` documents `Vec`/`String` as `{cap, ptr, len}` and
  `Option<String>`'s `None` as `isize::MIN` in the capacity slot. This crate
  agrees, and adds the `BTreeMap` layout (root node `+0`, height `+8`, length
  `+0x10`) and the btree node layout (`keys @+0x10` at 24 bytes each, values at
  `+0x118` at the value type's stride, children at `+0xb10`, the node length in
  the six bytes before the children).
* The stub list in `lib.rs` described the serde impls as "unresolved symbols
  (trait impls, closures, generics)". They are recoverable: the *visitor*
  functions are real symbols (e.g. `RoomRecord::__FieldVisitor::visit_str` at
  `0x1013c3cec`) and the inlined bodies still carry the field names, the field
  indices and the jump table. That is where the whole schema came from.
