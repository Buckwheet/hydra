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

`egui`, `serde`, `toml_edit`, `image`, `regex`, `jiff`, `phf` and `anyhow` are
**not** available. Each is replaced by a documented std substitute:

| original | substitute |
|---|---|
| `serde` + `toml`/`toml_edit` | `src/toml.rs` (hand-written reader/writer), `src/layer.rs` (`Over`: `over`/`differing`/`is_empty`) |
| `egui` drawing | no drawing at all: model/state/layout logic only (`view::PlayView`, `frame::lay`, `hotbar::Bar::grid`, `story::Story::painted`) |
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

211 `#[cfg(test)]` tests. They assert recovered literals rather than
tautologies: the 95 token colours and roles, the packed colour convention, all
`parse_hex` forms, the OKLCH constants and round-trips, `contrast(white, black)
== 21.0`, the eight scheme offset tables and their discriminants, both built-in
themes' seed/background/scheme, the day-phase hour buckets, the emoji table's
sort order and a sample of glyphs, the 21 creature names, the nine channels,
the eight perf stages, `Measures` parse/round-trip and `lay`'s maths, the four
prompt defaults, and the parsers for themes, highlights, hotbars and macros.
