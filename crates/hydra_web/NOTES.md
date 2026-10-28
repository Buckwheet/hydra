# `hydra_web` — reverse-engineering notes

Source: the arm64 Mach-O `/Users/cpmbpro/Code/hydra-inspect/hydra` (no DWARF),
demangled symbols in `symbols_demangled.txt`.  Every address below is an unslid
VA; a function ends where the next symbol begins (`nm -an`), *not* where the next
global symbol begins — several stubs are much smaller than the `recon.py`
boundaries suggested (e.g. `wire::banners` is 428 B, not 117 KB).

The original crate lived at `crates/hydra-web/src/...` (five of those paths
survive in `__cstring`).  It linked `tokio`, `tokio-tungstenite`,
`tungstenite`, `serde`/`serde_json`, `mdns-sd`, `rustls`, `getrandom`,
`bytes`, `futures-util`.

Phase 1 reconnected every crate of that list that is a workspace pin:
`serde`/`serde_json` (all wire documents), `tokio` (the driver),
`tungstenite` + `tokio-tungstenite` (handshake and frames), `base64` (the
accept key) and `mdns-sd` (behind `bonjour::Daemon`).  The four that are **not**
pinned — `rustls`, `getrandom`, `bytes`, `futures-util` (and `axum`, which the
original linked but this crate never used) — keep their `std` stand-ins.  §5 is
the current list, §7 the reconnection itself.

Legend:

| tag | meaning |
|---|---|
| **ESTABLISHED** | read out of the instruction stream, the immediates, the serde glue or `__cstring`; not in doubt. |
| **INFERRED** | fixed by the code shape and the surrounding API, but a type or a name had to be chosen. |
| **UNKNOWN** | the binary does not pin it down; the safest behaviour is implemented and marked `// UNKNOWN:` in the source. |

ABI facts used throughout: `&str` is `(ptr, len)`; `String`/`Vec` are
`cap@0, ptr@8, len@16`; `Option<String>` uses `isize::MIN` in the capacity word as
its `None`; a `char` niche is `0x110000`; sret is `x8`.

## 0. How the evidence was read

Throwaway tooling outside the repository:

* an `nm -an` symbol walk so every address has a real end (72 002 text symbols),
* an `objdump --no-show-raw-insn` driver with `adrp`/`add`/`ldr` literal
  resolution and `bl`→symbol annotation,
* a **serde extractor**: for every `…::serialize`/`serialize_entry` call site the
  `(key ptr, key len)` pair and the value offset are printed, which is how the
  field names *and their order* below were read,
* a `movz`/`movk` decoder for the word-compare `match`es (`"GET"`, `"PUT"`,
  `"ui"`, `"wheel"`, `"buttons"`, `"upgrade"`, `"websocket"`,
  `"sec-websocket-key"`, the `In` field names),
* a length-prefixed piece decoder for `__cstring` format strings
  (`<len><bytes>`, `0xC0` = `{}`), and
* a whole-`__text` `bl`-xref scanner (used to find the callers of
  `one_connections`, `http::request`, `lan_address`).

## 1. Recovered types

### 1.1 The wire protocol (`wire`)

`Out` is an **internally tagged** enum: the tag field is the one-byte key `"t"`
and every variant's struct fields are spliced next to it.  27 variants, tag
strings and payload keys ESTABLISHED from `Serialize for wire::Out`
(`0x10026f844`):

| variant | payload keys (in serialization order) |
|---|---|
| `welcome` | `login` |
| `workspace` | `ask`, `status` |
| `banners` | `banners`, `ground`, `across`, `down`, `width`, `opacity` |
| `offer` | `text` |
| `theme` | `colors`, `names`, `prompt` |
| `said` | `text` |
| `view` | `state`, `state_token`, `vitals`, `roundtime`, `casttime`, `left`, `right`, `prepared`, `room`, `effects`, `conditions`, `injuries`, `spells`, `merchants`, `targets`, `chips` |
| `push_paired` | — |
| `phone` | `kind`, `text` |
| `hotbars` | `bars`, `problems` |
| `stream` | `id`, `from` |
| `themes` | `names` |
| `detached` | — |
| `refused` | `why` |
| `highlights` | `categories`, `rules`, `sounds` |
| `tasks` | spliced `TasksOut` (`brief`, `bounty`, `objectives`) |
| `hub` | spliced `HubOut` |
| `trainer` | spliced `TrainerOut` |
| `wheel` | `wheel` |
| `attached` | `name`, `game`, `streams`, `story_lines`, `stream_lines`, `id` |
| `injuries` | spliced `DollOut` (`parts`, `aspect`) |
| `menu` | `noun`, `groups` |
| `lines` | `lines`, `from` |
| `note` | spliced `NoteOut` |
| `inventory` | spliced `InventoryOut` |
| `highlights_text` | `changed`, `text` |
| `highlighted` | `text`, `kept`, `done` |

`In` is the mirror image (`tag = "t"`, 37 variants).  Every variant name is
ESTABLISHED from the serde `"struct variant In::…"` strings at
`0x102070cc0`–`0x1020710c4`:

```
hotbar note_new login phone_put highlights_merge trainer inventory streams
push_pair highlights workspace reconnect note_write highlight_remove
highlight_switch hello clip theme launch trainer_profile choose notes attach
inventory_act launch_team link highlight_save remove trainer_step send tasks
push_forget phone_get button characters wheel note_forget
```

(serde's own `expecting` list is at `0x1020710c7`: `hub, link, characters, login,
streams, wheel, theme, workspace, highlights, highlights_text, inventory, tasks,
trainer, notes, …`.)  Only `In::Login`'s field names are recovered, from
`__FieldVisitor::visit_str` at `0x1000ec28c`: `account` (7), `password` (8),
`game` (4), `character` (9), `remember` (8) → indices 0…4, everything else
`Ignore` (5).

Leaf structs (field order ESTABLISHED, offsets = the binary's):

| type | addr | fields |
|---|---|---|
| `Run` | `0x100270cd4` | `text, token, color, named, background, bold, mono, link, prompt` |
| `Card` | `0x100270f4c` | `name(+0) game(+0x18) vitals(+0x30) room(+0x48) state(+0x60) state_token(+0x70) id(+0x80) closed(+0x84)` |
| `Line` | `0x1002710d8` | `runs(+0) stream(+0x18) kind(+0x88) picture help` |
| `View` | `0x100271244` | the 16 keys above |
| `Gauge` | `0x1002714b0` | `label(+0x18) token(+0x28) percent(+0) current(+8) max(+0x10)` |
| `BarOut` | `0x1002715e8` | `name slots(+0x18) title(+0x30) columns(+0x48)` |
| `HubOut` | `0x100271704` | `cards(+0) offered(+0x18) teams(+0x30) said(+0x48) listing(+0x60)` |
| `ChipOut` | `0x10027183c` | `name mark(+0x18) words(+0x30) rows(+0x48) color(+0x60) level(+0x78) shown(+0x88)` |
| `HelpOut` | `0x1002719ac` | `command(+0) says(+0x18)` |
| `RoomOut` | `0x100271db8` | `description(+0) creatures(+0x18) objects(+0x30) players(+0x48) title(+0x60) exits(+0x78)` |
| `TeamOut` | `0x100271f0c` | `name members` |
| `SpellOut` | `0x100272028` | `name(+0) circle(+0xc0) number(+0xd8)` |
| `BannerOut` | `0x100272160` | `text(+0) key(+0x18) left_ms(+0x20)` |
| `ButtonOut` | `0x100272298` | `id label fg bg countdown dim` |
| `EffectOut` | `0x1002723ec` | `text left(+0x18) percent(+0x20)` |
| `StreamOut` | `0x100272524` | `id name` |
| `TargetOut` | `0x100272640` | `name(+0) bare(+0xc0) noun(+0xd8) statuses(+0xf0) hint(+0x108) health(+0x120) current(+0x128)` |
| `ChipRowOut` | `0x1002727d0` | `name list left up(+0x38)` |
| `EffectList` | `0x1002728ec` | `effects(+0) list(+0x18)` |
| `ListingOut` | `0x100272a08` | `account game characters` |
| `PictureOut` | `0x100272b40` | `src alt width(+0x30) height(+0x38)` |
| `TargetsOut` | `0x100272c5c` | `look(+0) targets(+0x50) arms(+0x68) known(+0x70)` |
| `MerchantOut` | `0x100272d78` | `name(+0) room(+0x60) status(+0x78) left(+0x90) announcement said rules` |
| `MenuEntryOut` | `0x100272ee8` | `label command` |
| `MenuGroupOut` | `0x100273004` | `under entries` |

Other modules' payloads (same method, ESTABLISHED order):

* `tasks::{TasksOut,ObjectiveOut}` (`0x1002cc0f0`, `0x1002cc228`,
  `0x1002cc354`): `brief(+0) bounty(+0x18) objectives(+0x30)` /
  `name kind location state description` (5 Strings at 0x18 stride).
* `trainer::{TrainerOut,SheetOut,SkillOut}` (`0x1002cc9d4`, `0x1002ccaf0`,
  `0x1002cc748`, `0x1002cc880`): `profiles(+0) said(+0x18) sheet(+0x30)
  busy(+0x98)` / `title(+0) physical(+0x18) mental(+0x30) skills(+0x48)
  changed(+0x60)` / `section(+0) name(+0x18) cost(+0x30) has(+0x48) goal(+0x50)
  id(+0x58)`.
* `notes::{NoteOut,NotesOut}` (`0x1002c298c`, `0x1002c2ae0`, `0x1002c2bf8`):
  `who character name changed text lines`; `NotesOut` has **two** `Serialize`
  impls, one writing `notes`, one writing `problem` → an untagged enum.
* `inventory::{InventoryOut,GroupOut,ItemOut,DetailOut,ActOut}`
  (`0x1002c3434`, `0x1002c31c4`, `0x1002c2fe4`, `0x1002c32fc`, `0x1002c2ec8`):
  `summary(+0) groups(+0x18) items(+0x30) item(+0x48) known(+0x90)
  room_left(+0x91)` / `ids(+0) heading(+0x18) tab(+0x28)` / `within(+0) id(+0x18)
  parent(+0x30) noun(+0x48) name(+0x60) pounds(+0x120) acts(+0x138) count(+0x150)
  fill(+0x168) container(+0x178) locked(+0x179)` / `id name sections` /
  `act(+0) label(+0x10)`.
* `injuries::{DollOut,PartOut,ShapeOut,InjuryOut}` (`0x1002964e0`,
  `0x100296628`, `0x100296744`, `0x100296aac`): `parts(+0) aspect(+0x18)` /
  `shape(+0) id(+0x20) name(+0x30) anchor(+0x40)` / an internally tagged enum on
  `kind` with `circle{c,r} line{a,b,w} block{min,max} letter{c,letter}` /
  `id name token rank`.
* `highlights::{Form,RuleOut,MarkForm,CategoryOut}` (`0x1002c6140`,
  `0x1002c6588`, `0x1002c66c0`, `0x1002c67f8`): `Form` has **33** fields, in this
  serialization order: `category, find, pattern, case_sensitive, whole_word,
  color, background, line, squelch, substitute, sound, alert, notify, push, send,
  when, rearm, timer, seconds, caught, times, down, list, flag, status, rumble,
  picture, cooldown, volume, speak, characters, stream, off`; `RuleOut` =
  `name form problem`; `MarkForm` = `name seconds clear`; `CategoryOut` =
  `name on`.

### 1.2 Constants recovered from `__cstring`

| literal | where |
|---|---|
| `web.token` (9) | `0x1020725f5` — pairing file |
| `web` (3) | `0x1020725fe` — workspace directory |
| `web.storage_version` (19) | `0x1020725f5+9` — pairing/storage TXT key |
| `layout` (6) | `0x1020725f5+28` — TXT key |
| `v1_hydra._tcp.local.` (19) | `0x1020725fc` — mDNS service type |
| `highlights.toml` (15) | `0x1020725b9` |
| `sounds` (6) | `0x1020725b3` |
| `phone` (5) / `ui` / `wheel` / `buttons` | `0x102071b88`, `phone::file` |
| `{}_{}.workspace.json` | `0x103e263b8` |
| `{}.json` (phone state) | `0x103e26104` |
| `{} cannot be read: {}` / `{} cannot be written: {}` | `0x103e25e7b`, `0x103e23bbe` |
| `HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n` | `0x103e23430` |
| `HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n` | `0x103e23498` |
| `http://`, `:`, `/#pairing=` | `0x103e23590` |
| `[hydra] the web page is not announced on the network: {}` | `0x103e235ae` |
| `The web page cannot be served on port {}: {}` | `0x103e23568` |
| `Hydra is not ready for that yet.` | `0x1020699b3` |
| `/wheel-editor.js` | `0x102069a90` |
| `A character is logged in from the device Hydra runs on.` (55) | `0x102071fec` |
| `That character is no longer on the table.` (41) | `0x102072023` |
| `Not sent: that item does not offer that; ` (41) | `0x10207204c` |
| `Refresh the inventory.` (22) | `0x102072050` |
| `The copy is too big to merge.` (29) | `0x10207208b` |
| `Not yet: the character is not known yet.` (40) | `0x1020720ae` |
| `Push: this phone was not paired for push.` (41) | `0x102071f9e` |
| `Push: this phone gets no more pushes.` (37) | `0x102071fc7` |
| `Hydra does not update itself on this system` | `0x10207018c` (not this crate's) |
| `Load reads your skills from play.net.` / `Reading the skill manager…` / `Applying…` / `The skill manager` | `0x1020726f4`…`0x1020720d7` |

## 2. Function-by-function status

### 2.1 `lib.rs`

| function | addr | status | notes |
|---|---|---|---|
| `lan_address` | `0x1002cbf3c` | **fully** | `Option<IpAddr>`. `UdpSocket::bind(0.0.0.0:0)` (the 8 zeroed bytes at `sp+0x10` are `family=0, ip=0.0.0.0, port=0` in std's `SocketAddr`: `family u16@0, ip@2, port@6`, proved from `bind::inner` `0x101ea6198`), `connect(192.0.2.1:9)` (`movk w8,#0x102,lsl16` + `#0xc0` stored at `+0x12` = the octets `c0 00 02 01`; `strh #9` at `+0x16`), `local_addr()`. Result tags: `0`=`V4`, `1`=`V6`, `2`=`None` (the `cmp w9,#2` at `0x1002cbfc4` is the `Err` tag of `io::Result`); an all-zero address is `None`. |
| `Server::new`, `url`, `not_announced` | — | **modelled** | Root + pairing token + port + announced; the token comes from `pairing::pairing(root)` (`web.token`), the URL is `http://<lan>:<port>/#pairing=<token>` (`0x103e23590`). |
| `Server::bind`, `serve_blocking`, `page`, `page_html`, `answer` | `0x100155da0`-family (the accept loop) | **Phase 7 addition (`INFERRED`)** | The accept loop is now real and blocking: `bind` opens `0.0.0.0:<port>` **non-blocking** (the recovered address), `serve_blocking` polls it every `POLL` (10 ms) until its `stop` flag is set, and `answer` reads one request head (`HEAD_TIMEOUT`, 2 s), then answers `GET /` with `http::respond(200, "text/html; charset=utf-8", page)` and anything else with a `404`. `page_html` is a **synthetic stand-in**, not the recovered page: it says `Hydra`, the character and the pairing token. No websocket upgrade, no mDNS (`bonjour` untouched), no asset for `/wheel-editor.js`. Two unit tests cover the page/response bytes and a live `GET /` + `404` on `127.0.0.1`. |
| `one_connections` | `0x1002cc054` | **unknown** | 156 B, pure, no literals, no calls; a 2-bit tag match over a dereferenced word with byte compares against 36 and two bit-test masks. Only caller is the accept loop's `poll_fn` (`0x100155da0`), which drops the accepted stream when it returns `false`. Kept as "always allow", see the `// UNKNOWN:` block in `lib.rs`. |

### 2.2 `http.rs`

| function | addr | status | notes |
|---|---|---|---|
| `request` | `0x1002c2160` | **fully** | Splits the head on `\r\n` (`StrSearcher` needle at `0x1020722ae`), the request line on `' '`, every other line at the first `':'`, trims both halves, lower-cases the name (`(c&!0x20)` folding) and matches `upgrade` (7) / `sec-websocket-key` (17) against `websocket` (9). The target loses everything from `'#'`. Returns `None` iff `!upgrade_websocket && key.is_none()` (`0x1002c2880`). Result struct: 0x48 bytes, `method/target/version` at `&str` stride + `Option<String>` key; the original has one extra leading word (the `Option` niche) that is not readable — see `// UNKNOWN:` in the source. |
| `accept_key` | in `respond` | **fully** | `base64(sha1(key + GUID))`; the pool holds `ABCDEFGHIJ…+/` and the RFC 6455 GUID is required by the recovered `101` template. |
| `switching`, `respond`, `read_head` | `0x1000af284`, `0x1002b04cc` | **partly** | The *response bytes* are the recovered format strings; the async reader (`http::read_head`, `http::answer`, `http::respond` closures) became the synchronous helpers. |

### 2.3 `pairing.rs`

| function | addr | status | notes |
|---|---|---|---|
| `matches` | `0x1002ca7c8` | **fully** | Length compare (`x1` vs `x3`), empty ⇒ `true`, otherwise XOR-accumulate — constant time. |
| `pairing` | `0x1002ca91c` | **fully** | `root.join("web.token")`, `read_to_string`; `NotFound` or an invalid token ⇒ mint; other errors ⇒ `Err("{} cannot be read: {}")`. A valid token is exactly 64 hex characters (`cmp x1,#0x40` then `(c-0x30)<10 || ((c&!0x20)-0x41)<6`). Minting: 32 random bytes (`getrandom`, `w1=0x20`) → 64 lowercase hex via the `0123456789abcdef` table at `0x102072620`, written with `hydra_session::store::save_bytes`; failure ⇒ `Err("{} cannot be written: {}")`. |
| `fresh_token`, `random_bytes`, `is_token` | — | **INFERRED** split | Same behaviour, exposed for tests; `/dev/urandom` + xorshift fallback replaces `getrandom`. |

### 2.4 `workspace.rs`

| function | addr | status | notes |
|---|---|---|---|
| `path` | `0x1002cadd4` | **fully** | `root.join("web")` (3-byte join at `0x1020725fe`) then `format!("{}_{}.workspace.json", account.to_uppercase(), character.to_lowercase())` (`to_uppercase`/`to_lowercase` calls at `0x1002cae38`/`0x1002cae5c`, format piece at `0x103e263b8`). |
| `revision` | `0x1002cb280` | **fully** | `serde_json::from_*` then the `revision` field. |
| `answered` | `0x1002cafd4` | **fully** (control flow) | `(file, method, body)`. `read_to_string`; non-`NotFound` error ⇒ `500`; method must be 3 bytes and `GET` (`0x4547`+`'T'`) or `PUT` (`0x5550`+`'T'`) else `400`; `GET` with nothing stored ⇒ `404`, else `200` + document; `PUT` with an empty body or no `revision` ⇒ `400`, body > `0x10000` ⇒ `413` (`mov w8,#0x19d`), stored revision strictly newer ⇒ `412` (`mov w8,#0x19c`), otherwise it saves via `save_bytes` and stamps `.storage_version` (`0x102072601`). The stamp's content is `// UNKNOWN:` (decimal revision). |

### 2.5 `phone.rs`

| function | addr | status | notes |
|---|---|---|---|
| `file` | `0x10029f7cc` | **fully** | Kind ∈ {`ui` (2), `wheel` (5), `buttons` (7)} else `None`; `root.join("phone").join(format!("{kind}.json"))`. |
| `keep` | `0x10029f94c` | **fully** | Validate as JSON (`serde_json::de::from_trait`), create the parent (`DirBuilder::create`), write with `save_bytes`. |
| `kept` | `0x10029fd2c` | **fully** | `file` + `read_to_string` + `metadata` + `modified` + `duration_since(UNIX_EPOCH)` → `(text, secs)`; any missing piece ⇒ `None`. |

### 2.6 `notes.rs`, `tasks.rs`, `trainer.rs`, `injuries.rs`

| function | addr | status | notes |
|---|---|---|---|
| `notes::read` | `0x1002c2d30` | **fully** | `hydra_session::notes::load`; `Vec<NoteOut>` via `from_iter`; on `Refused` the `Display` text becomes the `problem` variant. |
| `tasks::tasks` | `0x1002cc48c` | **partly** | The stream `"tasks"` (needle len 6) is fetched from `hydra_ui::story::streams::Streams::get`; `brief`/`bounty`/`objectives` are filled from it. `// UNKNOWN:` the in-stream markup, so the parser accepts `brief:` / `bounty:` / an `objectives` heading with `name: description` rows and otherwise keeps the whole text as `brief`. |
| `trainer::trainer` | `0x1002ccc38` | **partly** | Built from `Seat::trainer` (`+0x1e0`), the goal profiles and the parsed sheet; the four advice strings are recovered. `// UNKNOWN:` the sheet markup (parsed as `section:name:cost:has:goal`). |
| `trainer::sheet` | — | **partly** | Same parser, exposed for tests. |
| `injuries::doll` | `0x100296be4` | **partly** | No arguments; allocates 1008 B = 14 × 72-byte `PartOut`, maps the static table at `0x104382b70` (f32s `0.085`, `0.5`, `0.105`, …) through `Map::fold`, and stores `aspect = 0.75` (`mov w8,#0x3f400000`, offset `+0x18`). `// UNKNOWN:` the per-part geometry/anchors; the names come from `0x102071980`. |
| `injuries::*` shapes | `0x100296744` | **fully** (shape) | `kind`-tagged `circle/line/block/letter`. Numeric widths are inferred from the field offsets. |

### 2.7 `inventory.rs`

| function | addr | status | notes |
|---|---|---|---|
| `inventory` | `0x1002c3f38` | **partly** | `hydra_ui::inventory::{roots, summary, left_room}` + `InventorySnapshot::{get, details}` + `add`; grouping captions are recovered (`Hands`, `Worn`, `Worn containers`, `At your feet`, `Reserved`, `Room containers`, `On the ground`). `// UNKNOWN:` how `roots` orders the groups. |
| `offered` | `0x1002c3e60` | **fully** (decision shape) | `InventorySnapshot::get` + `Act::named` + `Act::offered` (which consults `container`/`shut`). Which acts an item offers is `// UNKNOWN:` beyond container/wear. |
| `add` | `0x1002c3708` | **partly** | Field mapping recovered; `pounds`/`fill`/`count`/`acts` come from the snapshot detail. `fill` uses the recovered `filling`/`full`/`critical` words. |
| `Act` | — | **INFERRED** | Stand-in for `hydra_ui::inventory::Act`; the eight command words and labels are ESTABLISHED from the pool at `0x102071900`. |

### 2.8 `highlights.rs`

| function | addr | status | notes |
|---|---|---|---|
| `sounds` | `0x1002c99f8` | **fully** | `root.join("sounds")` + `read_dir`; an error returns an empty `Vec` (the `stp xzr, #8` at `0x1002c9a64` is `Vec::new`); the names are collected and **sorted** (`insertion_sort_shift_left` < 0x15, `driftsort_main` above). |
| `changed` | `0x1002c9b10` | **fully** | `root.join("highlights.toml")` + `metadata` + `modified` + `duration_since(UNIX_EPOCH)`; only the seconds word is returned, failures are `None` (`mov x0,#0`). |
| `listed` | `0x1002c9938` | **fully** | `HighlightFile::category_names` → `Vec<CategoryOut>`, then the rules → `Vec<RuleOut>`. |
| `Form::of` | `0x1002c6914` | **partly** | Field-by-field clone out of the rule; `Condition::clone` + `join(", ")` (`0x1020725b0` = `", "`) produce the `when` column. `// UNKNOWN:` which rule fields are UI-only. |
| `Form::worn` | `0x1002c74f8` | **partly** | `trim_matches` on `find` + field-by-field copy back; uses `MarkForm::mark`. |
| `MarkForm::mark` | `0x1002c9c80` | **fully** | Trim the name; empty ⇒ `None` (niche `0x8000000000000001`); otherwise name + `seconds` + `clear`. |
| `save` | `0x1002c8350` | **partly** | Each form → `Form::worn` → `HighlightFile::named`/`put`; the original then `format!`s the file text. `// UNKNOWN:` the TOML writer. |
| `merge` | `0x1002c93b8` | **partly** | `HighlightFile::parse` + `BTreeMap::insert`/`put`; the size refusal is `The copy is too big to merge.` `// UNKNOWN:` the size limit (64 KiB used) and the document syntax (a documented `category|pattern|color|background|squelch` line form is used so it round-trips). |

### 2.9 `follow.rs`

| function | addr | status | notes |
|---|---|---|---|
| `Follow::take` | `0x10029122c` | **partly** | Walks the notice queue, compares `hydra_session::notice::Body` (`PartialEq` is called from the loop), clones `hydra_ui::run::StyledRun` runs and collects `Vec<wire::Line>`. `// UNKNOWN:` the exact body variants (the recovered `hydra_session` does not expose that enum) — three bodies are modelled. |

### 2.10 `bonjour.rs`

| function | addr | status | notes |
|---|---|---|---|
| `Announced::start` | `0x1002c9d90` | **partly** | Instance name from `COMPUTERNAME` (12-byte env read at `0x1002c9dd0`), service type `v1_hydra._tcp.local.`, TXT keys `web.token`, `web.storage_version`, `layout`. The `mdns_sd` daemon is a trait seam (`Daemon`). `// UNKNOWN:` the TXT value encodings. |
| `Drop for Announced` | `0x1002c5fc4` | **fully** | `ServiceDaemon::unregister` then `shutdown` (`0x1002c5fc4` calls both) — reproduced through the seam. |

### 2.11 `client.rs`

`Page` field offsets that are visible: the outbox `Vec<Out>` at `+0x348`
(elements `0xa8` = 168 bytes apart — the `Out` size), the latch byte at `+0x594`,
and `Arc<Seat>`-ish values at `+0x328`.

| function | addr | status | notes |
|---|---|---|---|
| `message` | `0x1002bb490` | **fully** | `serde_json::to_string(&Out)` then `bytes::Bytes::from(String)`. std-only: returns the JSON bytes. |
| `refused_here` | `0x1002b33fc` | **fully** | Latch at `+0x594`; queues `Out::Refused{why: LOGGED_IN_HERE}` (55-byte literal), flips the latch, answers the new state. |
| `push_asked` | `0x1002b2f44` | **fully** | `hydra_board::push::{pair, forget}`; unpaired ⇒ `PUSH_NOT_PAIRED`, forget ⇒ `PUSH_EXHAUSTED`, else `PushPaired`. |
| `phone` | `0x1002b94a4` | **fully** | `phone::kept` → `Out::Phone{kind, text}`. |
| `phone_put` | `0x1002bb37c` | **fully** | `phone::keep` then re-send the state. |
| `highlights_text` | `0x1002b41c8` | **fully** | `Highlights::file` + `HighlightFile::to_toml` + `highlights::changed` → `Out::HighlightsText`. |
| `highlights_merge` | `0x1002b43d8` | **fully** | Too big ⇒ `TOO_BIG_TO_MERGE`; otherwise merge + re-send the text. |
| `workspace_of` | `0x1002b350c` | **fully** | `Sessions::glance` + `workspace::path`; `Out::Workspace{ask, status}`. |
| `game_said` | `0x1002bb1d4` | **fully** | `Seat::symbol` (fallback `'.'`), `Seat::lich_symbol` (fallback `';'`, both via `cmp #0x110000`+`csel`), `hydra_ui::run::game_line`, `Seat::send`. |
| `press` | `0x1002b96b4` | **fully** | `Seat::story` + `Seat::send`; no character ⇒ `NOT_KNOWN_YET`. |
| `button` | `0x1002bb018` | **fully** | `hotbars::button_lines` → each line through `Seat::send`. |
| `wheel` | `0x1002b9974` | **partly** | `pad::touch::touch_command` + `trim_matches` + `Seat::send`; `// UNKNOWN:` the direction table (modelled `up/down/left/right/centre`). |
| `link` | `0x1002b46cc` | **partly** | `object_menu::link_command` + `Seat::snapshot` + `Seat::send` + `game_said`; `// UNKNOWN:` the noun→command table. |
| `inventory_act` | `0x1002b3734` | **partly** | `Seat::snapshot` + `inventory::offered` + `Act::sent` + `Seat::symbol` + `Seat::send`; refusal text recovered. |
| `trainer_asked` | `0x1002b3ab0` | **partly** | `Seat::trainer`, `GoalProfiles::{load, change}`, `SkillGoals::targets`, `Seat::{change_goals, apply_goals}`, a `tokio` handle for the async save. Modelling only. |
| `trainer_step` | (branch of the above) | **partly** | Step edits are sent as `<symbol> train <skill> <step>`. |
| `attach` | `0x1002b9c04` | **partly** | `Story::scrollback` (story + streams), `Sessions::own_streams`, `injuries::doll`, `settings_store::load`, `pad::touch::touch_wheel` → `Out::Attached`/`Out::View`/`Out::Injuries`/`Out::Hub`. The payload shape is recovered; how the scrollback is trimmed is not. |
| `look` | `0x1002b4c84` | **partly** | The big state builder: `Highlights::{generation, file}`, `Sessions::{palette, glance}`, `theme::Palette::by_name`, `Own::{load, pictures_folder}`, `highlights::{listed, sounds}`, `HubOut`. Only the message set is modelled. |
| `panel` | `0x1002b8cd4` | **partly** | Notes: `notes::{write, rewrite, remove, stamp}` → `Out::Note`. Which note the panel opens is `// UNKNOWN:`. |
| `wear` | `0x1002b8980` | **partly** | `theme::file::Themes::{load, outfit}` → `Out::Themes`. |
| `TrainerPanel::open` | `0x1002b2e78` | **fully** | `root.join("trainer")` (7-byte join at `0x102071f8d`) + `GoalProfiles::load`; a missing folder is an empty map. |

### 2.12 `wire.rs` builders

| function | addr | status | notes |
|---|---|---|---|
| `run` | `0x100273120` | **fully** | Reads the run's text, `Option<String>` at `+0x30` (niche `isize::MIN`), token at `+0x48`/`+0x60`, bold at `+0xa9`, link slots; `hydra_ui::run::run_token` + `Token::name`. A `_`-only text is replaced by the token name. |
| `story_line` | `0x10026e804` | **partly** | 7 story kinds (dispatch on the tag word); runs converted with `run`. |
| `card` | `0x100273568` | **partly** | Reads id/game/state at `+0x10`, `+0x18` and the vitals vector. |
| `view` | `0x100273ae0` | **partly** | `GameState::game_time_now` + the 16 recovered fields; takes the pieces pre-extracted. |
| `banners` | `0x100274740` | **partly** | Appends to the outgoing `Vec<BannerOut>`; the timer arithmetic (`Instant::add`) is dropped. |
| `hand` | `0x100273888` | **fully** | The two hand-slot words (`Option<String>` pairs). |
| `picture_out` | `0x10026f7b4` | **fully** | `src/alt/width/height`. |

## 3. Counts

* Stubs in the crate: **49**. Implemented: **49**.
* **Fully** recovered: **26** — `lan_address`; `http::request`;
  `pairing::{matches, pairing}`; `workspace::{path, answered}`;
  `phone::{file, keep, kept}`; `notes::read`; `highlights::{sounds, changed,
  listed}`; `client::{message, refused_here, push_asked, phone, phone_put,
  highlights_text, highlights_merge, workspace_of, game_said, press, button,
  TrainerPanel::open}`; `wire::run`.
* **Partly** recovered: **22** — `bonjour::Announced::start`; `tasks::tasks`;
  `trainer::trainer`; `injuries::doll`; `inventory::inventory`;
  `highlights::{Form::of, Form::worn, save, merge}`; `follow::Follow::take`;
  `client::{attach, wheel, link, inventory_act, trainer_asked, look, panel,
  wear}`; `wire::{story_line, card, view, banners}`.
* **Unknown**: **1** — `one_connections`.
* Non-stub helpers that are fully recovered and tested: `http::{accept_key,
  base64, sha1, switching, respond, read_head}`, `pairing::{is_token,
  fresh_token}`, `workspace::revision`, `highlights::{parse, render}`,
  `highlights::MarkForm::mark`, `client::wheel_command`,
  `bonjour::Drop for Announced`, `wire::{hand, picture_out}`.

## 4. Tests

90 unit tests (`cargo test -p hydra_web --offline`), all asserting recovered
values:
the RFC 6455 accept key and the SHA-1/base64 vectors, the exact `101`/`200`
response templates, the `\r\n`/`:`/`' '` header parsing and the
`!upgrade && key.is_none() ⇒ None` rule, the `"t"`-tagged `Out`/`In` documents
(all 37 `In` tags round-trip), the 33 `Form` fields and their order, the 64-hex
pairing token (create → reuse → corrupt → replace), the constant-time compare,
`web/<ACCOUNT>_<character>.workspace.json`, the workspace endpoint's status table
(200/400/404/412/413/500), `phone/<kind>.json` (and the three legal kinds), the
`highlights.toml` mtime and sorted `sounds/` names, the 14-part doll with
`aspect = 0.75`, the notes round-trip through `hydra_session`, the
`shape kind`-tagged JSON, the recovered refusal sentences and their lengths.

Phase 1 added the assertions that the reconnected crates provide: every wire leaf
and every payload leaf serializes with `serde_json` to exactly the JSON its
`fields()` recovered order describes (and round-trips through the derived
`Deserialize`), all 26 serializable `Out` variants are checked against their tag
plus field list, all 37 `In` tags parse → serialize → parse, the
`highlight_save` form is spliced flat next to the tag, the `101` bytes are
accepted by `tokio-tungstenite`'s client (which verifies the accept key), the
handshake works over a local TCP pair, the async head reader waits for the blank
line, the relay pumps `In` → handler → `Out` over tokio channels and over a real
socket, and the mDNS record is the recovered `ServiceInfo`.

## 5. Departures left after Phase 1 (and other seams)

Struck in Phase 1 (now the real crate): ~~serde/serde_json~~, ~~tokio~~,
~~tungstenite's handshake and message layer~~, ~~base64~~, ~~mdns-sd~~.

1. **rustls** → the original's websocket was plain `ws://` on the LAN; `rustls`
   is only needed for `wss://` and is not a workspace pin, so no TLS is set up
   here (the pin list has `native-tls`, not `rustls`).
2. **getrandom** → `/dev/urandom` with a clock/pid-seeded xorshift fallback in
   `pairing::random_bytes`.  `getrandom` is not a workspace pin.
3. **bytes::Bytes** → `Vec<u8>` (`wire::Out::to_frame`, `client::message`).
   `bytes` is not a workspace pin; `tungstenite` re-exports its own `Bytes` but
   the public functions keep returning `Vec<u8>` so callers are unaffected.
   `client::frame` is the `tungstenite::Message` the socket actually writes.
4. **futures-util** → not a workspace pin, so `tokio_tungstenite::
   WebSocketStream` (whose `Stream`/`Sink` impls come from `futures-util`) cannot
   be polled here.  `client::serve_socket` therefore drives `tungstenite`'s
   *blocking* socket on a `tokio::task::spawn_blocking` task (a `SO_RCVTIMEO`
   tick drains the outgoing channel between reads) and joins the two directions
   with tokio channels; `client::relay` is transport-agnostic over those channels
   and `http::accept_websocket` still hands a stream to `tokio-tungstenite` for
   the handshake.  When `futures-util` is pinned, `serve_socket` becomes a
   `WebSocketStream` loop with no other change.
5. **axum** → the original linked it, but nothing in this crate used it (the HTTP
   side is the hand-rolled head parser plus two format strings); it is not a
   workspace pin and nothing was re-implemented for it.
6. **sha1** → `http::sha1` (RFC 3174) stays: no SHA-1/SHA-2 crate is pinned (the
   original's came from `tungstenite`, which still provides the accept key —
   `http::accept_key` is `tungstenite::handshake::derive_accept_key`).  A test
   asserts the hand-rolled digest and `tungstenite`'s agree.
7. **hydra_ui / hydra_board** (unrecovered siblings) → `client::Seat` (symbol,
   lich_symbol, send, name, game), `highlights::Rule`/`HighlightFile`,
   `inventory::Act`, `follow::Body`, `wire::TextRun`.  Every one is documented at
   its definition.  `hydra_session` and `hydra_model` *are* recovered and are
   used directly (`notes::{load, write, rewrite, remove, stamp}`,
   `store::save_bytes`, `state::inventory_snapshot::InventorySnapshot`).
   `client::asked` answers only the messages whose handlers exist here; the ones
   that need those crates (`login`, `characters`, `launch`, `launch_team`,
   `choose`, `reconnect`, `attach`, `streams`, `stream`, `theme`, `tasks`,
   `inventory`, `inventory_act`, `trainer`, `trainer_profile`, `hotbar`,
   `note_new`, `clip`, `remove`) are accepted and answered with nothing.
8. **`Page`'s remaining driver pieces** (`client::{sent, closed}`,
   `Page::{request, logging_in, highlight}`, `serve`, `accept`, `serve_to`,
   `listen`) are not modelled: `client::relay` + `client::asked` are the loop and
   the dispatch, and the socket ends are `http::handshake`/
   `http::accept_websocket` (the two handshakes) and `client::serve_socket`.
9. **`toml`** is a workspace pin but the highlight document syntax is still
   `// UNKNOWN:` (§2.8), so `highlights::{parse, render}` keep the documented
   round-trippable line form rather than guessing a TOML schema.
10. **Files touched**: only `crates/hydra_web/**` plus the workspace `Cargo.lock`
    (adding the crate dependencies updates it).

## 6. Conflicts and open questions

* `one_connections` cannot be read: the 2-bit tag match mixes a pointer (cases
  0/1 dereference) with a scalar (cases 2/3 shift by 32), which does not
  correspond to any layout reachable from its only caller, and the accept-state
  machine it lives in is a 4 KB async `poll`.  Implemented as "keep the
  connection" and documented.
* The `Out` serializer emits some payloads through the same epilogue
  (`said`/`refused` share the field-write block); the key was read from the
  register (`0x10207132b` = `text`), so `Out::Said` carries `text`, not `why`.
* Two `Serialize` impls exist for `notes::NotesOut`, `tasks::TasksOut` and
  `injuries::DollOut`.  For `NotesOut` the keys differ (`notes` vs `problem`),
  which is read as an untagged enum; for the other two the keys are identical and
  the duplicate is taken as a codegen artefact (one impl takes `&T`, the other a
  wrapped reference).
* `wire::Line`, `wire::Card` and `wire::View` mix `String`, `&'static str` and
  enum payloads at offsets that are only partly readable; the recovered *keys*
  are exact, a few field *types* are inferred and marked.

## 7. Phase 1 reconnection

### 7.1 Dependencies actually used

All at the root `[workspace.dependencies]` pins, taken as `{ workspace = true }`
in `crates/hydra_web/Cargo.toml`; `cargo --offline` only, nothing else was added
(the two path dependencies `hydra_model`/`hydra_session` predate this phase).

| crate | pin | what it backs now |
|---|---|---|
| `serde` | `=1.0.229` (derive) | every wire type derives `Serialize`/`Deserialize` |
| `serde_json` | `=1.0.151` | `wire::Out::to_json`, `wire::In::from_json`, `json::{parse, to_json, write_str}`, `phone::keep`'s validation, `workspace::revision` |
| `tokio` | `=1.53.1` (full) | `http::read_head_async`, `client::{frames, relay, serve_socket}` (`mpsc`, `spawn_blocking`); the async tests |
| `tungstenite` | `=0.29.0` | `http::accept_key` (`handshake::derive_accept_key`), `client::{frame, told}`, the blocking socket in `client::serve_socket`, the TCP-pair tests' client |
| `tokio-tungstenite` | `=0.29.0` | `http::accept_websocket` (`accept_async`) and the handshake tests (`client_async`) |
| `base64` | `=0.22.1` | `http::base64` (standard padded alphabet) |
| `mdns-sd` | `=0.21.5` | `bonjour::{service_info, Mdns}` — the `service_info` record and `Daemon` implementation, reached by `Announced::start_mdns` |

### 7.2 What is now real

* **The JSON.** `wire::Out` is `#[serde(tag = "t", rename_all = "snake_case")]`
  over struct/newtype variants, `wire::In` is the mirror with `Deserialize`,
  `injuries::ShapeOut` is `tag = "kind"`, `notes::NotesOut` is the untagged pair
  of objects, `highlights::Form` is a 33-field struct with `#[serde(default)]`.
  `crate::json` is no longer a parser or an escaping writer: `parse` is
  `serde_json::from_str` and `write_str`/`to_json` go through
  `serde_json`'s writer, so the escaping and the number formatting are the
  original's.  `json::Value` survives only as the order-preserving shape the
  recovered `fields()` lists build (its `Serialize`/`Deserialize` impls keep
  document order, which `serde_json::Map` would sort away without
  `preserve_order`).
* **The HTTP/WebSocket handshake.** `http::accept_key` is `tungstenite`'s
  RFC 6455 derivation; `http::read_head_async` reads the head off a tokio stream;
  `http::handshake` parses it with the recovered `request` and answers with the
  recovered `101` bytes; `http::accept_websocket` hands the stream to
  `tokio-tungstenite`.  Tests run a real `tokio-tungstenite` client against both
  handshake paths (duplex and a local TCP pair); the client verifies
  `Sec-WebSocket-Accept`, so a wrong key or header name fails the test.
* **The relay.** `client::relay` is the recovered loop over tokio channels
  (text frame → `wire::In` → handler → `wire::Out` → text frame) and
  `client::asked` is the `Page::asked` dispatch; `client::serve_socket` joins a
  real `tungstenite` socket to it.  Tested over tokio channels, through the
  `Page` handlers, and end-to-end over `127.0.0.1` with a `tungstenite` client.
* **mDNS.** `bonjour::Mdns` is a real `mdns_sd::ServiceDaemon`;
  `bonjour::service_info` builds the record (`<instance>.v1_hydra._tcp.local.`
  with the `web.token` / `web.storage_version` / `layout` TXT entries) and is
  tested without opening a socket.  `Announced::start` still uses the recording
  daemon, `Drop` still unregisters then shuts down, and only
  `Announced::start_mdns` touches the network.
* **base64.** The accept key's encoding is the `base64` crate.

### 7.3 Behaviour the reconnection pinned down (and small API notes)

* `notes::NotesOut` keeps its `Notes(Vec<NoteOut>)` / `Problem(String)` shape but
  serializes as `{"notes":[…]}` / `{"problem":"…"}`.  A plain `#[serde(untagged)]`
  over those newtype variants would have produced a bare array, so the impls go
  through a private mirror enum; `fields()` and the public variants are
  unchanged.
* `highlights::Form`'s fields are declared in the **serialization** order now
  (serde writes declaration order).  The original's memory offsets are kept as
  comments; construction is by field name, so no caller changes.
* `wire::Card` holds two `&'static str` state slots, so it — and therefore
  `Out::Hub` — is serialize-only; `Out` as a whole derives `Serialize` (it is the
  server→page direction) and `In` derives both.  Round-trip tests cover every
  leaf except `Card`.
* `wire::In::from_value` (the `json::Value` entry point) now re-serializes and
  calls `In::from_json`, so there is one parser; `highlights::Form::from_value`
  does the same.  `In::HighlightSave` is `#[serde(flatten)]`, because the page
  splices the 33 form fields next to the `"t"` tag (that is what the recovered
  handler reads).
* Every `In` payload field carries `#[serde(default)]`: the page's messages are
  sparse, and the existing test parses `{"t":tag}` for all 37 tags.
* `workspace::revision` deserializes a one-field `Revision` document instead of
  walking a `json::Value`; unknown fields (the page's own layout) are ignored and
  a negative/non-numeric `revision` is still `None`.
* `highlights::HighlightFile` gained `remove`/`switch`, the two operations
  `In::HighlightRemove`/`In::HighlightSwitch` need; `client::asked` uses them.
* The synchronous functions and types all keep their signatures
  (`http::{request, read_head, switching, respond, base64, sha1, accept_key}`,
  `client::{message, Page, Seat}`, every `fields()`): `message` still returns
  `Vec<u8>`, `client::frame` is the `tungstenite::Message` form alongside it.
