//! Every SQL literal the ledger uses, recovered from `__const`.
//!
//! Nothing here is invented: each constant carries the virtual address it was
//! read from and its length. The three migration batches are byte-exact (their
//! lengths come from the `(ptr, len)` pairs in `__DATA` at `0x104442760`, which
//! `hydra_ledger::migrate` walks); the query texts were transcribed from
//! `__const` with the length quoted from the `mov wN, #len` immediate that feeds
//! `Connection::prepare_with_flags` / `StatementCache::get`. A query that takes
//! its column list as an argument keeps the original's `SELECT {}` shape — see
//! [`columns`].
//!
//! The schema is also the *only* description of the rows: the store runs these
//! statements through `rusqlite` (Phase 1), so the text below is what SQLite
//! executes rather than documentation of a stand-in.

/// `PRAGMA user_version` — `0x103985c1b`, 19 bytes; read first by
/// [`crate::migrate`].
pub const USER_VERSION: &str = "PRAGMA user_version";

/// The column name handed to `Connection::pragma_update(None, …, n)` when a
/// migration finishes — `0x103985b8b`, 12 bytes.
pub const USER_VERSION_NAME: &str = "user_version";

/// The database file's name — `0x103985c80`, 8 bytes; joined onto
/// [`crate::Ledger`]'s directory by `Ledger::path_in` (`0x10168e91c`).
pub const DB_FILE: &str = "hydra.db";

/// The thread the writer runs on — `0x103983828`, 12 bytes
/// (`std::thread::Builder::name` in `Ledger::writer`, `0x10168e9e0`).
pub const WRITER_THREAD: &str = "hydra-ledger";

/// The highest schema version this Hydra understands. `migrate` compares the
/// database's `user_version` with this and refuses anything above it.
pub const SCHEMA_VERSION: u32 = 3;

/// `BEGIN DEFERRED` / `BEGIN IMMEDIATE` / `BEGIN EXCLUSIVE` — the three-entry
/// table at `0x1044428c0` (pointers) / `0x103985c68` (lengths). `migrate` picks
/// one by indexing that table with a byte it reads out of the connection (see
/// `NOTES.md` §5.4).
pub const BEGIN_DEFERRED: &str = "BEGIN DEFERRED";
/// `BEGIN IMMEDIATE` (15 bytes).
pub const BEGIN_IMMEDIATE: &str = "BEGIN IMMEDIATE";
/// `BEGIN EXCLUSIVE` (15 bytes).
pub const BEGIN_EXCLUSIVE: &str = "BEGIN EXCLUSIVE";
/// `COMMIT` — `0x103985c5c`.
pub const COMMIT: &str = "COMMIT";

/// The three migration batches, in order. Index `n` upgrades schema `n` to
/// schema `n + 1`; there are exactly three (`migrate` walks `0x104442760` for
/// `0x30` bytes of `(ptr, len)` pairs).
pub const MIGRATIONS: [&str; 3] = [MIGRATION_1, MIGRATION_2, MIGRATION_3];

/// The schema a fresh ledger is created with.
pub const BASE_SCHEMA: &str = MIGRATION_1;

/// Migration to schema version 1 — verbatim from `0x103983834` (8190 bytes).
pub const MIGRATION_1: &str = r#"CREATE TABLE pulses (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        game_time INTEGER,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        exp INTEGER NOT NULL,
        axp INTEGER NOT NULL,
        exp_gained INTEGER NOT NULL DEFAULT 0,
        axp_gained INTEGER NOT NULL DEFAULT 0,
        fxp INTEGER,
        fxp_max INTEGER,
        until_next INTEGER,
        lumnis INTEGER,
        rpa TEXT,
        fashlonae INTEGER,
        mind_percent INTEGER,
        mind_text TEXT,
        source TEXT NOT NULL DEFAULT 'normal',
        instant INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX pulses_who_at ON pulses(instance, character, at);
    CREATE TABLE snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        game_time INTEGER,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        kind TEXT NOT NULL,
        name TEXT,
        value INTEGER,
        weekly INTEGER,
        total INTEGER
    );
    CREATE INDEX snapshots_who_kind_at ON snapshots(instance, character, kind, at);
    CREATE TABLE names (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        number INTEGER,
        mnemonic TEXT
    );
    CREATE UNIQUE INDEX names_kind_name ON names(kind, name);
    CREATE INDEX names_mnemonic ON names(kind, mnemonic);
    CREATE TABLE hunts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        ended_at INTEGER
    );
    CREATE INDEX hunts_who ON hunts(instance, character, id);
    CREATE TABLE creatures (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        exist_id INTEGER,
        noun TEXT,
        name TEXT,
        first_seen INTEGER,
        last_seen INTEGER,
        killed_at INTEGER,
        killed_by_attack_id INTEGER,
        kill_credit TEXT
    );
    CREATE INDEX creatures_hunt ON creatures(hunt_id);
    CREATE INDEX creatures_exist ON creatures(exist_id);
    CREATE TABLE attacks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        seq INTEGER NOT NULL,
        occurred_at INTEGER NOT NULL,
        name TEXT NOT NULL,
        parent TEXT,
        parent_weapon TEXT,
        root_attack_id INTEGER,
        parent_attack_id INTEGER,
        parent_confidence TEXT,
        via TEXT,
        creature_id INTEGER REFERENCES creatures(id),
        target_kind TEXT NOT NULL,
        attacker TEXT,
        attacker_exist_id INTEGER,
        weapon TEXT,
        outcome TEXT,
        outcomes_all TEXT,
        aimed INTEGER NOT NULL DEFAULT 0,
        ambush INTEGER NOT NULL DEFAULT 0,
        attack_kind TEXT,
        inbound INTEGER NOT NULL DEFAULT 0,
        orphan INTEGER NOT NULL DEFAULT 0,
        foreign_caster INTEGER NOT NULL DEFAULT 0,
        unowned INTEGER NOT NULL DEFAULT 0,
        ours INTEGER NOT NULL DEFAULT 0,
        chunk_seq INTEGER,
        redirected_from TEXT,
        round INTEGER,
        right_item INTEGER,
        right_gear INTEGER REFERENCES names(id),
        left_item INTEGER,
        left_gear INTEGER REFERENCES names(id)
    );
    CREATE INDEX attacks_hunt ON attacks(hunt_id, seq);
    CREATE INDEX attacks_creature ON attacks(creature_id);
    CREATE INDEX attacks_name ON attacks(hunt_id, name);
    CREATE INDEX attacks_root ON attacks(root_attack_id);
    CREATE TABLE resolutions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        attack_id INTEGER NOT NULL REFERENCES attacks(id),
        flare_id INTEGER,
        seq INTEGER NOT NULL,
        type TEXT NOT NULL,
        attacker_stat INTEGER,
        defender_stat INTEGER,
        modifier INTEGER,
        roll INTEGER,
        bonus INTEGER,
        penalty INTEGER,
        result INTEGER,
        total REAL
    );
    CREATE INDEX resolutions_attack ON resolutions(attack_id);
    CREATE TABLE flare_rows (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        attack_id INTEGER NOT NULL REFERENCES attacks(id),
        seq INTEGER NOT NULL,
        name_id INTEGER NOT NULL REFERENCES names(id),
        damaging INTEGER NOT NULL DEFAULT 0,
        creature_id INTEGER REFERENCES creatures(id),
        weapon TEXT,
        outcome TEXT,
        ours INTEGER NOT NULL DEFAULT 0,
        eligible INTEGER,
        source TEXT
    );
    CREATE INDEX flare_rows_attack ON flare_rows(attack_id);
    CREATE INDEX flare_rows_name ON flare_rows(name_id);
    CREATE VIEW flares AS
        SELECT f.id, f.attack_id, f.seq, n.name, f.damaging, f.creature_id, f.weapon,
               f.outcome, f.ours, f.eligible, f.source, f.name_id
        FROM flare_rows f JOIN names n ON n.id = f.name_id;
    CREATE TABLE hits (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        attack_id INTEGER NOT NULL REFERENCES attacks(id),
        flare_id INTEGER,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        creature_id INTEGER REFERENCES creatures(id),
        seq INTEGER NOT NULL,
        damage INTEGER NOT NULL,
        location TEXT,
        body_part TEXT,
        crit_type TEXT,
        crit_rank INTEGER,
        wound_rank INTEGER,
        fatal INTEGER NOT NULL DEFAULT 0,
        amputated INTEGER NOT NULL DEFAULT 0,
        secondary_location TEXT,
        secondary_rank INTEGER,
        line_seq INTEGER
    );
    CREATE INDEX hits_attack ON hits(attack_id);
    CREATE INDEX hits_creature ON hits(hunt_id, creature_id);
    CREATE INDEX hits_creature_only ON hits(creature_id);
    CREATE INDEX hits_flare ON hits(flare_id);
    CREATE TABLE statuses (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        creature_id INTEGER REFERENCES creatures(id),
        subject TEXT,
        attack_id INTEGER,
        flare_id INTEGER,
        occurred_at INTEGER NOT NULL,
        kind TEXT NOT NULL,
        status TEXT,
        action TEXT,
        value INTEGER,
        spell INTEGER,
        spell_name TEXT,
        cause TEXT,
        source TEXT
    );
    CREATE INDEX statuses_creature ON statuses(hunt_id, creature_id);
    CREATE INDEX statuses_attack ON statuses(attack_id);
    CREATE TABLE imports (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        stamp TEXT NOT NULL,
        path TEXT NOT NULL,
        size INTEGER NOT NULL,
        imported_at INTEGER NOT NULL,
        first_at INTEGER,
        last_at INTEGER,
        hunts INTEGER NOT NULL DEFAULT 0,
        pulses INTEGER NOT NULL DEFAULT 0
    );
    CREATE UNIQUE INDEX imports_session ON imports(instance, character, stamp);
    CREATE TABLE gemstone_rows (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        number INTEGER NOT NULL,
        description TEXT NOT NULL,
        binding TEXT,
        property_id INTEGER NOT NULL REFERENCES names(id),
        rank INTEGER,
        ranks INTEGER
    );
    CREATE INDEX gemstone_rows_hunt ON gemstone_rows(hunt_id);
    CREATE INDEX gemstone_rows_property ON gemstone_rows(property_id);
    CREATE VIEW hunt_gemstones AS
        SELECT g.id, g.hunt_id, g.number, g.description, g.binding, n.mnemonic, n.name,
               g.rank, g.ranks, g.property_id
        FROM gemstone_rows g JOIN names n ON n.id = g.property_id;
    CREATE TABLE hunt_effects (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        name_id INTEGER NOT NULL REFERENCES names(id),
        began_at INTEGER NOT NULL,
        ended_at INTEGER
    );
    CREATE INDEX hunt_effects_hunt ON hunt_effects(hunt_id, name_id);
    CREATE TABLE hunt_gear (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        hunt_id INTEGER NOT NULL REFERENCES hunts(id),
        gear_id INTEGER NOT NULL REFERENCES names(id),
        exist_id INTEGER,
        slot TEXT NOT NULL,
        began_at INTEGER NOT NULL,
        ended_at INTEGER
    );
    CREATE INDEX hunt_gear_hunt ON hunt_gear(hunt_id, gear_id);"#;

/// Migration to schema version 2 — verbatim from `0x103985832` (451 bytes).
pub const MIGRATION_2: &str = r#"CREATE TABLE login_rewards (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        game_time INTEGER,
        instance TEXT NOT NULL,
        character TEXT NOT NULL,
        day TEXT NOT NULL,
        streak INTEGER,
        bridges INTEGER,
        reward TEXT NOT NULL,
        thanks TEXT NOT NULL,
        line TEXT NOT NULL
    );
    CREATE UNIQUE INDEX login_rewards_day ON login_rewards(instance, character, day);"#;

/// Migration to schema version 3 — verbatim from `0x1039859f5` (362 bytes).
pub const MIGRATION_3: &str = r#"CREATE TABLE store_visits (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        at INTEGER NOT NULL,
        account TEXT NOT NULL,
        game TEXT NOT NULL,
        claimed INTEGER NOT NULL,
        message TEXT NOT NULL,
        balance TEXT NOT NULL,
        next_at INTEGER
    );
    CREATE INDEX store_visits_account ON store_visits(account, game, at);"#;


/// Put a column list into one of the `SELECT {}` templates, exactly as the
/// binary's `format!("SELECT {} FROM …")` does.
pub fn columns(template: &str, columns: &str) -> String {
    template.replacen("{}", columns, 1)
}

// ------------------------------------------------------------- column lists

/// `pulses`, in `SELECT` order — `(ptr, len)` pair at `0x1044423c0`, text at
/// `0x103982ee9`, 160 bytes.
pub const PULSE_COLUMNS: &str =
    "at, game_time, instance, character, exp, axp, exp_gained, axp_gained, fxp, fxp_max, \
     until_next, lumnis, rpa, fashlonae, mind_percent, mind_text, source, instant";

/// `login_rewards`, in `SELECT` order — pair at `0x104442430`, text at
/// `0x103982fdf`, 78 bytes.
pub const REWARD_COLUMNS: &str =
    "at, game_time, instance, character, day, streak, bridges, reward, thanks, line";

/// `snapshots`, in `SELECT` order — pair at `0x104442458`, text at
/// `0x10398323e`, 68 bytes.
pub const SNAPSHOT_COLUMNS: &str = "at, game_time, instance, character, kind, name, value, weekly, total";

// ------------------------------------------------------------------ queries

/// `Reader::characters` — `0x103982e78`, 113 bytes (`mov w3, #0x71`; ends
/// where the pulse column list starts at `0x103982ee9`).
pub const CHARACTERS: &str = "SELECT instance, character FROM pulses\n                 \
     UNION SELECT instance, character FROM hunts ORDER BY 1, 2";

/// `Reader::has_reward_on` — `0x103982f89`, 86 bytes (`mov w3, #0x56`).
pub const HAS_REWARD_ON: &str =
    "SELECT COUNT(*) FROM login_rewards WHERE instance = ?1 AND character = ?2 AND day = ?3";

/// `Reader::latest_rewards` template — format blob `0x10398302d`.
pub const LATEST_REWARDS: &str = "SELECT {} FROM login_rewards\n             \
     WHERE id IN (SELECT MAX(id) FROM login_rewards GROUP BY instance, character)\n             \
     ORDER BY instance, character";

/// `Reader::pulses_between` — `0x1039830d1`, 91 bytes (`mov w3, #0x5b`).
/// It is a `COUNT(*)`, not a row query: see `NOTES.md` §3.6.
pub const PULSES_BETWEEN: &str =
    "SELECT COUNT(*) FROM pulses WHERE instance = ?1 AND character = ?2 AND at BETWEEN ?3 AND ?4";

/// `Reader::hunt_overlapping` — `0x10398312c`…`0x10398323e`, 274 bytes.
pub const HUNT_OVERLAPPING: &str = "SELECT h.id FROM hunts h\n             \
     WHERE h.instance = ?1 AND h.character = ?2\n               \
     AND h.started_at <= ?4\n               \
     AND COALESCE(h.ended_at, (SELECT MAX(occurred_at) FROM attacks WHERE hunt_id = h.id), h.started_at) >= ?3\n             \
     ORDER BY h.id LIMIT 1";

/// `Reader::latest_snapshots` template — format blob `0x103983283` (the
/// `SELECT` at `0x10398327d`)
pub const LATEST_SNAPSHOTS: &str = "SELECT {} FROM snapshots\n             \
     WHERE id IN (SELECT MAX(id) FROM snapshots\n                          \
     WHERE instance = ?1 AND character = ?2\n                          \
     GROUP BY kind, name)\n             ORDER BY kind, name";

/// `Reader::latest_store_claims` — `0x103983367`…`0x103983458`, 241 bytes.
pub const LATEST_STORE_CLAIMS: &str = "SELECT at, account, game, claimed, message, balance, next_at FROM store_visits\n             \
     WHERE id IN (SELECT MAX(id) FROM store_visits WHERE claimed = 1\n                          \
     GROUP BY account, game)\n             ORDER BY account, game";

/// `Reader::latest_store_visits` — `0x103983458`…`0x10398351d`, 197 bytes.
pub const LATEST_STORE_VISITS: &str = "SELECT at, account, game, claimed, message, balance, next_at FROM store_visits\n             \
     WHERE id IN (SELECT MAX(id) FROM store_visits GROUP BY account, game)\n             \
     ORDER BY account, game";

/// The `store_visits` column list, in the order [`LATEST_STORE_CLAIMS`] and
/// [`LATEST_STORE_VISITS`] write it. Quoted here once so the store's view and
/// the two readers cannot drift apart.
pub const STORE_VISIT_COLUMNS: &str = "at, account, game, claimed, message, balance, next_at";

/// `Reader::pulses` template — format blob `0x103e6e4d2`.
pub const PULSES: &str = "SELECT {} FROM pulses\n             \
     WHERE instance = ?1 AND character = ?2 AND at >= ?3 AND at < ?4\n             \
     ORDER BY at, id";

/// `Reader::last_pulses` template — format blob `0x103e6e457`.
pub const LAST_PULSES: &str = "SELECT {} FROM pulses\n             \
     WHERE instance = ?1 AND character = ?2\n             \
     ORDER BY at DESC, id DESC LIMIT ?3";

/// `Reader::rewards` template — format blob `0x103e6e553`.
pub const REWARDS: &str = "SELECT {} FROM login_rewards\n             \
     WHERE instance = ?1 AND character = ?2\n             \
     ORDER BY at DESC, id DESC LIMIT ?3";

/// `Reader::imported` — `0x10398351d`, 82 bytes.
pub const IMPORTED: &str =
    "SELECT COUNT(*) FROM imports WHERE instance = ?1 AND character = ?2 AND stamp = ?3";

/// `Reader::has_pulse` — `0x10398356f`, 91 bytes.
pub const HAS_PULSE: &str =
    "SELECT COUNT(*) FROM pulses WHERE instance = ?1 AND character = ?2 AND at = ?3 AND exp = ?4";

/// `Reader::snapshots` template — format blob `0x1039835ca`.
pub const SNAPSHOTS: &str = "SELECT {} FROM snapshots\n             \
     WHERE instance = ?1 AND character = ?2 AND kind = ?3 AND at >= ?4 AND at < ?5\n             \
     ORDER BY at, id";

// ------------------------------------------------------------------- writes

/// `Ledger::note_import` — the `imports` insert, `0x1039857a5`, 185 bytes
/// (`mov w3, #0xb9`).
pub const INSERT_IMPORT: &str = "INSERT INTO imports (instance, character, stamp, path, size, imported_at, first_at,\n                \
     last_at, hunts, pulses)\n             \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)";

/// `Ledger::prune_pulses(false)` — `0x103983763`, 108 bytes.
pub const DELETE_EMPTY_PULSES: &str = "DELETE FROM pulses WHERE instance = ?1 AND character = ?2\n             \
     AND exp_gained = 0 AND axp_gained = 0";

/// `Ledger::prune_pulses(true)` — `0x1039837cf`, 57 bytes.
pub const DELETE_ALL_PULSES: &str =
    "DELETE FROM pulses WHERE instance = ?1 AND character = ?2";

/// `writer::run` — the `pulses` insert, 303 bytes (`0x103985cd9`).
pub const INSERT_PULSE: &str = "INSERT INTO pulses (at, game_time, instance, character, exp, axp, exp_gained,\n            \
     axp_gained, fxp, fxp_max, until_next, lumnis, rpa, fashlonae, mind_percent,\n            \
     mind_text, source, instant)\n         \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)";

/// `writer::run` — the `store_visits` insert, 125 bytes (`0x103985e08`).
pub const INSERT_STORE_VISIT: &str = "INSERT INTO store_visits (at, account, game, claimed, message, balance, next_at)\n         \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)";

/// `writer::run` — the `login_rewards` upsert, 518 bytes (`0x103985e85`;
/// the next five bytes are a second copy of the `favor` literal).
pub const INSERT_REWARD: &str = "INSERT INTO login_rewards (at, game_time, instance, character, day, streak,\n            \
     bridges, reward, thanks, line)\n         \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)\n         \
     ON CONFLICT (instance, character, day) DO UPDATE SET\n            \
     at = excluded.at, game_time = excluded.game_time, streak = excluded.streak,\n            \
     bridges = excluded.bridges, reward = excluded.reward,\n            \
     thanks = excluded.thanks, line = excluded.line\n         \
     WHERE login_rewards.reward = '' AND excluded.reward <> ''";

/// `writer::run` — the `snapshots` insert, 157 bytes (`0x103986090`).
pub const INSERT_SNAPSHOT: &str = "INSERT INTO snapshots (at, game_time, instance, character, kind, name, value,\n            \
     weekly, total)\n         \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

/// `name_id`'s interning insert — `0x103985b97`, 82 bytes (`mov w3, #0x52`).
pub const INSERT_NAME: &str =
    "INSERT OR IGNORE INTO names (kind, name, number, mnemonic) VALUES (?1, ?2, ?3, ?4)";

/// `name_id`'s lookup — `0x103985be9`, 50 bytes (`mov w3, #0x32`).
pub const SELECT_NAME_ID: &str = "SELECT id FROM names WHERE kind = ?1 AND name = ?2";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_versions_are_the_three_recovered_batches() {
        assert_eq!(SCHEMA_VERSION, 3);
        assert_eq!(MIGRATIONS.len(), 3);
        assert_eq!(MIGRATIONS[0], MIGRATION_1);
        assert_eq!(BASE_SCHEMA, MIGRATION_1);
    }

    #[test]
    fn migration_one_creates_every_recovered_table_and_view() {
        let tables = [
            "CREATE TABLE pulses (",
            "CREATE TABLE snapshots (",
            "CREATE TABLE names (",
            "CREATE TABLE hunts (",
            "CREATE TABLE creatures (",
            "CREATE TABLE attacks (",
            "CREATE TABLE resolutions (",
            "CREATE TABLE flare_rows (",
            "CREATE TABLE hits (",
            "CREATE TABLE statuses (",
            "CREATE TABLE imports (",
            "CREATE TABLE gemstone_rows (",
            "CREATE TABLE hunt_effects (",
            "CREATE TABLE hunt_gear (",
        ];
        for table in tables {
            assert!(
                MIGRATION_1.contains(table),
                "missing {table:?} in the base schema"
            );
        }
        for view in ["CREATE VIEW flares AS", "CREATE VIEW hunt_gemstones AS"] {
            assert!(MIGRATION_1.contains(view), "missing {view:?}");
        }
        for index in [
            "CREATE INDEX pulses_who_at ON pulses(instance, character, at);",
            "CREATE INDEX snapshots_who_kind_at ON snapshots(instance, character, kind, at);",
            "CREATE UNIQUE INDEX names_kind_name ON names(kind, name);",
            "CREATE INDEX names_mnemonic ON names(kind, mnemonic);",
            "CREATE INDEX hunts_who ON hunts(instance, character, id);",
            "CREATE UNIQUE INDEX imports_session ON imports(instance, character, stamp);",
        ] {
            assert!(MIGRATION_1.contains(index), "missing {index:?}");
        }
        // The lengths are the `(ptr, len)` pair's, so they pin the text exactly.
        assert_eq!(MIGRATION_1.len(), 8190);
        assert_eq!(MIGRATION_2.len(), 451);
        assert_eq!(MIGRATION_3.len(), 362);
    }

    #[test]
    fn the_pulse_table_has_the_eighteen_selected_columns() {
        assert_eq!(PULSE_COLUMNS.split(", ").count(), 18);
        for column in PULSE_COLUMNS.split(", ") {
            assert!(
                MIGRATION_1.contains(column.trim()),
                "{column} is selected but not created"
            );
        }
        assert_eq!(REWARD_COLUMNS.split(", ").count(), 10);
        assert_eq!(SNAPSHOT_COLUMNS.split(", ").count(), 9);
        assert!(MIGRATION_2.contains("CREATE TABLE login_rewards ("));
        assert!(MIGRATION_3.contains("CREATE TABLE store_visits ("));
    }

    #[test]
    fn columns_fills_the_select_template() {
        assert_eq!(
            columns("SELECT {} FROM pulses", PULSE_COLUMNS),
            format!("SELECT {PULSE_COLUMNS} FROM pulses")
        );
    }

    #[test]
    fn the_store_visit_column_list_is_the_one_the_two_readers_select() {
        assert_eq!(STORE_VISIT_COLUMNS.split(", ").count(), 7);
        assert!(LATEST_STORE_CLAIMS.contains(STORE_VISIT_COLUMNS));
        assert!(LATEST_STORE_VISITS.contains(STORE_VISIT_COLUMNS));
        assert_eq!(STORE_VISIT_COLUMNS.len(), 53);
        // Each column is created by migration 3.
        for column in STORE_VISIT_COLUMNS.split(", ") {
            assert!(MIGRATION_3.contains(column.trim()), "{column} is not created");
        }
    }

    #[test]
    fn every_query_names_the_parameters_its_reader_binds() {
        // `?n` placeholders are how the reader methods' bind order is checked.
        assert_eq!(PULSES_BETWEEN.matches('?').count(), 4);
        assert_eq!(HAS_PULSE.matches('?').count(), 4);
        assert_eq!(HAS_REWARD_ON.matches('?').count(), 3);
        assert_eq!(IMPORTED.matches('?').count(), 3);
        assert_eq!(HUNT_OVERLAPPING.matches('?').count(), 4);
        assert_eq!(INSERT_IMPORT.matches('?').count(), 10);
        assert_eq!(INSERT_PULSE.matches('?').count(), 18);
        assert_eq!(INSERT_SNAPSHOT.matches('?').count(), 9);
        assert_eq!(INSERT_STORE_VISIT.matches('?').count(), 7);
        assert_eq!(INSERT_REWARD.matches('?').count(), 10);
        assert_eq!(INSERT_NAME.matches('?').count(), 4);
        assert_eq!(SELECT_NAME_ID.matches('?').count(), 2);
        // The orderings that the readers depend on.
        assert!(LAST_PULSES.contains("ORDER BY at DESC, id DESC LIMIT ?3"));
        assert!(PULSES.contains("ORDER BY at, id"));
        assert!(SNAPSHOTS.contains("ORDER BY at, id"));
        assert!(LATEST_REWARDS.contains("GROUP BY instance, character"));
        assert!(LATEST_SNAPSHOTS.contains("GROUP BY kind, name"));
        assert!(INSERT_REWARD.contains("WHERE login_rewards.reward = '' AND excluded.reward <> ''"));
        // Every length below is either a `mov wN, #len` immediate in front of
        // `prepare_with_flags`/`execute`, or the distance to the next literal in
        // `__const`, so they pin the transcribed text exactly.
        assert_eq!(CHARACTERS.len(), 113);
        assert_eq!(HAS_REWARD_ON.len(), 86);
        assert_eq!(PULSES_BETWEEN.len(), 91);
        assert_eq!(HUNT_OVERLAPPING.len(), 274);
        assert_eq!(LATEST_STORE_CLAIMS.len(), 241);
        assert_eq!(LATEST_STORE_VISITS.len(), 197);
        assert_eq!(IMPORTED.len(), 82);
        assert_eq!(HAS_PULSE.len(), 91);
        assert_eq!(DELETE_ALL_PULSES.len(), 57);
        assert_eq!(DELETE_EMPTY_PULSES.len(), 108);
        assert_eq!(INSERT_IMPORT.len(), 185);
        assert_eq!(INSERT_PULSE.len(), 303);
        assert_eq!(INSERT_STORE_VISIT.len(), 125);
        assert_eq!(INSERT_REWARD.len(), 518);
        assert_eq!(INSERT_SNAPSHOT.len(), 157);
        assert_eq!(INSERT_NAME.len(), 82);
        assert_eq!(SELECT_NAME_ID.len(), 50);
        assert_eq!(USER_VERSION.len(), 19);
        assert_eq!(PULSE_COLUMNS.len(), 160);
        assert_eq!(REWARD_COLUMNS.len(), 78);
        assert_eq!(SNAPSHOT_COLUMNS.len(), 68);
        assert_eq!(DB_FILE.len(), 8);
        assert_eq!(WRITER_THREAD.len(), 12);
        assert_eq!(USER_VERSION, "PRAGMA user_version");
        assert_eq!(DB_FILE, "hydra.db");
        assert_eq!(WRITER_THREAD, "hydra-ledger");
    }
}
