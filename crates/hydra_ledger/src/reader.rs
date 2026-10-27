//! `hydra_ledger::reader` — the read half of the ledger.
//!
//! [`Reader`] is a one-field wrapper around a read-only connection
//! (`Ledger::reader` at `0x10168e934` is `connect(true)` plus a move), and every
//! method below is one of the crate's recovered `SELECT`s, run through
//! `rusqlite` (**reconnected in Phase 1**): the text comes from
//! [`crate::schema`], the statement goes through `StatementCache::get`
//! (`Connection::prepare_cached` — the cache the binary indexes at `+0x20` in
//! the connection), and the rows are mapped with `query_map`/`query_row`.
//!
//! The predicate, ordering and limit of each query are the ones the SQL states,
//! so SQLite does the work. `NOTES.md` §3 has the per-query table and §7 the
//! evidence conflicts (`pulses_between` is a `COUNT(*)`, `last_pulses` is
//! newest-first, `latest_rewards` binds nothing).

use crate::row::{Pulse, Reward, Snapshot, SnapshotKind, StoreVisit, Who};
use crate::schema;
use crate::store::{
    pulse_from_row, reward_from_row, snapshot_from_row, store_visit_from_row, Connection,
};
use crate::Result;

/// The read handle.
///
/// The original is `struct Reader(Connection)`; `Reader::characters`'
/// `StatementCache::get(&self.cache /* +0x20 */, &self /* the connection */,
/// sql)` proves the connection is the first field and the statement cache lives
/// on it.
#[derive(Debug)]
pub struct Reader {
    conn: Connection,
}

impl Reader {
    /// Wrap an already-open connection (what `Ledger::reader` does).
    pub fn new(conn: Connection) -> Reader {
        Reader { conn }
    }

    /// The connection underneath (read-only).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// `Reader::characters(&self) -> Result<Vec<Who>>` — **full**.
    ///
    /// `@0x10168b4d8` (256 bytes), SQL [`schema::CHARACTERS`] (113 bytes,
    /// `mov w3, #0x71`), no binds (`bind_parameter_count == 0` → the
    /// `try_process` at `0x101696e54`), each row mapped to
    /// `(instance, character)`.
    ///
    /// ```sql
    /// SELECT instance, character FROM pulses
    ///                  UNION SELECT instance, character FROM hunts ORDER BY 1, 2
    /// ```
    pub fn characters(&self) -> Result<Vec<Who>> {
        let mut stmt = self.conn.raw().prepare_cached(schema::CHARACTERS)?;
        let rows = stmt.query_map([], |row| {
            Ok(Who::new(
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::last_pulses(&self, who, limit) -> Result<Vec<Pulse>>` — **full**.
    ///
    /// `@0x10168b5d8` (680 bytes), [`schema::LAST_PULSES`] (format blob
    /// `0x103e6e457`) with three binds: `&who.instance`, `&who.character`, and
    /// `limit` — which is clamped with `cmp x22, #0x7fff…; csel` before the
    /// bind, i.e. `limit.min(i64::MAX as u64)`.
    ///
    /// The rows come back **newest first**: `ORDER BY at DESC, id DESC LIMIT ?3`
    /// is the recovered text, and the mapper is `reader::pulse`.
    pub fn last_pulses(&self, who: &Who, limit: u64) -> Result<Vec<Pulse>> {
        let sql = schema::columns(schema::LAST_PULSES, schema::PULSE_COLUMNS);
        let mut stmt = self.conn.raw().prepare_cached(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![
                who.instance,
                who.character,
                limit.min(i64::MAX as u64) as i64
            ],
            pulse_from_row,
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::rewards(&self, who, limit) -> Result<Vec<Reward>>` — **full**.
    ///
    /// `@0x10168c874` (680 bytes), [`schema::REWARDS`] (format blob
    /// `0x103e6e553`), three binds like [`Reader::last_pulses`] and the same
    /// saturation.
    pub fn rewards(&self, who: &Who, limit: u64) -> Result<Vec<Reward>> {
        let sql = schema::columns(schema::REWARDS, schema::REWARD_COLUMNS);
        let mut stmt = self.conn.raw().prepare_cached(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![
                who.instance,
                who.character,
                limit.min(i64::MAX as u64) as i64
            ],
            reward_from_row,
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::has_reward_on(&self, who, day) -> Result<i64>` — **full**.
    ///
    /// `@0x10168b880` (316 bytes), [`schema::HAS_REWARD_ON`] (86 bytes,
    /// `mov w3, #0x56`), three binds (`mov w2, #0x3`) → `query_row` →
    /// `Result<i64>` (the two-word `niche + i64` result).
    pub fn has_reward_on(&self, who: &Who, day: &str) -> Result<i64> {
        let mut stmt = self.conn.raw().prepare_cached(schema::HAS_REWARD_ON)?;
        Ok(stmt.query_row(
            rusqlite::params![who.instance, who.character, day],
            |row| row.get(0),
        )?)
    }

    /// `Reader::latest_rewards(&self) -> Result<Vec<Reward>>` — **full**.
    ///
    /// `@0x10168b9bc` (372 bytes), [`schema::LATEST_REWARDS`] (format blob
    /// `0x10398302d`). No binds at all (`bind_parameter_count == 0`), so this
    /// one is **not** filtered by character:
    ///
    /// ```sql
    /// SELECT … FROM login_rewards
    ///  WHERE id IN (SELECT MAX(id) FROM login_rewards GROUP BY instance, character)
    ///  ORDER BY instance, character
    /// ```
    pub fn latest_rewards(&self) -> Result<Vec<Reward>> {
        let sql = schema::columns(schema::LATEST_REWARDS, schema::REWARD_COLUMNS);
        let mut stmt = self.conn.raw().prepare_cached(&sql)?;
        let rows = stmt.query_map([], reward_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::has_pulse(&self, who, at, exp) -> Result<i64>` — **full**.
    ///
    /// `@0x10168cc58` (340 bytes), [`schema::HAS_PULSE`] (91 bytes,
    /// `mov w3, #0x5b`), four binds (`mov w2, #0x4`) → `Result<i64>`.
    pub fn has_pulse(&self, who: &Who, at: i64, exp: i64) -> Result<i64> {
        let mut stmt = self.conn.raw().prepare_cached(schema::HAS_PULSE)?;
        Ok(stmt.query_row(
            rusqlite::params![who.instance, who.character, at, exp],
            |row| row.get(0),
        )?)
    }

    /// `Reader::hunt_overlapping(&self, who, from, to) -> Result<Option<i64>>` —
    /// **full**.
    ///
    /// `@0x10168bc28` (1356 bytes, most of it the `format!`, the four
    /// `bind_parameter` chains and the `MappedRows` machinery),
    /// [`schema::HUNT_OVERLAPPING`] (274 bytes, `mov w3, #0x112`), four binds.
    /// The rows are consumed with `query_map` + `next().transpose()`
    /// (`drop_in_place<MappedRows<hunt_overlapping::{{closure}}>>` at
    /// `0x10168dae4`, `Row::get::<i64>(0)` at `0x10168be40`), and the caller
    /// `hydra_import::import_into`'s closure (`0x100163684`) reads a two-word
    /// `Ok` payload, so the result is `Result<Option<i64>>`:
    ///
    /// ```sql
    /// SELECT h.id FROM hunts h
    ///  WHERE h.instance = ?1 AND h.character = ?2
    ///    AND h.started_at <= ?4
    ///    AND COALESCE(h.ended_at, (SELECT MAX(occurred_at) FROM attacks WHERE hunt_id = h.id), h.started_at) >= ?3
    ///  ORDER BY h.id LIMIT 1
    /// ```
    ///
    /// Note the argument order: `?3` is the *start* of the window and `?4` its
    /// end, so the predicate is "started at or before the window's end, and
    /// ended (or last attacked, or started) at or after the window's start".
    pub fn hunt_overlapping(&self, who: &Who, from: i64, to: i64) -> Result<Option<i64>> {
        let mut stmt = self.conn.raw().prepare_cached(schema::HUNT_OVERLAPPING)?;
        let mut rows = stmt.query_map(
            rusqlite::params![who.instance, who.character, from, to],
            |row| row.get::<_, i64>(0),
        )?;
        Ok(rows.next().transpose()?)
    }

    /// `Reader::latest_snapshots(&self, who) -> Result<Vec<Snapshot>>` —
    /// **full**.
    ///
    /// `@0x10168c174` (572 bytes), [`schema::LATEST_SNAPSHOTS`] (format blob
    /// `0x103983283`), two binds (`&who.instance`, `&who.character`).
    ///
    /// ```sql
    /// SELECT … FROM snapshots
    ///  WHERE id IN (SELECT MAX(id) FROM snapshots
    ///               WHERE instance = ?1 AND character = ?2
    ///               GROUP BY kind, name)
    ///  ORDER BY kind, name
    /// ```
    ///
    /// `GROUP BY kind, name` groups `NULL` names together, and SQLite sorts
    /// `NULL` before any text, which is why the sort puts `None` first.
    pub fn latest_snapshots(&self, who: &Who) -> Result<Vec<Snapshot>> {
        let sql = schema::columns(schema::LATEST_SNAPSHOTS, schema::SNAPSHOT_COLUMNS);
        let mut stmt = self.conn.raw().prepare_cached(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![who.instance, who.character],
            snapshot_from_row,
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::latest_store_claims(&self) -> Result<Vec<StoreVisit>>` —
    /// **full**.
    ///
    /// `@0x10168c3b0` (256 bytes), [`schema::LATEST_STORE_CLAIMS`] (241 bytes),
    /// no binds. Store visits belong to an **account**, not a character, which
    /// is why there is no `Who` here.
    pub fn latest_store_claims(&self) -> Result<Vec<StoreVisit>> {
        self.store_visits(schema::LATEST_STORE_CLAIMS)
    }

    /// `Reader::latest_store_visits(&self) -> Result<Vec<StoreVisit>>` —
    /// **full**.
    ///
    /// `@0x10168c4b0` (256 bytes), [`schema::LATEST_STORE_VISITS`] (197 bytes),
    /// no binds — the same query without the `claimed = 1` filter.
    pub fn latest_store_visits(&self) -> Result<Vec<StoreVisit>> {
        self.store_visits(schema::LATEST_STORE_VISITS)
    }

    fn store_visits(&self, sql: &str) -> Result<Vec<StoreVisit>> {
        let mut stmt = self.conn.raw().prepare_cached(sql)?;
        let rows = stmt.query_map([], store_visit_from_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::pulses(&self, who, from, to) -> Result<Vec<Pulse>>` — **full**.
    ///
    /// `@0x10168c5b0` (708 bytes), [`schema::PULSES`] (format blob
    /// `0x103e6e4d2`), four binds: `&who.instance`, `&who.character`, `from`,
    /// `to` (`stp x2, x3, [sp, #0x8]` — the two `i64`s).
    ///
    /// This is the half-open window: `at >= ?3 AND at < ?4`, oldest first
    /// (`ORDER BY at, id`). [`Reader::pulses_between`] is the *closed* count.
    pub fn pulses(&self, who: &Who, from: i64, to: i64) -> Result<Vec<Pulse>> {
        let sql = schema::columns(schema::PULSES, schema::PULSE_COLUMNS);
        let mut stmt = self.conn.raw().prepare_cached(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![who.instance, who.character, from, to],
            pulse_from_row,
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// `Reader::pulses_between(&self, who, from, to) -> Result<i64>` — **full**.
    ///
    /// `@0x10168bb30` (248 bytes), [`schema::PULSES_BETWEEN`] (91 bytes,
    /// `mov w3, #0x5b`), four binds (`mov w2, #0x4`) → `query_row` → `i64`.
    ///
    /// Despite the name this is `SELECT COUNT(*)`, not a row query: the row
    /// query is [`Reader::pulses`]. The window is **closed** here
    /// (`at BETWEEN ?3 AND ?4`). `NOTES.md` §7.1 records this.
    pub fn pulses_between(&self, who: &Who, from: i64, to: i64) -> Result<i64> {
        let mut stmt = self.conn.raw().prepare_cached(schema::PULSES_BETWEEN)?;
        Ok(stmt.query_row(
            rusqlite::params![who.instance, who.character, from, to],
            |row| row.get(0),
        )?)
    }

    /// `Reader::imported(&self, who, stamp) -> Result<i64>` — **full**.
    ///
    /// `@0x10168cb1c` (316 bytes), [`schema::IMPORTED`] (82 bytes,
    /// `mov w3, #0x52`), three binds (`mov w2, #0x3`) → `Result<i64>`.
    pub fn imported(&self, who: &Who, stamp: &str) -> Result<i64> {
        let mut stmt = self.conn.raw().prepare_cached(schema::IMPORTED)?;
        Ok(stmt.query_row(
            rusqlite::params![who.instance, who.character, stamp],
            |row| row.get(0),
        )?)
    }

    /// `Reader::snapshots(&self, who, kind, from, to) -> Result<Vec<Snapshot>>`
    /// — **full**.
    ///
    /// `@0x10168cdac` (5,420 bytes — the size is the `Format::format` call plus
    /// five `bind_parameter` chains, not extra logic), [`schema::SNAPSHOTS`]
    /// (format blob `0x1039835ca`), five binds:
    ///
    /// ```text
    /// ?1 &who.instance      ?2 &who.character
    /// ?3 SnapshotKind name  ?4 from          ?5 to
    /// ```
    ///
    /// `kind` is bound as its **text** (`ubfiz x8, x21, #3, #8` → the
    /// three-entry name table, `stp x9, x8, [sp, #0x58]`), and the window is
    /// half-open (`at >= ?4 AND at < ?5`).
    pub fn snapshots(
        &self,
        who: &Who,
        kind: SnapshotKind,
        from: i64,
        to: i64,
    ) -> Result<Vec<Snapshot>> {
        let sql = schema::columns(schema::SNAPSHOTS, schema::SNAPSHOT_COLUMNS);
        let mut stmt = self.conn.raw().prepare_cached(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params![who.instance, who.character, kind.name(), from, to],
            snapshot_from_row,
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Source;
    use crate::store::{Connection, Hunt, Import};

    const PRIME: &str = "prime";
    const SINDAR: &str = "Sindar";

    fn who() -> Who {
        Who::new(PRIME, SINDAR)
    }

    fn pulse(at: i64) -> Pulse {
        Pulse {
            instance: PRIME.to_string(),
            character: SINDAR.to_string(),
            at,
            exp: 1_000 + at,
            exp_gained: 100,
            ..Pulse::default()
        }
    }

    /// A reader over a fixed set of rows, plus the id of its single hunt.
    fn reader() -> (Reader, i64) {
        let conn = Connection::in_memory(false);
        for at in [100, 200, 300, 400, 500] {
            conn.insert_pulse(pulse(at)).unwrap();
        }
        // Another character's pulse, never visible through `who()`.
        conn.insert_pulse(Pulse {
            instance: PRIME.to_string(),
            character: "Someone".to_string(),
            at: 150,
            ..Pulse::default()
        })
        .unwrap();
        // Same instance, wrong instance name.
        conn.insert_pulse(Pulse {
            instance: "test".to_string(),
            character: SINDAR.to_string(),
            at: 250,
            ..Pulse::default()
        })
        .unwrap();
        let hunt = conn
            .insert_hunt(Hunt {
                instance: PRIME.to_string(),
                character: SINDAR.to_string(),
                started_at: 1_000,
                ended_at: Some(2_000),
            })
            .unwrap();
        (Reader::new(conn), hunt)
    }

    #[test]
    fn the_pulse_windows_are_half_open_and_newest_first_respectively() {
        let (reader, _) = reader();
        // `pulses` is at >= from AND at < to, oldest first.
        let window = reader.pulses(&who(), 200, 500).unwrap();
        assert_eq!(
            window.iter().map(|p| p.at).collect::<Vec<_>>(),
            vec![200, 300, 400]
        );
        // `last_pulses` is newest first and limited.
        let last = reader.last_pulses(&who(), 2).unwrap();
        assert_eq!(last.iter().map(|p| p.at).collect::<Vec<_>>(), vec![500, 400]);
        // `limit` saturates rather than overflowing.
        assert_eq!(reader.last_pulses(&who(), u64::MAX).unwrap().len(), 5);
        // `pulses_between` is a closed count.
        assert_eq!(reader.pulses_between(&who(), 200, 500).unwrap(), 4);
        assert_eq!(reader.pulses_between(&who(), 500, 500).unwrap(), 1);
        assert_eq!(reader.pulses_between(&who(), 501, 900).unwrap(), 0);
    }

    #[test]
    fn has_pulse_matches_at_and_exp_together() {
        let (reader, _) = reader();
        assert_eq!(reader.has_pulse(&who(), 300, 1_300).unwrap(), 1);
        assert_eq!(reader.has_pulse(&who(), 300, 1_301).unwrap(), 0);
        assert_eq!(reader.has_pulse(&who(), 301, 1_301).unwrap(), 0);
    }

    #[test]
    fn characters_unions_pulses_and_hunts_and_is_ordered() {
        let (reader, _) = reader();
        assert_eq!(
            reader.characters().unwrap(),
            vec![
                Who::new("prime", "Sindar"),
                Who::new("prime", "Someone"),
                Who::new("test", "Sindar"),
            ]
        );
    }

    #[test]
    fn hunt_overlapping_uses_the_closed_window_and_the_coalesce() {
        let (reader, hunt) = reader();
        // The hunt runs 1000..=2000.
        assert_eq!(
            reader.hunt_overlapping(&who(), 1_500, 1_600).unwrap(),
            Some(hunt)
        );
        // Touching the boundaries counts (`<= ?4` / `>= ?3`).
        assert_eq!(
            reader.hunt_overlapping(&who(), 2_000, 2_000).unwrap(),
            Some(hunt)
        );
        assert_eq!(
            reader.hunt_overlapping(&who(), 1_000, 9_999).unwrap(),
            Some(hunt)
        );
        // Entirely before or after does not.
        assert_eq!(reader.hunt_overlapping(&who(), 2_001, 3_000).unwrap(), None);
        assert_eq!(reader.hunt_overlapping(&who(), 500, 999).unwrap(), None);
        // Another character sees nothing.
        assert_eq!(
            reader
                .hunt_overlapping(&Who::new(PRIME, "Someone"), 1_500, 1_600)
                .unwrap(),
            None
        );
    }

    #[test]
    fn hunt_overlapping_falls_back_to_the_last_attack_then_the_start() {
        let conn = Connection::in_memory(false);
        let id = conn
            .insert_hunt(Hunt {
                instance: PRIME.to_string(),
                character: SINDAR.to_string(),
                started_at: 1_000,
                ended_at: None,
            })
            .unwrap();
        conn.insert_attack(crate::store::Attack {
            hunt_id: id,
            occurred_at: 4_000,
        })
        .unwrap();
        let reader = Reader::new(conn);
        // An open hunt with an attack at 4,000 lasts until 4,000.
        assert_eq!(reader.hunt_overlapping(&who(), 3_500, 3_600).unwrap(), Some(id));
        assert_eq!(reader.hunt_overlapping(&who(), 4_100, 4_200).unwrap(), None);
    }

    #[test]
    fn snapshots_are_filtered_by_kind_and_the_half_open_window() {
        let conn = Connection::in_memory(false);
        for (kind, at, value) in [
            (SnapshotKind::Favor, 10, 50),
            (SnapshotKind::Favor, 20, 60),
            (SnapshotKind::Favor, 30, 70),
            (SnapshotKind::Resource, 20, 1),
        ] {
            conn.insert_snapshot(Snapshot {
                instance: PRIME.to_string(),
                character: SINDAR.to_string(),
                kind,
                at,
                value: Some(value),
                name: Some("reading".to_string()),
                ..Snapshot::default()
            })
            .unwrap();
        }
        let reader = Reader::new(conn);
        let favor = reader
            .snapshots(&who(), SnapshotKind::Favor, 20, 30)
            .unwrap();
        assert_eq!(favor.iter().map(|s| s.value).collect::<Vec<_>>(), vec![Some(60)]);
        let resource = reader
            .snapshots(&who(), SnapshotKind::Resource, 0, i64::MAX)
            .unwrap();
        assert_eq!(resource.len(), 1);
        assert_eq!(resource[0].value, Some(1));
    }

    #[test]
    fn latest_snapshots_keeps_the_highest_id_per_kind_and_name() {
        let conn = Connection::in_memory(false);
        let insert = |kind, name: Option<&str>, at: i64, value: i64| {
            conn.insert_snapshot(Snapshot {
                instance: PRIME.to_string(),
                character: SINDAR.to_string(),
                kind,
                name: name.map(|n| n.to_string()),
                at,
                value: Some(value),
                ..Snapshot::default()
            })
            .unwrap();
        };
        insert(SnapshotKind::Favor, None, 10, 1);
        insert(SnapshotKind::Favor, None, 20, 2);
        insert(SnapshotKind::Suffused, Some("Essence"), 10, 3);
        insert(SnapshotKind::Resource, Some("Points"), 10, 4);
        // Another character's snapshots never show up.
        conn.insert_snapshot(Snapshot {
            instance: PRIME.to_string(),
            character: "Someone".to_string(),
            kind: SnapshotKind::Favor,
            at: 30,
            value: Some(9),
            ..Snapshot::default()
        })
        .unwrap();
        let reader = Reader::new(conn);
        let latest = reader.latest_snapshots(&who()).unwrap();
        assert_eq!(latest.len(), 3);
        // ORDER BY kind, name: favor(null) < resource < suffused.
        assert_eq!(latest[0].kind, SnapshotKind::Favor);
        assert_eq!(latest[0].value, Some(2));
        assert_eq!(latest[0].name, None);
        assert_eq!(latest[1].name.as_deref(), Some("Points"));
        assert_eq!(latest[2].name.as_deref(), Some("Essence"));
    }

    #[test]
    fn rewards_are_newest_first_and_latest_rewards_ignores_the_character() {
        let conn = Connection::in_memory(false);
        for (day, at, reward) in [
            ("2026-07-18", 10, ""),
            ("2026-07-19", 20, "a trinket"),
            ("2026-07-20", 30, "a locket"),
        ] {
            conn.insert_reward(Reward {
                instance: PRIME.to_string(),
                character: SINDAR.to_string(),
                day: day.to_string(),
                at,
                reward: reward.to_string(),
                ..Reward::default()
            })
            .unwrap();
        }
        conn.insert_reward(Reward {
            instance: PRIME.to_string(),
            character: "Someone".to_string(),
            day: "2026-07-20".to_string(),
            at: 40,
            reward: "a coin".to_string(),
            ..Reward::default()
        })
        .unwrap();
        let reader = Reader::new(conn);
        let rewards = reader.rewards(&who(), 10).unwrap();
        assert_eq!(
            rewards.iter().map(|r| r.day.as_str()).collect::<Vec<_>>(),
            vec!["2026-07-20", "2026-07-19", "2026-07-18"]
        );
        assert_eq!(reader.has_reward_on(&who(), "2026-07-19").unwrap(), 1);
        assert_eq!(reader.has_reward_on(&who(), "2026-07-21").unwrap(), 0);
        // No binds in the SQL: both characters, ordered by instance, character.
        let latest = reader.latest_rewards().unwrap();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].character, "Sindar");
        assert_eq!(latest[0].day, "2026-07-20");
        assert_eq!(latest[1].character, "Someone");
    }

    #[test]
    fn store_visits_latest_is_per_account_and_game() {
        let conn = Connection::in_memory(false);
        let visit = |account: &str, game: &str, claimed: bool, at: i64| {
            conn.insert_store_visit(StoreVisit {
                account: account.to_string(),
                game: game.to_string(),
                claimed,
                at,
                message: format!("visit {at}"),
                ..StoreVisit::default()
            })
            .unwrap();
        };
        visit("acct", "prime", false, 10);
        visit("acct", "prime", true, 20);
        visit("acct", "test", false, 30);
        let reader = Reader::new(conn);
        let all = reader.latest_store_visits().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].game, "prime");
        assert_eq!(all[0].at, 20);
        assert_eq!(all[1].game, "test");
        // `claimed = 1` keeps only the prime row.
        let claimed = reader.latest_store_claims().unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].game, "prime");
        assert_eq!(claimed[0].at, 20);
    }

    #[test]
    fn imported_counts_the_stamp() {
        let conn = Connection::in_memory(false);
        conn.insert_import(Import {
            instance: PRIME.to_string(),
            character: SINDAR.to_string(),
            stamp: "2026-07-20T13:05:52".to_string(),
            ..Import::default()
        })
        .unwrap();
        let reader = Reader::new(conn);
        assert_eq!(reader.imported(&who(), "2026-07-20T13:05:52").unwrap(), 1);
        assert_eq!(reader.imported(&who(), "2026-07-21T00:00:00").unwrap(), 0);
        assert_eq!(
            reader
                .imported(&Who::new(PRIME, "Someone"), "2026-07-20T13:05:52")
                .unwrap(),
            0
        );
    }

    #[test]
    fn the_pulse_mapper_keeps_every_column() {
        let conn = Connection::in_memory(false);
        conn.insert_pulse(Pulse {
            instance: PRIME.to_string(),
            character: SINDAR.to_string(),
            at: 1_784_567_152,
            game_time: Some(1_417),
            exp: 5_000_000,
            axp: 12,
            exp_gained: 1_200,
            axp_gained: 3,
            fxp: Some(10),
            fxp_max: Some(99),
            until_next: Some(250_000),
            lumnis: Some(2),
            rpa: Some("1.25x".to_string()),
            fashlonae: Some(1),
            mind_percent: Some(97),
            mind_text: Some("must rest".to_string()),
            source: Source::Bounty,
            instant: 0,
        })
        .unwrap();
        let row = &reader_from(conn).last_pulses(&who(), 1).unwrap()[0];
        assert_eq!(row.game_time, Some(1_417));
        assert_eq!(row.fxp_max, Some(99));
        assert_eq!(row.rpa.as_deref(), Some("1.25x"));
        assert_eq!(row.mind_text.as_deref(), Some("must rest"));
        assert_eq!(row.mind_percent, Some(97));
        assert_eq!(row.source, Source::Bounty);
    }

    /// The recovered readers against a real `hydra.db` file: a writable
    /// connection writes, a separate read-only one reads it back.
    #[test]
    fn the_readers_see_what_another_connection_committed() {
        let dir = std::env::temp_dir().join(format!(
            "hydra-ledger-reader-file-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = crate::Ledger::open(&dir).unwrap();
        {
            let conn = ledger.writing().unwrap();
            for at in [100, 200, 300] {
                conn.insert_pulse(pulse(at)).unwrap();
            }
            conn.insert_snapshot(Snapshot {
                instance: PRIME.to_string(),
                character: SINDAR.to_string(),
                kind: SnapshotKind::Favor,
                at: 50,
                value: Some(42),
                ..Snapshot::default()
            })
            .unwrap();
        }
        let reader = ledger.reader().unwrap();
        assert_eq!(
            reader
                .pulses(&who(), 0, i64::MAX)
                .unwrap()
                .iter()
                .map(|p| p.at)
                .collect::<Vec<_>>(),
            vec![100, 200, 300]
        );
        assert_eq!(reader.has_pulse(&who(), 200, 1_200).unwrap(), 1);
        assert_eq!(reader.pulses_between(&who(), 100, 300).unwrap(), 3);
        assert_eq!(
            reader
                .snapshots(&who(), SnapshotKind::Favor, 0, i64::MAX)
                .unwrap()[0]
                .value,
            Some(42)
        );
        assert_eq!(reader.characters().unwrap(), vec![who()]);
        drop(reader);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn reader_from(conn: Connection) -> Reader {
        Reader::new(conn)
    }
}
