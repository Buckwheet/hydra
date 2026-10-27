# hydra_board — reconstruction notes

`hydra_board` is the Hydra front end's board: one *seat* per character, the
controller pad, the chips and targets the story window draws, the hotbars and
macros, the highlights, and the files they all live in.

Everything here was recovered from the arm64 Mach-O binary
`/Users/cpmbpro/Code/hydra-inspect/hydra` (no DWARF).  Addresses are unslid VAs.

## Legend

| word | meaning |
| --- | --- |
| **ESTABLISHED** | the binary proves the signature *and* the behaviour: literals, constants and the shape of the code all agree. |
| **PARTLY** | the signature and most of the behaviour are proved, but one rule (an order, a limit, a separator) could not be read out of the code.  The chosen rule is stated in the doc comment. |
| **UNKNOWN** | the binary shows the *shape* — offsets, sizes, which field is read — but not the meaning.  The code takes the most conservative reading and says so at the site. |
| **std departure** | the original used a crate or a runtime this build does not have. |
| **seam** | a place where the original did I/O or UI work; the model logic is kept and the effect is handed back to the caller. |

## Where the evidence came from

* `nm -an` gives mangled local symbols; `rustfilt` demangles them.  That was the
  single most useful step: 819 symbols name `hydra_board`, including drop glue
  that spells a type's fields (`drop_in_place<hydra_board::pad::wheel::Slice>`)
  and closure paths (`hydra_board::seat::run_steps::{{closure}}`).
* `otool -Iv` maps the lazy stubs, so `atan2f` (`0x101f8443c`), `fmodf`
  (`0x101f84718`), `__sincosf_stret` (`0x101f84334`) and `bcmp` (`0x101f849dc`)
  are identified by name in the disassembly.
* `objdump -d --no-show-raw-insn` for the bodies, with `adrp`/`add`/`ldr`
  rewritten back into data addresses to recover the literals.
* Rust string literals are **not** NUL-terminated and the linker packs them
  together, so a `__cstring`/`__const` "string" is usually several literals
  concatenated.  A literal's real length came from the `mov w1, #len` beside the
  `adrp`/`add`, e.g. the default `controller.toml` is `0x102415fec`, 0x8eb = 2283
  bytes.
* The `serde` field visitors survive as real functions, so a struct's field
  names and their index order are readable: `Slice` has nine
  (`label, command, color, span, inner, slices, start, back, fire_type`), `Chip`
  eleven, `Pattern` five, `Threshold` two, `Phone` five, `Forgotten` two,
  `GoalProfiles` two.

## What was recovered exactly

The pure logic, with tests that assert the binary's own numbers:

* `pad::wheel::lay_out` (`0x101459e40`) — spans clamped to 30°, unsized slices
  sharing `(360 - sum) / n` (floored at 0), the `f32::EPSILON` and `0.001`
  thresholds, the `360 / total` rescale, and the centre sequence
  (`start - spans[0]/2`, then a running total).
* `pad::wheel::seat_at_angle` (`0x101457ab8`) — `atan2f(sin, cos)` in degrees
  (57.295776), first seat subtracted, folded into `[0, 360)`, cumulative spans.
* `pad::names::rank` (`0x101470618`) — the whole 0…17 ordering of the seventeen
  buttons, plus 17 for anything else.
* `pad::names::BindKey::{parse, canonical}` (`0x101470e58`, `0x101471014`) — split
  on `+`, dedup neighbours, last piece is the key; join with `+`.
* `pad::rumble::Pattern` (`0x10141eefc`) — the three built-ins (`short`
  0.5/160/1/120, `double` 0.8/180/2/120, `long` 0.9/450/1/120) and the clamps a
  configured pattern goes through (0.05…1.0, 20…2000 ms, count ≥ 8 or exactly 1,
  gap ≤ 2000).
* `pad::tuning::Tuning` — the fourteen field names in file order and the four
  defaults the constructor writes (`150, 150, 300, 40`, which pair with the two
  trigger percentages so that `open < close`), plus `trigger` being `f32::clamp`.
* `pad::file::ControllerFile::hydras` — the default `controller.toml`, all 2283
  bytes of it, parsed and asserted (eight wheel slices, four overlay chords,
  the four stance sub-slices and their colours).
* `binding` — the 29 unnumbered action names and their labels, `character_1…9`,
  `macro_set_0…9`, and `typed` being `replace("\\r", "\r")`.
* `banner::parse_hex`, `own::{web_page, web_page_port, records_sessions}`,
  `lookout::chime` (RIFF/WAVE header), `store`'s fold-and-flush queue,
  `stream_rules::fold_redirects`, `kept::{file, unread}`.

## Partly recovered (the rule was chosen, not read)

Each carries a **PARTLY** note at the site.  The important ones:

* `pad::wheel::View::{real, leaf, seats}` — the `(kind, index)` pairs and the
  `"back"` comparison are proved; what `kind` *means* is not.
* `pad::wheel::Open::aim` / `south` — the `Instant::saturating_duration_since`
  against the dwell is proved; the rule for a changed aim is not.
* `targets::{Target::hint, Target::shown, TargetsLook::threshold_color}` — the
  statuses and health are proved; the separators and the threshold search order
  are not.
* `chips::Chip::color_at`, `hotbars::Button::lines` (the `MAX_COMMAND` figure),
  `highlights::Highlights::call`, `trainer::{Trainer::applied, typed}`,
  `pad::interact::{Interact::step, StepKind}`.

## Unknown (shape only)

* `wheel::Meta` beyond its leading `Option<f32> start`.
* `wheel::Open`'s two `Vec`s (modelled as `path` and `picks`).
* `Slice::inner` / `Slice::back` — present, never given a meaning.
* `Pattern`'s three middle fields beyond `strength`, `ms`, `count`, `gap`.
* `Interact::StepKind`'s byte table beyond the five named kinds.
* `Clip`'s page link, `Disk`'s `Job` wording, `push`'s HTTP payload shape.

## std / UI / async departures

| original | here |
| --- | --- |
| `serde`, `serde_json`, `toml` + derived `Deserialize` | `src/text.rs`: a hand-written JSON reader and a TOML subset reader into one `Value` tree, plus writers.  The formats are the ones the literals show (`*.toml` files, `*.json` files, `format!("{}.json")`). |
| `hydra_ui::story::Story` (behind a `Mutex`) | `seat::Story`, a local model with `push`/`alert`/`typing`.  `hydra_ui` is a sibling crate this one may not depend on (and its `story.rs` is still stubs).  Noted as a **std/UI departure**. |
| `hydra_net::trainer::goals::SkillGoals` behind `Arc<dyn …>` | `trainer::SkillGoals`, a plain struct.  Same reason. |
| `tokio` tasks for the seats' channels, the save queue and the step delays | plain `Vec`/`BTreeMap` queues the caller drains: `Seat::{hotbar_presses, rumbles}`, `store::Saves::flush`, `Seat::run_steps` returning the total `Duration` instead of sleeping. |
| `reqwest` for push notifications | `push::{Request, allow, fitted, client}` describe the request; nothing is sent. |
| the web server (`Sessions::serve_web`) | the `WebAt` address is returned; nothing is bound. |
| ~~`hydra_gui`'s wheel editor calling `seat_at_angle`~~ | **struck in Phase 4**: `seat_at_angle` is unchanged, and `pad::wheel::{wedges, labels, paint, ui}` now draw the wheel this crate lays out. |
| the settings pages (`hydra_ui::*::{Page, Row}`) | `banner::Row` is a minimal row type; `Own::pages`, `BannerFile::page`, `lookout::{page, with_trigger_row}` return it. |
| `OnceLock`/`LazyLock`/`Mutex` fields | kept as-is where the accessors prove them (`Seat::log`, `Seat::team`, `Seat::rumbles`, `Seat::story`). |

Dependencies added to `crates/hydra_board/Cargo.toml`: `hydra_protocol`,
`hydra_model` (for `GameState` in `pad::rumble::Watch::moments`) and
`hydra_session` (for `SessionHandle`, `Notice`, `Step`, `PlayerLog`, `Clip`,
`Notes`), plus `egui` and `epaint` at the workspace's git rev in Phase 4 (the
two crates the binary's `hydra_board` calls — see below).  No `hydra_gui`.

## Honest conflicts

1. **`lay_out` returns centres, not starts.**  The initial accumulator is
   `start_angle - spans[0] * 0.5` (`0x10145a46c`); the only sequence that makes
   that offset meaningful is the centre recurrence
   `c_i = c_{i-1} + (s_{i-1} + s_i) / 2`.  `seat_at_angle` then treats the
   relative angle as starting at `seats[0].0`, i.e. at the first *centre*, so a
   hit can be half a slice out at the seam.  That is what the binary does; it is
   not corrected here.
2. **`seat_at_angle`'s second argument order.**  The one caller
   (`hydra_gui`'s wheel editor, `0x1006b50c8`) passes what `__sincosf_stret` just
   returned, so the two floats are a direction's sine and cosine and the function
   is `atan2f(sin, cos)`.  Naming them `x, y` would be misleading.
3. **`Slice::back` / `inner`.**  The serde visitor lists nine field names, but
   only seven are ever read: `slices` (`+0x30`), `span` (`+0x78`), `start`
   (`+0x80`) and the `"back"` *label* test at `+0x18`.  `back` is modelled as a
   string, which may not be what the original held.
4. **`i64::MIN` sentinels.**  `merchants::told` (`+0x60`), `wheel::View::leaf`
   (`+0x60`) and `rumble::Highlighted` (element `+0x0`) all compare a word
   against `0x8000000000000000` to mean "not set".  No plain Rust type this crate
   declares produces that niche, so the field is modelled as a value with a
   separate flag rather than reproducing it.
5. **`tuning`'s four defaults.**  `150, 150, 300, 40` are written by the
   constructor in one `stp`, but which field each lands on is inferred from the
   field-name order in the merged literal.  The `open < close` pairing of the two
   trigger percentages is what makes the reading consistent.
6. **`Chip`'s eleven fields.**  `name`, `list`, `color`, `empty`, `critical` and
   `order` are proved by the field visitor; `bar`, `height`, `percent`,
   `thresholds` and `shown` come from the literal blob.  `current` also appears
   there and is not modelled (it looks like runtime state).
7. **The default `chips`.**  `Chip::defaults` returns the four bars the chrome is
   known to read; the binary's own built-in list was not recovered field by
   field, so this is marked PARTLY.

## Phase 4: real egui

The binary's `hydra_board` touches egui in exactly two places (`docs/EGUI.md`
§3.5, read out of the `bl` edges of the whole `__TEXT` disassembly): `Ui::available_width`
inside `targets::Target::named`, and `epaint::TextureHandle` as the instantiation
of the generic `disk::Disk::kept_at` cache.  Both are restored, and so is the
third thing the section names: `hydra_gui`'s `EguiWindows` implementing
`hydra_board::seat::Windows`.

Everything else here is the crate's own drawing.  In the binary the *painting* of
the HUD lives in `hydra_gui` (`hydra_gui::widget::{status, hotbar, doll, …}`), so
the geometry below that the binary does not give is marked **UNKNOWN** at the
site — the model, the colours that come out of the settings files, and the
layout maths are the recovered parts.

### The two recovered egui callees, and the trait

| callee / item | here | evidence |
| --- | --- | --- |
| `egui::Ui::available_width` | `TargetsLook::word` at `Target::chars_in` → `paint::fit_chars`, used by `Target::{named, word_in, ui}` | the call edge into `Ui::available_width` from `Target::named`; the width is turned into the `usize` `Target::shown(NameShown::Fit, …)` already took |
| `epaint::TextureHandle` | `disk::Kept<T>::kept_at`, aliased `disk::Textures = Kept<TextureHandle>` and `disk::Pictures = Kept<(TextureHandle, Arc<[f32]>)>` | the drop glue names both instantiations; keeping the handle alive is why the cache exists (`epaint` frees a texture with its last handle) |
| `hydra_board::seat::Windows` | `seat::windows::Windows` (the trait) + `seat::EguiWindows` | `<hydra_gui::EguiWindows as hydra_board::seat::Windows>::{open_url, repaint, repaint_seat}` at `0x1005846b4`, `0x1005846a8`, `0x10058462c` |

`repaint_seat`'s body was read out of the disassembly: `request_repaint()`, then
`request_repaint_of(ViewportId::from_hash_of(("play", seat)))` — the four-byte
literal `play` at `0x10208216b`, hashed with the seat's `u32`.
`open_url` allocates a copy of the url and calls `Context::open_url` with
`new_tab = true`.

**Name clash, deliberate:** the binary calls the *trait*
`hydra_board::seat::Windows`; that name is already the recovered per-seat set of
open windows (`pub struct Windows { open: Vec<String> }`) and this crate does not
rename public items, so the trait lives one module down at
`hydra_board::seat::windows::Windows`.

### Public API added

```rust
// src/paint.rs — shared helpers
pub fn color(hex: &str) -> Option<egui::Color32>
pub fn color_or(hex: &str, fallback: egui::Color32) -> egui::Color32
pub fn fit_chars(ui: &egui::Ui, font: &egui::FontId, width: f32) -> usize

// src/targets.rs
pub const LINE_HEIGHT: f32 = 18.0;      // UNKNOWN
pub const BAR_HEIGHT: f32 = 4.0;        // UNKNOWN
pub const BAR_COLOR: egui::Color32;     // UNKNOWN
pub const TRACK: egui::Color32;         // UNKNOWN
impl Target {
    pub fn named(&self, ui: &egui::Ui, look: &TargetsLook) -> String
    pub fn word_in<'a>(&'a self, ui: &egui::Ui, look: &TargetsLook) -> &'a str
    pub fn chars_in(&self, ui: &egui::Ui) -> usize
    pub fn ui(&self, ui: &mut egui::Ui, look: &TargetsLook)
}

// src/chips.rs
pub const DEFAULT_HEIGHT: f32 = 10.0;   // the figure Chip::bar writes
pub const TRACK: egui::Color32;         // UNKNOWN
impl Chip {
    pub fn ui(&self, ui: &mut egui::Ui)                     // no reading
    pub fn ui_at(&self, ui: &mut egui::Ui, value: Option<f32>)
}
impl Row { pub fn ui(&self, ui: &mut egui::Ui) }

// src/hotbars.rs
pub const CELL: [f32; 2] = [56.0, 26.0]; // UNKNOWN
pub const GAP: f32 = 4.0;                // UNKNOWN
impl Button {
    pub fn words(&self) -> String
    pub fn ui(&self, ui: &mut egui::Ui) -> egui::Response
}
impl Hotbars { pub fn ui(&self, ui: &mut egui::Ui) -> Vec<String> }  // commands to send

// src/pad/wheel.rs
pub const ARC_STEP: f32 = 10.0;   // UNKNOWN
pub const LABEL_AT: f32 = 0.6;    // UNKNOWN
pub const LABEL_SIZE: f32 = 12.0; // UNKNOWN
pub const SLICE_COLOR: egui::Color32; // UNKNOWN
pub const SLICE_INK: egui::Color32;   // UNKNOWN
pub fn turn(degrees: f32) -> egui::Vec2
pub fn on_wheel(centre: egui::Pos2, radius: f32, degrees: f32) -> egui::Pos2
pub fn widths(view: &View) -> Vec<f32>
pub fn wedges(view: &View, centre: egui::Pos2, radius: f32) -> Vec<egui::Shape>
pub fn labels(view: &View, centre: egui::Pos2, radius: f32) -> Vec<(egui::Pos2, String)>
pub fn paint(painter: &egui::Painter, view: &View, centre: egui::Pos2, radius: f32)
pub fn ui(ui: &mut egui::Ui, view: &View, side: f32) -> egui::Rect

// src/disk.rs — the recovered texture cache
impl Disk { pub fn kept<T>() -> Kept<T> }
pub struct Kept<T>;  // BTreeMap<key, T>
impl<T> Kept<T> {
    pub fn new() -> Kept<T>
    pub fn kept_at(&mut self, key: &str, make: impl FnOnce() -> T) -> &T
    pub fn get(&self, key: &str) -> Option<&T>
    pub fn insert(&mut self, key: &str, value: T) -> Option<T>
    pub fn remove(&mut self, key: &str) -> Option<T>
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
    pub fn keys(&self) -> impl Iterator<Item = &String>
    pub fn clear(&mut self)
}
impl Kept<epaint::TextureHandle> {
    pub fn texture(&mut self, ctx: &egui::Context, key: &str,
                   image: impl Into<egui::ImageData>,
                   options: egui::TextureOptions) -> &epaint::TextureHandle
}
pub type Textures = Kept<epaint::TextureHandle>;
pub type Pictures = Kept<(epaint::TextureHandle, std::sync::Arc<[f32]>)>;

// src/seat.rs
pub mod windows { pub trait Windows { fn open_url(&self, url: &str); fn repaint(&self); fn repaint_seat(&self, seat: u32); } }
pub const PLAY: &str = "play";
pub struct EguiWindows<'a>(pub &'a egui::Context);   // impl windows::Windows
// kept as-is: pub struct Windows { pub open: Vec<String> }

// src/banner.rs, src/lookout.rs — the settings pages
impl banner::Row { pub fn ui(&self, ui: &mut egui::Ui) }
impl lookout::TriggerRow { pub fn ui(&self, ui: &mut egui::Ui) }  // delegates to Row::ui
```

### The tests, and the values they pin

The draw tests share `src/testkit.rs` (`#[cfg(test)]` only): one frame over a
fixed `RawInput` (`screen_rect` 800×600) and readers for the resulting
`ClippedShape`s — `words`, `text_colours`, `rect_fills`, `polygons`,
`angle_of`, `folded`.  Every assertion is a value, never "it did not panic".

| test | values asserted |
| --- | --- |
| `paint::colours_parse_like_the_theme_does` | `#ff0000` → `Color32::from_rgb(255,0,0)`, `00ff00`, `#00f` → blue, `#00000080` alpha 128, `nope` → `None` |
| `paint::a_thumb_measures_the_width_it_is_given` | `fit_chars(…, 100.0) == (100.0 / glyph_width).floor()`, monotone in the width, `0` at width `0` |
| `targets::named_says_the_name_and_the_hint` | `named` = `"a grey wolf stunned, 60%"` (default look) / `"grey wolf stunned, 60%"` (`Fit`); `word_in` = `"grey wolf"` at 800 points; `look.word(t, 4) == "wolf"` |
| `targets::u_i_draws_the_name_the_hint_and_the_health_bar` | two text shapes `["grey wolf", "stunned, 60%"]`, both in the `stunned` status colour `#cc8844`; track rect `(0,14)…(800,18)`, fill width `480 ± 0.5`, fill colour `#b03030` (the 50 % threshold), both `BAR_HEIGHT` tall |
| `targets::a_target_with_no_health_has_no_bar` | one text shape, **no** rects |
| `targets::the_fit_width_follows_the_window` | at 800 points the name is `"grey wolf"`, at 20 points the noun `"wolf"`; the bar is `800`/`20` wide |
| `chips::u_i_draws_the_bar_the_height_and_the_words` | track `(0,0)…(800,12)` in `TRACK`; fill `320 ± 0.5` wide, 12 tall, `#b03030`; text `"40%"` in the chip's `#404040` |
| `chips::the_fill_is_clamped_and_the_thresholds_step` | `400 %` → fill 800 wide in `#2e8b57`; `-5 %` → track only; no reading → `"--"` and one rect |
| `chips::a_chip_without_a_bar_is_only_its_number` | `bar = false`, `percent = false` → text `"42"`, no rects; `height = 0` → `DEFAULT_HEIGHT` |
| `chips::a_gathered_row_draws_its_own_colour` | the row's `#123456` becomes the fill; text `"70%"` |
| `hotbars::u_i_draws_a_cell_per_button_at_the_cell_size` | each `response.rect.size() == (56, 26)`; galleys `["heal","look","fight"]` in bar-name order |
| `hotbars::a_button_that_sends_nothing_is_insensitive` | a nested `hotbar …` command → not clickable, still `(56, 26)`, still says its label; a label-less button says its command's first line |
| `hotbars::pressing_a_drawn_button_sends_its_command` | three frames of a real context: the frame's words are `["asides","heal","main","look","fight"]`; a press alone sends nothing; the release sends `["cast heal"]` |
| `pad::wheel::a_wedge_covers_its_own_slice_of_the_circle` | two 180° slices → 20 points each (`centre + 18 edges + start`), every rim point at r = 50 ± 0.01, every point within ±90° of its centre angle; fills `#b03030` and `SLICE_COLOR` |
| `pad::wheel::the_labels_sit_on_their_own_slices` | labels `["look","ring"]` at `(100 ± 30, 100)` |
| `pad::wheel::the_drawn_wheel_and_its_hit_test_agree` | the middle of each painted wedge hits its own seat through `View::over` (= `seat_at_angle`) |
| `pad::wheel::u_i_paints_the_wheel_in_the_middle_of_its_square` | two polygons centred on `(100,100)` with all rim points 100 out; words `["look","ring"]` |
| `disk::kept_at_builds_once_and_keeps_the_value` | the builder runs once for three calls; `len`/`keys`/`insert`/`remove`/`clear` values |
| `disk::a_kept_texture_is_the_same_texture_twice` | two `Kept::texture` calls in one frame give the **same** `TextureId`, `handle.size() == [2,2]`, the id is in `textures_delta.set`, and removing it empties the cache |
| `disk::a_copy_of_the_cache_keeps_the_same_handle` | `clone()` hands back the same `TextureId`; the clone compares equal |
| `seat::egui_windows_opens_urls_in_a_new_tab` | one `OutputCommand::OpenUrl` with `url` and `new_tab == true` |
| `seat::egui_windows_repaints_a_seat_and_everything` | after `repaint_seat(3)`: `has_requested_repaint_for(ViewportId::from_hash_of(("play",3)))` and not for `("play",4)`, root repaint requested; the cause is from `seat.rs`; `repaint()` wakes only the root |
| `banner::a_row_draws_its_label_then_its_value` | label left of value on one line, 12 + 2 shapes for six rows, notes only where they exist |
| `lookout::a_trigger_row_draws_its_label_and_value` | words `["Words", "", note…, "Sound", "off", "Sound and banner.", "Banner", "off", "Seconds", "0"]` |

Two limits worth knowing when reading those tests:

* `Context::repaint_causes` reports the *previous* pass, and egui drops a child
  viewport that was never shown, so `repaint_seat`'s viewport request is read
  back **inside** the frame that made it (the test says so).
* the click test needs three frames on one context (find the cell, press,
  release), which is the cheapest honest way to prove the wiring.

### Still UNKNOWN

* Every geometry constant marked **UNKNOWN** above (`LINE_HEIGHT`,
  `BAR_HEIGHT`, `CELL`, `GAP`, `ARC_STEP`, `LABEL_AT`, `LABEL_SIZE`) and the
  colours `TRACK`, `BAR_COLOR`, `SLICE_COLOR`, `SLICE_INK`.  The binary paints
  these in `hydra_gui`, not here, so its numbers are in that crate's evidence.
* Which glyph `fit_chars` should measure: the binary's own measuring call was
  inlined past recognition; `x` is used.
* `Chip::ui` without a reading.  The recovered `Chip` has no field for the
  current value (the binary keeps `current` outside the file it reads), so
  `Chip::ui(ui)` draws the empty state and `Chip::ui_at(ui, value)` is the one
  that draws a reading.
* `seat::Windows::repaint_seat`'s argument: a `u32` (the disassembly stores 32
  bits into the hash input).  What it counts — seat index or character id — is
  UNKNOWN; it is only ever salted into the viewport id.

### Departures not struck

* `hydra_ui::story::Story` → `seat::Story` stays.  `Story` is still a local
  model; reconnecting it means depending on `hydra_ui`, which this crate must
  not do.
* the settings pages → `banner::Row` stays a *minimal* row: it now draws
  (`Row::ui`, a two-column `Grid`), but it is not `hydra_ui`'s `Page`.

---

## Tests

160 tests, all asserting recovered values (constants, file formats, command
strings, angle and layout maths, parsing round-trips, defaults) — 24 of them the
headless draw tests above.  Run with `cargo test -p hydra_board`.
