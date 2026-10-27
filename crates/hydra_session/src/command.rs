//! The session handle: the shared, thread-safe core every other part of the
//! session talks to.
//!
//! Recovered from `command::SessionHandle::*`:
//! `0x101a39308` (`generation`), `0x101a39314` (`set_symbol`),
//! `0x101a397a8` (`lich_symbol`), `0x101a39904` (`set_aliases`),
//! `0x101a399f0` (`say_unlogged`), `0x101a39a3c` (`chain_stopped`),
//! `0x101a39b08` (`command_words`), `0x101a3bb08` (`set_lich_view`),
//! `0x101a3bb50` (`set_lich_symbol`), `0x101a3bc9c` (`new`),
//! `0x101a3c0ec` (`say`), `0x101a3c1b8` (`clip`), `0x101a3c2cc` (`claim`),
//! `0x101a3c78c` (`lichs`), `0x101a3c89c` (`symbol`),
//! `0x101a3ca04` (`fold_in`), `0x101a3caf0` (`keep_log`),
//! `0x101a3cca4` (`set_clip`), `0x101a3ce34` (`set_desk`),
//! `0x101a3d44c` (`stopping`), `0x101a3d51c` (`log_typed`),
//! `0x101a3d710` (`run_steps`), `0x101a3e144` (`set_taker`),
//! `0x101a3e2a0` (`subscribe`), plus the five in `lich.rs`
//! (`hand_to_lich`, `lich_door`, `lich_running`, `lich_takes_typing`,
//! `to_lich`).
//!
//! **ESTABLISHED** layout of `SessionHandle` (from the field accesses; the
//! reconstruction keeps the same *shape* with std synchronisation):
//!
//! ```text
//! +0x08  broadcast::Sender<Event>      say / say_unlogged push here
//! +0x10  Arc<Cell<u32>>                generation()
//! +0x18  Arc<Mutex<claimant::Settings>> symbol() / lich_symbol() / claim()
//! +0x20  Arc<…{ PlayerLog@+0x10, …, bool busy@+0x50 }>  say() logs notices
//! ```
//!
//! * `say` (`0x101a3c0ec`): when the log is not idle and the notice's level
//!   byte (`+0x39`) is not `3`, every notice line goes through
//!   `PlayerLog::notice`; then the `Notice` (64 bytes) is moved onto the
//!   broadcast channel with `Sender::send`, and a `SendError` whose `Event`
//!   discriminant is `0x15` is silently dropped.
//! * `say_unlogged` (`0x101a399f0`): the same broadcast send, with no logging.
//! * `symbol` (`0x101a3c89c`): clone the settings behind `+0x18`, read the
//!   desk's symbol char, and map the `Option<char>` niche (`0x110000`) to "no
//!   symbol".
//! * `chain_stopped` (`0x101a39a3c`): formats `"Not run: {}"`
//!   (`0x103e7bf21`) and says it.
//! * `hand_to_lich` (`0x101a38d58`, in `lich.rs`) says
//!   `"Lich is not running for this character: not sent: {}"`
//!   (`0x103e7bea3`) or
//!   `"Lich is not reading what is typed for it ({} lines wait): not handed:
//!   {}"` (`0x103e7bed8`).
//! * `run_steps` (`0x101a3d710`) says the alias error
//!   `"The alias {}* has more than one command and no \\? for `{}`: it was not
//!   sent."` (`0x103e22a74`).
//!
//! **std-only departures**: no `tokio::sync::broadcast` (we keep an
//! `Arc<Mutex<EventLog>>` that subscribers read with a cursor, which is the
//! same "each subscriber sees every event once" contract), no `oneshot`, no
//! `CancellationToken` (a plain `AtomicBool`).
//!
//! **Phase 1** reconnected what fits behind the existing facade:
//!
//! * `stopping`/`stop` are a real `tokio_util::sync::CancellationToken`
//!   ([`SessionHandle::stop_token`] hands out a clone the relay can await);
//! * every emitted event also goes onto a real `tokio::sync::broadcast`
//!   channel ([`SessionHandle::watch`]), which is what `say`/`say_unlogged`
//!   drive in the binary; the cursor-based [`Listener`] stays because its
//!   synchronous `try_recv`/`drain` API is what the sibling crates use.

use crate::alias::Aliases;
use crate::claimant::Desk;
use crate::lifecycle::GenerationCell;
use crate::notes::Clip;
use crate::notice::{Notice, LEVEL_WARN};
use crate::queue::CommandQueue;
use crate::steps::{self, Step};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

/// How many events the shared log keeps for late subscribers.
pub const EVENT_BACKLOG: usize = 1024;

/// An event on the session's channel.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// something to show the player
    Said(Notice),
    /// a line the player typed
    Typed(String),
    /// the game answered with these lines
    Frames(Vec<String>),
    /// the command symbol changed
    Symbol(char),
    /// Lich's command symbol changed
    LichSymbol(char),
    /// the session is ending
    Over(String),
}

/// The shared event log: a bounded backlog plus a sequence number.
#[derive(Debug, Default)]
pub struct EventLog {
    log: VecDeque<(u64, Event)>,
    next: u64,
}

impl EventLog {
    /// Push an event, dropping the oldest when the backlog is full.
    fn push(&mut self, event: Event) {
        let seq = self.next;
        self.next += 1;
        self.log.push_back((seq, event));
        while self.log.len() > EVENT_BACKLOG {
            self.log.pop_front();
        }
    }

    /// The events with a sequence number greater than `cursor`, up to `want`.
    fn since(&self, cursor: u64, want: usize) -> (Vec<Event>, u64) {
        let mut out = Vec::new();
        let mut next = cursor;
        for (seq, event) in &self.log {
            if *seq < cursor {
                continue;
            }
            if out.len() == want {
                break;
            }
            out.push(event.clone());
            next = seq + 1;
        }
        (out, next)
    }
}

/// A subscriber's view of the session events.
#[derive(Debug)]
pub struct Listener {
    log: Arc<Mutex<EventLog>>,
    cursor: u64,
}

impl Listener {
    /// The next event, if one has been pushed since the last call.
    pub fn try_recv(&mut self) -> Option<Event> {
        let log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        let (mut events, next) = log.since(self.cursor, 1);
        drop(log);
        self.cursor = next;
        events.pop()
    }

    /// Everything pushed since the last call.
    pub fn drain(&mut self) -> Vec<Event> {
        let log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        let (events, next) = log.since(self.cursor, usize::MAX);
        drop(log);
        self.cursor = next;
        events
    }

    /// Wait up to `timeout` for an event.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Option<Event> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(event) = self.try_recv() {
                return Some(event);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// A snapshot of the session's state, for callers that need several values at
/// once.  The binary has a `command::Snapshot` (dropped in
/// `supervisor::answer_between`).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub generation: u32,
    pub symbol: Option<char>,
    pub lich_symbol: Option<char>,
    pub character: String,
    pub waiting: usize,
    pub owed: usize,
    pub stopping: bool,
}

/// Where a line goes when the session wants to write to the game.
pub type Taker = Sender<String>;

/// The shared state behind a [`SessionHandle`].
#[derive(Debug)]
pub struct Inner {
    events: Arc<Mutex<EventLog>>,
    /// the recovered `broadcast::Sender<Event>` (**ESTABLISHED** at `+0x08`)
    broadcast: broadcast::Sender<Event>,
    generation: GenerationCell,
    desk: Mutex<Option<Desk>>,
    lich_view: AtomicBool,
    queue: Mutex<CommandQueue>,
    aliases: Mutex<Aliases>,
    clip: Mutex<Option<Clip>>,
    log: Mutex<Vec<String>>,
    typed_log: Mutex<Vec<String>>,
    taker: Mutex<Option<Taker>>,
    /// the recovered `CancellationToken`: `stopping` reads it, `stop` trips it
    stop: CancellationToken,
    keep_log: AtomicBool,
    lich: Mutex<crate::lich::Slot>,
    /// the `.lich` settings (`lich.json`): the folder Lich lives in, whether it
    /// runs, and what the window shows.  Global, not per character — see
    /// [`crate::lich_command`].
    lich_settings: Mutex<crate::lich_command::LichSettings>,
}

/// The handle every other part of the session holds.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    inner: Arc<Inner>,
}

impl SessionHandle {
    /// A handle for a desk whose command symbol is `symbol` (default `.`).
    ///
    /// **ESTABLISHED** shape (`0x101a3bc9c`): allocates the generation cell,
    /// the settings, the player log and the broadcast channel.
    pub fn new(desk: Desk) -> Self {
        // The recovered session is `broadcast::channel(...)`; the capacity is
        // the same 1024-event backlog the cursor log keeps.
        let (broadcast, _) = broadcast::channel(EVENT_BACKLOG);
        SessionHandle {
            inner: Arc::new(Inner {
                events: Arc::new(Mutex::new(EventLog::default())),
                broadcast,
                generation: GenerationCell::new(),
                desk: Mutex::new(Some(desk)),
                lich_view: AtomicBool::new(false),
                queue: Mutex::new(CommandQueue::new()),
                aliases: Mutex::new(Aliases::new()),
                clip: Mutex::new(None),
                log: Mutex::new(Vec::new()),
                typed_log: Mutex::new(Vec::new()),
                taker: Mutex::new(None),
                stop: CancellationToken::new(),
                keep_log: AtomicBool::new(true),
                lich: Mutex::new(crate::lich::Slot::new()),
                lich_settings: Mutex::new(crate::lich_command::LichSettings::default()),
            }),
        }
    }

    /// The generation of this session; it changes when the character changes.
    ///
    /// **ESTABLISHED** (`0x101a39308`): `[[self+0x10]+0x10]` as a `u32`.
    pub fn generation(&self) -> u32 {
        self.inner.generation.get()
    }

    /// Bump the generation.
    pub fn advance_generation(&self) -> u32 {
        self.inner.generation.advance()
    }

    /// The command symbol, if a desk is installed.
    ///
    /// **ESTABLISHED** (`0x101a3c89c`).
    pub fn symbol(&self) -> Option<char> {
        self.desk().map(|d| d.symbol())
    }

    /// Lich's command symbol, if a desk is installed.
    ///
    /// **ESTABLISHED** (`0x101a397a8`).
    pub fn lich_symbol(&self) -> Option<char> {
        self.desk().map(|d| d.lich_symbol())
    }

    /// Change the command symbol.
    ///
    /// **ESTABLISHED** shape (`0x101a39314`): writes through the settings and
    /// announces `Event::Symbol`.
    pub fn set_symbol(&self, c: char) {
        if let Some(desk) = self.desk() {
            desk.set_symbol(c);
        }
        self.emit(Event::Symbol(c));
    }

    /// Change Lich's command symbol.
    pub fn set_lich_symbol(&self, c: char) {
        if let Some(desk) = self.desk() {
            desk.set_lich_symbol(c);
        }
        self.emit(Event::LichSymbol(c));
    }

    /// Show (or hide) Lich's own output.
    ///
    /// **ESTABLISHED** (`0x101a3bb08`): a single flag write.  Phase 3 keeps the
    /// flag and mirrors it into the `.lich` settings' `view` member, so
    /// `set_lich_view(true)` is `lich view lich` and `set_lich_view(false)` is
    /// `lich view game` ([`crate::lich_command::LichView::from_on`]).
    pub fn set_lich_view(&self, on: bool) {
        self.inner.lich_view.store(on, Ordering::Relaxed);
        crate::lich::lock(&self.inner.lich_settings).view =
            crate::lich_command::LichView::from_on(on);
        self.advance_generation();
    }

    /// Is Lich's output being shown?
    pub fn lich_view(&self) -> bool {
        self.inner.lich_view.load(Ordering::Relaxed)
    }

    /// The `.lich` settings in force (`lich.json`): the folder Lich lives in,
    /// whether it runs, and what the window shows.
    ///
    /// The session does not read or write the file — `hydra`'s binary does, at
    /// startup and whenever [`crate::lich_command::LichOutcome::changed`] says
    /// so; this is the in-memory copy the `.lich` commands work on.
    pub fn lich_settings(&self) -> crate::lich_command::LichSettings {
        crate::lich::lock(&self.inner.lich_settings).clone()
    }

    /// Install a `.lich` settings value, applying its view to
    /// [`SessionHandle::set_lich_view`] (which is the same flag the window
    /// reads).
    pub fn set_lich_settings(&self, settings: crate::lich_command::LichSettings) {
        let view = settings.view;
        *crate::lich::lock(&self.inner.lich_settings) = settings;
        if self.lich_view() != view.is_lich() {
            self.set_lich_view(view.is_lich());
        }
    }

    /// Is Lich (the view, not the process) in play?
    ///
    /// **ESTABLISHED** shape (`0x101a3c78c`).
    pub fn lichs(&self) -> bool {
        self.lich_view()
    }

    /// The current desk.
    ///
    /// **ESTABLISHED** shape (`0x101a3c2cc`): clones the settings.
    pub fn claim(&self) -> Option<Desk> {
        self.desk()
    }

    /// Install a different desk (a new character, a new symbol).
    ///
    /// **ESTABLISHED** (`0x101a3ce34`).
    pub fn set_desk(&self, desk: Desk) {
        *self.inner.desk.lock().unwrap_or_else(|e| e.into_inner()) = Some(desk);
        self.advance_generation();
    }

    /// Say something to the player, and log it unless it is quiet.
    ///
    /// **ESTABLISHED** (`0x101a3c0ec`): log the lines when the notice's level
    /// is not `3`, then put the notice on the event channel.
    pub fn say(&self, notice: &Notice) {
        if !notice.is_quiet() {
            let mut log = self.inner.log.lock().unwrap_or_else(|e| e.into_inner());
            for line in &notice.lines {
                log.push(line.clone());
            }
            while log.len() > EVENT_BACKLOG {
                log.remove(0);
            }
        }
        self.emit(Event::Said(notice.clone()));
    }

    /// Say something without writing it to the log.
    ///
    /// **ESTABLISHED** (`0x101a399f0`).
    pub fn say_unlogged(&self, notice: &Notice) {
        self.emit(Event::Said(notice.clone()));
    }

    /// Say one informational line.
    pub fn say_line(&self, level: u8, text: impl Into<String>) {
        self.say(&Notice::line(level, text));
    }

    /// A subscriber to the session's events.
    ///
    /// **ESTABLISHED** (`0x101a3e2a0`, 22 KB of inlined select machinery in the
    /// original): returns a receiver that sees every event from now on.
    pub fn subscribe(&self) -> Listener {
        let log = self.inner.events.lock().unwrap_or_else(|e| e.into_inner());
        Listener {
            log: self.inner.events.clone(),
            cursor: log.next,
        }
    }

    /// The lines the session has said so far (the player-visible log).
    pub fn said(&self) -> Vec<String> {
        self.inner
            .log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Where typed lines go.
    ///
    /// **ESTABLISHED** shape (`0x101a3e144`): installs the writer the session
    /// uses for text it did not generate itself.
    pub fn set_taker(&self, taker: Taker) {
        *self.inner.taker.lock().unwrap_or_else(|e| e.into_inner()) = Some(taker);
    }

    /// Write one line to the game, through the taker if there is one.
    pub fn to_game(&self, line: &str) {
        let guard = self.inner.taker.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(taker) = guard.as_ref() {
            let _ = taker.send(line.to_string());
        }
    }

    /// The command queue.
    pub fn queue(&self) -> MutexGuard<'_, CommandQueue> {
        self.inner.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The aliases in force.
    pub fn aliases(&self) -> MutexGuard<'_, Aliases> {
        self.inner.aliases.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Install a different alias set.
    ///
    /// **ESTABLISHED** (`0x101a39904`): moves the alias handle in.
    pub fn set_aliases(&self, aliases: Aliases) {
        *self.inner.aliases.lock().unwrap_or_else(|e| e.into_inner()) = aliases;
    }

    /// The current clip, if a note was just clipped.
    ///
    /// **ESTABLISHED** (`0x101a3c1b8`).
    pub fn clip(&self) -> Option<Clip> {
        self.inner
            .clip
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Set (or clear) the clip.
    ///
    /// **ESTABLISHED** (`0x101a3cca4`).
    pub fn set_clip(&self, clip: Option<Clip>) {
        *self.inner.clip.lock().unwrap_or_else(|e| e.into_inner()) = clip;
    }

    /// Is the player log being kept?
    ///
    /// **ESTABLISHED** shape (`0x101a3caf0`).
    pub fn keep_log(&self) -> bool {
        self.inner.keep_log.load(Ordering::Relaxed)
    }

    /// Turn the player log on or off.
    pub fn set_keep_log(&self, on: bool) {
        self.inner.keep_log.store(on, Ordering::Relaxed);
    }

    /// Is the session shutting down?
    ///
    /// **ESTABLISHED** shape (`0x101a3d44c`).  **Phase 1**: reads the real
    /// `CancellationToken` (the original's `cancel_token.is_cancelled()`).
    pub fn stopping(&self) -> bool {
        self.inner.stop.is_cancelled()
    }

    /// Start shutting down.
    ///
    /// **Phase 1**: trips the `CancellationToken`, which wakes every
    /// [`SessionHandle::stop_token`] waiter, then announces `Event::Over`.
    pub fn stop(&self, reason: &str) {
        self.inner.stop.cancel();
        self.emit(Event::Over(reason.to_string()));
    }

    /// A clone of the session's cancellation token.
    ///
    /// **ESTABLISHED**: the original shares one `CancellationToken` between
    /// the session, the relay and the player log; a clone can be awaited with
    /// `token.cancelled()` or passed to a child task.
    pub fn stop_token(&self) -> CancellationToken {
        self.inner.stop.clone()
    }

    /// A subscription to the session's event channel.
    ///
    /// **ESTABLISHED**: `say`/`say_unlogged` push onto a
    /// `tokio::sync::broadcast` channel; this is the receiving end of it, for
    /// callers that run under tokio.  [`SessionHandle::subscribe`] is the
    /// synchronous, backlog-replaying equivalent.
    pub fn watch(&self) -> broadcast::Receiver<Event> {
        self.inner.broadcast.subscribe()
    }

    /// The broadcasting end itself, for handing to another task.
    pub fn broadcaster(&self) -> broadcast::Sender<Event> {
        self.inner.broadcast.clone()
    }

    /// A command in the chain was not run.
    ///
    /// **ESTABLISHED** text (`0x101a3bf21`): `"Not run: {}"`, at level 1.
    pub fn chain_stopped(&self, reason: &str) {
        self.say_line(LEVEL_WARN, format!("Not run: {reason}"));
    }

    /// Record a typed line in the log.
    ///
    /// **ESTABLISHED** shape (`0x101a3d51c`): the typed line is logged with the
    /// `"{}/{}"` template.
    pub fn log_typed(&self, line: &str) {
        if !self.keep_log() {
            return;
        }
        let (day, _time) = crate::player_log::writer::civil::now();
        let mut log = self
            .inner
            .typed_log
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        log.push(format!("{day}/{line}"));
    }

    /// The typed lines the session has recorded.
    pub fn typed_log(&self) -> Vec<String> {
        self.inner
            .typed_log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The words of a command line after alias expansion.
    ///
    /// **ESTABLISHED** shape (`0x101a39b08`): splits on whitespace, expands
    /// aliases, and reports a broken alias with `"Not run: {}"`.
    pub fn command_words(&self, line: &str) -> Vec<String> {
        match self.aliases().expand(line) {
            Ok(Some(expanded)) => expanded
                .iter()
                .flat_map(|l| l.split_whitespace().map(str::to_string))
                .collect(),
            Ok(None) => line.split_whitespace().map(str::to_string).collect(),
            Err(problem) => {
                self.chain_stopped(&problem);
                Vec::new()
            }
        }
    }

    /// Expand a line into steps and hand them to the taker.
    ///
    /// **ESTABLISHED** shape (`0x101a3d710`): expand the aliases, say the
    /// recovered alias error when expansion fails, then send the steps.
    pub fn run_steps(&self, line: &str) -> Result<Vec<Step>, String> {
        let expanded = match self.aliases().expand(line) {
            Ok(Some(lines)) => lines,
            Ok(None) => vec![line.to_string()],
            Err(problem) => {
                self.say(&Notice::line(LEVEL_WARN, problem.clone()));
                return Err(problem);
            }
        };
        let mut out = Vec::new();
        for line in expanded {
            for step in steps::steps(&line) {
                if let Some(text) = step.line() {
                    self.to_game(text);
                    self.log_typed(text);
                }
                out.push(step);
            }
        }
        Ok(out)
    }

    /// Fold game output into the session.
    ///
    /// **ESTABLISHED** shape (`0x101a3ca04`): the frames land in the log and on
    /// the event channel.
    pub fn fold_in(&self, frames: Vec<String>) {
        if frames.is_empty() {
            return;
        }
        if self.keep_log() {
            let mut log = self.inner.log.lock().unwrap_or_else(|e| e.into_inner());
            log.extend(frames.iter().cloned());
        }
        self.emit(Event::Frames(frames));
    }

    /// The Lich slot for this session.
    pub fn slot(&self) -> MutexGuard<'_, crate::lich::Slot> {
        self.inner.lich.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Everything a caller needs at once.
    pub fn snapshot(&self) -> Snapshot {
        let queue = self.queue();
        let (waiting, owed) = (queue.waiting(), queue.owed());
        drop(queue);
        Snapshot {
            generation: self.generation(),
            symbol: self.symbol(),
            lich_symbol: self.lich_symbol(),
            character: self.desk().map(|d| d.character().to_string()).unwrap_or_default(),
            waiting,
            owed,
            stopping: self.stopping(),
        }
    }

    fn desk(&self) -> Option<Desk> {
        self.inner
            .desk
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Put an event on the session's channel.
    ///
    /// **ESTABLISHED**: the recovered `say` sends on the session's broadcast
    /// channel and drops the `SendError` when nobody is listening.  Phase 1
    /// does exactly that on the real `tokio::sync::broadcast` channel, and
    /// keeps pushing into the cursor log so the synchronous [`Listener`] still
    /// sees every event once.
    pub fn emit(&self, event: Event) {
        self.inner
            .events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event.clone());
        // `send` only fails when there is no receiver at all, which is not an
        // error the session cares about.
        let _ = self.inner.broadcast.send(event);
    }
}

/// A pair of ends for the "to game" channel.
pub fn taker_pair() -> (Taker, Receiver<String>) {
    channel()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claimant::Desk;
    use crate::notice::LEVEL_INFO;

    fn handle() -> SessionHandle {
        SessionHandle::new(Desk::new(None, "Aelith"))
    }

    #[test]
    fn the_basics_of_a_fresh_handle() {
        let s = handle();
        assert_eq!(s.generation(), 0);
        assert_eq!(s.symbol(), Some('.'));
        assert_eq!(s.lich_symbol(), Some(';'));
        assert_eq!(s.claim().unwrap().character(), "Aelith");
        assert!(!s.stopping());
        assert!(s.keep_log());
        assert!(!s.lichs());
        s.set_symbol('!');
        assert_eq!(s.symbol(), Some('!'));
        s.set_lich_symbol(',');
        assert_eq!(s.lich_symbol(), Some(','));
        s.set_lich_view(true);
        assert!(s.lichs());
        assert_eq!(s.generation(), 1);
    }

    #[test]
    fn say_logs_and_say_unlogged_does_not() {
        let s = handle();
        s.say(&Notice::line(LEVEL_INFO, "one"));
        assert_eq!(s.said(), vec!["one".to_string()]);
        s.say_unlogged(&Notice::line(LEVEL_INFO, "two"));
        assert_eq!(s.said(), vec!["one".to_string()]);
        s.say(&Notice::line(crate::notice::LEVEL_QUIET, "hush"));
        assert_eq!(s.said().len(), 1, "quiet notices are not logged");
    }

    #[test]
    fn subscribers_see_each_event_once() {
        let s = handle();
        let mut a = s.subscribe();
        s.say(&Notice::line(LEVEL_INFO, "one"));
        let mut b = s.subscribe();
        s.say(&Notice::line(LEVEL_INFO, "two"));
        assert_eq!(
            a.drain(),
            vec![
                Event::Said(Notice::line(LEVEL_INFO, "one")),
                Event::Said(Notice::line(LEVEL_INFO, "two"))
            ]
        );
        assert_eq!(a.try_recv(), None);
        assert_eq!(b.drain(), vec![Event::Said(Notice::line(LEVEL_INFO, "two"))]);
        assert_eq!(b.try_recv(), None);
    }

    #[test]
    fn chain_stopped_says_not_run_at_level_one() {
        let s = handle();
        s.chain_stopped("the stance was refused");
        assert_eq!(s.said(), vec!["Not run: the stance was refused".to_string()]);
        let mut l = s.subscribe();
        s.chain_stopped("again");
        match l.try_recv() {
            Some(Event::Said(n)) => {
                assert_eq!(n.level, 1);
                assert_eq!(n.first_line(), "Not run: again");
            }
            other => panic!("wanted a notice, got {other:?}"),
        }
    }

    #[test]
    fn running_steps_hands_lines_to_the_taker() {
        let s = handle();
        let (tx, rx) = taker_pair();
        s.set_taker(tx);
        let steps = s.run_steps("go north").unwrap();
        assert_eq!(steps, vec![Step::Send("go north".to_string())]);
        assert_eq!(rx.try_recv().unwrap(), "go north");
        // typed lines are logged too
        assert_eq!(s.said().len(), 0);
        assert_eq!(s.typed_log().len(), 1);
        let steps = s.run_steps("wait 2; kill rat").unwrap();
        assert_eq!(
            steps,
            vec![Step::Wait(2), Step::Send("kill rat".to_string())]
        );
        assert_eq!(rx.try_recv().unwrap(), "kill rat");
    }

    #[test]
    fn a_broken_alias_is_said_and_stops_the_run() {
        let s = handle();
        let aliases = Aliases::new();
        aliases
            .file()
            .aliases
            .insert("pr".to_string(), vec!["a".to_string(), "b".to_string()]);
        s.set_aliases(aliases);
        let e = s.run_steps("pr rat").unwrap_err();
        assert!(e.contains("has more than one command"));
        assert_eq!(s.said().len(), 1);
        assert!(s.command_words("pr rat").is_empty());
    }

    #[test]
    fn command_words_expands_aliases() {
        let s = handle();
        let aliases = Aliases::new();
        aliases
            .file()
            .aliases
            .insert("k".to_string(), vec!["kill ?".to_string()]);
        s.set_aliases(aliases);
        assert_eq!(s.command_words("k rat"), vec!["kill", "rat"]);
        assert_eq!(s.command_words("go north"), vec!["go", "north"]);
    }

    #[test]
    fn fold_in_delivers_frames() {
        let s = handle();
        let mut l = s.subscribe();
        s.fold_in(vec!["You see a cart.".to_string()]);
        assert_eq!(
            l.try_recv(),
            Some(Event::Frames(vec!["You see a cart.".to_string()]))
        );
        let snap = s.snapshot();
        assert_eq!(snap.character, "Aelith");
        assert_eq!(snap.waiting, 0);
    }

    #[test]
    fn clip_round_trips() {
        let s = handle();
        assert_eq!(s.clip(), None);
        let clip = crate::notes::Clip::of_room("0x1", "Town", "a cart", &[], "2024-05-01", 3);
        s.set_clip(Some(clip.clone()));
        assert_eq!(s.clip(), Some(clip));
    }

    #[test]
    fn stopping_marks_the_session_over() {
        let s = handle();
        let mut l = s.subscribe();
        s.stop("the player left");
        assert!(s.stopping());
        assert_eq!(l.try_recv(), Some(Event::Over("the player left".to_string())));
    }

    #[tokio::test]
    async fn the_stop_token_cancels_every_waiter() {
        let s = handle();
        let token = s.stop_token();
        assert!(!token.is_cancelled());
        // A clone shares the tripping: `cancelled()` wakes when `stop` runs.
        let waiter = token.clone();
        let waiting = tokio::spawn(async move {
            waiter.cancelled().await;
            true
        });
        tokio::task::yield_now().await;
        assert!(!waiting.is_finished());
        s.stop("bye");
        assert!(token.is_cancelled());
        assert!(waiting.await.unwrap());
        // Trip it twice: `CancellationToken::cancel` is idempotent.
        s.stop("again");
        assert!(s.stopping());
    }

    #[tokio::test]
    async fn every_said_event_reaches_the_broadcast_channel() {
        let s = handle();
        let mut rx = s.watch();
        s.say_line(LEVEL_INFO, "hello");
        s.stop("done");
        assert_eq!(
            rx.recv().await.unwrap(),
            Event::Said(crate::notice::Notice::line(LEVEL_INFO, "hello"))
        );
        assert_eq!(
            rx.recv().await.unwrap(),
            Event::Over("done".to_string())
        );
        // The synchronous listener sees the same two events, so the two views
        // agree.
        let mut l = s.subscribe();
        s.say_line(LEVEL_INFO, "later");
        assert_eq!(
            l.try_recv(),
            Some(Event::Said(crate::notice::Notice::line(LEVEL_INFO, "later")))
        );
        assert_eq!(
            rx.recv().await.unwrap(),
            Event::Said(crate::notice::Notice::line(LEVEL_INFO, "later"))
        );
    }
}
