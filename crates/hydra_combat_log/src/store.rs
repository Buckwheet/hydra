//! The ledger tables `hydra_combat_log` reads and writes, recovered from the SQL
//! the binary embeds — and, since Phase 1, executed by **real SQLite**.
//!
//! The original keeps these tables in the shared SQLite ledger
//! (`rusqlite::Connection`, reached through `hydra_ledger::Ledger::writing` /
//! `Ledger::reader`) and queries them with prepared statements: 21
//! `StatementCache::get` call sites, 86 `Connection::prepare_with_flags`, 41
//! `query_row` and 14 `query_map`.  [`Store`] is that database again: it holds a
//! [`hydra_ledger::Connection`] — so the schema is the ledger's recovered
//! `hunts`/`attacks`/`hits`/`creatures`/`flare_rows`/`statuses`/`hunt_effects`/
//! `hunt_gear`/`gemstone_rows`/`names` migration — and every method below runs
//! the statement quoted on it through `prepare_cached`.
//!
//! The column lists come from the `INSERT` statements at the addresses noted on
//! each type; the `SELECT`s are the recovered texts, quoted beside the method
//! that now really executes them.
//!
//! # Type seams the schema forces
//!
//! The ledger's migration is the authority on column types, and three of them
//! disagree with the row shapes recovered from the combat log's own statements.
//! Each is mapped at the boundary rather than changing a public row type:
//!
//! * `hunts.instance` is `TEXT` in the schema (`hydra_ledger`'s `Hunt` row
//!   agrees: `instance: String`), while [`Hunt::instance`] is an `i64` — the
//!   value the recovered `SELECT id FROM hunts WHERE instance = ?1 …`
//!   (0x102412509) binds. It is stored as its decimal text and parsed back.
//! * `attacks.attacker_exist_id` is `INTEGER` while
//!   [`Attack::attacker_exist_id`] is text; a non-numeric value becomes `NULL`.
//! * `attacks.weapon`, `attacks.redirected_from` and `flare_rows.weapon` are
//!   `TEXT` (name ids in the original) while the recovered rows carry `i64`
//!   name ids; the id's decimal text is stored and parsed back.

use std::path::{Path, PathBuf};

use hydra_ledger::{Connection, Ledger};
use rusqlite::Row;

use crate::Error;

/// The `hunt_gear.slot` values the recorder writes.
///
/// `worn` is the slot `report::worn_line` selects (`WHERE w.slot = 'worn'`,
/// 0x102412a25); `held` covers the hands; `gloves` is the word at 0x10241378e
/// used when a swing names gauntlets.
pub const GEAR_SLOTS: [&str; 3] = ["worn", "held", "gloves"];

/// The `names.kind` [`Store::name_id`] interns under.
///
/// `// UNKNOWN:` `hydra_ledger::name_id` (0x10168f25c) takes the kind as its
/// second argument, but the value the combat log passes is not visible at the
/// recovered call sites (it is a `&str` the caller builds); `name` is the
/// plain-intern kind, with [`GEAR_KIND`] and [`PROPERTY_KIND`] for the two
/// tables that name a kind themselves.
pub const NAME_KIND: &str = "name";
/// The `names.kind` the gear rows intern under.
pub const GEAR_KIND: &str = "gear";
/// The `names.kind` a gemstone's property is interned under
/// (`gemstone_rows.property_id`, recovered from the ledger's migration).
pub const PROPERTY_KIND: &str = "property";

// ------------------------------------------------------------------ SQL text

/// `SELECT id FROM hunts WHERE instance = ?1 AND character = ?2 ORDER BY id` —
/// 0x102412509; `report::resolve`'s session numbering.
const HUNTS_OF: &str = "SELECT id FROM hunts WHERE instance = ?1 AND character = ?2 ORDER BY id";
/// `SELECT id, instance, character, started_at, ended_at FROM hunts WHERE id = ?1`.
const HUNT: &str =
    "SELECT id, instance, character, started_at, ended_at FROM hunts WHERE id = ?1";
/// The `hunts` row `report::hunt_row` (0x101354ff8) reads for one session.
const INSERT_HUNT: &str =
    "INSERT INTO hunts (instance, character, started_at) VALUES (?1, ?2, ?3)";
/// `UPDATE hunts SET ended_at = ?1 WHERE id = ?2` — `Recorder::close`'s stamp.
const END_HUNT: &str = "UPDATE hunts SET ended_at = ?1 WHERE id = ?2";

/// The `attacks` column list, in the order the `INSERT` at 0x102413413 writes
/// it, with the two columns its own `UPDATE` (0x10241339a) fills in.
const ATTACK_COLUMNS: &str = "id, hunt_id, seq, occurred_at, name, parent, parent_weapon, \
     parent_confidence, via, creature_id, target_kind, attacker, attacker_exist_id, weapon, \
     outcome, outcomes_all, aimed, ambush, attack_kind, inbound, orphan, foreign_caster, unowned, \
     ours, chunk_seq, redirected_from, round, right_item, right_gear, left_item, left_gear, \
     root_attack_id, parent_attack_id";
/// `INSERT INTO attacks (…)` — 0x102413413 (the 30 columns are
/// [`ATTACK_COLUMNS`] minus `id`, `root_attack_id` and `parent_attack_id`).
const INSERT_ATTACK: &str = "INSERT INTO attacks (hunt_id, seq, occurred_at, name, parent, \
     parent_weapon, parent_confidence, via, creature_id, target_kind, attacker, attacker_exist_id, \
     weapon, outcome, outcomes_all, aimed, ambush, attack_kind, inbound, orphan, foreign_caster, \
     unowned, ours, chunk_seq, redirected_from, round, right_item, right_gear, left_item, \
     left_gear) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, \
     ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30)";
/// `UPDATE attacks SET root_attack_id = ?1, parent_attack_id = ?2 WHERE id = ?3`
/// — 0x10241339a.
const ATTACK_PARENTS: &str =
    "UPDATE attacks SET root_attack_id = ?1, parent_attack_id = ?2 WHERE id = ?3";
/// `SELECT DISTINCT name FROM attacks WHERE hunt_id {} AND ours = 1 ORDER BY
/// name` — 0x103e66ae2 (`detail::attack_names`).
const ATTACK_NAMES: &str =
    "SELECT DISTINCT name FROM attacks WHERE hunt_id {} AND ours = 1 ORDER BY name";
/// `SELECT id, parent FROM attacks WHERE parent_attack_id = ?1 ORDER BY seq` —
/// 0x102410c4a (`detail::tree`); [`Store::children_of`] reads whole rows so the
/// caller has the names too.
const ATTACK_CHILDREN: &str =
    "SELECT id, parent FROM attacks WHERE parent_attack_id = ?1 ORDER BY seq";

/// The `hits` column list — the `INSERT` at 0x10241308e plus its `id`.
const HIT_COLUMNS: &str = "id, attack_id, flare_id, hunt_id, creature_id, seq, damage, location, \
     body_part, crit_type, crit_rank, wound_rank, fatal, amputated, secondary_location, \
     secondary_rank, line_seq";
/// `INSERT INTO hits (…)` — 0x10241308e.
const INSERT_HIT: &str = "INSERT INTO hits (attack_id, flare_id, hunt_id, creature_id, seq, \
     damage, location, body_part, crit_type, crit_rank, wound_rank, fatal, amputated, \
     secondary_location, secondary_rank, line_seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, \
     ?10, ?11, ?12, ?13, ?14, ?15, ?16)";

/// The `creatures` column list — `INSERT INTO creatures (hunt_id, exist_id,
/// noun, name, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?5)`
/// (0x102413702) plus the three columns the `UPDATE` at 0x10241321c fills in.
const CREATURE_COLUMNS: &str = "id, hunt_id, exist_id, noun, name, first_seen, last_seen, \
     killed_at, killed_by_attack_id, kill_credit";
/// The recovered `creatures` insert (0x102413702), with the nullable columns
/// the row type carries spelled out instead of the doubled `?5`.
const INSERT_CREATURE: &str = "INSERT INTO creatures (hunt_id, exist_id, noun, name, first_seen, \
     last_seen, killed_at, killed_by_attack_id, kill_credit) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, \
     ?8, ?9)";
/// `UPDATE creatures SET killed_at = ?1, killed_by_attack_id = ?2, kill_credit =
/// ?3 WHERE id = ?4` — 0x10241321c.
const KILL: &str =
    "UPDATE creatures SET killed_at = ?1, killed_by_attack_id = ?2, kill_credit = ?3 WHERE id = ?4";

/// The `flares` view's columns — `flare_rows JOIN names`, the recovered
/// `Store::flares` shape.
const FLARE_COLUMNS: &str = "id, attack_id, seq, name_id, name, damaging, creature_id, weapon, \
     outcome, ours, eligible, source";
/// `INSERT INTO flare_rows (attack_id, seq, name_id, damaging, creature_id, weapon, outcome,
/// ours, eligible, source)` — 0x10241328b.
const INSERT_FLARE: &str = "INSERT INTO flare_rows (attack_id, seq, name_id, damaging, \
     creature_id, weapon, outcome, ours, eligible, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, \
     ?8, ?9, ?10)";
/// `SELECT DISTINCT f.name FROM flares f JOIN attacks a ON a.id = f.attack_id
/// WHERE a.hunt_id {} AND f.ours = 1 ORDER BY f.name` — 0x103e686b0
/// (`listing::flare_names`).
const FLARE_NAMES: &str =
    "SELECT DISTINCT f.name FROM flares f JOIN attacks a ON a.id = f.attack_id WHERE a.hunt_id {} \
     AND f.ours = 1 ORDER BY f.name";

/// `SELECT s.id, s.attack_id, s.creature_id, s.flare_id, s.kind, s.status,
/// s.action, s.value, s.subject, s.source, f.name, s.spell_name, s.cause FROM
/// statuses s LEFT JOIN flares f ON f.id = s.flare_id WHERE s.attack_id = ?1
/// ORDER BY s.id` — 0x102411004 (the flare join is the recovered one; the
/// flare's name is the spell that landed).
const STATUS_COLUMNS: &str = "s.id, s.attack_id, s.creature_id, s.flare_id, s.kind, s.status, \
     s.action, s.value, s.subject, s.source, f.name, s.spell_name, s.cause";
/// The same row, selected for one creature (`analytics::defense`/`blind`).
const INSERT_STATUS: &str = "INSERT INTO statuses (hunt_id, attack_id, creature_id, flare_id, \
     occurred_at, kind, status, action, value, subject, source, spell_name, cause) VALUES (?1, \
     ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)";

/// `INSERT INTO hunt_effects (hunt_id, name_id, began_at)` — 0x10241378e.
const INSERT_EFFECT: &str =
    "INSERT INTO hunt_effects (hunt_id, name_id, began_at) VALUES (?1, ?2, ?3)";
/// `UPDATE hunt_effects SET ended_at = ?1 WHERE id = ?2` — the effect's own
/// `ended_at` stamp (`Recorder::sync_effects`).
const END_EFFECT: &str = "UPDATE hunt_effects SET ended_at = ?1 WHERE id = ?2";
/// The recovered `SELECT n.name, e.began_at FROM hunt_effects e JOIN names n …`
/// (0x10241214d), with the row's own columns.
const EFFECTS_OF: &str = "SELECT e.id, e.hunt_id, e.name_id, n.name, e.began_at, e.ended_at FROM \
     hunt_effects e JOIN names n ON n.id = e.name_id WHERE e.hunt_id = ?1 ORDER BY e.began_at, e.id";

/// `INSERT INTO hunt_gear (hunt_id, gear_id, exist_id, slot, began_at)` —
/// 0x1024139df.
const INSERT_GEAR: &str =
    "INSERT INTO hunt_gear (hunt_id, gear_id, exist_id, slot, began_at) VALUES (?1, ?2, ?3, ?4, ?5)";
/// `UPDATE hunt_gear SET ended_at = ?1 WHERE id = ?2` — 0x102413a52.
const END_GEAR: &str = "UPDATE hunt_gear SET ended_at = ?1 WHERE id = ?2";
/// The recovered `SELECT w.began_at, w.ended_at, n.name FROM hunt_gear w …`
/// (0x102412a25), with the row's own columns.
const GEAR_OF: &str = "SELECT w.id, w.hunt_id, w.gear_id, w.exist_id, w.slot, n.name, w.began_at, \
     w.ended_at FROM hunt_gear w JOIN names n ON n.id = w.gear_id WHERE w.hunt_id = ?1 AND \
     w.slot = ?2 ORDER BY w.began_at, w.id";

/// `INSERT INTO gemstone_rows (hunt_id, number, description, property_id, rank,
/// ranks)` — `hunt_gemstones` is the ledger's view over this table plus `names`.
const INSERT_GEMSTONE: &str = "INSERT INTO gemstone_rows (hunt_id, number, description, \
     property_id, rank, ranks) VALUES (?1, ?2, ?3, ?4, ?5, ?6)";
/// The recovered `SELECT description, name, rank, ranks FROM hunt_gemstones
/// WHERE hunt_id = ?1 ORDER BY number, id` (0x10241227f), with the row's own
/// columns.
const GEMSTONES_OF: &str = "SELECT g.id, g.hunt_id, g.number, g.description, g.name, g.rank, \
     g.ranks FROM hunt_gemstones g WHERE g.hunt_id = ?1 ORDER BY g.number, g.id";

/// `SELECT name FROM names WHERE id = ?1` — `Store::name`.
const NAME_OF: &str = "SELECT name FROM names WHERE id = ?1";
/// `SELECT id, name FROM names ORDER BY id`.
const NAMES: &str = "SELECT id, name FROM names ORDER BY id";

/// `SELECT COUNT(*), COUNT(DISTINCT COALESCE(a.root_attack_id, a.id)) FROM
/// attacks a WHERE a.hunt_id = ?1 AND a.ours = 1` — 0x103e6792c
/// (`report::totals`).
const TOTALS_ATTACKS: &str = "SELECT COUNT(*), COUNT(DISTINCT COALESCE(a.root_attack_id, a.id)) \
     FROM attacks a WHERE a.hunt_id = ?1 AND a.ours = 1";
/// The two damage sums of the same report (0x103e679ad / 0x103e67a27), in one
/// pass: `dealt` is a hit of an own attack or of an own flare, everything else
/// is `taken` — the recovered `(a.ours = 1 OR EXISTS (SELECT 1 FROM flares f
/// WHERE f.id = h.flare_id AND f.ours = 1))` predicate (0x10240f8ae).
const DAMAGE_SPLIT: &str = "SELECT COALESCE(SUM(CASE WHEN a.ours = 1 OR EXISTS (SELECT 1 FROM \
     flares f WHERE f.id = h.flare_id AND f.ours = 1) THEN h.damage ELSE 0 END), 0), \
     COALESCE(SUM(CASE WHEN a.ours = 1 OR EXISTS (SELECT 1 FROM flares f WHERE f.id = h.flare_id \
     AND f.ours = 1) THEN 0 ELSE h.damage END), 0) FROM hits h JOIN attacks a ON a.id = h.attack_id \
     WHERE h.hunt_id = ?1";
/// `SELECT COALESCE(SUM(damage), 0) FROM hits WHERE hunt_id = ?1`.
const HUNT_DAMAGE: &str = "SELECT COALESCE(SUM(damage), 0) FROM hits WHERE hunt_id = ?1";
/// `SELECT COALESCE(SUM(damage), 0) FROM hits WHERE attack_id = ?1`.
const HIT_SUM: &str = "SELECT COALESCE(SUM(damage), 0) FROM hits WHERE attack_id = ?1";
/// The creature listing's own-swing count (`listing::creatures`, 0x103e68c1a).
const CREATURE_SWINGS: &str = "SELECT COUNT(*) FROM attacks WHERE creature_id = ?1 AND ours = 1";
/// The damage those swings did — the recovered per-creature damage sum's
/// `(a.ours = 1 OR EXISTS …)` predicate, restricted to the own swings the
/// listing counts.
const CREATURE_DAMAGE: &str = "SELECT COALESCE(SUM(h.damage), 0) FROM hits h JOIN attacks a ON \
     a.id = h.attack_id WHERE a.creature_id = ?1 AND a.ours = 1";
/// `SELECT COUNT(*) FROM hits h JOIN attacks a ON a.id = h.attack_id WHERE
/// a.creature_id = ?1` (`detail::creature`'s hit count).
const CREATURE_HITS: &str = "SELECT COUNT(*) FROM hits h JOIN attacks a ON a.id = h.attack_id \
     WHERE a.creature_id = ?1";

// ---------------------------------------------------------------- row types

/// `names(id, name)`; `hydra_ledger::name_id` interns a word here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    pub id: i64,
    pub name: String,
}

/// `hunts`; `report::resolve` selects `id FROM hunts WHERE instance = ?1 AND
/// character = ?2 ORDER BY id` (0x102412509), which is how a character's
/// sessions are numbered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunt {
    pub id: i64,
    /// The schema's `hunts.instance` is `TEXT` while the recovered row is an
    /// integer (the `?1` the session query binds): stored as decimal text,
    /// parsed back — see the module note.
    pub instance: i64,
    pub character: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// `attacks` — column list from the `INSERT INTO attacks (…)` at 0x102413413.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attack {
    pub id: i64,
    pub hunt_id: i64,
    pub seq: i64,
    pub occurred_at: i64,
    pub name: String,
    /// `parent` / `parent_weapon` / `parent_confidence` are the *text* of the
    /// line the swing was read out of, kept for `detail::tree`.
    pub parent: Option<String>,
    pub parent_weapon: Option<String>,
    /// `// UNKNOWN:` `parent_confidence` is written from `?7`; the recorded
    /// column type is not visible, it is carried as text here.
    pub parent_confidence: Option<String>,
    pub via: Option<String>,
    pub creature_id: Option<i64>,
    pub target_kind: Option<String>,
    pub attacker: Option<String>,
    /// `// UNKNOWN:` the column is `INTEGER` while the recovered row is the
    /// line's text; a non-numeric value is stored as `NULL`.
    pub attacker_exist_id: Option<String>,
    /// `// UNKNOWN:` the column is `TEXT` (name ids in the original) while the
    /// recovered row carries the id; its decimal text is stored.
    pub weapon: Option<i64>,
    pub outcome: Option<String>,
    pub outcomes_all: Option<String>,
    pub aimed: bool,
    pub ambush: bool,
    pub attack_kind: Option<String>,
    pub inbound: bool,
    pub orphan: bool,
    pub foreign_caster: bool,
    pub unowned: bool,
    /// Set when the swing belongs to the character whose log this is.
    pub ours: bool,
    pub chunk_seq: i64,
    /// `// UNKNOWN:` `TEXT` in the schema, a name id in the recovered row.
    pub redirected_from: Option<i64>,
    pub round: Option<i64>,
    pub right_item: Option<i64>,
    pub right_gear: Option<i64>,
    pub left_item: Option<i64>,
    pub left_gear: Option<i64>,
    pub root_attack_id: Option<i64>,
    pub parent_attack_id: Option<i64>,
}

/// `hits` — column list from the `INSERT INTO hits (…)` at 0x10241308e.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hit {
    pub id: i64,
    pub attack_id: i64,
    pub flare_id: Option<i64>,
    pub hunt_id: i64,
    pub creature_id: Option<i64>,
    pub seq: i64,
    pub damage: i64,
    /// The game's own wording (`left leg`).
    pub location: Option<String>,
    /// The normalised enum spelling (`left_leg`).
    pub body_part: Option<String>,
    pub crit_type: Option<String>,
    pub crit_rank: Option<i64>,
    pub wound_rank: Option<i64>,
    pub fatal: bool,
    pub amputated: bool,
    pub secondary_location: Option<String>,
    pub secondary_rank: Option<i64>,
    pub line_seq: i64,
}

/// `creatures` — `INSERT INTO creatures (hunt_id, exist_id, noun, name,
/// first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?5)` (0x102413702), plus
/// the columns the `UPDATE creatures SET killed_at = ?1, killed_by_attack_id =
/// ?2, kill_credit = ?3 WHERE id = ?4` statement (0x10241321c) fills in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Creature {
    pub id: i64,
    pub hunt_id: i64,
    /// `INTEGER` in the schema, text in the recovered row: a non-numeric value
    /// is stored as `NULL`.
    pub exist_id: Option<String>,
    pub noun: String,
    pub name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub killed_at: Option<i64>,
    pub killed_by_attack_id: Option<i64>,
    /// `kill`, `assist`, …
    pub kill_credit: Option<String>,
}

/// `flare_rows` — `INSERT INTO flare_rows (attack_id, seq, name_id, damaging,
/// creature_id, weapon, outcome, ours, eligible, source)` (0x10241328b).
///
/// `flares` in the reports is this table joined to `names` — [`Store::flares_of`]
/// reads the ledger's `flares` view, so `name` and `name_id` cannot drift apart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flare {
    pub id: i64,
    pub attack_id: i64,
    pub seq: i64,
    pub name_id: i64,
    pub name: String,
    pub damaging: bool,
    pub creature_id: Option<i64>,
    /// `// UNKNOWN:` `TEXT` in the schema, a name id in the recovered row.
    pub weapon: Option<i64>,
    pub outcome: Option<String>,
    pub ours: bool,
    pub eligible: bool,
    pub source: Option<String>,
}

/// `statuses` — the `SELECT s.kind, s.status, s.action, s.value, s.subject,
/// s.source, f.name, s.spell_name, s.cause FROM statuses s LEFT JOIN flares f ON
/// f.id = s.flare_id WHERE s.attack_id = ?1 ORDER BY s.id` (0x102411004).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub id: i64,
    pub attack_id: i64,
    pub creature_id: Option<i64>,
    pub flare_id: Option<i64>,
    /// `add` / `remove`.
    pub kind: String,
    pub status: String,
    pub action: Option<String>,
    pub value: Option<i64>,
    pub subject: Option<String>,
    pub source: Option<String>,
    pub spell_name: Option<String>,
    pub cause: Option<String>,
}

/// `hunt_effects` — `INSERT INTO hunt_effects (hunt_id, name_id, began_at) …`
/// (0x10241378e) and the `ended_at` update its own scan applies.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effect {
    pub id: i64,
    pub hunt_id: i64,
    pub name_id: i64,
    pub name: String,
    pub began_at: i64,
    pub ended_at: Option<i64>,
}

/// `hunt_gear` — `INSERT INTO hunt_gear (hunt_id, gear_id, exist_id, slot,
/// began_at) …` (0x1024139df) and `UPDATE hunt_gear SET ended_at = ?1 WHERE id =
/// ?2` (0x102413a52).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GearRow {
    pub id: i64,
    pub hunt_id: i64,
    pub gear_id: i64,
    pub exist_id: Option<i64>,
    pub slot: String,
    pub name: String,
    pub began_at: i64,
    pub ended_at: Option<i64>,
}

/// `hunt_gemstones` — `SELECT description, name, rank, ranks FROM
/// hunt_gemstones WHERE hunt_id = ?1 ORDER BY number, id` (0x10241227f).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gemstone {
    pub id: i64,
    pub hunt_id: i64,
    pub number: i64,
    pub description: String,
    pub name: String,
    pub rank: i64,
    pub ranks: i64,
}

// ------------------------------------------------------------------- values
//
// The three type seams above, in both directions.

/// An `i64` name id as the `TEXT` column stores it.
fn text_of(value: Option<i64>) -> Option<String> {
    value.map(|v| v.to_string())
}

/// A `TEXT` name id back as an `i64`.
fn i64_of(value: Option<String>) -> Option<i64> {
    value.and_then(|v| v.trim().parse().ok())
}

/// The `TEXT` form of an integer instance.
fn instance_text(instance: i64) -> String {
    instance.to_string()
}

/// An instance back out of its `TEXT` column.
fn instance_of(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

// ------------------------------------------------------------------ mappers

fn attack_row(row: &Row<'_>) -> rusqlite::Result<Attack> {
    Ok(Attack {
        id: row.get(0)?,
        hunt_id: row.get(1)?,
        seq: row.get(2)?,
        occurred_at: row.get(3)?,
        name: row.get(4)?,
        parent: row.get(5)?,
        parent_weapon: row.get(6)?,
        parent_confidence: row.get(7)?,
        via: row.get(8)?,
        creature_id: row.get(9)?,
        target_kind: row.get(10)?,
        attacker: row.get(11)?,
        attacker_exist_id: row.get::<_, Option<i64>>(12)?.map(|v| v.to_string()),
        weapon: i64_of(row.get(13)?),
        outcome: row.get(14)?,
        outcomes_all: row.get(15)?,
        aimed: row.get(16)?,
        ambush: row.get(17)?,
        attack_kind: row.get(18)?,
        inbound: row.get(19)?,
        orphan: row.get(20)?,
        foreign_caster: row.get(21)?,
        unowned: row.get(22)?,
        ours: row.get(23)?,
        chunk_seq: row.get::<_, Option<i64>>(24)?.unwrap_or(0),
        redirected_from: i64_of(row.get(25)?),
        round: row.get(26)?,
        right_item: row.get(27)?,
        right_gear: row.get(28)?,
        left_item: row.get(29)?,
        left_gear: row.get(30)?,
        root_attack_id: row.get(31)?,
        parent_attack_id: row.get(32)?,
    })
}

fn hit_row(row: &Row<'_>) -> rusqlite::Result<Hit> {
    Ok(Hit {
        id: row.get(0)?,
        attack_id: row.get(1)?,
        flare_id: row.get(2)?,
        hunt_id: row.get(3)?,
        creature_id: row.get(4)?,
        seq: row.get(5)?,
        damage: row.get(6)?,
        location: row.get(7)?,
        body_part: row.get(8)?,
        crit_type: row.get(9)?,
        crit_rank: row.get(10)?,
        wound_rank: row.get(11)?,
        fatal: row.get(12)?,
        amputated: row.get(13)?,
        secondary_location: row.get(14)?,
        secondary_rank: row.get(15)?,
        line_seq: row.get::<_, Option<i64>>(16)?.unwrap_or(0),
    })
}

fn creature_row(row: &Row<'_>) -> rusqlite::Result<Creature> {
    Ok(Creature {
        id: row.get(0)?,
        hunt_id: row.get(1)?,
        exist_id: row.get::<_, Option<i64>>(2)?.map(|v| v.to_string()),
        noun: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        name: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        first_seen: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        last_seen: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
        killed_at: row.get(7)?,
        killed_by_attack_id: row.get(8)?,
        kill_credit: row.get(9)?,
    })
}

fn flare_row(row: &Row<'_>) -> rusqlite::Result<Flare> {
    Ok(Flare {
        id: row.get(0)?,
        attack_id: row.get(1)?,
        seq: row.get(2)?,
        name_id: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
        name: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        damaging: row.get(5)?,
        creature_id: row.get(6)?,
        weapon: i64_of(row.get(7)?),
        outcome: row.get(8)?,
        ours: row.get(9)?,
        eligible: row.get::<_, Option<bool>>(10)?.unwrap_or(false),
        source: row.get(11)?,
    })
}

fn status_row(row: &Row<'_>) -> rusqlite::Result<Status> {
    let flare_name: Option<String> = row.get(10)?;
    let spell_name: Option<String> = row.get(11)?;
    Ok(Status {
        id: row.get(0)?,
        attack_id: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
        creature_id: row.get(2)?,
        flare_id: row.get(3)?,
        kind: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        status: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        action: row.get(6)?,
        value: row.get(7)?,
        subject: row.get(8)?,
        source: row.get(9)?,
        spell_name: spell_name.or(flare_name),
        cause: row.get(12)?,
    })
}

fn effect_row(row: &Row<'_>) -> rusqlite::Result<Effect> {
    Ok(Effect {
        id: row.get(0)?,
        hunt_id: row.get(1)?,
        name_id: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
        name: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        began_at: row.get(4)?,
        ended_at: row.get(5)?,
    })
}

fn gear_row(row: &Row<'_>) -> rusqlite::Result<GearRow> {
    Ok(GearRow {
        id: row.get(0)?,
        hunt_id: row.get(1)?,
        gear_id: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
        exist_id: row.get(3)?,
        slot: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        name: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        began_at: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
        ended_at: row.get(7)?,
    })
}

fn hunt_row(row: &Row<'_>) -> rusqlite::Result<Hunt> {
    Ok(Hunt {
        id: row.get(0)?,
        instance: instance_of(&row.get::<_, String>(1)?),
        character: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
    })
}

fn gemstone_row(row: &Row<'_>) -> rusqlite::Result<Gemstone> {
    Ok(Gemstone {
        id: row.get(0)?,
        hunt_id: row.get(1)?,
        number: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
        description: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        name: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        rank: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
        ranks: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
    })
}

// ------------------------------------------------------------------- Store

/// The combat log's ledger: a real SQLite database behind the recovered API.
///
/// [`Store::new`] is an in-memory database (`Connection::open(":memory:")` plus
/// the ledger's own `migrate`, so the tables are the recovered ones);
/// [`Store::open`] is the file-backed ledger and [`Store::open_read_only`] the
/// read-only reader a report uses.
///
/// `// UNKNOWN:` the original never had a `Store`; its recorder and reports took
/// a `rusqlite::Connection` (or `hydra_ledger::Ledger`) directly.  The type is
/// kept because every caller and test in this crate names it, and it now wraps
/// exactly that connection.  It is not `Clone` — a `Connection` is not — but
/// [`Store::connection`] hands the handle out and [`Store::open`] opens the same
/// file again.
#[derive(Debug)]
pub struct Store {
    /// The game instance the ledger belongs to; `resolve` binds it as `?1`.
    pub instance: i64,
    conn: Connection,
}

impl Default for Store {
    fn default() -> Store {
        Store::new()
    }
}

impl Store {
    /// An empty in-memory ledger with the recovered schema.
    ///
    /// `// UNKNOWN:` the original always had a `hydra_ledger::Ledger` directory
    /// and never opened an in-memory database; this constructor exists so the
    /// crate's own tests (and [`Store::default`]) have a store, and it is the
    /// same database the file-backed path opens — the ledger's three migration
    /// batches run on it through [`hydra_ledger::migrate`].
    ///
    /// # Panics
    ///
    /// Only when SQLite cannot create an in-memory database at all, which no
    /// caller can recover from. [`Store::try_new`] is the fallible form.
    pub fn new() -> Store {
        Store::try_new().expect("an in-memory SQLite database cannot fail to open")
    }

    /// The fallible form of [`Store::new`].
    pub fn try_new() -> Result<Store, Error> {
        // `Connection::open` — the ledger's recovered `Ledger::connect` — rather
        // than `in_memory`: same opening sequence, and it reports failure
        // instead of panicking.
        let mut conn = Connection::open(":memory:", false)?;
        hydra_ledger::migrate(&mut conn)?;
        Ok(Store { instance: 1, conn })
    }

    /// The ledger in `dir`: `hydra.db` inside it, created and migrated.
    ///
    /// The recovered `Recorder::open` path (`hydra_ledger::Ledger::writing`,
    /// 0x10168e92c): `Ledger::open` is the ledger's own "make the directory,
    /// connect, migrate" constructor, and [`Ledger::writing`] the read-write
    /// connection.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Store, Error> {
        let ledger = Ledger::open(dir)?;
        Ok(Store {
            instance: 1,
            conn: ledger.writing()?,
        })
    }

    /// The read-only reader over the ledger in `dir` — `command::answer`'s
    /// `hydra_ledger::Ledger::reader` (`Ledger::connect(true)`; the `Reader` the
    /// ledger hands back wraps exactly that connection but keeps it to itself,
    /// see `hydra_ledger::Ledger::reader`).
    pub fn open_read_only(dir: impl Into<PathBuf>) -> Result<Store, Error> {
        let ledger = Ledger::new(dir);
        Ok(Store {
            instance: 1,
            conn: ledger.connect(true)?,
        })
    }

    /// A store over a ledger the caller already holds.
    ///
    /// `read_only` picks the recovered `Ledger::writing` /
    /// `Ledger::connect(true)` pair; the database must already be migrated
    /// ([`Ledger::open`] does that).
    pub fn open_ledger(ledger: &Ledger, read_only: bool) -> Result<Store, Error> {
        let conn = if read_only {
            ledger.connect(true)?
        } else {
            ledger.writing()?
        };
        Ok(Store { instance: 1, conn })
    }

    /// The `hydra_ledger::Connection` underneath — the handle the statements
    /// below are prepared on, and what a caller needs to run its own.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// The database file, if this store has one.
    pub fn path(&self) -> Option<&Path> {
        self.conn.path()
    }

    /// One row-returning statement, through the statement cache.
    fn query<T, P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
        map: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>, Error> {
        let mut statement = self.conn.raw().prepare_cached(sql)?;
        let rows = statement.query_map(params, map)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// One row-returning statement that yields at most one row.
    fn one<T, P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
        map: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Option<T>, Error> {
        let mut statement = self.conn.raw().prepare_cached(sql)?;
        let mut rows = statement.query(params)?;
        match rows.next()? {
            Some(row) => Ok(Some(map(row)?)),
            None => Ok(None),
        }
    }

    /// A statement whose single column is an `i64`.
    fn scalar<P: rusqlite::Params>(&self, sql: &str, params: P) -> Result<i64, Error> {
        self.one(sql, params, |row| row.get(0))
            .map(|v| v.unwrap_or(0))
    }

    /// A statement whose single column is text.
    fn text<P: rusqlite::Params>(&self, sql: &str, params: P) -> Result<String, Error> {
        self.one(sql, params, |row| row.get::<_, String>(0))
            .map(|v| v.unwrap_or_default())
    }

    /// Run one writing statement, returning its row id.
    fn write<P: rusqlite::Params>(&mut self, sql: &str, params: P) -> Result<i64, Error> {
        self.conn.writable()?;
        self.conn.raw().execute(sql, params)?;
        Ok(self.conn.raw().last_insert_rowid())
    }

    /// Run one writing statement that changes rows instead of inserting them.
    fn update<P: rusqlite::Params>(&mut self, sql: &str, params: P) -> Result<usize, Error> {
        self.conn.writable()?;
        Ok(self.conn.raw().execute(sql, params)?)
    }

    // --------------------------------------------------------------- names

    /// `hydra_ledger::name_id` (0x10168f25c): intern a word, returning its id.
    ///
    /// The real interning again: `INSERT OR IGNORE INTO names …` against
    /// `names_kind_name` and a `SELECT id FROM names …` read back, through
    /// [`hydra_ledger::name_id`]. The first row's `number`/`mnemonic` stick.
    pub fn name_id(&mut self, word: &str) -> Result<i64, Error> {
        self.name_id_kind(NAME_KIND, word)
    }

    /// [`Store::name_id`] with the `names.kind` spelled out.
    pub fn name_id_kind(&mut self, kind: &str, word: &str) -> Result<i64, Error> {
        self.conn.writable()?;
        Ok(hydra_ledger::name_id(&self.conn, kind, word, None, None)?)
    }

    /// The word behind a name id, or the empty string.
    pub fn name(&self, id: i64) -> Result<String, Error> {
        self.text(NAME_OF, rusqlite::params![id])
    }

    /// Every interned word, in id order.
    pub fn names(&self) -> Result<Vec<Name>, Error> {
        self.query(NAMES, [], |row| {
            Ok(Name {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })
    }

    // --------------------------------------------------------------- hunts

    /// Open a session for `character`.
    ///
    /// `// UNKNOWN:` the original's `INSERT INTO hunts …` is not among the
    /// strings this crate keeps (the row is created by the ledger writer), so
    /// only the columns `report` reads are filled in.
    pub fn hunt_started(
        &mut self,
        instance: i64,
        character: &str,
        started_at: i64,
    ) -> Result<i64, Error> {
        self.write(
            INSERT_HUNT,
            rusqlite::params![instance_text(instance), character, started_at],
        )
    }

    pub fn hunt_ended(&mut self, id: i64, ended_at: i64) -> Result<usize, Error> {
        self.update(END_HUNT, rusqlite::params![ended_at, id])
    }

    /// The session ids of `character`, in order — `SELECT id FROM hunts WHERE
    /// instance = ?1 AND character = ?2 ORDER BY id` (0x102412509).
    pub fn hunts_of(&self, instance: i64, character: &str) -> Result<Vec<i64>, Error> {
        self.query(
            HUNTS_OF,
            rusqlite::params![instance_text(instance), character],
            |row| row.get(0),
        )
    }

    pub fn hunt(&self, id: i64) -> Result<Option<Hunt>, Error> {
        self.one(HUNT, rusqlite::params![id], hunt_row)
    }

    // ------------------------------------------------------------- attacks

    /// Insert an attack, assigning its id.
    pub fn attack(&mut self, a: Attack) -> Result<i64, Error> {
        let id = self.write(
            INSERT_ATTACK,
            rusqlite::params![
                a.hunt_id,
                a.seq,
                a.occurred_at,
                a.name,
                a.parent,
                a.parent_weapon,
                a.parent_confidence,
                a.via,
                a.creature_id,
                a.target_kind.unwrap_or_default(),
                a.attacker,
                i64_of(a.attacker_exist_id),
                text_of(a.weapon),
                a.outcome,
                a.outcomes_all,
                a.aimed,
                a.ambush,
                a.attack_kind,
                a.inbound,
                a.orphan,
                a.foreign_caster,
                a.unowned,
                a.ours,
                a.chunk_seq,
                text_of(a.redirected_from),
                a.round,
                a.right_item,
                a.right_gear,
                a.left_item,
                a.left_gear,
            ],
        )?;
        if a.root_attack_id.is_some() || a.parent_attack_id.is_some() {
            self.attack_parents(id, a.root_attack_id, a.parent_attack_id)?;
        }
        Ok(id)
    }

    /// `UPDATE attacks SET root_attack_id = ?1, parent_attack_id = ?2 WHERE id =
    /// ?3` (0x10241339a) — the second half of the recovered insert path.
    pub fn attack_parents(
        &mut self,
        id: i64,
        root: Option<i64>,
        parent: Option<i64>,
    ) -> Result<usize, Error> {
        self.update(ATTACK_PARENTS, rusqlite::params![root, parent, id])
    }

    /// The attacks of a session, in `seq` order.
    pub fn attacks_of(&self, hunt_id: i64) -> Result<Vec<Attack>, Error> {
        self.query(
            &format!("SELECT {ATTACK_COLUMNS} FROM attacks WHERE hunt_id = ?1 ORDER BY seq, id"),
            rusqlite::params![hunt_id],
            attack_row,
        )
    }

    /// One attack by row id.
    pub fn attack_by_id(&self, id: i64) -> Result<Option<Attack>, Error> {
        self.one(
            &format!("SELECT {ATTACK_COLUMNS} FROM attacks WHERE id = ?1"),
            rusqlite::params![id],
            attack_row,
        )
    }

    /// The attacks against one creature, in row order.
    pub fn attacks_of_creature(&self, creature_id: i64) -> Result<Vec<Attack>, Error> {
        self.query(
            &format!("SELECT {ATTACK_COLUMNS} FROM attacks WHERE creature_id = ?1 ORDER BY id"),
            rusqlite::params![creature_id],
            attack_row,
        )
    }

    /// The children of a root attack — `SELECT id, parent FROM attacks WHERE
    /// parent_attack_id = ?1 ORDER BY seq` (0x102410c4a), read as whole
    /// [`Attack`] rows so `detail::tree` also has the names.
    pub fn children_of(&self, root: i64) -> Result<Vec<Attack>, Error> {
        self.query(
            &format!(
                "SELECT {ATTACK_COLUMNS} FROM attacks WHERE parent_attack_id = ?1 ORDER BY seq, id"
            ),
            rusqlite::params![root],
            attack_row,
        )
    }

    /// `SELECT DISTINCT name FROM attacks WHERE hunt_id {} AND ours = 1 ORDER BY
    /// name` (0x103e66ae2) over the given sessions.
    pub fn attack_names(&self, hunt_ids: &[i64]) -> Result<Vec<String>, Error> {
        let sql = ATTACK_NAMES.replace("{}", &in_list(hunt_ids));
        self.query(&sql, [], |row| row.get::<_, String>(0))
    }

    // ---------------------------------------------------------------- hits

    /// Insert a hit, assigning its id.
    pub fn hit(&mut self, h: Hit) -> Result<i64, Error> {
        self.write(
            INSERT_HIT,
            rusqlite::params![
                h.attack_id,
                h.flare_id,
                h.hunt_id,
                h.creature_id,
                h.seq,
                h.damage,
                h.location,
                h.body_part,
                h.crit_type,
                h.crit_rank,
                h.wound_rank,
                h.fatal,
                h.amputated,
                h.secondary_location,
                h.secondary_rank,
                h.line_seq,
            ],
        )
    }

    /// The hits of an attack (`SELECT damage, location, crit_rank, crit_type,
    /// wound_rank, fatal, amputated, secondary_location, secondary_rank FROM
    /// hits WHERE attack_id = ?1 AND flare_id IS NULL ORDER BY seq`,
    /// 0x102410f34), restricted to hits the attack itself landed.
    pub fn hits_of(&self, attack_id: i64) -> Result<Vec<Hit>, Error> {
        self.query(
            &format!(
                "SELECT {HIT_COLUMNS} FROM hits WHERE attack_id = ?1 AND flare_id IS NULL \
                 ORDER BY seq, id"
            ),
            rusqlite::params![attack_id],
            hit_row,
        )
    }

    /// Every hit of a session, in row order.
    pub fn hits_of_hunt(&self, hunt_id: i64) -> Result<Vec<Hit>, Error> {
        self.query(
            &format!("SELECT {HIT_COLUMNS} FROM hits WHERE hunt_id = ?1 ORDER BY id"),
            rusqlite::params![hunt_id],
            hit_row,
        )
    }

    /// Every hit on one creature, in row order (`detail::wound_map`).
    pub fn hits_of_creature(&self, creature_id: i64) -> Result<Vec<Hit>, Error> {
        self.query(
            &format!("SELECT {HIT_COLUMNS} FROM hits WHERE creature_id = ?1 ORDER BY id"),
            rusqlite::params![creature_id],
            hit_row,
        )
    }

    /// `SELECT COALESCE(SUM(damage), 0) FROM hits WHERE attack_id = ?1`.
    pub fn hit_sum(&self, attack_id: i64) -> Result<i64, Error> {
        self.scalar(HIT_SUM, rusqlite::params![attack_id])
    }

    /// `SELECT COALESCE(SUM(damage), 0) FROM hits WHERE hunt_id = ?1`.
    pub fn hunt_damage(&self, hunt_id: i64) -> Result<i64, Error> {
        self.scalar(HUNT_DAMAGE, rusqlite::params![hunt_id])
    }

    /// The session's damage, split into `(dealt, taken)` — the recovered
    /// `(a.ours = 1 OR EXISTS (SELECT 1 FROM flares f WHERE f.id = h.flare_id AND
    /// f.ours = 1))` predicate over the session's hits (0x10240f8ae).
    pub fn damage_split(&self, hunt_id: i64) -> Result<(i64, i64), Error> {
        Ok(self
            .one(DAMAGE_SPLIT, rusqlite::params![hunt_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .unwrap_or((0, 0)))
    }

    // ----------------------------------------------------------- creatures

    /// Record that `noun`/`name` was seen; `INSERT INTO creatures (hunt_id,
    /// exist_id, noun, name, first_seen, last_seen)` (0x102413702).
    pub fn creature_seen(
        &mut self,
        hunt_id: i64,
        noun: &str,
        name: &str,
        at: i64,
    ) -> Result<i64, Error> {
        let existing: Option<i64> = self.one(
            "SELECT id FROM creatures WHERE hunt_id = ?1 AND noun = ?2 AND name = ?3 ORDER BY id",
            rusqlite::params![hunt_id, noun, name],
            |row| row.get(0),
        )?;
        match existing {
            Some(id) => {
                self.update(
                    "UPDATE creatures SET last_seen = ?1 WHERE id = ?2",
                    rusqlite::params![at, id],
                )?;
                Ok(id)
            }
            None => self.creature(Creature {
                hunt_id,
                noun: noun.to_string(),
                name: name.to_string(),
                first_seen: at,
                last_seen: at,
                ..Creature::default()
            }),
        }
    }

    /// Insert a whole `creatures` row (`INSERT INTO creatures …`, 0x102413702),
    /// assigning its id.
    pub fn creature(&mut self, c: Creature) -> Result<i64, Error> {
        self.write(
            INSERT_CREATURE,
            rusqlite::params![
                c.hunt_id,
                i64_of(c.exist_id),
                c.noun,
                c.name,
                c.first_seen,
                c.last_seen,
                c.killed_at,
                c.killed_by_attack_id,
                c.kill_credit,
            ],
        )
    }

    /// `UPDATE creatures SET killed_at = ?1, killed_by_attack_id = ?2,
    /// kill_credit = ?3 WHERE id = ?4` (0x10241321c).
    pub fn kill(
        &mut self,
        _hunt_id: i64,
        creature_id: i64,
        at: i64,
        by: Option<i64>,
        credit: &str,
    ) -> Result<usize, Error> {
        self.update(KILL, rusqlite::params![at, by, credit, creature_id])
    }

    pub fn creature_by_id(&self, id: i64) -> Result<Option<Creature>, Error> {
        self.one(
            &format!("SELECT {CREATURE_COLUMNS} FROM creatures WHERE id = ?1"),
            rusqlite::params![id],
            creature_row,
        )
    }

    /// The creatures seen in a session, in row order.
    pub fn creatures_of(&self, hunt_id: i64) -> Result<Vec<Creature>, Error> {
        self.query(
            &format!("SELECT {CREATURE_COLUMNS} FROM creatures WHERE hunt_id = ?1 ORDER BY id"),
            rusqlite::params![hunt_id],
            creature_row,
        )
    }

    /// `SELECT id, exist_id, name, noun FROM creatures WHERE id = ?1`
    /// (0x102411287): the name and noun behind a creature id.
    pub fn creature_name_row(&self, id: i64) -> Result<Option<(String, String)>, Error> {
        Ok(self.creature_by_id(id)?.map(|c| (c.name, c.noun)))
    }

    /// The own swings at a creature (`listing::creatures`' attack count).
    pub fn creature_swings(&self, creature_id: i64) -> Result<i64, Error> {
        self.scalar(CREATURE_SWINGS, rusqlite::params![creature_id])
    }

    /// The damage those swings did.
    pub fn creature_damage(&self, creature_id: i64) -> Result<i64, Error> {
        self.scalar(CREATURE_DAMAGE, rusqlite::params![creature_id])
    }

    /// Every hit on the creature's attacks, own or not (`detail::creature`).
    pub fn creature_hits(&self, creature_id: i64) -> Result<i64, Error> {
        self.scalar(CREATURE_HITS, rusqlite::params![creature_id])
    }

    // -------------------------------------------------------------- flares

    /// Insert a flare row, resolving its name through `name_id`.
    pub fn flare(&mut self, f: Flare) -> Result<i64, Error> {
        let name_id = if f.name_id != 0 {
            f.name_id
        } else if !f.name.is_empty() {
            self.name_id(&f.name)?
        } else {
            0
        };
        self.write(
            INSERT_FLARE,
            rusqlite::params![
                f.attack_id,
                f.seq,
                name_id,
                f.damaging,
                f.creature_id,
                text_of(f.weapon),
                f.outcome,
                f.ours,
                f.eligible,
                f.source,
            ],
        )
    }

    /// `SELECT f.id, f.name, f.outcome, f.creature_id, c.exist_id, c.name FROM
    /// flares f LEFT JOIN creatures c ON c.id = f.creature_id WHERE f.attack_id
    /// = ?1 ORDER BY f.seq` (0x102410c91) — the `flares` view's rows, which is
    /// `flare_rows JOIN names`.
    pub fn flares_of(&self, attack_id: i64) -> Result<Vec<Flare>, Error> {
        self.query(
            &format!("SELECT {FLARE_COLUMNS} FROM flares WHERE attack_id = ?1 ORDER BY seq, id"),
            rusqlite::params![attack_id],
            flare_row,
        )
    }

    /// One flare by row id.
    pub fn flare_by_id(&self, id: i64) -> Result<Option<Flare>, Error> {
        self.one(
            &format!("SELECT {FLARE_COLUMNS} FROM flares WHERE id = ?1"),
            rusqlite::params![id],
            flare_row,
        )
    }

    /// `SELECT DISTINCT f.name FROM flares f JOIN attacks a ON a.id =
    /// f.attack_id WHERE a.hunt_id {} AND f.ours = 1 ORDER BY f.name`
    /// (0x103e686b0) over the given sessions.
    pub fn flare_names(&self, hunt_ids: &[i64]) -> Result<Vec<String>, Error> {
        let sql = FLARE_NAMES.replace("{}", &in_list(hunt_ids));
        self.query(&sql, [], |row| row.get::<_, String>(0))
    }

    // ------------------------------------------------------------ statuses

    /// Insert a `statuses` row.
    ///
    /// `// UNKNOWN:` the recovered [`Status`] row carries neither `hunt_id` nor
    /// `occurred_at`, but the ledger's schema declares both `NOT NULL`, so the
    /// caller supplies them.
    pub fn status(&mut self, hunt_id: i64, at: i64, s: Status) -> Result<i64, Error> {
        self.write(
            INSERT_STATUS,
            rusqlite::params![
                hunt_id,
                s.attack_id,
                s.creature_id,
                s.flare_id,
                at,
                s.kind,
                s.status,
                s.action,
                s.value,
                s.subject,
                s.source,
                s.spell_name,
                s.cause,
            ],
        )
    }

    /// The statuses of an attack (0x102411004), with the flare's name as the
    /// spell that landed.
    pub fn statuses_of(&self, attack_id: i64) -> Result<Vec<Status>, Error> {
        let sql = format!(
            "SELECT {STATUS_COLUMNS} FROM statuses s LEFT JOIN flares f ON f.id = s.flare_id \
             WHERE s.attack_id = ?1 ORDER BY s.id"
        );
        self.query(&sql, rusqlite::params![attack_id], status_row)
    }

    /// The statuses recorded against a creature (the same row, keyed by
    /// `s.creature_id`).
    pub fn statuses_of_creature(&self, creature_id: i64) -> Result<Vec<Status>, Error> {
        let sql = format!(
            "SELECT {STATUS_COLUMNS} FROM statuses s LEFT JOIN flares f ON f.id = s.flare_id \
             WHERE s.creature_id = ?1 ORDER BY s.id"
        );
        self.query(&sql, rusqlite::params![creature_id], status_row)
    }

    // ------------------------------------------------------------- effects

    /// `INSERT INTO hunt_effects (hunt_id, name_id, began_at)` (0x10241378e),
    /// interning the effect's name first when it arrives without an id.
    pub fn effect(&mut self, e: Effect) -> Result<i64, Error> {
        let name_id = if e.name_id != 0 {
            e.name_id
        } else if !e.name.is_empty() {
            self.name_id(&e.name)?
        } else {
            0
        };
        self.write(
            INSERT_EFFECT,
            rusqlite::params![e.hunt_id, name_id, e.began_at],
        )
    }

    /// `UPDATE hunt_effects SET ended_at = ?1 WHERE id = ?2`.
    pub fn effect_ended(&mut self, id: i64, at: i64) -> Result<usize, Error> {
        self.update(END_EFFECT, rusqlite::params![at, id])
    }

    /// The effects a session saw, in `began_at` order, with the interned name
    /// joined on (0x10241214d).
    pub fn effects_of(&self, hunt_id: i64) -> Result<Vec<Effect>, Error> {
        self.query(EFFECTS_OF, rusqlite::params![hunt_id], effect_row)
    }

    // ---------------------------------------------------------------- gear

    /// `INSERT INTO hunt_gear (hunt_id, gear_id, exist_id, slot, began_at)`
    /// (0x1024139df), interning the gear's name as its `gear_id`.
    pub fn gear(&mut self, g: GearRow) -> Result<i64, Error> {
        let gear_id = if g.gear_id != 0 {
            g.gear_id
        } else if !g.name.is_empty() {
            self.name_id_kind(GEAR_KIND, &g.name)?
        } else {
            0
        };
        self.write(
            INSERT_GEAR,
            rusqlite::params![g.hunt_id, gear_id, g.exist_id, g.slot, g.began_at],
        )
    }

    /// `UPDATE hunt_gear SET ended_at = ?1 WHERE id = ?2` (0x102413a52).
    pub fn gear_ended(&mut self, id: i64, at: i64) -> Result<usize, Error> {
        self.update(END_GEAR, rusqlite::params![at, id])
    }

    /// The gear of one slot in a session (0x102412a25), with the item's name
    /// joined on from `names`.
    pub fn gear_of(&self, hunt_id: i64, slot: &str) -> Result<Vec<GearRow>, Error> {
        self.query(GEAR_OF, rusqlite::params![hunt_id, slot], gear_row)
    }

    // ----------------------------------------------------------- gemstones

    /// The gemstones a `gem` line reported, the way
    /// `Recorder::write_gemstones` (0x101375310) writes them: one
    /// `gemstone_rows` row per property, the property name interned through
    /// [`hydra_ledger::name_id`] (which the `hunt_gemstones` view joins back).
    pub fn gemstone(&mut self, g: Gemstone) -> Result<i64, Error> {
        let property_id = if !g.name.is_empty() {
            self.name_id_kind(PROPERTY_KIND, &g.name)?
        } else {
            0
        };
        self.write(
            INSERT_GEMSTONE,
            rusqlite::params![
                g.hunt_id,
                g.number,
                g.description,
                property_id,
                g.rank,
                g.ranks
            ],
        )
    }

    /// `SELECT description, name, rank, ranks FROM hunt_gemstones WHERE hunt_id =
    /// ?1 ORDER BY number, id` (0x10241227f).
    pub fn gemstones_of(&self, hunt_id: i64) -> Result<Vec<Gemstone>, Error> {
        self.query(GEMSTONES_OF, rusqlite::params![hunt_id], gemstone_row)
    }

    // -------------------------------------------------------------- totals

    /// `report::totals`' first statement (0x103e6792c): the own swings of a
    /// session and how many sequences they form.
    pub fn attack_and_sequence_counts(&self, hunt_id: i64) -> Result<(i64, i64), Error> {
        Ok(self
            .one(TOTALS_ATTACKS, rusqlite::params![hunt_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .unwrap_or((0, 0)))
    }
}

/// `IN (1,2,3)`, or `IN (NULL)` when the list is empty — the recovered
/// `Hunts::sql_in` (0x101352698) text, which the `{}` slots above splice in.
fn in_list(ids: &[i64]) -> String {
    if ids.is_empty() {
        return String::from("IN (NULL)");
    }
    let joined: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    format!("IN ({})", joined.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        Store::new()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hydra-combat-log-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_fresh_store_is_a_migrated_sqlite_database() {
        let s = store();
        // The ledger's own migration ran: `user_version` is the schema version
        // and the recovered tables exist.
        assert_eq!(
            s.connection().user_version(),
            hydra_ledger::schema::SCHEMA_VERSION
        );
        let tables: i64 = s
            .connection()
            .raw()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name IN \
                 ('hunts', 'attacks', 'hits', 'creatures', 'flare_rows', 'statuses', \
                  'hunt_effects', 'hunt_gear', 'gemstone_rows', 'names', 'flares', \
                  'hunt_gemstones')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 12);
        // An in-memory database: the only "path" is SQLite's own marker.
        assert_eq!(s.path(), Some(Path::new(":memory:")));
    }

    #[test]
    fn hunt_and_attack_round_trip_through_sql() {
        let mut s = store();
        let hunt = s.hunt_started(7, "Sindar", 1_000_000).unwrap();
        assert_eq!(hunt, 1);
        // The instance is the schema's TEXT column.
        let instance: String = s
            .connection()
            .raw()
            .query_row("SELECT instance FROM hunts WHERE id = ?1", [hunt], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(instance, "7");
        s.hunt_ended(hunt, 1_000_900).unwrap();
        let row = s.hunt(hunt).unwrap().expect("hunt");
        assert_eq!(row.instance, 7);
        assert_eq!(row.character, "Sindar");
        assert_eq!(row.ended_at, Some(1_000_900));
        assert_eq!(s.hunts_of(7, "Sindar").unwrap(), vec![hunt]);
        assert!(s.hunts_of(7, "Nobody").unwrap().is_empty());

        let attack = s
            .attack(Attack {
                hunt_id: hunt,
                seq: 4,
                occurred_at: 1_000_100,
                name: String::from("jab"),
                attacker: Some(String::from("Sindar")),
                attacker_exist_id: Some(String::from("1234")),
                weapon: Some(9),
                redirected_from: Some(11),
                ours: true,
                aimed: true,
                outcome: Some(String::from("hit")),
                root_attack_id: Some(0),
                ..Attack::default()
            })
            .unwrap();
        let back = s.attack_by_id(attack).unwrap().expect("attack");
        assert_eq!(back.seq, 4);
        assert_eq!(back.name, "jab");
        assert!(back.aimed && back.ours && !back.inbound);
        assert_eq!(back.attacker_exist_id.as_deref(), Some("1234"));
        assert_eq!(back.weapon, Some(9));
        assert_eq!(back.redirected_from, Some(11));
        assert_eq!(back.root_attack_id, Some(0));
        // The parent update is a second statement, as recovered.
        s.attack_parents(attack, Some(0), Some(0)).unwrap();
        assert_eq!(s.attacks_of(hunt).unwrap().len(), 1);
        assert_eq!(s.attack_and_sequence_counts(hunt).unwrap(), (1, 1));
    }

    #[test]
    fn hits_flares_and_creatures_round_trip() {
        let mut s = store();
        let hunt = s.hunt_started(1, "Sindar", 1_000_000).unwrap();
        let kobold = s.creature_seen(hunt, "kobold", "a kobold", 1_000_050).unwrap();
        // A second sighting moves `last_seen` and keeps the row.
        assert_eq!(
            s.creature_seen(hunt, "kobold", "a kobold", 1_000_080).unwrap(),
            kobold
        );
        let creature = s.creature_by_id(kobold).unwrap().expect("creature");
        assert_eq!(creature.first_seen, 1_000_050);
        assert_eq!(creature.last_seen, 1_000_080);
        assert_eq!(
            s.creature_name_row(kobold).unwrap(),
            Some((String::from("a kobold"), String::from("kobold")))
        );

        let swing = s
            .attack(Attack {
                hunt_id: hunt,
                seq: 1,
                occurred_at: 1_000_100,
                name: String::from("jab"),
                creature_id: Some(kobold),
                outcome: Some(String::from("hit")),
                ours: true,
                ..Attack::default()
            })
            .unwrap();
        s.hit(Hit {
            attack_id: swing,
            hunt_id: hunt,
            creature_id: Some(kobold),
            seq: 1,
            damage: 25,
            location: Some(String::from("left leg")),
            body_part: Some(String::from("left_leg")),
            crit_type: Some(String::from("crush")),
            crit_rank: Some(3),
            wound_rank: Some(2),
            fatal: false,
            amputated: true,
            line_seq: 1,
            ..Hit::default()
        })
        .unwrap();
        let hits = s.hits_of(swing).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].damage, 25);
        assert_eq!(hits[0].body_part.as_deref(), Some("left_leg"));
        assert_eq!(hits[0].crit_rank, Some(3));
        assert!(hits[0].amputated && !hits[0].fatal);
        assert_eq!(s.hit_sum(swing).unwrap(), 25);
        assert_eq!(s.hunt_damage(hunt).unwrap(), 25);
        assert_eq!(s.damage_split(hunt).unwrap(), (25, 0));

        // A damaging flare, resolved through `names`, reads back joined.
        let flare = s
            .flare(Flare {
                attack_id: swing,
                seq: 1,
                name: String::from("acid_flare"),
                damaging: true,
                ours: true,
                eligible: true,
                ..Flare::default()
            })
            .unwrap();
        let flares = s.flares_of(swing).unwrap();
        assert_eq!(flares.len(), 1);
        assert_eq!(flares[0].id, flare);
        assert_eq!(flares[0].name, "acid_flare");
        assert!(flares[0].name_id > 0);
        assert_eq!(s.flare_by_id(flare).unwrap().unwrap().name, "acid_flare");
        assert_eq!(s.flare_names(&[hunt]).unwrap(), vec!["acid_flare"]);

        // The creature's own-swing counts.
        assert_eq!(s.creature_swings(kobold).unwrap(), 1);
        assert_eq!(s.creature_damage(kobold).unwrap(), 25);
        assert_eq!(s.creature_hits(kobold).unwrap(), 1);

        s.kill(hunt, kobold, 1_000_200, Some(swing), "kill")
            .unwrap();
        let dead = s.creature_by_id(kobold).unwrap().expect("creature");
        assert_eq!(dead.killed_at, Some(1_000_200));
        assert_eq!(dead.killed_by_attack_id, Some(swing));
        assert_eq!(dead.kill_credit.as_deref(), Some("kill"));
    }

    #[test]
    fn effects_gear_and_gemstones_round_trip() {
        let mut s = store();
        let hunt = s.hunt_started(1, "Sindar", 1_000_000).unwrap();
        let effect = s
            .effect(Effect {
                hunt_id: hunt,
                name: String::from("strength"),
                began_at: 1_000_010,
                ..Effect::default()
            })
            .unwrap();
        s.effect_ended(effect, 1_000_500).unwrap();
        let effects = s.effects_of(hunt).unwrap();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].name, "strength");
        assert_eq!(effects[0].began_at, 1_000_010);
        assert_eq!(effects[0].ended_at, Some(1_000_500));
        assert!(effects[0].name_id > 0);

        let worn = s
            .gear(GearRow {
                hunt_id: hunt,
                slot: String::from("worn"),
                name: String::from("a cuirbouilli coat"),
                began_at: 1_000_000,
                ..GearRow::default()
            })
            .unwrap();
        let gear = s.gear_of(hunt, "worn").unwrap();
        assert_eq!(gear.len(), 1);
        assert_eq!(gear[0].name, "a cuirbouilli coat");
        assert!(gear[0].gear_id > 0);
        assert!(gear[0].ended_at.is_none());
        s.gear_ended(worn, 1_000_600).unwrap();
        assert_eq!(s.gear_of(hunt, "worn").unwrap()[0].ended_at, Some(1_000_600));
        assert!(s.gear_of(hunt, "held").unwrap().is_empty());

        s.gemstone(Gemstone {
            hunt_id: hunt,
            number: 1,
            description: String::from("a blue sapphire"),
            name: String::from("sapphire"),
            rank: 2,
            ranks: 4,
            ..Gemstone::default()
        })
        .unwrap();
        let gems = s.gemstones_of(hunt).unwrap();
        assert_eq!(gems.len(), 1);
        assert_eq!(gems[0].description, "a blue sapphire");
        assert_eq!(gems[0].name, "sapphire");
        assert_eq!(gems[0].rank, 2);
        assert_eq!(gems[0].ranks, 4);
    }

    #[test]
    fn statuses_round_trip_through_the_flare_join() {
        let mut s = store();
        let hunt = s.hunt_started(1, "Sindar", 1_000_000).unwrap();
        let kobold = s.creature_seen(hunt, "kobold", "a kobold", 1_000_050).unwrap();
        let swing = s
            .attack(Attack {
                hunt_id: hunt,
                seq: 1,
                occurred_at: 1_000_100,
                name: String::from("jab"),
                ..Attack::default()
            })
            .unwrap();
        s.status(
            hunt,
            1_000_100,
            Status {
                attack_id: swing,
                creature_id: Some(kobold),
                kind: String::from("status"),
                status: String::from("ds"),
                action: Some(String::from("add")),
                value: Some(120),
                ..Status::default()
            },
        )
        .unwrap();
        let rows = s.statuses_of(swing).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "ds");
        assert_eq!(rows[0].value, Some(120));
        assert_eq!(rows[0].attack_id, swing);
        assert_eq!(s.statuses_of_creature(kobold).unwrap().len(), 1);
        assert!(s.statuses_of(swing + 1).unwrap().is_empty());
    }

    #[test]
    fn name_ids_are_the_ledger_interning() {
        let mut s = store();
        let first = s.name_id("barkskin").unwrap();
        let again = s.name_id("barkskin").unwrap();
        assert_eq!(first, again);
        // The kind is part of the identity, as `names_kind_name` says.
        let gear = s.name_id_kind(GEAR_KIND, "barkskin").unwrap();
        assert_ne!(first, gear);
        assert_eq!(s.name(first).unwrap(), "barkskin");
        assert_eq!(s.name(9_999).unwrap(), "");
        assert_eq!(s.names().unwrap().len(), 2);
    }

    #[test]
    fn attack_and_flare_names_use_the_recovered_in_clause() {
        let mut s = store();
        let one = s.hunt_started(1, "Sindar", 1_000_000).unwrap();
        let two = s.hunt_started(1, "Sindar", 2_000_000).unwrap();
        for (hunt, name) in [(one, "jab"), (one, "jab"), (two, "punch")] {
            s.attack(Attack {
                hunt_id: hunt,
                occurred_at: 1_000_100,
                name: String::from(name),
                ours: true,
                ..Attack::default()
            })
            .unwrap();
        }
        assert_eq!(
            s.attack_names(&[one, two]).unwrap(),
            vec![String::from("jab"), String::from("punch")]
        );
        assert_eq!(s.attack_names(&[two]).unwrap(), vec![String::from("punch")]);
        // An empty scope is `IN (NULL)`, which selects nothing.
        assert!(s.attack_names(&[]).unwrap().is_empty());
        assert!(s.flare_names(&[]).unwrap().is_empty());
    }

    #[test]
    fn the_ledger_reads_the_hunts_the_combat_log_wrote() {
        // The real cross-crate check: the rows `hydra_combat_log` writes are the
        // rows `hydra_ledger`'s own reader queries.  `Reader::hunt_overlapping`
        // (0x10398312c) runs the recovered `SELECT h.id FROM hunts h …` with the
        // `COALESCE(h.ended_at, (SELECT MAX(occurred_at) FROM attacks …))`
        // fallback, against the file this store wrote.
        let dir = scratch("ledger-reader");
        let hunt = {
            let mut store = Store::open(&dir).expect("opens the ledger");
            let hunt = store.hunt_started(1, "Sindar", 1_000_000).unwrap();
            store
                .attack(Attack {
                    hunt_id: hunt,
                    seq: 1,
                    occurred_at: 1_000_400,
                    name: String::from("jab"),
                    ours: true,
                    ..Attack::default()
                })
                .unwrap();
            hunt
        };
        let ledger = Ledger::new(&dir);
        let reader = ledger.reader().expect("reader");
        // An open session: the ledger falls back to the last attack's stamp.
        let who = hydra_ledger::Who::new("1", "Sindar");
        assert_eq!(
            reader.hunt_overlapping(&who, 1_000_300, 1_000_500).unwrap(),
            Some(hunt)
        );
        // A window past the last attack finds nothing.
        assert_eq!(
            reader.hunt_overlapping(&who, 1_000_500, 1_000_600).unwrap(),
            None
        );
        // And the interning `name_id` wrote is the ledger's `names` table.
        let mut store = Store::open_ledger(&ledger, false).expect("writing");
        let id = store.name_id("barkskin").unwrap();
        assert_eq!(store.connection().name_id_of(NAME_KIND, "barkskin"), Some(id));
        drop(store);
        let db = reader.connection().db();
        assert!(db.names.iter().any(|n| n.row.name == "barkskin"));
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_backed_ledger_round_trips_and_a_reader_refuses_writes() {
        let dir = scratch("file");
        {
            let mut writing = Store::open(&dir).expect("opens the ledger");
            assert_eq!(writing.path(), Some(dir.join("hydra.db").as_path()));
            let hunt = writing.hunt_started(1, "Sindar", 1_000_000).unwrap();
            writing.hunt_ended(hunt, 1_000_600).unwrap();
        }
        let mut reading = Store::open_read_only(&dir).expect("reads the ledger");
        assert_eq!(reading.hunts_of(1, "Sindar").unwrap().len(), 1);
        assert_eq!(reading.hunt(1).unwrap().unwrap().ended_at, Some(1_000_600));
        let error = reading.hunt_started(1, "Sindar", 2_000_000).unwrap_err();
        assert!(error.message().contains("readonly"), "{}", error.message());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
