//! The database seam — a `std`-only replacement for `rusqlite`.
//!
//! **This module is a departure, not a recovery.** The original crate talks to
//! SQLite through `rusqlite`; there is no SQLite and no `rusqlite` in this
//! workspace, and no network to fetch one. So this module implements the
//! *recovered relational schema* as typed `Vec`s of the recovered row types and
//! offers the operations the crate performs, with the original SQL text kept
//! verbatim as the doc-comment of each operation (and in [`crate::schema`]).
//!
//! What is deliberately **not** here:
//!
//! * no SQL parser and no statement engine — statements are dispatched by
//!   identity against the recovered literals ([`Connection::execute_batch`]),
//!   and every query is a typed operation over the row vectors, exactly as the
//!   crate's SQL says it should behave;
//! * no SQLite file format. [`Connection::commit`] writes a small documented
//!   line format instead (see [`Connection::save_to`]); a real `hydra.db` file
//!   cannot be read or written by this build.
//!
//! Everything that *is* here was read out of the binary: the tables, the columns,
//! the indexes, the ordering, the predicates, and the uniqueness rules.

use std::cell::{Ref, RefCell};
use std::path::{Path, PathBuf};

use crate::row::{Pulse, Reward, Row, Snapshot, Source, SnapshotKind, StoreVisit};
use crate::{Error, Result};

/// A row with its `INTEGER PRIMARY KEY AUTOINCREMENT` id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored<T> {
    /// The `id` column.
    pub id: i64,
    /// The row itself.
    pub row: T,
}

impl<T> Stored<T> {
    /// Wrap a row.
    pub fn new(id: i64, row: T) -> Stored<T> {
        Stored { id, row }
    }
}

/// One `hunts` row — only the four columns `Reader::hunt_overlapping` reads.
///
/// The original table is created by [`crate::schema::BASE_SCHEMA`] and written
/// by `hydra_combat_log`; the ledger only queries it, through
/// `SELECT h.id FROM hunts h …`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hunt {
    /// `instance`.
    pub instance: String,
    /// `character`.
    pub character: String,
    /// `started_at`.
    pub started_at: i64,
    /// `ended_at` (nullable).
    pub ended_at: Option<i64>,
}

/// One `attacks` row — only the two columns `Reader::hunt_overlapping` needs
/// for its `COALESCE(h.ended_at, (SELECT MAX(occurred_at) FROM attacks WHERE
/// hunt_id = h.id), h.started_at)` fallback.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attack {
    /// `hunt_id`.
    pub hunt_id: i64,
    /// `occurred_at`.
    pub occurred_at: i64,
}

/// One `imports` row (10 columns, in `INSERT` order):
/// `instance, character, stamp, path, size, imported_at, first_at, last_at,
/// hunts, pulses`.
///
/// Written by [`crate::Ledger::note_import`], read by
/// [`crate::reader::Reader::imported`] and by the unique-index rule
/// `imports_session(instance, character, stamp)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Import {
    /// `instance`.
    pub instance: String,
    /// `character`.
    pub character: String,
    /// `stamp` — the import's identity within a character.
    pub stamp: String,
    /// `path` — where the log came from.
    pub path: String,
    /// `size` — its length in bytes.
    pub size: u64,
    /// `imported_at` — when the import ran (unix seconds).
    pub imported_at: i64,
    /// `first_at` — the earliest pulse it carried (nullable).
    pub first_at: Option<i64>,
    /// `last_at` — the latest pulse it carried (nullable).
    pub last_at: Option<i64>,
    /// `hunts` — how many hunts it carried.
    pub hunts: u64,
    /// `pulses` — how many pulses it carried.
    pub pulses: u64,
}

/// One `names` row: `id, kind, name, number, mnemonic`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Name {
    /// `kind` — what the name names (`gear`, `property`, …).
    pub kind: String,
    /// `name`.
    pub name: String,
    /// `number` — the game's own number for it, when it has one.
    pub number: Option<i64>,
    /// `mnemonic` — the short form.
    pub mnemonic: Option<String>,
}

/// The recovered relational store.
///
/// Tables are named after the SQL and hold rows in insertion order, which is
/// what `ORDER BY … id` and `MAX(id)` rely on.
#[derive(Debug, Default)]
pub struct Db {
    next_id: i64,
    /// `pulses`
    pub pulses: Vec<Stored<Pulse>>,
    /// `snapshots`
    pub snapshots: Vec<Stored<Snapshot>>,
    /// `login_rewards`
    pub rewards: Vec<Stored<Reward>>,
    /// `store_visits`
    pub store_visits: Vec<Stored<StoreVisit>>,
    /// `imports`
    pub imports: Vec<Stored<Import>>,
    /// `names`
    pub names: Vec<Stored<Name>>,
    /// `hunts`
    pub hunts: Vec<Stored<Hunt>>,
    /// `attacks`
    pub attacks: Vec<Stored<Attack>>,
}

impl Db {
    fn id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }
}

/// A `rusqlite::Connection` stand-in.
///
/// The original's `Connection` has interior mutability (`RefCell<InnerConnection>`)
/// and every write in this crate goes through `&self`, so this one keeps its
/// [`Db`] behind a `RefCell` too — the same (`&self`) call shapes, minus SQLite.
#[derive(Debug)]
pub struct Connection {
    path: Option<PathBuf>,
    read_only: bool,
    user_version: RefCell<u32>,
    db: RefCell<Db>,
}

impl Connection {
    /// An in-memory connection. `read_only` mirrors the flags
    /// [`crate::Ledger::connect`] passes to `InnerConnection::open_with_flags`.
    pub fn in_memory(read_only: bool) -> Connection {
        Connection {
            path: None,
            read_only,
            user_version: RefCell::new(0),
            db: RefCell::new(Db::default()),
        }
    }

    /// Open (and, when not read-only, create) the store at `path`.
    ///
    /// The original is `InnerConnection::open_with_flags(path, flags)` with
    /// `0x8001` (read-only) or `0x8006` (read-write + create), then
    /// `busy_timeout(5s)` and — for a writable connection — `PRAGMA
    /// journal_mode = WAL` and `PRAGMA synchronous = NORMAL`. Here a writable
    /// connection also makes sure the parent directory exists, and reads back
    /// whatever a previous [`Connection::commit`] wrote.
    pub fn open(path: impl Into<PathBuf>, read_only: bool) -> Result<Connection> {
        let path = path.into();
        if !read_only {
            if let Some(dir) = path.parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir).map_err(|e| Error::Io(e.to_string()))?;
                }
            }
        }
        let (user_version, db) = if path.exists() {
            decode(&std::fs::read_to_string(&path).map_err(|e| Error::Io(e.to_string()))?)?
        } else if read_only {
            // SQLITE_OPEN_READ_ONLY against a missing file is `SQLITE_CANTOPEN`.
            return Err(Error::Sqlite(format!(
                "unable to open database file: {}",
                path.display()
            )));
        } else {
            (0, Db::default())
        };
        Ok(Connection {
            path: Some(path),
            read_only,
            user_version: RefCell::new(user_version),
            db: RefCell::new(db),
        })
    }

    /// The database file, if this connection has one.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether this connection is read-only (`SQLITE_OPEN_READ_ONLY`).
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The tables.
    pub fn db(&self) -> Ref<'_, Db> {
        self.db.borrow()
    }

    /// `PRAGMA user_version`.
    pub fn user_version(&self) -> u32 {
        *self.user_version.borrow()
    }

    /// `PRAGMA user_version = n` — `Connection::pragma_update(None,
    /// "user_version", n)` in [`crate::migrate`].
    pub fn set_user_version(&self, version: u32) -> Result<()> {
        self.writable()?;
        *self.user_version.borrow_mut() = version;
        self.autocommit()
    }

    /// Dispatch one of the recovered statement batches.
    ///
    /// **Departure:** the original hands the text to SQLite. This build only
    /// recognises the statements the ledger itself issues and refuses anything
    /// else, so a typo can never be silently "executed":
    ///
    /// * the three `BEGIN …` texts (recovered at `0x103985b5f`/`…b6d`/`…b7c`)
    ///   and `COMMIT` — transaction bookkeeping;
    /// * the three schema batches ([`crate::schema::MIGRATIONS`]) — the tables
    ///   are statically typed here, so applying one only records the version;
    /// * the two `DELETE FROM pulses` texts — see [`Connection::delete_pulses`].
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        let sql = sql.trim();
        match sql {
            crate::schema::BEGIN_DEFERRED
            | crate::schema::BEGIN_IMMEDIATE
            | crate::schema::BEGIN_EXCLUSIVE => self.writable(),
            "COMMIT" => {
                self.writable()?;
                self.commit()
            }
            other if other.starts_with("DELETE FROM pulses") => Err(Error::Unsupported(
                "run Ledger::prune_pulses instead of the DELETE text".to_string(),
            )),
            other if crate::schema::MIGRATIONS.contains(&other) => {
                // The schema is the type system here: nothing to create.
                self.writable()
            }
            other => Err(Error::Unsupported(other.to_string())),
        }
    }

    /// Fail with `Error::ReadOnly` when this connection cannot write
    /// (`SQLITE_READONLY`).
    pub fn writable(&self) -> Result<()> {
        if self.read_only {
            return Err(Error::ReadOnly);
        }
        Ok(())
    }

    /// `INSERT INTO pulses (…) VALUES (?1 … ?18)` — the statement at
    /// `0x103985cd9` (588 bytes), used by `writer::run`.
    pub fn insert_pulse(&self, mut row: Pulse) -> Result<i64> {
        self.writable()?;
        let (exp, axp) = Pulse::absorbed(row.exp, row.axp);
        row.exp = exp;
        row.axp = axp;
        let mut db = self.db.borrow_mut();
        let id = db.id();
        db.pulses.push(Stored::new(id, row));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// `INSERT INTO snapshots (…) VALUES (?1 … ?9)` — `0x103985f71`…, used by
    /// `writer::run`.
    pub fn insert_snapshot(&self, row: Snapshot) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        let id = db.id();
        db.snapshots.push(Stored::new(id, row));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// `INSERT INTO store_visits (…) VALUES (?1 … ?7)` — `0x103985f02`…, used
    /// by `writer::run`.
    pub fn insert_store_visit(&self, row: StoreVisit) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        let id = db.id();
        db.store_visits.push(Stored::new(id, row));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// The `login_rewards` upsert, in full:
    ///
    /// ```sql
    /// INSERT INTO login_rewards (at, game_time, instance, character, day, streak,
    ///     bridges, reward, thanks, line)
    ///  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
    ///  ON CONFLICT (instance, character, day) DO UPDATE SET
    ///     at = excluded.at, game_time = excluded.game_time, streak = excluded.streak,
    ///     bridges = excluded.bridges, reward = excluded.reward,
    ///     thanks = excluded.thanks, line = excluded.line
    ///  WHERE login_rewards.reward = '' AND excluded.reward <> ''
    /// ```
    ///
    /// The `WHERE` on the `DO UPDATE` means an existing row is only overwritten
    /// when it has no reward text yet and the incoming row does; otherwise the
    /// insert is a no-op. Returns the row id — the existing row's id when the
    /// conflict was hit, the new id otherwise. (`Reader::has_reward_on` is how
    /// the crate asks whether a day already has a row.)
    pub fn insert_reward(&self, row: Reward) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        let existing = db.rewards.iter().position(|stored| {
            stored.row.instance == row.instance
                && stored.row.character == row.character
                && stored.row.day == row.day
        });
        let written = match existing {
            Some(index) => {
                let keeps_reward = db.rewards[index].row.reward.is_empty() && !row.reward.is_empty();
                if keeps_reward {
                    let id = db.rewards[index].id;
                    db.rewards[index].row = row;
                    id
                } else {
                    db.rewards[index].id
                }
            }
            None => {
                let id = db.id();
                db.rewards.push(Stored::new(id, row));
                id
            }
        };
        drop(db);
        self.autocommit()?;
        Ok(written)
    }

    /// `INSERT INTO imports (instance, character, stamp, path, size,
    /// imported_at, first_at, last_at, hunts, pulses) VALUES (?1 … ?10)` —
    /// `0x1039857a5`…, used by [`crate::Ledger::note_import`].
    ///
    /// `imports_session` is `UNIQUE (instance, character, stamp)`, so a second
    /// note for the same import is a constraint violation
    /// ([`Error::Constraint`]) rather than a duplicate row.
    pub fn insert_import(&self, row: Import) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        if db.imports.iter().any(|stored| {
            stored.row.instance == row.instance
                && stored.row.character == row.character
                && stored.row.stamp == row.stamp
        }) {
            return Err(Error::Constraint(
                "UNIQUE constraint failed: imports.instance, imports.character, imports.stamp"
                    .to_string(),
            ));
        }
        let id = db.id();
        db.imports.push(Stored::new(id, row));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// A `hunts` row. Written by `hydra_combat_log` in the original; the ledger
    /// only reads it, so this exists for tests and for the reader's predicate.
    pub fn insert_hunt(&self, row: Hunt) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        let id = db.id();
        db.hunts.push(Stored::new(id, row));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// An `attacks` row (again `hydra_combat_log`'s table; kept for
    /// [`crate::reader::Reader::hunt_overlapping`]'s `COALESCE`).
    pub fn insert_attack(&self, row: Attack) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        let id = db.id();
        db.attacks.push(Stored::new(id, row));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// `DELETE FROM pulses WHERE instance = ?1 AND character = ?2 [AND
    /// exp_gained = 0 AND axp_gained = 0]` — the two texts behind
    /// [`crate::Ledger::prune_pulses`] (`0x103983763`, 108 bytes, and
    /// `0x1039837cf`, 57 bytes). Returns the number of rows deleted
    /// (`Connection::execute`'s count).
    pub fn delete_pulses(&self, instance: &str, character: &str, drop_all: bool) -> Result<usize> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        let before = db.pulses.len();
        db.pulses.retain(|stored| {
            let who = stored.row.instance == instance && stored.row.character == character;
            let empty = stored.row.exp_gained == 0 && stored.row.axp_gained == 0;
            !(who && (drop_all || empty))
        });
        let dropped = before - db.pulses.len();
        drop(db);
        self.autocommit()?;
        Ok(dropped)
    }

    /// The interning behind [`crate::name_id`]: `INSERT OR IGNORE INTO names
    /// (kind, name, number, mnemonic) VALUES (?1, ?2, ?3, ?4)` (82 bytes,
    /// `0x103985b97`) followed by `SELECT id FROM names WHERE kind = ?1 AND
    /// name = ?2` (50 bytes, `0x103985be9`).
    ///
    /// `names_kind_name` is `UNIQUE (kind, name)`, so the insert is a no-op for
    /// a name already interned and the select finds that row's id. The number
    /// and mnemonic of the *first* insert stick.
    pub fn intern_name(
        &self,
        kind: &str,
        name: &str,
        number: Option<i64>,
        mnemonic: Option<&str>,
    ) -> Result<i64> {
        self.writable()?;
        let mut db = self.db.borrow_mut();
        if let Some(stored) = db
            .names
            .iter()
            .find(|stored| stored.row.kind == kind && stored.row.name == name)
        {
            return Ok(stored.id);
        }
        let id = db.id();
        db.names.push(Stored::new(
            id,
            Name {
                kind: kind.to_string(),
                name: name.to_string(),
                number,
                mnemonic: mnemonic.map(|text| text.to_string()),
            },
        ));
        drop(db);
        self.autocommit()?;
        Ok(id)
    }

    /// The id [`Connection::intern_name`] would return, without inserting.
    pub fn name_id_of(&self, kind: &str, name: &str) -> Option<i64> {
        self.db
            .borrow()
            .names
            .iter()
            .find(|stored| stored.row.kind == kind && stored.row.name == name)
            .map(|stored| stored.id)
    }

    /// Write the store now if this connection has a file.
    ///
    /// `rusqlite` runs every statement in SQLite's *auto-commit* mode, so a row
    /// written by one connection is visible to the next one straight away. This
    /// build copies that: every mutating operation persists when the connection
    /// is file-backed, and [`Connection::commit`] is only the explicit end of a
    /// transaction (which `migrate` and the writer's batch use).
    fn autocommit(&self) -> Result<()> {
        match (&self.path, self.read_only) {
            (Some(path), false) => self.save_to(path),
            _ => Ok(()),
        }
    }

    /// Persist the store (the original's `COMMIT`).
    ///
    /// With no file this is a no-op; with a file it rewrites it in the line
    /// format described on [`Connection::save_to`]. A read-only connection
    /// never writes.
    pub fn commit(&self) -> Result<()> {
        if self.read_only {
            return Ok(());
        }
        match self.path.clone() {
            Some(path) => self.save_to(&path),
            None => Ok(()),
        }
    }

    /// Write the store to `path`.
    ///
    /// **Departure:** not SQLite's file format. One record per line, `\t`
    /// separated, with `\\`, `\t`, `\n` and `\r` escaped inside strings and an
    /// empty field for `NULL`. The first line is `hydra-ledger <version>`.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let text = encode(self.user_version(), &self.db.borrow());
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir).map_err(|e| Error::Io(e.to_string()))?;
            }
        }
        std::fs::write(path, text).map_err(|e| Error::Io(e.to_string()))
    }

    /// Write a [`Row`] the way `writer::run` does.
    pub fn insert_row(&self, row: Row) -> Result<i64> {
        match row {
            Row::Pulse(pulse) => self.insert_pulse(pulse),
            Row::Snapshot(snapshot) => self.insert_snapshot(snapshot),
            Row::Reward(reward) => self.insert_reward(reward),
            Row::StoreVisit(visit) => self.insert_store_visit(visit),
        }
    }
}

// ---------------------------------------------------------------- line codec

const HEADER: &str = "hydra-ledger";

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

fn unesc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn num(value: i64) -> String {
    value.to_string()
}

fn num_in(field: &str) -> Option<i64> {
    field.parse().ok()
}

fn opt_num(value: Option<i64>) -> String {
    value.map(num).unwrap_or_default()
}

fn opt_u32(value: Option<u32>) -> String {
    value.map(|n| n.to_string()).unwrap_or_default()
}

fn opt_u32_in(field: &str) -> Option<u32> {
    field.parse().ok()
}

fn opt_text(value: &Option<String>) -> String {
    value.as_deref().map(esc).unwrap_or_default()
}

fn opt_text_in(field: &str) -> Option<String> {
    if field.is_empty() {
        None
    } else {
        Some(unesc(field))
    }
}

fn encode(version: u32, db: &Db) -> String {
    let mut out = format!("{HEADER}\t{version}\n");
    for stored in &db.pulses {
        let p = &stored.row;
        out.push_str(&format!(
            "pulse\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            num(p.at),
            opt_u32(p.game_time),
            esc(&p.instance),
            esc(&p.character),
            num(p.exp),
            num(p.axp),
            num(p.exp_gained),
            num(p.axp_gained),
            opt_u32(p.fxp),
            opt_u32(p.fxp_max),
            opt_u32(p.until_next),
            opt_u32(p.lumnis),
            opt_text(&p.rpa),
            opt_u32(p.fashlonae),
            opt_u32(p.mind_percent),
            opt_text(&p.mind_text),
            esc(p.source.name()),
            num(p.instant),
        ));
    }
    for stored in &db.snapshots {
        let s = &stored.row;
        out.push_str(&format!(
            "snapshot\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            num(s.at),
            opt_u32(s.game_time),
            esc(&s.instance),
            esc(&s.character),
            esc(s.kind.name()),
            opt_text(&s.name),
            opt_num(s.value),
            opt_num(s.weekly),
            opt_num(s.total),
        ));
    }
    for stored in &db.rewards {
        let r = &stored.row;
        out.push_str(&format!(
            "reward\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            num(r.at),
            opt_u32(r.game_time),
            esc(&r.instance),
            esc(&r.character),
            esc(&r.day),
            opt_num(r.streak),
            opt_num(r.bridges),
            esc(&r.reward),
            esc(&r.thanks),
            esc(&r.line),
        ));
    }
    for stored in &db.store_visits {
        let v = &stored.row;
        out.push_str(&format!(
            "visit\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            num(v.at),
            esc(&v.account),
            esc(&v.game),
            if v.claimed { "1" } else { "0" },
            esc(&v.message),
            esc(&v.balance),
            opt_num(v.next_at),
        ));
    }
    for stored in &db.imports {
        let i = &stored.row;
        out.push_str(&format!(
            "import\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            esc(&i.instance),
            esc(&i.character),
            esc(&i.stamp),
            esc(&i.path),
            num(i.size as i64),
            num(i.imported_at),
            opt_num(i.first_at),
            opt_num(i.last_at),
            num(i.hunts as i64),
            num(i.pulses as i64),
        ));
    }
    for stored in &db.names {
        let n = &stored.row;
        out.push_str(&format!(
            "name\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            esc(&n.kind),
            esc(&n.name),
            opt_num(n.number),
            opt_text(&n.mnemonic),
        ));
    }
    for stored in &db.hunts {
        let h = &stored.row;
        out.push_str(&format!(
            "hunt\t{}\t{}\t{}\t{}\t{}\n",
            stored.id,
            esc(&h.instance),
            esc(&h.character),
            num(h.started_at),
            opt_num(h.ended_at),
        ));
    }
    for stored in &db.attacks {
        let a = &stored.row;
        out.push_str(&format!(
            "attack\t{}\t{}\t{}\n",
            stored.id,
            num(a.hunt_id),
            num(a.occurred_at),
        ));
    }
    out
}

fn decode(text: &str) -> Result<(u32, Db)> {
    let mut db = Db::default();
    let mut version = 0u32;
    let mut first = true;
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if first {
            first = false;
            if fields.first() != Some(&"hydra-ledger") {
                return Err(Error::Unsupported(format!(
                    "not a hydra-ledger store: {line:?}"
                )));
            }
            version = fields.get(1).and_then(|f| f.parse().ok()).unwrap_or(0);
            continue;
        }
        let mut it = fields.iter().copied();
        let Some(tag) = it.next() else { continue };
        let mut next = |what: &str| -> Result<String> {
            it.next()
                .map(|field| field.to_string())
                .ok_or_else(|| Error::Unsupported(format!("{tag}: missing {what}")))
        };
        match tag {
            "pulse" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Pulse {
                    at: next("at")?.parse().unwrap_or(0),
                    game_time: opt_u32_in(&next("game_time")?),
                    instance: unesc(&next("instance")?),
                    character: unesc(&next("character")?),
                    exp: next("exp")?.parse().unwrap_or(0),
                    axp: next("axp")?.parse().unwrap_or(0),
                    exp_gained: next("exp_gained")?.parse().unwrap_or(0),
                    axp_gained: next("axp_gained")?.parse().unwrap_or(0),
                    fxp: opt_u32_in(&next("fxp")?),
                    fxp_max: opt_u32_in(&next("fxp_max")?),
                    until_next: opt_u32_in(&next("until_next")?),
                    lumnis: opt_u32_in(&next("lumnis")?),
                    rpa: opt_text_in(&next("rpa")?),
                    fashlonae: opt_u32_in(&next("fashlonae")?),
                    mind_percent: opt_u32_in(&next("mind_percent")?),
                    mind_text: opt_text_in(&next("mind_text")?),
                    source: Source::parse(&unesc(&next("source")?)),
                    instant: next("instant")?.parse().unwrap_or(0),
                };
                db.next_id = db.next_id.max(id);
                db.pulses.push(Stored::new(id, row));
            }
            "snapshot" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Snapshot {
                    at: next("at")?.parse().unwrap_or(0),
                    game_time: opt_u32_in(&next("game_time")?),
                    instance: unesc(&next("instance")?),
                    character: unesc(&next("character")?),
                    kind: SnapshotKind::parse(&unesc(&next("kind")?)),
                    name: opt_text_in(&next("name")?),
                    value: num_in(&next("value")?),
                    weekly: num_in(&next("weekly")?),
                    total: num_in(&next("total")?),
                };
                db.next_id = db.next_id.max(id);
                db.snapshots.push(Stored::new(id, row));
            }
            "reward" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Reward {
                    at: next("at")?.parse().unwrap_or(0),
                    game_time: opt_u32_in(&next("game_time")?),
                    instance: unesc(&next("instance")?),
                    character: unesc(&next("character")?),
                    day: unesc(&next("day")?),
                    streak: num_in(&next("streak")?),
                    bridges: num_in(&next("bridges")?),
                    reward: unesc(&next("reward")?),
                    thanks: unesc(&next("thanks")?),
                    line: unesc(&next("line")?),
                };
                db.next_id = db.next_id.max(id);
                db.rewards.push(Stored::new(id, row));
            }
            "visit" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = StoreVisit {
                    at: next("at")?.parse().unwrap_or(0),
                    account: unesc(&next("account")?),
                    game: unesc(&next("game")?),
                    claimed: next("claimed")? == "1",
                    message: unesc(&next("message")?),
                    balance: unesc(&next("balance")?),
                    next_at: num_in(&next("next_at")?),
                };
                db.next_id = db.next_id.max(id);
                db.store_visits.push(Stored::new(id, row));
            }
            "import" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Import {
                    instance: unesc(&next("instance")?),
                    character: unesc(&next("character")?),
                    stamp: unesc(&next("stamp")?),
                    path: unesc(&next("path")?),
                    size: next("size")?.parse().unwrap_or(0),
                    imported_at: next("imported_at")?.parse().unwrap_or(0),
                    first_at: num_in(&next("first_at")?),
                    last_at: num_in(&next("last_at")?),
                    hunts: next("hunts")?.parse().unwrap_or(0),
                    pulses: next("pulses")?.parse().unwrap_or(0),
                };
                db.next_id = db.next_id.max(id);
                db.imports.push(Stored::new(id, row));
            }
            "name" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Name {
                    kind: unesc(&next("kind")?),
                    name: unesc(&next("name")?),
                    number: num_in(&next("number")?),
                    mnemonic: opt_text_in(&next("mnemonic")?),
                };
                db.next_id = db.next_id.max(id);
                db.names.push(Stored::new(id, row));
            }
            "hunt" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Hunt {
                    instance: unesc(&next("instance")?),
                    character: unesc(&next("character")?),
                    started_at: next("started_at")?.parse().unwrap_or(0),
                    ended_at: num_in(&next("ended_at")?),
                };
                db.next_id = db.next_id.max(id);
                db.hunts.push(Stored::new(id, row));
            }
            "attack" => {
                let id: i64 = next("id")?.parse().unwrap_or(0);
                let row = Attack {
                    hunt_id: next("hunt_id")?.parse().unwrap_or(0),
                    occurred_at: next("occurred_at")?.parse().unwrap_or(0),
                };
                db.next_id = db.next_id.max(id);
                db.attacks.push(Stored::new(id, row));
            }
            other => {
                return Err(Error::Unsupported(format!("unknown record {other:?}")));
            }
        }
    }
    Ok((version, db))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pulse(at: i64, exp_gained: i64) -> Pulse {
        Pulse {
            instance: "prime".to_string(),
            character: "Sindar".to_string(),
            at,
            exp_gained,
            rpa: Some("1.25x".to_string()),
            mind_text: Some("clear".to_string()),
            ..Pulse::default()
        }
    }

    #[test]
    fn escaping_round_trips_every_awkward_byte() {
        let text = "a\tb\nc\\d\re";
        assert_eq!(unesc(&esc(text)), text);
        assert_eq!(esc("plain"), "plain");
    }

    #[test]
    fn a_store_round_trips_through_the_file_format() {
        let dir = std::env::temp_dir().join(format!("hydra-ledger-store-{}", std::process::id()));
        let path = dir.join("hydra.db");
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path, false).unwrap();
            conn.set_user_version(3).unwrap();
            conn.insert_pulse(pulse(10, 100)).unwrap();
            conn.insert_pulse(pulse(20, -5)).unwrap();
            conn.insert_snapshot(Snapshot {
                instance: "prime".to_string(),
                character: "Sindar".to_string(),
                kind: SnapshotKind::Suffused,
                name: Some("Suffused Essence".to_string()),
                value: Some(4_000),
                at: 30,
                ..Snapshot::default()
            })
            .unwrap();
            conn.insert_reward(Reward {
                instance: "prime".to_string(),
                character: "Sindar".to_string(),
                day: "2026-07-20".to_string(),
                reward: "a trinket".to_string(),
                at: 40,
                ..Reward::default()
            })
            .unwrap();
            conn.insert_store_visit(StoreVisit {
                account: "acct".to_string(),
                game: "prime".to_string(),
                claimed: true,
                message: "claimed".to_string(),
                at: 50,
                next_at: Some(86_400),
                ..StoreVisit::default()
            })
            .unwrap();
            conn.intern_name("gear", "a sword", Some(7), Some("sword"))
                .unwrap();
            conn.commit().unwrap();
        }
        let conn = Connection::open(&path, true).unwrap();
        assert_eq!(conn.user_version(), 3);
        let db = conn.db();
        assert_eq!(db.pulses.len(), 2);
        assert_eq!(db.pulses[0].row.at, 10);
        assert_eq!(db.pulses[0].row.rpa.as_deref(), Some("1.25x"));
        assert_eq!(db.pulses[1].row.exp_gained, -5);
        assert_eq!(db.pulses[1].id, 2);
        assert_eq!(db.snapshots[0].row.kind, SnapshotKind::Suffused);
        assert_eq!(
            db.snapshots[0].row.name.as_deref(),
            Some("Suffused Essence")
        );
        assert_eq!(db.rewards[0].row.reward, "a trinket");
        assert_eq!(db.store_visits[0].row.next_at, Some(86_400));
        assert_eq!(db.names[0].row.mnemonic.as_deref(), Some("sword"));
        // A read-only connection refuses to write (`SQLITE_READONLY`).
        assert_eq!(conn.insert_pulse(pulse(99, 1)), Err(Error::ReadOnly));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn a_missing_database_is_an_error_for_a_read_only_connection() {
        let path = std::env::temp_dir().join("hydra-ledger-does-not-exist.db");
        let _ = std::fs::remove_file(&path);
        assert!(Connection::open(&path, true).is_err());
    }

    #[test]
    fn execute_batch_only_knows_the_recovered_statements() {
        let conn = Connection::in_memory(false);
        assert_eq!(conn.execute_batch(crate::schema::BEGIN_DEFERRED), Ok(()));
        assert_eq!(conn.execute_batch("COMMIT"), Ok(()));
        assert_eq!(conn.execute_batch(crate::schema::MIGRATIONS[0]), Ok(()));
        assert!(matches!(
            conn.execute_batch("SELECT 1"),
            Err(Error::Unsupported(_))
        ));
    }
}
