//! The database seam: a real [`rusqlite`] connection to `hydra.db`.
//!
//! **Reconnected in Phase 1.** The original crate talks to SQLite through
//! `rusqlite`, and so does this one: [`Connection`] wraps a
//! `rusqlite::Connection` opened with the recovered flags, migrates the
//! recovered schema ([`crate::schema::MIGRATIONS`]) inside one transaction per
//! step, and runs the recovered statements with `execute`, `execute_batch`,
//! `query_row`, `query_map` and `prepare_cached` (rusqlite's `StatementCache`).
//! The pre-Phase-1 `std` stand-in — typed row vectors plus a `hydra-ledger` line
//! file, with statements dispatched by identity — is gone; `NOTES.md` §6 records
//! what it was.
//!
//! Two things are *not* recovered and are documented as such:
//!
//! * [`Db`] and [`Connection::db`] — a read-only **view** of the tables as row
//!   vectors. The binary has no such type; it exists because the crate's own
//!   tests express their expectations as `conn.db().pulses.len()`, and the
//!   parent's rule is to keep the call shapes. It is loaded from SQLite on
//!   every call (`SELECT … ORDER BY id`), so it can never be stale.
//! * `Connection::open` creates the database's directory. SQLite itself does
//!   not (`SQLITE_CANTOPEN`); `Ledger::open` did it for the original and doing
//!   it here too keeps `Ledger::writing()` usable on a fresh directory, which is
//!   what the writer tests rely on.
//!
//! Everything else — the tables, the columns, the indexes, the ordering, the
//! predicates, the uniqueness rules, the pragmas and the busy timeout — was
//! read out of the binary; `NOTES.md` has the addresses.

use std::cell::{Ref, RefCell};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::OpenFlags;

use crate::row::{Pulse, Reward, Row, Snapshot, SnapshotKind, Source, StoreVisit};
use crate::schema;
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

/// One `hunts` row — the four columns `Reader::hunt_overlapping` reads.
///
/// The original table is created by [`crate::schema::MIGRATION_1`] and written
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

/// One `attacks` row — the two columns `Reader::hunt_overlapping` needs for its
/// `COALESCE(h.ended_at, (SELECT MAX(occurred_at) FROM attacks WHERE
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
/// [`crate::reader::Reader::imported`] and constrained by the unique index
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

/// The tables of a ledger, as row vectors — **a view, not the store**.
///
/// The binary has no such type: this is the shape the crate's tests were
/// written against before the Phase-1 reconnection, kept so they (and any
/// sibling that reads `conn.db()`) still compile. [`Connection::db`] fills it
/// with `SELECT … ORDER BY id` on every call, so it always mirrors SQLite.
#[derive(Debug, Default)]
pub struct Db {
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

/// A `rusqlite::Connection` with the ledger's recovered opening sequence.
///
/// The original's `Connection` is `rusqlite`'s; the read-only flag and the
/// database path are kept alongside it because [`crate::migrate`] prints the
/// path in its schema-version refusal and because the crate's API asks
/// [`Connection::read_only`].
#[derive(Debug)]
pub struct Connection {
    conn: rusqlite::Connection,
    path: Option<PathBuf>,
    read_only: bool,
    view: RefCell<Db>,
}

impl Connection {
    /// An in-memory connection with the ledger's schema applied.
    ///
    /// `read_only` gates this crate's own mutators (`insert_*`, `delete_pulses`,
    /// `set_user_version`, `intern_name`), exactly as before the reconnection;
    /// SQLite cannot open an in-memory database read-only, so the flag is
    /// enforced here rather than by `SQLITE_OPEN_READ_ONLY`.
    pub fn in_memory(read_only: bool) -> Connection {
        let conn = rusqlite::Connection::open_in_memory()
            .expect("could not open an in-memory SQLite connection");
        let mut connection = Connection {
            conn,
            path: None,
            read_only: false,
            view: RefCell::new(Db::default()),
        };
        crate::migrate(&mut connection).expect("could not apply the ledger schema in memory");
        connection.read_only = read_only;
        connection
    }

    /// Open (and, when not read-only, create) the ledger at `path`.
    ///
    /// The recovered body of `Ledger::connect` (`0x10168e614`), in order:
    ///
    /// 1. `InnerConnection::open_with_flags(path, flags)` with
    ///    `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` (`0x8001`) for a
    ///    read-only connection and `SQLITE_OPEN_READ_WRITE |
    ///    SQLITE_OPEN_CREATE | SQLITE_OPEN_NO_MUTEX` (`0x8006`) otherwise;
    /// 2. `busy_timeout(Duration::of_secs(5))` (`mov w1, #0x5; mov w2, #0x0`);
    /// 3. only for a writable connection, `pragma_update(None, "journal_mode",
    ///    "WAL")` and `pragma_update(None, "synchronous", "NORMAL")`.
    pub fn open(path: impl Into<PathBuf>, read_only: bool) -> Result<Connection> {
        let path = path.into();
        if !read_only {
            // Not SQLite's behaviour: see the module docs.
            if let Some(dir) = path.parent() {
                if !dir.as_os_str().is_empty() {
                    std::fs::create_dir_all(dir).map_err(|e| Error::Io(e.to_string()))?;
                }
            }
        }
        let flags = if read_only {
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
        };
        let conn = rusqlite::Connection::open_with_flags(&path, flags)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        if !read_only {
            conn.pragma_update(None, "journal_mode", "WAL")?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
        }
        Ok(Connection {
            conn,
            path: Some(path),
            read_only,
            view: RefCell::new(Db::default()),
        })
    }

    /// The `rusqlite::Connection` underneath — the handle the readers prepare
    /// their statements on.
    pub fn raw(&self) -> &rusqlite::Connection {
        &self.conn
    }

    /// The database file, if this connection has one.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Whether this connection was opened read-only (`SQLITE_OPEN_READ_ONLY`).
    pub fn read_only(&self) -> bool {
        self.read_only
    }

    /// The tables, as a fresh view loaded from SQLite.
    pub fn db(&self) -> Ref<'_, Db> {
        let view = self.load_view().unwrap_or_default();
        *self.view.borrow_mut() = view;
        self.view.borrow()
    }

    /// `PRAGMA user_version` — 0 when the pragma cannot be read (a file that is
    /// not a database, for instance). [`Connection::try_user_version`] is the
    /// fallible form, which [`crate::migrate`] uses.
    pub fn user_version(&self) -> u32 {
        self.try_user_version().unwrap_or(0)
    }

    /// `PRAGMA user_version` — `Connection::pragma_query_value` in
    /// [`crate::migrate`].
    pub fn try_user_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .pragma_query_value(None, schema::USER_VERSION_NAME, |row| row.get(0))?)
    }

    /// `PRAGMA user_version = n` — `Connection::pragma_update(None,
    /// "user_version", n)` in [`crate::migrate`].
    pub fn set_user_version(&self, version: u32) -> Result<()> {
        self.writable()?;
        self.conn
            .pragma_update(None, schema::USER_VERSION_NAME, version)?;
        Ok(())
    }

    /// `Connection::execute_batch` — the original hands SQL text straight to
    /// SQLite, and so does this: the three recovered `BEGIN …` texts, the
    /// migrations, `COMMIT`, or any other statement a caller wants to run.
    ///
    /// Before the reconnection this method dispatched by identity and answered
    /// `Error::Unsupported` for anything it did not recognise; that seam is
    /// gone (there is a SQL engine now). A write on a read-only file
    /// connection is refused by SQLite itself (`SQLITE_READONLY` →
    /// [`Error::ReadOnly`]); for an in-memory connection —
    /// [`Connection::in_memory`] — the read-only flag is this crate's, and the
    /// typed mutators are what enforce it.
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        self.conn.execute_batch(sql)?;
        Ok(())
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
    /// `0x103985cd9` (303 bytes), used by `writer::run`. Returns the new row's
    /// `id` (`Connection::last_insert_rowid`).
    ///
    /// The `exp`/`axp` clamp (`max(v, 0)`, `bic x10, x23, x23, asr #63`) is
    /// applied here, as the recovered row mapper applies it.
    pub fn insert_pulse(&self, row: Pulse) -> Result<i64> {
        self.writable()?;
        insert_pulse_on(&self.conn, &row)
    }

    /// `INSERT INTO snapshots (…) VALUES (?1 … ?9)` — `0x103986090` (157 bytes).
    pub fn insert_snapshot(&self, row: Snapshot) -> Result<i64> {
        self.writable()?;
        insert_snapshot_on(&self.conn, &row)
    }

    /// `INSERT INTO store_visits (…) VALUES (?1 … ?7)` — `0x103985e08`
    /// (125 bytes).
    pub fn insert_store_visit(&self, row: StoreVisit) -> Result<i64> {
        self.writable()?;
        insert_store_visit_on(&self.conn, &row)
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
    /// upsert changes nothing. Returns the row's id — the existing row's id when
    /// the conflict was hit (the upsert does not move it, so the id is read back
    /// rather than taken from `last_insert_rowid`).
    pub fn insert_reward(&self, row: Reward) -> Result<i64> {
        self.writable()?;
        insert_reward_on(&self.conn, &row)
    }

    /// `INSERT INTO imports (…) VALUES (?1 … ?10)` — `0x1039857a5` (185 bytes),
    /// used by [`crate::Ledger::note_import`].
    ///
    /// `imports_session` is `UNIQUE (instance, character, stamp)`, so a second
    /// note for the same import is a constraint violation
    /// ([`Error::Constraint`]) rather than a duplicate row.
    pub fn insert_import(&self, row: Import) -> Result<i64> {
        self.writable()?;
        insert_import_on(&self.conn, &row)
    }

    /// A `hunts` row. Written by `hydra_combat_log` in the original; the ledger
    /// only reads it, so this exists for tests and for the reader's predicate.
    pub fn insert_hunt(&self, row: Hunt) -> Result<i64> {
        self.writable()?;
        self.conn.execute(
            "INSERT INTO hunts (instance, character, started_at, ended_at) \
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![row.instance, row.character, row.started_at, row.ended_at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// An `attacks` row (again `hydra_combat_log`'s table; kept for
    /// [`crate::reader::Reader::hunt_overlapping`]'s `COALESCE`).
    pub fn insert_attack(&self, row: Attack) -> Result<i64> {
        self.writable()?;
        // `attacks` has many NOT NULL columns; these are the ones the battle
        // log's own writer fills with defaults.
        self.conn.execute(
            "INSERT INTO attacks (hunt_id, seq, occurred_at, name, target_kind) \
             VALUES (?1, 0, ?2, '', 'creature')",
            rusqlite::params![row.hunt_id, row.occurred_at],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// `DELETE FROM pulses WHERE instance = ?1 AND character = ?2 [AND
    /// exp_gained = 0 AND axp_gained = 0]` — the two texts behind
    /// [`crate::Ledger::prune_pulses`] (`0x103983763`, 108 bytes, and
    /// `0x1039837cf`, 57 bytes). Returns the number of rows deleted
    /// (`Connection::execute`'s count).
    pub fn delete_pulses(&self, instance: &str, character: &str, drop_all: bool) -> Result<usize> {
        self.writable()?;
        let sql = if drop_all {
            schema::DELETE_ALL_PULSES
        } else {
            schema::DELETE_EMPTY_PULSES
        };
        Ok(self.conn.execute(sql, rusqlite::params![instance, character])?)
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
        self.conn.execute(
            schema::INSERT_NAME,
            rusqlite::params![kind, name, number, mnemonic],
        )?;
        Ok(self.conn.query_row(
            schema::SELECT_NAME_ID,
            rusqlite::params![kind, name],
            |row| row.get(0),
        )?)
    }

    /// The id [`Connection::intern_name`] would return, without inserting.
    pub fn name_id_of(&self, kind: &str, name: &str) -> Option<i64> {
        self.conn
            .query_row(schema::SELECT_NAME_ID, rusqlite::params![kind, name], |row| {
                row.get(0)
            })
            .ok()
    }

    /// The original's explicit `COMMIT`.
    ///
    /// Every statement runs in SQLite's auto-commit mode, so there is nothing
    /// left to do: `migrate` and the writer each end their work with a real
    /// `Transaction::commit`, and this stays for the call sites that used it
    /// (`Ledger::{open, prune_pulses, note_import}`).
    pub fn commit(&self) -> Result<()> {
        Ok(())
    }

    /// Write a [`Row`] the way `writer::run` does, inside `conn` (a connection
    /// or an open transaction — hence the parameter).
    pub(crate) fn insert_row_on(&self, conn: &rusqlite::Connection, row: Row) -> Result<i64> {
        match row {
            Row::Pulse(pulse) => insert_pulse_on(conn, &pulse),
            Row::Snapshot(snapshot) => insert_snapshot_on(conn, &snapshot),
            Row::Reward(reward) => insert_reward_on(conn, &reward),
            Row::StoreVisit(visit) => insert_store_visit_on(conn, &visit),
        }
    }

    /// Write a [`Row`] the way `writer::run` does.
    pub fn insert_row(&self, row: Row) -> Result<i64> {
        self.writable()?;
        self.insert_row_on(&self.conn, row)
    }

    /// Load every table into a [`Db`] view.
    fn load_view(&self) -> Result<Db> {
        Ok(Db {
            pulses: load_pulses(&self.conn)?,
            snapshots: load_snapshots(&self.conn)?,
            rewards: load_rewards(&self.conn)?,
            store_visits: load_store_visits(&self.conn)?,
            imports: load_imports(&self.conn)?,
            names: load_names(&self.conn)?,
            hunts: load_hunts(&self.conn)?,
            attacks: load_attacks(&self.conn)?,
        })
    }
}

// ------------------------------------------------------------------ inserts
//
// Free functions so the writer can run them inside one `Transaction`: a
// `rusqlite::Transaction` derefs to `Connection`, so `&*tx` is accepted here.

fn insert_pulse_on(conn: &rusqlite::Connection, row: &Pulse) -> Result<i64> {
    let (exp, axp) = Pulse::absorbed(row.exp, row.axp);
    conn.execute(
        schema::INSERT_PULSE,
        rusqlite::params![
            row.at,
            row.game_time,
            row.instance,
            row.character,
            exp,
            axp,
            row.exp_gained,
            row.axp_gained,
            row.fxp,
            row.fxp_max,
            row.until_next,
            row.lumnis,
            row.rpa,
            row.fashlonae,
            row.mind_percent,
            row.mind_text,
            row.source.name(),
            row.instant,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_snapshot_on(conn: &rusqlite::Connection, row: &Snapshot) -> Result<i64> {
    conn.execute(
        schema::INSERT_SNAPSHOT,
        rusqlite::params![
            row.at,
            row.game_time,
            row.instance,
            row.character,
            row.kind.name(),
            row.name,
            row.value,
            row.weekly,
            row.total,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_store_visit_on(conn: &rusqlite::Connection, row: &StoreVisit) -> Result<i64> {
    conn.execute(
        schema::INSERT_STORE_VISIT,
        rusqlite::params![
            row.at,
            row.account,
            row.game,
            row.claimed as i64,
            row.message,
            row.balance,
            row.next_at,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_reward_on(conn: &rusqlite::Connection, row: &Reward) -> Result<i64> {
    conn.execute(
        schema::INSERT_REWARD,
        rusqlite::params![
            row.at,
            row.game_time,
            row.instance,
            row.character,
            row.day,
            row.streak,
            row.bridges,
            row.reward,
            row.thanks,
            row.line,
        ],
    )?;
    // The upsert may have kept the existing row, in which case
    // `last_insert_rowid` still points at whatever was inserted last.
    Ok(conn.query_row(
        "SELECT id FROM login_rewards WHERE instance = ?1 AND character = ?2 AND day = ?3",
        rusqlite::params![row.instance, row.character, row.day],
        |row| row.get(0),
    )?)
}

fn insert_import_on(conn: &rusqlite::Connection, row: &Import) -> Result<i64> {
    // The recovered `Ledger::note_import` clamps the size with
    // `cmp x22, #0x7fff…; csel`, i.e. at `i64::MAX`.
    let size = row.size.min(i64::MAX as u64) as i64;
    let hunts = row.hunts.min(i64::MAX as u64) as i64;
    let pulses = row.pulses.min(i64::MAX as u64) as i64;
    conn.execute(
        schema::INSERT_IMPORT,
        rusqlite::params![
            row.instance,
            row.character,
            row.stamp,
            row.path,
            size,
            row.imported_at,
            row.first_at,
            row.last_at,
            hunts,
            pulses,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

// -------------------------------------------------------------------- reads

/// The `pulses` columns of a `SELECT`, in the recovered order.
pub(crate) fn pulse_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Pulse> {
    let (exp, axp) = Pulse::absorbed(row.get(4)?, row.get(5)?);
    Ok(Pulse {
        at: row.get(0)?,
        game_time: row.get(1)?,
        instance: row.get(2)?,
        character: row.get(3)?,
        exp,
        axp,
        exp_gained: row.get(6)?,
        axp_gained: row.get(7)?,
        fxp: row.get(8)?,
        fxp_max: row.get(9)?,
        until_next: row.get(10)?,
        lumnis: row.get(11)?,
        rpa: row.get(12)?,
        fashlonae: row.get(13)?,
        mind_percent: row.get(14)?,
        mind_text: row.get(15)?,
        source: Source::parse(&row.get::<_, String>(16)?),
        instant: row.get(17)?,
    })
}

pub(crate) fn snapshot_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
    Ok(Snapshot {
        at: row.get(0)?,
        game_time: row.get(1)?,
        instance: row.get(2)?,
        character: row.get(3)?,
        kind: SnapshotKind::parse(&row.get::<_, String>(4)?),
        name: row.get(5)?,
        value: row.get(6)?,
        weekly: row.get(7)?,
        total: row.get(8)?,
    })
}

pub(crate) fn reward_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reward> {
    Ok(Reward {
        at: row.get(0)?,
        game_time: row.get(1)?,
        instance: row.get(2)?,
        character: row.get(3)?,
        day: row.get(4)?,
        streak: row.get(5)?,
        bridges: row.get(6)?,
        reward: row.get(7)?,
        thanks: row.get(8)?,
        line: row.get(9)?,
    })
}

pub(crate) fn store_visit_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoreVisit> {
    Ok(StoreVisit {
        at: row.get(0)?,
        account: row.get(1)?,
        game: row.get(2)?,
        claimed: row.get(3)?,
        message: row.get(4)?,
        balance: row.get(5)?,
        next_at: row.get(6)?,
    })
}

fn load_pulses(conn: &rusqlite::Connection) -> Result<Vec<Stored<Pulse>>> {
    let sql = format!("SELECT {}, id FROM pulses ORDER BY id", schema::PULSE_COLUMNS);
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(row.get(18)?, pulse_from_row(row)?));
    }
    Ok(out)
}

fn load_snapshots(conn: &rusqlite::Connection) -> Result<Vec<Stored<Snapshot>>> {
    let sql = format!(
        "SELECT {}, id FROM snapshots ORDER BY id",
        schema::SNAPSHOT_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(row.get(9)?, snapshot_from_row(row)?));
    }
    Ok(out)
}

fn load_rewards(conn: &rusqlite::Connection) -> Result<Vec<Stored<Reward>>> {
    let sql = format!(
        "SELECT {}, id FROM login_rewards ORDER BY id",
        schema::REWARD_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(row.get(10)?, reward_from_row(row)?));
    }
    Ok(out)
}

fn load_store_visits(conn: &rusqlite::Connection) -> Result<Vec<Stored<StoreVisit>>> {
    let sql = format!(
        "SELECT {}, id FROM store_visits ORDER BY id",
        schema::STORE_VISIT_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(row.get(7)?, store_visit_from_row(row)?));
    }
    Ok(out)
}

fn load_imports(conn: &rusqlite::Connection) -> Result<Vec<Stored<Import>>> {
    let mut stmt = conn.prepare(
        "SELECT id, instance, character, stamp, path, size, imported_at, first_at, last_at, \
         hunts, pulses FROM imports ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(
            row.get(0)?,
            Import {
                instance: row.get(1)?,
                character: row.get(2)?,
                stamp: row.get(3)?,
                path: row.get(4)?,
                size: row.get(5)?,
                imported_at: row.get(6)?,
                first_at: row.get(7)?,
                last_at: row.get(8)?,
                hunts: row.get(9)?,
                pulses: row.get(10)?,
            },
        ));
    }
    Ok(out)
}

fn load_names(conn: &rusqlite::Connection) -> Result<Vec<Stored<Name>>> {
    let mut stmt = conn
        .prepare("SELECT id, kind, name, number, mnemonic FROM names ORDER BY id")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(
            row.get(0)?,
            Name {
                kind: row.get(1)?,
                name: row.get(2)?,
                number: row.get(3)?,
                mnemonic: row.get(4)?,
            },
        ));
    }
    Ok(out)
}

fn load_hunts(conn: &rusqlite::Connection) -> Result<Vec<Stored<Hunt>>> {
    let mut stmt = conn
        .prepare("SELECT id, instance, character, started_at, ended_at FROM hunts ORDER BY id")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(
            row.get(0)?,
            Hunt {
                instance: row.get(1)?,
                character: row.get(2)?,
                started_at: row.get(3)?,
                ended_at: row.get(4)?,
            },
        ));
    }
    Ok(out)
}

fn load_attacks(conn: &rusqlite::Connection) -> Result<Vec<Stored<Attack>>> {
    let mut stmt = conn.prepare("SELECT id, hunt_id, occurred_at FROM attacks ORDER BY id")?;
    let mut rows = stmt.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(Stored::new(
            row.get(0)?,
            Attack {
                hunt_id: row.get(1)?,
                occurred_at: row.get(2)?,
            },
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hydra-ledger-store-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn pulse(at: i64, exp_gained: i64) -> Pulse {
        Pulse {
            instance: "prime".to_string(),
            character: "Sindar".to_string(),
            at,
            exp: 1_000,
            axp: 5,
            exp_gained,
            rpa: Some("1.25x".to_string()),
            mind_text: Some("clear".to_string()),
            game_time: Some(1_417),
            source: Source::Bounty,
            ..Pulse::default()
        }
    }

    #[test]
    fn a_real_sqlite_file_round_trips_every_table() {
        let dir = scratch("round-trip");
        let path = dir.join("hydra.db");
        {
            let mut conn = Connection::open(&path, false).unwrap();
            assert_eq!(conn.user_version(), 0);
            crate::migrate(&mut conn).unwrap();
            assert_eq!(conn.user_version(), 3);
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
        }
        // A second, read-only connection sees all of it: this is the SQLite
        // file, not a cache.
        let conn = Connection::open(&path, true).unwrap();
        assert_eq!(conn.user_version(), 3);
        let db = conn.db();
        assert_eq!(db.pulses.len(), 2);
        assert_eq!(db.pulses[0].row.at, 10);
        assert_eq!(db.pulses[0].row.rpa.as_deref(), Some("1.25x"));
        assert_eq!(db.pulses[0].row.exp, 1_000);
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
        drop(db);
        // A read-only connection refuses to write (`SQLITE_READONLY`).
        assert_eq!(conn.insert_pulse(pulse(99, 1)), Err(Error::ReadOnly));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nullable_columns_come_back_as_none_and_exp_is_clamped() {
        let dir = scratch("nullable");
        let mut which = Connection::open(dir.join("hydra.db"), false).unwrap();
        crate::migrate(&mut which).unwrap();
        // Every optional column left at `None` …
        which
            .insert_pulse(Pulse {
                instance: "prime".to_string(),
                character: "Sindar".to_string(),
                at: 10,
                exp: -1,
                axp: -2,
                ..Pulse::default()
            })
            .unwrap();
        // … and a fully populated one.
        which
            .insert_pulse(Pulse {
                instance: "prime".to_string(),
                character: "Sindar".to_string(),
                at: 20,
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
                source: Source::Instant,
                instant: 4_242,
            })
            .unwrap();
        let db = which.db();
        let bare = &db.pulses[0].row;
        assert_eq!(bare.game_time, None);
        assert_eq!(bare.fxp, None);
        assert_eq!(bare.rpa, None);
        assert_eq!(bare.mind_text, None);
        // `max(v, 0)` — the recovered `bic x10, x23, x23, asr #63`.
        assert_eq!((bare.exp, bare.axp), (0, 0));
        let full = &db.pulses[1].row;
        assert_eq!(full.game_time, Some(1_417));
        assert_eq!(full.until_next, Some(250_000));
        assert_eq!(full.lumnis, Some(2));
        assert_eq!(full.fashlonae, Some(1));
        assert_eq!(full.mind_percent, Some(97));
        assert_eq!(full.mind_text.as_deref(), Some("must rest"));
        assert_eq!(full.source, Source::Instant);
        assert_eq!(full.instant, 4_242);
        // The NULLs really are NULL in SQLite, not empty strings.
        let nulls: i64 = which
            .raw()
            .query_row(
                "SELECT COUNT(*) FROM pulses WHERE at = 10 AND game_time IS NULL \
                 AND fxp IS NULL AND rpa IS NULL AND mind_text IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(nulls, 1);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_writable_connection_is_wal_and_normal_with_a_five_second_busy_timeout() {
        let dir = scratch("pragmas");
        let conn = Connection::open(dir.join("hydra.db"), false).unwrap();
        let journal_mode: String = conn
            .raw()
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let synchronous: i64 = conn
            .raw()
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        let busy_timeout: i64 = conn
            .raw()
            .pragma_query_value(None, "busy_timeout", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        // NORMAL is 1 (OFF 0, FULL 2, EXTRA 3).
        assert_eq!(synchronous, 1);
        assert_eq!(busy_timeout, 5_000);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_database_is_an_error_for_a_read_only_connection() {
        let path = std::env::temp_dir().join(format!(
            "hydra-ledger-does-not-exist-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert!(Connection::open(&path, true).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_database_is_refused_not_mis_parsed() {
        let dir = scratch("foreign");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hydra.db");
        // The pre-Phase-1 line format: a real SQLite build must not read it.
        std::fs::write(&path, "hydra-ledger\t3\npulse\t1\t10\n").unwrap();
        assert!(!crate::is_this_builds_store(&path));
        // A writable connection sets `journal_mode`, which makes SQLite read
        // the header, so opening refuses a file that is not a database.
        let error = Connection::open(&path, false).unwrap_err();
        assert!(error.to_string().contains("file is not a database"), "{error}");
        // A read-only connection opens lazily; the version read is where
        // SQLite notices, and `migrate` reports the same.
        let mut conn = Connection::open(&path, true).unwrap();
        assert!(conn.try_user_version().is_err());
        assert_eq!(conn.user_version(), 0);
        assert!(crate::migrate(&mut conn).is_err());
        // An empty file *is* a database SQLite will initialise.
        std::fs::write(&path, b"").unwrap();
        let mut conn = Connection::open(&path, false).unwrap();
        crate::migrate(&mut conn).unwrap();
        assert_eq!(conn.user_version(), 3);
        assert!(crate::is_this_builds_store(&path));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execute_batch_runs_the_recovered_texts_and_any_other_sql() {
        let conn = Connection::in_memory(false);
        // A fresh in-memory connection already carries the schema, so applying
        // the batches again is a constraint error — from SQLite, not from a
        // dispatch table.
        assert!(conn.execute_batch(schema::MIGRATIONS[0]).is_err());
        // Transactions and arbitrary SQL are real now.
        assert_eq!(conn.execute_batch(schema::BEGIN_DEFERRED), Ok(()));
        assert_eq!(
            conn.execute_batch(
                "CREATE TABLE scratch (id INTEGER PRIMARY KEY, note TEXT);
                 INSERT INTO scratch (note) VALUES ('hello');"
            ),
            Ok(())
        );
        assert_eq!(conn.execute_batch(schema::COMMIT), Ok(()));
        let note: String = conn
            .raw()
            .query_row("SELECT note FROM scratch", [], |row| row.get(0))
            .unwrap();
        assert_eq!(note, "hello");
        // A syntax error is a `rusqlite` error, reported as `Error::Sqlite`.
        assert!(matches!(
            conn.execute_batch("SELECT FROM"),
            Err(Error::Sqlite(_))
        ));
    }

    #[test]
    fn the_views_and_joined_tables_of_the_base_schema_exist() {
        let conn = Connection::in_memory(false);
        let mut stmt = conn
            .raw()
            .prepare("SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view')")
            .unwrap();
        let names: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        for wanted in ["pulses", "snapshots", "names", "imports", "login_rewards", "store_visits"] {
            assert!(
                names.iter().any(|(name, kind)| name == wanted && kind == "table"),
                "missing table {wanted}"
            );
        }
        for view in ["flares", "hunt_gemstones"] {
            assert!(
                names.iter().any(|(name, kind)| name == view && kind == "view"),
                "missing view {view}"
            );
        }
    }

    #[test]
    fn the_pulse_view_and_the_recovered_select_agree_on_the_columns() {
        // `load_pulses` appends `id` after the recovered column list, so the
        // mapper's indices and the view's are the same ones.
        assert_eq!(schema::PULSE_COLUMNS.split(", ").count(), 18);
        let conn = Connection::in_memory(false);
        conn.insert_pulse(pulse(10, 100)).unwrap();
        let db = conn.db();
        assert_eq!(db.pulses[0].id, 1);
        assert_eq!(db.pulses[0].row.at, 10);
        drop(db);
    }
}
