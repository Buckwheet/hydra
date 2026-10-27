# `hydra_travel` — recovery notes

Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(demangled globals in `/Users/cpmbpro/Code/hydra-inspect/symbols_demangled.txt`).
The crate had 75 `todo!()` stubs over 40 files; nothing is left
(`grep -R "todo!()\|unimplemented!()" crates/hydra_travel/src` is empty) and

* `cargo check -p hydra_travel --offline` — clean, no warnings
* `cargo test -p hydra_travel --offline` — **164 tests pass**

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
