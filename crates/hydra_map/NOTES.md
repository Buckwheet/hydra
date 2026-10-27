# `hydra_map` — recovery notes

Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled globals in `symbols_demangled.txt`). The crate had 46
`todo!()` stubs over 12 files; this build has none —
`grep -Rn 'todo!\|unimplemented!' crates/hydra_map/src` is empty — and

* `cargo check -p hydra_map --offline` — clean, no warnings
* `cargo test -p hydra_map --offline` — **67 tests pass**

**Phase 2 audit pass** re-inventoried the crate from both symbol sources, found a
whole module in the binary that this build does not have (`lay::{open_place,
places,marks,convert,lay_out}`), and corrected three behaviours. See
[Phase 2 audit: coverage table](#phase-2-audit-coverage-table).
* `cargo check --workspace --offline` — clean (no other crate was touched)

Phase 1 reconnected `serde`, `serde_json` and `tokio` (see *Phase 1
reconnection*). `cena_map`/`cena_map_layout` are not in the workspace pins and
stay as seams.

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow, literals, offsets and data tables read out of the instruction stream; asserted by a test. |
| **partly** | the entry contract, literals and helper calls are recovered, but one step (a predicate, a struct's field list, or a call into an out-of-crate dependency) could not be; the body implements the evidenced part and says so at the site. |
| **UNKNOWN** | the binary does not pin this down. The safest behaviour was chosen and the choice is written where it is used. |

Every uncertain decision also has a `// UNKNOWN:` or "Partly recovered" comment at
the point of use; this document collects them.

---

## Tooling

Read-only, all scratch under `$TMPDIR` (nothing written into the repo):

* `.work/recon.py` — the shared driver (`syms`, `disas`, `bytes`, `str`, `findstr`).
* **A whole-symbol-table demangler.** `rustfilt` over `nm -an` gives 120 043
  symbols (72 002 text), which is where the *private* `hydra_map` functions come
  from: `atlas::{ask, laid, waiting_for, work}`, `next_door::{cells, beside}`,
  `overrides::{written, filed_in}`, `room::found::{{closure}}`, the serde
  `Serialize`/`Deserialize` impls and every monomorphization of
  `SerializeMap::serialize_entry` that the scene writer calls.
* **A per-function annotator** over `objdump -d --no-show-raw-insn` that tracks
  `adrp`/`add`/`ldr` so every reference prints the string or scalar it addresses,
  and resolves `bl` to its symbol. Function boundaries come from the local symbol
  table, not just the globals: this is what makes `Atlas::ask` (1 484 B) and
  `Atlas::work` (8 084 B) separable instead of one 8 572-byte blob.
* **A whole-`__text` call/literal cross-reference scanner** (`target → referring
  functions`), used to find the callers of `hydra_map::{load, decode, shared}` and
  the owners of the literal pools.
* **A format-piece decoder.** rustc stores `format!` arguments as a
  `<len><text>` run with a single `0xC0` separator per `{}` and a `0x00`
  terminator (the sibling crates' `NOTES.md` describe the same scheme). Every
  message quoted below came out of that decoder, e.g.
  `0x103e6cdc1` → `"the map cannot be used: {}"`.
* **A `&str`/table reader** for `__DATA`/`__const`, which is how the ocean's two
  `i32` heading-delta tables and the tab-separated data literals were read.

---

## ABI facts this crate re-confirms

* `Vec`/`String` are `{cap @0, ptr @8, len @16}`, and `Option<String>`'s `None`
  is the word `isize::MIN` in the capacity slot. Confirmed three ways:
  `decode`'s sret writes (`0x1015bd9d4`), `Atlas::ask`'s push
  (`stp x21, x24, [x8]; str x21, [x8, #0x10]` with `x21 = len`), and
  `Overrides::load`'s empty value.
* `HashMap`/`HashSet` are `{bucket_mask @0, ctrl @8, growth_left @0x10, items
  @0x18, RandomState @0x20}`; inside a bucket **the value comes before the key**,
  which is why `HashMap<u32, String>`'s lookup ends with
  `ldp x0, x1, [bucket - 0x10]` (the string's `ptr`/`len`). A `HashMap<K, ()>`
  (`HashSet`) has a 24-byte stride, `HashMap<u32, String>` 32, `HashMap<&str, V>`
  and `HashMap<String, ()>` 24, `HashMap<(i32, i32), usize>` 16,
  `HashMap<String, Arc<_>>` 32.
* A `RwLock`/`Mutex` is `{state @0, poison @8}` and its data follows at `+0x10`;
  a `std::sync::Mutex` on macOS first resolves its `pthread_mutex_t` through a
  `OnceBox`, which is the `ldapr`/`cbz`/`OnceBox::initialize` dance in every
  locked function here.
* An `Arc<T>` clone is `ldadd x8, #1, [ptr]` with an overflow check
  (`tbz x8, #63` → `brk`); that is how `Atlas::laid` was identified as returning
  `Option<Arc<_>>`.
* `Option<fieldless enum>` uses the variant count as the `None` tag: `Heading`
  has eight variants and `Heading::named` returns `8` for `None`
  (`mov w21, #8`).

---

## Function table

Addresses are unslid VAs. "local" marks a symbol that only exists in the local
symbol table (it was not in the stub list, so it is reconstructed here because the
stubs call it).

### `lib.rs`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `decode` | `0x1015bd850` | **partly** | `fn decode(&[u8]) -> Result<(String, Arc<Map>), String>`. Calls `cena_map::binary::decode::decode` (`0x1015bd87c`); a failure becomes `format!("the map cannot be used: {}", e)` (`0x103e6cdc1`, 24-byte piece). On success it allocates one 160-byte `ArcInner<Map>` (`rust_alloc(160, 8)` at `0x1015bd938`, with `1` written into both strong and weak counts) and computes FNV-1a 64 over the raw bytes (`0xcbf29ce484222325`, `× 0x100000001b3`) which it formats into a `String`. The `cena_map` decoder is out of this crate — see **departures**. |
| `load` | `0x1015bd6d0` | **full** (one UNKNOWN) | `fn load() -> Result<(String, Arc<Map>), String>`. `std::env::var_os("HYDRA_MAP")` (`0x10245bffa`, `mov w1, #9`); on `None` it decodes the embedded image at `0x10245c10f` (16-byte `(ptr, len)` pair at `0x10443eb78`); otherwise `std::fs::read` (`0x1015bd738`) and `decode`. A read error becomes `format!("{} cannot be read: {}", path.display(), e)` (`0x103e25e7b`). UNKNOWN: the embedded image (see below). |
| `shared` | `0x1015bda48` | **full** | `fn shared() -> &'static Result<…>`. `static SHARED: OnceLock<Result<(String, Arc<Map>), String>>` at `0x104586ea8` with its state word at `+0x20` (`0x104586ec8`) — i.e. a 32-byte payload — and the accessor is `SHARED.get_or_init(load)`. The initialiser is the `Once::call_once_force` closure at `0x1015ac58c`, which calls `hydra_map::load` and copies 32 bytes into the lock. |

### `scene.rs` — the scene file (serde)

All five `Serialize` impls and the one surviving `Deserialize` field visitor were
read entry by entry. Field *names* are the literals passed to
`SerializeMap::serialize_entry`; field *types* are the monomorphizations it calls:

| monomorphization | what it serialises | used by |
|---|---|---|
| `0x1015a1c14` | a string (`format_escaped_str`) | `area`, `place`, `title`, `paths`, `text` |
| `0x1015a14d0` | `u32` (`itoa::Unsigned for u32`) | `street`, `inside`, `a`, `b`, `id` |
| `0x1015a2ef0` | a `(i32, i32)` tuple, written `[x,y]` with `cneg` for negatives | `min`, `max`, `cell` |
| `0x1015a1b00` | a `(f32, f32)` tuple (`ldp s0, s1`) | `at` |
| `0x10159fc0c` | `bool` (writes `true`/`false`) | `named` |
| `0x1015a2288` | `u64` **or** `null` | `building` |
| `0x1015a3418` | a string **or** `null` | `edge.label` |
| `0x1015a06b0` | `Vec<(f32, f32)>` | `edge.path` |
| `0x1015a2bd8` / `0x1015a3574` / `0x10159f6e0` / `0x1015a2764` | `Vec<SceneRoom>` (stride 0x68) / `Vec<SceneEdge>` / `Vec<SceneDoor>` / `Vec<SceneLabel>` | |
| `0x1015a3798` | `Vec<String>` | `buildings`, door `marks` |
| `0x10159fad0` | `EdgeKind` as a unit-variant string | `edge.kind` |

| symbol | addr | status | notes |
|---|---|---|---|
| `MapScene::serialize` | `0x1015ae688` | **full** | eight entries: `area`, `rooms`, `edges`, `doors`, `buildings`, `labels`, `min`, `max` — in that order, at offsets `0x00, 0x18, 0x30, 0x48, 0x60, 0x78, 0x90, 0x98`. |
| `SceneDoor::serialize` | `0x1015aea00` | **full** | `street` `inside` `at` `place` `named` `marks` (offsets `0x38, 0x3c, 0x30, 0x00, 0x40, 0x18`). |
| `SceneEdge::serialize` | `0x1015aeb54` | **full** | `a` `b` `kind` `path` `label` `building` (`0x40, 0x44, 0x48, 0x10, 0x28, 0x00`). |
| `SceneRoom::serialize` | `0x1015aeca8` | **full** | `id` `cell` `title` `building` `paths` (`0x60, 0x58, 0x10, 0x00, 0x28`). |
| `SceneLabel::serialize` | `0x1015aedfc` | **full** | `text` `at` `building` (`0x10, 0x28, 0x00`). |
| `MapScene` `__FieldVisitor::visit_str` | `0x1015ac318` | **full** | length-dispatch confirms the same eight tags and an `ignore` arm (tag 8). The `expecting` strings are `"struct MapScene"`, `"struct SceneDoor"`, `"struct SceneEdge"`, `"struct SceneRoom"`, `"struct SceneLabel"` at `0x10245be60`..`0x10245be80`. |
| `EdgeKind::deserialize` | `0x1015ae814` | **full** (names) | the three variant names `Directional`, `Connector`, `Stub` (`0x102422e63`, `…e6e`, `…e77`). |
| `MapScene::room` | `0x1015aef34` | **full** | `fn room(&self, u32) -> Option<&SceneRoom>`. A `OnceLock<ById>` at `+0xa0` (state word `+0xd0`) is initialised on first use with `&self` as the closure's capture; `ById` is a `HashMap<u32, usize>` (16-byte stride); the hit is bounds-checked against `rooms.len` before `&rooms[index]` (stride 0x68). |

### `overrides.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `Overrides::load` | `0x1015bea40` | **full** | `fn load(&Path) -> Overrides`: `Path::join("map-overrides.json")` (`0x10245c017`, 18 bytes), `fs::read_to_string`, `serde_json::from_str`; *any* failure gives the empty document (two zero words + zero length at `0x1015beb24`). 24 bytes returned = one `Vec`. |
| `Overrides::save` | `0x1015bebc4` | **full** | one `serialize_entry("areas", self.areas)` (`0x10245c012`, 5 bytes) into a `{"…"}` object, then `written`. |
| `Overrides::export` | `0x1015bf05c` | **full shape** | writes `map-overrides-export.json` (`0x10245c029`, 25 bytes) as a flat sequence of `Exported` (`Serializer::collect_seq` at `0x1015bf130`). |
| `Overrides::applied` | `0x1015bf4d8` | **partly** | clones the map and calls `cena_map::map::Map::from_rooms`, then rewrites matched rooms. The matching keys are inferred (id, else uid) — the `filed_in` helper rewrites one room. |
| `Exported::serialize` | `0x1015bda88` | **full** | four keys in order: `id` (`0x10245c003`), `uid` (`0x10245c005`), `title` (`0x10245c009`), `area` (`0x10245c00e`); types `u32`, `u64`, `String`, `String`. |
| `written` (local) | `0x1015be258` | **full** | `process::id` → `format!` → `Path::with_extension` → `OpenOptions` → `write_all` → `File::sync_all` → `fs::rename`, with `fs::remove_file` on the failure path. The temporary name is `*.{pid}.tmp`. |
| `filed_in` (local) | `0x1015be424` | **partly** | clones the room's `Vec<String>` fields, `Vec::retain`s the old name out and formats the new one in. Which vec is which is inferred from the `Vec<String>` drop shims. |
| `pocket` | `0x1015bdba0` | **partly** | loops over `cena_map::Map::room`, folds into a `BTreeMap` and extends a `VecDeque`; the grouping is recovered, the argument list is not (the stub had none and the body never uses a `self`). |

### `atlas.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `Atlas::area_names` | `0x1015971d4` | **full** | `fn area_names(&self) -> Vec<String>`: collects the `HashSet<String>` at `+0x30` (`SpecFromIterNested<Vec<String>>` at `0x101597220`) and `slice::sort`s it (`insertion_sort_shift_left` under 21 elements at `0x101597264`, `driftsort_main` above). |
| `Atlas::area_of` | `0x10159b034` | **full** | `fn area_of(&self, u32) -> Option<&str>`: the `HashMap<u32, String>` at `+0x00`; the empty-map early return is `0x10159b04c`; the value is read as `ldp x0, x1, [bucket - 0x10]`. |
| `Atlas::close` | `0x101599ba8` | **full** | locks the queue, sets the closed byte at `+0x290`, unlocks, `notify_all` on `+0x2a8` and `+0x298`. |
| `Atlas::held` | `0x101597a2c` | **full** | twelve bytes: a plain load of `hydra_map::atlas::LIVE` at `0x10458d760`. Modelled as `static LIVE: AtomicUsize`, incremented while a worker thread runs. |
| `Atlas::progress` | `0x10159b10c` | **full** | read-locks the place map at `+0x60`, walks its keys and counts those the area-name set contains (`HashMap::contains_key` at `0x10159b1d0`), then returns `(count, area_name_count)` where the second is `[self + 0x48]`. |
| `Atlas::scene_of` | `0x10159b290` | **partly** | `area_of(room)` → `laid(area)` → `MapScene::room(place + 0x10, room)` (`0x10159b388`, so the scene sits 16 bytes into the `Arc` payload); a miss consults (and fills) the `RwLock<HashMap<u32, String>>` cache at `+0x170`, and asks for the area when the atlas has no sheet. The return is modelled as `Option<Arc<Sheet>>` so the caller cannot dangle. |
| `Atlas::sheet` | `0x101599c78` | **full** | `laid(name)`, or `ask(name)` and `None`. |
| `Atlas::start` | `0x101599cc4` | **partly** | spawns the layout worker (`spawn_unchecked<Atlas::start::{{closure}}, ()>` survives in the symbol table) and turns a spawn failure into `format!("[map] cannot start a layout worker: {}", e)` (`0x103e6c98a`, a 36-byte piece). The worker's *source* of scenes is a seam. |
| `Atlas::ask` (local) | `0x101597460` | **full** | queue semantics, in order: known→no-op, closed→no-op, remove from `marks`, in `asked`→no-op, already queued→no-op, else push + `notify_one`. |
| `Atlas::laid` (local) | `0x101597a38` | **full** | read-locks the place map at `+0x60`, looks up by `&str`, `Arc::clone`s the value. |
| `Atlas::waiting_for` (local) | `0x10159728c` | **full** | `2` when the name is in the done set (`0x101597410`), else `ask` and `1` (`0x101597428`). |
| `Atlas::work` (local) | `0x101597c14` | **partly** | the loop: lock, stop when closed, pop from the queue (then from `marks`), clone into `asked`, unlock, resolve, store, notify. The name is split on `'@'` (`movi.2s v0, #0x40`) and searched in a sorted table of 24-byte entries; the resolution *itself* is `cena_map_layout`, outside this crate. |

`Atlas`'s field map (all from the offsets the recovered functions touch):
`+0x00` `HashMap<u32, String>`, `+0x30` `HashSet<String>`, `+0x60`
`RwLock<HashMap<String, Arc<Sheet>>>`, `+0x170` `RwLock<HashMap<u32, String>>`,
`+0x1e0` `Mutex<Queue>`, `+0x290` `bool`, `+0x298`/`+0x2a8` `Condvar`.
`Queue` = two `VecDeque<String>` (`+0x00`, `+0x20`) and two `HashSet<String>`
(`+0x40`, `+0x70`).

### `next_door.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `Atlas::next_door` | `0x10159b628` | **partly** | 7 176 bytes. Recovered: the place name is split on `'@'` (`0x10159b67c`), the sheet comes from `Atlas::laid`, the geometry from `cells`/`beside`, and unknown places are `ask`ed (`0x10159c1d0`). The two extra arguments (stored beside the name at `[sp, #0x90]`/`[x29, #-0xe0]`) and the `NextDoor` field list are not recoverable — the struct is only ever built and dropped here (`InPlaceDstDataSrcBufDrop<Placed, NextDoor>`). |
| `next_door::cells` (local) | `0x1015b781c` | **partly** | reads a room vector at `[arg + 0x20]` and returns a 0x30-byte record; modelled as "the rooms that leave the sheet in this heading". |
| `next_door::beside` (local) | `0x1015b7934` | **partly** | the companion predicate: a room with something beside it is not a way out. |

### `ocean.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `ocean` | `0x1015a6c10` | **full** accessor | `static OCEAN: OnceLock<Ocean>` at `0x104586e18` (state word `+0x40`); returns `&'static Ocean`. The initialiser is **UNKNOWN** (it parsed the embedded tables). |
| `Heading::named` | `0x1015a6c50` | **full** | `trim()` + `to_ascii_lowercase()` (the vectorised `add 0xbf / cmhi 0x1a / or 0x20` loop), then a match on the byte length: 1 = `n e s w`, 2 = `ne se sw nw`, 4 = `east west`, 5 = `north south`, 9 = the four long diagonals; everything else (including every 3-byte input) is `None` (tag 8). |
| `Ocean::at` | `0x1015a4c90` | **full** | `fn at(&self, (i32, i32)) -> Option<&OceanRoom>`: `HashMap<(i32,i32), usize>` at `+0x78`, then `&rooms[index]` (stride 0xc8 = 200). |
| `Ocean::room` | `0x1015a6570` | **full** | the same over `HashMap<u64, usize>` at `+0x48`. |
| `Ocean::port_at` | `0x1015a6a10` | **full** | linear walk of `self.ports` (stride 0x48) comparing the `u64` at `+0x40`. |
| `Ocean::port_named` | `0x1015a4638` | **full** | three passes over `self.ports` on the lower-cased name: equal, then `starts_with`, then any word split on `' '` and `'\''` (`0x27`/`0x20` comparisons) equal. Empty input never matches. |
| `Ocean::port_of_pier` | `0x1015a4a74` | **full** | `HashMap<u64, usize>` at `+0xa8` → `&ports[index]`. |
| `Ocean::port_of_pier_titled` | `0x1015a4b64` | **full** | `HashMap<String, usize>` at `+0xd8` (32-byte stride) → `&ports[index]`. |
| `Ocean::piers_of` | `0x1015a6a44` | **partly** | returns the `+0xa8` map's iterator with the argument captured, i.e. pier→port pairs filtered by the port. Modelled as `impl Iterator<Item = u64>` over pier uids; the original's `Item` is not readable off the opaque iterator. |
| `Ocean::piers_titled` | `0x1015a4970` | **partly** | `self.piers.iter()` (32-byte elements at `+0x30`) filtered by the needle, mapped to the room id, collected into a `Vec<u64>` (`0x1015a49a4`) and `sort_unstable`ed (`ipnsort` at `0x1015a4a48`). The filter's comparison is not in the dump; a case-insensitive substring test is used. |
| `Ocean::bound` | `0x1015a6660` | **full** glyphs, INFERRED names | `port_named` first (→ `{name: port.name, number: port.number, named: true}`), else the trimmed text is parsed as an `i64` with an optional `u`/`U` prefix and sign (`0x1015a6aac`: the `'u'` test at `0x1015a66c8`, `'+'`/`'-'` at `0x1015a6700`) and `room(n)` looked up (→ `{name: format!("{}", room.uid), number: n, named: false}`); empty or sign-only input is the `isize::MIN` niche. |
| `Ocean::runs_true` | `0x1015a6a78` | **full** | `fn runs_true(&self, &OceanRoom, Heading) -> bool`: (1) `room.neighbours[h]` is `Some` (tag `1` at `room + h*16`), (2) that room sits at `room.cell + h.delta()` (`ldp w12, w14, [next, #0xb8]`), (3) that room's neighbour in the *opposite* heading (`eor #4`, `TABLE[h ^ 4]` at `0x10207d628`) is `Some(room.uid)` (`room + 0xb0`). |
| `Ocean::read` | `0x1015a4d90` | **partly** | `lines()` + tab split + an unrolled decimal reader; the four `HashMap`s it builds are the ones the getters use. The column names/order come from the two data literals (`0x102422e7b`, `0x102422e31`): 13 columns `uid ocean sea col row n ne e se s sw w nw` for a sea cell, 3 columns `uid sea pier-title` for a pier. **UNKNOWN:** how one text is told from the other — this build dispatches on the field count. |

`Ocean`'s field map: `+0x00` `Vec<OceanRoom>`, `+0x18` `Vec<Port>`, `+0x30`
`Vec<Pier>`, `+0x48`/`+0x78`/`+0xa8`/`+0xd8` the four maps
(`u64→usize`, `(i32,i32)→usize`, `u64→usize`, `String→usize`) with their
`RandomState`s at `+0x68`/`+0x98`/`+0xc8`/`+0xf8`.

### `course.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `Ocean::course_holding` | `0x1015a6fc8` | **partly** | `fn course_holding(&self, from: u64, held: Option<Heading>, to: u64) -> Option<Course>`. Recovered: both ends are looked up in the `+0x48` map (`0x1015a700c`, `0x1015a70b4`), the third argument is masked to a byte (`and x20, x23, #0xff`, `0x1015a7344`) and used as the second index of a nine-slot-per-room `i32` distance table (`len * 36`, `0x1015a7350`), seeded with `dist[from][held] = 0`; the frontier is a 24-byte-record binary heap; the answer is a 48-byte `Course { runs: Vec<(u32, Heading)>, rooms: Vec<u64> }` rebuilt from the predecessor chain with an RLE merge (`0x1015a78b8`). **UNKNOWN:** the cost metric — `course` is 16 936 bytes of inlined search and its comparator is not in the dump. This build minimises `(moves, turns)` and requires a held heading to be used for the first move (otherwise the ninth state is unobservable). |
| `Ocean::course` | `0x1015a7a90` | **partly** | the same search with `held = None`. |

### `follow.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `ways_out_of` | `0x1015b28cc` | **partly** | `fn ways_out_of(&Map, RoomId) -> Vec<WayOut>`: `Map::room` then the exit vector at `+0xc8`/`+0xd0` (stride 0xb8). Each exit's word is lower-cased (the vectorised `to_ascii_lowercase` at `0x1015b29e8`) and compared against the compass words (`"east"`/`"west"` folded into one immediate by the `| 0x412` trick at `0x1015b2940`; `"north"`/`"south"`/`"northwest"`/`"up"` at `0x1015b2ca4`..`0x1015b2cd4`), but the entry is *named after the place it leads to*: `Map::room(exit.to)` (`0x1015b2d60`), the first area-path element trimmed of `[`/`]` (`0x1015b2d80`), or `format!("room {}", to)` when the map cannot name it (`0x103e23ae3`). A `BTreeMap` de-duplicates by that name and orders by it; an unknown room returns the empty `Vec` niche. UNKNOWN: what the compass-word comparisons select. |
| `room_of` | `0x1015b3304` | **partly** | four arguments `(map, state, hint, generation)`; builds a 0x60-byte record from the room number (parsed with the shared decimal reader), `title_from_subtitle`, the `"room exits"` component (a 10-byte name at `0x10245becc`) and two styled strings, then calls `cena_map::Map::locate`. The hint/`generation` interplay is recovered from `Follower::view`'s `csinc` chain. |
| `Follower::view` | `0x1015b3690` | **partly** | `Follower`'s cache is `{discriminant @0x78, generation @0x7c, room @0x80}`; the body re-places the walker when the counter (`state + 0x1340`) moved, then calls `scene_of`, `progress`, `Navigator::fix`, `Heading::named` and `next_door` and packages a `MinimapView`. The drawing half is a UI seam; this build returns the placement, the ways out and the area. |
| `Follower::ways_out` | `0x1015b3fb4` | **full** | `cmp w9, #1` on the discriminant; a placed walker gives `ways_out_of(map, room)`, an unplaced one the empty `Vec` niche. |

### `room.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `found` | `0x1015b0bbc` | **partly** (numeric half **fixed**) | `fn found(&Map, &str, bool) -> Vec<Found>`: trims, strips `u`/`U`, parses an `i64`, lower-cases the needle, visits every room (384-byte stride) and sorts the hits (insertion sort under 21). **Corrected in the Phase 2 audit:** the numeric half is *two* lookups — `Map::room` for a bare number and `Map::ids_for_uid` for either shape, de-duplicated by id — so a number names rooms by uid as well as by map id. The word half's keep-predicate and the third argument are still not recoverable (the closure captures nothing, so the filter was inlined into the collection loop); modelled as "every word of the needle appears in the title/areas/note". |
| `room::found::{{closure}}` (local) | `0x1015b1090` | **full** | builds one `Found`: the first element of a `Vec<String>` trimmed of `['[', ']']`, the room title (`+0x160`) cloned, and the `u32` at `+0x178`. |
| `described` | `0x1015b11cc` | **partly** | `fn described(&Map, RoomId, bool) -> Option<Vec<String>>`. `Map::room` or the `isize::MIN` niche. `numbered` → one line `format!("#{} {}", id, title)` (`0x103e2b401`). Otherwise `format!("#{} {}:", id, title)` (`0x103e6cba0`), `format!("{} {}", a, b)` (`0x103e2252a`), then the last of the area `Vec<String>` (`+0x50`/`+0x58`) and the last of the spec `Vec<String>` (`+0x68`/`+0x70`). The `" (u{})"` (`0x103e6cba7`) and `" ({})"` (`0x103e28dbe`) fragments build `a`/`b`; which is which is inferred from the stack-slot order. |

### `sailing.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `Navigator::fix` | `0x1015b3fe0` | **partly** (state byte **fixed**) | needs a room name (`state.room + 0xa70` must not be the `isize::MIN` niche, `0x1015b4010`), parses the room number with the shared reader, consults `hydra_model::state::ship::aboard` (`0x1015b41b4`), and resolves the port through `port_of_pier_titled` (`0x1015b452c`, `0x1015b4548`) / `port_of_pier`. The incoming state byte at `+0x28` is compared with `4` (unfixed); the three successful arms write `1`, `2`, `3`, all through the same tail, so **which arm writes which is still UNKNOWN** — this build writes one value ([`sailing::FIXED`]) and records the one-bit aboard flag the original keeps beside it (`strb w8, [x21, #0x1]` at `0x1015b4cf4`) in `Navigator::aboard`. |

### `shown.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `show_route` | `0x1015af084` | **full shape** | `static BOARD: LazyLock<Mutex<HashMap<String, Arc<ShownRoute>>>>` at `0x104586e18` (state word `+0x40`); locks it and inserts. |
| `shown_route` | `0x1015af774` | **full shape** | locks `BOARD` and returns the `Arc`. |
| `show_course` | `0x1015af418` | **full shape** | `static COURSES` at `0x104586e60` (state word `+0x40`). |
| `shown_course` | `0x1015afa14` | **full** (**fixed**: was *partly*) | 672 bytes — exactly `shown_route`'s size — with the same call set one for one (`Mutex::lock`/`unlock`, `to_lowercase`, `hash_one`, `OnceLock::initialize`, `Arc` drop). No insert, no ocean call, **no build-on-miss**. The key is the lower-cased name. |
| `ShownRoute::from` | `0x1015af040` | **full** | `fn from(&self, u32) -> &[u32]`: walks the vector backwards (`lsl x10, x9, #2`, `ldur w11, [ptr + x10 - 4]`) and returns the tail starting at the **last** match — `&rooms[len - count..]`; no match returns the whole slice. |
| `ShownCourse::from` | `0x1015af3d4` | **full** | the same over `Vec<u64>` (`lsl x10, x9, #3`). |

---

## Recovered constants and tables

| what | value | evidence |
|---|---|---|
| env var | `HYDRA_MAP` | `0x10245bffa`, `mov w1, #9` |
| overrides file | `map-overrides.json` (18) | `0x10245c017` |
| overrides export | `map-overrides-export.json` (25) | `0x10245c029` |
| map magic | `HYDRAMAP` | `0x10245c10f`; the 16-byte `(ptr, len)` pair at `0x10443eb78` has `0x1524665` as the length |
| FNV-1a 64 | basis `0xcbf29ce484222325`, prime `0x100000001b3` | `0x1015bd94c`..`0x1015bd990` |
| scene keys | `area rooms edges doors buildings labels min max` | `0x102422e7b`, `0x10245bdf7` |
| overrides keys | `areas id uid title area` | `0x10245c012`, `0x10245c003`..`0x10245c00e` |
| layout-worker error | `[map] cannot start a layout worker: {}` | `0x103e6c98a` (a 36-byte piece) |
| read error | `{} cannot be read: {}` | `0x103e25e7b` |
| decode error | `the map cannot be used: {}` | `0x103e6cdc1` |
| follower messages | `Where you are is not known yet.` (32 B at `0x10245bf22`), `This place is not on the map's areas.` (`0x10245becc`), `This place's map could not be laid out.` (`0x10245bed6`) | literal pool |
| heading names | `north northeast east southeast south southwest west northwest` order | `Heading::named`'s compare chain |
| heading columns | `[0,1,1,1,0,-1,-1,-1]` (col) / `[-1,-1,0,1,1,1,0,-1]` (row) | 2 × 8 `i32` at `0x102422c28` / `0x102422c48` |
| heading↔neighbour index | identity (the `TABLE[h ^ 4]` at `0x10207d628` is `[0..7, 0x15…]`) | `runs_true`, `course_holding` |
| ocean columns | `uid ocean sea col row n ne e se s sw w nw` | `0x102422e7b` + the row data at `0x102422e90` |
| pier columns | `uid sea pier-title` | `0x10245b001`, `0x10245b120` |
| `OceanRoom` offsets | neighbours `+0x00` (8 × `Option<u64>`, 16 B each), uid `+0xb0`, col `+0xb8`, row `+0xbc`; size 200 | `runs_true`, `bound`, `at` |
| `Port` offsets | name `String @+0x10`, sea `String @+0x28`, number `u64 @+0x40`; size 72 | `port_named`, `bound`, `port_at` |
| `Ocean` map layout | 4 maps at `+0x48`, `+0x78`, `+0xa8`, `+0xd8` | the four getters |
| `Follower` cache | `+0x78` discriminant, `+0x7c` counter, `+0x80` room | `view`, `ways_out` |
| `SceneRoom` stride | `0x68` | `spec_extract` in `MapScene`'s `rooms` entry |
| `Room` stride (`cena_map`) | `0x180` (384) | `room::found` |
| `Exit` stride (`cena_map`) | `0xb8` (184) | `ways_out_of` |

---

## Departures that remain

Phase 1 struck the three that could be reconnected (`serde`, `serde_json`,
`tokio` — see the next section). What is left:

| original dependency | recovered use | what this build does |
|---|---|---|
| `cena_map` (external crate, **not in the workspace pins**) | `Map::room`, `Map::from_rooms`, `Map::locate`, `locate::title_from_subtitle`, `Room` (384 B), `Exit` (184 B), `exit::Cost::price`, `cond::Cond` | [`cena`]: the same accessors over a room model with the recovered field *shapes* (`id`, `title`, `paths`, `areas`, `specs`, `note`, `exits`) and the recovered `Vec<String>`/`Option<String>` kinds. |
| `cena_map::binary::decode` (**not in the pins**) | `decode`'s first call | [`cena::decode`] accepts the recovered `HYDRAMAP` magic and then a record layout of its own ([`cena::encode`] writes it for the tests). **This is the largest remaining departure in the crate**: the real image is a 22 MB blob whose record layout lives outside `hydra_map`. |
| `cena_map_layout` (**not in the pins**) | `atlas::work`'s per-name resolution, `next_door`, `lay::{open_place, marks, places, convert, lay_out}` | the queue/worker plumbing is reproduced; the *layout* is supplied by the caller as a closure ([`atlas::Atlas::start`]/[`Atlas::work`]). The `lay` helpers are not reproduced at all — they belong to that crate. |
| `anyhow`-style errors | `String` errors from `decode`/`load` | `Result<_, String>` with the recovered `Display` wording preserved byte for word. `anyhow` **is** pinned (`1.0.104`), but the recovered signatures are `Result<_, String>` (`decode`'s sret is one `Vec`-shaped 24-byte `String`), so switching would break the sibling-visible API for no gain. |
| `egui`/`wgpu` drawing | `scene::MinimapView`, `Follower::view`'s `View` | the model/state is kept, the draw calls are not: `View` carries the placement, the ways out and the area. Not this crate's dependencies here and not in the pins (Phase 4 in `PLAN.md`). |

---

## Phase 1 reconnection — what is real again

Three crates, each the workspace pin declared as `x = { workspace = true }` in
this crate's `Cargo.toml` and built with `cargo --offline`:

| real crate (pin) | replaced | where | notes |
|---|---|---|---|
| `serde` 1.0.229, feature `derive` | the hand-written `to_json`/`from_json` pairs on the five scene structs and `Overrides` | `scene::{MapScene, SceneDoor, SceneEdge, SceneRoom, SceneLabel, EdgeKind}`, `overrides::{Overrides, Area, Exported}` | the derives *are* the crate's five `SerializeMap::serialize_entry` chains and five `__FieldVisitor::expecting` symbols. A derived `Serialize` writes its fields in declaration order, which is the recovered order, so the documents keep their byte-for-byte shape — the exact-text assertions did not change. |
| `serde_json` 1.0.151 | `src/json.rs` (a `Value`, a parser and a `Writer`) | `scene::{to_json, from_json, from_value}`, `overrides::{load, save, export, from_json, to_json}` | `src/json.rs` is **deleted** and `pub mod json` with it: the two documents above were its only users, and no sibling crate names `hydra_map::json`. `to_json`/`from_json` keep their names and meaning; their error type is `serde_json::Error` instead of the local `json::Error`. |
| `tokio` 1.53.1, feature `full` | "nothing is spawned; `shared` is a plain `OnceLock`" | `lib::{decode_task, load_task, shared_task}` | the recovered shape — `hydra_map::load`/`hydra_map::shared` running on tokio's blocking pool — is now reachable, while `load`, `decode` and `shared` stay synchronous for the sibling-facing API. |

Path dependencies: **none**. `cena_map`/`cena_map_layout` are not in the
workspace, so the crate still carries [`cena`] and the two seams above.

### Behaviour that changed as a direct result of the real crates

1. **Error text.** A broken scene or overrides file reports `serde_json`'s
   wording (`invalid type: integer `7`, expected a string at line 1 column 9`,
   `unknown variant `stub`, expected one of `Directional`, `Connector`, `Stub``)
   instead of the hand-written sentences. Nothing on disk changed, and
   `Overrides::load` still swallows every error into the empty document before
   the text can reach a caller (`0x1015beb24`).
2. **A struct may be spelled as a sequence.** serde's derive accepts the tuple
   form of a struct, so `Overrides::from_json("[]")` is now
   `Overrides { areas: [] }` where the hand-written reader refused it. The
   object form is untouched, and `{"areas": …}` with a non-sequence under it is
   still a type error.
3. **Absent keys default.** `#[serde(default)]` on each document (INFERRED:
   the recovered visitor has eight tags plus an `ignore` arm and says nothing
   about missing keys; the previous reader defaulted them). Unknown keys are
   ignored, as before — that part is recovered (`ignore`, tag 8).
4. **Duplicate keys.** `src/json.rs` kept the last one; `serde_json` keeps the
   last one too, so this did not change — but it is now the real crate's
   documented behaviour rather than a coincidence.
5. **`serde_json` brings `impl PartialEq<Value> for u32` into the crate**, which
   made one existing inference in `shown`'s test ambiguous
   (`&[][..]` → `&[0u32; 0][..]`). Library code is unaffected; it is a compile
   nit, not a behaviour change.
6. **A loader that panics on the blocking pool.** `decode_task`/`load_task`
   turn a `JoinError` into `[map] the map loader did not finish: {}`
   ([`CANNOT_FINISH`]) — a Phase 1 addition, not recovered; the original let the
   panic escape.

---

## Significant unknowns and conflicts

1. **The embedded map image.** `load`'s fallback is a `&[u8]` at `0x10245c10f`
   of length `0x1524665` that begins `HYDRAMAP`. Its record layout belongs to
   `cena_map::binary`, so [`DEFAULT_MAP`] is empty here and `load()` without
   `HYDRA_MAP` answers with the recovered error text instead.
2. **`decode`'s fingerprint spelling.** FNV-1a 64 is certain (the constants are
   in the instruction stream); the *format* is inside the piece blob at
   `0x10245c048`, which does not decode with the crate's `<len><text>`/`0xC0`
   scheme (`0xc3 0x20` is not a length). 16 lower-case hex digits is a guess; the
   value is asserted, the spelling is not.
3. **`cena_map`'s room model.** Only four offsets are directly observed
   (`id +0x178`, `title +0x160`, the paths/areas/specs vecs, the optional note,
   the exit list). Field *names* in [`cena::Room`] are ours; `Room::area()` is
   "the innermost of `areas`" by inference from the pier titles
   (`"Sleeping Drake Harbor, Pier One"`).
4. **`Map::locate`'s precedence.** `room_of` builds the record and hands it over;
   the order in which number, name and exits are weighed is inside `cena_map`.
   Number first, then `title, subtitle`, then an id found in the `"room exits"`
   component.
5. **`course`'s cost metric.** See the table above. The *state machine* (nine
   states per room), the RLE output shape and the `(u32, Heading)` run element
   are exact; the comparison is not.
6. **`Overrides`'s element type.** The 24-byte struct means a single `Vec` and
   `save` writes exactly one key; an element grouping rooms under an area name is
   inferred from `filed_in` and `applied`.
7. **`NextDoor`'s field list.** The struct is only ever built and dropped inside
   `next_door`; the fields carried here are the ones the plumbing demonstrably
   has (the `'@'`-split place name, the room and cell, the heading).
8. **`room::found`'s predicate and third argument.** See the table.
9. **`ways_out_of`'s compass-word comparisons.** The exit word is lower-cased
   and compared against `"east"`/`"west"`/`"north"`/`"south"`/`"northwest"`/`"up"`
   but what the result selects (probably "a plain exit is named after the
   neighbouring area") was not reconstructed; the place naming itself is
   recovered and used.
10. **`Navigator::fix`'s state byte.** `1`, `2`, `3` and `4` all appear as writes;
    which case means what is not recoverable, so only `4` (= unfixed) is named.
11. **`Atlas::work`'s name table.** The worker binary-searches a sorted table of
    24-byte entries after splitting the name on `'@'`; the table's contents
    (place names and their geometry) are not in the binary as data this crate can
    reach.
12. **`atlas::LIVE`'s exact type.** `held` is a bare 8-byte load, so `AtomicUsize`
    with a relaxed load is a rendering choice, not an observation.
13. **`hydra_travel` duplicates a slice of this crate.** That crate's
    `route::here` is a four-byte tail call to `hydra_map::follow::room_of`
    (`0x1015b3304`), but it was recovered while `hydra_map` was still a stub and
    therefore carries its own `map` module and a `room_of(map, state)` with two
    arguments. This crate's `room_of` takes the four the binary takes (plus a
    [`follow::Standing`] instead of a `GameState`). Nothing in `hydra_travel` was
    changed — unifying the two is a follow-up, not part of this recovery.

---

## Tests

`cargo test -p hydra_map --offline` — **65 tests**, all passing, all asserting
recovered values:

* `scene` (10) — the full eight-key document round-tripped byte for byte, the four
  child structs' key order/types, an unknown key ignored exactly like serde's
  `ignore` arm, the `OnceLock` room index (including a miss), and the three
  `EdgeKind` names; then the derive itself: text→value→struct and
  struct→text→struct, each child struct on its own, the string escape set
  (`"`, `\\`, `\n`, `\r`, `\t`, `\b`, `\f`, `\u00XX`, `/` and non-ASCII passed
  through), `\uXXXX` including a surrogate pair, every JSON number form in an
  `f32` slot, serde's own type/variant error wording, and the proof that the
  skipped `OnceLock` index never enters the document.
* `cena` (4) — `title_from_subtitle` on both shapes, `locate`'s
  number-then-name precedence (including a stale number falling through), the
  `HYDRAMAP` round trip plus its two refusal wordings, and
  first-title-wins indexing.
* `overrides` (8) — the exact `{"areas":[{"area":…,"rooms":[…]}]}` document and
  the `[{"id","uid","title","area"}]` export, save→load→export against a real
  directory, `applied` rewriting a title and an area, `pocket`'s grouping,
  `written`'s atomic replace (no `*.tmp` left behind); then the derive:
  value round trip, absent/unknown keys, a lone `Exported`, a broken file
  loading as the empty document (`{"areas":{}}` included), and the escape set.
* `ocean` (7) — all eight heading names and abbreviations plus the negatives, the
  two delta tables and the four opposites, the thirteen-column parse (cells,
  eight neighbours, ports, piers), `runs_true`'s three conditions including the
  reverse-link failure, `bound` by name/number/`u`-prefix, `port_named`'s
  exact/prefix/word passes, `piers_of`/`piers_titled`/`port_at`/`port_of_pier*`.
* `course` (5) — `runs_true` across a 3×3 block, a straight run, a diagonal run, a
  held heading forcing the first move and then coming about, and the
  unreachable-destination cases.
* `follow` (3) — ways out named after the target's area (brackets trimmed) or
  `room <id>`, de-duplicated by that name and ordered by it, with the word
  trimmed/lower-cased and the price and condition kept; `room_of`'s
  hint-versus-generation rule; and a `Follower` placing the walker, listing the
  ways out and clearing the placement when the room is lost.
* `room` (4) — search by number (`u`-prefixed) and by word, the bracket-trimmed
  path, both `described` shapes line for line, and the shared decimal reader.
* `sailing` (3) — the fix by pier title, the failure path resetting the state
  byte to `4`, and `aboard` selecting the state byte.
* `atlas` (7) — area indexing and the sorted name list, `sheet` queueing once,
  `progress`'s two counts, `waiting_for` going `Queued`→`Already`, a closed atlas
  refusing work, the worker loop laying a place out and stopping, and `held`
  counting live workers.
* `next_door` (3) — edge rooms per heading, `beside`, and `next_door` naming
  `area@room` while queueing an unknown place.
* `shown` (3) — `from` returning the tail at the *last* passage for both route
  and course, and the two caches staying separate.
* `lib` (8) — three FNV-1a 64 vectors, `decode`'s fingerprint stability, refusal
  of a non-map blob with the recovered wording, `load` through `HYDRA_MAP`
  (success and the `cannot be read` wording), `shared` answering the same way
  twice; then the real `tokio`: `decode_task` answering exactly what `decode`
  answers (fingerprint, room, and the refusal path), `load_task` reading the
  `HYDRA_MAP` file and its `cannot be read` wording on the blocking pool, and
  `shared_task` initialising the same `OnceLock` `shared` reads. The two
  `HYDRA_MAP` tests (sync and async) take a mutex, because the variable is
  process-wide.

Verification:

```
cargo check -p hydra_map --offline   # clean, no warnings
cargo test  -p hydra_map --offline   # 65 passed
cargo check --workspace --offline    # clean
grep -Rn 'todo!\|unimplemented!' crates/hydra_map/src   # empty
```

---

# Phase 2 audit: coverage table

## Inventory method and counts

1. `symbols_demangled.txt` — the 53 `hydra_map` lines (52 `T` functions plus the
   one data symbol `atlas::LIVE`).
2. `nm -a hydra | rustfilt` — **133** `hydra_map` text symbols (`T`+`t`), i.e.
   **81** the globals file never lists: `atlas::{ask,laid,waiting_for,work}`,
   `next_door::{cells,beside}`, `overrides::{written,filed_in}`,
   `room::found::{{closure}}`, the whole `lay` module, every
   `Deserialize`/`Serialize` impl and every `drop_in_place`.

Extents come from the local table, so every size below (and above) is a real
function size — this is what makes `Atlas::ask` (1 484 B) and `Atlas::work`
(8 084 B) separable instead of one 8 572-byte blob.

**Deduplication.** 133 local symbols → **138** inventory rows before filtering,
**79** excluding `drop_in_place` glue. Only true monomorphisations are collapsed:
the same `drop_in_place<T>` emitted once per codegen unit (e.g.
`Vec<scene::SceneRoom>` twice, `Overrides` twice) is one row, and the two
`SpecFromIterNested`/`SerializeMap::serialize_entry` instantiations are grouped
under the item that owns them.

## Status vocabulary

**implemented** — behaviour read out of the instruction stream and asserted ·
**partial** — door/literals/call graph recovered, a predicate or an out-of-crate
dependency is not · **unknown** — the binary shows the shape, not the behaviour ·
**missing** — in the binary, absent (or default-only) here · **fixed** — changed
this pass.

## `lib.rs`, `scene.rs`, `overrides.rs`, `atlas.rs`, `next_door.rs`

| symbol | B | status | evidence / what is missing |
|---|---|---|---|
| `decode` | 504 | partial | FNV-1a 64 + `ArcInner<Map>` certain; `cena_map::binary::decode` out of crate (see below) |
| `decode`'s fingerprint spelling | — | unknown | the format is inside the piece blob at `0x10245c048`, which does not decode with the crate's scheme; 16 lower-case hex digits is a guess (value asserted, spelling not) |
| `load` | 384 | implemented (one seam) | `HYDRA_MAP` → `fs::read` → `decode`; the embedded image is a seam |
| `shared` | 64 | implemented | `OnceLock` + `get_or_init(load)` |
| `scene::{MapScene,SceneDoor,SceneEdge,SceneRoom,SceneLabel}::serialize`, `Exported::serialize` | 280…396 | implemented (serde) | key order read entry by entry |
| `scene::*::__Visitor::expecting`, `MapScene::__FieldVisitor::visit_str`, `EdgeKind::deserialize` | 20/20/432/492 | implemented (serde) | five visitors + one full field visitor |
| `scene::MapScene::room` | 268 | implemented | room → scene room by place + room id |
| `Overrides::{load,save,export,applied}` | 184…1176 | implemented / partial (`applied`'s matching keys inferred) | |
| `overrides::{written,filed_in}`, `overrides::pocket` | 460/1564/1720 | implemented / partial (`pocket`'s argument list inferred) | |
| `Atlas::{area_names,area_of,close,held,progress,sheet}` | 12…388 | implemented | field map at `+0x00/+0x30/+0x60/+0x170/+0x1e0/+0x290` |
| `Atlas::ask`, `Atlas::laid`, `Atlas::waiting_for` | 1484/476/468 | implemented | queue semantics in order; `2` = already laid |
| `Atlas::{scene_of,start}` | 920/4976 | partial | the scene source and the layout worker are `cena_map_layout` seams |
| `Atlas::work` | 8084 | partial | loop/queue/notify recovered; the **name resolution is `cena_map_layout`** — the split on `'@'` and the 24-byte sorted table are only its input. The earlier "the worker binary-searches a sorted table" was right about the *search*, but the table is not data this crate can reach |
| `atlas::LIVE` | data | implemented / unknown type | 8-byte load; `AtomicUsize` is a modelling choice |
| `Atlas::next_door` | 7176 | partial | place split on `'@'`, sheet from `laid`, geometry from `cells`/`beside`, unknown places `ask`ed. The two extra arguments are stored beside the name at `[sp, #0x90]`/`[x29, #-0xe0]` and the `NextDoor` field list is not in the dump (the struct is only built and dropped here) |
| `next_door::cells`, `next_door::beside` | 280/2072 | partial | a room vector at `[arg + 0x20]` → a 0x30-byte record; `beside` is 2 072 B, bigger than "a companion predicate" suggests, and its comparison is not in the dump |

## `lay.rs` — **missing module**

| symbol | B | status | evidence |
|---|---|---|---|
| `lay::open_place` | 616 | **missing → none** | `Vec::from_iter` over `(ptr, ptr + len*4)` (u32 room ids) → `cena_map::Map::from_rooms` → `cena_map_layout::pipeline::generate_layout` → `cena_map_layout::scene::build_scene` → `cena_map_layout::open::open_place` (4) → `lay::convert(x0, x1, x2, &scene or &open)`; `None` niche on either miss |
| `lay::places` | 268 | **missing → none** | `cena_map_layout::areas::layout_rooms` → `Map::from_rooms` → … |
| `lay::marks` | 172 | **missing → none** | `Map::room` → `Vec<String>` → `sort` → `Vec::dedup_by` |
| `lay::convert` | 884 | **missing → none** | 5th arg is a `MapScene`; builder for `hydra_map::scene` |
| `lay::lay_out` | 324 | **missing → none** | |

**Why they were not implemented.** All five are thin wrappers whose middle step is
`cena_map_layout` (layout generation, area layout, `lay::{open_place,marks,
places,convert,lay_out}`), which is **not in the workspace pins**. What *is*
in the binary is the wrapper shape (`lay::marks` alone could be reproduced —
`Map::room`, collect, sort, dedup — but the `Vec<String>` it collects is produced
by an iterator whose body is not in the dump, so which strings they are would be a
guess). The earlier note said these helpers "belong to that crate"; they do not —
they are `hydra_map` functions. That claim is corrected here, and the seam is
recorded rather than guessed.

## `ocean.rs`, `course.rs`, `follow.rs`, `room.rs`, `sailing.rs`, `shown.rs`

| symbol | B | status | evidence / what is missing |
|---|---|---|---|
| `ocean` (accessor) + `OCEAN` | 64 | implemented / partial init | `OnceLock<Ocean>`; the initialiser parsed the embedded tables |
| `Heading::named` | 888 | implemented | length-dispatched compare chain, tag 8 = `None` |
| `Ocean::{at,room,port_at,port_named,port_of_pier,port_of_pier_titled}` | 52…824 | implemented | four maps at `+0x48/+0x78/+0xa8/+0xd8` |
| `Ocean::{piers_of,piers_titled}` | 52/260 | partial | the `Item` of the opaque iterator is not readable; a case-insensitive substring test is used |
| `Ocean::bound` | 944 | implemented / names inferred | glyphs exact |
| `Ocean::runs_true` | 408 | implemented | the three conditions, `TABLE[h ^ 4]` |
| `Ocean::read` | 6112 | partial | 13-column and 3-column rows; how a text is told from another is a field-count dispatch here |
| `Ocean::course`, `Ocean::course_holding` | 12/2760 | partial | nine states per room, RLE output and the `(u32, Heading)` run element are exact; **the cost metric is not in the dump** — this build minimises `(moves, turns)` |
| `follow::ways_out_of` | 2616 | partial | **corrected**: the compass compare is against an **8-byte tag at `exit + 0x50`**, not against the exit's word; the word is the `String` at `+0x60`, lower-cased and used only for naming. Each entry is named after the target place — `Map::room(exit.to)` → first path string trimmed of `[`/`]`, or `format!("room {}", to)` (`0x103e23ae3`, decoded) — de-duplicated and ordered by that name. What the tag comparison *selects* is still not recovered |
| `follow::room_of` | 908 | partial | four arguments; `title_from_subtitle`, the `"room exits"` component and `Map::locate` |
| `follow::Follower::{view,ways_out}` | 2340/44 | partial | cache at `+0x78/+0x7c/+0x80`; the drawing half is a UI seam |
| `room::found` | 1236 | **fixed (partial)** | see below |
| `room::found::{{closure}}` | 316 | implemented | path (first area, `[]`-trimmed), title, id at `+0x30` |
| `room::described` | 1192 | partial | formats exact; which of `" (u{})"`/`" ({})"` goes where is inferred from stack order |
| `sailing::Navigator::fix` | 3360 | **fixed (partial)** | see below |
| `ShownRoute::from`, `ShownCourse::from` | 68/68 | implemented | tail at the *last* match |
| `show_route`, `show_course` | 848/860 | **fixed** | keys are lower-cased (`<str>::to_lowercase`); the body also `remove_entry`s first (not observable with one value per key) |
| `shown_route` | 672 | **fixed** | keys are lower-cased |
| `shown_course` | 672 | **fixed (full)** | 672 B, the same call set as `shown_route` one for one: `Mutex::lock/unlock`, `to_lowercase`, `hash_one`, `OnceLock::initialize`, `Arc` drop. **No insert, no ocean call → no build-on-miss.** The earlier "may build a course from the ocean on a miss" is refuted |

## What changed this pass

1. **`room::found`'s numeric half is two lookups, not one.** A bare number is
   asked of `cena_map::map::Map::room` (`0x1015b0eac`) *and* of
   `cena_map::map::Map::ids_for_uid` (`0x1015b0f00`); a `u`-prefixed number is
   asked only of the uid index (`b.eq 0x1015b0ef8` at `0x1015b0ea0`); ids already
   collected are skipped (`0x1015b0f74`). Added
   [`cena::Map::ids_for_uid`] (the uid is the last element of `Room::paths`, which
   is what `room::described` formats as `" (u{})"`) and rewrote `found` around it.
   New test `a_number_is_looked_up_as_a_map_id_and_as_a_uid`, plus a fixture room
   whose map id (`500`) and uid (`7116002`) differ so the two halves are
   distinguishable.
2. **`shown_*` normalises its keys.** All four entry points lower-case the name
   before touching the map, so `"Bank"` and `"bank"` are one entry. New test
   `keys_are_lower_cased`. `shown_course` is now recorded as **full**.
3. **`Navigator::fix` no longer pretends to tell `1`/`2`/`3` apart.** The state
   byte is set to [`sailing::FIXED`] on a fix and [`sailing::UNFIXED`] (`4`,
   compared at `0x1015b4028`) otherwise, and the one-bit aboard field the
   original keeps beside it (`and w8, #1` / `strb w8, [x21, #1]` at
   `0x1015b4cf0`) is now `Navigator::aboard`. The invented `aboard → 3` is gone.
4. **`lay` is recorded as a missing module of this crate**, with all five
   signatures and call graphs, instead of being attributed to `cena_map_layout`.
5. **`ways_out_of`'s compass comparison is on a tag, not on the word.**
6. **`cargo test -p hydra_map --offline` is 67** (65 before; two new tests, one
   existing one rewritten in place).

## What remains, honestly

* `lay::{open_place,places,marks,convert,lay_out}` — needs `cena_map_layout`;
* the embedded 22 MB map image and `cena_map::binary::decode`'s record layout —
  no recoverable layout, so [`DEFAULT_MAP`] stays empty and `decode` accepts this
  build's own encoding;
* `Ocean::course*`'s cost metric, `ways_out_of`'s tag selection,
  `next_door`'s two extra arguments and `NextDoor`'s fields, `Atlas::work`'s
  resolution, `room::found`'s word predicate and third argument,
  `room::described`'s note placement, `Overrides::applied`'s matching keys,
  `piers_of`/`piers_titled`'s item type, `decode`'s fingerprint spelling;
* `Navigator::fix`'s three success numerals and the order of its arms.
