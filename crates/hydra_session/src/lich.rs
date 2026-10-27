//! The door to Lich: what the session hands typed lines to.
//!
//! This is the contract `hydra_lich`'s relay expects (see
//! `crates/hydra_lich/NOTES.md` §2), recovered from
//! `hydra_session::lich::*`:
//!
//! | function | address |
//! | --- | --- |
//! | `lock` | `0x101a884e0` |
//! | `Slot::takes_typing` | `0x101a88014` |
//! | `Slot::hand` | `0x101a88070` |
//! | `Slot::wire` | `0x101a881a4` |
//! | `Slot::begin` | `0x101a882b8` |
//! | `Slot::close` | `0x101a88408` |
//! | `Slot::waits` | `0x101a8846c` |
//! | `Wire::fell_behind` | `0x101a884bc` |
//! | `Wire::begin` | `0x101a884d0` |
//! | `Typing::waiting` | `0x101a88554` |
//! | `LichDoor::lich_symbol` | `0x101a88558` |
//! | `LichDoor::session_over` | `0x101a8855c` |
//! | `LichDoor::say` | `0x101a88608` |
//! | `LichDoor::show` | `0x101a8860c` |
//! | `LichDoor::claim` | `0x101a88960` |
//! | `LichDoor::symbol` | `0x101a89030` |
//! | `SessionHandle::hand_to_lich` | `0x101a38d58` |
//! | `SessionHandle::lich_running` | `0x101a38f34` |
//! | `SessionHandle::lich_takes_typing` | `0x101a39040` |
//! | `SessionHandle::to_lich` | `0x101a39154` |
//! | `SessionHandle::lich_door` | `0x101a3923c` |
//!
//! **ESTABLISHED** layout of `Slot` (from `close`/`hand`/`wire`/`begin`):
//!
//! ```text
//! +0x00  8 bytes, cleared first by `close`
//! +0x08  Tap  (0x20 bytes)   the "hand": what typed lines are written to
//! +0x28  Tap  (0x20 bytes)   the "wire": the game stream Lich mirrors
//! +0x48  bool session_over
//! ```
//!
//! A `Tap` is `+0x00` an `Option<Sender<…>>` (the null test in `close` is the
//! `Option` niche, and `takes_typing`/`hand` ask the channel's semaphore at
//! `Sender+0x1c8` whether it is closed), `+0x08` the sender actually used and
//! `+0x10` an `Arc` whose payload bool at `+0x10` is set when a send fails —
//! that is the "fell behind" flag `Wire::fell_behind` reads and
//! `Wire::begin` clears.
//!
//! Behaviour, function by function:
//!
//! * `Slot::takes_typing` — true when the hand tap is open, or when the wire
//!   tap is open *and* its typing flag is set.
//! * `Slot::hand(line)` — clone the message, `try_acquire(1)` on the hand
//!   channel, send; on a closed channel set the tap's flag and answer `false`.
//! * `Slot::wire(line)` — the same for the wire tap, and drop the tap when the
//!   send fails (`str xzr,[self]` at the end).
//! * `Slot::begin(line)` — take the wire tap out of the slot, send it the line,
//!   put back whatever is left; the return value is `old_tap.is_some()`
//!   (`cmp x23,#0; cset ne`).
//! * `Slot::close` — drop both taps (via `drop_in_place<Tap>`), set
//!   `session_over` at `+0x48`.
//! * `Slot::waits` — the wire tap is open and its waiting flag is set.
//! * `LichDoor::session_over` — lock the shared slot, read `+0x68` (the same
//!   flag, seen through the door's `Arc<Mutex<…>>`).
//! * `LichDoor::symbol` — `SessionHandle::symbol`, with the `Option<char>`
//!   niche `0x110000` mapped to `'.'` (`csel w8, #0x2e, w0, eq`).
//! * `LichDoor::claim` — builds a **4096**-slot channel and a **64**-slot
//!   channel (`Semaphore::new(0x1000)` / `Semaphore::new(0x40)`) plus two
//!   24-byte `Arc`s, and installs them on the slot.
//! * `LichDoor::show(line)` — locks the shared text parser, turns the line into
//!   frames + marks (`lich::text::LichText::read`), and folds the resulting
//!   `Vec<Frame>` into the session with `SessionHandle::fold_in`.
//! * `SessionHandle::hand_to_lich(line)` — `Slot::hand`; when the hand is not
//!   open it says `"Lich is not running for this character: not sent: {}"`
//!   (level 2, `0x103e7bea3`) or
//!   `"Lich is not reading what is typed for it ({} lines wait): not handed:
//!   {}"` (level 1, `0x103e7bed8`).
//!
//! **std-only departures**: `tokio::sync::mpsc::{Sender,Receiver}` →
//! `std::sync::mpsc`; `Arc<Mutex<…>>` stays.
//!
//! The token and password handshake live in `hydra_lich` (`new_token` produces
//! the 64-character token; `--pipe` makes Lich listen on `127.0.0.1:<port>`);
//! this crate only carries the door.  See `NOTES.md` §"Lich door contract".

use crate::command::{Event, SessionHandle};
use crate::notice::{Notice, LEVEL_INFO, LEVEL_WARN};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

/// How many lines the door buffers coming *from* Lich
/// (**ESTABLISHED**: `Semaphore::new(0x1000)` in `LichDoor::claim`).
pub const LINES: usize = 4096;
/// How many typed lines the door buffers going *to* Lich
/// (**ESTABLISHED**: `Semaphore::new(0x40)`).
pub const TYPED: usize = 64;

/// Lock a mutex, ignoring poisoning.
///
/// **ESTABLISHED** (`0x101a884e0`): a `Mutex::lock` wrapper that returns the
/// guard.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The flag a tap reports to.
#[derive(Debug, Clone, Default)]
pub struct Wire {
    flag: Arc<AtomicBool>,
}

impl Wire {
    /// A wire flag that has not tripped.
    pub fn new() -> Self {
        Wire::default()
    }

    /// True once a send failed: Lich fell behind the game stream.
    ///
    /// **ESTABLISHED** (`0x101a884bc`): reads the bool at `+0x10` of the shared
    /// cell.
    pub fn fell_behind(&self) -> bool {
        self.flag.load(Ordering::Relaxed)
    }

    /// Mark the stream as broken.
    ///
    /// **ESTABLISHED** (`0x101a884d0`): writes `1` into the shared cell.
    pub fn begin(&self) {
        self.flag.store(true, Ordering::Relaxed);
    }

    /// Clear the flag (a fresh stream).
    pub fn reset(&self) {
        self.flag.store(false, Ordering::Relaxed);
    }
}

/// One connection to Lich.
#[derive(Debug)]
pub struct Tap {
    /// where lines go
    pub tx: Sender<String>,
    /// the "fell behind" flag for this tap
    pub wire: Wire,
    /// whether this tap takes the player's typing
    pub typing: bool,
    /// whether this tap is waiting for something
    pub waiting: bool,
}

impl Tap {
    /// A tap that sends on `tx`.
    pub fn new(tx: Sender<String>) -> Self {
        Tap {
            tx,
            wire: Wire::new(),
            typing: false,
            waiting: false,
        }
    }

    /// A tap that also takes the player's typing.
    pub fn taking_typing(mut self) -> Self {
        self.typing = true;
        self
    }

    /// A tap that is waiting for something.
    pub fn waiting_for(mut self) -> Self {
        self.waiting = true;
        self
    }

    /// Send one line; `false` when the other end is gone.
    pub fn send(&self, line: &str) -> bool {
        if self.tx.send(line.to_string()).is_err() {
            self.wire.begin();
            return false;
        }
        true
    }
}

/// The state of the slot.
#[derive(Debug, Default)]
pub struct SlotState {
    /// the tap typed lines are handed to
    pub hand: Option<Tap>,
    /// the tap the game stream is mirrored to
    pub wire: Option<Tap>,
    /// set by `close`
    pub session_over: bool,
    /// the lines that are waiting to go to Lich
    pub pending: Vec<String>,
    /// where the relay pushes the lines Lich sends back
    pub lines: Option<Receiver<String>>,
}

/// The current Lich slot for a session.
#[derive(Debug, Clone, Default)]
pub struct Slot {
    state: Arc<Mutex<SlotState>>,
}

impl Slot {
    /// An empty slot.
    pub fn new() -> Self {
        Slot::default()
    }

    /// The state behind the slot.
    pub fn state(&self) -> MutexGuard<'_, SlotState> {
        lock(&self.state)
    }

    /// Does a tap exist that takes the player's typing?
    ///
    /// **ESTABLISHED** (`0x101a88014`): true when the hand tap is open, or when
    /// the wire tap is open and carries the typing flag.
    pub fn takes_typing(&self) -> bool {
        let state = self.state();
        if state.hand.is_some() {
            return true;
        }
        match &state.wire {
            Some(tap) => tap.typing,
            None => false,
        }
    }

    /// Is a tap waiting for something?
    ///
    /// **ESTABLISHED** (`0x101a8846c`): the wire tap exists and its waiting
    /// flag is set.
    pub fn waits(&self) -> bool {
        match &self.state().wire {
            Some(tap) => tap.waiting,
            None => false,
        }
    }

    /// Hand one line to Lich.  `false` when nothing took it.
    ///
    /// **ESTABLISHED** (`0x101a88070`): the hand tap first, then the wire tap;
    /// a failed send trips the tap's "fell behind" flag.
    pub fn hand(&self, line: &str) -> bool {
        let state = self.state();
        if let Some(tap) = state.hand.as_ref() {
            return tap.send(line);
        }
        match state.wire.as_ref() {
            Some(tap) => tap.send(line),
            None => false,
        }
    }

    /// Send one mirrored game line to Lich.  `false` when it did not go.
    ///
    /// **ESTABLISHED** (`0x101a881a4`): a failed send drops the wire tap and
    /// records that Lich fell behind.
    pub fn wire(&self, line: &str) -> bool {
        let mut state = self.state();
        let Some(tap) = state.wire.as_ref() else {
            return false;
        };
        if tap.send(line) {
            return true;
        }
        let fell = tap.wire.clone();
        state.wire = None;
        drop(state);
        fell.begin();
        false
    }

    /// Open the door: send the first line to the wire tap.
    ///
    /// **ESTABLISHED** (`0x101a882b8`): takes the wire tap out of the slot and
    /// answers whether there was one.
    pub fn begin(&mut self, line: &str) -> bool {
        let mut state = lock(&self.state);
        let Some(tap) = state.wire.take() else {
            return false;
        };
        let sent = tap.send(line);
        if sent {
            state.wire = Some(tap);
        } else {
            tap.wire.begin();
        }
        true
    }

    /// Close the slot: drop both taps and mark the session over.
    ///
    /// **ESTABLISHED** (`0x101a88408`): `drop_in_place<Tap>` twice, then
    /// `session_over = true` at `+0x48`.
    pub fn close(&mut self) {
        let mut state = lock(&self.state);
        state.hand = None;
        state.wire = None;
        state.session_over = true;
        state.pending.clear();
    }

    /// Is the session over?
    pub fn session_over(&self) -> bool {
        self.state().session_over
    }

    /// Install a tap for typed lines.
    pub fn set_hand(&self, tap: Tap) {
        lock(&self.state).hand = Some(tap);
    }

    /// Install a tap for the mirrored game stream.
    pub fn set_wire(&self, tap: Tap) {
        lock(&self.state).wire = Some(tap);
    }

    /// The wire flag, if a wire tap is installed.
    pub fn wire_flag(&self) -> Option<Wire> {
        self.state().wire.as_ref().map(|t| t.wire.clone())
    }

    /// Take the next line the relay pushed, if there is one.
    pub fn take_line(&self) -> Option<String> {
        let state = self.state();
        state.lines.as_ref()?.try_recv().ok()
    }

    /// The typed lines that could not be handed over.
    pub fn pending(&self) -> Vec<String> {
        lock(&self.state).pending.clone()
    }
}

/// The lines a session holds for Lich.
///
/// **ESTABLISHED** (`0x101a88554`): `Typing::waiting` is a
/// `Vec::from_iter(...)`, so it materialises the queue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Typing {
    /// the lines still to go
    pub pending: Vec<String>,
}

impl Typing {
    /// A typing queue holding `pending`.
    pub fn new(pending: Vec<String>) -> Self {
        Typing { pending }
    }

    /// The lines that are waiting.
    pub fn waiting(&self) -> Vec<String> {
        self.pending.clone()
    }

    /// How many lines are waiting.
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// Is nothing waiting?
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Add a line to the back of the queue.
    pub fn push(&mut self, line: impl Into<String>) {
        self.pending.push(line.into());
    }

    /// Take the whole queue.
    pub fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending)
    }
}

/// What a `LichDoor::claim` hands back: the two ends of the relay.
///
/// The relay pushes everything Lich writes into `lines`, and pulls the lines the
/// player typed out of `typed` to write them to the child's stdin.
#[derive(Debug)]
pub struct Claim {
    /// lines Lich sent back (push these; the door parses them)
    pub lines: Sender<String>,
    /// typed lines for Lich (take these and write them to the child's stdin)
    pub typed: Receiver<String>,
    /// the "fell behind" flag for the mirrored stream
    pub wire: Wire,
}

/// The door the Lich relay talks to.
///
/// **ESTABLISHED**: every method forwards to the session handle (the binary
/// tail-calls `command::SessionHandle::say` from `LichDoor::say`, so the door
/// and the handle share their state).
#[derive(Debug, Clone)]
pub struct LichDoor {
    session: SessionHandle,
}

impl LichDoor {
    /// A door onto `session`.
    pub fn new(session: SessionHandle) -> Self {
        LichDoor { session }
    }

    /// The session behind the door.
    pub fn session(&self) -> &SessionHandle {
        &self.session
    }

    /// Open the relay: create the two channels and install them on the slot.
    ///
    /// **ESTABLISHED** (`0x101a88960`): a 4096-slot channel for the lines Lich
    /// sends back, a 64-slot channel for the typed lines going in, and two
    /// small shared cells.
    pub fn claim(&self) -> Claim {
        let (line_tx, line_rx) = channel();
        let (typed_tx, typed_rx) = channel();
        let mut slot = self.session.slot();
        slot.close();
        slot.set_wire(Tap::new(typed_tx.clone()).waiting_for());
        // The hand tap takes the player's typing and forwards it to Lich.
        slot.set_hand(Tap::new(typed_tx).taking_typing());
        let wire = slot.wire_flag().unwrap_or_default();
        {
            let mut state = slot.state();
            state.lines = Some(line_rx);
            // A fresh relay means the session is not over any more.
            state.session_over = false;
        }
        drop(slot);
        Claim {
            lines: line_tx,
            typed: typed_rx,
            wire,
        }
    }

    /// Take the next line the relay has pushed, if there is one.
    pub fn poll(&self) -> Option<String> {
        self.session.slot().take_line()
    }

    /// Say something to the player.
    ///
    /// **ESTABLISHED** (`0x101a88608`): a tail call into
    /// `SessionHandle::say`.
    pub fn say(&self, notice: &Notice) {
        self.session.say(notice);
    }

    /// The command symbol, `.` when the session has none.
    ///
    /// **ESTABLISHED** (`0x101a89030`): `SessionHandle::symbol` with the
    /// `Option<char>` niche mapped to `0x2e`.
    pub fn symbol(&self) -> char {
        self.session.symbol().unwrap_or(crate::claimant::DEFAULT_SYMBOL)
    }

    /// Lich's command symbol.
    ///
    /// **ESTABLISHED** (`0x101a88558`): a tail call into
    /// `SessionHandle::lich_symbol`; `None` means Lich has not said yet.
    pub fn lich_symbol(&self) -> Option<char> {
        self.session.lich_symbol()
    }

    /// Has the session ended?
    ///
    /// **ESTABLISHED** (`0x101a8855c`): the bool at `+0x68` of the shared
    /// state, read under its lock.
    pub fn session_over(&self) -> bool {
        self.session.slot().session_over()
    }

    /// Mark the session as over.
    pub fn over(&self) {
        self.session.slot().close();
    }

    /// Has Lich fallen behind the game stream?
    ///
    /// **ESTABLISHED**: `Wire::fell_behind` through the slot.
    pub fn fell_behind(&self) -> bool {
        match self.session.slot().wire_flag() {
            Some(wire) => wire.fell_behind(),
            None => false,
        }
    }

    /// Take one line of Lich's output and fold the frames it produces into the
    /// session.
    ///
    /// **ESTABLISHED** (`0x101a8860c`): read the line through the shared text
    /// parser (`lich::text::LichText::read`) into a `Vec<Frame>`, then
    /// `SessionHandle::fold_in`.  The parser itself is not in this crate's
    /// symbol table (it is `hydra_session::lich::text`, see the
    /// `crates/hydra-session/src/lich/text.rs` path in `__TEXT`), so we take the
    /// line's own text as the frame.
    pub fn show(&self, line: &str) {
        let frames: Vec<String> = line
            .split(|c| c == '\n' || c == '\r')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        self.session.fold_in(frames);
    }

    /// Announce that the relay started.
    pub fn started(&self) {
        self.session.say_line(LEVEL_INFO, "Lich started.");
    }
}

impl SessionHandle {
    /// The door for this session.
    ///
    /// **ESTABLISHED** (`0x101a3923c`): builds the door from the session.
    pub fn lich_door(&self) -> LichDoor {
        LichDoor::new(self.clone())
    }

    /// Is a Lich runner attached?
    ///
    /// **ESTABLISHED** (`0x101a38f34`): locks the slot and asks the hand
    /// channel whether it is closed.
    pub fn lich_running(&self) -> bool {
        let slot = self.slot();
        let mut state = slot.state();
        let hand = state.hand.is_some();
        if !hand {
            state.hand = None;
        }
        hand
    }

    /// Does Lich take the player's typing?
    ///
    /// **ESTABLISHED** (`0x101a39040`): the same channel check as
    /// [`Slot::takes_typing`].
    pub fn lich_takes_typing(&self) -> bool {
        self.slot().takes_typing()
    }

    /// Hand one line to Lich.  `false` when nothing took it.
    ///
    /// **ESTABLISHED** (`0x101a38d58`): tries the hand tap, and when that
    /// fails says one of the two recovered messages.
    pub fn hand_to_lich(&self, line: &str) -> bool {
        let slot = self.slot();
        if !slot.takes_typing() {
            drop(slot);
            self.say_line(
                LEVEL_INFO,
                format!("Lich is not running for this character: not sent: {line}"),
            );
            return false;
        }
        if slot.hand(line) {
            return true;
        }
        let waiting = slot.pending().len();
        drop(slot);
        self.say_line(
            LEVEL_WARN,
            format!(
                "Lich is not reading what is typed for it ({waiting} lines wait): not handed: \
                 {line}"
            ),
        );
        false
    }

    /// Hand one line to Lich, quietly.
    ///
    /// **ESTABLISHED** (`0x101a39154`): `Slot::hand` with no notice.
    pub fn to_lich(&self, line: &str) -> bool {
        self.slot().hand(line)
    }

    /// Mirror one game line to Lich.
    pub fn wire_to_lich(&self, line: &str) -> bool {
        self.slot().wire(line)
    }

    /// Put a line in the queue for Lich.
    pub fn queue_for_lich(&self, line: &str) {
        self.slot().state().pending.push(line.to_string());
    }

    /// The lines queued for Lich.
    pub fn lich_pending(&self) -> Typing {
        Typing::new(self.slot().pending())
    }

    /// Tell the session Lich has gone.
    ///
    /// **ESTABLISHED** text: `Lich stopped.` at level 2 (see `Ended::notice` in
    /// `hydra_lich`); the slot is closed first so no further lines are handed
    /// over.
    pub fn lich_gone(&self) {
        self.slot().close();
        self.say_line(LEVEL_INFO, "Lich stopped.");
        self.emit(Event::Over("Lich stopped.".to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claimant::Desk;
    use crate::notice::LEVEL_INFO;

    fn session() -> SessionHandle {
        SessionHandle::new(Desk::new(None, "Aelith"))
    }

    #[test]
    fn the_door_maps_a_missing_symbol_to_dot() {
        let s = session();
        let door = s.lich_door();
        assert_eq!(door.symbol(), '.');
        s.set_symbol('!');
        assert_eq!(door.symbol(), '!');
        assert_eq!(door.lich_symbol(), Some(';'));
        assert!(!door.session_over());
    }

    #[test]
    fn the_door_says_through_the_session() {
        let s = session();
        let door = s.lich_door();
        door.say(&Notice::line(LEVEL_INFO, "from lich"));
        assert_eq!(s.said(), vec!["from lich".to_string()]);
    }

    #[test]
    fn claiming_installs_the_taps() {
        let s = session();
        let door = s.lich_door();
        let claim = door.claim();
        assert!(s.lich_running());
        assert!(s.lich_takes_typing());
        assert!(!door.session_over());
        assert!(s.to_lich("hello"));
        assert_eq!(claim.typed.try_recv().unwrap(), "hello");
        assert!(claim.lines.send("back".to_string()).is_ok());
        assert_eq!(door.poll().as_deref(), Some("back"));
        assert_eq!(door.poll(), None);
        assert!(!door.fell_behind());
    }

    #[test]
    fn hand_to_lich_says_the_recovered_text_when_nothing_is_there() {
        let s = session();
        assert!(!s.hand_to_lich("go north"));
        assert_eq!(
            s.said(),
            vec!["Lich is not running for this character: not sent: go north".to_string()]
        );
    }

    #[test]
    fn a_dropped_reader_makes_the_hand_report_lines_waiting() {
        let s = session();
        let door = s.lich_door();
        let claim = door.claim();
        drop(claim.typed);
        let _ = &claim.lines;
        for line in ["a", "b"] {
            s.queue_for_lich(line);
        }
        assert!(!s.hand_to_lich("c"));
        let said = s.said();
        assert_eq!(
            said,
            vec!["Lich is not reading what is typed for it (2 lines wait): not handed: c"
                .to_string()]
        );
        let _ = claim.lines;
    }

    #[test]
    fn closing_the_slot_marks_the_session_over() {
        let s = session();
        let door = s.lich_door();
        let _claim = door.claim();
        door.over();
        assert!(door.session_over());
        assert!(!s.lich_running());
    }

    #[test]
    fn show_folds_the_lines_into_the_session() {
        let s = session();
        let door = s.lich_door();
        let mut listener = s.subscribe();
        door.show("one\r\ntwo");
        match listener.try_recv() {
            Some(Event::Frames(frames)) => assert_eq!(frames, vec!["one", "two"]),
            other => panic!("wanted frames, got {other:?}"),
        }
    }

    #[test]
    fn wire_reports_falling_behind() {
        let slot = Slot::new();
        let (tx, rx) = channel();
        slot.set_wire(Tap::new(tx).waiting_for());
        assert!(slot.waits());
        assert!(slot.wire("a line"));
        assert_eq!(rx.try_recv().unwrap(), "a line");
        drop(rx);
        assert!(!slot.wire("lost"));
        assert!(!slot.waits(), "the failed send drops the tap");
    }

    #[test]
    fn begin_reports_whether_a_wire_was_open() {
        let mut slot = Slot::new();
        assert!(!slot.begin("hello"));
        let (tx, rx) = channel();
        slot.set_wire(Tap::new(tx));
        assert!(slot.begin("hello"));
        assert_eq!(rx.try_recv().unwrap(), "hello");
    }

    #[test]
    fn typing_queue() {
        let mut t = Typing::default();
        assert!(t.is_empty());
        t.push("a");
        t.push("b");
        assert_eq!(t.len(), 2);
        assert_eq!(t.waiting(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(t.take(), vec!["a".to_string(), "b".to_string()]);
        assert!(t.is_empty());
    }
}
