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
| `hydra_gui`'s wheel editor calling `seat_at_angle` | the function is recovered; `View::seats`/`over` are the local way of building its input. |
| the settings pages (`hydra_ui::*::{Page, Row}`) | `banner::Row` is a minimal row type; `Own::pages`, `BannerFile::page`, `lookout::{page, with_trigger_row}` return it. |
| `OnceLock`/`LazyLock`/`Mutex` fields | kept as-is where the accessors prove them (`Seat::log`, `Seat::team`, `Seat::rumbles`, `Seat::story`). |

Dependencies added to `crates/hydra_board/Cargo.toml`: `hydra_protocol`,
`hydra_model` (for `GameState` in `pad::rumble::Watch::moments`) and
`hydra_session` (for `SessionHandle`, `Notice`, `Step`, `PlayerLog`, `Clip`,
`Notes`).  No external crates, and no `hydra_gui`.

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

## Tests

136 tests, all asserting recovered values (constants, file formats, command
strings, angle and layout maths, parsing round-trips, defaults).  Run with
`cargo test -p hydra_board`.
