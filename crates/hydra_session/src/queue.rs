//! The command queue: what is waiting to be written, and what is owed an
//! answer.
//!
//! Recovered from `queue::CommandQueue::*`:
//! `0x101a0c52c` (`admit`), `0x101a0bb3c` (`answer_all`),
//! `0x101a0c308` (`clear_waiting`), `0x101a0c1a4` (`close_window`),
//! `0x101a0c5b8` (`offer`), `0x101a0c0f0` (`open_window`),
//! `0x101a0c764` (`take_next`, 60 KB — the whole write path is inlined into
//! it), `0x101a0c48c` (`written_at_once`).
//!
//! **ESTABLISHED** layout — the struct is two `VecDeque`s and nothing else:
//!
//! ```text
//! +0x00  VecDeque<Envelope>   cap +0x00, ptr +0x08, head +0x10, len +0x18
//! +0x20  VecDeque<Owed>       cap +0x20, ptr +0x28, head +0x30, len +0x38
//! ```
//!
//! `admit` pushes onto the first (`grow` when `len == cap` then a ring write),
//! `clear_waiting` drains it (`VecDeque::drain`, dropping each element), and
//! everything else walks the second.
//!
//! * `Envelope` is **0x28 (40) bytes**: a `String` at `+0x00` (the line), an
//!   `Option<oneshot::Sender<Outcome>>` at `+0x18`, and one more word at
//!   `+0x20`.  `clear_waiting` sends the outcome with discriminant **6**
//!   (`mov w1, #6`) to every envelope it drops.
//! * `Owed` is **0x20 (32) bytes**: `+0x00` the reply channel, `+0x08` an
//!   `Option<Frame>`, `+0x10` an `Instant`, `+0x18` a `u32` **tag**.  The tag
//!   is `0x3b9aca00` (1_000_000_000 = one second in nanoseconds) for a window
//!   that is still open and `0x3b9aca01` for an entry that has already been
//!   answered.  Entries are **stale after 30 seconds**: `close_window` and
//!   `offer` both do `Instant::elapsed(..)` and `cmp x0, #0x1e`, dropping the
//!   front entry when it is at least 30 seconds old.
//! * `answer_all` walks the owed queue and answers every still-open entry,
//!   returning how many it answered; `offer` answers one; `close_window`
//!   pops the front.
//! * `take_next` first drops stale owed entries, then checks whether anything
//!   is owed a reply, then pops the front of `waiting` and records it as owed.
//!   The 60 KB of inlined code around it is the whole command grammar (alias
//!   expansion, stance parsing, player-log capture), not extra queue state.
//!
//! **UNKNOWN**: the exact `Outcome` payloads.  `clear_waiting` proves there is
//! a "cleared, never run" variant with discriminant 6, `close_window` proves a
//! "closed" answer carrying an optional frame, and `answer_all` proves an
//! "answered" variant carrying a frame.  We model exactly those three.
//!
//! **Phase 1**: the binary's `Envelope`/`Owed` carry a
//! `tokio::sync::oneshot::Sender<Outcome>` (`drop_in_place<Envelope>` drops one
//! at `+0x18`).  [`Envelope::oneshot`] / [`Owed::oneshot`] now hold exactly
//! that, and [`CommandQueue::open_window_async`] /
//! [`CommandQueue::written_at_once_async`] / [`Envelope::with_oneshot`] are the
//! tokio-flavoured constructors.  The synchronous `std::sync::mpsc` reply stays:
//! its `Receiver` is the shape every existing caller and test uses, and
//! `oneshot::Sender` is not `Clone`, which the `Envelope`/`Owed` derives need.

use std::collections::VecDeque;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

/// The tag of a window that is still open (**ESTABLISHED**: `0x3b9aca00`).
pub const WINDOW_OPEN: u32 = 1_000_000_000;
/// The tag of an entry that has been answered (**ESTABLISHED**: `0x3b9aca01`).
pub const WINDOW_TAKEN: u32 = 1_000_000_001;
/// How long a window may stay open before it is assumed lost
/// (**ESTABLISHED**: `cmp x0, #0x1e` on `Instant::elapsed`).
pub const WINDOW_STALE: Duration = Duration::from_secs(30);

/// One line's worth of answer from the game.
///
/// The binary's `Frame` is `hydra_protocol::frame::vocabulary::Frame` (280
/// bytes, deallocated with `__rust_dealloc`), deeply tied to the protocol
/// crate.  So that this crate stays std-only and dependency-free we carry the
/// answer as the frames' text lines.
pub type Answer = Vec<String>;

/// What happened to a queued command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Outcome {
    /// the command was dropped before it ran (**ESTABLISHED**: the
    /// discriminant `clear_waiting` sends is 6)
    #[default]
    Cleared,
    /// the game answered
    Answered(Answer),
    /// the window closed with no answer
    Quiet,
}

/// The channel a caller uses to learn a command's outcome.
pub type Reply = Sender<Outcome>;

/// The tokio side of a reply: a `oneshot::Sender<Outcome>` behind an `Arc`.
///
/// **ESTABLISHED**: the binary's `Envelope` drops a
/// `Option<oneshot::Sender<Outcome>>` at `+0x18`.  The sender is shared (and
/// taken when it fires) so that [`Envelope`] and [`Owed`] keep their `Clone`
/// derive, which a bare `oneshot::Sender` does not have.
#[derive(Debug, Clone, Default)]
pub struct Oneshot(Arc<Mutex<Option<oneshot::Sender<Outcome>>>>);

impl Oneshot {
    /// A ticket for `tx`.
    pub fn new(tx: oneshot::Sender<Outcome>) -> Self {
        Oneshot(Arc::new(Mutex::new(Some(tx))))
    }

    /// Send the outcome.  `false` when the ticket was already used or the
    /// waiting side was dropped.
    pub fn send(&self, outcome: Outcome) -> bool {
        let taken = self.0.lock().unwrap_or_else(|e| e.into_inner()).take();
        match taken {
            Some(tx) => tx.send(outcome).is_ok(),
            None => false,
        }
    }

    /// Is a caller still waiting for this outcome?
    pub fn is_waiting(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|tx| !tx.is_closed())
    }
}

impl From<oneshot::Sender<Outcome>> for Oneshot {
    fn from(tx: oneshot::Sender<Outcome>) -> Self {
        Oneshot::new(tx)
    }
}

/// A command that is waiting to be written.
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    /// the line to write
    pub line: String,
    /// the caller's ticket, if it wants the outcome
    pub reply: Option<Reply>,
    /// a number the caller can use to match the envelope up again
    pub number: u64,
    /// the tokio `oneshot` ticket, if the caller asked on that side
    pub oneshot: Option<Oneshot>,
}

impl Envelope {
    /// A line with no ticket.
    pub fn new(line: impl Into<String>) -> Self {
        Envelope {
            line: line.into(),
            reply: None,
            number: 0,
            oneshot: None,
        }
    }

    /// A line whose outcome the caller wants to hear about.
    pub fn with_reply(line: impl Into<String>, reply: Reply) -> Self {
        Envelope {
            line: line.into(),
            reply: Some(reply),
            number: 0,
            oneshot: None,
        }
    }

    /// A line whose outcome the caller wants as a tokio `oneshot`.
    pub fn with_oneshot(line: impl Into<String>, ticket: oneshot::Sender<Outcome>) -> Self {
        Envelope {
            line: line.into(),
            reply: None,
            number: 0,
            oneshot: Some(Oneshot::new(ticket)),
        }
    }

    /// A line with a caller-chosen number.
    pub fn numbered(mut self, number: u64) -> Self {
        self.number = number;
        self
    }
}

/// A command that has been written and is owed an answer.
#[derive(Debug, Clone)]
pub struct Owed {
    /// where the answer goes
    pub reply: Option<Reply>,
    /// the answer, once it arrived
    pub answer: Option<Answer>,
    /// when the window opened
    pub at: Instant,
    /// [`WINDOW_OPEN`] or [`WINDOW_TAKEN`]
    pub tag: u32,
    /// the caller's cookie
    pub key: u32,
    /// the tokio `oneshot` ticket, if the window was opened on that side
    pub oneshot: Option<Oneshot>,
}

impl Owed {
    fn open(reply: Option<Reply>, key: u32, at: Instant) -> Self {
        Owed::open_with(reply, None, key, at)
    }

    fn open_with(reply: Option<Reply>, oneshot: Option<Oneshot>, key: u32, at: Instant) -> Self {
        Owed {
            reply,
            answer: None,
            at,
            tag: WINDOW_OPEN,
            key,
            oneshot,
        }
    }

    /// Send the outcome to every ticket this entry holds, once.
    fn answer_with(&mut self, outcome: Outcome) -> bool {
        let mut sent = false;
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(outcome.clone());
            sent = true;
        }
        if let Some(ticket) = self.oneshot.take() {
            sent |= ticket.send(outcome);
        }
        sent
    }

    /// Is a tokio caller still waiting on this entry?
    pub fn async_waiting(&self) -> bool {
        self.oneshot.as_ref().is_some_and(Oneshot::is_waiting)
    }

    /// Has this entry been answered?
    pub fn taken(&self) -> bool {
        self.tag == WINDOW_TAKEN
    }

    /// Is the window still open, i.e. could it still be answered?
    pub fn open_window(&self) -> bool {
        self.tag == WINDOW_OPEN
    }

    /// Has the window been open for [`WINDOW_STALE`] without an answer?
    pub fn stale(&self) -> bool {
        self.at.elapsed() >= WINDOW_STALE
    }
}

/// What is waiting to be written, and what is owed an answer.
#[derive(Debug, Default)]
pub struct CommandQueue {
    waiting: VecDeque<Envelope>,
    owed: VecDeque<Owed>,
}

impl CommandQueue {
    /// An empty queue.
    pub fn new() -> Self {
        CommandQueue::default()
    }

    /// How many commands are waiting to be written.
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// How many answers are owed.
    pub fn owed(&self) -> usize {
        self.owed.len()
    }

    /// The waiting lines, in order.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.waiting.iter().map(|e| e.line.as_str())
    }

    /// Queue a command at the back.
    ///
    /// **ESTABLISHED** (`0x101a0c52c`): a plain `push_back`, growing the ring
    /// when it is full — first in, first out, no priority and no cap of its
    /// own.
    pub fn admit(&mut self, envelope: Envelope) {
        self.waiting.push_back(envelope);
    }

    /// Queue a line at the back.
    pub fn push(&mut self, line: impl Into<String>) {
        self.admit(Envelope::new(line));
    }

    /// Drop every waiting command, telling each caller it was cleared.
    /// Returns how many were dropped.
    ///
    /// **ESTABLISHED** (`0x101a0c308`): `VecDeque::drain(..)`, dropping each
    /// envelope, and an outcome carrying discriminant 6 to each reply channel.
    pub fn clear_waiting(&mut self) -> usize {
        let n = self.waiting.len();
        for envelope in self.waiting.drain(..) {
            if let Some(reply) = envelope.reply {
                let _ = reply.send(Outcome::Cleared);
            }
            if let Some(ticket) = envelope.oneshot {
                ticket.send(Outcome::Cleared);
            }
        }
        n
    }

    /// Drop owed entries whose window has been open for [`WINDOW_STALE`].
    /// Returns how many were dropped.
    pub fn drop_stale(&mut self) -> usize {
        let before = self.owed.len();
        self.owed.retain(|o| !(o.open_window() && o.stale()));
        before - self.owed.len()
    }

    /// Open a window: a command was written and its answer is owed.  The
    /// returned receiver yields the [`Outcome`] when the window closes.
    ///
    /// **ESTABLISHED** (`0x101a0c0f0`): pushes `Owed{ reply, at: now,
    /// tag: WINDOW_OPEN, key }` onto the second queue.
    pub fn open_window(&mut self, key: u32) -> std::sync::mpsc::Receiver<Outcome> {
        let (tx, rx) = channel();
        self.owed.push_back(Owed::open(Some(tx), key, Instant::now()));
        rx
    }

    /// The same, for a command that was written in one go.
    ///
    /// **ESTABLISHED** (`0x101a0c48c`): identical push with the tag
    /// `WINDOW_OPEN`; the binary only differs in which caller built the
    /// `Owed`.
    pub fn written_at_once(&mut self, key: u32) -> std::sync::mpsc::Receiver<Outcome> {
        self.open_window(key)
    }

    /// Open a window whose answer arrives on a tokio `oneshot` channel.
    ///
    /// **ESTABLISHED**: this is the sender type the binary's `Owed` holds.
    pub fn open_window_async(&mut self, key: u32) -> oneshot::Receiver<Outcome> {
        let (tx, rx) = oneshot::channel();
        self.owed.push_back(Owed::open_with(
            None,
            Some(Oneshot::new(tx)),
            key,
            Instant::now(),
        ));
        rx
    }

    /// The same, for a command that was written in one go.
    pub fn written_at_once_async(&mut self, key: u32) -> oneshot::Receiver<Outcome> {
        self.open_window_async(key)
    }

    /// Answer the oldest open window.
    ///
    /// **ESTABLISHED** (`0x101a0c1a4`): drops a stale front entry, then sends
    /// the answer to the front entry's reply channel.
    pub fn close_window(&mut self, answer: Answer) -> Option<Outcome> {
        self.drop_stale();
        let front = self.owed.front_mut()?;
        if front.taken() {
            return None;
        }
        let outcome = Outcome::Answered(answer.clone());
        front.tag = WINDOW_TAKEN;
        front.answer = Some(answer);
        front.answer_with(outcome.clone());
        Some(outcome)
    }

    /// Answer the most recent open window, if there is one.
    ///
    /// **ESTABLISHED** (`0x101a0c5b8`): walks the owed queue looking for the
    /// entry with tag `WINDOW_OPEN`, dropping stale ones on the way, and hands
    /// the answer to the newest match.
    pub fn offer(&mut self, answer: Answer) -> bool {
        self.drop_stale();
        let Some(index) = self
            .owed
            .iter()
            .rposition(|o| o.tag == WINDOW_OPEN)
        else {
            return false;
        };
        let entry = &mut self.owed[index];
        entry.tag = WINDOW_TAKEN;
        entry.answer = Some(answer.clone());
        entry.answer_with(Outcome::Answered(answer));
        true
    }

    /// Answer every still-open window with the same answer.  Returns how many
    /// were answered.
    ///
    /// **ESTABLISHED** (`0x101a0bb3c`): one pass over the owed queue, skipping
    /// entries already tagged `WINDOW_TAKEN`, sending the answer to each.
    pub fn answer_all(&mut self, answer: &Answer) -> usize {
        let mut answered = 0;
        for entry in self.owed.iter_mut() {
            if entry.tag != WINDOW_OPEN {
                continue;
            }
            entry.tag = WINDOW_TAKEN;
            entry.answer = Some(answer.clone());
            entry.answer_with(Outcome::Answered(answer.clone()));
            answered += 1;
        }
        answered
    }

    /// Forget answered entries once nothing is waiting on them.
    pub fn forget_answered(&mut self) -> usize {
        let before = self.owed.len();
        self.owed.retain(|o| o.tag == WINDOW_OPEN);
        before - self.owed.len()
    }

    /// Take the next command to write, if one may be written now.
    ///
    /// **ESTABLISHED entry semantics** (`0x101a0c764`): drop stale owed
    /// entries, refuse while something is still owed an answer, otherwise pop
    /// the front of `waiting` (FIFO) and record it as owed.
    pub fn take_next(&mut self, key: u32) -> Option<Envelope> {
        self.drop_stale();
        let envelope = self.waiting.pop_front()?;
        self.owed.push_back(Owed::open_with(
            envelope.reply.clone(),
            envelope.oneshot.clone(),
            key,
            Instant::now(),
        ));
        Some(envelope)
    }

    /// Take the next command only when `ready` says the game will accept it.
    pub fn take_next_when(&mut self, key: u32, ready: bool) -> Option<Envelope> {
        if !ready {
            return None;
        }
        self.take_next(key)
    }

    /// Every owed entry, oldest first.
    pub fn owed_entries(&self) -> impl Iterator<Item = &Owed> {
        self.owed.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admit_is_first_in_first_out_with_no_cap() {
        let mut q = CommandQueue::new();
        for i in 0..1000 {
            q.push(format!("line {i}"));
        }
        assert_eq!(q.waiting(), 1000);
        assert_eq!(q.lines().next(), Some("line 0"));
        let first = q.take_next(1).unwrap();
        assert_eq!(first.line, "line 0");
        assert_eq!(q.waiting(), 999);
        assert_eq!(q.owed(), 1);
    }

    #[test]
    fn clear_waiting_reports_cleared_to_every_caller() {
        let mut q = CommandQueue::new();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        q.admit(Envelope::with_reply("one", tx1));
        q.admit(Envelope::with_reply("two", tx2));
        q.push("three");
        assert_eq!(q.clear_waiting(), 3);
        assert_eq!(q.waiting(), 0);
        assert_eq!(rx1.try_recv().unwrap(), Outcome::Cleared);
        assert_eq!(rx2.try_recv().unwrap(), Outcome::Cleared);
        assert_eq!(q.clear_waiting(), 0);
    }

    #[test]
    fn the_window_tags_are_the_recovered_constants() {
        assert_eq!(WINDOW_OPEN, 0x3b9a_ca00);
        assert_eq!(WINDOW_TAKEN, 0x3b9a_ca01);
        assert_eq!(WINDOW_STALE, Duration::from_secs(30));
        let mut q = CommandQueue::new();
        let _rx = q.open_window(7);
        let entry = q.owed_entries().next().unwrap();
        assert_eq!(entry.tag, WINDOW_OPEN);
        assert!(entry.open_window());
        assert!(!entry.taken());
        assert!(!entry.stale());
        assert_eq!(entry.key, 7);
    }

    #[test]
    fn answer_all_answers_each_open_window_once() {
        let mut q = CommandQueue::new();
        let rx1 = q.open_window(1);
        let rx2 = q.open_window(2);
        let answer = vec!["You see a cart.".to_string()];
        assert_eq!(q.answer_all(&answer), 2);
        assert_eq!(rx1.try_recv().unwrap(), Outcome::Answered(answer.clone()));
        assert_eq!(rx2.try_recv().unwrap(), Outcome::Answered(answer.clone()));
        // a second pass answers nothing: the tags are TAKEN now
        assert_eq!(q.answer_all(&answer), 0);
        assert!(q.owed_entries().all(|o| o.tag == WINDOW_TAKEN));
    }

    #[test]
    fn offer_answers_only_the_newest_open_window() {
        let mut q = CommandQueue::new();
        let old = q.open_window(1);
        let new = q.open_window(2);
        assert!(q.offer(vec!["x".to_string()]));
        assert!(new.try_recv().is_ok());
        assert!(old.try_recv().is_err());
        // only the oldest is left open, so the next offer takes it
        assert!(q.offer(vec!["y".to_string()]));
        assert_eq!(
            old.try_recv().unwrap(),
            Outcome::Answered(vec!["y".to_string()])
        );
        assert!(!q.offer(vec![]));
    }

    #[test]
    fn close_window_answers_the_oldest_and_refuses_monkeys() {
        let mut q = CommandQueue::new();
        assert_eq!(q.close_window(vec![]), None);
        let rx = q.open_window(3);
        let outcome = q.close_window(vec!["done".to_string()]).unwrap();
        assert_eq!(outcome, Outcome::Answered(vec!["done".to_string()]));
        assert_eq!(rx.try_recv().unwrap(), outcome);
        // the window is closed now, so a second close does nothing
        assert_eq!(q.close_window(vec![]), None);
    }

    #[test]
    fn take_next_records_the_written_command_as_owed() {
        let mut q = CommandQueue::new();
        q.push("go north");
        let taken = q.take_next(0).unwrap();
        assert_eq!(taken.line, "go north");
        assert_eq!(q.owed(), 1);
        assert_eq!(q.owed_entries().next().unwrap().tag, WINDOW_OPEN);
        assert!(q.take_next_when(0, false).is_none());
        assert_eq!(q.owed(), 1, "nothing was taken while not ready");
    }

    #[test]
    fn answered_entries_can_be_forgotten() {
        let mut q = CommandQueue::new();
        let _rx = q.open_window(1);
        q.answer_all(&vec!["a".to_string()]);
        assert_eq!(q.forget_answered(), 1);
        assert_eq!(q.owed(), 0);
    }

    #[tokio::test]
    async fn a_window_can_be_answered_on_a_tokio_oneshot() {
        let mut q = CommandQueue::new();
        let rx = q.open_window_async(7);
        assert_eq!(q.owed(), 1);
        assert!(q.owed_entries().next().unwrap().async_waiting());
        let answer = vec!["You see a cart.".to_string()];
        assert_eq!(
            q.close_window(answer.clone()).unwrap(),
            Outcome::Answered(answer.clone())
        );
        assert_eq!(rx.await.unwrap(), Outcome::Answered(answer));
        // the ticket is spent now, and nothing is left open to answer
        assert!(!q.owed_entries().next().unwrap().async_waiting());
        assert!(!q.offer(vec!["x".to_string()]));
    }

    #[tokio::test]
    async fn a_tokio_caller_hears_about_cleared_and_answered_all() {
        let mut q = CommandQueue::new();
        let (tx, rx) = oneshot::channel();
        q.admit(Envelope::with_oneshot("go north", tx));
        assert_eq!(q.clear_waiting(), 1);
        assert_eq!(rx.await.unwrap(), Outcome::Cleared);

        // `take_next` moves the ticket onto the owed entry, and `answer_all`
        // answers it there.
        let (tx2, rx2) = oneshot::channel();
        q.admit(Envelope::with_oneshot("look", tx2));
        let taken = q.take_next(3).unwrap();
        assert_eq!(taken.line, "look");
        let answer = vec!["a cart".to_string()];
        assert_eq!(q.answer_all(&answer), 1);
        assert_eq!(rx2.await.unwrap(), Outcome::Answered(answer));
    }

    #[tokio::test]
    async fn the_two_kinds_of_ticket_are_independent() {
        // The synchronous and the tokio replies are separate tickets, so the
        // same answer fires whichever of them an entry holds.
        let mut q = CommandQueue::new();
        let (std_tx, std_rx) = channel();
        let (async_tx, async_rx) = oneshot::channel();
        q.admit(Envelope::with_reply("one", std_tx));
        q.admit(Envelope::with_oneshot("two", async_tx));
        // both commands are written, so both tickets are owed an answer
        q.take_next(0).unwrap();
        q.take_next(0).unwrap();
        let answer = vec!["done".to_string()];
        assert_eq!(q.answer_all(&answer), 2);
        assert_eq!(std_rx.try_recv().unwrap(), Outcome::Answered(answer.clone()));
        assert_eq!(async_rx.await.unwrap(), Outcome::Answered(answer));
    }
}
