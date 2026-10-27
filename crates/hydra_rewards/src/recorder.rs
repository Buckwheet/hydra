//! Recording live frames as login-reward rows.
//!
//! Recovered from `hydra_rewards::recorder::start` (`0x1014e1ea8`) and its
//! closure (the `drop_in_place<recorder::start::{{closure}}>` glue at
//! `0x1014ccf8c`, `0x1014d9768`, `0x1014dfe54`), plus
//! `hydra_rewards::recorder::run`'s closure (`0x1014d9464`).

use crate::report::Source;
use crate::row::{Stamp, Who};
use crate::watcher::{heard, Chunk, Heard, Said, Watcher};

/// Where the recorder writes what it hears.
///
/// **ESTABLISHED**: `recorder::start` is a `tokio` task (its `CoreStage`
/// drop glue is in the symbol table) that, for every reward the
/// [`Watcher`] reports, writes a row through `hydra_ledger::writer::Writer::
/// record` — the same writer `hydra_table::reward_claim::claim_one` calls at
/// `0x100072f6c`, whose `login_rewards` `INSERT` is the blob at
/// `0x103985e59`.
///
/// **Departure**: `hydra_ledger` is not reconstructed and nothing is spawned,
/// so the write is a trait and the "task" is a loop.
pub trait Sink {
    /// Keep one hearing.  The error is the ledger's own text.
    fn record(&mut self, heard: &Heard) -> Result<(), String>;
}

impl Sink for Vec<Heard> {
    fn record(&mut self, heard: &Heard) -> Result<(), String> {
        self.push(heard.clone());
        Ok(())
    }
}

/// What one recording run did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recorded {
    /// How many reward rows were written.
    pub written: usize,
    /// The first error, if the ledger refused a row.
    pub error: Option<String>,
}

/// Watch `chunks` and keep every reward they announce.
///
/// `recorder::start(...)` — `@0x1014e1ea8` (**partly**: the loop, the
/// grammar and the row are recovered; the tokio plumbing and the ledger write
/// are the [`Sink`] seam).
///
/// ```text
/// start(sink, &who, Stamp::now(), &[Chunk::line(
///     "You have earned the following reward: a brass ring")])
///     == Recorded { written: 1, error: None }
/// ```
///
/// Every chunk is handed to [`Watcher::hear`]; a [`Said::Reward`] becomes a
/// [`Heard`] through [`heard`] (the streak line's numbers and text travel with
/// it) and is written to the sink.  A sink that refuses a row stops the loop
/// and reports the error, which is what the original's `?` on the writer did.
pub fn start(
    sink: &mut dyn Sink,
    who: &Who,
    stamp: Stamp,
    chunks: &[Chunk<'_>],
) -> Recorded {
    let mut watcher = Watcher::new();
    let mut out = Recorded::default();
    for chunk in chunks {
        let said = watcher.hear(chunk);
        if let Said::Reward { .. } = said {
            if let Some(row) = heard(&said, who, stamp) {
                if let Err(e) = sink.record(&row) {
                    out.error = Some(e);
                    return out;
                }
                out.written += 1;
            }
        }
    }
    out
}

/// The rewards a source already holds, for a character, newest first.
///
/// **ESTABLISHED**: `report::answer` reads them through
/// `hydra_ledger::reader::Reader::rewards` with a limit of
/// [`crate::report::LAST`] (`0x1014cf9a4`); `recorder` needs the same list to
/// avoid writing a row it already has (the `INSERT … WHERE
/// login_rewards.reward = '' AND excluded.reward <> ''` at `0x103986049`
/// upgrades an empty reward rather than adding a row).
pub fn existing(source: &dyn Source, who: &Who) -> Result<Vec<Heard>, String> {
    source.rewards(who, crate::report::LAST)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who() -> Who {
        Who::new("GS4", "Aelith")
    }

    const STREAK: &str = "Thank you for logging into GemStone IV!  You have logged in \
                          4 consecutive days and crossed 9 login bridges.";

    #[test]
    fn a_reward_line_is_written_once() {
        let mut sink: Vec<Heard> = Vec::new();
        let stamp = Stamp::new(1_705_348_200, 1_705_348_200);
        let chunks = [
            Chunk::line(STREAK),
            Chunk::line("You have earned the following reward: a brass ring"),
            Chunk::line("You wave."),
        ];
        let done = start(&mut sink, &who(), stamp, &chunks);
        assert_eq!(done, Recorded { written: 1, error: None });
        assert_eq!(sink.len(), 1);
        assert_eq!(sink[0].reward, "a brass ring");
        assert_eq!(sink[0].day, Some(4));
        assert_eq!(sink[0].bridges, Some(9));
        assert_eq!(sink[0].day_text, "2024-01-15");
        assert_eq!(sink[0].said, STREAK);
    }

    #[test]
    fn a_line_split_over_frames_is_still_one_row() {
        let mut sink: Vec<Heard> = Vec::new();
        let chunks = [
            Chunk::more("You have earned the following "),
            Chunk::more("reward: a tin cup"),
            Chunk::new(b"", true),
        ];
        let done = start(&mut sink, &who(), Stamp::default(), &chunks);
        assert_eq!(done.written, 1);
        assert_eq!(sink[0].reward, "a tin cup");
    }

    #[test]
    fn a_sink_that_refuses_stops_the_run() {
        struct Deaf;
        impl Sink for Deaf {
            fn record(&mut self, _heard: &Heard) -> Result<(), String> {
                Err("no such table: login_rewards".to_string())
            }
        }
        let mut sink = Deaf;
        let chunks = [Chunk::line(
            "You have earned the following reward: a brass ring",
        )];
        let done = start(&mut sink, &who(), Stamp::default(), &chunks);
        assert_eq!(done.written, 0);
        assert_eq!(done.error.as_deref(), Some("no such table: login_rewards"));
    }

    #[test]
    fn nothing_announced_writes_nothing() {
        let mut sink: Vec<Heard> = Vec::new();
        let done = start(&mut sink, &who(), Stamp::default(), &[Chunk::line("You wave.")]);
        assert_eq!(done, Recorded::default());
        assert!(sink.is_empty());
    }
}
