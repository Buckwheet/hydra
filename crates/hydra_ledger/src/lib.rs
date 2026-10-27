//! `hydra_ledger` — Hydra's SQLite-backed ledger.
//!
//! Recovered from the arm64 Mach-O binary `~/Code/hydra-inspect/hydra` (no
//! DWARF; 25 external text symbols in this crate plus the local helpers
//! `reader::{pulse,reward,snapshot,store_visit}`, `row::{Source::parse,
//! Who::new}`, `writer::run`, `writer::{Writer::record,Writer::flush}` and
//! `Ledger::open`, which the demangled symbol file omits and `nm -an` has).
//! `NOTES.md` has the per-function tables, every address, and the evidence.
//!
//! # Shape of the crate
//!
//! ```text
//! Ledger  { dir }                       the database's directory
//!   .path_in()      -> dir/hydra.db     0x10168e91c
//!   .connect(ro)    -> Connection       0x10168e614
//!   .writing()      -> Connection       0x10168e92c  (= connect(false))
//!   .reader()       -> Reader           0x10168e934
//!   .writer()       -> Writer           0x10168e9e0  (spawns a thread)
//!   .note_import(…) -> ()               0x10168e2d8
//!   .prune_pulses(…) -> usize           0x10168e4ec
//! migrate(conn)                        0x10168ed90   user_version 0 -> 3
//! name_id(conn, kind, name, …) -> i64   0x10168f25c   the `names` interning
//! ```
//!
//! # Dependencies (Phase 1 reconnection)
//!
//! The two crates the original linked are back at their workspace pins:
//!
//! * **`rusqlite` 0.37.0 (`bundled`)** — [`store::Connection`] is a real
//!   `rusqlite::Connection` over a real `hydra.db`. [`migrate`] runs the three
//!   recovered batches inside one transaction each and moves
//!   `PRAGMA user_version` 0 → 3; the readers run the recovered `SELECT`s
//!   through `prepare_cached` (rusqlite's `StatementCache`); the writer's four
//!   `INSERT`s go through `execute` inside a `Transaction`.
//! * **`tokio` 1.53.1 (`full`)** — [`writer::Writer`] owns a
//!   `tokio::sync::mpsc::Sender<writer::Sent>` over a 1024-slot channel,
//!   `try_send`s rows with `Writer::try_record`, answers flushes through a
//!   `tokio::sync::oneshot`, and drives `writer::run_async` on the crate's
//!   runtime; `record_async`/`flush_async` are the recovered `async fn`s, with
//!   `record`/`flush` as blocking wrappers for synchronous siblings.
//!
//! `NOTES.md` §6 keeps the table of what used to stand in for both.

#![allow(clippy::too_many_arguments)]

use std::fmt;
use std::path::{Path, PathBuf};

pub mod reader;
pub mod row;
pub mod schema;
pub mod store;
pub mod writer;

mod runtime;

pub use row::{Pulse, Reward, Row, Snapshot, SnapshotKind, Source, Stamp, StoreVisit, Who};
pub use store::{Attack, Connection, Db, Hunt, Import, Name, Stored};

/// Everything that can go wrong.
///
/// The original's errors are whatever `rusqlite` (and `std::io` for the writer
/// thread) produced; the crate passes them to `Display` and prints the text.
/// This type keeps the same shapes so callers can still match; the `rusqlite`
/// arms are filled by the [`From`] conversion below, so `?` on a
/// `rusqlite::Result` lands here instead of unwrapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A `rusqlite::Error`'s `Display` text.
    Sqlite(String),
    /// A `std::io::Error`'s `Display` text.
    Io(String),
    /// The ledger's own message (the schema-version refusal, for instance).
    Message(String),
    /// A write on a connection opened read-only (`SQLITE_READONLY`).
    ReadOnly,
    /// Kept for source compatibility: the pre-Phase-1 store refused statements
    /// it did not recognise. There is a SQL engine now, so nothing produces
    /// this — a bad statement is [`Error::Sqlite`].
    Unsupported(String),
    /// A unique index (or another constraint) refused the row;
    /// `SQLITE_CONSTRAINT`'s own message text.
    Constraint(String),
}

impl From<rusqlite::Error> for Error {
    /// `rusqlite::Error` → this crate's error.
    ///
    /// The two arms that carry meaning for callers besides the text are
    /// `SQLITE_READONLY` (→ [`Error::ReadOnly`]) and `SQLITE_CONSTRAINT`
    /// (→ [`Error::Constraint`], so `note_import`'s documented refusal of a
    /// duplicate stamp keeps reporting "UNIQUE constraint failed: …").
    fn from(error: rusqlite::Error) -> Error {
        match &error {
            rusqlite::Error::SqliteFailure(code, message) => match code.code {
                rusqlite::ErrorCode::ReadOnly => Error::ReadOnly,
                rusqlite::ErrorCode::ConstraintViolation => Error::Constraint(
                    message
                        .clone()
                        .unwrap_or_else(|| rusqlite::Error::to_string(&error)),
                ),
                _ => Error::Sqlite(error.to_string()),
            },
            _ => Error::Sqlite(error.to_string()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sqlite(text) | Error::Io(text) | Error::Message(text) => f.write_str(text),
            Error::ReadOnly => f.write_str("attempt to write a readonly database"),
            Error::Unsupported(text) => write!(f, "unsupported statement in this build: {text}"),
            Error::Constraint(text) => write!(f, "constraint failed: {text}"),
        }
    }
}

impl std::error::Error for Error {}

/// The crate's result.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// The ledger's directory, and the door to its database.
///
/// The original struct is a `PathBuf`-holding handle: `Ledger::connect`
/// (`0x10168e614`) loads the path as a `&str` from `+0x08`/`+0x10` (the
/// `String` layout `{ cap @0, ptr @8, len @0x10 }`) and hands it to
/// `rusqlite::path_to_cstring`, and `Ledger::path_in` (`0x10168e91c`, 16 bytes)
/// is `self.0.join("hydra.db")` — a `std::path::Path::_join` tail call with the
/// eight-byte literal at `0x103985c80`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// The directory the database lives in (the original's first field).
    pub dir: PathBuf,
}

impl Ledger {
    /// A ledger over `dir`. `Ledger::open` (`0x101282b78`, not a stub — it is
    /// inlined into the callers that build a ledger) is this plus `connect`,
    /// `migrate` and a directory that exists.
    pub fn new(dir: impl Into<PathBuf>) -> Ledger {
        Ledger { dir: dir.into() }
    }

    /// `Ledger::open(path) -> Result<Ledger>` — open (creating) the database in
    /// `dir` and bring its schema up to date.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Ledger> {
        let ledger = Ledger::new(dir);
        std::fs::create_dir_all(&ledger.dir).map_err(|e| Error::Io(e.to_string()))?;
        let mut conn = ledger.connect(false)?;
        migrate(&mut conn)?;
        conn.commit()?;
        Ok(ledger)
    }

    /// `Ledger::path_in(&self) -> PathBuf` — **full**.
    ///
    /// `@0x10168e91c`: `adrp/add` the eight-byte `"hydra.db"` at `0x103985c80`
    /// and tail-call `Path::_join`, i.e. `self.dir.join(schema::DB_FILE)`.
    pub fn path_in(&self) -> PathBuf {
        self.dir.join(schema::DB_FILE)
    }

    /// `Ledger::connect(&self, read_only: bool) -> Result<Connection>` —
    /// **full**.
    ///
    /// `@0x10168e614` (776 bytes):
    ///
    /// 1. the path (a `&str` at `+0x08`/`+0x10`) goes through
    ///    `rusqlite::path_to_cstring`;
    /// 2. the flags are `csel w2, w9(=0x8001), w8(=0x8006), ne` on the argument:
    ///    `read_only != 0` → `0x8001` = `SQLITE_OPEN_READ_ONLY |
    ///    SQLITE_OPEN_NO_MUTEX`, otherwise `0x8006` = `SQLITE_OPEN_READ_WRITE |
    ///    SQLITE_OPEN_CREATE | SQLITE_OPEN_NO_MUTEX`;
    /// 3. `InnerConnection::open_with_flags(path, flags, None)`;
    /// 4. `Connection::busy_timeout(Duration { secs: 5, nanos: 0 })`
    ///    (`mov w1, #0x5; mov w2, #0x0`);
    /// 5. only when *not* read-only (`tbz w8, #0x0` on the argument):
    ///    `pragma_update(None, "journal_mode", "WAL")` (12/3 bytes at
    ///    `0x103983808`/`0x103983814`) and `pragma_update(None, "synchronous",
    ///    "NORMAL")` (11/6 bytes at `0x103983817`/`0x103983822`).
    ///
    /// The read-only flag is what [`Ledger::reader`] passes (`mov w1, #1`) and
    /// what [`Ledger::writer`], [`Ledger::writing`], `note_import` and
    /// `prune_pulses` pass as `0`.
    pub fn connect(&self, read_only: bool) -> Result<Connection> {
        Connection::open(self.path_in(), read_only)
    }

    /// `Ledger::writing(&self) -> Result<Connection>` — **full** (8 bytes).
    ///
    /// `@0x10168e92c`: `mov w1, #0; b Ledger::connect` — a read-write
    /// connection, no wrapper.
    pub fn writing(&self) -> Result<Connection> {
        self.connect(false)
    }

    /// `Ledger::reader(&self) -> Result<Reader>` — **full**.
    ///
    /// `@0x10168e934` (172 bytes): `connect(1)` and then a straight move of the
    /// connection value into the result — `Reader` is a one-field wrapper around
    /// the connection (its `drop_in_place` at `0x10008bf4c` is
    /// `drop_in_place<rusqlite::Connection>` inlined). `Reader::characters`'
    /// `add x0, x0, #0x20` shows the `StatementCache` living at `+0x20` inside
    /// that connection.
    pub fn reader(&self) -> Result<reader::Reader> {
        Ok(reader::Reader::new(self.connect(true)?))
    }

    /// `Ledger::writer(&self) -> Result<Writer>` — **full for the setup**.
    ///
    /// `@0x10168e9e0` (944 bytes): `connect(0)`; a
    /// `tokio::sync::Semaphore::new(1024)`; `tokio::sync::mpsc::channel`; a
    /// `std::thread::Builder::name("hydra-ledger")` (`0x103983828`, twelve
    /// bytes assembled from a `movk` pair) `spawn_unchecked` of `writer::run`;
    /// and, if the spawn fails,
    /// `format!("ledger thread: {}", io_error)` (blob `0x103e6e5d5`).
    ///
    /// The queue is a real `tokio::sync::mpsc::channel(1024)` again (Phase 1),
    /// and the thread drives `writer::run_async` on this crate's tokio runtime;
    /// `std::thread::Builder::spawn` replaces `spawn_unchecked`. See
    /// `writer.rs`.
    pub fn writer(&self) -> Result<writer::Writer> {
        writer::Writer::start(self.connect(false)?)
    }

    /// `Ledger::prune_pulses(&self, who: &Who, drop_all: bool) -> Result<usize>`
    /// — **full**.
    ///
    /// `@0x10168e4ec` (296 bytes): `connect(0)`, then one
    /// `Connection::execute` of either
    /// `DELETE FROM pulses WHERE instance = ?1 AND character = ?2` (57 bytes at
    /// `0x1039837cf`) when `drop_all != 0`, or
    /// `DELETE FROM pulses WHERE instance = ?1 AND character = ?2 AND
    /// exp_gained = 0 AND axp_gained = 0` (108 bytes at `0x103983763`) otherwise
    /// (`csel x2, …, ne` on the flag, `w3 = 57`/`108`). The binds are
    /// `&who.instance` (`+0x00`) and `&who.character` (`add x20, #0x18`).
    pub fn prune_pulses(&self, who: &Who, drop_all: bool) -> Result<usize> {
        let conn = self.connect(false)?;
        let dropped = conn.delete_pulses(&who.instance, &who.character, drop_all)?;
        conn.commit()?;
        Ok(dropped)
    }

    /// `Ledger::note_import(&self, …) -> Result<()>` — **full**.
    ///
    /// `@0x10168e2d8` (532 bytes): `connect(0)`, `SystemTime::now()` →
    /// `duration_since(UNIX_EPOCH)` for `imported_at` (seconds clamped to
    /// `i64::MAX`, `csel` on the error arm), and one `Connection::execute` of
    /// the 185-byte `INSERT INTO imports (…) VALUES (?1 … ?10)` at
    /// `0x1039857a5` with ten binds (`mov w5, #0xa`).
    ///
    /// The argument registers give the shape: `x1 = &Who`, `x2/x3` = the first
    /// `&str`, `x4/x5` = the second, `x6` = a `u64` clamped to `i64::MAX`
    /// (`cmp x22, #0x7fff…; csel`), `x7` = a pointer to the 32-byte
    /// `(Option<i64>, Option<i64>)` pair that rustc passes indirectly for
    /// `first_at`/`last_at` (`stp x20, …` with the same `ToSql` vtable for
    /// `x20` and `x20 + 0x10`), and the last two numbers arrive on the stack
    /// (`ldp x9, x8, [x29, #0x10]`).
    pub fn note_import(
        &self,
        who: &Who,
        stamp: &str,
        path: &str,
        size: u64,
        times: (Option<i64>, Option<i64>),
        hunts: u64,
        pulses: u64,
    ) -> Result<()> {
        let conn = self.connect(false)?;
        let stamp_now = {
            use std::time::{SystemTime, UNIX_EPOCH};
            match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(elapsed) => i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
                Err(_) => 0,
            }
        };
        conn.insert_import(Import {
            instance: who.instance.clone(),
            character: who.character.clone(),
            stamp: stamp.to_string(),
            path: path.to_string(),
            size: size.min(i64::MAX as u64),
            imported_at: stamp_now,
            first_at: times.0,
            last_at: times.1,
            hunts,
            pulses,
        })?;
        conn.commit()
    }
}

/// `migrate(conn) -> Result<()>` — **full**.
///
/// `@0x10168ed90` (1228 bytes). In order:
///
/// 1. `prepare_with_flags("PRAGMA user_version", 19)` (`0x103985c1b`) and
///    `query_row(())` → the schema version;
/// 2. `cmp w22, #3; b.ls` — a version **above** [`schema::SCHEMA_VERSION`]
///    returns the `format!` at `0x103e6e5e7`:
///    `the ledger is at schema {}, which this Hydra does not know: move {}
///    aside and import the recordings again` (both arguments `Display`);
/// 3. when the version is `n < 3`, for each `i` in `n..3`: open a deferred
///    transaction (`BEGIN DEFERRED`, the first entry of the three-entry table),
///    `execute_batch(MIGRATIONS[i])`, then `pragma_update(None,
///    "user_version", i + 1)` and `COMMIT` — one transaction per migration,
///    `i + 1` saturating (`csel` against `0xffff_ffff`);
/// 4. `Ok(())`.
///
/// The transaction is a `rusqlite::Transaction` (`unchecked_transaction`,
/// which issues exactly [`schema::BEGIN_DEFERRED`]), so a batch that fails
/// rolls back instead of leaving the connection inside a transaction.
///
/// `// UNKNOWN:` the `BEGIN` variant. The binary indexes the three-entry table
/// at `0x1044428c0` with `ldrb w8, [x20, #0x68]` — one byte **past** the 0x68
/// bytes of the connection value it was handed, i.e. the caller's `Result` tag
/// byte in practice (`Ledger::open` writes the tag there and only calls
/// `migrate` when it is 0). That makes it `BEGIN DEFERRED` in every recovered
/// call path; this build always uses the deferred form and keeps the three
/// texts in [`schema`] for the record.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    let version = conn.try_user_version()?;
    if version > schema::SCHEMA_VERSION {
        return Err(Error::Message(format!(
            "the ledger is at schema {}, which this Hydra does not know: move {} aside and import the recordings again",
            version,
            conn.path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| schema::DB_FILE.to_string()),
        )));
    }
    for index in (version as usize)..schema::MIGRATIONS.len() {
        let next = (index as u64 + 1).min(u32::MAX as u64) as u32;
        let tx = conn.raw().unchecked_transaction()?;
        tx.execute_batch(schema::MIGRATIONS[index])?;
        tx.pragma_update(None, schema::USER_VERSION_NAME, next)?;
        tx.commit()?;
    }
    conn.commit()
}

/// `name_id(conn, kind, name, number, mnemonic) -> Result<i64>` — **full**.
///
/// `@0x10168f25c` (352 bytes). `Connection::execute` of
/// [`schema::INSERT_NAME`] (82 bytes, `mov w3, #0x52`, four binds — `x1/x2` and
/// `x3/x4` are the two `&str`s, `x5/x6` the `Option<i64>` number, `x7/x8` the
/// `Option<&str>` mnemonic), then `prepare_with_flags(schema::SELECT_NAME_ID,
/// 50)` and `query_row` with two binds → the interned id.
///
/// `names_kind_name` is a unique index, so a name is interned once; the first
/// row's `number`/`mnemonic` stick. This is the seam `hydra_combat_log`'s
/// `sync_gear` uses for its `name_id` columns.
pub fn name_id(
    conn: &Connection,
    kind: &str,
    name: &str,
    number: Option<i64>,
    mnemonic: Option<&str>,
) -> Result<i64> {
    conn.intern_name(kind, name, number, mnemonic)
}

/// The ledger's error text when the writer thread is gone — `Closed`'s
/// `Display`, `<hydra_ledger::writer::Closed as core::fmt::Display>::fmt`
/// (`0x1016946f8`): one 43-byte literal at `0x1039861b2`.
pub const CLOSED_TEXT: &str = "the ledger is not taking rows, or lost some";

/// Whether `path` is a ledger database: a file with SQLite's header.
///
/// The pre-Phase-1 store wrote a `hydra-ledger` line file and used this to
/// refuse a real SQLite `hydra.db`. Now that the store *is* SQLite, the same
/// question is answered by SQLite's 16-byte file header
/// (`"SQLite format 3\0"`), read from the file and nothing else — cheap, and no
/// connection or lock involved.
///
/// Note what this does *not* prove: every SQLite database answers `true`,
/// whatever its schema or `user_version`. Whether it is a ledger this build
/// can use is [`migrate`]'s question, and it refuses a `user_version` above
/// [`schema::SCHEMA_VERSION`].
pub fn is_this_builds_store(path: &Path) -> bool {
    use std::io::Read;

    let mut magic = [0u8; 16];
    match std::fs::File::open(path) {
        Ok(mut file) => file.read_exact(&mut magic).is_ok() && &magic == b"SQLite format 3\0",
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hydra-ledger-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn path_in_joins_the_database_name() {
        let ledger = Ledger::new("/tmp/ledger");
        assert_eq!(ledger.path_in(), Path::new("/tmp/ledger/hydra.db"));
        assert_eq!(schema::DB_FILE, "hydra.db");
    }

    #[test]
    fn connect_is_read_only_only_when_asked() {
        let dir = scratch("connect");
        let ledger = Ledger::new(&dir);
        // A writable connection creates the database; a read-only one may then
        // read it back.
        let writing = ledger.writing().unwrap();
        assert!(!writing.read_only());
        writing.commit().unwrap();
        let reading = ledger.reader().unwrap();
        assert!(reading.connection().read_only());
        // A read-only connection refuses to write (`SQLITE_READONLY`).
        assert_eq!(
            reading
                .connection()
                .insert_pulse(Pulse {
                    instance: "prime".to_string(),
                    character: "Sindar".to_string(),
                    at: 10,
                    ..Pulse::default()
                })
                .map(|_| ()),
            Err(Error::ReadOnly)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_walks_the_three_batches_and_records_the_version() {
        let dir = scratch("migrate");
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = Ledger::new(&dir);
        let mut conn = ledger.connect(false).unwrap();
        assert_eq!(conn.user_version(), 0);
        migrate(&mut conn).unwrap();
        assert_eq!(conn.user_version(), 3);
        // Running it again is a no-op.
        migrate(&mut conn).unwrap();
        assert_eq!(conn.user_version(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_only_runs_the_steps_a_ledger_is_missing() {
        let dir = scratch("migrate-partial");
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = Ledger::new(&dir);
        let mut conn = ledger.connect(false).unwrap();
        // A ledger that stopped after the first batch: schema 1, no
        // `login_rewards`, no `store_visits`.
        conn.execute_batch(schema::MIGRATION_1).unwrap();
        conn.set_user_version(1).unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(conn.user_version(), 3);
        for table in ["login_rewards", "store_visits"] {
            let count: i64 = conn
                .raw()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "{table}");
        }
        // Batch 1 was not re-run: creating `pulses` twice would have failed.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrate_refuses_a_newer_schema_with_the_recovered_text() {        let dir = scratch("migrate-newer");
        std::fs::create_dir_all(&dir).unwrap();
        let ledger = Ledger::new(&dir);
        let mut conn = ledger.connect(false).unwrap();
        conn.set_user_version(9).unwrap();
        let error = migrate(&mut conn).unwrap_err();
        assert!(error.to_string().starts_with("the ledger is at schema 9, which this Hydra does not know: move "));
        assert!(error
            .to_string()
            .ends_with(" aside and import the recordings again"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_id_interns_once_and_keeps_the_first_number() {
        let conn = Connection::in_memory(false);
        let first = name_id(&conn, "gear", "a bronze sword", Some(7), Some("sword")).unwrap();
        let again = name_id(&conn, "gear", "a bronze sword", Some(99), None).unwrap();
        assert_eq!(first, again);
        let other = name_id(&conn, "gear", "a steel dagger", None, None).unwrap();
        assert_ne!(first, other);
        // The kind is part of the identity.
        let other_kind = name_id(&conn, "property", "a bronze sword", None, None).unwrap();
        assert_ne!(first, other_kind);
        let db = conn.db();
        assert_eq!(db.names.len(), 3);
        assert_eq!(db.names[0].row.number, Some(7));
        assert_eq!(db.names[0].row.mnemonic.as_deref(), Some("sword"));
        assert_eq!(conn.name_id_of("gear", "a bronze sword"), Some(first));
        assert_eq!(conn.name_id_of("gear", "nothing"), None);
    }

    #[test]
    fn note_import_records_the_row_and_refuses_a_duplicate_stamp() {
        let dir = scratch("note-import");
        // The original writes into a ledger `Ledger::open` has already
        // migrated; `connect` alone does not create the schema.
        let ledger = Ledger::open(&dir).unwrap();
        let who = Who::new("prime", "Sindar");
        ledger
            .note_import(
                &who,
                "2026-07-20T13:05:52",
                "/logs/2026-07-20.log",
                4_096,
                (Some(1_784_567_000), Some(1_784_567_152)),
                3,
                42,
            )
            .unwrap();
        let conn = ledger.connect(true).unwrap();
        let db = conn.db();
        assert_eq!(db.imports.len(), 1);
        let row = &db.imports[0].row;
        assert_eq!(row.instance, "prime");
        assert_eq!(row.character, "Sindar");
        assert_eq!(row.stamp, "2026-07-20T13:05:52");
        assert_eq!(row.path, "/logs/2026-07-20.log");
        assert_eq!(row.size, 4_096);
        assert_eq!(row.first_at, Some(1_784_567_000));
        assert_eq!(row.last_at, Some(1_784_567_152));
        assert_eq!(row.hunts, 3);
        assert_eq!(row.pulses, 42);
        assert!(row.imported_at >= 1_577_836_800);
        drop(db);
        // `imports_session` is unique on (instance, character, stamp).
        let error = ledger
            .note_import(
                &who,
                "2026-07-20T13:05:52",
                "/logs/2026-07-20.log",
                4_096,
                (None, None),
                0,
                0,
            )
            .unwrap_err();
        assert!(error.to_string().contains("UNIQUE constraint failed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_pulses_drops_empty_rows_then_everything() {
        let dir = scratch("prune");
        let ledger = Ledger::open(&dir).unwrap();
        let who = Who::new("prime", "Sindar");
        {
            let conn = ledger.connect(false).unwrap();
            for (at, gained) in [(10, 100), (20, 0), (30, 0), (40, 250)] {
                conn.insert_pulse(Pulse {
                    instance: who.instance.clone(),
                    character: who.character.clone(),
                    at,
                    exp_gained: gained,
                    ..Pulse::default()
                })
                .unwrap();
            }
            // Someone else's rows survive both prunes.
            conn.insert_pulse(Pulse {
                instance: who.instance.clone(),
                character: "Someone".to_string(),
                at: 50,
                exp_gained: 0,
                ..Pulse::default()
            })
            .unwrap();
        }
        assert_eq!(ledger.prune_pulses(&who, false).unwrap(), 2);
        assert_eq!(ledger.prune_pulses(&who, true).unwrap(), 2);
        let conn = ledger.connect(true).unwrap();
        let db = conn.db();
        assert_eq!(db.pulses.len(), 1);
        assert_eq!(db.pulses[0].row.character, "Someone");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_creates_the_directory_and_the_schema_version() {
        let dir = scratch("open");
        let ledger = Ledger::open(&dir).unwrap();
        assert!(ledger.path_in().exists());
        assert!(is_this_builds_store(&ledger.path_in()));
        // A real SQLite database, not the pre-Phase-1 line file.
        let header = std::fs::read(ledger.path_in()).unwrap();
        assert_eq!(&header[..16], b"SQLite format 3\0");
        let conn = ledger.connect(true).unwrap();
        assert_eq!(conn.user_version(), schema::SCHEMA_VERSION);
        // The recovered tables are really there.
        let tables: i64 = conn
            .raw()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
                 ('pulses', 'snapshots', 'names', 'imports', 'login_rewards', 'store_visits')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 6);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_this_builds_store_answers_for_sqlite_files_only() {
        let dir = scratch("magic");
        std::fs::create_dir_all(&dir).unwrap();
        // Missing file.
        assert!(!is_this_builds_store(&dir.join("nothing.db")));
        // A foreign text file (the old line format, say).
        let text = dir.join("text.db");
        std::fs::write(&text, "hydra-ledger\t3\npulse\t1\n").unwrap();
        assert!(!is_this_builds_store(&text));
        // A real ledger.
        let ledger = Ledger::open(dir.join("ledger")).unwrap();
        assert!(is_this_builds_store(&ledger.path_in()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_schema_newer_than_this_build_is_refused_and_left_alone() {
        let dir = scratch("newer-file");
        let ledger = Ledger::open(&dir).unwrap();
        {
            let conn = ledger.writing().unwrap();
            conn.set_user_version(4).unwrap();
        }
        // `Ledger::open` runs `migrate`, which refuses.
        let error = Ledger::open(&dir).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "the ledger is at schema 4, which this Hydra does not know: move {} aside and import the recordings again",
                ledger.path_in().display()
            )
        );
        // Nothing was touched on the way out: the version is still 4.
        let conn = ledger.connect(true).unwrap();
        assert_eq!(conn.user_version(), 4);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_closed_text_is_the_recovered_literal() {
        assert_eq!(CLOSED_TEXT, "the ledger is not taking rows, or lost some");
        assert_eq!(CLOSED_TEXT.len(), 43);
        assert_eq!(writer::Closed.to_string(), CLOSED_TEXT);
    }
}
