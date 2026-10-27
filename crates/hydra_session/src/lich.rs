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
//! **Phase 1**: the door now carries the real `tokio::sync::mpsc` channels too.
//! [`Tap`] can hold either a `std::sync::mpsc::Sender` (the synchronous
//! [`LichDoor::claim`], unchanged) or a `tokio::sync::mpsc::Sender` (the new
//! [`LichDoor::claim_async`], with the recovered capacities [`LINES`] and
//! [`TYPED`]); [`Tap::send`] prefers the tokio end when both are installed, and
//! a full or closed tokio channel trips the same "fell behind" flag a failed
//! `mpsc` send does.  The synchronous `Claim` is kept because that is the shape
//! `hydra_lich`'s relay and this crate's tests use.
//!
//! The token and password handshake live in `hydra_lich` (`new_token` produces
//! the 64-character token).  The observed direction is: Hydra **listens** on
//! `127.0.0.1:<ephemeral>`, Lich is started with `-g 127.0.0.1:<port>` and
//! connects back, so the accept runs on the server side (see
//! [`accept_keyed`]/[`client_handshake`] and `docs/LICH_WIRE.md`).  This crate
//! carries the door plus the small protocol helpers the session itself needs;
//! the relay proper is `hydra_lich`.  See `NOTES.md` §"Lich door contract".

use crate::command::{Event, SessionHandle};
use crate::notice::{Notice, LEVEL_INFO, LEVEL_WARN};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::mpsc as tokio_mpsc;

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
    /// the tokio end, when the relay was claimed with
    /// [`LichDoor::claim_async`]; it wins over `tx` when it is installed
    pub tx_async: Option<tokio_mpsc::Sender<String>>,
    /// the receiver that keeps `tx` alive on a tokio tap (so the `std` end is
    /// a working channel rather than a dead one)
    pub keep: Option<Receiver<String>>,
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
            tx_async: None,
            keep: None,
            wire: Wire::new(),
            typing: false,
            waiting: false,
        }
    }

    /// A tap that sends on a tokio channel.
    ///
    /// **ESTABLISHED** channel capacities: [`LINES`] for what Lich sends back
    /// and [`TYPED`] for what the player types.  A `std` sender is still
    /// created (and its receiver kept by the slot) so the `tx` field stays a
    /// real channel; `tx_async` is what `send` uses.
    pub fn async_(tx: tokio_mpsc::Sender<String>) -> Self {
        let (plain, rx) = channel();
        Tap {
            tx: plain,
            tx_async: Some(tx),
            keep: Some(rx),
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

    /// Send one line; `false` when the other end is gone or full.
    ///
    /// **ESTABLISHED** (`0x101a88070`): the original does `try_acquire(1)` on
    /// the channel's semaphore and then sends, so a full channel counts as a
    /// failed send; a tokio `try_send` is the same test.
    pub fn send(&self, line: &str) -> bool {
        if let Some(tx) = &self.tx_async {
            if tx.try_send(line.to_string()).is_err() {
                self.wire.begin();
                return false;
            }
            return true;
        }
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
    /// the same, for a relay claimed with [`LichDoor::claim_async`]
    pub lines_async: Option<tokio_mpsc::Receiver<String>>,
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
    ///
    /// Both ends are tried: the synchronous channel [`LichDoor::claim`] installs
    /// and the tokio channel [`LichDoor::claim_async`] installs.
    pub fn take_line(&self) -> Option<String> {
        let mut state = self.state();
        if let Some(rx) = state.lines.as_ref() {
            if let Ok(line) = rx.try_recv() {
                return Some(line);
            }
        }
        if let Some(rx) = state.lines_async.as_mut() {
            if let Ok(line) = rx.try_recv() {
                return Some(line);
            }
        }
        None
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

/// What [`LichDoor::claim_async`] hands back: the same two ends, on tokio
/// channels with the recovered capacities ([`LINES`] and [`TYPED`]).
#[derive(Debug)]
pub struct AsyncClaim {
    /// lines Lich sent back (push these; the door parses them)
    pub lines: tokio_mpsc::Sender<String>,
    /// typed lines for Lich (take these and write them to the child's stdin)
    pub typed: tokio_mpsc::Receiver<String>,
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

    /// Open the relay on tokio channels.
    ///
    /// **ESTABLISHED** (`0x101a88960`): the same two channels with the same
    /// capacities, but the real `tokio::sync::mpsc` types the binary used, so a
    /// relay task can `recv().await` on the typed end and `send(..).await` on
    /// the line end.
    pub fn claim_async(&self) -> AsyncClaim {
        let (line_tx, line_rx) = tokio_mpsc::channel::<String>(LINES);
        let (typed_tx, typed_rx) = tokio_mpsc::channel::<String>(TYPED);
        let mut slot = self.session.slot();
        slot.close();
        slot.set_wire(Tap::async_(typed_tx.clone()).waiting_for());
        slot.set_hand(Tap::async_(typed_tx).taking_typing());
        let wire = slot.wire_flag().unwrap_or_default();
        {
            let mut state = slot.state();
            state.lines_async = Some(line_rx);
            state.session_over = false;
        }
        drop(slot);
        AsyncClaim {
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

/// The 60-second deadline the relay allows Lich to connect
/// (`hydra_lich/NOTES.md` §2.2: "did not connect within 60 seconds").
pub const ACCEPT_TIMEOUT: Duration = Duration::from_secs(60);

/// The client-identification line, exactly as observed on both channels
/// (`docs/LICH_WIRE.md` §2).  Duplicated from `hydra_lich::wire` because this
/// crate must not depend on the relay crate; keep the two spellings identical.
pub const CLIENT_IDENTIFICATION: &str = "/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML";

/// The byte Lich puts in front of every forwarded game chunk on its stdout
/// (US, 0x1F).  Lich's own lines are unprefixed.
pub const GAME_CHUNK_PREFIX: u8 = 0x1F;

/// The Stormfront prefix on a command Lich originated (`<c>exp`); text the
/// player typed and Lich forwarded is the bare line (`look`).
pub const COMMAND_TAG: &str = "<c>";

/// Longest handshake line accepted (the real ones are 64 and 51 bytes).
pub const MAX_HANDSHAKE_LINE: usize = 4096;

/// Hydra's side of the keyed accept, **observed** (`docs/LICH_WIRE.md` §2).
///
/// Hydra mints a 64-character token (`hydra_lich::child::new_token`), binds
/// `127.0.0.1:<ephemeral>`, starts Lich with
/// `--pipe --stormfront --sentinel -g 127.0.0.1:<port>` and **listens**.  Lich
/// connects and the first two lines of the stream must be
///
/// ```text
/// <token>\n
/// /FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n
/// ```
///
/// i.e. the same two lines Hydra wrote to Lich's stdin right after the spawn
/// (see [`client_handshake`]).  Any other content is a refusal; the caller
/// drops the stream, which closes it.  The token is never logged.
///
/// This is the *server* half.  [`client_handshake`] is Lich's half, used by the
/// fake Lich in tests and by anything that needs to prove itself to an accept.
pub fn accept_keyed<S: Read>(stream: &mut S, token: &str) -> io::Result<()> {
    let first = read_line(stream)?;
    if first != token {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Lich's first line was not this session's token",
        ));
    }
    let second = read_line(stream)?;
    if second != CLIENT_IDENTIFICATION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Lich's second line was not the client identification line",
        ));
    }
    Ok(())
}

/// Lich's side of the handshake: write the two lines and flush.
///
/// This is what Hydra sends to Lich's **stdin** immediately after the spawn
/// (`docs/LICH_WIRE.md` §2, step 1) and what Lich then sends back as the first
/// bytes of the socket stream.  It is kept here for tests and fake Lich
/// helpers; Hydra itself no longer *sends* it on a socket.
pub fn client_handshake<S: Write>(stream: &mut S, token: &str) -> io::Result<()> {
    stream.write_all(&handshake_bytes(token))?;
    stream.flush()
}

/// The exact bytes of the two-line handshake (`<token>\n<client line>\n`).
pub fn handshake_bytes(token: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(token.len() + CLIENT_IDENTIFICATION.len() + 2);
    bytes.extend_from_slice(token.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(CLIENT_IDENTIFICATION.as_bytes());
    bytes.push(b'\n');
    bytes
}

/// Read one `\n`-terminated line, stripping the `\n` and a preceding `\r`.
///
/// One byte at a time, so the bytes after the handshake are not swallowed by a
/// buffer the caller cannot see.
fn read_line<S: Read>(stream: &mut S) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {
                if byte[0] == b'\n' {
                    break;
                }
                line.push(byte[0]);
                if line.len() > MAX_HANDSHAKE_LINE {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Lich sent an over-long handshake line",
                    ));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// Connect to `127.0.0.1:<port>` and perform Lich's half of the handshake.
///
/// The address is always `127.0.0.1:<port>` (**ESTABLISHED**: the relay's
/// `build_command` passes `-g 127.0.0.1:<port>`).  The connection, read and
/// write all use [`ACCEPT_TIMEOUT`] (60 s).  Used by the fake Lich and by
/// tests; Hydra accepts, it does not connect.
pub fn connect_keyed(port: u16, token: &str) -> io::Result<TcpStream> {
    connect_keyed_to(SocketAddr::from(([127, 0, 0, 1], port)), token)
}

/// [`connect_keyed`] for an explicit address (tests use an ephemeral port).
pub fn connect_keyed_to(addr: SocketAddr, token: &str) -> io::Result<TcpStream> {
    let mut stream = TcpStream::connect_timeout(&addr, ACCEPT_TIMEOUT)?;
    stream.set_read_timeout(Some(ACCEPT_TIMEOUT))?;
    stream.set_write_timeout(Some(ACCEPT_TIMEOUT))?;
    client_handshake(&mut stream, token)?;
    Ok(stream)
}

/// `true` when Lich prefixed this stdout line with [`GAME_CHUNK_PREFIX`].
pub fn has_game_chunk_prefix(line: &str) -> bool {
    line.starts_with(GAME_CHUNK_PREFIX as char)
}

/// The forwarded game chunk with its 0x1F prefix removed, or `None` when the
/// line is Lich's own output (banner, `<output …/>` messages, errors).
pub fn strip_game_chunk_prefix(line: &str) -> Option<&str> {
    line.strip_prefix(GAME_CHUNK_PREFIX as char)
}

/// One line Lich sent up the socket, split by provenance.
///
/// `<c>exp` is a command Lich (or a script) originated; `look` is text the
/// player typed that Lich forwarded; `_injury 2` is a bare protocol line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LichFrame {
    /// `<c>…` — the command without the prefix.
    Command(String),
    /// A bare line, unchanged.
    Bare(String),
}

impl LichFrame {
    /// Classify one line, ignoring any trailing `\n`/`\r`.
    pub fn parse(line: &str) -> Self {
        let line = line.trim_end_matches(['\n', '\r']);
        match line.strip_prefix(COMMAND_TAG) {
            Some(command) => LichFrame::Command(command.to_string()),
            None => LichFrame::Bare(line.to_string()),
        }
    }

    /// The line without its `<c>` prefix and without the line ending.
    pub fn text(&self) -> &str {
        match self {
            LichFrame::Command(text) | LichFrame::Bare(text) => text,
        }
    }

    /// Was this a Lich-originated command?
    pub fn is_command(&self) -> bool {
        matches!(self, LichFrame::Command(_))
    }

    /// Render the line back to its wire form (no line ending added).
    pub fn to_wire(&self) -> String {
        match self {
            LichFrame::Command(text) => command_frame(text),
            LichFrame::Bare(text) => text.clone(),
        }
    }
}

/// Build a `<c>` command frame: `command_frame("exp") == "<c>exp"`.
pub fn command_frame(command: &str) -> String {
    format!("{COMMAND_TAG}{command}")
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

    const TOKEN: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_handshake_is_the_two_observed_lines() {
        assert_eq!(
            handshake_bytes(TOKEN),
            format!("{TOKEN}\n/FE:WRAYTH /VERSION:1.0.1.28 /P:WIN_UNKNOWN /XML\n").into_bytes()
        );
        let mut wire = std::io::Cursor::new(Vec::new());
        client_handshake(&mut wire, TOKEN).unwrap();
        assert_eq!(wire.into_inner(), handshake_bytes(TOKEN));
        assert_eq!(ACCEPT_TIMEOUT, Duration::from_secs(60));
    }

    #[test]
    fn accept_keyed_reads_the_token_then_the_client_line() {
        let mut wire = std::io::Cursor::new(handshake_bytes(TOKEN));
        accept_keyed(&mut wire, TOKEN).unwrap();
        // Only the two handshake lines were consumed: nothing is left over for
        // the game stream reader that follows.
        let mut rest = Vec::new();
        wire.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty());
    }

    #[test]
    fn accept_keyed_tolerates_crlf() {
        let mut wire = std::io::Cursor::new(
            format!("{TOKEN}\r\n{CLIENT_IDENTIFICATION}\r\n").into_bytes(),
        );
        accept_keyed(&mut wire, TOKEN).unwrap();
    }

    #[test]
    fn accept_keyed_refuses_a_wrong_token() {
        let mut wire = std::io::Cursor::new(handshake_bytes(&"f".repeat(64)));
        let err = accept_keyed(&mut wire, TOKEN).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert!(!err.to_string().contains(TOKEN), "the token must not leak");
    }

    #[test]
    fn accept_keyed_refuses_a_wrong_client_line() {
        let mut wire = std::io::Cursor::new(
            format!("{TOKEN}\n/FE:WRAYTH /VERSION:0.0.0 /XML\n").into_bytes(),
        );
        let err = accept_keyed(&mut wire, TOKEN).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn accept_keyed_refuses_a_truncated_handshake() {
        let mut wire = std::io::Cursor::new(format!("{TOKEN}\n").into_bytes());
        let err = accept_keyed(&mut wire, TOKEN).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn connect_keyed_performs_lichs_half_over_loopback() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0u16)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let accept = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            accept_keyed(&mut stream, TOKEN)
        });
        let stream = connect_keyed(port, TOKEN).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        accept.join().unwrap().unwrap();
    }

    #[test]
    fn framing_separates_c_commands_from_bare_lines() {
        assert_eq!(
            LichFrame::parse("<c>locker info\n"),
            LichFrame::Command("locker info".to_string())
        );
        assert_eq!(LichFrame::parse("look\n"), LichFrame::Bare("look".into()));
        assert_eq!(
            LichFrame::parse("_injury 2\n"),
            LichFrame::Bare("_injury 2".into())
        );
        assert_eq!(command_frame("exp"), "<c>exp");
        assert_eq!(
            LichFrame::Command("exp".into()).to_wire(),
            "<c>exp"
        );
        assert!(!LichFrame::parse("look").is_command());
    }

    #[test]
    fn the_game_chunk_prefix_is_recognised_and_stripped() {
        let chunk = format!("{}<room>1</room>", GAME_CHUNK_PREFIX as char);
        assert!(has_game_chunk_prefix(&chunk));
        assert_eq!(strip_game_chunk_prefix(&chunk), Some("<room>1</room>"));
        let own = "<output class=\"mono\"/>Lich is active.<output class=\"\"/>";
        assert!(!has_game_chunk_prefix(own));
        assert_eq!(strip_game_chunk_prefix(own), None);
        assert_eq!(GAME_CHUNK_PREFIX, 0x1F);
    }

    #[tokio::test]
    async fn the_async_claim_carries_typed_lines_on_a_tokio_channel() {
        let s = session();
        let door = s.lich_door();
        let mut claim = door.claim_async();
        assert!(s.lich_running());
        assert!(s.lich_takes_typing());
        assert!(!door.session_over());
        // typed lines arrive on the tokio receiver, in order
        assert!(s.to_lich("hello"));
        assert!(s.to_lich("again"));
        assert_eq!(claim.typed.recv().await.unwrap(), "hello");
        assert_eq!(claim.typed.recv().await.unwrap(), "again");
        // lines Lich sends back are seen by the door
        claim.lines.send("back".to_string()).await.unwrap();
        assert_eq!(door.poll().as_deref(), Some("back"));
        assert_eq!(door.poll(), None);
        assert!(!door.fell_behind());
        // the tokio channels have the recovered capacities
        assert_eq!(claim.lines.max_capacity(), LINES);
        assert_eq!(claim.typed.max_capacity(), TYPED);
    }

    #[tokio::test]
    async fn a_full_tokio_channel_trips_the_fell_behind_flag() {
        let s = session();
        let door = s.lich_door();
        let claim = door.claim_async();
        // The typed channel holds TYPED lines; one more is refused, which is
        // the recovered `try_acquire` failure.
        let mut sent = 0;
        while s.to_lich("line") {
            sent += 1;
            assert!(sent <= TYPED, "the channel took more than its capacity");
        }
        assert_eq!(sent, TYPED, "the typed channel holds exactly TYPED lines");
        // Dropping the reader makes every later hand fail too.
        drop(claim);
        assert!(!s.to_lich("after"));
    }
}
