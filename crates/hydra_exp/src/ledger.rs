//! `hydra_exp::ledger` — the real `hydra_ledger` behind the crate's seams.
//!
//! The binary does not keep its own rows: `command::read`/`answer` open
//! `hydra_ledger::Ledger::reader()` (`0x10168e934`) and call
//! `Reader::{last_pulses 0x10168b5d8, pulses 0x10168c5b0, latest_snapshots
//! 0x10168c174, snapshots 0x10168cdac}`, `answer` writes through
//! `writer::Writer::try_record` (`0x10169398c`) and `Ledger::prune_pulses`
//! (`0x10168e4ec`), and `recorder::start` walks `latest_snapshots` and writes
//! snapshots. This module is that half, as two thin adapters:
//!
//! | adapter | implements | over |
//! |---|---|---|
//! | [`LedgerSource`] | [`crate::command::Source`] | `Ledger::{reader, writer, prune_pulses}` |
//! | [`LedgerSink`] | [`crate::recorder::Sink`] | `Ledger::{reader, writer}` |
//!
//! Both are bound to one `hydra_ledger::row::Who` (an **instance + character**
//! pair) because that is what every `Reader` query filters on, while
//! [`crate::row::Who`] carries the character name alone.
//!
//! # The mapping, field by field
//!
//! [`crate::row::Pulse`] and `hydra_ledger::row::Pulse` are the same 208-byte
//! row (`stride 0xd0`); the names differ where `hydra_exp`'s copy was read off
//! the reports and the ledger's off the `pulses` table:
//!
//! | `hydra_exp` | `hydra_ledger` |
//! |---|---|
//! | `bonus` (`+0xc0`, the brooch/instant amount) | `instant` (the column name) |
//! | `fashlonae: Option<u8>` | `fashlonae: Option<u32>` (the whole value word) |
//! | `source: crate::row::Source` | `source: hydra_ledger::row::Source` (same five bytes) |
//! | — | `instance`, `character`, `mind_text`, `game_time`, `fxp`, `fxp_max`, `mind_percent` |
//!
//! The fields this crate has no source for are filled from the bound `Who`
//! (instance/character) or left `None` on the way in, and dropped on the way
//! out. `fashlonae` is narrowed to `u8` **clamping** rather than discarding,
//! because `report::bonuses` only asks whether the tag is `1` and the word is
//! `2`: the presence of the bonus is what carries the meaning.
//!
//! [`crate::row::Snapshot`] against `hydra_ledger::row::Snapshot`:
//!
//! | `hydra_exp` | `hydra_ledger` |
//! |---|---|
//! | `label` (`+0x60`) | `name` (`snapshots.name`) |
//! | `kind: u8` (`+0x88`) | `kind: SnapshotKind` (`favor`/`resource`/`suffused`) |
//! | — | `instance`, `character`, `game_time` |
//!
//! # Two deliberate differences from the binary
//!
//! 1. **One writer per recorded row.** The binary opens `Ledger::writer()` once
//!    and keeps the handle, and `Writer::try_record` only `try_send`s. This
//!    workspace's ledger `Connection` is a whole-file store, so a long-lived
//!    writer holds the database as it was when the thread started and every
//!    `COMMIT` rewrites the file from that copy — it would clobber rows another
//!    connection (a `prune_pulses`, say) deleted in the meantime. Each write
//!    here therefore uses its own `Writer` and flushes it, which makes
//!    `try_record` blocking and durable: `Ok(())` means the row is on disk and
//!    a later read sees it. With a real SQLite connection
//!    (`hydra_ledger`'s `rusqlite` phase) the long-lived writer becomes both
//!    safe and faithful again.
//! 2. **Closed windows.** `Reader::pulses`/`Reader::snapshots` are half-open
//!    (`at >= from AND at < to`); [`crate::command::Source`] documents closed
//!    intervals, so the adapter passes `to + 1` (saturating).
//!
//! # Integration risk
//!
//! `hydra_ledger` is being rewritten in parallel (its own Phase-1 worktree), so
//! this module is kept deliberately thin: it only uses
//! `Ledger::{open, reader, writer, prune_pulses}`, the four `Reader` methods the
//! binary calls, `Writer::{try_record, flush}`, `Closed`'s `Display` and the two
//! row types. A change to any of those signatures breaks this file and nothing
//! else in `hydra_exp`.

use hydra_ledger::row as lrow;
use hydra_ledger::Ledger;

use crate::command::Source;
use crate::recorder::Sink;
use crate::row::{Pulse, Snapshot, Who};

/// `hydra_ledger::row::Pulse` → [`crate::row::Pulse`] (the reports' view).
pub fn pulse_out(row: &lrow::Pulse) -> Pulse {
    Pulse {
        at: row.at,
        rpa: row.rpa.clone(),
        until_next: row.until_next,
        lumnis: row.lumnis,
        // The ledger keeps the full value word; `report::bonuses` only compares
        // it with `2`, so clamping keeps the tag's meaning without pretending
        // this crate's `Option<u8>` can hold a `u32`.
        fashlonae: row.fashlonae.map(|word| word.min(u32::from(u8::MAX)) as u8),
        exp: row.exp,
        axp: row.axp,
        exp_gained: row.exp_gained,
        axp_gained: row.axp_gained,
        // `pulses.instant` is the amount a brooch/instant row holds.
        bonus: row.instant,
        source: crate::row::Source::from_u8(row.source.to_u8()),
    }
}

/// [`crate::row::Pulse`] → `hydra_ledger::row::Pulse`, filed under `who`.
pub fn pulse_in(row: &Pulse, who: &lrow::Who) -> lrow::Pulse {
    lrow::Pulse {
        instance: who.instance.clone(),
        character: who.character.clone(),
        rpa: row.rpa.clone(),
        at: row.at,
        until_next: row.until_next,
        lumnis: row.lumnis,
        fashlonae: row.fashlonae.map(u32::from),
        exp: row.exp,
        axp: row.axp,
        exp_gained: row.exp_gained,
        axp_gained: row.axp_gained,
        instant: row.bonus,
        source: lrow::Source::from_u8(row.source as u8),
        // The fields only the ledger's own queries use.
        mind_text: None,
        game_time: None,
        fxp: None,
        fxp_max: None,
        mind_percent: None,
    }
}

/// `hydra_ledger::row::Snapshot` → [`crate::row::Snapshot`] (`name` → `label`,
/// typed kind → the `which` byte).
pub fn snapshot_out(row: &lrow::Snapshot) -> Snapshot {
    Snapshot {
        value: row.value,
        weekly: row.weekly,
        total: row.total,
        label: row.name.clone(),
        at: row.at,
        kind: row.kind.to_u8(),
    }
}

/// [`crate::row::Snapshot`] → `hydra_ledger::row::Snapshot`, filed under `who`.
pub fn snapshot_in(row: &Snapshot, who: &lrow::Who) -> lrow::Snapshot {
    lrow::Snapshot {
        value: row.value,
        weekly: row.weekly,
        total: row.total,
        name: row.label.clone(),
        at: row.at,
        kind: kind_in(row.kind),
        instance: who.instance.clone(),
        character: who.character.clone(),
        game_time: None,
    }
}

/// The `which` byte → the ledger's typed kind. An unknown byte falls back to
/// `favor`, the same way `hydra_ledger::row::SnapshotKind::parse` does.
fn kind_in(which: u8) -> lrow::SnapshotKind {
    match which {
        1 => lrow::SnapshotKind::Resource,
        2 => lrow::SnapshotKind::Suffused,
        _ => lrow::SnapshotKind::Favor,
    }
}

/// [`crate::command::Source`] over a real ledger, bound to one character.
///
/// The four readers are the `Reader` methods the binary calls; `try_record` and
/// `prune_pulses` are `Writer::try_record` and `Ledger::prune_pulses`. Errors
/// are `Display` text, as the recovered tag-`1` replies are.
#[derive(Clone, Debug)]
pub struct LedgerSource {
    ledger: Ledger,
    who: lrow::Who,
}

impl LedgerSource {
    /// A seam over the ledger in `ledger.dir`, for `instance`/`character`.
    /// Nothing is opened until the first query.
    pub fn new(ledger: Ledger, instance: impl Into<String>, character: impl Into<String>) -> Self {
        LedgerSource {
            ledger,
            who: lrow::Who::new(instance, character),
        }
    }

    /// `Ledger::open(dir)` (creating and migrating it) plus the `Who`.
    pub fn open(
        dir: impl Into<std::path::PathBuf>,
        instance: impl Into<String>,
        character: impl Into<String>,
    ) -> Result<Self, String> {
        let ledger = Ledger::open(dir).map_err(|error| error.to_string())?;
        Ok(LedgerSource::new(ledger, instance, character))
    }

    /// The ledger this seam writes to.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// The instance + character every query is filtered by (what the ledger
    /// needs and [`crate::row::Who`] does not carry).
    pub fn who(&self) -> &lrow::Who {
        &self.who
    }

    /// The character name, for `report::*`'s header argument.
    pub fn character(&self) -> &str {
        &self.who.character
    }

    /// This seam's character as the reports' [`Who`].
    pub fn as_who(&self) -> Who {
        Who::new(self.who.character.clone())
    }

    /// One recovered `SELECT`: a reader, the query, and the error as text.
    fn query<T>(
        &self,
        run: impl FnOnce(&hydra_ledger::reader::Reader) -> hydra_ledger::Result<T>,
    ) -> Result<T, String> {
        let reader = self.ledger.reader().map_err(|error| error.to_string())?;
        run(&reader).map_err(|error| error.to_string())
    }

    /// One recovered `INSERT`: a fresh writer thread, the row, and a flush, so
    /// the row is on disk when this returns (see the module docs).
    fn record(&self, row: lrow::Row) -> Result<(), String> {
        let writer = self.ledger.writer().map_err(|error| error.to_string())?;
        if !writer.try_record(row) {
            return Err(hydra_ledger::CLOSED_TEXT.to_string());
        }
        writer.flush().map_err(|error| error.to_string())
    }
}

impl Source for LedgerSource {
    /// `Reader::last_pulses` returns **newest first**; the seam's contract is
    /// the newest `limit` rows **oldest first** (the reports index `last()` for
    /// the newest pulse), so the rows are reversed.
    fn last_pulses(&self, limit: usize) -> Result<Vec<Pulse>, String> {
        let mut rows = self.query(|reader| reader.last_pulses(&self.who, limit as u64))?;
        rows.reverse();
        Ok(rows.iter().map(pulse_out).collect())
    }

    /// `Reader::pulses` (`at >= from AND at < to`, oldest first) over the
    /// seam's closed interval.
    fn pulses_between(&self, from: i64, to: i64) -> Result<Vec<Pulse>, String> {
        let rows = self.query(|reader| reader.pulses(&self.who, from, to.saturating_add(1)))?;
        Ok(rows.iter().map(pulse_out).collect())
    }

    fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String> {
        let rows = self.query(|reader| reader.latest_snapshots(&self.who))?;
        Ok(rows.iter().map(snapshot_out).collect())
    }

    fn snapshots(&self, which: u8, from: i64, to: i64) -> Result<Vec<Snapshot>, String> {
        let rows = self.query(|reader| {
            reader.snapshots(&self.who, kind_in(which), from, to.saturating_add(1))
        })?;
        Ok(rows.iter().map(snapshot_out).collect())
    }

    /// `Writer::try_record`: `false` is the writer's closed arm, whose `Display`
    /// is [`hydra_ledger::CLOSED_TEXT`].
    fn try_record(&mut self, pulse: Pulse) -> Result<(), String> {
        self.record(lrow::Row::Pulse(pulse_in(&pulse, &self.who)))
    }

    fn prune_pulses(&mut self, drop_all: bool) -> Result<usize, String> {
        self.ledger
            .prune_pulses(&self.who, drop_all)
            .map_err(|error| error.to_string())
    }
}

/// [`crate::recorder::Sink`] over a real ledger, bound to one character.
///
/// `latest_snapshots` is `Reader::latest_snapshots(&who)` and `record` is a
/// snapshots `INSERT` through `Writer::try_record`.
#[derive(Clone, Debug)]
pub struct LedgerSink {
    ledger: Ledger,
    who: lrow::Who,
}

impl LedgerSink {
    /// A sink over the ledger in `ledger.dir` for `instance`/`character`.
    pub fn new(ledger: Ledger, instance: impl Into<String>, character: impl Into<String>) -> Self {
        LedgerSink {
            ledger,
            who: lrow::Who::new(instance, character),
        }
    }

    /// `Ledger::open(dir)` (creating and migrating it) plus the `Who`.
    pub fn open(
        dir: impl Into<std::path::PathBuf>,
        instance: impl Into<String>,
        character: impl Into<String>,
    ) -> Result<Self, String> {
        let ledger = Ledger::open(dir).map_err(|error| error.to_string())?;
        Ok(LedgerSink::new(ledger, instance, character))
    }

    /// The ledger this sink writes to.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// The instance + character `latest_snapshots` reads.
    pub fn who(&self) -> &lrow::Who {
        &self.who
    }
}

impl Sink for LedgerSink {
    fn latest_snapshots(&self) -> Result<Vec<Snapshot>, String> {
        let reader = self.ledger.reader().map_err(|error| error.to_string())?;
        let rows = reader
            .latest_snapshots(&self.who)
            .map_err(|error| error.to_string())?;
        Ok(rows.iter().map(snapshot_out).collect())
    }

    /// The row is filed under this sink's **instance** and the `who` the caller
    /// hands in (the character the recorder is running for), falling back to the
    /// bound character when that name is empty.
    fn record(&mut self, snap: &Snapshot, who: &Who) -> Result<(), String> {
        let character = if who.name.is_empty() {
            self.who.character.clone()
        } else {
            who.name.clone()
        };
        let filed = lrow::Who::new(self.who.instance.clone(), character);
        let row = snapshot_in(snap, &filed);
        let writer = self.ledger.writer().map_err(|error| error.to_string())?;
        if !writer.try_record(lrow::Row::Snapshot(row)) {
            return Err(hydra_ledger::CLOSED_TEXT.to_string());
        }
        writer.flush().map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Source as PulseSource;
    use std::path::PathBuf;

    /// A throwaway ledger directory, removed first in case a run left one.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hydra-exp-ledger-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// 2026-07-20 13:05:52 Eastern.
    const T: i64 = 1_784_567_152;

    fn pulse(at: i64, exp_gained: i64) -> Pulse {
        Pulse {
            at,
            rpa: Some("1.25".to_string()),
            until_next: Some(4_200),
            lumnis: Some(2),
            fashlonae: Some(2),
            exp: 1_000 + exp_gained,
            axp: 44,
            exp_gained,
            axp_gained: 0,
            bonus: 0,
            source: PulseSource::Normal,
        }
    }

    #[test]
    fn a_recorded_pulse_comes_back_through_the_ledger() {
        let dir = scratch("pulse");
        let mut src = LedgerSource::open(&dir, "prime", "Sindar").unwrap();
        assert_eq!(src.character(), "Sindar");
        assert_eq!(src.who().instance, "prime");

        let row = Pulse {
            bonus: 750,
            source: PulseSource::Brooch,
            ..pulse(T, 1_000)
        };
        src.try_record(row.clone()).unwrap();

        let read = src.last_pulses(1).unwrap();
        assert_eq!(read.len(), 1);
        let back = &read[0];
        assert_eq!(back.at, T);
        assert_eq!(back.rpa.as_deref(), Some("1.25"));
        assert_eq!(back.until_next, Some(4_200));
        assert_eq!(back.lumnis, Some(2));
        assert_eq!(back.fashlonae, Some(2));
        assert_eq!(back.exp, 2_000);
        assert_eq!(back.axp, 44);
        assert_eq!(back.exp_gained, 1_000);
        assert_eq!(back.axp_gained, 0);
        // `bonus` is the ledger's `instant` column.
        assert_eq!(back.bonus, 750);
        assert_eq!(back.source, PulseSource::Brooch);
        // And the reports read it: the bonus column is fashlonae, the amount is
        // the brooch bucket.
        assert_eq!(crate::report::bonuses(back), "lumnis 2x + rpa 1.25x + fashlonae");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_pulses_is_oldest_first_and_capped_at_the_limit() {
        let dir = scratch("order");
        let mut src = LedgerSource::open(&dir, "prime", "Sindar").unwrap();
        for step in 0..3 {
            src.try_record(pulse(T + step, 100)).unwrap();
        }
        assert_eq!(
            src.last_pulses(2)
                .unwrap()
                .iter()
                .map(|p| p.at)
                .collect::<Vec<_>>(),
            vec![T + 1, T + 2]
        );
        assert_eq!(src.last_pulses(10).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pulses_between_is_a_closed_window() {
        let dir = scratch("between");
        let mut src = LedgerSource::open(&dir, "prime", "Sindar").unwrap();
        for step in 0..5 {
            src.try_record(pulse(T + step, 100)).unwrap();
        }
        // Both ends included, oldest first.
        assert_eq!(
            src.pulses_between(T + 1, T + 3)
                .unwrap()
                .iter()
                .map(|p| p.at)
                .collect::<Vec<_>>(),
            vec![T + 1, T + 2, T + 3]
        );
        // The whole span, and the open-ended window `command::read` asks for.
        assert_eq!(src.pulses_between(T, T + 4).unwrap().len(), 5);
        assert_eq!(src.pulses_between(T, i64::MAX).unwrap().len(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn another_characters_rows_stay_out_of_the_seam() {
        let dir = scratch("who");
        let mut sindar = LedgerSource::open(&dir, "prime", "Sindar").unwrap();
        let mut other = LedgerSource::open(&dir, "prime", "Other").unwrap();
        sindar.try_record(pulse(T, 100)).unwrap();
        other.try_record(pulse(T, 900)).unwrap();
        assert_eq!(sindar.last_pulses(10).unwrap().len(), 1);
        assert_eq!(sindar.last_pulses(10).unwrap()[0].exp_gained, 100);
        // And a different instance does not see it either.
        let elsewhere = LedgerSource::new(Ledger::new(&dir), "test", "Sindar");
        assert_eq!(elsewhere.last_pulses(10).unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_drops_the_empty_rows_and_then_everything() {
        let dir = scratch("prune");
        let mut src = LedgerSource::open(&dir, "prime", "Sindar").unwrap();
        src.try_record(pulse(T, 0)).unwrap();
        src.try_record(pulse(T + 1, 500)).unwrap();
        assert_eq!(src.prune_pulses(false).unwrap(), 1);
        // The surviving row is still there: a prune is a committed delete, not
        // a stale writer that comes back.
        assert_eq!(
            src.last_pulses(10)
                .unwrap()
                .iter()
                .map(|p| p.exp_gained)
                .collect::<Vec<_>>(),
            vec![500]
        );
        assert_eq!(src.prune_pulses(true).unwrap(), 1);
        assert!(src.last_pulses(10).unwrap().is_empty());
        // Writing after a prune works and does not resurrect the old rows.
        src.try_record(pulse(T + 2, 250)).unwrap();
        assert_eq!(
            src.last_pulses(10)
                .unwrap()
                .iter()
                .map(|p| p.exp_gained)
                .collect::<Vec<_>>(),
            vec![250]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_answer_writes_through_the_ledger() {
        let dir = scratch("answer");
        let mut src = LedgerSource::open(&dir, "prime", "Sindar").unwrap();
        src.try_record(pulse(T - 60, 100)).unwrap();
        let who = src.as_who();
        assert_eq!(
            crate::command::answer(&mut src, &who, &crate::command::Parsed::AddGain(500), T),
            crate::command::Reply::Say("Added an experience gain of 500 by hand.".to_string())
        );
        let rows = src.last_pulses(10).unwrap();
        assert_eq!(rows.len(), 2);
        let manual = rows.last().unwrap();
        assert_eq!(manual.at, T);
        assert_eq!(manual.exp_gained, 500);
        assert_eq!(manual.source, PulseSource::Manual);
        // `exp clean` then drops the zero-gain rows the game wrote.
        assert_eq!(
            crate::command::answer(&mut src, &who, &crate::command::Parsed::Clean, T),
            crate::command::Reply::Say("Dropped 0 rows with no experience gained.".to_string())
        );
        // And the reports come back over the real rows.
        match crate::command::run(&src, &who, "exp last", T) {
            crate::command::Reply::Lines(lines) => {
                assert_eq!(lines[0], "Last 2 pulses:");
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_rows_keep_their_kind_and_name() {
        let dir = scratch("snapshots");
        let mut sink = LedgerSink::open(&dir, "prime", "Sindar").unwrap();
        let who = Who::new("Sindar");
        sink.record(
            &Snapshot {
                value: Some(1_234),
                label: Some("Voln Favor".to_string()),
                at: T,
                kind: 0,
                ..Snapshot::default()
            },
            &who,
        )
        .unwrap();
        for (weekly, total, at) in [(12_000, 240_000, T), (13_000, 241_000, T + 3_600)] {
            sink.record(
                &Snapshot {
                    weekly: Some(weekly),
                    total: Some(total),
                    label: Some("resource".to_string()),
                    at,
                    kind: 1,
                    ..Snapshot::default()
                },
                &who,
            )
            .unwrap();
        }
        // A row for another character must not show up.
        sink.record(
            &Snapshot {
                value: Some(9),
                at: T,
                kind: 0,
                ..Snapshot::default()
            },
            &Who::new("Other"),
        )
        .unwrap();

        // One latest row per (kind, name): the newer resource reading.
        let latest = sink.latest_snapshots().unwrap();
        assert_eq!(latest.len(), 2);
        assert_eq!(latest[0].kind, 0);
        assert_eq!(latest[0].label.as_deref(), Some("Voln Favor"));
        assert_eq!(latest[0].value, Some(1_234));
        assert_eq!(latest[1].kind, 1);
        assert_eq!(latest[1].weekly, Some(13_000));
        assert_eq!(latest[1].total, Some(241_000));

        // The `which`-filtered, windowed query `command::read` uses, and the
        // report over it.
        let src = LedgerSource::new(Ledger::new(&dir), "prime", "Sindar");
        let resource = src.snapshots(1, T, i64::MAX).unwrap();
        assert_eq!(resource.len(), 2);
        assert_eq!(resource[0].weekly, Some(12_000));
        assert_eq!(resource[1].weekly, Some(13_000));
        assert!(src.snapshots(2, T, i64::MAX).unwrap().is_empty());
        let lines = crate::report::snapshots(1, &latest, &resource);
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].contains("13000/50,000 weekly, 241000/200,000 total"),
            "as-of line = {}",
            lines[0]
        );
        // `report::snapshots` measures the day's movement from the rows'
        // `value` column, which a resource reading leaves empty — the recovered
        // report reads `value` for every kind, so the movement is 0 here.
        assert_eq!(lines[1], "  resource movement today: +0");

        // The `exp resource` command over the real ledger: `read` fetches the
        // latest snapshots and today's window from the two adapter methods.
        match crate::command::run(&src, &Who::new("Sindar"), "exp resource", T) {
            crate::command::Reply::Lines(lines) => {
                assert_eq!(lines.len(), 2);
                assert!(
                    lines[0].contains("13000/50,000 weekly"),
                    "as-of line = {}",
                    lines[0]
                );
            }
            other => panic!("{other:?}"),
        }
        // And a kind with no rows prints the recovered "no data" line.
        assert_eq!(
            crate::command::run(&src, &Who::new("Sindar"), "exp suffused", T),
            crate::command::Reply::Lines(vec![
                "No suffused data captured yet.".to_string()
            ])
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recorder_start_writes_only_the_named_snapshots_through_the_ledger() {
        let dir = scratch("recorder");
        let mut sink = LedgerSink::open(&dir, "prime", "Sindar").unwrap();
        // Seed the two labelled rows the recorder filters on (`Voln Favor:` and
        // `Suffused …`) plus one it must ignore.
        let who = Who::new("Sindar");
        for (label, value) in [
            ("Voln Favor:", 987i64),
            ("Suffused Essence", 4_321),
            ("Health", 100),
        ] {
            sink.record(
                &Snapshot {
                    value: Some(value),
                    label: Some(label.to_string()),
                    at: T,
                    ..Snapshot::default()
                },
                &who,
            )
            .unwrap();
        }
        // The recorder walks `latest_snapshots` and records what it accepts —
        // which, for this build, is what is already there. The count is the
        // filter's answer, and the point of the test is that the whole path is
        // a real ledger round trip.
        let written = crate::recorder::start(&mut sink, &who, T + 10).unwrap();
        assert_eq!(written, 2);
        let rows = sink.latest_snapshots().unwrap();
        assert!(rows
            .iter()
            .any(|row| row.label.as_deref() == Some("Suffused Essence")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_ledger_reports_the_open_error_as_text() {
        let dir = scratch("missing");
        // No `Ledger::open`, so nothing created the directory: the read-only
        // connection is `SQLITE_CANTOPEN` and its text is the reply.
        let src = LedgerSource::new(Ledger::new(&dir), "prime", "Sindar");
        let error = src.last_pulses(1).unwrap_err();
        assert!(
            error.contains("unable to open database file"),
            "error text = {error}"
        );
    }

    #[test]
    fn the_row_mapping_is_field_for_field() {
        let who = lrow::Who::new("prime", "Sindar");
        let mut ledger_row = lrow::Pulse {
            instance: "prime".to_string(),
            character: "Sindar".to_string(),
            at: T,
            rpa: Some("2x".to_string()),
            mind_text: Some("clear".to_string()),
            game_time: Some(13 * 60 + 5),
            fxp: Some(10),
            fxp_max: Some(20),
            until_next: Some(1),
            lumnis: Some(3),
            // A value word wider than `u8`: the tag stays, the number clamps.
            fashlonae: Some(300),
            mind_percent: Some(63),
            exp: 1,
            axp: 2,
            exp_gained: 3,
            axp_gained: 4,
            instant: 5,
            source: lrow::Source::Instant,
        };
        let out = pulse_out(&ledger_row);
        assert_eq!(out.fashlonae, Some(u8::MAX));
        assert_eq!(out.bonus, 5);
        assert_eq!(out.source, PulseSource::Instant);
        // And back: the fields this crate has no source for are `None`.
        let back = pulse_in(&out, &who);
        ledger_row.fashlonae = Some(u32::from(u8::MAX));
        ledger_row.mind_text = None;
        ledger_row.game_time = None;
        ledger_row.fxp = None;
        ledger_row.fxp_max = None;
        ledger_row.mind_percent = None;
        assert_eq!(back, ledger_row);

        let snap = crate::row::Snapshot {
            value: Some(1),
            weekly: Some(2),
            total: Some(3),
            label: Some("resource".to_string()),
            at: T,
            kind: 1,
        };
        let row = snapshot_in(&snap, &who);
        assert_eq!(row.name.as_deref(), Some("resource"));
        assert_eq!(row.kind, lrow::SnapshotKind::Resource);
        assert_eq!(row.instance, "prime");
        assert_eq!(snapshot_out(&row), snap);
    }
}
