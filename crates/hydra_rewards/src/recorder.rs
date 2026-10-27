//! Recording live frames as login-reward rows.
//!
//! Recovered from `hydra_rewards::recorder::start` (`0x1014e1ea8`) and its
//! closure (the `drop_in_place<recorder::start::{{closure}}>` glue at
//! `0x1014ccf8c`, `0x1014d9768`, `0x1014dfe54`), plus
//! `hydra_rewards::recorder::run`'s closure (`0x1014d9464`).
//!
//! **Phase 1 reconnection**: the original `start` is a `tokio` task — its
//! `CoreStage`/`Harness` drop glue is in the symbol table next to the
//! `start`/`run` closures — driven by the session's frame stream, writing each
//! reward through `hydra_ledger::writer::Writer::record`.  `tokio` is back:
//! [`spawn`] starts the same loop as a task fed by a
//! [`tokio::sync::mpsc`] channel of owned frames ([`OwnedChunk`]), and [`run`]
//! is the task body, so a session relay can hand it text while the reader is
//! elsewhere.  The synchronous entry point [`start`] stays for the callers
//! that already have their frames in hand — this crate's own tests, and any
//! sibling that wants a pure function — and both entry points share one state
//! machine (`Recorder`), so they cannot drift apart.
//!
//! **KEPT (seam)**: the write itself is still [`Sink`], not `rusqlite`.  See
//! the trait's note.

use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::report::Source;
use crate::row::{Stamp, Who};
use crate::watcher::{heard, Chunk, Heard, Said, Watcher};

/// How many frames a spawned recorder will hold while it works.
///
/// **UNKNOWN**: the capacity of the original's channel.  The task is driven by
/// a session stream, so the number is a backpressure choice, not a recovered
/// fact; 64 frames is a few seconds of game text.
pub const FRAMES: usize = 64;

/// Where the recorder writes what it hears.
///
/// **ESTABLISHED**: `recorder::start` is a `tokio` task that, for every reward
/// the [`Watcher`] reports, writes a row through
/// `hydra_ledger::writer::Writer::record` — the same writer
/// `hydra_table::reward_claim::claim_one` calls at `0x100072f6c`, whose
/// `login_rewards` `INSERT` is the blob at `0x103985e59`.
///
/// **KEPT (seam)**: `hydra_ledger` is being rewritten around `rusqlite` in
/// parallel with this crate, and its `Writer::record` takes a
/// `hydra_ledger::row::Row`, not a [`Heard`].  Adopting it now would couple
/// two in-flight APIs and could not preserve the public signature the
/// siblings use; the four reads and this write stay behind
/// [`Source`]/[`Sink`], with `String` as the error.  A `rusqlite` adapter
/// belongs here once the ledger's rows are settled.
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

/// A sink two tasks can share: the recorder writes, the owner reads.
impl Sink for Arc<Mutex<Vec<Heard>>> {
    fn record(&mut self, heard: &Heard) -> Result<(), String> {
        match self.lock() {
            Ok(mut rows) => {
                rows.push(heard.clone());
                Ok(())
            }
            Err(_) => Err("the recorder's sink was poisoned".to_string()),
        }
    }
}

/// A sink that hands each row to another task — the shape the original's
/// ledger writer had, one hop away from `rusqlite`.
impl Sink for mpsc::UnboundedSender<Heard> {
    fn record(&mut self, heard: &Heard) -> Result<(), String> {
        self.send(heard.clone())
            .map_err(|_| "the ledger task is gone".to_string())
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

/// The loop both entry points run.
///
/// **ESTABLISHED**: the recovered `start`'s body: a [`Watcher`], the row
/// [`heard`] makes from a [`Said::Reward`], and `?` on the writer — a refusal
/// ends the loop.
struct Recorder {
    who: Who,
    stamp: Stamp,
    watcher: Watcher,
    out: Recorded,
}

impl Recorder {
    fn new(who: Who, stamp: Stamp) -> Recorder {
        Recorder {
            who,
            stamp,
            watcher: Watcher::new(),
            out: Recorded::default(),
        }
    }

    /// Feed one chunk.  Answers `false` once a row was refused, which stops
    /// the loop.
    fn hear(&mut self, chunk: &Chunk<'_>, sink: &mut dyn Sink) -> bool {
        let said = self.watcher.hear(chunk);
        if let Said::Reward { .. } = said {
            if let Some(row) = heard(&said, &self.who, self.stamp) {
                if let Err(e) = sink.record(&row) {
                    self.out.error = Some(e);
                    return false;
                }
                self.out.written += 1;
            }
        }
        true
    }
}

/// Watch `chunks` and keep every reward they announce.
///
/// `recorder::start(...)` — `@0x1014e1ea8` (**full** for the loop, the grammar
/// and the row; the `tokio` plumbing is [`spawn`], the ledger write is the
/// [`Sink`] seam).
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
///
/// This is the synchronous spelling of the recovered task: same loop, frames
/// already in hand.  [`spawn`] is the `tokio` one.
pub fn start(
    sink: &mut dyn Sink,
    who: &Who,
    stamp: Stamp,
    chunks: &[Chunk<'_>],
) -> Recorded {
    let mut recorder = Recorder::new(who.clone(), stamp);
    for chunk in chunks {
        if !recorder.hear(chunk, sink) {
            break;
        }
    }
    recorder.out
}

// ---------------------------------------------------------------------------
// The `tokio` task
// ---------------------------------------------------------------------------

/// A chunk the spawned recorder owns.
///
/// [`Chunk`] borrows the frame's bytes, so it cannot cross into a task; this is
/// the same pair of facts (`bytes`, `ends_line`) with the bytes owned.
///
/// **ESTABLISHED**: those two facts are the ones `Watcher::hear` reads out of
/// the original's `hydra_protocol::frame::vocabulary::Frame` — the byte slice
/// at `+0x08`/`+0x10` and the "line ends here" byte at `+0x110`
/// (`0x1014effb0`, `0x1014effbc`).  The `Frame` of the local `hydra_protocol`
/// crate is a simplified model of that enum and does not carry the flag (its
/// own `NOTES.md` says so), so this crate keeps its own two-field value rather
/// than lose it; a sibling that owns the raw stream builds one of these with
/// [`OwnedChunk::new`], flag and all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedChunk {
    /// The bytes this frame adds to the line.
    pub bytes: Vec<u8>,
    /// Whether the line ends with this frame.
    pub ends_line: bool,
}

impl OwnedChunk {
    pub fn new(bytes: impl Into<Vec<u8>>, ends_line: bool) -> OwnedChunk {
        OwnedChunk {
            bytes: bytes.into(),
            ends_line,
        }
    }

    /// One whole line.
    pub fn line(text: impl Into<Vec<u8>>) -> OwnedChunk {
        OwnedChunk::new(text, true)
    }

    /// More text for the line being built.
    pub fn more(text: impl Into<Vec<u8>>) -> OwnedChunk {
        OwnedChunk::new(text, false)
    }

    /// The borrowed chunk, for the shared loop.
    pub fn as_chunk(&self) -> Chunk<'_> {
        Chunk::new(&self.bytes, self.ends_line)
    }
}

impl<'a> From<&Chunk<'a>> for OwnedChunk {
    fn from(chunk: &Chunk<'a>) -> OwnedChunk {
        OwnedChunk::new(chunk.bytes.to_vec(), chunk.ends_line)
    }
}

/// The sending half of a spawned recorder's frame stream.
pub type Frames = mpsc::Sender<OwnedChunk>;
/// The receiving half, owned by the task.
pub type FrameStream = mpsc::Receiver<OwnedChunk>;

/// A spawned recorder: the channel to feed and the task to await.
pub struct Spawned {
    /// Send frames here.  Dropping it ends the stream and the task.
    pub frames: Frames,
    /// The task's answer: what it wrote, or the ledger's first error.
    pub task: JoinHandle<Recorded>,
}

/// Spawn the recorder task — the original's `tokio::spawn` of
/// `recorder::start`.
///
/// The returned [`Spawned::frames`] is the stream's head: every chunk sent is
/// handed to the [`Watcher`] in order, and each reward becomes a row in `sink`.
/// Dropping the sender ends the task, which answers [`Spawned::task`] with
/// what it wrote.  A sink that refuses a row ends the task early and closes
/// the channel, so the next `send` fails — the same stop the synchronous
/// [`start`] reports.
///
/// Must be called from within a `tokio` runtime.
///
/// **UNKNOWN**: the capacity ([`FRAMES`]) and whether the original keyed its
/// rows by character before spawning; nothing in the recovered bodies says.
pub fn spawn<S: Sink + Send + 'static>(sink: S, who: Who, stamp: Stamp) -> Spawned {
    let (frames, stream) = mpsc::channel(FRAMES);
    let task = tokio::spawn(run(sink, who, stamp, stream));
    Spawned { frames, task }
}

/// The task body: keep every reward `frames` announces.
///
/// **ESTABLISHED**: `recorder::run`'s closure (`0x1014d9464`).  This is
/// [`start`] with the slice replaced by a channel: `recv().await` until the
/// stream ends, one `Recorder` for the whole run so a line split over frames
/// still produces one row.
pub async fn run<S: Sink>(
    mut sink: S,
    who: Who,
    stamp: Stamp,
    mut frames: FrameStream,
) -> Recorded {
    let mut recorder = Recorder::new(who, stamp);
    while let Some(frame) = frames.recv().await {
        if !recorder.hear(&frame.as_chunk(), &mut sink) {
            break;
        }
    }
    recorder.out
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

    // --- the `tokio` task (Phase 1) ---------------------------------------

    #[tokio::test]
    async fn the_spawned_task_records_what_the_stream_says() {
        let rows: Arc<Mutex<Vec<Heard>>> = Arc::new(Mutex::new(Vec::new()));
        let mut spawned = spawn(
            Arc::clone(&rows),
            who(),
            Stamp::new(1_705_348_200, 1_705_348_200),
        );
        // a streak line, then a reward split across two frames
        spawned.frames.send(OwnedChunk::line(STREAK)).await.unwrap();
        spawned
            .frames
            .send(OwnedChunk::more("You have earned the following "))
            .await
            .unwrap();
        spawned
            .frames
            .send(OwnedChunk::line("reward: a brass ring"))
            .await
            .unwrap();
        // dropping the stream ends the task, as the session's stream ending did
        drop(spawned.frames);
        assert_eq!(
            spawned.task.await.unwrap(),
            Recorded {
                written: 1,
                error: None
            }
        );
        let rows = rows.lock().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].reward, "a brass ring");
        assert_eq!(rows[0].day, Some(4));
        assert_eq!(rows[0].said, STREAK);
    }

    #[tokio::test]
    async fn the_task_hands_each_row_to_the_ledger_channel() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let spawned = spawn(tx, who(), Stamp::default());
        spawned
            .frames
            .send(OwnedChunk::line(
                "You have earned the following reward: a tin cup",
            ))
            .await
            .unwrap();
        spawned
            .frames
            .send(OwnedChunk::line("You have earned the following reward: a blue orb"))
            .await
            .unwrap();
        drop(spawned.frames);
        assert_eq!(spawned.task.await.unwrap().written, 2);
        // the sink's channel still holds both rows
        let mut rewards = Vec::new();
        while let Ok(row) = rx.try_recv() {
            rewards.push(row.reward);
        }
        assert_eq!(rewards, ["a tin cup", "a blue orb"]);
    }

    #[tokio::test]
    async fn a_refusal_ends_the_task_and_the_stream() {
        struct Deaf;
        impl Sink for Deaf {
            fn record(&mut self, _heard: &Heard) -> Result<(), String> {
                Err("no such table: login_rewards".to_string())
            }
        }
        let mut spawned = spawn(Deaf, who(), Stamp::default());
        spawned
            .frames
            .send(OwnedChunk::line(
                "You have earned the following reward: a brass ring",
            ))
            .await
            .unwrap();
        let done = spawned.task.await.unwrap();
        assert_eq!(done.written, 0);
        assert_eq!(done.error.as_deref(), Some("no such table: login_rewards"));
        // the task is gone, so the stream is closed
        assert!(spawned
            .frames
            .send(OwnedChunk::line("You wave."))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn a_frame_carries_the_two_recovered_facts() {
        let chunk = Chunk::new(b"a blue orb", true);
        let owned = OwnedChunk::from(&chunk);
        assert_eq!(owned.bytes, b"a blue orb".to_vec());
        assert!(owned.ends_line);
        assert_eq!(owned.as_chunk(), chunk);
        assert!(!OwnedChunk::more("more").ends_line);
    }
}
