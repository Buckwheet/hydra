# `hydra_model` — recovery notes

Recovered from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(demangled symbols in `/Users/cpmbpro/Code/hydra-inspect/symbols_demangled.txt`).
The crate has 85 source files, 280 `todo!()` stubs and 317 surviving symbols.
No `todo!()`/`unimplemented!()` remains
(`grep -R "todo!()\|unimplemented!()" crates/hydra_model/src` is empty).

**Phase 1 (dependency reconnection) is done for this crate**: the `regex` and
`serde` stand-ins are gone, see
[Phase 1 reconnection](#phase-1-reconnection-regex-and-serde). Remaining
std-only bits: the `ascii` rewrite is done at source level (no `regex-syntax`
pin) and text entry points still take `&str` rather than styled runs.

Status: `cargo check -p hydra_model --offline` and
`cargo test -p hydra_model --offline` pass (**341 tests**).

---

## Legend

| tag | meaning |
|---|---|
| **full** | signature, control flow and constants read directly out of the instruction stream; covered by tests with real values. |
| **partly** | the entry contract, literals and helper calls are recovered, but a data table is missing (regex captures are no longer a limitation), so the body implements the evidenced subset only. |
| **unknown** | the binary shows the call shape but not the behaviour. A safe value (`None`, empty, `false`) is returned, with a `// UNKNOWN:` comment at the site describing what *was* visible. |

Recovered tooling used (read-only):
`/tmp/hydra_recon/recon.py`, `/tmp/hydra_recon/cstrings.txt`,
`/tmp/hydra_recon/hydra_model_inventory.md`, plus a per-function
literal/call extractor built on the same driver.

---

## Type consolidation (duplicate declarations removed)

The stubs declared the *same* type once per file. Each was collapsed onto one
canonical definition and the other files now write
`impl crate::state::GameState { … }` / `impl crate::state::character::Character { … }`:

| type | canonical home | files that now only `impl` it |
|---|---|---|
| `GameState` | `state.rs` | `state/{clock,stun,vitals,spell_time,custom_timers,login,reconnect}.rs` |
| `Character` | `state/character.rs` | `state/character/{identity,stance,stats}.rs` |

Public module paths and type names are unchanged.

`GameState`'s field set is the recovered member types (see the struct), but only
two offset ranges are established from the binary: the clock block
(`+0x1118`…`+0x1150`) and the vitals map root/length (`+0x1178`/`+0x1180`).
`Character`'s field set is likewise module-derived.

---

## `std`-only departures

| original dependency | recovered use | what this build does |
|---|---|---|
| ~~`regex`~~ | `pattern::{ascii,compile,compile_gated}`, `MatchIndex`, every classifier's named captures | **RECONNECTED** to `regex = "=1.13.1"` (`{ workspace = true }`), see [Phase 1](#phase-1-reconnection-regex-and-serde). |
| ~~`serde`~~ (`serde_core::de::Visitor`) | the 30 `Deserialize` impls (one `expecting` symbol each) | **RECONNECTED** to `serde = "=1.0.229"` with `derive`, see [Phase 1](#phase-1-reconnection-regex-and-serde). |
| `regex-syntax` | `pattern::ascii` drove `ast::parse::Parser` + a `Visitor` (`Edits`) | **KEPT as a source-level rewrite**: `regex-syntax 0.8.11` is in the binary's crate list (it is `regex`'s own dependency) but is **not** a workspace pin, so it is not a direct dependency here. `pattern::ascii` reproduces the recovered edit set by scanning the source; see that function. |
| `tokio` | none observed in this crate | n/a |
| `egui`/`wgpu` | none observed in this crate (the drawing lives in `hydra_gui`) | n/a |
| `toml_edit` | none observed in this crate (`toml` is a *dev*-dependency for the wire tests only) | n/a |
| `hydra_protocol` (path dep, allowed) | `Runs`/`Run`/`Link` in `state/lines.rs`, `state/combat/target.rs` | used as-is. |
| `gs_base` (path dep, allowed) | `body::BodyPart` in `state/creatures/body.rs` | used as-is. |

---

## Phase 1 reconnection (`regex` and `serde`)

### `regex`

`Cargo.toml` now has `regex = { workspace = true }` (`=1.13.1`).
`src/pattern.rs` is a thin layer over the real engine:

* `compile` = `ascii` + `Regex::new` — exactly the recovered
  `0x101b8a0d8` (`bl regex::regex::string::Regex::new @0x101d8c61c`).
* `compile_gated` = `ascii` + `RegexBuilder::new` + `dfa_size_limit(0)` +
  `build` — `0x101b89800`; the limit argument is `x1 = 0` at `0x101b89830`
  and `regex` documents `0` as *no limit*.
* `Pattern` keeps its public shape (`source`, `case_insensitive`, `error`,
  `is_match`, `is_ok`; `PartialEq` is now hand-written because `Regex` is not
  comparable) and **gains captures**: `Pattern::{captures, capture,
  group_names, groups}` and the owned [`pattern::Groups`], which is the
  recovered `Captures::get_group_by_name` result set.
* `ascii`'s edit set is now *recovered*, not guessed (see the function's
  table): `\d \D \s \S \w \W` → `[0-9]` / `[^0-9]` /
  `[\t\n\x0B\x0C\r ]` / `[^\t\n\x0B\x0C\r ]` / `[0-9A-Za-z_]` /
  `[^0-9A-Za-z_]`, and `\b` → `(?-u:\b)`. A `(?u)` flag group is **not**
  stripped (there is no `Ast::Flags` branch); the old build's
  "drop a leading `(?u)`" was an invention and is gone.

Consequences for behaviour:

* Syntax the hand-written matcher rejected is compiled for real: `\b`, `\p{…}`,
  `{n,m}`, nested classes, … `Pattern::is_ok` therefore answers `true` for
  them now. The two tests that pinned the *stand-in's* limits
  (`\p{L}` must fail, `\b` is a literal `b`) were rewritten to pin the real
  behaviour instead — they documented the matcher, not recovered behaviour.
* Look-around is **not** `regex` syntax. The embedded crit table has exactly
  one look-ahead row (`slash/head/3`, `^(?!.*removes skull.)Blow to head.`);
  `MatchIndex::build` handles it the way the binary does (`str::replace` of the
  20-byte literal by the empty string + a separately compiled 16-byte gate
  `.*removes skull.`), so all 2394 rows still compile
  (`MatchIndex::compiled() == 2394`, `gated() == 1`) and the row is suppressed
  on `Blow to head removes skull.` — pinned by a new test.
* `crit::match_index::MatchIndex` is a `Vec` of compiled patterns + optional
  gate, **not** a `RegexSet`: the recovered `build` (`0x101b9b0d4`) compiles one
  `Regex` per row into a 64-byte record. (The `RegexSetBuilder` symbols in the
  binary belong to `hydra_lookout`, whose `ignore` list is an
  `Option<RegexSet>`.)

Sibling impact: `hydra_lookout` is the only crate that uses
`hydra_model::pattern` directly; it compiles `\b`-bearing and `(?i)` patterns
through `pattern::compile` and its 41 tests still pass
(`cargo test -p hydra_lookout --offline`).

### `serde`

`Cargo.toml` now has `serde = { workspace = true }` (`=1.0.229`, `derive`).
All **30** recovered `Deserialize` impls are implemented (see the address list
in `src/lib.rs` and the tests in `src/serde_tests.rs`):

`targetid::Shown`, `character::psm::{PsmSet,PsmRanks}`,
`character::stats::{Stat,StatKind,StatValue,Identity}`,
`character::skills::{Skill,SkillKind,SkillSet}`,
`character::enhancive::{Bonus,EnhanciveTotals,Resource,Section}`,
`character::currency::Currency`,
`character::standing::{Standing,ResourceAmounts}`,
`character::gemstones::{Binding,Gemstone,Gemstones,Property}`,
`character::snapshot::{CharacterSnapshot,Group}` and
`character::vocabulary::{AccountType,Che,DeathsSting,PsmCategory,ResourceType,Society,Warcry}`.

Five further derives are needed to make `CharacterSnapshot` work and are
additive only: `Character`, `StatSet`, `Stance`, `Experience` and
`state::vitals::Vital`.

Three of the thirty were *named* by the binary but had no body in the stubs, so
they are declared here for the first time with a `// UNKNOWN:` field set:
`targetid::Shown` (a newtype over the 20-byte vitals record `of` builds),
`stats::Identity` (the identity block, reachable as `Character::identity()`) and
`enhancive::Bonus` (`{ amount, ranks }`).

Deviations, honestly:

* **No `Serialize`.** The binary has no `Serialize` impl for any `hydra_model`
  type (only `toml`/`serde_json`/`toml_edit` serializer internals), so none is
  derived. `src/serde_tests.rs` is therefore one-directional
  (document → model), using `serde_json` and `toml` as dev-dependencies.
* **Wire format unknown.** The impls are derives over the recovered *field
  names*; no `rename`/`tag`/`default` attributes are applied, so the enum
  forms are the variant identifiers (`"OrderOfVoln"`, not `"Order of Voln"`),
  which the binary does not carry. A field is required unless it is
  `Option`, and `BTreeMap<Resource, i32>` keys are enum identifiers (a
  `Resource::Stat(..)` key cannot be a JSON object key).

Text entry points: the recovered parsers take styled runs and call
`Runs::plain` first (visible in `SkillLine::classify_with_bold`,
`InfoReport::read`, `Uses of Runs::plain` in 20+ functions). This build threads
`&str` instead; the two places that need a `Runs` keep it
(`ChunkLine::text`, `ProcLine::read_runs`). Recorded at each site as a
departure, not as recovered behaviour.

The one large chunk of *data* that **is** embedded in the binary has been
extracted verbatim instead of guessed:

* `crit/data/crit.tsv` — 228812 bytes from `0x103a3ff08`
  (`CritTables::load` calls `parse_tsv(0x103a3ff08, 0x37dcc)`), 19 columns,
  2394 rows. `include_str!`-ed as `crit::load::EMBEDDED_TSV`; the crit
  classifier therefore works against the real table and the tests assert real
  rows (`acid/head/1 → damage 5`, `vacuum/right_leg/9 → damage 45, stunned 12,
  amputated, wound_rank 3`, …).

---

## Function table

`addr` is the unslid VA from the inventory. 280 rows, one per stub.

### `pattern.rs`

| symbol | addr | status | recovered signature / notes |
|---|---|---|---|
| `ascii` | `0x101b89924` | full (edit set) | `fn ascii(&str) -> String`. `regex_syntax::ast::Parser` + `Visitor` (`Edits`) with the two entry points `visit_pre` (`0x101b8cc64`) and `visit_class_set_item_pre` (`0x101b8cb40`), `driftsort_main` over the edits, `String::replace_range` from the highest offset down. Edits: perl classes → `[<class>]` / `[^<class>]` with the class table `0-9` / `\t\n\x0B\x0C\r ` / `0-9A-Za-z_` (`0x1044a3798` + lengths at `0x1039d6c00`), and `\b` → `(?-u:\b)` (`movz/movk`, node kind word `4`). Implemented at source level in `src/pattern.rs` (no `regex-syntax` pin). |
| `compile` | `0x101b8a0d8` | full | `fn compile(&str) -> Pattern` = `ascii` + `regex::Regex::new`. |
| `compile_gated` | `0x101b89800` | full | `fn compile_gated(&str) -> Option<Pattern>` = `ascii` + `RegexBuilder::new(..).dfa_size_limit(0).build()`. |

### `crit.rs`, `crit/types.rs`, `crit/load.rs`, `crit/match_index.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `crit::CritTables::from_entries` | `0x101c0dd30` | full | `fn(Vec<CritEntry>) -> CritTables`; builds `MatchIndex`. |
| `crit::CritTables::load` | `0x101c0df94` | full | `fn() -> Result<CritTables, BuildError>`; parses the embedded 228812-byte table. **No arguments.** |
| `crit::CritTables::parse` | `0x101c0e024` | full | `fn(&self, &str) -> Vec<&CritEntry>`. |
| `crit::load::parse_tsv` | `0x101bab758` | full (grammar) / partly (semantics of unused columns) | header + 19 columns recovered verbatim, three error texts + the `header is {}` format. |
| `crit::match_index::MatchIndex::build` | `0x101b9b0d4` | full | one `regex::Regex` per row (64-byte record), no `RegexSet`; a row containing the 20-byte literal `(?!.*removes skull.)` has that literal replaced by `""` and gets the 16-byte `.*removes skull.` compiled through `compile_gated` as a *negative gate* (`Option<Regex>`, `None` = no gate). Only `slash/head/3` is gated. |
| `crit::match_index::MatchIndex::matches` | `0x101b9bd88` | full | `fn(&self, &str) -> Vec<usize>`; lowercases the line, reports every matching index. |
| `crit::types::Position::parse` | `0x101b6c214` | full | length dispatch: `kneeling` 0, `prone` 1, `sitting` 2, everything else (incl. `standing`, empty) 3. |
| `crit::types::WoundLocation::parse` | `0x101b6c014` | full | `head` 0, `neck` 1, `chest` 2, `abdomen` 3, `back` 4, `nerves` 5, `right_leg` 6, `both_eyes` 7, else 8. **`left_arm`, `left_leg`, `*_hand`, `*_eye` are not accepted** (tested). |

### `status.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `StatusInfo::get` | `0x101bf463c` | full | `fn(&self, &str) -> bool`; strip literal `Icon` (`0x49636f6e`), ASCII-lowercase, `BTreeMap` walk. |
| `StatusInfo::set` | `0x101bf48c4` | full | `fn(&mut self, &str, bool) -> bool` = "value changed" (`insert` + `map_or(true, old != value)`). |
| `StatusInfo::forget` | `0x101bf4acc` | full | `fn(&mut self, &str)` = `BTreeMap::remove`. |
| `StatusInfo::is_known` | `0x101bf4cdc` | full | `fn(&self, &str) -> bool` = `contains_key`. |
| `StatusInfo::dead` | `0x101bf4abc` | full | 16-byte tail call: `self.get("dead")`. |
| `StatusReading::{dead,hidden,invisible,kneeling,prone,sitting,standing,stunned}` | `0x101bf4f5c`…`0x101bf518c` | full | each is `is_known(name) ? Some(get(name)) : None` (`None` = tag `2`, the `Option<bool>` discriminant). |

### `state/clock.rs`, `state/stun.rs`, `state/vitals.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `clock::in_roundtime` | `0x101bcd0d8` | full | `Option<bool>`; `+0x1118` bit 0 + `+0x111c`. |
| `clock::in_casttime` | `0x101bcd034` | full | as above with `+0x1120`/`+0x1124`. |
| `clock::game_time_now` | `0x101bcd17c` | full | `Option<u32>`; tag word is `+0x1130`, payload `+0x1134 + elapsed`. |
| `clock::until_next_second` | `0x101bcd1ec` | full | `Option<Duration>` = `1 - fract(elapsed)`, floor `0.01` (`fmaxnm` with `0x3f847ae147ae147b`), `Duration::from_secs_f64`. |
| `clock::casttime_remaining` | `0x101bcd3e0` | full | `Option<u32>`. |
| `clock::roundtime_remaining` | `0x101bcd484` | full | `Option<u32>`. |
| `clock::casttime_remaining_after` | `0x101bcd528` | full | 64 B, recovered instruction by instruction; takes the "after" seconds as `w1`. |
| `clock::roundtime_remaining_after` | `0x101bcd568` | full | 60 B of code (the inventory's 13316 B runs into the following unnamed functions). |
| `stun::stun_remaining` | `0x101bccf90` | full | `Option<u32>`; `Some(0)` (not `None`) when armed with no stun. |
| `vitals::vital` | `0x101bd5118` | full | `fn(&self, &str) -> Option<Vital>`; 20-byte values, `None` tag `2`. |
| `vitals::{mana,health,spirit,stamina}` | `0x101bd5010`, `0x101bd521c`, `0x101bd5324`, `0x101bd542c` | full | 264-byte wrappers over the literal `"manahealthspiritstamina"` (lengths 4/6/6/7). |

### `spells.rs`, `spells/expr.rs`, `spells/extras.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `spells::all` / `spells::with_cooldowns` | `0x101bc4c20` | full (accessor) / unknown (table) | same address; `OnceLock` over a static slice. The spell table is a generated data file — not embedded, so the slice is empty. |
| `spells::spell_named` | `0x101bc4768` | full | `fn(&str) -> Option<&'static Spell>`; `trim` → ASCII-lowercase → hash probe → sorted `u16` binary search → pointer array; null niche. |
| `spells::spell` | `0x101bc54a0` | full | `fn(u16) -> Option<&'static Spell>` (id lookup). |
| `spells::timer_change` | `0x101bc4ac4` | partly | `fn(&str) -> Option<TimerChange>`; per-spell pattern (and optional second pattern) match, then name/`f64`/`u16` copied. `// UNKNOWN:` the `Option` niche word `0x8000000000000001`. |
| `spells::cooldown_landing` | `0x101bc4c90` | partly | `"your group"` + the `noun` capture. |
| `spells::Role::parse` | `0x101bc5274` | full | `attack` 0, `offense`/`offensive` 1, `defense` 2, `utility` 3, `timer` 4, `bonus` 5, `None` = 6. `defensive` is **not** accepted (tested). |
| `spells::Spell::is` | `0x101bc53d0` | full | `fn(&self, u16) -> bool` on the `+0x60` id. |
| `spells::expr::evaluate` | `0x101c03e8c` | partly | `fn(&str, &dyn Fn(&str)->Option<f64>) -> Option<f64>`; recursive descent over `+ - * /`, parentheses, unary minus, numeric literals (`f64::from_str`) and variables. Exact operator set beyond that UNKNOWN. |
| `spells::extras::ProcLine::read` | `0x101bc5568` | partly | TSV row (`debuff\topponent\t13\t\tsilenced\tWarding, CS 431-437.`), `f32` duration, `#{target}` placeholder. |

### `state/afflictions.rs`, `bounty.rs`, `bounty_status.rs`, `creature_message.rs`, `message.rs`, `departure.rs`, `movement.rs`

| symbol | addr | status | notes |
|---|---|---|---|
| `afflictions::classify` | `0x101bfd0c0` | partly | six recovered message prefixes → `Affliction`. |
| `bounty::classify` | `0x101bbc060` | partly | five recovered task openers + the five town names. |
| `bounty::Task::requirement` | `0x101bbbff80` | partly | display text per task kind. |
| `bounty_status::read_line` | `0x101b6c51c` | full | vouchers sentence + `All done with that assignment?` recovered verbatim. |
| `bounty_status::classify_refusal` | `0x101b6c878` | partly | four refusal kinds. |
| `bounty_status::{stream_said,stream_cleared,clear}` | `0x101b6c2ec`,`0x101b6c3cc`,`0x101b6c41c` | full | thin wrappers. |
| `creature_message::classify` | `0x101b6d690` | partly | the recovered dispatch chain (ucs → status → flare → spell_loss → resolution → crit_rider → outcome → attack → damage → crit table → template). 54548 B. |
| `creature_message::match_template` | `0x101b6cc18` | unknown (table) | only `Maximum Health Points:` recovered. |
| `message::classify` | `0x101b87cb0` | full | `says, "` / `asks, "` / `whispers, "` / `whispers to you, "`, `to you`. |
| `departure::classify` | `0x101c005ec` | partly | `You walk/go/run <direction>`; 14496 B. |
| `departure::is_direction` | `0x101b?` (372 B) | full | ten compass words + `in`/`out`. |
| `movement::classify` | `0x101b94dd0` | partly | 25348 B; self/other + compass direction. The phrase table is not reachable. |

### `state/combat/*`

| symbol | addr | status | notes |
|---|---|---|---|
| `defs::defs` | `0x101b?` (56 B) | full (accessor) / unknown (table) | `OnceLock`; the defs TSV is a generated data file and is not embedded → empty table. |
| `Def::extra` | `0x101b9f?` (568 B) | full | `k=v;…` parsing (`damaging=1;aoe=0;spawns=0;key=ThornsImpact_GS;`). |
| `Defs::{any_match,first_match,first_match_with_role}` | `0x101b9efa0`-ish | full (plumbing) / partly (table) | pattern match over the table; `first_match_groups` / `first_match_with_role_groups` hand out the row's named groups (`Def::{groups,group,has_group}`), i.e. the recovered `get_group_by_name`. The table itself is still not embedded. |
| `Defs::{barrier,family,first_flight,gemstone_property,nock_weapon,swing_weapon,flare_names}` | various | partly | family-scoped lookups; `flare`/`flare_mirror` recovered. |
| `attack::{AttackLine,AttackPrefix,RedirectPrefix}::classify` | `0x101b8fd08`,`0x101b90e68`,`0x101b91624` | partly (groups filled, table missing) | def families + group names (`attacker`, `interceptor`, `intended`, `group`, `weapon`, `aimed`, `environment`, `ambush_prefix`, `reaction_prefix`, `redirect…`, `target`); every group is now read out of the hit. `RedirectPrefix` is modelled by `AttackPrefix`'s `redirect_from`/`redirect_to` (the recovered name run is truncated). |
| `attack::{attackerless_line,swing_weapon,coup_kill_location}` | `0x101b919d8`,`0x101b9139c`,`0x101b91c8c` | partly | `attackerless_line` tests the recovered family list (`ATTACKERLESS_FAMILIES`); `swing_weapon` reads the `weapon` group; `coup_kill_location` uses family `coup_kill`. |
| `bracket::{assault_start,assault_end,sequence_start,sequence_end}` | `0x101be67b8`,… | partly | roles `assault`/`sequence`; the `target` group is filled. |
| `bracket::AssaultName::parse` | 372 B | full | non-empty text. |
| `damage::DamageLine::classify` | `0x101ba031c` | partly | family `damage`, `" hits!"`, `target` group filled. |
| `flare::FlareLine::classify` | `0x101b8dbc0` | partly | families `flare`/`flare_mirror`; all nine recovered groups (`attacker`, `flare_weapon_link`, `spawns`, `target`, `reactive`, `barriers`, `via_cast`, `released`, `aura`) are read; the `bool` fields are group presence. |
| `outcome::Outcome::classify` | `0x101bffc98` | partly | 15 outcome names recovered verbatim (`hit evade block parry warded ward_failed intercept unaffected fumble hindrance already_dead miss resisted confused frenzied`). |
| `resolution::Resolution::classify` | `0x101be4bb0` | partly | family `resolution`, group `as` parsed with `f64::from_str` (falling back to the first number on the line). |
| `resolution::crit_rider_line` | `0x101be54c0` | partly | family `crit_rider`. |
| `resolution::ResolutionKind::parse` | 580 B | partly | codes `as cs uaf fs ds td udf fd avd cva mm` recovered, plus `smr ssr activation maneuver_roll` from the serde variant list. |
| `spell_loss::SpellLoss::classify` | `0x101bfe79c` | partly | family `spell_loss`, groups `target`/`spell_name` (falls back to the row `name`). |
| `status::{StatusLine::classify,StatusName::parse}` | `0x101b91df8`,`0x101b9216c` | partly | family `status`, group `target` filled. |
| `target::{link_name,link_in,bolded_link,any_link,is_self,pattern_addresses_self}` | `0x101bff3fc`-ish | partly | link identity is `noun`/`exist`; boldness lives on the run, not the link. |
| `tracker::{set_crit_tables,publish,take_facts,invalidate_for_reconnect}` | `0x101bdb?` | full | field names recovered from `Debug::fmt`: `held_cast`, `held_pre_flares`, `active_assault`, `volley_round`, `tremors`, `crit_tables`, `pending`, `dropped`. |
| `ucs::{PositionTier::parse,UcsAttack::parse,UcsLine::classify}` | `0x101be58?` | partly | the three recovered phrases; the `tier` group is fed to `PositionTier::parse` first. |

### `state/creatures/*`

| symbol | addr | status | notes |
|---|---|---|---|
| `creature::{by_name,by_noun,in_room,creatures}` | various | full (lookups) / unknown (bestiary) | the bestiary TSV is not embedded. |
| `Creature::{description,found_at,tips}` | 136/52/228 B | full | accessors. |
| `creature::parts::{AttackCategory::parse,MessageKind::parse,Treasure::is_empty}` | 532/888/80 B | partly | key sets UNKNOWN beyond the call sites. |
| `creature::status::Classification::parse` | `0x101c0f3b8` | full (28 words) | the concatenated literal gives the whole, ordered word list. |
| `creatures::{get,register,mark_in_room,cleanup_old,on_nav,note_status,apply_room_objs,read_departure}` | various | full / partly | 14 reveal phrases recovered verbatim for `register`. |
| `creatures::ally::classify` | `0x101b94a24` | partly | `passive npc` marker. |
| `creatures::body::from_wound` | `0x101b88e28` | full | crit spelling → `gs_base::body::BodyPart`; `both_eyes` → `Head` is UNKNOWN. |
| `creatures::instance::*` (17 fns) | `0x101bf1fec`… | full (small accessors) / partly (`sync_crtr_status_at` 4088 B, `appendage` 2408 B) | field names recovered from the SQL/serde runs (`template_max_hp`, `stated_health`, `stated_max_health`, `stun_rounds`, `stun_until`, `first_seen_at`, `last_seen_at`, `ucs_position`, `ucs_tier`, …). |
| `creatures::prose::read` | `0x101be74cc` | full | five recovered prose phrases → `ProseEvent`. |

### `state/character/*`

| symbol | addr | status | notes |
|---|---|---|---|
| `skills::SkillKind::key` | `0x101b64d9c` | full | 46 display names; 37 from the ordered table at `0x102420160`, 9 more from the literals inside `key`. |
| `skills::SkillKind::parse` | `0x101b657fc` | partly | case-insensitive lookup over the keys. |
| `skills::{is_table_header,is_table_end,training_footer,read_table,SkillLine::classify,SkillLine::classify_with_bold}` | various | partly | recovered header/footer texts. |
| `skills::SkillSet::{get,ranks,circle,replace}` | 116/128/228/740 B | full | map operations. |
| `stance::{Stance::from_percent,parse,parse_bar_text}` | `0x101b7aba4`,`0x101b7ad74`,`0x101b7abe8` | full | thresholds 0/21/41/61/81/101 recovered exactly; `from_percent` 68 B read instruction by instruction. Variant 6 (>100 %) UNKNOWN. |
| `stats::{StatLine::classify,classify_with_bold}` | `0x101c10660`,`0x101c10410` | full | `"Label: value (bonus)"`. |
| `standing::*` (8 fns) | various | full/partly | society, citizenship, resources, urchins, warcry messages recovered verbatim. |
| `enhancive::*` (4 fns) | various | partly | ` Bonus: `, ` Ranks: `, `": +"` markers. |
| `experience::{read_line,apply_bar}` | `0x101bc3e9c`,`0x101bc3900` | full | ` experience point`, ` in your brooch`. |
| `currency::absorb_chunk` | `0x101bf0b08` | full | two recovered phrases. |
| `gemstones::{read_chunk,equipped_properties}` | `0x101ba27b8`,`0x101ba397c` | partly | ` (Rank `, `(greater/lesser binding)`, `(equipped)`. |
| `psm::{read_tables,classify_header,PsmLine::classify,PsmSet::{has_table,replace_category}}` | various | partly | `Mnemonic` header, `", "` split. |
| `profile::read_lines` | `0x101beff60` | full | `": "`, `" is "`, `" has "`. |
| `blocks::InfoReport::read` | `0x101b63750` | full | ` Race: `, ` Profession: `, ` Age: `. |
| `snapshot::*` (4 fns) | various | partly | clone/restore. |
| `shadow::line` | `0x101ba2168` | full | nine recovered shadow-essence phrases. |
| `vocabulary::*` | various | partly | `Society` (3, with `Display`), `ResourceType` (5, with `Display`), `PsmCategory`, `Warcry`. |
| `identity::exist_id` | `0x101bd87ac` | partly | stored field. |

### Remaining `state/*` modules

| symbol(s) | addr | status | notes |
|---|---|---|---|
| `GameState::apply` | `0x101bd5534` | partly | recovered reader order; argument type (`Runs` vs `&str`) is a documented departure. |
| `GameState::{login,invalidate_for_reconnect}` | `0x101bd096c`,`0x101bd8dc8` | partly | attribute names (`yourLvl`, `stance`, `pbarStance`, `playerID`, `FREETRUE`) and the stale-status list (`bound calmed cutthroat silenced sleeping thorned`) recovered. |
| `spell_time::*` (4) | various | full/partly | expression evaluation via `spells::expr`. |
| `custom_timers::*` (5) | various | full | `"<name> <seconds>"`, `Instant`/`SystemTime` countdown. |
| `lines::{Chunk::push_line,ChunkLine::text,ChunkLine::is_spoken}` | `0x101bfe3a4`,`0x101bfe51c`,`0x101bfe520` | full | `text` is a 4-byte tail call to `Runs::plain`. |
| `text::{creature,item}` | `0x101b8d968`,300 B | full | article extraction. |
| `targeting::read` | `0x101b5bebc` | partly | numbered target rows. |
| `stream_windows::{Windows::declare,Closed::read}` | `0x101becc28`,`0x101bec?` | full | `pushStream`/`closeStream`/`clearStream`. |
| `group::{apply,contains,emptied,classify}` | various | full | group phrasing. |
| `containers::*` (7) | various | partly | `Set "<x>" to be your STOW container.` recovered verbatim. |
| `hands::read` | `0x101bbaedc` | full | `<left>`/`<right>`/`<held>`. |
| `dialogs::{read,get,scripted,Dialog::location}` | various | full | `dialogData` + the four recovered dialog ids. |
| `ship::{aboard,lapse}` + `reports::{look,is_mine}` | various | full | `Docked at: `, `not on the ocean`. |
| `bank::local` | `0x101bdf184` | partly | balance + the insufficient-funds phrase. |
| `objectives::apply` | `0x101bec0b0` | partly | `key: value` rows. |
| `inventory_snapshot::*` (7) | various | partly | counts, pages, detail rows. |
| `known_spells::{classify,knows}` | `0x101c0e4dc`,`0x101c0e1bc` | full | lowercase set. |
| `room::{saw_players,component}` + `PlayerStatus` `Display` | `0x101b61fb4`,`0x101b63018`,`0x101b6aa14` | partly | `room players`, `id`/`value`/`text`. |
| `incident::{read,Incidents::push}` | `0x101b93cb4`,580 B | partly | dedupe queue. |
| `gameobj::{classify,ObjectTypes::is}` | `0x101b92ec0`,`0x101b92224` | partly | `passive npc`/`aggressive npc` markers; object-type table not embedded. |
| `maneuvers::{ready_line,cooldown_refusal}` | `0x101b5ba10`,`0x101b5bd3c` | full | ready/refusal phrasing. |
| `cooldowns::{record_group,record_target}` | `0x101be6f68`,600 B | full | keyed store. |
| `effects::*` (8) | various | full/partly | category filters, lasting inserts, message-driven inserts (4 recovered phrases). |
| `overwatch::clear` | `0x101b80450` | partly | the two recovered volley messages. |
| `menu::*` (10) | various | full/partly | the on-disk menu format (header `# cmd`, `timestamp`, `coord\tlabel\tcommand\tcategory`) recovered from the embedded sample rows. |
| `targetid::{of,tag}` | `0x101be7ce4`,`0x101be7d7c` | full (`of`, 152 B) / partly (`tag`, 876 B) | `of` recovered instruction by instruction (0x110000 marker, 400 default max, percentage clamp); `tag`'s id grammar is UNKNOWN. |

---

## Recovered constants worth keeping

* `StatusInfo`: `Icon` prefix (`0x49636f6e`), ASCII-lowercase folding.
* Clock: `nanos == 1_000_000_000` (`0x3b9aca00`) is the "no anchor" niche;
  `fmaxnm` floor `0.01` (`0x3f847ae147ae147b`); `1e9` divisor
  (`0x41cdcd6500000000`).
* `vitals`: 20-byte record, values at node offset `0xfc`, stride `0x14`.
* `targetid::of`: `0x110000` = "no reading"; default max `400`.
* `crit`: 19 columns, 2394 rows, `0x37dcc` bytes at `0x103a3ff08`;
  errors `file is empty; expected a header row`, `header is {}`,
  `unknown secondary wound location {}`,
  `secondary wound has a location or a rank but not both`, `empty pattern`.
* `skills`: the 37-name ordered table at `0x102420160`.
* `stance`: percentage thresholds 0/21/41/61/81/101.
* `creatures`: 14 hiding/reveal phrases; 28 creature status words.
* `menu`: `coord\tlabel\tcommand\tcategory` rows with `#`/`@` placeholders.
* `reconnect`: stale status words `bound calmed cutthroat silenced sleeping thorned`.

---

## Evidence gaps / conflicts

1. ~~**Regex captures.**~~ **Closed in Phase 1**: the real `regex` supplies the
   capture set, and every classifier that reads named groups
   (`Defs::first_match`, `FlareLine`, `AttackLine`, `AttackPrefix`,
   `SpellLoss`, `StatusLine`, `DamageLine`, `Resolution`, `UcsLine`, the
   brackets) now fills its fields. What is still missing for those classifiers
   is the **def table** (gap 2), not the captures. Residual: the exact *names*
   of the `redirect…` and `tier`-adjacent groups are partly truncated in the
   string pool, and a def row whose pattern declares no group leaves the field
   `None`.
2. **Data tables that are not in the binary.** The spell table, the bestiary
   (`creatures.tsv`), the combat `defs` TSV, the object-type table and the
   movement phrase table are all generated data files. Only `crit.tsv` happens
   to be embedded as a literal, and it is extracted verbatim. Lookups in the
   other tables are consequently empty (`UNKNOWN`), which is also what the
   recovered code does with an empty table.
3. **`WoundLocation::parse` symmetry.** Only eight spellings are accepted;
   `left_arm`, `left_leg`, `left_hand`, `right_hand`, `left_eye`, `right_eye`
   and `unspecified` all fall into variant 8 even though the embedded table uses
   some of them. The disassembly is unambiguous here (length dispatch on 4/5/6/7/9
   only) and the behaviour is pinned by a test; it may be an upstream bug.
4. **`timer_change`'s `Option` encoding.** The binary writes the niche word
   `0x8000000000000001` into the first 8 bytes for `None` and copies a 40-byte
   payload otherwise; modelled as `Option<TimerChange>`.
5. **`GameState` layout.** Only the clock block and the vitals map offsets are
   established; the struct is a faithful field *set*, not a byte layout.
6. **Inventory sizes.** The per-file inventory's last function in a block can
   report a huge size because the next *named* symbol is far away
   (`roundtime_remaining_after` claims 13316 B but only 60 B are its own). The
   real function was recovered and the extra bytes belong to unnamed functions.
7. ~~**`ascii`'s edit set.**~~ **Closed in Phase 1**: the two visitor entry
   points and their static tables give the exact edits (perl classes →
   ASCII classes, `\b` → `(?-u:\b)`). The *implementation* differs from the
   original: `regex-syntax` is not a workspace pin, so the edits are applied by
   a source scan rather than on a `regex_syntax::ast::Ast`. A `\b` inside a
   bracketed class is left alone (the AST never yields an assertion there); a
   `\\` escape followed by `d`/`b` is handled correctly because the scan
   consumes escapes in pairs.
8. **`serde` wire format.** The `Deserialize` impls are derives over the
   recovered field names; renames, tagging and defaults are `// UNKNOWN:` and
   none is applied. No `Serialize` exists in the binary for these types, so the
   crate can read its wire form but not write it.
9. **Three types without a body.** `targetid::Shown`, `stats::Identity` and
   `enhancive::Bonus` are only known to exist because their `Deserialize`
   `expecting` symbols survive; their field sets here are a reconstruction.
