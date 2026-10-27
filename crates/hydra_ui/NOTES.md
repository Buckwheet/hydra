# `hydra_ui` — reconstruction notes

Rebuilt from the arm64 `hydra` binary (no DWARF). Everything below was read out
of `__TEXT,__text` (function bodies), `__TEXT,__const` (packed literals and
tables) and `__DATA_CONST,__const` (relocated pointer tables). Legend:

* **established** — the data/constant/algorithm is transcribed from the binary.
* **INFERRED** — the code is the recovered behaviour, but the exact Rust shape
  (types, field order) is a reconstruction.
* **UNKNOWN** — the body did not yield the detail; the shipped behaviour is the
  safest non-invented one and the module says so at the call site.

`todo!()`/`unimplemented!()` do not appear anywhere in `src/`.

## Dependencies

Two sibling crates are used rather than duplicated, because the binary calls
into them from `hydra_ui`:

* `hydra_protocol` — `runs::{Run, Runs}`, `frame::payload::{Link, LinkTarget}`
  (seen at the `story`/`run`/`hotbar`/`object_menu` call sites).
* `hydra_model` — `state::GameState`, `state::bounty::{Task, TaskKind}` (seen in
  `condition::Condition::holds` @0x10183c98c and `bounty::brief` @0x10185afc4).
* `gs_base` — `elanthian_time` for the US-Eastern offset `DayPhase::at` and
  `story::stamp` need.
* `egui` + `epaint` — pinned to the binary's own git rev
  (`3da022d`, see `docs/EGUI.md`). Phase 4 uses them for drawing only:
  `Ui`, `Painter`, `Color32`, `FontId`, `TextFormat`, `LayoutJob`, `Rect`,
  `Shape`.  No `eframe`, no `winit`, no `wgpu`: this crate never opens a window.

`serde`, `toml_edit`, `image`, `regex`, `jiff`, `phf` and `anyhow` are **not**
available. Each is replaced by a documented std substitute:

| original | substitute |
|---|---|
| `serde` + `toml`/`toml_edit` | `src/toml.rs` (hand-written reader/writer), `src/layer.rs` (`Over`: `over`/`differing`/`is_empty`) |
| `egui` drawing | **Phase 4 removed this departure**: the seams are real `epaint` painting now — see "Phase 4: drawing is back" below. The pure maths (`view::PlayView::project`, `frame::lay`, `hotbar::Bar::grid`, `story::Story::painted`) is unchanged and still the source of truth. |
| `regex` | `highlight::matcher::Pattern` — literal needle with case folding, whole-word and `^`/`$` anchors |
| `jiff` | `gs_base::elanthian_time` for the Eastern offset |
| `phf` | the emoji map is reproduced as a sorted table + binary search |
| `anyhow` | `Result<_, String>` |

## Established data

### Theme tokens (95) — `src/theme.rs`

* `Token::name` @0x101842e08: 95 `&str`s (lengths @0x103992420, pointers
  @`__DATA_CONST` 0x104447a58).
* `Token::bare` @0x101842dd0: three 95-byte tables at 0x1039922ff (blue),
  0x10399235e (green), 0x1039923bd (red). The packed word is
  **`r | g << 8 | b << 16`** (an `egui::Color32` minus the alpha byte) — proven
  by `oklch::contrast`, which reads `r` from the low byte, and by
  `Palette::bare`'s byte stores.
  Cross-checks: `health` #cd4d4d, `mana` #4784d9, `blood` #8b1a1a,
  `wound3` #ff0000, `veil` #000000, `grid` #ffffff, `mark` #ffff00.
* `Token::role` @0x101842e28: a 32-byte enum per token. The discriminant word is
  0, 1, 2, 3, 4 or 5; the payloads were decoded from the 57 distinct arms of the
  dispatch table at 0x1039919b8. Mapping (see `Role`):
  `0` = "under surface *n*, lightness delta *d*" (21 tokens), `1` = hue
  rotation, `2` = absolute OKLCH lightness (0.5…0.75), `3` = lightness lift,
  `4`/`5` = fixed.

### Palette — `src/theme.rs`

`Palette` is 304 bytes (`Palette::bare` @0x101847978 copies exactly 0x130):
`names: Arc<BTreeMap<String, Rgb>>` @+0x00, `prompt: Arc<BTreeMap<u32,u8>>`
@+0x08, `bare: [Rgb; 95]` @+0x10. The four default prompt accents are
`(82, 15)`, `(33, 15)`, `(83, 14)`, `(72, 12)` — written as immediates at
0x101847ae4. `Palette::prompt` @0x101847cb4 defaults to `2`.

### Colour maths — `src/theme/oklch.rs`

`rgb_to_oklab` @0x101871c54, `from_lch` @0x1018723ac and `contrast` @0x101871fa4
use the published Oklab matrices; every `fmov` immediate was decoded, so the
constants are the binary's exactly (0.04045/0.055/1.055/2.4/12.92, the M1 and M2
rows, the inverse rows, 0.2126/0.7152/0.0722 and the 0.05 contrast knee).
`contrast` = `(max + 0.05) / (min + 0.05)`, so identical colours give 1.0 and
black/white gives 21.0 exactly. OKLCH hues are **degrees**.

### Harmony — `src/theme/harmony.rs`

* `Scheme::parse` @0x10194da6c returns discriminants 0,1,2,3,4,5,6,8 for
  `monochrome analogoous…` — specifically `monochrome`(10), `analogous`(9),
  `complementary`(13), `split`(5), `triadic`(7), `tetradic`(8), `golden`(6),
  `compound`(8).
* The scheme hue offsets are a real table: lengths @0x1039bea50, `f64` arrays
  pointed at from 0x104490288 —
  `[0]`, `[0,30,-30,60,-60]`, `[0,180]`, `[0,150,210]`, `[0,120,240]`,
  `[0,90,180,270]`, `[0,137.5,275,52.5,190,327.5]`, `[0,30,180,210]`, `[]`.
  Slot 7 is unreachable from `parse`; slot 8 (compound) is empty.
* `hue_for` @0x10194e328: for a non-`Under` token, the OKLCH hue of the token's
  own colour; for an `Under(base, d)` token,
  `normalise(seed_hue + offsets[base % len] * d)`.
* Built-in themes (`Theme::light` @0x101948518, `Theme::despana` @0x101949a64):
  **Light** — seed `#477ab3`, background `#f4f1ea`, scheme `triadic`;
  **Despana** — seed `#d7ad63`, background `#0d1115`, scheme `triadic`.

### Conditions — `src/condition.rs`

* 26 variants (`variant index 0 <= i < 26`), 21 struct variants with the field
  counts transcribed from 0x103993e4c; the field-name pool is
  `condition conditions name_match cmp seconds id active value level number
  phase item_type count hostile percent vital`.
* `Cmp`, `DayPhase`, `HandSlot`, `NameMatch`, `VitalKind`, `VitalUnit`,
  `ResourceKind`, `EffectCategory`, `TargetKindName` variant names are
  transcribed (`EffectCategory::ActiveSpells` serialises as `Active Spells`).
* `DayPhase::at` @0x10183c5b4 resolves `America/New_York` and buckets the local
  hour with the recovered bit test: `hour & 0x1e == 6` → dawn (hours 6–7),
  `8..=17` → day, `hour & 0x1e == 0x12` → dusk (hours 18–19), else night.
  The error path returns the out-of-range discriminant `4`; here that is `None`.
* `Flags::is_up` @0x10183c460: lowercased `BTreeMap` lookup; value word 0 bit 0
  means "needs a level", word 1 is the level compared with `<`.
* `NameMatch::matches` @0x10183dd58: lowercase both, then `==` or `contains`
  with the **first** argument as the haystack.
* `condition::prepared` @0x10183c8c4: trim, and a case-insensitive `"none"` is
  absent.

### Highlight rules — `src/highlight/`

Rule field names transcribed from the pool (`regex case_sensitive whole_word
color squelch substitute status sound volume alert notify speak rumble picture
timer seconds caught times rearm character stream origin off window spans`).
`HighlightFile` has `categories`, `highlight`, `widgets`.
`TabStyle::named` @0x101843be4 decodes to `plain`(0) `separated`(1) `boxed`(2)
`filled`(3) with `4` as "no match".

### Emoji — `src/emoji.rs`, `src/emoji_table.rs`

`expanded` @0x101939150 scans for `:shortcode:` where the body is
`[A-Za-z0-9_+-]+`, lowercases it and looks it up in a
`phf::map::Map<&str,u32>` @0x10449bdc8 (**1913** entries, 383 displacements)
whose value indexes a 3944-slot table of `&str` cells at 0x104452338 (stride 64).
Both tables were read out of the binary; `emoji_table.rs` holds all 1913
`(shortcode, glyph)` pairs sorted, and the lookup is a binary search. The
return type is `Option<String>` because the binary keeps `isize::MIN` in the
result capacity when nothing changed.

### Other established tables

* `channel_line::CHANNELS` — the nine `[Name] -> token` pairs at 0x103995f04.
* `creature_line::CREATURES` — the 21 `adjective+noun`/`noun` pairs at
  0x1039929c4.
* `perf::STAGES` — the eight instrumentation stages and their descriptions
  (0x103994328).
* `frame::Measures` — `bar overhang corner run` (0x1039be7ee); `lay`'s guards
  (overhang>0, scale>0, corner>0, w>0), `k = scale / w`, run floored at 1.0.
* `hotbar::icon` — `Sheet::cell` @0x1018d54cc returns the cell size when both
  words are non-zero, else **64**.
* `theme::wrayth::PRESET_MAP` — the nine `presetX` names and their tokens
  (0x10399213f), and the recovered error strings.
* `highlight::sound::SOUND_EXTENSIONS`, `highlight::file::CATEGORIES`.
* `theme::shape` — the fourteen `[shape]` field names and the three style
  triples (`flat/outlined/filled`, `tight/normal/roomy`, `plain/separated/boxed`).
* `theme::typeface` — the eight `[type]` field names; `Face` layout from
  `Face::under` @0x101951438 (`Option<String>` @0x00, `u32` @0x18, `f32` @0x1c).

## Phase 4: drawing is back

`docs/EGUI.md` §3.6 recorded that **no egui call edge survives** from `hydra_ui`
(one ICF-residue drop glue, no calls) and asked for `Ui`/`Painter`/`Color32` only
when drawing returned here.  That is what this section did.  Nothing was
re-invented: every entry point below is the *same* recovered maths/data, with an
`epaint` layer on top.

Design rule applied throughout: **the pure function stays the single source of
truth**, and the drawing function only converts its arguments and its result.
`frame::lay` still decides the geometry, `hotbar::Bar::grid` still decides the
grid, `story::Story::painted` still decides the token of every run, and
`view::PlayView::project` still decides the text.  Every new entry point is
*additive* — no existing public item was renamed or removed.

### `src/paint.rs` — the `epaint` bridge (new module)

| item | what it does | recovered basis |
|---|---|---|
| `color(Rgb) -> Color32` / `packed(Color32) -> Rgb` | unpack/pack `r \| g << 8 \| b << 16` | `Token::bare` @0x101842dd0 (`r` is the low byte — `oklch::contrast` reads it that way) |
| `ink(&Palette, Token) -> Color32` | a token's colour, ready to paint | `Palette::get` @0x1018478b8 |
| `accent(&Palette, at) -> (Token, Color32)` | the prompt accent for `at` | `Palette::prompt` @0x101847cb4 (+ its `mov w0, #2` default) |
| `format(&Palette, Token, FontId)` | a `TextFormat` in a token's colour | — |
| `families(&Ui)` | the families the context bound | `epaint`'s `"FontFamily::… is not bound to any fonts"` panic |
| `face(&Face, fallback, bound) -> FontId` | a `[type]` face as a `FontId` | `theme::typeface::Face` (`size == 0.0` means "unset") |
| `faces(&Theme) -> Type`, `ui_font`, `story_font`, `line_spacing` | the theme's `[type]` section | `TypeFile::resolve` + `Face::under` @0x101951438 |

`proportional`/`monospace` resolve to egui's two built-in families; any other
name is a `FontFamily::Name` and is **only** used if the app bound it (otherwise
the fallback family is kept and just the size applies) — an unbound named family
panics inside `epaint`.

### `view::PlayView::{ui, ui_with, described}` (+ `view::Laid`)

* `ui(ui)` = `ui_with(ui, &Theme::default())` — the built-in Light theme.
* `ui_with(ui, &Theme)` draws, in order: `ui.heading` for the room name, one
  `ui.colored_label` per description line, a `ui.horizontal` row of
  `egui::Button`s for the exits, then the prompt.
* Colours (all exact palette values): room name `Token::RoomName` **#d7ad63**,
  description `Token::RoomDesc` **#b4b4b4**, exit words `Token::Link`
  **#477ab3**, prompt `Token::Command` **#477ab3**.
* `described()` is the description with the `Obvious exits:` / `Obvious paths:`
  caption removed — `exits_in` already turned it into the exit row, so drawing
  both would repeat the words.  The model keeps the description verbatim.
* `Laid { title, description: Vec<Rect>, exits: Vec<(String, Rect)>, prompt }`
  is returned for hit-testing (and for the tests).

### `story::Story::{paint, paint_with, paint_into}` (+ `story::Ink`, `Painted`, `PaintedLine`, `Section`)

* `paint(ui)` = `paint_with(ui, &Ink::of(&Theme::default(), ui))`.
* `paint_with(ui, &Ink)` claims `ui.available_rect_before_wrap()` (the story
  window is the one that scrolls) and forwards to
  `paint_into(painter, rect, &Ink)`.
* `paint_into` lays **one `LayoutJob` per line** with one `TextFormat` section
  per run (`ui.painter().layout_job`), so a line keeps its per-run colours in a
  single galley, and paints it with `Painter::galley`.  A run's colour is
  `Palette::get` of its `token` (the run's own `color` only when it has no
  token) — the token colours are recovered data and are used exactly.
* Bottom-anchored: the newest line sits on the bottom edge and the log grows
  upwards, keeping the newest lines when the box fills up.
* `typing == false` (scrolled back) hides the `missed` newest lines and paints
  the count bottom-right in `Token::Warning` **#d7ad63**; `typing()` clears it.
* `Ink::prompt(text)` paints the prompt as the **final** line in
  `Token::Command` **#477ab3** — the token `StyledRun::prompt` splits at and
  `run::prompt_coloured` paints the prompt with.
* `Ink::highlighted(&Matcher)` splits each run at `highlight::spans` (new: the
  `matcher::caught` @0x1019659e8 span of every firing rule) and paints a covered
  piece in the rule's own `color`; a rule without `color` leaves the token
  colour alone.  No regex behaviour was invented — the spans are the matcher's.
* `:shortcode:` emoji are expanded with `emoji::expanded` @0x101939150 before
  layout (the model text is untouched).

### `hotbar::Bar::{ui, ui_with, cell_rect, size}` (+ `hotbar::Look`, `Grid`, `Cell`)

* `ui(ui)` = `ui_with(ui, &Look::new(&Palette::bare()))`.
* `ui_with` allocates `Bar::size()` (`columns * button_width` by
  `rows * button_height` from `grid()` @0x1018d4fa4) and paints one cell per
  button at `cell_rect(row, column)` — `x = column * button_width`,
  `y = row * button_height`.
* Per cell: `rect_filled` in `Button::look` @0x1018d5810's `bg`
  (`Token::Raised` **#464646** when unset), `rect_stroke` in `Token::Line`
  **#3c3c3c**, a `rect_filled` icon box of `icon_size` points centred in the cell
  in `Token::Inset` **#0a0a0a** (there are no textures in this crate, so the
  label is painted inside that box), the label in `look`'s `fg` (`Token::Text`
  **#b4b4b4**), and — for a button that has a `countdown` — `countdown_text`
  bottom-right in `Token::Warning` **#d7ad63**.  `dim` halves the fill with
  `Color32::gamma_multiply(0.5)`.  A state style (`roundtime`/`casttime`/
  `effect`) wins over `default_style`, exactly as `Button::look` resolves it.
* `Grid { rect, cells }` returns every `Rect`, icon box, caption, command and the
  per-cell `clicked` flag, so a caller can hit-test without recomputing.

### `frame::{framed, painted, titled}` (+ `frame::Framed`, `Painted`, `Size::{rect, of}`, `Frames::inner_rect`)

* `framed(&Measures, scale, Rect)` = `lay` in `egui::Rect` terms: `inner` is
  `Frames.inner`, `bar` is the top strip of `outer` of height `Frames.bar`.
* `painted(&Painter, &Framed, &Palette)` draws the bar in `Token::Raised`
  **#464646**, the outline as a 1-point `Token::Line` **#3c3c3c** stroke, and the
  four carved runs (`frames.run` long, `frames.corner` in from each corner) in
  `Token::LineStrong` **#969696**.
* The recovered guards are respected: when `lay` returns its identity
  (`overhang <= 0 || scale <= 0 || corner <= 0 || w <= 0`) `Frames::is_degenerate`
  is true and `painted` draws **nothing** — no shapes at all.
* `titled(..)` puts a title in the bar where `[shape] title_align`
  (`left`/`centre`/`right`) asks, in `Token::Text` **#b4b4b4**.

### What is *not* drawn (and why)

* `hotbar::Bar::title` — the window frame draws titles (`frame::titled`); the
  binary's hotbar caption lives in `hydra_gui::bar`.
* No textures: `hotbar::icon::Sheet`/`source` name the PNG and `hydra_gui` owns
  the `TextureHandle` cache, so an icon is a filled placeholder box here.
* Scrollback scrolling (`ScrollArea`) is `hydra_gui`'s; `paint_into` takes the
  rectangle it is given and clips to it.

### Phase-4 `UNKNOWN:` items

* The **missed-count badge** — `Story::painted` @0x101858a5c keeps the count but
  its 924-byte body is clock arithmetic; where the number goes is a drawing
  decision.
* The **frame run edges** — that a run is carved from all four client edges is
  the chosen reading; the disassembly gives the length and the corner offset
  only.
* **`Face::weight`** — egui's `FontId` has no weight, so it is ignored rather
  than turned into a stretch or a second family.
* **`Palette::prompt` accents are not used for the prompt line** — its default
  index is `2` (`Token::Raised`, a surface colour), so the prompt is painted in
  `Token::Command`, which is what `StyledRun::prompt` and `prompt_coloured`
  actually key on. The accent table is still exposed (`paint::accent`).
* `Story::prompted` @0x10185a7f0 (which pushes the prompt into the scrollback)
  is **not** modelled: `Story` has no `prompt`/`drawn` pair here, so the prompt
  arrives through `story::Ink::prompt` instead. The model is unchanged.

## `UNKNOWN:` items (each also marked at the call site)

* `theme::Palette::prompt`'s value width — the map is `u32 -> u8` as written by
  `Palette::bare`, but the reader loads eight bytes. Modelled as `u8`.
* `harmony::generate` — the 12 KB body interleaves per-role arithmetic with
  gamut mapping; the roles, the hue arithmetic and the scheme tables are exact,
  the `variance`/`contrast`/`separation`/`room_spread` blend is not.
* `harmony::seed_swatches` — the two calls are visible (`0.05` then `0.0`), the
  predicate between them is not.
* `theme::file::Chosen`/`File`/`PromptFile` — field *names* are recovered
  (`name base recipe pins names prompt shape type frame`,
  `struct variant PromptFile::Both`), the exact nesting is inferred.
* `theme::shape::CornersFile` — the type name survives, no field names do.
* `theme::wrayth` / `highlight::vellum` / `highlight::wrayth` — the importers are
  hundreds of KB of inlined grammar; the scanners here accept the attribute
  spellings the recovered literals name and reject everything else.
* `condition::Condition::holds` — the 8.5 KB body calls into `hydra_model` for
  every value; the branches whose accessor is not recoverable (injuries, target
  kinds, resources, spell affordability) answer `false`.
* `highlight::matcher` — `regex` is unavailable; `Pattern` is the documented
  subset. `fired_alone`, `has_stream`, `timer_default` (1 second) are exact.
* `story::*` — `Story`'s line storage and the `Shown`/`Heard`/`Call` shapes are
  inferred from drop glue and the `"lookout"`/`"combat"` literals.
* `hotbar::Countdown::left` (78 KB) — only the clock arithmetic is reproduced.
* `inventory`, `sorter`, `merge`, `macros`, `settings`, `history` — the field
  names and the module split are recovered; the 10–70 KB bodies (which inline
  the game's word lists) are reduced to the observable rules.
* `view::exit_word` returns the caller's slice for unknown words (the binary's
  48-byte trampoline); `PlayView::project` is text-only.

## Tests

**244** `#[cfg(test)]` tests. The original 211 assert recovered literals rather
than tautologies: the 95 token colours and roles, the packed colour convention,
all `parse_hex` forms, the OKLCH constants and round-trips,
`contrast(white, black) == 21.0`, the eight scheme offset tables and their
discriminants, both built-in themes' seed/background/scheme, the day-phase hour
buckets, the emoji table's sort order and a sample of glyphs, the 21 creature
names, the nine channels, the eight perf stages, `Measures` parse/round-trip and
`lay`'s maths, the four prompt defaults, and the parsers for themes, highlights,
hotbars and macros.

### Phase-4 drawing tests (33 new, no window is ever opened)

Every drawing test builds `egui::Context::default()`, feeds one fixed
`RawInput` (`screen_rect = (0,0)-(800,600)`), runs one frame with
`ctx.run_ui(input, ..)` and asserts on the resulting shapes or on the rectangles
the painting reports. **No** test only checks "it did not panic".

| module | test | exact values asserted |
|---|---|---|
| `paint` (5) | `packed_colours_round_trip` | `color(0x001b1b1b) == Color32::from_rgb(0x1b,0x1b,0x1b)`; `health` `#cd4d4d`, `mana` `#4784d9`; `packed(color(row.3)) == row.3` for all 95 rows |
| | `ink_is_the_palette_colour_of_the_token` | `ink(&bare, t) == color(t.bare())` for all 95 tokens |
| | `prompt_accents_come_from_the_palette` | `accent(82) == Wrong`, `accent(83) == Warning`, `accent(72) == Link`, `accent(0) == Raised` |
| | `a_face_uses_the_themed_size_and_only_bound_families` | size-only `18/Proportional`; unbound `PragmataPro` → `20/Proportional`; `Monospace` → built-in `Monospace`; a bound custom family is used; an empty face is the fallback |
| | `formats_carry_the_token_colour` | `warning` `#d7ad63` |
| `frame` (5) | `framed_is_lay_in_rect_terms` | `frames == lay(..)`; `inner == Rect::from_min_size((12,42),(396,276))`; `bar == ((10,20),(400,20))`; `Size::of(rect).rect() == rect` |
| | `framed_keeps_the_run_floor` | `run == 1.0` for `run = 0.1`; a zero-width rect degenerates |
| | `the_frame_paints_bar_outline_and_four_runs` | 2 rects: bar `#464646` at the bar rect and the outline `#3c3c3c` at `outer`; 4 segments `#969696` 1.0 wide at `(14,42)-(22,42)`, `(14,320)-(22,320)`, `(10,46)-(10,54)`, `(410,46)-(410,54)` |
| | `a_degenerate_frame_paints_nothing_at_all` | `Measures::default()` → no bar/outline/runs and `out.shapes.is_empty()` |
| | `the_title_sits_in_the_bar_where_the_theme_asks` | one text `"Hydra"` `#b4b4b4`; left-aligned `min.x == 12.0`; centre is right of left; no bar → `None` |
| `story` (8) | `a_line_is_painted_in_its_token_colour` | `lines[0].text() == "A dark cave."`, token `Text`, colour `#b4b4b4`, `rect.bottom() == 100.0`, and the same text/colour in the frame's galley |
| | `each_run_keeps_its_own_token_colour_inside_one_galley` | two sections `"a "` `#b4b4b4` and `"troll"` `#bd8cff` (`creature`) |
| | `the_prompt_is_the_last_line` | prompt `"s>"`, token `Command`, `#477ab3`, `rect.bottom() == box.bottom()`, above it the last story line, and it is the **last** of the 3 text shapes |
| | `scrolled_back_lines_are_not_drawn_and_the_count_is` | `typing == false`, `missed == 2` → 1 line (`"first"`), badge `(2, right - 2.0)`, badge text `"2"` in `#d7ad63`; `typing()` → 3 lines and no badge |
| | `a_full_box_keeps_the_newest_lines` | 40 lines in a 40-point box → fewer than 40, newest `"line 39"` on the bottom edge, nothing above the box |
| | `highlight_spans_are_painted_in_the_rules_colour` | sections `["an ", "orc", " arrives"]`, the match `#ff0000` from the rule's `color`, the rest `#b4b4b4`; a rule with no colour leaves `#b4b4b4` |
| | `shortcodes_are_expanded_before_painting` | `"hi :smile:"` paints `"hi \u{1f604}"`; `Story::text` still gives `"hi :smile:"` |
| | `paint_claims_the_available_box` | `paint(ui)` uses the whole available rect; the only line ends on its bottom edge |
| `hotbar` (7) | `cells_are_placed_by_the_recovered_grid_maths` | `grid() == (3, 40.0, 30.0)`, grid `80×90`, every cell `== cell_rect(origin,row,column)`, cell 5 at `origin + (0,60)`, cell (1,1) at `origin + (40,30)`; 15 rects: fill `#464646`, outline `#3c3c3c`, icon box `#0a0a0a` — all at the exact rects |
| | `every_button_paints_its_recovered_label` | captions `["north","south","east","west","up"]`, all `#b4b4b4` |
| | `a_state_style_and_a_countdown_recolour_the_cell` | `roundtime` label `"wait"` `#ff0000`, fill `#000080`, one `"7"` readout in `#d7ad63` at `cell.right()-1`, `cell.bottom()-1`, and no readout on the button without a countdown |
| | `a_dimmed_button_is_the_same_colour_halved` | `#464646` then `#464646.gamma_multiply(0.5)` |
| | `a_cell_that_a_button_does_not_fill_is_not_drawn` | 1 cell → 2 rects (no icon box when `icon_size == 0`), `button(0,1).is_none()` |
| | `a_bar_with_no_cell_size_draws_nothing` | `button_width == 0.0` → no cells, `rect == Rect::NOTHING`, `out.shapes.is_empty()` |
| | `ui_is_the_built_in_palette` | `Bar::ui` draws exactly what `ui_with(&Look::new(&Palette::bare()))` draws; `command == "north"` |
| `view` (6) | `the_description_drops_the_exit_caption` | `described() == [2 lines]`, the model still holds `"Obvious exits: …"` |
| | `the_view_draws_heading_description_exits_and_prompt` | text order `"A Dim Cave"` `#d7ad63`, two description lines `#b4b4b4`, no `"Obvious exits"` text, exits `["n","se","u"]` `#477ab3` in 3 button rects that advance rightwards, prompt `"s>"` `#477ab3` last and below the exit row |
| | `an_empty_view_draws_nothing` | `Laid::default()` and no shapes |
| | `the_theme_palette_is_what_colours_the_view` | writing `palette.bare[RoomName]` cyan turns the heading `#00ffff` |
| | `ui_is_the_built_in_theme` | `ui` and `ui_with(&Theme::default())` lay out identically, 3 exits |
| | `blank_description_lines_still_take_a_row` | 3 labels incl. `" "` for the blank line, and the row has height |
| `highlight` (2) | `spans_are_the_matches_of_every_firing_rule` | 2 spans in matcher order (`"a large orc"` 0..11 first, `"orc"` 8..11 second with its `#ff0000`), `&text[8..11] == "orc"`, nothing matches → empty |
| | `spans_respect_the_rule_flags` | `whole_word` fires on `"an orc"`, not on `"an orcish thing"` |

