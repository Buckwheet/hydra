# `hydra_gui` — recovery notes

Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(no DWARF; demangled globals in `symbols_demangled.txt`). The stub crate had 12
`todo!()` functions over 13 files; this build has none —
`grep -Rn 'todo!\|unimplemented!' crates/hydra_gui/src` is empty — and

* `cargo check -p hydra_gui` — clean, no warnings
* `cargo test -p hydra_gui` — **108 tests pass**
* `cargo check --workspace` — clean (no other crate was touched)

Phase 4 replaced the `egui`/`eframe` seams with the real thing: the drawing goes
through the pinned `egui` (git `3da022d`) and the window is an `eframe` window.
What that changed, what it pinned down and what is still a stand-in is in
**Phase 4: real egui/eframe** below; `docs/EGUI.md` is the version research it
builds on.

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow, literals, table data and offsets read out of the instruction stream and the `__const`/`__DATA_CONST` tables; asserted by a test. |
| **partly** | the entry contract, literals and helper calls are recovered, but one step (a struct's field types, or a call into an out-of-crate dependency) could not be; the body implements the evidenced part and says so at the site. |
| **UNKNOWN** | the binary does not pin this down. The safest non-invented behaviour was chosen and the choice is written where it is used. |

Every uncertain decision also has a `// UNKNOWN:` comment at the point of use.

---

## Tooling

Read-only, all scratch outside the repo (`$TMPDIR/rg`):

* `.work/recon.py` — the shared helper (`syms`, `sizes`, `disas`, `bytes`,
  `str`, `findstr`). Its `func_end` walks the *demangled globals*, which is much
  too coarse for this crate (`play::titlebar::right_at_first` is 92 bytes but the
  next global is 715 420 bytes away), so:
* **A local-symbol boundary map.** `nm -an` gives 72 002 text symbols; `rustfilt`
  demangles them. Every `func_end` below is the next *local* text symbol, which
  is what separates `Bar::look` (1 068 B) from `Bar::said` (264 B).
* **An annotating disassembler** (`ann.py`) over `objdump -d --no-show-raw-insn`
  that tracks `adrp`/`add` and prints the string or data a literal pointer
  addresses, and resolves `bl` to the demangled name.
* **A whole-`__text` cross-reference scanner** (`xref.py`) for `bl` and `adrp`
  targets, and a **pointer scanner** (`ptr.py`) that finds 8-byte occurrences of
  an address anywhere in the image plus `adrp`+`add` pairs. This is how the two
  `badge::pixels` callers (the two `Once` initialisers) and the
  `left_at_first`/`right_at_first` users (`TitleBar::set` and a serde
  `deserialize_struct`) were found.
* **A `format!`-piece decoder.** rustc stores `format!` literals as
  `<len><text>` runs with one `0xC0` per `{}` and a `0x00` terminator. Every
  message quoted below came out of it, e.g. `0x103e274f1` →
  `` `{}` in `{}` is not Ctrl, Shift, Alt or Cmd. ``
* **Static-table readers.** `__const` arrays are plain: the 11-entry default
  binding table at `0x10439e418`, the 16-entry key alias table at `0x104394298`,
  the 117-byte search permutation at `0x1021403e1`, the 138-entry `egui::Key`
  name tables at `0x1043947b0`/`0x102140490` and the drawer default floats at
  `0x102156cc0`. All were read as `(ptr, len)` pairs or as raw `f32`/`u8` runs.
* **Panic `Location` structs.** Each `panic_bounds_check` argument is a
  `(&str, len, line, col)` record, so a bounds check names its own source file:
  `0x104386428` → `crates/hydra-gui/src/keys.rs`, line 342, column 30.

---

## ABI facts this crate re-confirms

* `&str` = `(ptr, len)`; `String`/`Vec` = `{cap @0, ptr @8, len @16}` and are
  returned through `x8`.
* `Option<String>`'s `None` is `isize::MIN` in the capacity slot — which is how
  `Chord` (`{key: String, mods: u8}`) is packed into `Result<Chord, String>`:
  the error variant *is* the niche (`str x8 = -0x8000000000000000, [x19]`
  @`0x1003d8460`), so the whole thing is 0x20 bytes.
* `Option<&T>` is a bare pointer with `0` as `None`.
* A *fieldless enum with a `String` payload* uses the capacity niche plus an
  offset: `Item`'s unit variants are `isize::MIN + k` and the payload variant is
  index 18 (`Item::label` @`0x1003da1c0`).
* `f32` immediates dominate this crate; `mov w8, #0x4..; fmov s0, w8` is how
  40.0, 60.0, 100.0, 144.0, 160.0, 240.0 and 660.0/520.0 appear.
* A `Vec<T>` by value is `(x0, x1, x2)` = cap/ptr/len (`vessel::running`'s
  argument, and how its `max(len, 1)` size hint is computed).

---

## Function table

Addresses are unslid VAs.

### `app.rs`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `run` | `0x100512f78` | **partly** | `fn(?, &str, &Path, &Path, bool, ?) -> …` — seven arguments plus `x8` `sret`, 72-byte return. Order: `Placements::load(&path)` → `Placements::builder(name, "hub", size)` → a `String` `"Hydra"` → `hydra_board::own::graphics(&path)` → `egui_wgpu::WgpuConfiguration::default()` → `eframe::epi::NativeOptions::default()` → `Box<App>` (112 bytes) → `eframe::run_native("Hydra", 5, &options, app, vtable, sret)` → `hydra_board::seat::Wake::closed` → copy the `Result` out. |

`run`'s body is 1 200 bytes of struct copies; the recovered *constants* are what
matter and they are all here:

* **title** `"Hydra"` — written byte-wise at `0x100513014`
  (`mov w8, #0x7948; movk w8, #0x7264, lsl #16` = `"Hydr"`, `strb w8=0x61` = `a`)
  and passed as `(&str, 5)` to `run_native` @`0x1005132a0`.
* **placement** `"hub"` — 3 bytes at `0x10207ecc3`, length in `w2` @`0x100512fe4`.
* **size** `660.0 × 520.0` — `mov x3, #0x44250000; movk x3, #0x4402, lsl #48`
  @`0x100512fe8` packs `0x44250000` = 660.0 and `0x44020000` = 520.0 into one
  register for `Placements::builder`. UNKNOWN: which half is the width; 660 wide
  is assumed.
* **the logged-on flag** — `cmp w5, #0x0; cset w26, ne` @`0x100512fbc`, carried
  into the app value at `0x338` (`strb w26` @`0x10051324c`).

[`app::plan`] therefore returns an [`app::RunPlan`] holding the title,
placement, size, graphics folder, data folder, flag, renderer and the 0x70-byte
app size; [`app::build`] turns that into the real `NativeOptions` plus a boxed
[`app::Hub`], and [`app::run`] is the recovered `run_native` call (never invoked
by a test). See **Phase 4: real egui/eframe**.

### `badge.rs`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `pixels` | `0x1007cdcd0` | **full** | `fn(square: bool, size: u32) -> (Vec<u8>, u32)`. `image::load_from_memory(&PNG[..0x2144d])`; on `Err`, drop the `ImageError` and return `(vec![], 0)`; if `square && size != image.width()` then `resize_exact(size, size, FilterType(4) = Lanczos3)`; finally `to_rgba8().into_raw()` with the *post-resize* width. |

* The embedded PNG is 136 269 bytes at `0x102157f69` (`mov w1, #0x144d; movk
  w1, #0x2, lsl #16` @`0x1007cdcfc`), verbatim in `src/badge/badge.png`.
* IHDR `00 00 01 00 / 00 00 01 00 / 08 / 06 / 0 / 0 / 0`: **256 × 256, 8-bit,
  colour type 6 (RGBA), not interlaced**.
* The returned struct is 28 bytes (`{Vec<u8>, u32}` — the caller at
  `0x100546d3c` duplicates that one `u32` into the width *and* height of an
  `egui::IconData`-shaped `{rgba, width, height}`, so the badge is always
  square). Two `OnceLock`-style initialisers call it with `w0 = 0`
  (`pixels(false, _)`).
* UNKNOWN: the second argument at the only two call sites. `x1` is not written
  by either caller, so its value there is unrecoverable; it only matters when
  `square` is true, which no recovered caller passes.

### `bar.rs`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `Bar::look` | `0x100708334` | **partly** | `fn look(&mut self, over: &Look) -> Look`. Copies the live look out of `self`, overlays the written fields, forces every colour's alpha to `0xff` (`orr w12, w12, #0xff000000` @`0x100708610`), clamps an emblem index with `min(_, 4)` (`csel w0, w0, w1(=4), lo` @`0x10070869c`), stores the `fills`/`place` byte pair back into `self` (`strh w9, [x0, #0x165]` @`0x100708344`) and returns a 0x170-byte copy. |
| `Bar::said` | `0x100708760` | **full** | `parts()` then `join(" ")`: `join_generic_copy` with the 1-byte literal `" "` at `0x102144b98`. |
| `Bar::parts` | `0x100708868` | **partly** | `fn parts(&self) -> (String, String)` — builds a `Vec<String>` of pieces, joins each group, returns the pair. |
| `Bar::words` | `0x100708dd0` | **full** | The caption, or `"{said} {place} : hunt #{n} on file spans the same"` when the hunt words at `0x130`/`0x138` are present. |
| `Bar::behind` | `0x100709280` | **partly** | Hit test against the filled part: `fraction = min(fills,100)/100` (`csel` against 100 @`0x1007092b8`, `0x42c80000` = 100.0 @`0x1007092c4`); a `Corner` fill grows the area by `min(w,h) * 0.5` (`fmul s3, s3, #0.5` @`0x100709300`); the fill edge then moves along the axis the `Place` selects. |
| `Bar::fitted` | `0x1007093d8` | **full** | See below. |

`Bar::fitted` (the layout maths):

1. `ui.available_size()` — `bl egui::ui::Ui::available_size` @`0x100709404`.
2. The caption is `parts().0` when the place byte is 5 (`cmp w8, #5`
   @`0x100709410`), otherwise `words()`.
3. Only places 1..=5 measure anything: `sub w9, w8, #1; cmp w9, #4; b.hi`
   @`0x100709458`.
4. The caption is laid out and its **height** measured:
   `TextStyle::resolve` @`0x100709488`, an `egui` layout call @`0x1007094cc`,
   `ldp d9, d8, [galley, #0x30]`.
5. `+ 4.0` padding on both axes (`fmov.2s v1, #4.00000000` @`0x1007094fc`).
6. The bit mask `0x26` (bits 1, 2, 5 = `Above | Below | Corner`) takes the
   caption height off the **height**; the mask `0x18` (bits 3, 4 =
   `Left | Right`) takes it off the **width** (`tst w8, w9` @`0x100709514`,
   `tst w8, #0x18` @`0x100709544`).
7. `max(_, 8.0)` on both axes (`movi.2s v1, #0x41, lsl #24` = 8.0
   @`0x100709568`), stored at `0x144` with the flag at `0x140`, and the bar
   (368 bytes) is returned by copy.

`Place` is the 7-variant enum whose names and tags come from
`<… Deserialize for Place>::…visit_str` @`0x1006d1e70`: `hidden` 0, `above` 1,
`below` 2, `left` 3, `right` 4, `corner` 5, `inside` 6.

`Says` field names/order (`label`, `numbers`, `percent`, `words`) and `Look`
field names/order (`fills`, `place`, `says`, `color`, `overlay`, `background`,
`track`, `fill_image`, `ring`, `clock`, `threshold`, `text_color`,
`outline_color`, `outline`, `second_emblem`) are the literals passed to
`SerializeMap::serialize_entry` by `Says::serialize` @`0x100707a14` and
`Look::serialize` @`0x1007076c0`.

### `bar::stack`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `Stack::words` | `0x10065abe4` | **full** | Four labelled percentages joined with `", "` (the 2-byte literal at `0x10214036a`). Labels in record order `0x50`, `0x68`, `0x80`, `0x98`: `Sp`, `HP`, `MP`, `SP` (the 8 bytes at `0x102140362`). Each record's first word is compared with `2` — "unknown" (`cmp w9, #2` @`0x10065ac2c`) → `format!("{} ?", label)` (pieces at `0x103e2b848`); otherwise the word at `+0x10` is the percentage, clamped to 100, formatted `format!("{} {}%", …)` (pieces at `0x103e2b84d`). |
| `Stack::paint` | `0x10065a828` | **partly** | Inscribed square: `centre = (min+max)*0.5`, `radius = min(w,h)*0.5`, bail out when empty (`fcmgt.2s v0, v11, v8` @`0x10065a884`). Its body looks the style up (`Context::global_style` @`0x10065a890`) and calls `bar::shape::paint` once per vital (four `bl`s to `0x100524814`); that helper is not reconstructed, so each vital is painted here as a stroked track, an arc of `percent/100` of the turn and its `Vital::word` label. |

UNKNOWN: the word at `+0x14` of each record (`ldr w8, [x20, #0x64]`
@`0x10065a8d0`), kept as `Vital::clock`.

### `bar::vessel`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `running` | `0x1004c7f7c` | **full** | `fn(sizes: impl IntoIterator<Item = u64>) -> Vec<f32>`: cumulative stop positions, `out[0] = 0.0`, `out[i+1] = Σsizes[..=i] / Σsizes`, and the last element forced to `1.0` only when the sum is non-zero. |
| `paint` | `0x1004c7da0` | not reconstructed | One `rect_filled` per segment of [`vessel::running`]'s cumulative stops; `vessel::segments` exposes the same rects without painting. |

### `keys.rs`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `Keys::does` | `0x1003d7be8` | **partly** | `fn does(&self, kind: Option<&T>, chord: &Chord) -> Option<&Macro>`: the byte at offset `0xf0` of `kind` (clipped with `min(_, 9)`) selects one of **10** maps at `self + 0x18` (`umaddl x8, w0, w9(=0x18), x8` @`0x1003d7e48`, bounds panic on `crates/hydra-gui/src/keys.rs:342`); a hit returns that layer's `Option<&Macro>` (a `None` action shadows the shared map), a miss falls back to the `BTreeMap` at `self + 0x00` (`ldr x22, [x26]` @`0x1003d7f14`). The chord is `{key: String @0, mods: u8 @0x18}`: the lookup compares `x2 + 8`/`x2 + 0x10` (the `String`'s ptr/len) and `x2 + 0x18`. |
| `Keys::bound` | `0x1003d81f0` | **partly** | The iterator form of the same lookup: builds a 1- or 2-byte "layer key" buffer (`min(byte, 9)`), then `BTreeSet::from_iter`. |
| `Keys::take` | `0x1003d8050` | **full** | `Vec::retain` over 72-byte `egui::Event`s: every `Event::Text` is dropped (discriminant `0xc`, `cmp w8, #0xc` @`0x1003d80d4`) and every bound `Event::Key { pressed: true, .. }` (discriminant `0xd`, retain closure @`0x100398dd0`) is turned into a `Macro` and removed. See **Phase 4** for the discriminant proof. |
| `Chord::parse` | `0x1003d831c` | **full** | Split on `'+'`, modifiers for all but the last token, `names::known` for the last, three recovered messages. |
| `Chord::written` | `0x1003d8a64` | **partly** | Formats the modifiers then the key. |
| `names::known` | `0x10065b430` | **full** | 16-entry `(name, alias)` table at `0x104394298` (numpad keys ↔ `num_*`), compared case-insensitively against both columns and returning the *name*; then `winit_name` over the 117-byte permutation at `0x1021403e1`. |
| `names::winit_name` | `0x10065af2c` | **full** | `egui::Key` → chord name, see below. |
| `names::capturable` | `0x101ee2a08` | **full** | `HashSet` of five `egui::Key`s: `0x77` (`LaunchMail`), `0x76` (`LaunchApp2`), `0x75` (`LaunchApp1`), `0x35` (`H`), `0x36` (`I`). |
| `defaults::defaults` | `0x100796548` | **full** | 11-entry `(&str, &str)` table copied from `0x10439e418` (`memcpy` of `0x160` bytes, `mov w21, #0xb` @`0x100796578`). |

`winit_name`'s rules (all recovered, each with its branch):

| input (`egui::Key`) | output | evidence |
|---|---|---|
| `0x6c..=0x73` | `None` | `sub w8, w21, #0x6c; cmp w8, #8; b.hs` @`0x10065af50`; the neighbouring message calls them "bound to nothing" |
| `0x0f..=0x12`, `0x16..=0x18`, `0x1b..=0x1c`, `0x20` | `None` | `b.lo 0x10065b0d0` chains @`0x10065b058`..`0x10065b0b8` |
| `0x19`/`0x1a`/`0x1d`/`0x21` | `BracketLeft`/`BracketRight`/`Backquote`/`Equal` | `0x10065b104`/`0x10065b098`/`0x10065b114`/`0x10065b0c0` |
| a one-character name (`A`..`Z`) | `"Key" + name` | `format!("Key{}")` at `0x103e290ec`, @`0x10065af9c` |
| `Num0`..`Num9` | `Digit0`..`Digit9` | `format!("Digit{}", &text[3..])` at `0x103e290e4`, @`0x10065b000` |
| anything else | unchanged | `0x10065b15c` |

The `egui::Key` names themselves come from
`<egui::data::key::Key as core::fmt::Debug>::fmt` @`0x10064d2bc`: the pointer
table at `0x1043947b0` and the length table at `0x102140490`, 138 entries,
`ArrowDown … Again`.

The default bindings, verbatim from `0x10439e418`:

| chord | command | chord | command |
|---|---|---|---|
| `Numpad8` | `north` | `Numpad9` | `northeast` |
| `Numpad2` | `south` | `Numpad7` | `northwest` |
| `Numpad6` | `east` | `Numpad3` | `southeast` |
| `Numpad4` | `west` | `Numpad1` | `southwest` |
| `Numpad5` | `out` | `Numpad0` | `down` |
| `NumpadDecimal` | `up` | | |

The modifier bits are `ctrl = 0x1`, `shift = 0x2`, `alt/option = 0x4`,
`cmd/command/win/super = 0x8` (the `orr w26, w8, w26` chain in `Chord::parse`
and, independently, `vellum::Spelled::of` @`0x1004cefd8`).

### `layout::drawers`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `Drawers::is_default` | `0x1007cf5a4` | **full** | All four drawers must equal the default. |
| `Zone::serialize` | `0x1007ce860` | **full** | Unit variant names `main`/`top`/`bottom`/`left`/`right` at `0x102157570`..`0x102157581`. |
| `Mode::deserialize` | `0x1007ce910` | fieldless enum | Names `pushing`/`clipping` at `0x10217d7df`/`0x10217e6c0`; the option captions in `play::options::window::drawers_set` @`0x10081d644` give the third, "Drawn". |
| `Zones::of` | `0x1007cef40` | **partly** | See below. |
| `Zones::strip`/`main_rect_in` | — | the reconstruction | The same zone extents, as rects inside a window: `Top`/`Left` hang off the top-left corner, `Bottom`/`Right` off the opposite one. |
| `Drawers::paint` | — | the reconstruction | Fills each open drawer's strip with the theme fill scaled by `opacity`, for the modes it is given (`Drawn`/`Pushing` behind the play area, `Clipping` over it), and returns what it painted. |
| `Zones::fit`/`keep`/`inside` | `0x1007cf34c`/`0x1007cf3f8`/`0x1007cf478` | **partly** | 20-byte placed records (`ldrb w11, [x9, #0x10]; add x9, x9, #0x14` in `fit`). |

`is_default` in full:

* the `mode` bytes at `0x9`, `0x15`, `0x21`, `0x2d` and the `open` bytes at
  `0x8`, `0x14`, `0x20`, `0x2c` are OR-ed and must all be zero
  (`tst w8, #0xff` @`0x1007cf634`), with the right drawer's mode byte checked
  last (`cset w0, eq` @`0x1007cf644`);
* the eight floats are compared with the static vectors at `0x102156cc0`
  (`fcmeq.4s` @`0x1007cf5ec`/`0x1007cf5f8`), masked by the lane-bit table at
  `0x102156ce0` (`01 00 02 00 … 80 00`) and reduced with `addv.8h`.

That table is the `Drawers::default()` value as well — `q1 = [160.0, 1.0,
160.0, 1.0]`, `q2 = [240.0, 1.0, 240.0, 1.0]`:

| drawer | `size` | `opacity` | `open` | `mode` |
|---|---|---|---|---|
| top | 160.0 | 1.0 | false | `drawn` |
| bottom | 160.0 | 1.0 | false | `drawn` |
| left | 240.0 | 1.0 | false | `drawn` |
| right | 240.0 | 1.0 | false | `drawn` |

`Drawer`'s serde order is `open` (`+0x8`), `size` (`+0x0`), `mode` (`+0x9`),
`opacity` (`+0x4`); the layout is `size` f32 @0, `opacity` f32 @4, `open` bool
@8, `mode` @9 — 12 bytes. `Drawers`' order is `top` @0, `bottom` @0xc, `left`
@0x18, `right` @0x24.

`Zones::of`'s recovered constants: a drawer must be open (`cmp w8, #0x1`) and
finite, its size is clamped up to **40.0** (`0x42200000` @`0x1007cef98`), the
drawers may only take `max(height − 60, 0)` (`0xC2700000` = −60.0 @`0x1007cefdc`)
and `max(width − 144, 0)` (`0xC3100000` = −144.0 @`0x1007cf088`), and when they
ask for more they are both scaled by `room / (a + b)`.

### `play::titlebar`

| symbol | addr | status | recovered signature and behaviour |
|---|---|---|---|
| `left_at_first` | `0x1003d97c0` | **full** | `fn() -> Vec<Item>` with `cap = 2`, `len = 2`: `[Character, Connection]` (`isize::MIN + 0`, `isize::MIN + 2`). |
| `right_at_first` | `0x1003d981c` | **full** | `[Menu, Zoom]` (`isize::MIN + 0xb`, `isize::MIN + 0xd`). |
| `TitleBar as Default` | `0x1003ccb64` | **full** | left = the two above, middle = empty, right = the two above, `strh w8 = 0x100` at `0x48` (so `tucked = false`, `settled = true`). |
| `TitleBar as From<Written>` | `0x1003c6b98` | **full** | Copy the three lists, set the second flag, and insert `Menu` at the first `Settings` in the right list when no list already holds one and `tucked` is clear. |
| `Item::label` | `0x1003da1ac` | **full** | 19-entry jump table at `0x1020740af`. |
| `TitleBar::ui` | — | the reconstruction | One real `egui::Button` per item (`ui.button(item.label())`): left + middle left-to-right, the right list right-to-left so `Menu` sits left of `Zoom`. Returns `(Item, Response)` pairs for the caller. |
| `Item::key` | `0x1003d9d84` | full (names) | `character`, `profession`, `connection`, `room_title`, …, `top_drawer`, `bottom_drawer`, `left_drawer`, `right_drawer`. |

`Item`'s 18 unit variants, in discriminant order and with the recovered labels:
`Character`, `Profession`, `Connection`, `RoomTitle` ("Room title"),
`RoomNumber` ("Room number"), `Lock`, `Arrange`, `DrawersMenu` ("Drawers menu"),
`LayoutMenu`, `KeysMenu`, `Log`, `Menu`, `Settings`,
`Zoom` ("Zoom − and +"), `TopDrawer` ("Top drawer icon"), `BottomDrawer`,
`LeftDrawer`, `RightDrawer`, plus `Text(String)` at index 18.

`TitleBar`/`Written` are 0x50 bytes: three `Vec<Item>`s at 0, 0x18 and 0x30 and
two flags at 0x48/0x49.

---

## Phase 4: real egui/eframe

`docs/EGUI.md` established the pin — `github.com/emilk/egui` at git rev
**`3da022d98072fb631443a72f49973cfb6c93f865`** (every crate labelled `0.35.0`),
`eframe` with `default_fonts` + `wgpu` and **no** `glow`/`accesskit`/`persistence`.
The three departures that document replaced (egui → `seam`, no text metrics,
eframe → `RunPlan`) are now gone.

### The seam is `egui`

`crate::seam` is a re-export layer, not a model: `pub use egui::{Color32, Pos2,
Rect, Vec2}`, `pub type {Galley, Painter, Ui}` aliases, and three helpers
(`pos`, `rect`, `rect_from_min_max`) that bridge `Vec2` offsets to `egui::Pos2`
corners. What the old seam asserted still holds and is asserted again
(`seam::tests::the_layout_identity_is_the_recovered_one`): `Color32` 4 bytes with
alpha last, `Vec2` 8, `Pos2` 8, `Rect` 16, **`egui::Event` 72** (the `0x48`
stride `Keys::take` walks), and the 117-entry `egui::Key` table
(`H` = 0x35, `I` = 0x36, `IntlBackslash` = 0x74) that `names::winit_name` walks.

`Color32` has no `to_u32`/`from_u32` of its own, so `seam::Color32Ext` adds the
two recovered conversions (`r | g << 8 | b << 16 | a << 24`) plus `opaque()`,
the `orr w12, w12, #0xff000000` of `Bar::look` @`0x100708610`. `Look`'s JSON
still writes the packed word, so the round-trip tests are unchanged.

`RecordingPainter`/`TestUi`/`seam::Draw` are gone: every test that used them now
runs a real frame (`crate::headless`, a `#[cfg(test)]` module in `lib.rs`: one
`egui::Context`, one `RawInput` with a fixed `screen_rect`, one
`Context::run_ui`, then assertions on the `ClippedShape`s — rect fills, circles,
text galleys). No test opens a window, starts a winit loop or creates a wgpu
device.

### The app: `Hub`, `Launch`, `build`, `run`

`app::run` @`0x100512f78` is split exactly where the binary splits it:

| piece | what it is |
|---|---|
| `pub fn plan(start: &Start) -> RunPlan` | the Phase-1 body: everything the original had before `run_native` (title `"Hydra"`, placement `"hub"`, 660 × 520, graphics folder, placements path, logged-on flag, `app_size` 0x70) |
| `pub fn run_plan(start: &Start) -> RunPlan` | the Phase-1 name, forwarded |
| `pub struct Hub` | the boxed app value: `TitleBar`, `Drawers`, `Keys`, `Room`, `Start`, the `0x338` logged-on flag |
| `pub fn build(start: &Start, logged_on: bool) -> Launch` | `NativeOptions::default()`, `renderer = Renderer::Wgpu` (the only variant this build has — `egui_glow` is not linked), `viewport.with_title("Hydra").with_inner_size([660.0, 520.0]).with_icon(badge)`, `wgpu_options = WgpuConfiguration::default()`, and `Box<Hub>`. Pure value construction: no device, no window |
| `pub fn run(start: &Start, logged_on: bool) -> eframe::Result` | the recovered call, `eframe::run_native(&title, options, Box::new(move \|_cc\| Ok(app)))`. Nothing in the tests calls it |

`Hub` implements `eframe::App` with this rev's trait method
`fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame)` (the binary's
impl is `<App as eframe::epi::App>::ui` @`0x1004f79a0` — `ui`, not `update`), and
forwards to `Hub::ui_contents`, which is callable headlessly
(`ctx.run_ui(input, |ui| hub.ui_contents(ui))`).

`Hub::chrome(window) -> Chrome` is the layout, and it is all recovered maths:
`Zones::of(&drawers, window.size())` (`0x1007cef40`: the 40.0 extent floor, the
60/144 reserves, the proportional scaling) gives the four strips and the main
area; the main area splits into a title-bar strip (`Hub::TITLE_BAR_HEIGHT`, no
recovered literal — UNKNOWN) and the play area. `ui_contents` paints, in order:
`Drawn`/`Pushing` drawers (behind), the play area in
`visuals.extreme_bg_color` plus the room title/number/lines/prompt, the title-bar
strip plus one real `ui.button(item.label())` per recovered `Item`, then the
`Clipping` drawers over the top. A clicked drawer icon flips its drawer
(`Hub::press`, the state transition `TitleBar::set` performs).

### Widgets

* **`bar::Bar` is an `egui::Widget`** (`<Bar as egui::widgets::Widget>::ui`
  @`0x1006f4920`): `fitted` → `allocate_exact_size` → `write`. `Bar::fitted`
  now measures for real — `ui.available_size()`, the caption from
  `parts().0`/`words()`, `egui::TextStyle::Body.resolve(ui.style())` (the
  static handed to `TextStyle::resolve` at `0x100709488` has variant tag `1` =
  `Body`) and `ui.painter().layout(..)`, then `galley.size().y`, the `+4.0`
  padding, the `0x26`/`0x18` axis masks and the `8.0` floor, all unchanged.
  `Bar::write` paints `track`, `background`, the fill (`Bar::fill_rect`, the
  same axis choice `behind` hit-tests), `outline`, `ring`, the emblem
  (`min(index, 4)`) and the caption in `text_color`.
* **`bar::stack::Stack::paint`** draws the inscribed square's four nested rings
  (`circle_stroke` tracks), one arc per known vital (`Shape::line`, 25 points,
  from 12 o'clock clockwise, `percent/100` of the turn) and the four
  `Vital::word` labels (`Sp 50%`, `MP ?` for the unknown tag).
* **`bar::vessel::paint`** is one `rect_filled` per segment of the recovered
  cumulative stops (`vessel::running`); `vessel::segments` exposes the same
  geometry without painting.
* **`layout::drawers::Drawers::paint`** fills each open drawer's
  `Zones::strip` with the theme fill scaled by `opacity`
  (`Color32::gamma_multiply`) and reports what it painted; `Zones::strip` /
  `Zones::main_rect_in` are the zone rects inside a window.
* **`play::titlebar::TitleBar::ui`** is one real `ui.button` per item — left and
  middle left-to-right, right list right-to-left so `Menu` sits left of `Zoom`.

### Things Phase 4 pinned down that the Phase-1 notes guessed

* **`egui::Event`'s discriminants.** The Phase-1 note said tag `0xc` was
  "Text-like". It is exactly `Event::Text`: at this rev the tags are
  `Copy` 9, `Cut` 10, `Paste` 11, **`Text` 12**, **`Key` 13**,
  `PointerMoved` 14 … (a probe on the pinned rev confirmed each value; the
  niche-based numbering is why they do not start at 0). The binary's own reads
  match: `Event::Key`'s `pressed` at `+0x4`, `repeat` at `+0x5`, `Modifiers` at
  `+0x6`, `key` at `+0xb`, `physical_key` at `+0xc` with `0x75` as
  `Option<Key>`'s `None`. So `Keys::take` drops every `Event::Text` and takes
  out bound `Event::Key { pressed: true, .. }` presses, naming the chord from
  `physical_key.unwrap_or(key)` through `names::winit_name` and looking it up
  with `Keys::does`.
* **The caption's text style.** `Bar::fitted`'s `TextStyle::resolve` is handed
  the static at `0x102144b78`, whose first word is `1` — `TextStyle::Body`
  (which the pinned rev defaults to `FontId::proportional(13.0)`).
* **`Keys::take`'s shape.** It is
  `fn(&self, kind: Option<&T>, events: &mut Vec<egui::Event>) -> Vec<Macro>`:
  the retain closure captured `(self, kind)` and passed both to `Keys::does`
  (`ldp x25, x26, [x8]`, `mov x0, x25; mov x1, x26` @`0x100398e38`/`@0x100398eac`).
  `Keys::take` keeps the one-argument form and `Keys::take_on` takes the layer.

### Remaining UNKNOWNs in this crate

* `Hub::TITLE_BAR_HEIGHT` (24.0) and `Hub::PAD` (8.0) — no recovered literal.
* `Bar::write`'s instruction stream, the emblem's geometry (`min(index, 4)` is
  recovered, the placement is not), `ring`'s exact drawing, and where each
  `Place` puts the caption (drawn centred).
* `bar::shape::paint` @`0x100524814` — the helper `Stack::paint` calls four
  times — was not reconstructed; the arcs are drawn here instead.
* `bar::vessel::paint` @`0x1004c7da0`'s body: the segments are recovered, the
  primitives are the obvious ones.
* `TitleBar::set`/`page`'s layout (which group is centred, spacing) — the three
  lists are drawn left/middle/right in one row.
* `Drawers`' `mode`: `Zones::of` subtracts every open drawer, so a `Clipping`
  drawer still shrinks the main area here; only the paint order honours the mode.
* Repeating key presses (`Event::Key { repeat: true, .. }`) take an extra
  branch in `Keys::take` that was not read; they are treated as first presses.
* The `emblem` field has no recovered serde name, so `Look`'s JSON does not
  carry it (the round-trip test says so).

---

## Departures (serde/image → std, and the parallel crates)

1. **`serde`/`serde_json` → `json`.** `crate::json` is a small hand-written
   JSON value/parser/writer; `Drawers`/`Drawer`, `Look`/`Says`, `Written`/`Item`,
   `Chord` and `Keybinds` implement `to_json`/`from_json` by hand in the
   *recovered field order*. Round-trip tests assert byte-exact output. serde's
   `#[serde(default)]` behaviour is reproduced where it is evidenced
   (`Written::left`/`right` default to `left_at_first`/`right_at_first`, which
   is why those two functions are reachable from a `Deserialize` caller at
   `0x100462f94`).
2. **`image` → a pre-decoded asset.** `badge::pixels` used
   `image::load_from_memory`/`resize_exact`/`to_rgba8`. The PNG bytes are
   embedded verbatim (`src/badge/badge.png`, 136 269 bytes, the recovered
   asset), and `src/badge/badge.rgba` is the *same* image decoded ahead of time
   (256 × 256 × 4 = 262 144 bytes, byte-for-byte what `to_rgba8()` produced),
   because a zlib inflater would be a new dependency rather than recovered
   crate logic. The `resize_exact(.., Lanczos3)` branch is implemented in-tree
   (separable Lanczos3, support 3.0, widened by the sampling ratio) so the
   recovered control flow is real code. `badge.png` is kept so the asset is not
   lost if an inflater is ever added.
3. **`hydra_board` is not a dependency.** The recovered code stores
   `hydra_board::binding::Macro` values (the `drop_in_place` symbol for
   `Keys::layers`'s iterator at `0x1003860e4` spells out
   `BTreeMap<Chord, Option<Macro>>` and `BTreeMap<Chord, Macro>`), calls
   `hydra_board::own::graphics` (`0x10147042c`) and
   `hydra_board::seat::Wake::closed` (`0x1013e9080`), and implements
   `hydra_board::seat::Windows` for an `EguiWindows` type. `hydra_board` is
   being reconstructed in parallel and the original relationship was circular
   (it is generic over the GUI through `hydra_gui::PlayWindow`), so:
   * [`keys::Macro`] is a local stand-in holding the recovered command word;
   * `app::graphics_dir` keeps the recovered call shape and joins `graphics`
     explicitly (documented in place);
   * `PlayWindow`/`Windows` are not reconstructed at all.
4. **`Mode`, `Says`, `Look` field types.** The serde *names and order* are
   recovered; the *types* of several `Look` fields are not (they serialise
   through shared monomorphisations). `Color32`, `Place`, `Says` and the flags
   are evidenced; `fill_image`/`second_emblem` are modelled as asset names and
   `threshold` as an `Option<f32>`. Marked `UNKNOWN:` at the definition.
5. **Struct layouts.** The original `Bar` is 368 bytes, `Look` 0x88, `Stack`
   ~0xb0, `TitleBar`/`Written` 0x50. The reconstructed structs keep the
   *evidenced* fields and offsets in their docs but are not byte-compatible.

---

## Conflicts

* **`Keys::does`'s second argument.** `x1` is checked with `cbz` (so
  `Option<&T>`) and its byte at offset `0xf0` — clipped to 9 — indexes the layer
  array. What that 0xf0-offset value *is* could not be pinned down (the big
  structs it lives in are `hydra_board` types that are being reconstructed in
  parallel), so `does` takes `kind: Option<u8>` and the doc comment records the
  original shape. The 10-element array and its two bounds checks
  (`crates/hydra-gui/src/keys.rs:342` and `:344`) are exact.
* **`Bar::parts`' split.** The recovered function builds a `Vec<String>` and
  joins it twice, returning `(String, String)`; which pieces land in which half
  is inferred from `Says { label, numbers, percent, words }` (label left, the
  rest right). The two `format!`s it uses for the value side
  (`"{} {}%"`, `"{} ?"`) are recovered verbatim from `Stack::words`, and the
  bar's own `Said`/`%` rendering is not.
* **`Bar::look`'s two `Place` bytes.** `0x165` and `0x166` are both `Place`s in
  the original (`behind` reads the first, `fitted` the second); which is
  `fills` and which is `place` follows the serde order, not a proven mapping.
* **`badge::pixels`'s `x1`.** Neither recovered caller writes `x1`, so the
  second argument's value at the only call sites is unrecoverable. The signature
  is `(square: bool, size: u32)` because the callee reads both.
* **`Item` label vs key.** The recovered `Item::label` returns *capitalised
  prose* ("Room number") while `Item::key` returns the file identifier
  (`character`, `top_drawer`, …). `Written`'s JSON therefore uses the keys
  (`Item::key`), and unknown identifiers survive as `Item::Text`.
* **`app::run`'s size pair.** `660.0`/`520.0` are packed into one register; which
  of the two is the width is not proven.

---

## What is *not* reconstructed

The original crate is far larger than the stub list suggests (996 globals):
`card`, `compass_art`, `doll_art`, `hub`, `logs`, `main_menu`, `menu`, `pad`,
`placement`, `play::{draw, holders, options}`, `surveyor`, `titlebar_page`,
`tracked`, `widget::{draw, status, roster, effects, lines, split, minimap, …}`
and most `egui` widget impls. `lib.rs` keeps the module names the stub tree had
(`chat`, `reports`, `theme_editor`, `widget::effects`) with the types the
unresolved-symbol list names, but no behaviour — they are still stubs after
Phase 4. `bar::shape::{cap, kit, laid_over, paint, wrapped}`,
`Item::of_key` beyond the inverse of `Item::key`, `TitleBar::set`/`page`,
`Place::serialize`, `Keybinds::in_file`/`rows`/`beneath` and
`keys::{vellum, wrayth, file, page, write}` are named in the tables above where
they were read but are not implemented. `Bar::write`, `bar::vessel::paint`,
`bar::stack::Stack::paint`, `TitleBar::ui` and `Drawers::paint` **are**
implemented now, but from their recovered fields rather than instruction by
instruction — the `UNKNOWN:` notes say where.

---

## Test inventory

108 tests, all asserting recovered values rather than restating the code. Every
drawing test runs one headless `egui` frame (`crate::headless`) and asserts on the
shapes that frame produced — a filled rect's exact geometry and colour, a
circle's centre and radius, a text shape's string — never just "it drew
something".

| file | tests | what they pin down |
|---|---|---|
| `json.rs` | 4 | parse/write round-trips, number formatting, error cases |
| `seam.rs` | 3 | the egui pin (sizes 4/8/8/16/**72**, `Key::ALL` = 117, `H`/`I`/`IntlBackslash`), `Color32` packing + `opaque`, `Rect`/`Pos2` geometry |
| `lib.rs` (`headless`) | — | one `Context`, a fixed `RawInput`, shape/rect/circle/text extractors |
| `bar.rs` | 16 | `Place` names and the `0x26`/`0x18` axis masks, fraction clamping at 100, `said`/`parts`/`words`, `fitted`'s axis cases against the **real galley height** (+4.0 padding, 8.0 floor, `Corner` measuring only the label half), `fill_rect` agreeing with `behind`, the `Corner` growth, alpha forcing + emblem clamp, the painted background/fill/caption rects, track/outline/ring strokes, the emblem disc positions, and the `Widget` impl taking the fitted rect |
| `bar/stack.rs` | 6 | the `Sp/HP/MP/SP` labels + `", "` separator, `?` for the unknown tag, the inscribed square, the four nesting radii, the arc geometry (25 points, 12 o'clock, clockwise), the painted tracks/arcs/labels and the empty-square bail |
| `bar/vessel.rs` | 6 | cumulative stops, empty input, zero-sum stays flat, hand-computed fractions, exact segment rects, one `rect_filled` per segment |
| `badge.rs` | 7 | the verbatim PNG (length, signature, IHDR 256×256/8/RGBA), decoded spot colours, the `square && size != width` guard, the Lanczos3 kernel, solid-colour preservation, alpha-shape preservation |
| `keys.rs` | 21 | modifier bits and every alias, `+` parsing, the three error strings, `written`→`parse` round-trip, the 11 default moves, alias lookup, the `egui::Key` name table, `winit_name`'s translations and gaps, the 117-byte search order, the five capturable codes, layer clipping `min(_, 9)`, layer-then-shared lookup, `take` on real `egui::Event`s (bound press removed + its `Macro`, `Text` always dropped, physical key preferred, modifiers read, layers, releases and unbound keys left alone), `Mods::of_egui`, key-file round-trip |
| `layout/drawers.rs` | 13 | the default 160/160/240/240 table, each field flipping `is_default`, extent floor 40, the 60/144 reserves, proportional scaling, the main rect, zone/mode names, JSON, the four `Zones::strip` rects in a 660×520 window, the painted strips, opacity → alpha, and the mode-filtered paint passes |
| `layout.rs` | 2 | main rect with shut/open drawers |
| `play/titlebar.rs` | 12 | the first-paint entries, `Default`, the label/key literals, `of_key` inversion, `From<Written>`'s `Menu` insertion and `tucked`, JSON defaults, `items()` order, the four real buttons (their labels as painted text, all inside the strip, `Zoom` flush right) |
| `app.rs` | 11 | the title/placement/size constants, the run plan, `build`'s `Renderer::Wgpu` + 660×520 + `"Hydra"` + the 256×256 RGBA badge icon, `Hub::chrome` for shut and open drawers, `press`'s drawer toggles, the painted play area (`extreme_bg_color`) and title-bar strip + four items + room text, the four open strips in `panel_fill`, and `Clipping` painting last |
