# `hydra_protocol` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(demangled *global* symbols in `hydra-inspect/symbols_demangled.txt`; the
crate-local functions below come from `nm -a`, which is the only complete
source — `symbols_demangled.txt` lists global symbols only and its "next symbol"
extents therefore overrun into neighbouring functions; several sizes previously
recorded in `.work/RECON.md` were inflated by exactly that, see
[Corrections](#corrections-to-workreconmd)).

The crate is the original workspace package `hydra-protocol` (hyphenated: the
panic paths in `__cstring` read `crates/hydra-protocol/src/...`), a **human-made
crate** of the Hydra workspace, not a vendored dependency. It is the wire-format
layer: a byte-streaming parser for the GemStone IV / Lich XML-ish protocol, a
small entity/attribute/string toolkit, the `Runs` styled-text model, and the
serialisers that write the same format back out.

Everything is `std`-only — and unlike most crates in this workspace that is not
a departure: see [Departures](#departures-what-the-original-used-vs-this-reconstruction).

---

## Legend

| tag | meaning |
|---|---|
| **established** | read out of the instruction stream / literal pool; the value or control flow is not in doubt. |
| **INFERRED** | constrained by evidence (struct offsets, sizes, literal strings) but a source-level detail (field name/type) had to be chosen. |
| **UNKNOWN** | the binary does not pin this down. A safe default was chosen and is marked in the source. |
| ✅ / ◐ / — | in the status column: fully reimplemented / partially / not reimplemented. |

Tooling used: `.work/recon.py` (`syms`, `sizes`, `disas`, `bytes`, `findstr`)
driving `objdump`; `nm -a` for the complete local symbol table; `otool -Iv` to
resolve the `__stubs` calls (`memcmp`/`memcpy`/`memmove`/`memset`).

---

## Module layout: original vs this reconstruction

The original source files are recoverable from `__cstring` panic paths
(`grep crates/hydra-protocol`), and the module paths from the mangled symbols:

| original file | original modules (from symbols) | here |
|---|---|---|
| `src/parser.rs` | `parser::Parser::parse_line`, `parser::is_paired` | `parser.rs` ◐, `parser::is_paired` ✅ (in `parser/markup.rs`, re-exported) |
| `src/parser/read.rs` | `parser::read::{push_bytes, finish}` | `parser/read.rs` ✅ |
| `src/parser/emit.rs` | `parser::emit::{text_frame, parse_runs_reporting, flush, push_run}` | folded into `parser.rs` ◐ |
| `src/parser/dispatch.rs` | `parser::dispatch::{dispatch, prompt, inline_paired, structural_only, open_stream, pop_stream, clear_stream, eg_merchant, objectives, objective, menu, menu_item, cmdlist, inline_image, progress_bar, unmodelled_after}` | — (tag dispatch reduced to a `match` in `parse_line`) |
| `src/parser/inner.rs` | `parser::inner::{children, body_tags, open_tag, unread, inner_text, inner_display_text, other_children, inside_each}` | — |
| `src/parser/view_item.rs` | `parser::view_item::{ViewItem, open_view_item, view_item_text, finish_view_item}`, `ViewItem::{append_text, close_section, push_link}` | — |
| `src/parser/inventory.rs` | `parser::inventory::{inventory_manager, num}` | `parser/inventory.rs`: `num` ✅, `inventory_manager` — |
| `src/parser/thin.rs` | `parser::thin::{thin_frame, room_meta, script_timer, stray_objective}` | — |
| `src/parser/markup.rs` | `parser::markup::{markup_state, is_markup}` | `parser/markup.rs`: `is_markup` ✅, `is_paired` ✅ (`markup_state` —) |
| `src/parser/wire.rs` | `parser::wire::{decode_wire_line, client_region}` | `parser/wire.rs`: `client_region` ✅; `decode_wire_line` still in `parser.rs` ✅ |
| `src/parser/compass.rs` | `parser::compass::directions` | `parser/compass.rs` ✅ |
| `src/text.rs` | `text::{tag_name, attribute, attribute_u32, link_from_tag, find_tag_start, is_prose, pairs}` | `text.rs` ✅ (`pairs` → `text::attributes`) |
| `src/text/entities.rs` | `text::entities::{decode_entities, strip_control_chars, decode_attribute_value}` | `text/entities.rs` ✅ |
| `src/numbers.rs` | `numbers::{parse_amount, parse_duration_secs, signed}` | `numbers.rs` ✅ |
| `src/runs.rs` | `runs::Run::object`, `runs::Runs::{plain, bold_fragments, is_empty}` | `runs.rs` ✅ |
| `src/frame.rs` | `frame::vocabulary::Frame`, `frame::methods::<impl Frame>::structural` | `frame.rs` ◐ — `Frame` lives at `frame::Frame`; `structural` ✅ as `Frame::structural` |
| `src/frame/payload.rs` | `frame::payload::*` (see below) | `frame/payload.rs` ◐ |
| `src/tags.rs` | `tags::is_known` | `tags.rs` ✅ |
| `src/write.rs` | `write::{escape, escape_attribute, empty, element, runs, attributes, open, same}`, `write::Writing::close_links` | `write.rs` ✅, except `open` ◐ (`open_link`) and `close_links` ◐ (`close_link`); `same` ✅ |

`crates/hydra-protocol/src/parser/dispatch.rs` existing in the original confirms
the 6.3 KB `parse_line` is a *front door*: it scans a line and hands tags to a
`dispatch` table, with `emit` owning the run/flush machinery.

### Type inventory (from `nm -a`)

`nm -a` names every monomorphisation over this crate's types, so the *payload
vocabulary* is fully recoverable even though no body is:

| type | evidence in `nm -a` | modelled here |
|---|---|---|
| `frame::vocabulary::Frame` | `Debug`, `Clone`, `drop_in_place<Frame>` (25), `Option<Box<Frame>>` (17+2), `Vec<Frame>` (5), `Chain<IntoIter<Frame>, IntoIter<Frame>>` | yes (`frame::Frame`), 9 of its ≥59 tags |
| `frame::payload::Link` | `Clone` (21), `PartialEq::eq` (3), `drop_in_place` (32), `Option<Link>` (40) | yes |
| `frame::payload::LinkKind` | `Debug` (2), `drop_in_place` (25) — **variant names recovered**, see below | yes, as `LinkTarget` |
| `frame::payload::Style` | `Debug` (3) — a 3-field struct with field names `bold`/`depth`/`preset` in its pool | **no** |
| `frame::payload::MerchantStatus` | `Debug` (2), `drop_in_place` (12) | yes |
| `frame::payload::TextFrame` | `Clone` (9), `drop_in_place` (33), stride `0x60` | yes |
| `frame::payload::RoomMeta` | `drop_in_place` (24), `Option<RoomMeta>` (39) | **no** |
| `frame::payload::ItemDetail` | `drop_in_place` (15), `Vec<ItemDetail>` (13), `BTreeMap<String, Vec<ItemDetail>>` | **no** |
| `frame::payload::ItemView` | `Clone` (9), `drop_in_place` (33) | **no** |
| `frame::payload::InventoryItem` | `Clone` (2), `drop_in_place` (16), `Vec<InventoryItem>` (19), `BTreeMap<String, InventoryItem>` | yes (partial) |
| `frame::payload::InventoryResponse` | `Clone` (9), `drop_in_place` (33) | **no** |
| `frame::payload::EbonGateMerchant` | `Clone` (9), `drop_in_place` (33) | **no** |
| `frame::payload::Menu` | `Clone` (10), `drop_in_place` (35), `Option<(u64, Menu)>` | **no** |
| `frame::payload::MenuItem` | `drop_in_place` (13), `Vec<MenuItem>` (12) | **no** |
| `frame::payload::DialogWidgets` | `Clone` (10), `drop_in_place` (33) | **no** |
| `frame::payload::ProgressBar` | `Clone` (10), `drop_in_place` (33) | **no** |
| `frame::payload::InlineImage` | `Clone` (9), `drop_in_place` (34) | **no** |
| `frame::payload::ActiveEffect` | `Clone` (10), `drop_in_place` (33) | **no** |
| `frame::payload::Continuation` | `drop_in_place` (7), `Vec<Continuation>` (26) | **no** |
| `frame::payload::CmdListUpdate` | `drop_in_place` (32) | **no** |
| `frame::payload::CmdListEntry` | `drop_in_place` (10), `Vec<CmdListEntry>` (10) | **no** |
| `frame::payload::Objective` | `Clone` (1), `drop_in_place` (16), `Vec<Objective>` (33), `BTreeMap<String, Objective>` (12) | **no** |
| `frame::payload::ObjectivesAction` | `Debug` (1) — **variants recovered**: `FullRefresh`, `Patch`, `Delete`, `DeleteGroup`, `Other(_)` | **no** |
| `frame::payload::ObjectiveAction` | `drop_in_place` (1), `Vec<ObjectiveAction>` (28) | **no** |
| `frame::payload::ObjectiveReward` | `drop_in_place` (1), `Vec<ObjectiveReward>` (14) | **no** |
| `parser::view_item::ViewItem` | `Option<ViewItem>` (13) | **no** |
| `write::Writing` | `drop_in_place<Writing>` (1) — `{ String, Option<Link>, Option<Link> }`, `0xf0` B | **no** (see below) |
| `runs::Run`, `runs::Runs` | `drop_in_place<Run>` (16), `drop_in_place<Runs>` (57), `VecDeque<Runs>`, `BTreeMap<String, Runs>`, `Ring<Runs>` | yes |

`ObjectivesAction`'s variant names are literal bytes in its `Debug` impl
(`0x1002145f0`): `FullRefresh` (11 B at `0x10206f424`), `Patch` (5 B at
`0x10206f42f`), `Delete` (6 B at `0x10206f434`), `DeleteGroup` (11 B at
`0x10206f43a`) and the tuple variant `Other(_)` (5 B at `0x10206f445`).

Here only `Link`, `LinkTarget` (the original name is **`LinkKind`**),
`MerchantStatus`, `Capacity`, `InventoryItem` and `TextFrame` exist. The other
payload types are not modelled; the parser keeps their raw text in
`Frame::Unknown`. Sibling crates consume only `Link`/`LinkTarget`/`Run`/`Runs`/
`Frame`/`Parser`, so nothing in the public API was removed.

### `LinkKind` — variant names and layout recovered

`<LinkKind as Debug>::fmt` (`0x1001c62f8` and ICF copy `0x10044a5d8`) is a
five-way switch on a niche decoded out of the first word:

```text
idx = if word >= 0 { 0 } else { (word - (2^63 - 1)) mod 2^64 }   // and …, asr #63
```

| idx | raw word | variant | fields | `write::open` tag | `Debug` name site |
|---|---|---|---|---|---|
| 0 | any non-negative value (it is `exist`'s capacity) | `Exist` | 2 strings at `+0x00` / `+0x18` (`exist`, `noun`) | `<a exist=… noun=…>` | `0x10206e532` |
| 1 | `0x8000_0000_0000_0000` | `Direct` | 1 string at `+0x08` (`cmd`) | `<d cmd=…>` | `0x10206e537` |
| 2 | `…0000_0001` | `DirectText` | none | `<d>` | `0x10206e540` |
| 3 | `…0000_0002` | `Url` | 1 string at `+0x08` (`href`) | `<a href=…>` | `0x10206e54a` |
| ≥4 | `…0000_0003` | `NotActionable` | none | `<a>` | `0x10206e54d` |

The struct is exactly `0x30` bytes: `Exist`'s two strings need `0x30`, the
one-string variants need `0x20` and put their discriminator in the first word.

This is the *strongest* new evidence in this pass — it is what makes
`write::open`, `write::same` and `Writing::close_links` readable, and it pins
the `Run`/`Link`/`TextFrame` layouts below.

### `Frame` — the tag word is a niche, and 59 tags exist

Every `Frame` construction in the crate stores its tag as
`0x8000_0000_0000_00XX` (`mov xN, #0xXX` + `movk xN, #0x8000, lsl #48`).
Scanning every such pair inside `0x101c194c0..0x101c3a070` gives this map from
producing function to tag:

| tag | producing function(s) | sites |
|---|---|---|
| `0x02` | `link_from_tag`, `close_links`, `Link::clone`, `command::parse` | 4 |
| `0x09` | `dispatch::progress_bar` | 1 |
| `0x0c` | `dispatch::inline_paired` | 2 |
| `0x0d` | `dispatch::open_stream` | 1 |
| `0x10` | `dispatch::pop_stream` | 1 |
| `0x11` | `dispatch::prompt` | 1 |
| `0x13` | `dispatch::clear_stream` | 1 |
| `0x14` | `dispatch::dispatch` | 11 |
| `0x18` | `thin::thin_frame` | 31 |
| `0x22` | `dispatch::objectives` (2), `thin::stray_objective` (1) | 3 |
| `0x24` | `dispatch::cmdlist` | 1 |
| `0x2d` | `inventory::inventory_manager` | 1 |
| `0x2e` | `view_item::finish_view_item` | 1 |
| `0x32` | `dispatch::eg_merchant` | 1 |
| `0x35` | `wire::client_region` (the `<c>`-text variant, `0x3a - 5`) | 1 |
| `0x37` | `dispatch::unmodelled_after` | 1 |
| `0x38` | `frame::methods::<Frame>::structural` | 1 |
| `0x39` | `parse_line` (5), `read::push_bytes` (1), `thin::script_timer` (2) | 8 |
| `0x3a` | `wire::client_region` (the empty variant) | 1 |

So `Frame` has **at least 59 tags** (the highest is `0x3a`), and one of its
variants carries a `Box<Frame>` / a niche-bearing field, which is why the
discriminant is stored that way rather than as a plain `u8`. That explains the
`Option<Box<Frame>>` (17+2 instantiations) and `Vec<Frame>` glue in `nm -a`.

This reconstruction models 9 tags: `Text`, `Prompt`, `Echo`, `Raw` (`0x39`),
`MerchantStatus`, `Unknown`, `Structural` (`0x38`), `ClientText` (`0x35`) and
`ClientRegion` (`0x3a`). The rest stay `Frame::Unknown`.

---

## Coverage table (all 81 crate-local symbols, `nm -a`)

**Method.** Two sources, unioned:

1. `symbols_demangled.txt` — 27 global (`T`) symbols for this crate.
2. `nm -a | rustfilt` — **all** 1246 symbol lines naming `hydra_protocol`, which
   collapse to 171 distinct demangled names: 80 distinct crate-local functions
   (81 symbols — `inventory::num` has two monomorphisations), 91
   monomorphisations of `core`/`alloc` machinery over this crate's types
   (`drop_in_place<Frame>` 25×, `<Link as Clone>::clone` 21×,
   `drop_in_place<Menu>` 35×, `<Style as Debug>::fmt`, …) — and, crucially, the
   **payload type vocabulary**, which only the monomorphisations carry (see
   [Type inventory](#type-inventory-from-nm--a)).

Sizes are the exact extents from `nm -a` (distance to the next symbol of *any*
kind), not the global-only file.

Statuses: **implemented** = the recovered behaviour is in this crate;
**partial** = some of it is; **unknown** = the body is not decoded;
**missing** = decoded here but not implemented (with the reason).

Count: **36 implemented, 13 partial, 30 unknown, 1 missing**.

| # | symbol | addr | size | status | evidence / recovered behaviour |
|---|---|---|---|---|---|
| 1 | `hydra_protocol::parser::emit::<impl hydra_protocol::parser::Parser>::text_frame` | `0x101c194c0` | 828 | partial | decodes entities + strips control chars of the input, then appends the text to **every** open stream frame (`Vec<TextFrame>` stride `0x60` at `Parser+0x50`) with a 1024-byte (`0x400`) per-frame budget, cloning the preset at `Parser+0x20` and the link vector at `Parser+0x38`, and reading the style `u16` at `+0x164` / the flag `u8` at `+0x166`. The per-stream append loop and the `0x400` budget are newly established; the frame it finally returns is not fully pinned. |
| 2 | `hydra_protocol::parser::emit::<impl hydra_protocol::parser::Parser>::parse_runs_reporting` | `0x101c197fc` | 1476 | unknown | 1476 B; calls `markup::is_markup` (`0x101c19a24`). Style/link transition tracking was not decoded; `parse_line` does the equivalent work inline here. |
| 3 | `hydra_protocol::parser::emit::<impl hydra_protocol::parser::Parser>::flush` | `0x101c19dc0` | 308 | missing | 308 B, fully decoded: `fn flush(&mut self, line: &mut String, out: &mut Vec<Frame>)` — return when `line` is empty; when `Parser+0x78` is `Some` **or** the byte at `Parser+0x167` is set, `trim_matches` the line and return when it is blank; otherwise `mem::take` the line, push `text_frame(..)` into `out` and free the old buffer. Not implemented because this reconstruction has no line buffer. |
| 4 | `hydra_protocol::parser::emit::<impl hydra_protocol::parser::Parser>::push_run` | `0x101c19ef4` | 916 | partial | 916 B; builds a `Run` from text + current style/preset/link state (matches `Run`'s `+0x30` style `u16`, `+0x32` flag, `+0x18` preset, `+0x38` link). `Parser::push_run` here builds the same shape. |
| 5 | `hydra_protocol::parser::read::<impl hydra_protocol::parser::Parser>::push_bytes` | `0x101c1a288` | 1028 | implemented | the byte state machine, recovered exactly (see "Parser layout" below). |
| 6 | `hydra_protocol::parser::read::<impl hydra_protocol::parser::Parser>::finish` | `0x101c1a68c` | 212 | implemented | flush the last line, stripping one leading `\r`. |
| 7 | `hydra_protocol::parser::Parser::parse_line` | `0x101c1a760` | 6352 | partial | 6352 B front door. Implemented for the common tags; the `dispatch` table it calls is not reconstructed. `wire::client_region` is called from this function (`0x101c1b614`) in a branch this build does not take. |
| 8 | `hydra_protocol::parser::markup::<impl hydra_protocol::parser::Parser>::markup_state` | `0x101c1c030` | 1208 | unknown | 1208 B; open-tag stack bookkeeping. |
| 9 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::pop_stream` | `0x101c1c4e8` | 612 | unknown | 612 B; produces frame tag `0x10`. |
| 10 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::eg_merchant` | `0x101c1c74c` | 1652 | unknown | 1652 B; produces frame tag `0x32` (the `EbonGateMerchant` variant). |
| 11 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::open_stream` | `0x101c1cdc0` | 316 | unknown | 316 B; produces frame tag `0x0d`. |
| 12 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::clear_stream` | `0x101c1cefc` | 304 | unknown | 304 B; produces frame tag `0x13`. |
| 13 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::inline_paired` | `0x101c1d02c` | 816 | unknown | 816 B; handles `<b>`, `<i>`, `<d>`, `<a>`, `<comp>`, …; produces frame tag `0x0c`. |
| 14 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::structural_only` | `0x101c1d35c` | 184 | partial | 184 B, fully decoded shape: `fn structural_only(&mut self, first: &str, second: &str, line: &mut String, out: &mut Vec<Frame>)` = `self.flush(line, out); out.push(Frame::structural(first, second))` (`bl flush`, `bl Frame::structural`, then a `Vec<Frame>` push with stride `0x118`). Only its `Frame::structural` half is reconstructed (as `Frame::structural`); the flush half needs the line buffer this crate does not have. It has **no** out-of-line callers — it is inlined into `dispatch`. |
| 15 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::prompt` | `0x101c1d414` | 996 | partial | 996 B; produces frame tag `0x11`. `<prompt>` is handled inline in `parse_line` here. |
| 16 | `hydra_protocol::parser::dispatch::<impl hydra_protocol::parser::Parser>::dispatch` | `0x101c1d7f8` | 6656 | unknown | 6656 B tag-dispatch table; produces frame tag `0x14` from 11 sites. Attribute names it reads: `id class status expires clear picture title rm action objective group icon continuation clim command malformed exist state closed truncated subtitleText item mana loc weight encum min_max on_max in_selector locker family vault flags coord cmd ltid` (pool at `0x103bf898e`). No per-tag behaviour map is readable without decoding all 6.6 KB. |
| 17 | `hydra_protocol::parser::is_paired` | `0x101c1f1f8` | 572 | implemented | 572 B; length-dispatched compare chain, table recovered name-for-name. Implemented in `parser::markup::is_paired`, re-exported as `parser::is_paired`. |
| 18 | `hydra_protocol::parser::view_item::<impl hydra_protocol::parser::Parser>::open_view_item` | `0x101c1f434` | 1252 | unknown | 1252 B; `<inv>` detail view; `Option<ViewItem>` at `Parser`. |
| 19 | `hydra_protocol::parser::view_item::<impl hydra_protocol::parser::Parser>::view_item_text` | `0x101c1f918` | 204 | unknown | 204 B. |
| 20 | `hydra_protocol::parser::view_item::<impl hydra_protocol::parser::Parser>::finish_view_item` | `0x101c1f9e4` | 800 | unknown | 800 B; produces frame tag `0x2e`. |
| 21 | `hydra_protocol::text::attribute_u32` | `0x101c259f0` | 644 | implemented | `fn attribute_u32(&str, &str) -> Option<u32>`. |
| 22 | `hydra_protocol::text::link_from_tag` | `0x101c25c74` | 572 | implemented | `fn link_from_tag(&str) -> Option<Link>` — `href` > `exist`+`noun` > `cmd`, `coord` on every variant. |
| 23 | `hydra_protocol::text::find_tag_start` | `0x101c25eb0` | 836 | implemented | `fn find_tag_start(&str) -> Option<usize>`. |
| 24 | `hydra_protocol::text::pairs` | `0x101c261f4` | 832 | implemented | `text::attributes`. |
| 25 | `hydra_protocol::text::is_prose` | `0x101c26534` | 1020 | implemented | `fn is_prose(&str) -> bool`. |
| 26 | `hydra_protocol::text::tag_name` | `0x101c26930` | 332 | implemented | `fn tag_name(&str) -> &str`. |
| 27 | `hydra_protocol::text::attribute` | `0x101c26a7c` | 1364 | implemented | `fn attribute(&str, &str) -> Option<String>`. |
| 28 | `hydra_protocol::text::entities::decode_entities` | `0x101c27354` | 2420 | implemented | `fn decode_entities(&str) -> String`. |
| 29 | `hydra_protocol::text::entities::strip_control_chars` | `0x101c27cc8` | 340 | implemented | `fn strip_control_chars(&str) -> String`. |
| 30 | `hydra_protocol::text::entities::decode_attribute_value` | `0x101c27e1c` | 348 | implemented | decode + strip. |
| 31 | `hydra_protocol::parser::inventory::inventory_manager` | `0x101c27f78` | 7872 | unknown | 7872 B; produces frame tag `0x2d`; fills a `BTreeMap<String, InventoryItem>`. Not reconstructed. |
| 32 | `hydra_protocol::parser::inventory::num` | `0x101c29e38` | 644 | implemented | two monomorphisations (644 B / 692 B) of the same reader; recovered and implemented as `parser::inventory::num(text, name) -> Option<u32>`. |
| 33 | `hydra_protocol::parser::thin::thin_frame` | `0x101c2a6f4` | 8508 | unknown | 8508 B; produces frame tag `0x18` from 31 sites — the largest single body in the crate, and the only loader of the `phy_left`/`mnt_left`/`phy_conv`/`mnt_conv` literals. No behaviour map is defensible from 8.5 KB of straight-line reducer in one pass. |
| 34 | `hydra_protocol::parser::thin::script_timer` | `0x101c2c830` | 528 | unknown | 528 B; produces frame tag `0x39` from 2 sites. |
| 35 | `hydra_protocol::parser::thin::stray_objective` | `0x101c2ca40` | 180 | partial | 180 B, fully decoded: builds one `Objective` (288 B, via `dispatch::objective`), boxes it, and returns frame tag `0x22` with a `Vec` (cap 1, ptr, len 1) at frame `+0x20` — an objectives frame holding a single stray objective. The `Objective` payload is not modelled here. |
| 36 | `hydra_protocol::parser::thin::room_meta` | `0x101c2caf4` | 5048 | unknown | 5048 B; fills `Option<RoomMeta>` (39 `Option<RoomMeta>` drop instantiations in the crate). |
| 37 | `hydra_protocol::numbers::parse_amount` | `0x101c2dfc0` | 644 | implemented | `fn parse_amount(&str) -> Option<(i32, i32)>`. |
| 38 | `hydra_protocol::numbers::parse_duration_secs` | `0x101c2e244` | 588 | implemented | `u32` accumulate with checked overflow. |
| 39 | `hydra_protocol::numbers::signed` | `0x101c2e490` | 404 | implemented | private `fn signed(&str) -> Option<i32>`. |
| 40 | `hydra_protocol::runs::Run::object` | `0x101c2f580` | 144 | implemented | `fn object(&self) -> Option<&Link>` — `+0x98` slot before `+0x38`. |
| 41 | `hydra_protocol::runs::Runs::bold_fragments` | `0x101c2f610` | 20 | implemented | `fn bold_fragments(&self) -> Vec<String>`. |
| 42 | `hydra_protocol::runs::Runs::plain` | `0x101c2f624` | 232 | implemented | `fn plain(&self) -> String`. |
| 43 | `hydra_protocol::runs::Runs::is_empty` | `0x101c2f70c` | 48 | implemented | `fn is_empty(&self) -> bool`. |
| 44 | `hydra_protocol::frame::methods::<impl hydra_protocol::frame::vocabulary::Frame>::structural` | `0x101c2f73c` | 272 | implemented | 272 B, fully decoded: `fn structural(tag: &str, text: &str) -> Frame`. Body = two `__rust_alloc(len, 1)` + `memcpy` (capacity = length) and one store of frame tag `0x8000_0000_0000_0038`. **Correction:** the earlier note called this "frame carries no visible text"; it is a two-string *constructor*. |
| 45 | `hydra_protocol::frame::payload::InventoryItem::in_capacity` | `0x101c2f84c` | 76 | implemented | `(1,0)` → tag 2; else `(v%10, v/10)`. |
| 46 | `hydra_protocol::frame::payload::MerchantStatus::parse` | `0x101c2f898` | 664 | implemented | `start`/`rules`/`stop`, else owned lowercased. |
| 47 | `hydra_protocol::frame::payload::TextFrame::as_run` | `0x101c2fb30` | 484 | implemented | straight field clone into the `Run`. |
| 48 | `hydra_protocol::parser::inner::inner_text` | `0x101c315a4` | 964 | unknown | 964 B; the closest thing to a parser among the `inner` helpers. Returns a `String` (24-byte sret). |
| 49 | `hydra_protocol::parser::inner::inside_each` | `0x101c31968` | 880 | unknown | 880 B. |
| 50 | `hydra_protocol::parser::inner::other_children` | `0x101c31cd8` | 844 | unknown | 844 B. |
| 51 | `hydra_protocol::parser::inner::inner_display_text` | `0x101c32024` | 1044 | unknown | 1044 B. |
| 52 | `hydra_protocol::parser::inner::unread` | `0x101c32438` | 364 | unknown | 364 B. |
| 53 | `hydra_protocol::parser::inner::children` | `0x101c325a4` | 156 | partial | 156 B: `Vec::from_iter(inner_text(..).split('<'))` — it splits the extracted inner text on `<` (`movi.2s v0, #0x3c`) and collects. The element type is not pinned. |
| 54 | `hydra_protocol::parser::inner::open_tag` | `0x101c32640` | 284 | partial | 284 B: scans forward for the next `>` (`core::slice::memchr::memchr_aligned` for `0x3e`, with an unrolled ≤15-byte head), rejecting any candidate that is not on a `char` boundary (`ldrsb`/`cmn #0x40`), and returns the *rest* `&str` from just past that `>`. It has no `None` return; the `slice_error_fail` at `0x101c32744` is the `&s[..i]` guard. |
| 55 | `hydra_protocol::parser::inner::body_tags` | `0x101c3275c` | 188 | partial | 188 B: `Vec::from_iter(inner_text(..).split(..))` returning 56-byte-stride elements. The 16-byte constant it loads from `0x10206ca40` before the split is not decoded, so the pattern is only partly known. |
| 56 | `hydra_protocol::parser::dispatch::objectives` | `0x101c32e00` | 1288 | unknown | 1288 B; produces frame tag `0x22` from 2 sites. |
| 57 | `hydra_protocol::parser::dispatch::inline_image` | `0x101c33308` | 796 | unknown | 796 B. |
| 58 | `hydra_protocol::parser::dispatch::progress_bar` | `0x101c33624` | 824 | unknown | 824 B; produces frame tag `0x09`. |
| 59 | `hydra_protocol::parser::dispatch::unmodelled_after` | `0x101c3395c` | 840 | unknown | 840 B; produces frame tag `0x37`. |
| 60 | `hydra_protocol::parser::dispatch::menu` | `0x101c33ca4` | 832 | unknown | 832 B; fills `Menu` / `Vec<MenuItem>`. |
| 61 | `hydra_protocol::parser::dispatch::cmdlist` | `0x101c33fe4` | 212 | partial | 212 B: `inner::children(..)` then a second `Vec::from_iter` (24-byte elements) after freeing the first, ending in frame tag `0x24` with a `Vec` at frame `+0x08`. The element type is not pinned. |
| 62 | `hydra_protocol::parser::dispatch::menu_item` | `0x101c340b8` | 344 | unknown | 832 B; fills `Menu` / `Vec<MenuItem>`. |
| 63 | `hydra_protocol::parser::dispatch::objective` | `0x101c34210` | 928 | partial | 928 B: builds one `Objective` (the `0x120` allocation `stray_objective` makes) from a `&str` pair. |
| 64 | `hydra_protocol::parser::view_item::ViewItem::append_text` | `0x101c345b0` | 380 | unknown | 380 B. |
| 65 | `hydra_protocol::parser::view_item::ViewItem::close_section` | `0x101c3472c` | 700 | unknown | 700 B. |
| 66 | `hydra_protocol::parser::view_item::ViewItem::push_link` | `0x101c349e8` | 224 | unknown | 224 B. |
| 67 | `hydra_protocol::tags::is_known` | `0x101c3603c` | 488 | implemented | sorted 128-entry table + branchless binary search. |
| 68 | `hydra_protocol::write::attributes` | `0x101c37a20` | 596 | implemented | private `attributes(&[(&str, &str)]) -> String`. |
| 69 | `hydra_protocol::write::escape_attribute` | `0x101c37c74` | 904 | implemented | `escape` + `&#9;` / `&#10;` / `&#13;`. |
| 70 | `hydra_protocol::write::open` | `0x101c37ffc` | 696 | partial | 696 B, **fully decoded**: builds the opening tag from a per-variant attribute list — `Exist` → `<a exist=… noun=…>`, `Direct` → `<d cmd=…>`, `DirectText` → `<d>`, `Url` → `<a href=…>`, `NotActionable` → `<a>` — appends `coord` when present, then formats `<{tag}{attrs}>`. Here it is `write::open_link`, a private per-variant writer with the same attribute names and tags; the `DirectText`/`NotActionable` variants have no counterpart in the public `LinkTarget`. |
| 71 | `hydra_protocol::write::runs` | `0x101c382b4` | 2556 | implemented | 2556 B; re-emits `<pushBold/>`/`<popBold/>`, `<preset id="…">`/`</preset>`, `<output class=""/>`/`<output class="mono"/>`, links via `open`/`close_links`, and calls `write::same` at `0x101c38564` / `0x101c38630`. Two normalisation differences, see below. |
| 72 | `hydra_protocol::write::same` | `0x101c38cb0` | 328 | implemented | 328 B, fully decoded: `fn same(a: Option<&Link>, b: Option<&Link>) -> bool` comparing `coord` and `target` only — **never** `attrs`. |
| 73 | `hydra_protocol::write::empty` | `0x101c38df8` | 160 | implemented | `<name k="v"/>`. |
| 74 | `hydra_protocol::write::escape` | `0x101c38e98` | 996 | implemented | all five XML entities. |
| 75 | `hydra_protocol::write::Writing::close_links` | `0x101c3927c` | 356 | partial | 356 B, fully decoded: takes and drops the anchor slot (`Writing+0x90`) writing `</a>`, then takes and drops the command slot (`Writing+0x30`) writing `</d>` unless the target is `Url`/`NotActionable` (`cmp x20, #0x8000_0000_0000_0002; csel …, lt`). Exposed as `write::close_link`; this crate's `write::runs` keeps `</a>` for every link (see below). |
| 76 | `hydra_protocol::write::element` | `0x101c393e0` | 172 | unknown | not classified |
| 77 | `hydra_protocol::parser::markup::is_markup` | `0x101c3948c` | 264 | implemented | 264 B, fully decoded: `a`/`b`/`d` (a 4-bit mask test on `b - b'a'`), `style`, `preset`, `output`, `popBold`, `pushBold`. |
| 78 | `hydra_protocol::parser::wire::client_region` | `0x101c39b14` | 528 | implemented | 528 B, fully decoded: `fn client_region(line: &str) -> (Frame, &str)` — require a 15-byte prefix, split `&line[15..]` at `<!-- ENDCLIENT -->`, and return `Frame::ClientText(decode+strip of the head after a `<c>` prefix)` / `Frame::ClientRegion` plus the tail. Frame tags `0x35` and `0x3a`. |
| 79 | `hydra_protocol::parser::wire::decode_wire_line` | `0x101c39d24` | 276 | implemented | `u8` → `char` fallback, not `from_utf8_lossy`. |
| 80 | `hydra_protocol::parser::compass::directions` | `0x101c39e38` | 568 | implemented | 568 B, fully decoded: `fn directions(text: &str) -> Vec<String>` collecting the `value` attribute of every `<dir value…>`, breaking (not skipping) on an unterminated tag. |

The single `missing` row is `emit::flush`: its control flow is fully decoded
but it takes `(&mut String, &mut Vec<Frame>)` — a pending-line buffer this
reconstruction does not have, because `Parser::parse_line` here is
line-at-a-time and returns its frames directly. Implementing it would mean
rebuilding the text-frame pipeline (`emit` + `dispatch` + the `Vec<TextFrame>`
stream ring at `Parser+0x50`), which is the `unknown` bulk of the table.

### How the 30 `unknown` rows break down

| group | functions | bytes | what is missing |
|---|---|---|---|
| `dispatch` | 14 | 17,704 | the per-tag behaviour map; only the attribute-name vocabulary (`0x103bf898e`) and the frame tag each produces are recovered |
| `inner` | 2 (+3 partial) | 2,116 | the inner-tag scanner proper (`inner_text`, `inside_each`, `other_children`, `inner_display_text`, `unread`) |
| `thin` | 3 | 14,084 | `thin_frame` (8.5 KB), `room_meta` (5.0 KB) |
| `view_item` | 6 | 3,560 | `open_view_item`, `view_item_text`, `finish_view_item`, `ViewItem::{append_text, close_section, push_link}` |
| `inventory` | 1 | 7,872 | `inventory_manager` |
| `markup`/`emit` | 2 | 2,684 | `markup_state`, `parse_runs_reporting` |
| `Parser` | 1 | 6,352 | the rest of `parse_line` |

That is 48 KB of the crate's ~72 KB of `.text`, all of it readable in
principle but none of it defensible from a single pass, and all of it
dependent on the 24 payload types that this crate does not model.

---

## Recovered constants and wire facts

### `tags::is_known` — a sorted table, **not** a PHF

`.work/RECON.md` describes this as "a PHF set with 128 entries". That is
**wrong**; the disassembly shows something much simpler:

* The table at `0x1044a7810` is 128 × 16 bytes = 128 `(ptr, len)` pairs
  pointing into the string pool at `0x103bf8ee8` — i.e. a `&[&str; 128]`, sorted
  by `str`'s byte order (verified: dumping the 128 pairs yields exactly the
  sorted names, entry 65 = `"macros"`, 127 = `"vScrollBar"`).
* `is_known` is 488 bytes of straight-line code: a **fully unrolled, branchless
  binary search**. Probe 0 clamps `min(6, len)` and `bcmp`s against the
  midpoint entry `"macros"` (len 6 folded in as a constant); the remaining six
  probes step the index by `or #0x20`, `or #0x10`, `or #0x8`, `or #0x4`,
  `add #0x2`, `add #0x1`, each comparing `min(len)` bytes with `bcmp` and then
  the lengths (`str`'s `Ord`); the result is `cset w0, eq`.
* There is no hash, no displacement table, no `phf_shared::hash` in the
  function, and the crate calls nothing outside `core`/`alloc`/`std`.

So the reconstructed `KNOWN_TAGS.binary_search(&tag)` is not a stand-in for a
hash lookup — it is *the same algorithm* as the original. The table content is
identical (checked entry-for-entry, and pinned by
`tags::tests::table_matches_the_recovered_binary_snapshot`).

### `Parser` layout and the byte-streaming state machine

`push_bytes` (`0x101c1a288`) reads these `Parser` fields (offsets are
**established**):

| offset | field | meaning |
|---|---|---|
| `+0x00`/`+0x08`/`+0x10` | `Vec<u8>` | current line buffer (cap/ptr/len) |
| `+0x60`/`+0x68`/`+0x70` | `Vec<u8>` | rolling 11-byte window used inside a settings block |
| `+0x168` | `bool` | "discard everything until the next `\n`" flag (set after an over-long line) |
| `+0x169` | `u8` | settings state: `0` normal, `1` inside `<settings ...>`, `2` `</settings>` seen |

Behaviour, byte by byte:

1. `\n`: clear the skip flag; if it *was* set, drop the byte. Else clear the
   window; if state was `2`, strip leading `\r` from the buffer and reset to
   `0`; then `decode_wire_line(buffer)` → `parse_line`.
2. skip flag set and byte ≠ `\n`: discard.
3. state `1`: the byte goes into the 11-byte rolling window (never into the line
   buffer). If the window equals `</settings>` → state `2`, window cleared. Else
   if the window ends with `<prompt` → state `0`, window cleared and `<prompt`
   seeded into the line buffer (a prompt aborts an unterminated settings block).
4. otherwise: append to the line buffer; if the length was already
   `>= 0x40000` (`0x101c1a444: cmp x22, #0x40000`), decode the buffer, emit it
   as a raw frame and set the skip flag.

`finish` (`0x101c1a68c`): clear the skip flag and the window; empty buffer →
`Vec::new()`; else strip leading `\r`, `decode_wire_line`, `parse_line`.

Both are implemented in `parser/read.rs` in exactly this shape, including the
`0x40000` boundary, the `</settings>` window and the `<prompt` escape.

### Line decoding

`parser::wire::decode_wire_line` (276 bytes) keeps valid UTF-8 as-is and maps
**each invalid byte to the `U+00xx` character of that value** — not
`String::from_utf8_lossy` (no `U+FFFD`). Reimplemented that way.

### Tag scanning (`text`)

| function | established behaviour |
|---|---|
| `tag_name` | strips one leading `</` or `<`, then stops at `>`/`/`/whitespace; returns `&str` (empty for empty input), never `Option` |
| `find_tag_start` | repeated `core::slice::memchr` for `<`; skips `$<` escapes; requires the next byte to be an ASCII letter, `!`, `/`, `:`, or `_`; scans to the next `>` and rejects the candidate when `is_prose` says so |
| `is_prose` | inner span of `<`…`>`; contains `=` → markup (`false`); contains `#` → prose (`true`); otherwise prose iff it contains whitespace (the binary compares against `0x3d`, `0x23`, `0x20`, `0x3e`, `0x2f`) |
| `pairs`/`attributes` | `key=value` scanner over the tag body, quoted (`"`/`'`) or bare value |
| `attribute` | first `pairs` hit for the name, raw (undecoded) value |
| `attribute_u32` | `attribute` → trim → optional leading `+` → decimal with `checked_mul`/`checked_add`; `-`, empty and overflow → `None` |
| `link_from_tag` | `href` → `LinkKind::Href`, else `exist`(+`noun`) → `LinkKind::Object`, else `cmd` → `LinkKind::Command`; `coord` copied on all three; no usable attribute → `None` |

### Entities (`text::entities`)

* named: `amp`, `lt`, `gt`, `quot`, `apos`, **`nbsp` (`U+00A0`)**;
* numeric: `&#N;` decimal, `&#xN;`/`&#XN;` hex;
* the `;` search only looks at the next ≤ 13 characters;
* unknown names, bad digits, surrogates (`D800..DFFF`) and `> U+10FFFF` are
  copied through **verbatim** (`&` then continue, so `&bogus;` survives);
* `strip_control_chars` keeps TAB, `0x20..=0x7E` and everything at `>= U+00A0`;
  it drops the other C0 controls, DEL and the C1 range (`0x7F..=0x9F`);
* `decode_attribute_value` = `strip_control_chars(decode_entities(..))`.

### Numbers

* `signed` (404 bytes): optional `+`/`-`, decimal digits, `i32` checked.
* `parse_amount` (644 bytes): `str::trim` → `memrchr` for the **last** `/` →
  `trim_end_matches` on the left / `trim_start_matches` on the right →
  `rsplit`-style last/whitespace token on the left, first token on the right →
  `signed` on both sides. No slash, a comma group (`1,234`) or a trailing word
  on the left → `None`.
* `parse_duration_secs` (588 bytes): `str::trim`, `memchr` for `:`, 2 or 3
  segments only (`and w8, w22, #0xfffffffe; cmp w8, #2`), digits with an optional
  leading `+` (`-` → `None`), and `total = total * 60 + value` in **`u32`** with
  `checked_mul`/`checked_add` overflow → `None`. No unit suffixes.

### Frames, runs and links

| fact | evidence |
|---|---|
| `Frame` stride `0x118` (280 bytes) | `madd x10, x10, x26(=0x118), x8` in `push_bytes` |
| a raw over-long line is a `Frame` whose tag word is `0x8000_0000_0000_0039` | `mov x8, #0x39` + `movk x8, #0x8000, lsl #48` |
| `Run` stride `0xf8` (248 bytes) | `Vec<Run>` arithmetic throughout (`RECON.md`, consistent) |
| `Run` fields: `+0x00` text, `+0x18` `Option<String>` preset, `+0x30` `u16` style word, `+0x32` extra flag, `+0x38` `Option<Link>`, `+0x98` `Option<Link>` | `Run::object` / `as_run` offsets (`RECON.md`, consistent) |
| `Run::object` prefers the `+0x98` slot, then `+0x38`, keeping only object-kind links | `0x101c2f580` |
| `Link` = `+0x00` `Vec<(String,String)>` attrs, `+0x18` `Option<String>` coord, `+0x30` 48-byte target union (bit 63 of the first word clear ⇒ object link) | `RECON.md` + `link_from_tag` |
| `MerchantStatus` tags: `Start = isize::MIN`, `Rules = isize::MIN+1`, `Stop = …`, other → owned lowercased string; input trimmed and ASCII-lowercased in place | `0x101c2f898` (`star`+`t`, `rule`+`s`, `stop`) |
| `InventoryItem::in_capacity`: words at `+0x118`/`+0x11c`; `(1,0)` → tag 2 (`Unknown`); else tag `= (w2 % 10 != 0)`, payload `(w2 % 10, w2 / 10)` | `0x101c2f84c` |

### Writer (`write`)

* `escape` compares `"` `&` `'` `<` `>` (`0x22 0x26 0x27 0x3c 0x3e`) and emits
  `&quot; &amp; &apos; &lt; &gt;` — all five, matching the reconstruction.
* `escape_attribute` = `escape` plus `&#9;`, `&#10;`, `&#13;` for TAB/LF/CR
  (compares `0x9`, `0xa`, `0xd`).
* `empty(name, attrs)` → `<name k="v"/>`; `element` → `<name k="v">body</name>`
  built with `alloc::fmt::format_inner` and the literal pieces `<`, `>`, `</`,
  `>` (`0x103bf9130`).
* attribute names seen in the writer's pool: `exist`, `a`, `cmd`, `d`, `coord`,
  `href`, `noun`, `id`, `preset`, `<preset id=`, `</preset>`.
* `write::runs` uses the literals `<output class=""/>`,
  `<output class="mono"/>`, `<pushBold/>`, `<popBold/>`, `<!-- ENDCLIENT -->`,
  `<dir value…` and `write::same` to coalesce identical adjacent runs.

---

## Dependency verdict: **none**, re-verified this pass

**Verdict: `hydra_protocol` has no third-party dependency — not in the
original, and therefore not here.** `Cargo.toml` is `[package]` only, and no
`x = { workspace = true }` line appears in it.

**New method for this pass (the previous evidence was negative: "no such call
target").** Every `bl` in the whole crate's `.text`
(`0x101c194c0..0x101c3a070`, 135 KB) was resolved against the full `nm -a`
symbol table and bucketed by crate prefix:

| caller crate of the `bl` target | count |
|---|---|
| `rustc` (the allocator/panic shims, `__rustc`) | 937 |
| `core` | 489 |
| `hydra_protocol` (the crate itself) | 346 |
| local branches with no symbol (inlining inside the crate) | 326 |
| `alloc` | 210 |
| `_<alloc…`, `_<core…`, `_<&alloc…` (generic impls) | 133 |

Nothing else. So the only providers are `core`, `alloc`, `std`, the compiler
shims, this crate, and the four libc stubs (`memcmp`/`memcpy`/`memmove`/
`memset` at `0x101f849dc`/`0x101f849e8`/`0x101f849f4`/`0x101f84a00`; `bcmp`
*is* `memcmp`).

| candidate | verdict | evidence |
|---|---|---|
| `phf` 0.13.1 (in `crate_versions.txt`, **not** a workspace pin) | **not used** | no `phf`/`phf_shared` call target anywhere in the range; the 128-entry table is a sorted `(ptr, len)` array searched with `bcmp` |
| `memchr` 2.8.3 (in `crate_versions.txt`, **not** a workspace pin) | **not used** | every scan is `core::slice::memchr::{memchr_aligned, memrchr}` (`CsitGFVNq7Ifu_4core`), i.e. what `str::find`/`<[u8]>::position` compile to. The `memchr` crate *is* linked into the binary, for `regex`/`tokio` consumers |
| `serde`/`serde_core` 1.0.229, `toml` 1.1.6, `regex` 1.13.1, `jiff`, `rusqlite`, `tokio` | **not used** | no call target, and no monomorphisation of any of them over a `hydra_protocol` type in `nm -a` (contrast `hydra_exp`, which has 15 `tokio`-naming symbols) |
| `indexmap`, `smallvec`, `parking_lot`, `flume`, any workspace pin | **not used** | same |

Two earlier LIT annotations that appeared to point at
`phf_shared`/`serde_core`/`regex-automata` inside `parse_line` were artefacts of
`recon.py` resolving a stale `adrp` register (they sit on `add x0, x0, #0x8`
inside `drop_in_place`), and the `(?is)<input\b[^>]*>` regex in the pool
belongs to `hydra_net::trainer::page::hidden_fields` at `0x101c3c654`.

The remaining gaps are **coverage, not dependencies**: 30 of the 80
crate-local functions are still `unknown` (see the coverage table). They need
the 24 unmodelled payload types recovered first, because every one of them
builds a payload variant.

---

## Phase 1 reconnection

| | |
|---|---|
| **reconnected** | *nothing* — the crate is genuinely dependency-free in the original, so there is no std stand-in to remove |
| **kept** | the sorted-table `tags` module (identical algorithm to the original), the `core`-based scanners, the `std`-only parser and writer |
| **added** | no crate; no feature flags; no `Cargo.toml` change |
| **strengthened instead** | streaming fidelity + tests (below) |

What changed in this phase (all inside `crates/hydra_protocol/`):

1. `parser/read.rs` rewritten to the recovered byte state machine: the
   `<settings …>` block is now consumed at byte level with the rolling 11-byte
   `</settings>` window, the `<prompt`-aborts-settings escape, the state-2
   leading-`\r` strip, and the exact `0x40000` over-long-line boundary.
   Previously settings suppression was line-level only, so an unterminated
   settings block swallowed the rest of the stream forever.
2. `numbers::parse_duration_secs` now rejects `u32` overflow like the original
   (it was accumulating in `u64`); `+`-prefixed segments documented and pinned.
3. `tags` doc comments corrected: the table is not a PHF; `is_known` is a
   branchless binary search over `(&str, len)` pairs.
4. `lib.rs` doc comment corrected (the original had no helper crates).
5. Tests: 47 → 54 (see below).

Public API: unchanged. `Parser` gained two private fields (`settings_window`,
`settings_closed`); the struct is only constructible through `new()`/`default()`
outside the crate, and `Parser` already derived `Clone`/`Debug`/`Default`, which
still hold. No public item was renamed or removed. `frame::payload::LinkTarget`
still models the original's `LinkKind` (the name differs; changing it would break
`hydra_model` and `hydra_ui`).

---

## What changed in this pass (audit-core / Phase 2)

Seven new pieces of behaviour were recovered and implemented, each with
value-asserting tests; the `frame::payload`/`Frame` vocabulary was recovered for
the first time; and three earlier claims in this file were **wrong** and are
corrected.

### Implemented

| item | module | what it does now |
|---|---|---|
| `parser::wire::client_region` | `src/parser/wire.rs` | `fn client_region(line: &str) -> (Frame, &str)`: 15-byte prefix, `<!-- ENDCLIENT -->` split, `<c>`-prefixed head entity-decoded into `Frame::ClientText`, else `Frame::ClientRegion`; returns the tail. 7 tests. |
| `parser::is_paired` | `src/parser/markup.rs` (re-exported as `parser::is_paired`) | the exact 14-name paired-tag table, length-dispatched as in the binary. 3 tests, including the whole table and 19 negative spellings. |
| `parser::markup::is_markup` | `src/parser/markup.rs` | `a`/`b`/`d` via the 4-bit mask (so `c` is out), plus `style`, `preset`, `output`, `popBold`, `pushBold`. 3 tests. |
| `parser::compass::directions` | `src/parser/compass.rs` | `fn directions(&str) -> Vec<String>`; the needle is `"<dir value"` (the space is part of it), an unterminated tag *breaks* the scan. 6 tests. |
| `parser::inventory::num` | `src/parser/inventory.rs` | `fn num(text, name) -> Option<u32>`: `attribute` → `trim` → optional `+` → checked decimal. `""` → `None`, `"+"` → `Some(0)`, `"-5"` → `None`. 7 tests. |
| `frame::Frame::structural` | `src/frame.rs` | the recovered two-`String` constructor, frame tag `0x38`; `Frame::Structural`/`structural_parts`. 2 tests. |
| `write::same` | `src/write.rs` | `fn same(Option<&Link>, Option<&Link>) -> bool` — `coord` + `target`, and deliberately **not** `attrs`. 3 tests. |
| `write::close_link` | `src/write.rs` | the `Writing::close_links` delimiter rule (`</d>` for `Exist`/`Direct`, `</a>` for `Url`). 1 test. |

New public (additive) items: `parser::{compass, inventory, markup, wire}`,
`parser::is_paired`, `Frame::{Structural, ClientText, ClientRegion}`,
`Frame::{structural, structural_parts}`, `write::{same, close_link}`. Nothing
was renamed or removed.

### Corrections to this file

1. **`frame::methods::<Frame>::structural` was described as "frame carries no
   visible text".** It is a *constructor*: `fn structural(tag: &str, text:
   &str) -> Frame`, two `String`s and tag `0x38`. Its only caller is
   `dispatch::structural_only`, which flushes first.
2. **`write::same` was described as an "adjacent-run equality test".** It takes
   two `Option<&Link>`, and the interesting fact is that it ignores `Link.attrs`
   — which is why the original's writer coalesced runs whose links differed only
   in their extra attributes.
3. **`parser::is_paired`, `parser::compass::directions`,
   `parser::markup::is_markup`, `parser::write::same`,
   `write::Writing::close_links` and `frame::methods::…::structural` were listed
   as unrecovered.** All six are now decoded; four are implemented outright.
4. **The type inventory was listed as 24 payload types from "mangled type
   names" only.** `nm -a` also names the trait impls, the containers
   (`BTreeMap<String, Objective>`, `Vec<CmdListEntry>`, `Option<(u64, Menu)>`,
   `Ring<Runs>`, `VecDeque<Runs>`, …) and two more enums' variant sets
   (`LinkKind`, `ObjectivesAction`). See the type inventory above.
5. **`Frame` was described as "a 280-byte tagged struct with many payload
   variants".** It is a niche-tagged enum with **≥59** tags; the tag map is
   above.

### Deliberately *not* changed

* `write::runs` still closes every link with `</a>`. The binary's
  `Writing::close_links` writes `</d>` for the `Exist`/`Direct` variants in the
  *command* slot, which for an `<a exist=…>` link is ill-formed markup. That is
  what the instructions say, so `write::close_link` reproduces it on its own and
  `runs` keeps the existing (correct) output — all 8 pre-existing `write` tests
  are untouched. `Writing` itself is not modelled: it is a 4-field state
  machine (`String`, `Option<Link>`, `Option<Link>`, …) and this crate's writer
  is a single function.
* `client_region` is not called from `parse_line`. In the binary it is called at
  `parse_line+0xeb4`, inside the dispatch chain; calling it unconditionally
  would emit a `ClientRegion` frame for every line of 15+ bytes.
* `is_paired` is not used by `parse_line`. `parse_line`'s own
  `inner_until`-based handling stays as it was, so no existing output changes.

---

## Tests

`cargo test -p hydra_protocol --offline` → **86 passed** (54 before this
pass; the 32 new ones are listed after the Phase-1 ones below).

Phase 1 added (all still green):

* `tags::table_matches_the_recovered_binary_snapshot` — all 128 names in the
  binary's table order, verbatim.
* `tags::every_table_entry_is_recognised_and_nothing_else_is` — every entry is
  known; case, padding and prefix variants are not (the `bcmp` semantics).
* `tags::known_tags_iterates_the_whole_table`.
* `parser::settings_block_bytes_never_reach_the_line_buffer` — markup and
  newlines inside a settings block are dropped, `</settings>` re-enables the
  line, leading `\r` dropped, the tail of the closing line still parses.
* `parser::a_prompt_aborts_an_unterminated_settings_block` — `<prompt` inside an
  open block ends it and the prompt is still emitted.
* `parser::an_over_long_line_becomes_a_raw_frame_and_the_tail_is_dropped` —
  `0x40001` bytes → one `Frame::Raw`, remainder of the line discarded.
* `numbers::colon_durations_overflow_in_32_bits` — `None` past `u32::MAX`,
  `Some(u32::MAX)` just below.
* `numbers::parses_colon_durations` gained the `+1:+02` case.

Audit pass added (32):

* `parser::wire::tests::*` (7) — the 15-byte prefix and the 18-byte marker are
  the recovered lengths; nothing after the prefix with no marker; the tail
  starts right after the marker; a `<c>` head is entity-decoded and
  control-stripped while a non-`<c>` head is not; a short line answers the
  empty frame (the original panics).
* `parser::markup::tests::*` (5) — the exact 14-name paired table plus 19
  negative spellings; paired tags accept a whole tag; the single-letter mask is
  exactly `a`/`b`/`d`; the five named markup tags; `is_markup` does not
  normalise a whole tag.
* `parser::compass::tests::*` (6) — order, missing `value`, the needle's single
  required space, the break-on-unterminated case, empty input, and that values
  come back raw.
* `parser::inventory::tests::*` (7) — plain, trimmed, `+`-prefixed, `""` vs
  `"+"`, `u32` overflow (including the 23-digit `umull` path), non-numeric, and
  first-match-wins.
* `frame::tests::*` (3) — `structural` copies both strings with
  capacity == length, the frame carries no visible text, and the two
  `client_region` frames report their text.
* `write::tests::*` (4) — both-`None` is "same"; `attrs` is not compared but
  `coord` and `target` are; different `LinkKind`s never match; `close_link`
  follows the niche ordering.

---

## Corrections to `.work/RECON.md`

| RECON.md said | actually |
|---|---|
| `tags::is_known` "is a PHF set with 128 entries" | a sorted `[&str; 128]` searched with a branchless binary search + `bcmp`; no `phf` |
| `parse_line` "is 45 KB of inlined tag dispatch" | `parse_line` is 6352 bytes; the 45 KB was the over-estimate from the global-only symbol table. The dispatch lives in `parser::dispatch::dispatch` (6656 B) + 17 dispatch helpers + `parser::thin::thin_frame` (8508 B) + `parser::inventory::inventory_manager` (7872 B) + `parser::inner::*` |
| `parse_line` "preserve unknown tags as raw/text" | confirmed: unknown tags are re-emitted as literal runs |
| `MerchantStatus::parse` "likely start/stop/open/closed … (verify)" | exactly `start`, `rules`, `stop`; anything else becomes an owned lowercased `Other` |
| `attribute_u32` "negative input likely returns `None` (verify)" | confirmed: a leading `-` → `None`; a leading `+` is accepted |
| `parse_duration_secs` "returns total seconds (`u64`/`u32` — inspect)" | `u32` accumulate with checked overflow; `Option<u64>` in the public signature here |
| `numbers::signed` at `0x101c2e490` | confirmed, 404 bytes, private symbol |
| source-file list not recorded | 10 paths recoverable (`parser.rs`, `parser/{dispatch,emit,inner,wire,view_item,compass}.rs`, `text.rs`, `text/entities.rs`, `numbers.rs`) |

---

## Open questions / integration risk for the parent merge

1. **No dependency changed, so nothing merges into `Cargo.toml`/`Cargo.lock`.**
   L1's other crates (`gs_base`, `hydra_lich`, `hydra_net`, `hydra_session`) may
   still add pins; this crate contributes no lines.
2. **`Parser` gained private fields.** Anything that pattern-matches or
   struct-literals `Parser` outside the crate would break, but the struct's
   fields were already private.
3. **Streaming behaviour changed for settings blocks.** `hydra_lich`/
   `hydra_session` feed real streams through `push_bytes`; the new path drops
   settings *bytes* instead of settings *lines*, which is what the original did.
   Lines that only looked like settings content from a caller's perspective will
   now never reach `parse_line`. `parse_line`'s own line-level suppression is
   unchanged for direct callers.
4. **`parse_duration_secs` can now return `None` where it previously returned a
   huge number** (above `u32::MAX`). If a sibling crate has a test asserting the
   old wrapping behaviour, it will fail — none did at the time of writing
   (`cargo check --workspace --offline` green).
5. The large remaining gap is coverage, not dependencies: 19 of the 24
   `frame::payload` types and the `dispatch`/`inner`/`thin`/`view_item`/
   `inventory`/`markup` machinery are unrecovered (30 of 80 functions). That
   needs one recon pass per group; `dispatch` alone needs its 59-way tag map
   decoded.
6. **`client_region` is now recovered** — the `<prompt`-inside-settings rule was
   already in `read.rs`. What is *not* known is where in `parse_line` the
   binary calls `client_region`, so it is exposed but uncalled (calling it
   unconditionally would emit a `Frame::ClientRegion` for every line of 15+
   bytes).
7. **`Frame::Structural`/`ClientText`/`ClientRegion` are new public variants.**
   Additive, and no sibling matches on `Frame` (only `hydra_rewards` and
   `hydra_session` mention it in doc comments), but any exhaustive `match` on
   `Frame` outside this crate would now fail to compile. `grep` says there is
   none.
8. **`write::same`/`write::close_link` are new public functions.** Additive.
   `write::runs` output is unchanged, so no sibling sees a different rendering.
