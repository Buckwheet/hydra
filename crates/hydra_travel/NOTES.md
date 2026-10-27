# `hydra_travel` — recovery notes

Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(demangled globals in `/Users/cpmbpro/Code/hydra-inspect/symbols_demangled.txt`).
The crate had 75 `todo!()` stubs over 40 files; nothing is left
(`grep -R "todo!()\|unimplemented!()" crates/hydra_travel/src` is empty) and

* `cargo check -p hydra_travel --offline` — clean, no warnings
* `cargo test -p hydra_travel --offline` — **167 tests pass**

**Phase 2 audit pass (this document's last section) re-read the whole crate
against the local symbol table.** It found three things the earlier reading got
wrong — the *function extents* in this file were taken from the globals-only file,
so several "N bytes of inlined …" claims were really "N bytes until the next
**global** symbol", i.e. whole functions merged with their neighbours, and the
crate's async half was missed entirely. See
[Phase 2 audit: coverage table](#phase-2-audit-coverage-table).

Phase 1 reconnected `serde` 1.0.229 and `serde_json` 1.0.151 (the `travel.json`
document); everything else in the crate is still std-only. See
"Phase 1 reconnection" and "`std`-only departures".

---

## Legend

| tag | meaning |
|---|---|
| **ESTABLISHED** | signature, control flow, literals and offsets read directly out of the instruction stream; asserted by a test. |
| **Partly recovered** | the entry contract, literals and helper calls are recovered, but a predicate or a data table could not be read out of the binary; the body implements the evidenced subset and says so at the site. |
| **UNKNOWN** | the binary shows the call shape but not the behaviour. A safe value is returned and a `// UNKNOWN:` comment records what *was* visible. |

Every uncertain decision has a comment at the site; this document collects them.

---

## Tooling

Read-only, all under `$TMPDIR` (nothing was written into the repo):

* `.work/recon.py` — the shared driver (`syms`, `disas`, `bytes`, `str`, `findstr`).
* **A local-symbol demangler.** The binary keeps **120 043** defined local
  symbols, but `symbols_demangled.txt` only lists 10 694 globals. A small legacy
  Rust demangler (`__ZN…E` → `a::b::c`, with the `$LT$`/`$u20$` escapes) applied
  to `nm -n --defined-only` produced the *private* function names, which is where
  most of the structure in these notes comes from: `walk::run::{Run::tick,
  Run::work, Job, Out, Deed, Pick, Until}`, `walk::mover::Mover`,
  `walk::pace::{typeahead_of, Batch::landed}`, `walk::recovery::Attempt`,
  `walk::replies::{word_after, numbered}`, `walk::routines::Next`,
  `routines::casting`, `routines::shopping`, `knows::*`, `file::read`,
  `file::key_in`, `file::spot_in`, `file::spot_name`, `sail::go`,
  `settings::said`, `settings::on`, `route::how`, `route::why_shut`,
  `route::not_known`, `cutter::names_the_walker`, … (628 `hydra_travel` rows).
* **A per-function annotator** over `objdump -d --no-show-raw-insn` that tracks
  `adrp`/`add`/`mov` constants so every `add`/`ldr` off a page base prints the
  string or scalar it addresses, and resolves every `bl` to its symbol. 323
  functions were dumped this way.
* **A whole-`__text` reference scanner** (`adrp`+`add`/`ldr` pairs over the
  entire section) giving `target → referencing functions`, used to find who
  reads a literal (183 871 distinct targets).
* **A `&str` table reader** that decodes the `__DATA_CONST` `(ptr, len)` arrays.
  This is how the swim direction table, the mural verse table and the day-pass
  town list were read exactly.
* **A length-prefixed piece decoder** for `__cstring`. Many `format!` pieces are
  stored as *`<varint length><text>NUL`* runs (one byte below 0x80, else
  `0xc0 | len>>8`, `len & 0xff`), e.g.
  `0x103e6beb4: 16 "travel.json is schema " c0 26 ", and this build of Hydra
  reads up to " c0 00`. Every `format!` string quoted below came out of that.

---

## Recovered types

```rust
// file.rs
pub const NAME: &str = "travel.json";   // 0x10241bc0b, len 11
pub const SCHEMA: u32 = 3;              // cmp w8, #0x3 at 0x101508f68

pub enum Refused { Read(String), Newer(u32), NoName }
// Display, verbatim:
//   "travel.json cannot be read: {}"                                 0x103e6bef5
//   "travel.json is schema {}, and this build of Hydra reads up to {}" 0x103e6beb4
//   "the character has no name a file can hold"                      0x10241bce0
// From<Refused> for io::Error: kind 0x15 (InvalidData) at 0x101505470.

// All four of the types below carry the recovered `Serialize`/`Deserialize`
// derives since Phase 1; `json::Value` is `serde_json::Value`.
#[derive(Serialize, Deserialize)]      // visitor at 0x101509934
pub struct Spot {                       // serde names from 0x101509934
    pub memories: BTreeMap<String, json::Value>,   // drop glue 0x10150a8c4
    pub targets: BTreeMap<String, Vec<u32>>,       // drop glue 0x10150a8c4
    pub last_room: u32,
}
#[derive(Serialize, Deserialize)]      // "struct Shared with 4 elements" 0x10241dfac
pub struct Shared {
    pub schema_version: u32,            // +0x48, 0x101508f68
    pub characters: BTreeMap<String, Spot>,
    pub instabilities: BTreeMap<String, Vec<walk::instability::Seen>>,
    pub memories: BTreeMap<String, json::Value>,   // UNKNOWN — see below
}

// walk/instability.rs — "struct Seen with 3 elements" 0x10241dfd6
#[derive(Serialize, Deserialize)]
pub struct Seen { pub room: u32, pub element: String, pub seen: u32 }  // 0x10241f4ee

// walk/routines.rs — the field names are the serializer's (0x101572368)
#[derive(Serialize, Deserialize)]
pub struct Routine { kind, rooms, hands_free_in, by_uid, enter, batter, puzzle,
                     route, errand, starts, landmarks, after, noun, open }
#[derive(Serialize, Deserialize)]
pub struct Landmark { noun, open, enter, after }  // 0x1024224bd
pub const KINDS: [&str; 31] = …        // 0x1024223ed..0x10242264d
```

`Vec`/`String` on this toolchain lay out as **`{cap @0, ptr @8, len @16}`** —
confirmed again here (`file::read` writes the error `String` at `+8/+0x10/+0x18`
of the sret and `routine_name` reads `ptr @+0x10`, `len @+0x18` of a
`serde_json::Value::String`), so an `Option<Vec<T>>`/`Option<String>` niche is
the word `isize::MIN` in the capacity slot.

---

## Recovered constants and tables

### `file.rs`

| what | value | evidence |
|---|---|---|
| file name | `travel.json` (len 11) | `0x10241bc0b`, `mov w3, #0xb` |
| schema | `3` | `cmp w8, #0x3` at `0x101508f68` |
| spot key | `format!("{}_{}", safe_component(a), safe_component(b))` | two `hydra_session::store::safe_component` calls then `format_inner` with the 2-piece list at `0x103e21bc4` |
| `Spot` fields | `memories`, `targets`, `last_room` | `Spot`'s `__FieldVisitor::visit_str` `0x101509934`: len 8 → `memories` (index `0x16`), len 7 → `targets` (`0x17`), len 9 → `last_room` (`0x18`) |
| `Shared` fields | `schema_version`, `characters`, `instabilities`, + 1 | `Shared::serialize` `0x10241bc46`/`0x10241bc54`/`0x10241bc2c`, and `struct Shared with 4 elements` |
| `forget_target` return | `Result<bool, _>` | tag at `+0` and the flag byte at `+1` of the sret (`0x101505664`) |

### `settings.rs` — the whole travel schema

Read verbatim out of the merged run at `0x10241c575`..`0x10241d520`
(`key` ∥ `title` ∥ `description` ∥ default ∥ options). The table in
`src/settings.rs` is that data; the notable entries:

| key | title | default |
|---|---|---|
| `use_urchins` | Use the urchin guides | off |
| `use_portmasters` | Use the portmasters | off |
| `use_own_ship` | Use your own ship | off |
| `call_wind` | Call wind into the sail | off |
| `use_seeking` | Use Voln's symbol of seeking | off |
| `ice` | Ice (options `""`/`run`/`wait`) | blank |
| `vaalor_shortcut` | Vaalor shortcut | off |
| `car_to_sos` / `car_from_sos` | Caravan to/from Sanctum of Scales | off |
| `use_day_pass` / `day_pass_sack` / `buy_day_pass` | day passes | off / blank / off |
| `fwi_trinket` | Mist Harbor trinket | blank |
| `rogue_password`, `key`, `keysack` | rogue guild / house key / key sack | blank |
| `allow_vornavis` | Allow Vornavis | off |
| `hate_familiar` | Send the familiar home | off |
| `mularos_lover`, `peregrine`, `journeys_end`, `safe_harbor`, `spindrift`, `sunset_cabin`, `black_swan`, `cuddfan_hollow`, `oleander_house`, `shivergale` | private properties | blank |
| `get_silvers` / `get_return_trip_silvers` | fetch silver | off |
| `use_gigas_hwtravel` / `gigas_min_number` | gigas fragments | off / `4` |
| `typeahead` | How many moves the game holds… | `2` |
| `delay` | Pause in each room | `0` |
| `stop_for_dead` | Stop beside the dead | off |
| `hinterwilds_location`, `redforest_location`, `duskruin_origin`, `ebon_gate_origin`, `talondown_origin`, `marksofthebeast_origin`, `fwi_return_room` | entry points | blank |

Recovered rejection / confirmation messages (piece tables):

```
{key} is on or off.                        0x103e63082
{key} is a whole number, {a} to {b}.       0x103e6c134
{key} is a number of seconds, {a} to {b}.  0x103e6c154
{key} is one of {list}.                    0x103e6c179
{key} is one of {list}, or blank.          0x103e6c1a5
{key} is a room number…                    0x103e6c1c0
{key} is back to {default}.                0x103e6c117
{key} is off.                              0x103e6c129
Travel has no setting {key}.               0x103e6c18a
Not saved: {e} / Nothing was changed: {e}  0x103e62c4d / 0x103e62c56
```

### `command.rs`

Verbs `go2`, `route2`, `sail`, `portal`, `exp`; flags `--global`, `--brief`;
messages

```
go2 where? .go2 bank, or .go2 targets for a list      0x10241f561
route2 where? .route2 bank                            0x10241f5f4
sail where? .sail kraken, or .sail help for the ports 0x10241f596
the course where? .sail route kraken                  0x10241f5d0
it needs a name: .go2 save den                        0x10241f52d
```

### `swim.rs`

`toward` reads the 8-entry `&[&str]` at `0x10443a128`:
`south, north, southwest, northeast, west, east, northwest, southeast` (lengths
5, 5, 9, 9, 4, 4, 9, 9). The recovered `"south"` arm returns element **1**
(`ldp x0, x1, [x8, #0x10]`), so the answer is the opposite of the pair.

### `sail.rs`

`Only the Captain can begin your voyage`,
`you are not aboard a ship, nor on a pier beside one.`, `you have left the ship.`,
`I find no course from here.`, `the ship would not depart.`,
`this is not your ship.`, `there is no ship here to board.`,
`I could not get aboard.`,
`the ship has come in at another port than the one I sailed for.`,
`I have lost the ship's place: look at the ocean, and sail again.`,
`the anchor will not go down.`, `the anchor will not come up.`,
`the sail will not stay out.`, `the wheel will not turn where I turn it.`;
commands `go gangplank`, `pull gangplank`, `push gangplank`, `cast sail`,
`lower sail`, `wheel`, `anchor`, `push capstan`, `kick capstan`,
`look ocean`, `watch ocean`, `tell eye to return`, `go barrier`, `go grotto`.

### `route.rs`

`not for this character`, `Hydra does not walk this yet`,
`the map gives it no cost`, `; (through the urchins)`,
`(through the urchins)`, `names you chose:`.

### `walk/instability.rs`

One pool run at `0x10241f460`:
`ATTUNE SET | gust of wind | air | burst of sparks | lightning | waft of heat |
earth | puff of rock dust | water | puff of mist | The air is oddly warped and
distorted here | Seen | room | element | seen`.
`element_in` needs the 10-byte needle `element of` and a trimmed length of at
least `0x22` (34).

### `walk.rs` — the movement refusals

`You cannot do that while mounted.`, `There's not enough room to do that!`,
`There is not enough room to stand up in here.`, `You'd tip the boat over!`,
`You are overburdened and cannot manage to stand.` (`0x10241e2d2`..`0x10241e35b`).

### `walk/routines.rs` — the routine vocabulary

The 31 `kind` words, the 14 `Routine` field names and the 20-deity mural table
(see `routines/mural_of_deities.rs`: 20 names at `0x10443cf78` stride `0x20`,
108 verse lines at `0x10443c8b8`; `Jastev`/`Koar` have eight verses and
`Ronan`/`Tonis` six, which is why `20 × 5 ≠ 108`).

`solver_for`'s two jump tables: a **byte** table of 16 entries at `0x1024207c0`
and a **u16** table at `0x1024207d0` (offsets `× 4` from `0x10156b80c` and
`0x10156bc10`); the intermediate tags are the niche words `isize::MIN + 1 … + 0xb`.

---

## Function table

`addr` is the unslid VA. Statuses: **full**, **partly**, **unknown**.

### `command.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `parse` | `0x101557e78` | full | 3664 B of inlined `char_indices`; verbs, flags and the five messages recovered. `named` (`0x101557c20`) joins the rest with `" "` and splits the first `=`. |

### `coverage.rs`, `coverage/variant.rs`, `coverage/variant_is.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `kinds` | `0x10154c15c` | full | the 8 recovered kind names, written through `variant::Name`. |
| `census` | `0x10154c758` | partly | counts rooms/exits/priced/routine rooms; the "priced" predicate is `Cost::price().is_some()`. |
| `report` | `0x10154cadc` | partly | renders the census; the exact header wording is not in `__cstring`. |
| `walked` | `0x10154cd90` | partly | counts the steps whose exit is priced and whose room has no routine. |
| `variant::Name as Write::write_str` | `0x10154e0d4` | full | appends every character but returns `Err` at the first one matching `c >= 0x30 && (c & !0x20) < 0x41` (the digit test). |
| `variant_is::Matching as Write::write_str` | `0x10154e20c` | full | progressive subsequence matcher: non-alphanumeric or a mismatched char is `Err`. |

### `facts.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `walker_from` | `0x1015240f8` | partly | the flag/profession/society/skill names are recovered (`stunned`, `hidden`, `invisible`, `mounted`, `urchin_access`, `premium_account`, `kneeling`/`sitting`/`prone`, the ten professions, `Order of Voln`, `Guardians of Sunfist`); the per-flag plumbing is not. |
| `listed_names` | `0x101525b9c` | partly | splits on `,` and ` and `. |

### `file.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `travel_path` | `0x101505b8` | full | 16 B: `dir.join("travel.json")`. |
| `save_target` | `0x1015054f8` | full (shape) | read/edit/write of the `targets` entry. Original: `hydra_session::store::changing` with a closure. |
| `forget_target` | `0x1015055c8` | full | `Result<bool, _>`; the flag byte is at `+1` of the sret. |
| `instabilities` | `0x1015056ac` | full | the `instabilities` map entry for the spot key. |
| `save_instabilities` | `0x101506240` | full | ditto. |
| `save_last_room` | `0x101506174` | full | the room id arrives in `w6`. |
| `save_memories` | `0x101505a18` | full | ditto. |
| `take_settings` | `0x101505ae0` | partly | reads the spot's `memories` into `TravelSettings`. |
| `load` | `0x10150858c` | partly | **the merge policy is not recoverable.** The body calls `spot_name` then `read`, clones the three maps and inserts the loaded entries one by one; this build returns the loaded image (and an empty one when the file is unusable). |
| `read` | `0x101508de8` | full | `fs::read_to_string` + serde; both failures become a `String`; a schema above 3 becomes `Refused::Newer`. |
| `key_in` / `spot_in` / `spot_name` | `0x101509198`… | full | `entry(..).or_default()` shapes; `spot_name` is `safe_component` twice plus `{}_{}`. |
| `Refused as Display::fmt` | `0x101510ea0` | full | the three texts above. |
| `From<Refused> for io::Error` | `0x101505338` | full | `Error::new(InvalidData, refused.to_string())`. |

### `route.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `here` | `0x10154fd20` | full | 4 B tail call to `hydra_map::follow::room_of`. |
| `described` | `0x101551634` | full | trim → ASCII-lowercase → split on `.` → map lookup (384-byte room stride). |
| `destination` | `0x10154ebc4` | full | same pipeline, last resolvable part wins. |
| `table` | `0x10154fd24` | partly | routes from the saved `targets`; the saved-step syntax (`; (through the urchins)`) is recovered, the parser is not. |
| `places` | `0x10155082c` | partly | the maps' priced rooms. |
| `itinerary` | `0x1015517e4` | partly | defers to `Trip::path_from`. |
| `how` / `why_shut` / `not_known` | `0x10154f7fc`, `0x10155138c`, `0x101551ccc` | partly | literals recovered, predicates partly. |

### `sail.rs`, `sail/voyage.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `wind_known` | `0x101531800` | full | matches the wind words. |
| `Runner::not_the_captain` | `0x101531af8` | full | returns the recovered literal. |
| `Runner::wheel_already_turned` | `0x101531c84` | full | ditto. |
| `guided` | `0x101531eb4` | partly | the refusal literals and the four commands are recovered; the predicate that picks between them is not. |
| `MakeReady::next` | `0x1015322cc` | partly | the four commands and the six `contains` closures are recovered; the trigger order is inferred from the command order. |
| `Voyage::next` | `0x101554f40` | partly | raise sail → wheel → wait → arrived; the "wheel is turned enough" predicate is not recoverable. |
| `Voyage::{wind,attend,wheel_wanted}` | `0x101555ac0`, `0x101555d78`, `0x101554da4` | partly | |

### `settings.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `key` | `0x10152a3c0` | full | table walk. |
| `TravelSettings::for_walker` | `0x101528ef8` | full | `shown` + `on_word` + `HashMap::insert`. |
| `TravelSettings::on` | `0x1015292cc` | full | goes through `on_word`. |
| `TravelSettings::set` | `0x1015293b8` | full | validates against the schema, `remove`s the empty value or `insert`s the new one, then `said`. |
| `TravelSettings::shown` | `0x10152a0e8` | full | one row per setting. |
| `TravelSettings::said` | `0x101529ec4` | full | the confirmation shapes above. |
| `settings::on` | `0x10152a2a0` | full | |
| `memory` | `0x10152aaf8` | partly | |
| `Memory::accept` / `Memory::called` | `0x10152a4b0`, `0x10152aa18` | partly | `No memory {key}.` recovered (`0x103e63126`). |
| `on_word` | `0x10152b780` | full | on/yes/true vs off/no/false (no single-digit arms). |

### `swim.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `toward` | `0x101532e88` | full | the 8-entry table above. |
| `strokes` | `0x101533020` | partly | the roundtime handling is recovered (`Cast Roundtime`), the stroke count is not. |

### `walk.rs` — `Trip`

| symbol | addr | status | notes |
|---|---|---|---|
| `ahead` | `0x101542828` | full | 96 B. |
| `aside` | `0x101542888` | full | 192 B. |
| `cannot_send` | `0x101541e10` | full | the five refusal literals. |
| `heard` | `0x101542948` | partly | |
| `tick` | `0x101542670` | partly | |
| `to` | `0x1015423e4` | partly | |
| `seeded` | `0x101542bf8` | full | compares the path with the saved target. |
| `path_from` | `0x101542dbc` | partly | 24504 B: the A* with `Cost` prices and routine rooms excluded; this build searches breadth-first with the same exclusions. |

### `walk/drive.rs`, `walk/drive/preflight.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `routine_name` | `0x10153831c` | full | `name` from the serialized routine, `_` → ` `, else the literal `way`. |
| `gone` | `0x101538570` | full | did the room change. |
| `is_a_move` | `0x101538650` | partly | 31448 B of compare chain; the exit word is used instead. |
| `preflight::halted` | `0x101557924` | partly | |

### `walk/hands.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `cast_commands` | `0x10154e7c4` | full | `Option<Vec<String>>`; `prepare {name}` + `cast {name}` with the name lowercased. |
| `store_commands` | `0x10154e9b0` | full | `store right`, then `store left`. |
| `take_back` | `0x10154ea10` | partly | walks the four container slots. |

### `walk/instability.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `senses` | `0x101556f08` | full | phrase table. |
| `element_in` | `0x101555f40` | full | needle + length floor. |
| `element_given_off` | `0x101556134` | partly | |
| `asked` | `0x101556534` | partly | only the 11-byte needle `instability` is recovered. |
| `narrow` | `0x10155680c` | partly | |
| `half_tag` | `0x101557810` | partly | two 64-byte tables not recovered. |

### `walk/kept.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `taken_from` | `0x101526e50` | partly | the seven shop names → the kept item. |
| `with_items` | `0x101527090` | full | |
| `language_in` | `0x101527878` | partly | `Guildspeak`. |
| `put_back` | `0x101527c10` | full | |
| `is_spoken` | `0x101527cbc` | partly | |

### `walk/preflight.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `silver_for` | `0x101528238` | partly | the ` banker for ` / ` silvers` pieces and the two silver settings. |
| `gigas_least` | `0x1015285cc` | full | default `4`. |
| `gigas_way` | `0x101528c70` | partly | |
| `out_of_a_shop` | `0x101528788` | partly | seven shop names. |
| `withdraw_command` | `0x101528bcc` | full | `withdraw {amount}`. |

### Supporting modules revealed by the local symbol table

`walk/mover.rs`, `walk/pace.rs`, `walk/recovery.rs`, `walk/replies.rs`,
`walk/run.rs` and `knows.rs` are new files: the stubs did not carry them, but the
binary has 17 functions in them that the 75 stubs call. They are implemented at
the same fidelity as the rest (**partly**, with the recovered literals:
`Race`/`See`/`then`/`Rung`/`Exits`/`Are`/`Unported`, the `first … twelfth`
ordinals, `go`/`climb`/`open`/`stow`, `Roundtime`, the `typeahead` description).

### `walk/routines.rs` and the 29 routines

| what | addr | status | notes |
|---|---|---|---|
| `Solver::{next,keep}` | per routine; defaults `0x101523b00`, `0x1015337e4`, `0x101557bac`, `0x10156cc08`… | full (trait) | `keep` is a defaulted method. |
| `Next` | — | partly | 32-byte enum, `String` payload, non-`String` tags `isize::MIN + n`; variant *names* not recoverable. |
| `routines::Seen::answered` | `0x10156ca94` | full | scans `Vec<ChunkLine>`, `contains` per line. |
| `routines::holds` | `0x10156cb54` | full | |
| `solver_for` | `0x10156b7a0` | partly | room lookup, the two jump tables, `DayPass::new`, `SwordGorge::new` and the `Isle of Four Winds`/`Mist Harbor` literals are established; the tag → solver mapping is not, so the dispatch is on the routine's `kind` word. |
| 29 `*.rs` routines | see `src/walk/routines/` | partly | each file's doc comment records its own evidence. `mirror`, `ring_wedges`, `colour_barrier`, `flight_of_steps`, `familiar_doors`, `minotaur_maze`, `bronze_gate`, `rolaren_gate`, `rune_staircase`, `labyrinth_entry`, `vaalorn_door`, `crown_door`, `mural_of_deities`, `day_pass` and `eye_spy_runes` have their command sequences recovered verbatim; `altar_levers`, `bridge_wheel`, `confluence`, `cutter`, `giant`, `guild_password`, `patrol`, `search_rooms`, `seeking`, `signposts`, `sword_gorge`, `three_pillars`, `trinket` and `workshop_pillars` have **no** command literal and answer `Next::Wait` with an `// UNKNOWN:` note. |

---

## `std`-only departures

Rows struck through are **gone**: Phase 1 put the real crate back (see
"Phase 1 reconnection" below for the versions and the behaviour that came with
them).

| original dependency | recovered use | what this build does |
|---|---|---|
| ~~`serde` + `serde_core::de::Visitor`~~ | ~~`file::{Spot,Shared}`, `walk::instability::Seen` (all four `expecting` symbols are in the crate's unresolved list)~~ | **Reconnected.** `#[derive(Serialize, Deserialize)]` on `file::{Spot,Shared}`, `walk::instability::Seen` and `walk::routines::{Routine,Landmark}`; the field names, order and defaults are the recovered ones. |
| ~~`serde_json`~~ | ~~`travel.json`, `routine_name`'s `Value` index~~ | **Reconnected.** `crate::json` is now a three-line alias over `serde_json`: `crate::json::Value` *is* `serde_json::Value`, and `file::{read,write}` go through `from_str`/`to_string`. |
| `hydra_session::store::changing` | read/edit/write of `travel.json` | **Kept (seam).** `file::changing(dir, closure)`: read, edit, write. `hydra_session::store::changing` does not exist in the reconnected `hydra_session` — `hydra_session::store` exposes `safe_component`, `character_path`, `temp_extension`, `thread_hash` and `save_bytes`, and has no `changing` counterpart — so the read/edit/write cycle lives here. If `changing` comes back to `hydra_session`, `file::changing` becomes a two-line call to it. |
| `cena_map` (external crate) | `Map::room`, `Cost::price`, `Cond::ask`, `cena_map::locate`, `routine::Routine` | **Kept, deliberately.** `cena_map` (and `cena_map_layout`) are **not in the workspace pins** (root `Cargo.toml` `[workspace.dependencies]`), so there is nothing to reconnect to. `crate::map` stays: a room index with the same accessors (`room`, `room_id`, `area`, `exits`, `routines`). The routine *field names* and *kind words* are the recovered ones and its serde shape is real (see above). |
| `hashbrown`/`BTreeMap` in `TravelSettings` | settings map | `std::collections::BTreeMap` (the original used a `HashMap` for `for_walker`; the ordering is not observable). Not a workspace pin, and not worth adding `hashbrown` for. |
| `tokio` | none observed in this crate | n/a — nothing async to reconnect, so the pin was not added. |
| `regex` | none observed in this crate | n/a — no `regex` call site in this crate, so the pin was not added. |

`hydra_model`, `hydra_protocol` and `hydra_session` are used as path
dependencies (allowed and encouraged): `GameState`, `state::lines::ChunkLine`,
`spells::spell_named`/`spell` and `store::safe_component`.

---

## Phase 1 reconnection

Both dependencies are workspace pins, declared as `x = { workspace = true }` in
this crate's `Cargo.toml` and used through `cargo --offline`:

| real crate (pin) | replaced | where |
|---|---|---|
| `serde` **1.0.229** (feature `derive`) | the hand-written `to_json`/`from_json` bodies | `file::{Spot, Shared}`, `walk::instability::Seen`, `walk::routines::{Routine, Landmark}` |
| `serde_json` **1.0.151** | `src/json.rs`'s parser/writer and `Value` | `file::{read, write, to_json, from_json}`, `walk::instability::Seen::{to_json,from_json}`, `crate::json` (the alias), `settings::TravelSettings::{from_memories, memories}` |

`Cargo.lock` gained the two dependency edges of `hydra_travel` and nothing else.

### Why these two are the *recovered* ones, not a guess

* The binary carries the derive machinery for exactly the three types this crate
  persists: `<impl Deserialize for file::Spot>::Visitor::expecting`
  (`0x101509934`, literal `struct Spot` at `0x10241bc8b`),
  `<… for file::Shared>` (`0x10241bc96`, field count
  `struct Shared with 4 elements` at `0x10241dfac`) and
  `<… for walk::instability::Seen>` (`0x10241dfd6`, `struct Seen with 3
  elements`). A hand-written reader would not produce `expecting`/field-count
  constants.
* The file format is `serde_json`: `serde_json::de::from_trait` at `0x101537760`
  is what `file::read` calls, and `serde_json::ser::Serializer` is inlined into
  `file::Shared::serialize` (`0x101506304`).
* The free-form maps are `serde_json::Value`: `drop_in_place<file::Spot>`
  (`0x10150a8c4`) drops a `BTreeMap<String, serde_json::Value>`, and
  `walk::drive::routine_name` (`0x10153831c`) reads a `Value::String`'s
  `ptr`/`len` at `+0x10`/`+0x18` of it.

### Behaviour that changed as a direct result of using the real crates

1. **On-disk field order.** The old writer built a `BTreeMap`-backed `Value`, so
   the document came out alphabetically (`characters`, `instabilities`,
   `schema_version`). `serde_json::to_string(&Shared)` writes the *recovered*
   serializer order — `schema_version`, `characters`, `instabilities`, then
   `memories` when it is not empty — which is what `Shared::serialize` at
   `0x10241bc46`/`0x10241bc54`/`0x10241bc2c` does. The names, nesting, and the
   single trailing newline are unchanged; `file::tests::on_disk_document_is_the_
   recovered_field_order` pins the bytes both with and without a non-empty root
   `memories`.
2. **Integer fidelity in `memories`.** The hand-written `Value` kept every number
   as `f64`, so anything past 2^53 was rounded; `serde_json` keeps the integer
   and round-trips it exactly (`json::tests::integers_stay_exact`).
3. **Broken-file message.** It is still `Refused::Read(String)`, but the string
   is `serde_json`'s (`expected value at line 1 column 3`) instead of the old
   `… at byte N`. A document with **no** `schema_version` is now
   `missing field \`schema_version\`` from `serde`, not the stand-in's
   `missing schema_version`; it is still a refusal, because `schema_version` is
   the one field without `#[serde(default)]`.
4. **Malformed spot rows fail the whole file.** The old reader `filter_map`ped a
   non-integer out of a `targets` path; `Vec<u32>` now rejects the document
   (`invalid type`), which is what the derived `Deserialize` the binary carries
   would do.
5. **Absent fields still default** (`#[serde(default)]`): the three `Spot` fields,
   `Seen::seen`, `Shared::{characters,instabilities,memories}` and every
   `Routine`/`Landmark` field. Unknown keys are ignored, as before.
6. **Free-form object key order inside `memories` is normalised** to
   `serde_json`'s default `Map` (`BTreeMap`), i.e. sorted. The old `Value` did
   the same, so nothing regressed; adding `serde_json`'s `preserve_order`
   feature would change it and was not done.

### `Routine`/`Landmark`

`cena_map`'s routine type was a serde document (`routine_name` reads a
deserialized routine), so the *serialization* is recoverable even though the
crate itself is not pinned: `walk::routines::{Routine, Landmark}` now derive
`Serialize`/`Deserialize` in the recovered declaration order (the serializer at
`0x101572368` writes `kind`, `rooms`, `hands_free_in`, `by_uid`, `enter`,
`batter`, `puzzle`, `route`, `errand`, `starts`, `landmarks`, `after`, `noun`,
`open`). **INFERRED:** whether the original skipped a `None` field; the bare
derive writes `null`.

### Tests

`cargo test -p hydra_travel --offline` is **164 passing** (154 before the pass,
plus ten new ones; the three `src/json.rs` tests were rewritten in place because
there is no hand-written parser left to test):

* `json::tests` — the alias is really `serde_json::Value`, integers stay exact,
  garbage is rejected.
* `file::tests::{spot,shared}_round_trips_through_serde` — derive round trips,
  free-form `memories` included, with the exact serialised text.
* `file::tests::absent_fields_default_and_unknown_fields_are_ignored`.
* `file::tests::schema_three_is_read_and_anything_newer_is_refused`.
* `file::tests::on_disk_document_is_the_recovered_field_order`.
* `file::tests::free_form_memories_survive_a_change` — a foreign `memories`
  object survives `read` → edit → `write` intact, and `take_settings` still
  reads the setting words out of it.
* `walk::instability::tests::seen_serialises_in_the_recovered_order`,
  `…::seen_defaults_and_ignores_unknown_fields`.
* `walk::routines::tests::routine_serialises_in_the_recovered_field_order`,
  `…::routine_reads_a_partial_document`.

---

## Evidence conflicts and open questions

1. **`Shared`'s fourth field.** The serializer writes three names; the
   deserializer's field list says four. The only candidate literal is `memories`
   (`0x10241dff8`), which is why it is carried as free-form JSON and preserved on
   rewrite. Nothing reads it.
2. **`waft of heat` and `earth`.** Five element phrases, four element names.
   The pool order pairs `waft of heat` with `earth`; semantically it should be
   `fire`, which would make the element set five and disagree with
   `element_in`'s phrase table. Recorded, not resolved.
3. **`swim::toward` returns the *opposite* direction.** The `"south"` arm
   demonstrably returns table element 1 (`north`). That is what the instructions
   say; it may be a bug that shipped.
4. **`sail::NOT_THE_CAPTAIN` length.** The `mov w1, #0x28` at `0x101531af8`
   passes 40 for a 38-character literal, so the immediate is not the string
   length (or two trailing bytes are part of the slice). The text is asserted;
   the length is not.
5. **`variant::Name` rejects digits.** The recovered predicate is
   `c >= 0x30 && (c & !0x20) < 0x41`, which also covers `0x3a..=0x40`; that is
   reproduced literally rather than "fixed" to `is_digit`.
6. **`Next`'s variant names.** The tag values are known (`MIN+1 … MIN+0xb`), the
   names are not. `Say`/`SayBoth`/`Wait`/`Done`/`Stuck` are this build's.
7. **The `Routine` enum of `cena_map`.** The recovered serializer shows
   `Routine` carries `kind` plus the 13 other fields, and the 31 kind words; the
   original enum shape (`Errand`, `Puzzle`, `Opening`, `Landmark`) is recorded
   here but flattened into one struct.

8. **`walk::hands` versus `hydra_model`'s `Containers`.** The binary's
   `Containers::ready` takes a slot number and returns the slot's container
   (`Option<&str>` at `0x101bdc9ac`), while the already-committed
   `hydra_model::state::containers::Containers::ready` takes a line and returns
   `bool`. `store_commands` and `take_back` are therefore reconstructed against a
   simplified seam with `// UNKNOWN:` at the call sites; the recovered call
   shapes (`fn(&GameState) -> Vec<String>` and
   `fn(&GameState, &str) -> Vec<String>`) are recorded there. Fixing this needs a
   change to `hydra_model`, which is outside this crate's scope.
9. **`route::not_known`, `coverage::report`, `walk::pace::typeahead_said` and
   `walk::recovery::Attempt::react`.** Their predicates and counts are recovered,
   but the exact output wording is not in the literal pool. The reconstructed
   sentences are labelled `// UNKNOWN:` in the sources and are not presented as
   evidence.

---

# Phase 2 audit: coverage table

## Inventory method and counts

Two sources, deduplicated by symbol, not by eyeball:

1. `symbols_demangled.txt` — the 113 `hydra_travel` lines (globals: `T` symbols).
2. `nm -a hydra | rustfilt` — **395** `hydra_travel` text symbols (`T`+`t`), i.e.
   **282** symbols the globals file never lists: private functions
   (`command::named`, `file::read`/`key_in`/`spot_in`/`spot_name`,
   `route::{how,why_shut,not_known}`, `sail::go`, `settings::{on,said}`,
   `knows::*`, `walk::routines::holds`, …), every `{{closure}}`, every
   `Solver`/`Write`/`Deserialize` trait impl, and all `drop_in_place` glue.

Extents come from the local table (`n` symbols are true function starts), so
**every size quoted here and above is a real function size**. The 113 globals are
a strict subset of the 395 locals; nothing needed to be reconciled between them.

**Deduplication rule.** 395 local symbols collapse to **374** inventory rows by
collapsing *true monomorphisations only*:

* `walk::drive::Driver<F, W>` instantiated at `F=W=hydra_table::travel_command::
  set_out::{{closure}}::{{closure}}` and at `…::sea::walk::{{closure}}::{{closure}}`
  — the two instantiations of one generic item, so the generic spelling
  `Driver<F,W>` is the row and the two addresses are recorded with it;
* `core::ptr::drop_in_place<…>` for the same type at different codegen units
  (`Vec<walk::drive::Taken>` appears 5×, `Vec<walk::instability::Seen>` 12×,
  `Solver::keep` 27×): one row each;
* `sail::sail<W,W>`, `sail::guide<W>`, `walk::drive::walk<W,W>` — one row each.

Excluding `drop_in_place` glue, the table below covers **251** rows.

`n=` in the table is the number of local symbols in that row; `B` is the smallest
of their sizes (they are equal except for monomorphisations).

## Status vocabulary

* **implemented** — behaviour read out of the instruction stream and asserted.
* **partial** — the door, the literals and the call graph are recovered, but a
  predicate/table sits in an out-of-crate dependency or is not in the dump; the
  site says which.
* **unknown** — the binary shows the call shape, not the behaviour.
* **missing** — present in the binary, absent (or default-only) in this build.
* **fixed** — the status changed in this pass.

## `command.rs`, `coverage.rs`, `facts.rs`, `file.rs`, `knows.rs`, `settings.rs`, `swim.rs`

| symbol | B | n | status | evidence / what is missing |
|---|---|---|---|---|
| `command::parse` | 3664 | 1 | implemented | verbs/flags/messages recovered |
| `command::parse::{{closure}}`, `command::named` | 68/… | 3 | implemented | `named` joins and splits the first `=` |
| `coverage::kinds` | 124 | 1 | implemented | the 8 kind names via `variant::Name` |
| `coverage::census` | 532 | 1 | partial | "priced" predicate inferred |
| `coverage::report` | 788 | 1 | partial | header wording not in `__cstring` |
| `coverage::walked` | 404 | 1 | partial | counts priced steps in routineless rooms |
| `coverage::variant::Name as Write::write_str` | 200 | 1 | implemented | the `c >= 0x30 && (c & !0x20) < 0x41` reject |
| `coverage::variant_is::Matching as Write::write_str` | 160 | 1 | implemented | progressive subsequence |
| `facts::walker_from` | 2892 | 1 | partial | flag/profession names recovered, plumbing not |
| `facts::listed_names` | 288 | 1 | partial | splits on `,` and ` and ` |
| `file::travel_path` | 16 | 1 | implemented | `dir.join("travel.json")` |
| `file::save_target` | 152 | 1 | implemented | read/edit/write of one `targets` entry |
| `file::forget_target` | 152 | 1 | implemented | `Result<bool, _>`, flag at `+1` |
| `file::{instabilities,save_instabilities,save_last_room,save_memories,take_settings}` | 204… | 5 | implemented | spot map entries |
| `file::load` | 1260 | 1 | partial | merge policy not recoverable; returns the loaded image |
| `file::read` | 1436 | 1 | implemented | `read_to_string` + serde + schema 3 |
| `file::key_in`, `file::spot_in`, `file::spot_name` | 92/… | 3 | implemented | `entry().or_default()`; `safe_component` twice |
| `file::Refused as Display::fmt` | 332 | 1 | implemented | the three texts |
| `file::<impl From<Refused> for io::Error>::from` | 52 | 1 | implemented | `InvalidData` |
| `file::Shared::serialize` | 1408 | 1 | implemented (serde) | recovered field order |
| `file::Spot`/`Shared` `__Visitor::expecting`, `__FieldVisitor::visit_str` | 20/… | 3 | implemented (serde) | `struct Spot`, `struct Shared with 4 elements` |
| `knows::{known_spells,deciding_ranks,affordable_spells}` | 412/… | 3 | implemented | spell/rank plumbing over `hydra_model::spells` |
| `settings::key` | 2276 | 1 | implemented | schema table walk |
| `settings::TravelSettings::{for_walker,on,set,shown}` | 356… | 4 | implemented | validation, `said`, one row per setting |
| `settings::TravelSettings::said` | 876 | 1 | implemented | the confirmation shapes |
| `settings::{on,on_word,memory,Memory::accept,Memory::called}` | 100… | 5 | implemented | `No memory {key}.` |
| `swim::toward` | 408 | 1 | implemented | the 8-entry opposite table |
| `swim::strokes` | 468 | 1 | partial | roundtime handling recovered, stroke count not |
| `swim::sensed` | 296 | 1 | **missing → none** | local-only symbol (`0x10121448c`); nothing in `swim.rs` corresponds — it is a `Sensed` builder over the `instability` phrase table and needs that module's `Sensed` type, which is also absent |

## `route.rs`

| symbol | B | n | status | evidence / what is missing |
|---|---|---|---|---|
| `route::here` | 4 | 1 | implemented | tail call to `hydra_map::follow::room_of` |
| `route::described` | 432 | 1 | implemented | trim → lower → split `.` → map lookup |
| `route::destination` | 2840 | 1 | partial | **re-read**: goes through `described`, `Map::ids_for_uid`, `Map::room` **and** `Map::routes`; our build stops at the first two. Not "4444 B" |
| `route::table` | 2824 | 1 | partial | **re-read**: a *report* renderer — header `"STEP      TRIP    TIME  "` (`0x10241efc0`) and `"[status] \| HP \| MP \| RT \| CT \| L \| R "` (`0x103e23aeb`), one 120-byte row per route. Row type is `cena_map::route`, out of crate |
| `route::places` | 2912 | 1 | partial | builds `BTreeMap<&str, String>` + `Map::routes` + an upper-case step; our build answers the priced rooms. Not "3592 B" |
| `route::itinerary` | 1256 | 1 | partial | `Map::routes` → `Routes::exits_to` → `Cost::price` → `coverage::walked` → `route::how`, collected into `Vec<route::Leg>`. Not "11004 B"; `Leg` is absent here |
| `route::how` | 1316 | 1 | partial | `join_generic_copy` over `Vec<String>`; the fragments are recovered, the join order is this build's |
| `route::why_shut` | 680 | 1 | partial (seam) | asks `Cond::ask` over a `Vec<*const Cond>` built from 0x40-byte records, calls `not_known`, formats once. `Cond` is out of crate; the three refusal literals are in the pool but the selecting branch is not in the dump |
| `route::not_known` | 160 | 1 | **fixed (partial)** | **it is a predicate over a `Cond` tree returning `bool`** (`ask() == 2`), not a 9748-byte sentence builder. Rewritten docs; body is a seam because `Cond` has no counterpart here |

## `sail.rs`, `sail/voyage.rs`

| symbol | B | n | status | evidence / what is missing |
|---|---|---|---|---|
| `sail::wind_known` | 288 | 1 | implemented | wind words |
| `sail::Runner::not_the_captain`, `…::wheel_already_turned` | 100/… | 2 | implemented | the two literals |
| `sail::guided` | 1564 | 1 | partial | refusal literals + four commands recovered |
| `sail::go` | 208 | 1 | partial | entry point |
| `sail::MakeReady::next`, `::next::{{closure}}` | 2544 | 7 | partial | the six `contains` triggers, order inferred |
| `sail::{sail<W,W>, guide<W>, voyage<W,W>}` | — | 6 | **missing** | only drop glue survives (`drop_in_place<sail::sail<…>::{{closure}}>`); the async drivers are in this pass's largest gap |
| `sail::Runner` (`+ drop_in_place`) | — | 6 | **missing** | `Runner::{send,hold,out_of_roundtime}::{{closure}}` exist and call `tokio::time`, `tokio_util::CancellationToken`, `hydra_session::command::SessionHandle::{join,write}`, `hydra_model::state::clock::in_roundtime`, `broadcast::Receiver::recv` — an async runner with no counterpart in this build |
| `sail::voyage::Voyage::{next,wind,attend,wheel_wanted}`, `next::{{closure}}` | 1148 | 5 | partial | raise sail → wheel → wait → arrived; the "turned enough" predicate not recoverable |

## `walk.rs` — `Trip`

| symbol | B | n | status | evidence / what is missing |
|---|---|---|---|---|
| `Trip::ahead`, `Trip::aside` | 96/192 | 2 | implemented | `path[at+1]`, `path[at+2]` |
| `Trip::cannot_send` | 160 | 1 | implemented | the five refusals |
| `Trip::to` | 284 | 1 | implemented | records and answers the command |
| `Trip::tick` | 440 | 1 | implemented | delegates to `tick_inner` |
| `Trip::tick_inner` | 4364 | 1 | partial | calls `plan`, `replan`, `step_from`, `send_ahead`, `run::Run::tick`, `pace::{Batch::landed,typeahead_said}`, `aside_said`, `give_up`. Our `tick` is a simplified loop |
| `Trip::heard` | 424 | 1 | partial | `hydra_model::movement::classify`; our build only matches the refusals |
| `Trip::seeded` | 304 | 1 | implemented | compares the path with the saved target |
| `Trip::path_from` | **488** (not 24504) | 1 | **fixed (partial)** | seeds `vec![from]`, calls `cena_map::route::<impl Map>::routes` then `Routes::path_to`. **The A\* is in `cena_map`, not here.** Docs corrected; BFS kept as the documented stand-in |
| `Trip::{plan,replan,step_from,give_up,aside_said,exit_between}` | 368…1332 | 6 | **missing** | all six are local-only symbols; `exit_between` is the priced-step walk (`Cost::price` + `coverage::walked`), `plan` is `Map::routes` + `Routes::path_to` |

## `walk/drive.rs` — the async `Driver` tree (the largest gap)

The binary's `walk::drive` is **33 symbols** of an async, generic driver:
`Driver<F, W>` where `F`/`W` are the caller's finder/writer closures (instantiated
at `hydra_table::travel_command::{set_out, sea::walk}::{closure}::{{closure}}`).

| symbol | B | n | status | evidence |
|---|---|---|---|---|
| `drive::routine_name` | 432 | 1 | implemented | `Value["name"]`, `_` → ` `, else `way` |
| `drive::gone` | 224 | 1 | implemented | did the room change |
| `drive::is_a_move` | 31448 | 1 | partial | **synth compare chain**; our `command_for` uses the map's own exit word |
| `drive::preflight::halted` | 144 | 1 | partial | passes the refusals through |
| `Driver::walker` | 388 | 2 | **missing** | 2 addresses (2 instantiations) |
| `Driver::locate` | 128 | 2 | **missing** | |
| `Driver::fold` | 692 | 2 | **missing** | folds a `hydra_session::event::Event` into the walker |
| `Driver::show` | 408 | 2 | **missing** | `alloc::fmt` + `hydra_session::notice::Notice::line` |
| `Driver::price_the_way_out` | 912 | 2 | **missing** | |
| `Driver::{send,turn,walk,hold,drain,rejoin,out_of_roundtime,until_the_room_is_told}::{{closure}}` | 312…10640 | 22 | **missing** | every one of them is an async fn over `tokio::time::{Instant,sleep}`, `tokio_util::sync::CancellationToken`, `tokio::sync::broadcast::Receiver<hydra_session::event::Event>` and `hydra_session::command::SessionHandle::{join,write,say}`. `turn` alone is 6 104/10 640 B and is the real walk loop |
| `drive::deeds::{put,cast,deed,fill_hands,cast_to_be_carried,affords}` | 204…8508 | 11 | **missing** | `deed` calls `hands::store_commands`, `kept::{taken_from,put_back,language_in,is_spoken}`, `hydra_model::spells::spell_named` |
| `drive::solve::{walk_to,await_line,ask_answered,aside,ask_solver,solve}` | 336…2532 | 10 | **missing** | |
| `drive::preflight::{preflight,silver,by_gigas,day_passes,out_of_a_shop,by_way_of_the_bank,silver_needed,warn}` | 204…9632 | 13 | **missing** | |
| `drive::confluence::{look_for,nearest,nearest_half,enter_the_plane,find_instability}` | 348…1544 | 6 | **missing** | `look_for` calls `instability::element_given_off`, `hydra_map::follow::room_of` |
| `drive::walk<W,W>` | — | 2 | **missing** | 14 404 B closure: `Trip::{to,seeded}`, `map::routes`, `Routes::seconds_to`, `preflight`, `hands::take_back`, `instability::{senses,narrow,element_in}` |
| `drive::{Ended,Setting,Taken,Walked}` | — | 4 | **missing** | only drop glue survives |

**Why this was left unimplemented.** These are `async fn`s. Reproducing them
means (a) `hydra_travel` taking the `tokio`/`tokio_util` pins, (b) a
`Driver<F, W>` type whose bounds are the caller's closure futures, and (c) an
async `hydra_session::command::SessionHandle` (`join`/`write`/`send`/`say`) plus
`hydra_session::event::Event` and `hydra_session::notice::Notice`. (c) is a public
API question in a sibling crate, so nothing here was changed; the seam is
documented instead. **This is the crate's largest hole and it is not a detail: the
walk loop the binary runs lives in `drive::Driver::turn`, not in `Trip`.**

## `walk/{hands,instability,kept,mover,pace,preflight,recovery,replies,run}.rs`

| symbol | B | n | status | evidence / what is missing |
|---|---|---|---|---|
| `hands::cast_commands` | 412 | 1 | implemented | `prepare {name}`, `cast {name}`, lower-cased |
| `hands::store_commands` | 132 | 1 | implemented | `store right`, `store left` |
| `hands::take_back` | 360 | 1 | partial | four container slots; `Containers::ready` mismatch (see open question 8) |
| `instability::senses` | 2312 | 1 | implemented | phrase table |
| `instability::element_in` | 500 | 1 | implemented | 10-byte needle `element of`, length floor 34 |
| `instability::element_given_off` | 536 | 1 | partial | `StrSearcher` over the phrase table |
| `instability::asked` | 728 | 1 | **fixed (partial)** | four words by byte length → tags `0/1/2/3` (`confluence`, `confluence-hot`, `confluence-cold`, `instability`), plus a fall-through that needs the caller's tag and compares the room's `paths[0]` with `"[Elemental Confluence]"`. The invented `instability → Earth` mapping is gone; the return type is now `Option<Asked>` |
| `instability::narrow` | 1788 | 1 | partial | `Map::room` + `Vec<Sensed>`; `Sensed` is missing here |
| `instability::half_tag` | 276 | 1 | **fixed** | **it is a predicate, not a builder.** Two `&str` tables read out of `__DATA_CONST` (`0x10443b8a8` = `air/earth/ice/water`, `0x10443b8e8` = `fire/lava/steam/lightning`), selected by `csel … ne` on the first argument, and compared with `format!("huge {word} elemental")`. The fabricated `["","","",""]` table is gone |
| `instability::instability_table` | 296 | 1 | **missing → none** | thread-local `HashMap` built from `Map::routes` (`0x10118bc68`); needs `cena_map::route` |
| `instability::Seen::serialize` + `__Visitor::expecting` | 488/20 | 2 | implemented (serde) | recovered field order |
| `kept::taken_from` | 576 | 1 | partial | seven shop names → the kept item |
| `kept::with_items` | 2024 | 1 | implemented | |
| `kept::language_in` | 920 | 1 | partial | `Guildspeak` |
| `kept::is_spoken` | 332 | 1 | partial | |
| `kept::put_back` | 172 | 1 | implemented | |
| `mover::{Mover::new,Mover::tick,is_stunned,exchange_of}` | 176… | 4 | implemented | |
| `pace::{typeahead_of,typeahead_said,Batch::landed}`, `pace::<impl Trip>::send_ahead` | 456… | 4 | partial | description recovered, exact wording not |
| `preflight::{silver_for,gigas_least,gigas_way,out_of_a_shop,withdraw_command}` | 164…1092 | 5 | partial | `gigas_least` default `4`; `gigas_way` uses `Map::ids_for_uid` |
| `recovery::{Attempt::new,Attempt::again,Attempt::react,with_verb}` | 176… | 4 | partial | |
| `replies::{word_after,numbered}` | 208/272 | 2 | implemented | |
| `run::{Run::tick,Run::work,said,filled}` | 136… | 4 | partial | |
| `walk::Said`, `walk::drive::{Taken,Ended,Setting,Walked}`, `walk::hands::Stored`, `walk::pace::Batch`, `walk::run::{Job,Deed,Until}`, `walk::instability::Sensed`, `walk::recovery::{First,Reaction}`, `walk::routines::Kept`, `file::Notes`, `route::{Leg,Shut}` | — | 12 | **missing** | types that survive only as `drop_in_place` glamour; several are the payloads of the `missing` functions above |

## `walk/routines.rs` and the 29 routines

| symbol | n | status | evidence |
|---|---|---|---|
| `routines::solver_for` | 1 | partial | room lookup + the two jump tables (`0x1024207c0` byte, `0x1024207d0` u16) + `DayPass::new`/`SwordGorge::new`; dispatch is on the `kind` word |
| `routines::holds`, `routines::Seen::answered` | 2 | implemented | line scans |
| `routines::Solver::next`, `Solver::keep` | 27 + 2 | partial | trait shape recovered; `keep` is defaulted |
| 29 `routines::<name>::*::next` (impls) | 29 | partial | 15 have their command sequences verbatim; 14 answer `Next::Wait` with an `// UNKNOWN:` note |
| `routines::cutter::{Cutter::{asked,board,hands,known_containers,listed,looked,open_next,resume,searched,travel,wait}, names_the_walker, one, tickets_in}` | 14 | partial | command literals recovered for the visible half |
| `routines::casting::{active,affords,knows,Casting::turn}` | 4 | **missing → none** | no `casting` module here; `knows::*` covers part of the same ground |
| `routines::shopping::{BankRun::{arrived,next}, Search::next, held}` | 4 | **missing → none** | no `shopping` module here |
| `routines::day_pass::{Pass::serves,flag_for,passes_in,read_pass,sack_in,DayPass::{new,ask,bank,end,arrive,look_at}}` | 11 | partial | town list recovered |
| `routines::crown_door::{CrownDoor::{cast,finish,push}}`, `seeking::Seeking::landed`, `flight_of_steps::climb_for`, `rune_staircase::{RuneStaircase::work,letter_after}`, `colour_barrier::walk_in`, `confluence::Learned::dir_to`, `altar_levers::row_of`, `familiar_doors::FamiliarDoors::{work,ending}`, `minotaur_maze::MinotaurMaze::choosing`, `patrol::Patrol::open`, `rolaren_gate::RolarenGate::leave`, `sword_gorge::SwordGorge::{new,put,stow,recover_or_rise}`, `trinket::{Trinket::{note,take_out},at_hand,named}` | 20 | partial | per-file evidence |
| `routines::Routine`/`Landmark`, `Solver` drop glue | 5 | implemented (serde) | recovered field order |

## What changed this pass

1. **`walk::instability::half_tag` is now the recovered predicate** — the two
   `&str` tables were read out of `__DATA_CONST`, the format string decoded from
   the piece run at `0x103e6c7d2`, and the equality test reproduced. The previous
   version returned an invented string from a table of empty strings. New tests:
   `half_tag_tables_are_the_recovered_words`,
   `half_tag_matches_the_formatted_phrase_exactly` (15 assertions).
2. **`walk::instability::asked` now recognises all four questions and returns the
   recovered tags** (`Asked::{Confluence,ConfluenceHot,ConfluenceCold,Instability}`
   with tags `0/1/2/3`), instead of mapping `"instability"` to `Element::Earth`.
   The `"[Elemental Confluence]"` fall-through is documented, not invented. New
   tests: `asked_answers_the_recovered_needles`,
   `asked_tags_and_word_lengths_are_the_recovered_ones`.
3. **Function extents were recomputed from the local symbol table.** Four claims
   in this document were wrong and are corrected above: `Trip::path_from` is 488 B
   (not 24 504) and delegates the search to `cena_map::route::Routes::path_to`;
   `route::not_known` is 160 B and is a `Cond`-tree predicate, not a sentence;
   `route::itinerary` is 1 256 B and answers `Vec<Leg>`; `route::places` is
   2 912 B. The `route::table` "saved steps" description was replaced with the
   report-renderer reading.
4. **The crate's async half was missing from these notes and from the code.** 33
   `walk::drive::Driver<F,W>` symbols, 6 `sail::{Runner,sail,guide,voyage}`
   symbols, `Trip::{plan,replan,step_from,give_up,aside_said,exit_between}`,
   `instability::{Sensed,instability_table}`, `routines::{casting,shopping}`,
   `swim::sensed` and a dozen payload types are now listed as **missing** with
   their call sets. `tokio`/`tokio_util` are used by the original and are **not**
   dependencies here — the earlier "no `tokio` call site in this crate" claim was
   wrong.
5. **`cargo test -p hydra_travel --offline` is 167** (164 before; three new
   tests, none removed).

## What remains, honestly

* the whole `walk::drive` async driver tree (33 symbols, ~120 KB of text);
* `sail::{Runner,sail,guide,voyage}` and the four `Voyage` methods' predicates;
* `Trip::{plan,replan,step_from,give_up,aside_said,exit_between}`;
* `route`'s `Routes`/`Leg`/`Cond` halves — they live in `cena_map`, which is not
  in the workspace pins, so `table`/`places`/`itinerary`/`why_shut` stay seams;
* `routines::{casting,shopping}`, `swim::sensed`, `instability::instability_table`;
* every partial predicate whose dependency is out of crate (`Cond`, `Cost`,
  `Routes`, `Containers::ready`).

Nothing above was replaced with a plausible guess: where the binary does not pin a
value, the site says `// UNKNOWN:` and this table says **partial**.
