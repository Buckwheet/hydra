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
| `src/parser.rs` | `parser::Parser::parse_line`, `parser::is_paired` | `parser.rs` ◐ |
| `src/parser/read.rs` | `parser::read::{push_bytes, finish}` | `parser/read.rs` ✅ |
| `src/parser/emit.rs` | `parser::emit::{text_frame, parse_runs_reporting, flush, push_run}` | folded into `parser.rs` ◐ |
| `src/parser/dispatch.rs` | `parser::dispatch::{dispatch, prompt, inline_paired, structural_only, open_stream, pop_stream, clear_stream, eg_merchant, objectives, objective, menu, menu_item, cmdlist, inline_image, progress_bar, unmodelled_after}` | — (tag dispatch reduced to a `match` in `parse_line`) |
| `src/parser/inner.rs` | `parser::inner::{children, body_tags, open_tag, unread, inner_text, inner_display_text, other_children, inside_each}` | — |
| `src/parser/view_item.rs` | `parser::view_item::{ViewItem, open_view_item, view_item_text, finish_view_item}`, `ViewItem::{append_text, close_section, push_link}` | — |
| `src/parser/inventory.rs` | `parser::inventory::{inventory_manager, num}` | — |
| `src/parser/thin.rs` | `parser::thin::{thin_frame, room_meta, script_timer, stray_objective}` | — |
| `src/parser/markup.rs` | `parser::markup::{markup_state, is_markup}` | — |
| `src/parser/wire.rs` | `parser::wire::{decode_wire_line, client_region}` | `decode_wire_line` ✅ (in `parser.rs`), `client_region` — |
| `src/parser/compass.rs` | `parser::compass::directions` | — |
| `src/text.rs` | `text::{tag_name, attribute, attribute_u32, link_from_tag, find_tag_start, is_prose, pairs}` | `text.rs` ✅ (`pairs` → `text::attributes`) |
| `src/text/entities.rs` | `text::entities::{decode_entities, strip_control_chars, decode_attribute_value}` | `text/entities.rs` ✅ |
| `src/numbers.rs` | `numbers::{parse_amount, parse_duration_secs, signed}` | `numbers.rs` ✅ |
| `src/runs.rs` | `runs::Run::object`, `runs::Runs::{plain, bold_fragments, is_empty}` | `runs.rs` ✅ |
| `src/frame.rs` | `frame::vocabulary::Frame`, `frame::methods::<impl Frame>::structural` | `frame.rs` ◐ (`Frame` lives at `frame::Frame`, `structural` not ported) |
| `src/frame/payload.rs` | `frame::payload::*` (see below) | `frame/payload.rs` ◐ |
| `src/tags.rs` | `tags::is_known` | `tags.rs` ✅ |
| `src/write.rs` | `write::{escape, escape_attribute, empty, element, runs, attributes, open, same}`, `write::Writing::close_links` | `write.rs` ✅/◐ |

`crates/hydra-protocol/src/parser/dispatch.rs` existing in the original confirms
the 6.3 KB `parse_line` is a *front door*: it scans a line and hands tags to a
`dispatch` table, with `emit` owning the run/flush machinery.

### `frame::payload` type inventory (established, from mangled type names)

`Link`, `LinkKind`, `Style`, `MerchantStatus`, `TextFrame`, `RoomMeta`,
`ItemDetail`, `ItemView`, `InventoryItem`, `InventoryResponse`,
`EbonGateMerchant`, `Menu`, `MenuItem`, `DialogWidgets`, `ProgressBar`,
`InlineImage`, `ActiveEffect`, `Continuation`, `CmdListUpdate`, `CmdListEntry`,
`Objective`, `ObjectivesAction`, `ObjectiveAction`, `ObjectiveReward`;
plus `parser::view_item::ViewItem`, `frame::vocabulary::Frame`, `runs::{Run, Runs}`.

Here only `Link`, `LinkTarget` (original name: **`LinkKind`**), `MerchantStatus`,
`Capacity`, `InventoryItem` and `TextFrame` exist. The other payload types are
not modelled; the parser keeps their raw text in `Frame::Unknown` instead of
dropping it. Sibling crates consume only `Link`/`LinkTarget`/`Run`/`Runs`/
`Frame`/`Parser`, so nothing in the public API was removed.

---

## Function table (all 81 crate-local functions, `nm -a`)

`size` is the exact extent (distance to the next symbol of *any* kind).
Status refers to this reconstruction.

### Public API (the 27 symbols in `symbols_demangled.txt`)

| symbol | address | size | status | recovered signature / behaviour |
|---|---|---|---|---|
| `parser::read::<impl Parser>::push_bytes` | `0x101c1a288` | 1028 | ✅ | `fn push_bytes(&mut self, &[u8]) -> Vec<Frame>` — byte state machine, see below |
| `parser::read::<impl Parser>::finish` | `0x101c1a68c` | 212 | ✅ | `fn finish(&mut self) -> Vec<Frame>` — flush the last line, stripping leading `\r` |
| `parser::Parser::parse_line` | `0x101c1a760` | 6352 | ◐ | `fn parse_line(&mut self, &str) -> Vec<Frame>` — original dispatches the whole tag vocabulary; here only the common tags |
| `text::attribute_u32` | `0x101c259f0` | 644 | ✅ | `fn attribute_u32(&str, &str) -> Option<u32>` |
| `text::link_from_tag` | `0x101c25c74` | 572 | ✅ | `fn link_from_tag(&str) -> Option<Link>` — `href` > `exist`+`noun` > `cmd`, `coord` kept on every variant |
| `text::find_tag_start` | `0x101c25eb0` | 836 | ✅ | `fn find_tag_start(&str) -> Option<usize>` |
| `text::is_prose` | `0x101c26534` | 1020 | ✅ | `fn is_prose(&str) -> bool` |
| `text::tag_name` | `0x101c26930` | 332 | ✅ | `fn tag_name(&str) -> &str` (empty slice, not `Option`) |
| `text::attribute` | `0x101c26a7c` | 1364 | ✅ | `fn attribute(&str, &str) -> Option<String>` |
| `text::entities::decode_entities` | `0x101c27354` | 2420 | ✅ | `fn decode_entities(&str) -> String` |
| `text::entities::strip_control_chars` | `0x101c27cc8` | 340 | ✅ | `fn strip_control_chars(&str) -> String` |
| `text::entities::decode_attribute_value` | `0x101c27e1c` | 348 | ✅ | `fn decode_attribute_value(&str) -> String` (= decode + strip) |
| `numbers::parse_amount` | `0x101c2dfc0` | 644 | ✅ | `fn parse_amount(&str) -> Option<(i32, i32)>` |
| `numbers::parse_duration_secs` | `0x101c2e244` | 588 | ✅ | `fn parse_duration_secs(&str) -> Option<u64>` (colon forms, `u32` accumulate) |
| `runs::Run::object` | `0x101c2f580` | 144 | ✅ | `fn object(&self) -> Option<&Link>` — `+0x98` slot before `+0x38` |
| `runs::Runs::bold_fragments` | `0x101c2f610` | 20 | ✅ | `fn bold_fragments(&self) -> Vec<String>` |
| `runs::Runs::plain` | `0x101c2f624` | 232 | ✅ | `fn plain(&self) -> String` |
| `runs::Runs::is_empty` | `0x101c2f70c` | 48 | ✅ | `fn is_empty(&self) -> bool` |
| `frame::payload::InventoryItem::in_capacity` | `0x101c2f84c` | 76 | ✅ | `fn in_capacity(&self) -> Capacity` |
| `frame::payload::MerchantStatus::parse` | `0x101c2f898` | 664 | ✅ | `fn parse(&str) -> MerchantStatus` |
| `frame::payload::TextFrame::as_run` | `0x101c2fb30` | 484 | ✅ | `fn as_run(&self) -> Run` |
| `tags::is_known` | `0x101c3603c` | 488 | ✅ | `fn is_known(&str) -> bool` — sorted table + binary search |
| `write::escape_attribute` | `0x101c37c74` | 904 | ✅ | `fn escape_attribute(&str) -> String` |
| `write::runs` | `0x101c382b4` | 2556 | ✅ | `fn runs(&Runs) -> String` |
| `write::empty` | `0x101c38df8` | 160 | ✅ | `fn empty(&str, &[(&str,&str)]) -> String` |
| `write::escape` | `0x101c38e98` | 996 | ✅ | `fn escape(&str) -> String` |
| `write::element` | `0x101c393e0` | 172 | ✅ | `fn element(&str, &[(&str,&str)], &str) -> String` |

### Crate-local helpers (recovered, not exported; only `nm -a` has them)

| symbol | address | size | status | notes |
|---|---|---|---|---|
| `parser::emit::<impl Parser>::push_run` | `0x101c19ef4` | 916 | ◐ | builds a `Run` from text + current style/preset/link state |
| `parser::emit::<impl Parser>::text_frame` | `0x101c194c0` | 828 | ◐ | wraps accumulated runs into a `Frame` |
| `parser::emit::<impl Parser>::parse_runs_reporting` | `0x101c197fc` | 1476 | — | style/link transition tracking while emitting |
| `parser::emit::<impl Parser>::flush` | `0x101c19dc0` | 308 | ◐ | end-of-line flush |
| `parser::markup::<impl Parser>::markup_state` | `0x101c1c030` | 1208 | — | open-tag stack / markup bookkeeping |
| `parser::markup::is_markup` | `0x101c3948c` | 264 | — | |
| `parser::dispatch::<impl Parser>::dispatch` | `0x101c1d7f8` | 6656 | — | the tag dispatch table |
| `parser::dispatch::<impl Parser>::prompt` | `0x101c1d414` | 996 | ◐ | `<prompt>` (minimal version in `parse_line`) |
| `parser::dispatch::<impl Parser>::inline_paired` | `0x101c1d02c` | 816 | — | `<b>`, `<i>`, `<d>`, `<a>`, `<comp>` … |
| `parser::dispatch::<impl Parser>::structural_only` | `0x101c1d35c` | 184 | ◐ | known structural tag: consume, emit nothing |
| `parser::dispatch::<impl Parser>::open_stream` | `0x101c1cdc0` | 316 | — | `<streamWindow>`/`<stream>` |
| `parser::dispatch::<impl Parser>::pop_stream` | `0x101c1c4e8` | 612 | — | |
| `parser::dispatch::<impl Parser>::clear_stream` | `0x101c1cefc` | 304 | — | |
| `parser::dispatch::<impl Parser>::eg_merchant` | `0x101c1c74c` | 1652 | — | Ebon Gate merchant window |
| `parser::dispatch::<impl Parser>::objectives` | `0x101c32e00` | 1288 | — | `<objectives>` list |
| `parser::dispatch::<impl Parser>::objective` | `0x101c34210` | 928 | — | single `<objective>` |
| `parser::dispatch::<impl Parser>::menu` | `0x101c33ca4` | 832 | — | `<menu>` |
| `parser::dispatch::<impl Parser>::menu_item` | `0x101c340b8` | 344 | — | `<menuItem>` |
| `parser::dispatch::<impl Parser>::cmdlist` | `0x101c33fe4` | 212 | — | `<cmdlist>` |
| `parser::dispatch::<impl Parser>::inline_image` | `0x101c33308` | 796 | — | `<image>` |
| `parser::dispatch::<impl Parser>::progress_bar` | `0x101c33624` | 824 | — | `<progressBar>` |
| `parser::dispatch::<impl Parser>::unmodelled_after` | `0x101c3395c` | 840 | — | fallthrough for tags without a model |
| `parser::is_paired` | `0x101c1f1f8` | 572 | — | tag-pair table lookup |
| `parser::inner::*` (8 fns) | `0x101c315a4`…`0x101c3275c` | 964/880/844/1044/364/156/284/188 | — | inner-tag scanner: `children`, `body_tags`, `open_tag`, `unread`, `inner_text`, `inner_display_text`, `other_children`, `inside_each` |
| `parser::view_item::<impl Parser>::open_view_item` | `0x101c1f434` | 1252 | — | `<inv>` detail view |
| `parser::view_item::<impl Parser>::view_item_text` | `0x101c1f918` | 204 | — | |
| `parser::view_item::<impl Parser>::finish_view_item` | `0x101c1f9e4` | 800 | — | |
| `parser::view_item::ViewItem::{append_text, close_section, push_link}` | `0x101c345b0`/`0x101c3472c`/`0x101c349e8` | 380/700/224 | — | type does not exist here |
| `parser::inventory::inventory_manager` | `0x101c27f78` | 7872 | — | `<inventoryManager>` panel |
| `parser::inventory::num` (2 ×) | `0x101c29e38`, `0x101c2a0bc` | 644/692 | — | |
| `parser::thin::thin_frame` | `0x101c2a6f4` | 8508 | — | the big "thin out a frame" reducer |
| `parser::thin::room_meta` | `0x101c2caf4` | 5048 | — | `<room>` metadata |
| `parser::thin::script_timer` | `0x101c2c830` | 528 | — | |
| `parser::thin::stray_objective` | `0x101c2ca40` | 180 | — | |
| `parser::wire::decode_wire_line` | `0x101c39d24` | 276 | ✅ | `parser::decode_wire_line` (`u8` → `char` fallback, not `from_utf8_lossy`) |
| `parser::wire::client_region` | `0x101c39b14` | 528 | — | loads the `<!-- ENDCLIENT -->` literal (`0x103bf919d`, at `0x101c39b60`/`0x101c39b98`): strips Lich's client-region markers. Not recovered |
| `parser::compass::directions` | `0x101c39e38` | 568 | — | builds `"<dir value"`-style output (literals `0x103bf91af`/`0x103bf91b4`, panic sites in `crates/hydra-protocol/src/parser/compass.rs`); the `phy_left`/`mnt_left`/`phy_conv`/`mnt_conv` strings sit in the same pool but their owner is not pinned |
| `text::pairs` | `0x101c261f4` | 832 | ✅ | `text::attributes` |
| `numbers::signed` | `0x101c2e490` | 404 | ✅ | `numbers::signed` (private) |
| `frame::methods::<impl Frame>::structural` | `0x101c2f73c` | 272 | — | "frame carries no visible text" |
| `write::attributes` | `0x101c37a20` | 596 | ✅ | `write::attributes` (private) |
| `write::open` | `0x101c37ffc` | 696 | ◐ | `write::open_link` (per-variant, not one function) |
| `write::same` | `0x101c38cb0` | 328 | — | adjacent-run equality test used by `runs` |
| `write::Writing::close_links` | `0x101c3927c` | 356 | ◐ | link-close handling inside `write::runs` |

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

## Departures: what the original used vs this reconstruction

**There are none.** Every `bl` in the 81 crate-local functions resolves to
`core`, `alloc`, `std` or the crate itself; the only non-Rust calls are the four
libc stubs `memcmp`, `memcpy`, `memmove`, `memset`
(`otool -Iv` on `__TEXT,__stubs` → addresses `0x101f849dc`, `0x101f849e8`,
`0x101f849f4`, `0x101f84a00`; `bcmp` *is* `memcmp`).

| candidate | verdict | evidence |
|---|---|---|
| `phf` 0.13.1 (in `crate_versions.txt`, **not** in the workspace pins) | **not used by this crate** | no `phf`/`phf_shared` call target in any of the 81 functions; the 128-entry table is a sorted `(ptr,len)` array searched with `bcmp` |
| `memchr` 2.8.3 (in `crate_versions.txt`, **not** in the workspace pins) | **not used by this crate** | all 22 scan calls are `core::slice::memchr::{memchr_aligned, memrchr}` (`CsitGFVNq7Ifu_4core`), i.e. what `<[u8]>::position`/`str::find` compile to. The `memchr` crate *is* linked into the binary, but for `regex`/`tokio` consumers |
| `serde` / `serde_core` 1.0.229, `toml` 1.1.6, `regex` 1.13.1, `jiff`, … | **not used by this crate** | no such call target. Earlier LIT annotations that appeared to point at `phf_shared`/`serde_core`/`regex-automata`/`emit.rs` inside `parse_line` were artefacts of `recon.py` resolving a stale `adrp` register (they sit on `add x0, x0, #0x8` inside `drop_in_place`), and the `(?is)<input\b[^>]*>` regex in the pool belongs to `hydra_net::trainer::page::hidden_fields` at `0x101c3c654`, which the *global-only* symbol table mis-attributed to `write::element` |

Consequences for the crate:

* no dependency was added to `crates/hydra_protocol/Cargo.toml`; it stays
  dependency-free (`[package]` only), and `x = { workspace = true }` appears
  nowhere;
* `Cargo.toml` is unchanged in this phase, so the workspace `Cargo.lock` is
  unchanged too.

The **behavioural** departures that remain are parser coverage, not
dependencies (see the function table): the tag dispatch (`dispatch`, 6.6 KB),
inner-tag scanner (`inner`), `view_item`, `inventory`, `thin`, `markup`,
`compass` and `write::same`/`close_links` are not reimplemented. That is a
pre-existing gap of the reconstruction, deliberately left alone here because it
needs the type vocabulary of `frame::payload` (24 types) to be recovered first.

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

## Tests

`cargo test -p hydra_protocol --offline` → **54 passed**.

Existing 47 kept unchanged. Added in this phase:

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
5. The large remaining gap is coverage, not dependencies: 20 of the 24
   `frame::payload` types and the `dispatch`/`inner`/`thin`/`view_item`/
   `inventory` machinery are unrecovered. That belongs to a later recon pass,
   together with the `<prompt`-inside-settings and `client_region`
   (`<!-- ENDCLIENT -->`) rules, whose exact behaviour is only partly pinned.
