//! The ledger tables `hydra_combat_log` reads and writes, recovered from the SQL
//! the binary embeds, plus the scans that replace each of those statements.
//!
//! `// UNKNOWN:` the original kept these tables in the shared SQLite ledger
//! (`rusqlite::Connection`, reached through `hydra_ledger::Ledger::connect` /
//! `Ledger::reader`) and queried them with prepared statements.  This build has
//! no `rusqlite`, so the rows live in vectors and every recovered statement has
//! a hand-written equivalent.  The column lists below come from the `INSERT`
//! statements at the addresses noted on each type; the `SELECT`s are quoted next
//! to the scan that replaces them.

/// The `hunt_gear.slot` values the recorder writes.
///
/// `worn` is the slot `report::worn_line` selects (`WHERE w.slot = 'worn'`,
/// 0x102412a25); `held` covers the hands; `gloves` is the word at 0x10241378e
/// used when a swing names gauntlets.
pub const GEAR_SLOTS: [&str; 3] = ["worn", "held", "gloves"];

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
    pub attacker_exist_id: Option<String>,
    /// `// UNKNOWN:` `weapon` is set from a name id when the swing's weapon is
    /// known (`UPDATE attacks SET … weapon = ?`), kept as a name id.
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
/// `flares` in the reports is this table joined to `names`; [`Store::flares`]
/// keeps the joined name so the scans read like the SQL.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Flare {
    pub id: i64,
    pub attack_id: i64,
    pub seq: i64,
    pub name_id: i64,
    pub name: String,
    pub damaging: bool,
    pub creature_id: Option<i64>,
    pub weapon: Option<i64>,
    pub outcome: Option<String>,
    pub ours: bool,
    pub eligible: bool,
    pub source: Option<String>,
}

/// `statuses` — `SELECT s.kind, s.status, s.action, s.value, s.subject,
/// s.source, f.name, s.spell_name, s.cause FROM statuses s LEFT JOIN flares f
/// ON f.id = s.flare_id WHERE s.attack_id = ?1 ORDER BY s.id` (0x102411004).
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

/// The in-memory ledger the reports read.
#[derive(Clone, Debug, Default)]
pub struct Store {
    /// The game instance the ledger belongs to; `resolve` binds it as `?1`.
    pub instance: i64,
    pub names: Vec<Name>,
    pub hunts: Vec<Hunt>,
    pub attacks: Vec<Attack>,
    pub hits: Vec<Hit>,
    pub creatures: Vec<Creature>,
    pub flares: Vec<Flare>,
    pub statuses: Vec<Status>,
    pub effects: Vec<Effect>,
    pub gear: Vec<GearRow>,
    pub gemstones: Vec<Gemstone>,
    next_id: i64,
}

impl Store {
    pub fn new() -> Store {
        Store {
            instance: 1,
            next_id: 1,
            ..Store::default()
        }
    }

    fn id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// `hydra_ledger::name_id` (0x10168f25c): intern a word, returning its id.
    pub fn name_id(&mut self, word: &str) -> i64 {
        if let Some(n) = self.names.iter().find(|n| n.name == word) {
            return n.id;
        }
        let id = self.id();
        self.names.push(Name {
            id,
            name: word.to_string(),
        });
        id
    }

    /// The word behind a name id, or the empty string.
    pub fn name(&self, id: i64) -> &str {
        self.names
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.name.as_str())
            .unwrap_or("")
    }

    /// Open a session for `character`.
    ///
    /// `// UNKNOWN:` the original's `INSERT INTO hunts …` is not among the
    /// strings this crate keeps (the row is created by the ledger writer), so
    /// only the columns `report` reads are filled in.
    pub fn hunt_started(&mut self, instance: i64, character: &str, started_at: i64) -> i64 {
        let id = self.id();
        self.hunts.push(Hunt {
            id,
            instance,
            character: character.to_string(),
            started_at,
            ended_at: None,
        });
        id
    }

    pub fn hunt_ended(&mut self, id: i64, ended_at: i64) {
        if let Some(h) = self.hunts.iter_mut().find(|h| h.id == id) {
            h.ended_at = Some(ended_at);
        }
    }

    /// The session ids of `character`, in order — `SELECT id FROM hunts WHERE
    /// instance = ?1 AND character = ?2 ORDER BY id` (0x102412509).
    pub fn hunts_of(&self, instance: i64, character: &str) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .hunts
            .iter()
            .filter(|h| h.instance == instance && h.character == character)
            .map(|h| h.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    pub fn hunt(&self, id: i64) -> Option<&Hunt> {
        self.hunts.iter().find(|h| h.id == id)
    }

    /// Insert an attack, assigning its id.
    pub fn attack(&mut self, mut a: Attack) -> i64 {
        if a.id == 0 {
            a.id = self.next_id;
            self.next_id += 1;
        } else {
            self.next_id = self.next_id.max(a.id + 1);
        }
        let id = a.id;
        self.attacks.push(a);
        id
    }

    /// Insert a hit, assigning its id.
    pub fn hit(&mut self, mut h: Hit) -> i64 {
        if h.id == 0 {
            h.id = self.next_id;
            self.next_id += 1;
        } else {
            self.next_id = self.next_id.max(h.id + 1);
        }
        let id = h.id;
        self.hits.push(h);
        id
    }

    /// Record that `noun`/`name` was seen; `INSERT INTO creatures (hunt_id,
    /// exist_id, noun, name, first_seen, last_seen)` (0x102413702).
    pub fn creature_seen(&mut self, hunt_id: i64, noun: &str, name: &str, at: i64) -> i64 {
        if let Some(c) = self
            .creatures
            .iter_mut()
            .find(|c| c.hunt_id == hunt_id && c.noun == noun && c.name == name)
        {
            c.last_seen = at;
            return c.id;
        }
        let id = self.id();
        self.creatures.push(Creature {
            id,
            hunt_id,
            exist_id: None,
            noun: noun.to_string(),
            name: name.to_string(),
            first_seen: at,
            last_seen: at,
            ..Creature::default()
        });
        id
    }

    /// `UPDATE creatures SET killed_at = ?1, killed_by_attack_id = ?2,
    /// kill_credit = ?3 WHERE id = ?4` (0x10241321c).
    pub fn kill(&mut self, _hunt_id: i64, creature_id: i64, at: i64, by: Option<i64>, credit: &str) {
        if let Some(c) = self.creatures.iter_mut().find(|c| c.id == creature_id) {
            c.killed_at = Some(at);
            c.killed_by_attack_id = by;
            c.kill_credit = Some(credit.to_string());
        }
    }

    pub fn creature(&self, id: i64) -> Option<&Creature> {
        self.creatures.iter().find(|c| c.id == id)
    }

    /// `SELECT id, exist_id, name, noun FROM creatures WHERE id = ?1`
    /// (0x102411287).
    pub fn creature_name_row(&self, id: i64) -> Option<(&str, &str)> {
        self.creature(id).map(|c| (c.name.as_str(), c.noun.as_str()))
    }

    /// Insert a flare row, resolving its name through `name_id`.
    pub fn flare(&mut self, mut f: Flare) -> i64 {
        if f.name_id == 0 && !f.name.is_empty() {
            f.name_id = self.name_id(&f.name.clone());
        } else if f.name.is_empty() && f.name_id != 0 {
            f.name = self.name(f.name_id).to_string();
        }
        if f.id == 0 {
            f.id = self.next_id;
            self.next_id += 1;
        }
        let id = f.id;
        self.flares.push(f);
        id
    }

    /// `SELECT f.id, f.name, f.outcome, f.creature_id, c.exist_id, c.name FROM
    /// flares f LEFT JOIN creatures c ON c.id = f.creature_id WHERE f.attack_id
    /// = ?1 ORDER BY f.seq` (0x102410c91).
    pub fn flares_of(&self, attack_id: i64) -> Vec<&Flare> {
        let mut rows: Vec<&Flare> = self.flares.iter().filter(|f| f.attack_id == attack_id).collect();
        rows.sort_by_key(|f| f.seq);
        rows
    }

    /// The attacks of a session, in `seq` order.
    pub fn attacks_of(&self, hunt_id: i64) -> Vec<&Attack> {
        let mut rows: Vec<&Attack> = self.attacks.iter().filter(|a| a.hunt_id == hunt_id).collect();
        rows.sort_by_key(|a| a.seq);
        rows
    }

    /// The hits of an attack (`SELECT damage, location, crit_rank, crit_type,
    /// wound_rank, fatal, amputated, secondary_location, secondary_rank FROM
    /// hits WHERE attack_id = ?1 AND flare_id IS NULL ORDER BY seq`,
    /// 0x102410f34), restricted to hits the attack itself landed.
    pub fn hits_of(&self, attack_id: i64) -> Vec<&Hit> {
        let mut rows: Vec<&Hit> = self
            .hits
            .iter()
            .filter(|h| h.attack_id == attack_id && h.flare_id.is_none())
            .collect();
        rows.sort_by_key(|h| h.seq);
        rows
    }

    /// The creatures seen in a session.
    pub fn creatures_of(&self, hunt_id: i64) -> Vec<&Creature> {
        let mut rows: Vec<&Creature> = self
            .creatures
            .iter()
            .filter(|c| c.hunt_id == hunt_id)
            .collect();
        rows.sort_by_key(|c| c.id);
        rows
    }

    pub fn effects_of(&self, hunt_id: i64) -> Vec<&Effect> {
        let mut rows: Vec<&Effect> = self.effects.iter().filter(|e| e.hunt_id == hunt_id).collect();
        rows.sort_by_key(|e| e.began_at);
        rows
    }

    pub fn gear_of(&self, hunt_id: i64, slot: &str) -> Vec<&GearRow> {
        let mut rows: Vec<&GearRow> = self
            .gear
            .iter()
            .filter(|g| g.hunt_id == hunt_id && g.slot == slot)
            .collect();
        rows.sort_by_key(|g| (g.began_at, g.id));
        rows
    }

    pub fn gemstones_of(&self, hunt_id: i64) -> Vec<&Gemstone> {
        let mut rows: Vec<&Gemstone> = self
            .gemstones
            .iter()
            .filter(|g| g.hunt_id == hunt_id)
            .collect();
        rows.sort_by_key(|g| (g.number, g.id));
        rows
    }
}
