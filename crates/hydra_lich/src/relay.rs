//! The Lich relay: pending typed lines, notices, line readers and the
//! child-process lifecycle.
//!
//! The original crate is inherently `tokio`-based.  Phase 1 reconnected the
//! real crates, so this module now has two layers over the same recovered
//! semantics:
//!
//! * **tokio-native (new)**: [`read_lines_async`] (`tokio::io::BufReader` ->
//!   `tokio::sync::mpsc::channel(64)`), [`keep_errors_async`] (last
//!   [`KEPT_ERRORS`] lines under a `tokio::sync::Mutex`), [`spawn_async`] /
//!   [`build_command_async`] (`tokio::process`), [`RelayAsync`] and
//!   [`run_async`] / [`run_until`] (a real `tokio::select!` loop with a
//!   `tokio_util::sync::CancellationToken`).
//! * **synchronous facade (kept)**: [`read_lines`], [`keep_errors`], [`spawn`],
//!   [`show`], [`run`] and [`Relay`], with the same signatures and observable
//!   ordering as before, driven by [`crate::runtime`].
//!
//! `Waits`, [`Notice`], [`Typed`], [`never_given`], [`shared_symbol`] and
//! [`Ended`] are unchanged value types shared by both layers.
//!
//! **Observed wire protocol** (`docs/LICH_WIRE.md`, see [`crate::wire`]): Hydra
//! listens on `127.0.0.1:<ephemeral>` and Lich (`-g 127.0.0.1:<port>`) connects
//! back.  [`start_keyed`] performs the whole sequence — bind, spawn, write the
//! two-line stdin handshake, accept and validate the two-line socket handshake
//! with the recovered 60-second deadline — and returns the relay plus the
//! accepted socket.  [`show`] keeps its old signature (spawn + wire the pipes,
//! no socket) for callers and tests that do not speak the handshake.
//!
//! See `NOTES.md` for the recovered wire protocol, the established-vs-UNKNOWN
//! split and the Phase 1 reconnection table.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, ReadBuf};
use tokio_util::sync::CancellationToken;

/// Bounded capacity of the child-stdout line channel (`mpsc::channel(0x40)`
/// in `read_lines` @ `0x10167dcc0`).
pub const LINE_CHANNEL_CAPACITY: usize = 64;

/// Number of stderr lines kept by `keep_errors` (the closure at
/// `0x10167aa64` pops the front whenever the deque length reaches `0x14`).
pub const KEPT_ERRORS: usize = 20;

/// `Waits::new` stores `Instant::now() + Duration::from_secs(10)`.
///
/// UNKNOWN: none of `tell`/`typed`/`hand_held` read this deadline in the
/// binary; the relay select loop probably uses it as a grace period for the
/// child to announce the character.  It is exposed via [`Waits::deadline`].
pub const HAND_HOLD_GRACE: Duration = Duration::from_secs(10);

/// First `-e` expression passed to Ruby (`0x103982c25`, length 78).
pub const RUBY_PRELUDE: &str =
    r#"trace_var(:$frontend) { |name| $frontend = "stormfront" if name == "unknown" }"#;

/// Second `-e` expression (`0x103982c73`, length 24).
pub const RUBY_LOAD_LICH: &str = "$0 = ARGV.shift; load $0";

/// Fixed arguments after the Lich script path, in recovered order:
/// `--pipe`, `--stormfront`, `--sentinel`, `-g`
/// (`0x103982c8b`, `0x103982c91`, `0x103982c9d`, `0x103982ca7`).
pub const LICH_ARGS: [&str; 4] = ["--pipe", "--stormfront", "--sentinel", "-g"];

/// A user-visible notice.
///
/// Recovered layout details from `Waits::tell` / `Ended::notice`: a `kind`
/// word at +0, a `Vec<String>` at +8 (`cap`, `ptr`, `len`) and two byte flags
/// at +0x38/+0x39.  The exact meaning of the flags is not visible in the
/// disassembly; `+0x39` is a level/verbosity (`1` for relay warnings, `2`
/// for "Lich stopped"), and `+0x38` is always `false` in the recovered
/// constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Enum-like tag at +0.  `1` is used by manually built relay notices
    /// (tell/shared-symbol), `2` by `Notice::line(2, …)` (login hint), and
    /// `4` doubles as the "no notice" discriminant.
    pub kind: u32,
    /// Message lines (the recovered notices always contain exactly one).
    pub lines: Vec<String>,
    /// Byte at +0x38; always false in the recovered constructors.
    /// UNKNOWN: meaning.
    pub logged: bool,
    /// Byte at +0x39; `1` = relay warning, `2` = "Lich stopped"/exit.
    /// UNKNOWN: exact enum name.
    pub level: u8,
}

impl Notice {
    /// Construct a notice with one line; mirrors `Notice::line(kind, text)`.
    pub fn line(kind: u32, text: impl Into<String>) -> Self {
        Self {
            kind,
            lines: vec![text.into()],
            logged: false,
            level: 0,
        }
    }

    /// Construct a notice from several lines.
    pub fn from_lines(kind: u32, lines: Vec<String>) -> Self {
        Self {
            kind,
            lines,
            logged: false,
            level: 0,
        }
    }

    /// Set the `+0x39` level byte (builder style).
    pub fn with_level(mut self, level: u8) -> Self {
        self.level = level;
        self
    }

    /// The lines joined with `\n`, convenient for tests and display.
    pub fn text(&self) -> String {
        self.lines.join("\n")
    }
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text())
    }
}

/// One branch of [`Waits::typed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// Lich has not confirmed the character yet: the line was queued.  The
    /// notice is `Some` only when this was the first queued line, matching
    /// the binary's `len == 0` check before pushing.
    Queued(Option<Notice>),
    /// `hand_held` was armed: the line was handed straight to Lich.
    Handed(String),
}

/// Pending typed lines and the "hand held" / "tell" state.
///
/// Recovered layout from `Waits::new` @ `0x10167e430`:
/// `Vec<String>` at +0, deadline at +0x18/+0x20, a pointer/flag from the
/// shared session state at +0x28/+0x30, `hand_held` at +0x38 and `tell` at
/// +0x39.  The std-only version drops the opaque session pointers and keeps
/// the queue/flag semantics that are visible in `typed`/`tell`/`hand_held`.
#[derive(Debug)]
pub struct Waits {
    pending: Vec<String>,
    deadline: Instant,
    hand_held: bool,
    told: bool,
}

impl Waits {
    /// New queue.  `hand_held` mirrors a flag read under the shared
    /// `RwLock<bool>` in the binary; when true, typed lines are handed to
    /// Lich immediately instead of being queued.
    pub fn new(hand_held: bool) -> Self {
        Self::with_deadline(hand_held, Instant::now() + HAND_HOLD_GRACE)
    }

    /// New queue with an explicit deadline (test seam).
    pub fn with_deadline(hand_held: bool, deadline: Instant) -> Self {
        Self {
            pending: Vec::new(),
            deadline,
            hand_held,
            told: false,
        }
    }

    /// Deadline stored at +0x18/+0x20.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Whether lines are handed immediately.
    pub fn is_hand_held(&self) -> bool {
        self.hand_held
    }

    /// Arm the hold (the shared flag is copied into `hand_held` at
    /// construction; the relay loop flips it later).
    pub fn set_hand_held(&mut self, hand_held: bool) {
        self.hand_held = hand_held;
    }

    /// Whether [`Waits::tell`] has already spoken.
    pub fn has_told(&self) -> bool {
        self.told
    }

    /// Lines currently waiting.
    pub fn pending(&self) -> &[String] {
        &self.pending
    }

    /// Queue or hand a typed line.
    ///
    /// Recovered (`Waits::typed` @ `0x10167e668`):
    /// * if `hand_held` (+0x38 bit 0): call `hand_typed` immediately and
    ///   consume the line — no notice;
    /// * otherwise, if the queue is empty, say
    ///   "Lich is still logging in; what you typed follows once it knows the
    ///   character." (`Notice::line(2, …)`) and then push;
    /// * otherwise push silently.
    pub fn typed(&mut self, line: String) -> Typed {
        if self.hand_held {
            return Typed::Handed(line);
        }
        let was_empty = self.pending.is_empty();
        self.pending.push(line);
        if was_empty {
            Typed::Queued(Some(Notice::line(
                2,
                "Lich is still logging in; what you typed follows once it knows the character.",
            )))
        } else {
            Typed::Queued(None)
        }
    }

    /// Report that Lich has not yet acknowledged the character.
    ///
    /// Recovered (`Waits::tell` @ `0x10167e58c`): always sets the `tell`
    /// flag (+0x39 = 1); if the queue is non-empty, speaks
    /// `Lich has not said it knows the character yet: {n} line(s) wait, and
    /// go as soon as it does.` with a kind-1 notice.
    pub fn tell(&mut self) -> Option<Notice> {
        self.told = true;
        let n = self.pending.len();
        if n == 0 {
            None
        } else {
            Some(
                Notice::line(
                    1,
                    format!(
                        "Lich has not said it knows the character yet: {n} line(s) wait, and go as soon as it does."
                    ),
                )
                .with_level(1),
            )
        }
    }

    /// Drain and return every waiting line.
    ///
    /// Recovered (`Waits::hand_held` @ `0x10167e780`): does nothing unless
    /// the `hand_held` flag is set, then drains the whole `Vec` (via
    /// `Drain`) and hands each line, in FIFO order.  The flag itself is not
    /// cleared by the recovered function.
    pub fn hand_held(&mut self) -> Vec<String> {
        if self.hand_held {
            std::mem::take(&mut self.pending)
        } else {
            Vec::new()
        }
    }
}

/// Merge already-queued typed lines with lines recovered from Lich.
///
/// Recovered (`never_given` @ `0x10167de8c`): the function builds a
/// `Chain<vec::IntoIter<String>, vec::IntoIter<String>>` and collects it
/// with `Vec::from_iter` (the monomorphised `SpecFromIterNested::from_iter`
/// at `0x101681fc8`).  The first argument is a `Vec<String>` moved out of
/// the relay state; the second is a by-value collection whose pointer/length
/// are consumed as an `IntoIter`.
///
/// UNKNOWN: the caller's second argument comes from a task result typed
/// `Vec<Vec<u8>>` in one symbol alias, but the iterator element type is
/// unambiguously `String` (24-byte stride).  We model it as `Vec<String>`;
/// the observable behaviour is concatenation in order.
pub fn never_given(pending: Vec<String>, recovered: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(pending.len() + recovered.len());
    out.extend(pending);
    out.extend(recovered);
    out
}

/// Build the warning shown when Hydra and Lich share a command symbol.
///
/// Recovered (`shared_symbol` @ `0x10167decc`): compares the two `char`
/// arguments (`w0`/`w1`) and, when equal, formats
/// "Hydra's commands start with '{s}', as Lich's do, so a line typed with
/// '{s}' is Hydra's and Lich's commands can't be reached. Give Hydra
/// another command symbol, such as ., on the settings menu's General page."
/// The template at `0x103982ad0` contains two placeholders and a 4-byte
/// style escape (`c8 00 00 80 84`) that our plain-string equivalent drops.
/// Returns `None` when the symbols differ.
pub fn shared_symbol(symbol: char, lich_symbol: char) -> Option<Notice> {
    if symbol != lich_symbol {
        return None;
    }
    Some(
        Notice::line(
            1,
            format!(
                "Hydra's commands start with '{symbol}', as Lich's do, so a line typed with '{symbol}' is Hydra's and Lich's commands can't be reached. Give Hydra another command symbol, such as ., on the settings menu's General page."
            ),
        )
        .with_level(1),
    )
}

/// Why the Lich child stopped.
///
/// The message set is recovered from `Ended::notice` @ `0x10167e138`; the
/// discriminant mapping below is inferred from the branch structure and the
/// strings it selects.  `Finished` is the branch that writes the "no notice"
/// discriminant (4) and returns no message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// `Lich stopped.`
    Stopped,
    /// `Lich exited ({code}).` — `None` means terminated without an exit
    /// code (a signal on Unix).
    Exited(Option<i32>),
    /// `Lich did not connect within {seconds} seconds, and was stopped.`
    /// The binary passes the literal `60` (`mov w8, #0x3c`).
    DidNotConnect { seconds: u64 },
    /// `Lich fell {chunks} chunks behind the game, and was stopped: a stream
    /// with a hole in it would mislead its scripts.`
    FellBehind { chunks: u64 },
    /// A clean teardown that needs no message (kind 4 / `None`).
    Finished,
    /// `Lich already runs for this character.`
    AlreadyRunning,
    /// `Lich could not start: {message}`
    CouldNotStart(String),
}

impl Ended {
    /// Map a relay termination reason to the user-visible notice.
    ///
    /// Fully recovered message text and per-branch level bytes:
    /// * stopped -> level 2
    /// * exited / fell behind -> level 1
    /// * did not connect -> level 0
    /// * already running / could not start -> level 0
    /// * finished -> `None`
    pub fn notice(&self) -> Option<Notice> {
        match self {
            Ended::Stopped => Some(Notice::line(1, "Lich stopped.").with_level(2)),
            Ended::Exited(code) => {
                let code = match code {
                    Some(code) => code.to_string(),
                    // UNKNOWN: the binary formats the whole ExitStatus with
                    // Display; code None means a signal termination.
                    None => "signal".to_string(),
                };
                Some(Notice::line(1, format!("Lich exited ({code}).")).with_level(1))
            }
            Ended::DidNotConnect { seconds } => Some(Notice::line(
                1,
                format!("Lich did not connect within {seconds} seconds, and was stopped."),
            )),
            Ended::FellBehind { chunks } => Some(
                Notice::line(
                    1,
                    format!(
                        "Lich fell {chunks} chunks behind the game, and was stopped: a stream with a hole in it would mislead its scripts."
                    ),
                )
                .with_level(1),
            ),
            Ended::Finished => None,
            Ended::AlreadyRunning => Some(Notice::line(
                1,
                "Lich already runs for this character.",
            )),
            Ended::CouldNotStart(message) => {
                Some(Notice::line(1, format!("Lich could not start: {message}")))
            }
        }
    }
}

/// Read text lines from an async reader into a bounded channel of capacity
/// [`LINE_CHANNEL_CAPACITY`].
///
/// **Reconnected in Phase 1** (`tokio` 1.53.1).  Recovered (`read_lines` @
/// `0x10167dcc0`): a `tokio::io::BufReader` over the child's stdout, a bounded
/// `mpsc` of `String` with capacity 64, and a detached task that forwards
/// every line; the sender closes at EOF, which disconnects the receiver.
///
/// The binary's `BufReader` allocates `0x2000` bytes — tokio's default
/// capacity is exactly 8192, so [`tokio::io::BufReader::new`] is the faithful
/// equivalent.  Line splitting uses `read_until(b'\n')` rather than
/// `AsyncBufReadExt::lines` on purpose: the binary runs `String::from_utf8_lossy`
/// over the bytes, while `lines()` would return an `InvalidData` error on the
/// first invalid byte.  The delimiter and a preceding `\r` are stripped, like
/// `Lines`.
///
/// **Observed protocol** (`docs/LICH_WIRE.md` §4): Lich prefixes every
/// forwarded game chunk with the byte 0x1F.  That byte is stripped here, so a
/// consumer of this channel (the protocol parser) never sees it; use
/// [`read_lich_out_async`] when the game/Lich distinction itself matters.
pub async fn read_lines_async<R>(reader: R) -> tokio::sync::mpsc::Receiver<String>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    read_lines_mapped(reader, strip_game_chunk).await
}

/// Classify every stdout line as a forwarded game chunk or Lich's own output.
///
/// Same reader, same bounded channel and same CRLF handling as
/// [`read_lines_async`], but the receiver yields [`crate::wire::LichOut`] so a
/// caller can tell Lich's banner and `<output …/>` lines from the game XML it
/// relays.  Game chunks arrive with the 0x1F prefix already stripped.
pub async fn read_lich_out_async<R>(
    reader: R,
) -> tokio::sync::mpsc::Receiver<crate::wire::LichOut>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    read_lines_mapped(reader, |line| crate::wire::LichOut::parse(&line)).await
}

/// Shared body of [`read_lines_async`] and [`read_lich_out_async`]: split the
/// reader into CRLF-stripped lossy `String` lines and push them (bounded at
/// [`LINE_CHANNEL_CAPACITY`]) through `map`.
async fn read_lines_mapped<R, T, F>(reader: R, map: F) -> tokio::sync::mpsc::Receiver<T>
where
    R: AsyncRead + Unpin + Send + 'static,
    T: Send + 'static,
    F: Fn(String) -> T + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::channel(LINE_CHANNEL_CAPACITY);
    crate::runtime::spawn(async move {
        let mut reader = tokio::io::BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break,
                Ok(_) => {
                    strip_line_ending(&mut buf);
                    let line = String::from_utf8_lossy(&buf).into_owned();
                    // Bounded send: a slow consumer stalls the reader, exactly
                    // like the recovered `Semaphore::new(0x40)` channel.
                    if tx.send(map(line)).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

/// Remove the 0x1F chunk prefix from a stdout line, keeping Lich's own lines
/// untouched (they are already unprefixed).
fn strip_game_chunk(line: String) -> String {
    match crate::wire::strip_game_chunk_prefix(&line) {
        Some(text) => text.to_string(),
        None => line,
    }
}

/// Synchronous [`read_lines_async`]: a `std::io::BufRead` in, a
/// `std::sync::mpsc::Receiver` out.
///
/// The recovered signature is `fn(OwnedReadHalf) -> mpsc::Receiver<String>`
/// and the existing public API of this crate (and its tests) is synchronous, so
/// this facade keeps both while doing the actual work on tokio: the blocking
/// reader is adapted through [`BlockingReader`], the lines come from
/// [`read_lines_async`] (real tokio `BufReader` + bounded tokio channel), and
/// only the last hop back to `std::sync::mpsc` is a plain channel, kept in step
/// by one bridge thread.
///
/// The returned receiver disconnects when the reader reaches EOF or errors.
///
/// The 0x1F game-chunk prefix is stripped exactly as in [`read_lines_async`];
/// [`read_lich_out`] is the classifying variant.
pub fn read_lines<R>(reader: R) -> Receiver<String>
where
    R: BufRead + Send + 'static,
{
    let lines = crate::runtime::block_on(read_lines_async(BlockingReader::new(reader)));
    bridge_to_std(lines)
}

/// Synchronous [`read_lich_out_async`]: classify each stdout line as a game
/// chunk or Lich's own output while keeping the CRLF handling.
pub fn read_lich_out<R>(reader: R) -> Receiver<crate::wire::LichOut>
where
    R: BufRead + Send + 'static,
{
    let lines = crate::runtime::block_on(read_lich_out_async(BlockingReader::new(reader)));
    bridge_to_std(lines)
}

/// Move a tokio line channel onto a `std::sync::mpsc` receiver.
///
/// Both channels are bounded at [`LINE_CHANNEL_CAPACITY`], so the sync facade
/// applies the same backpressure as the tokio channel.  The bridge thread ends
/// when the tokio sender closes (EOF) or the std receiver is dropped.
fn bridge_to_std<T: Send + 'static>(mut lines: tokio::sync::mpsc::Receiver<T>) -> Receiver<T> {
    let (tx, rx) = mpsc::sync_channel(LINE_CHANNEL_CAPACITY);
    thread::spawn(move || {
        while let Some(line) = lines.blocking_recv() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// How many bytes [`BlockingReader`] reads per chunk from the blocking source.
const READ_CHUNK: usize = 8192;

/// How many chunks the adapter may run ahead of the async reader.
const CHUNK_BUFFER: usize = 2;

/// Adapts a blocking [`std::io::Read`] into a tokio [`AsyncRead`].
///
/// The original never needed this: every reader it fed to `read_lines` /
/// `keep_errors` was already a tokio resource (`tokio::process::ChildStdout`).
/// Our synchronous facade still accepts plain `std::io::BufRead` values (a
/// `Cursor<Vec<u8>>` in the tests, a `BufReader<ChildStdout>` in [`show`]), so
/// this adapter reads them on a dedicated thread and hands the bytes over a
/// small bounded tokio channel.  That keeps blocking I/O off the runtime
/// workers while the line splitting itself stays in [`read_lines_async`].
pub struct BlockingReader {
    chunks: tokio::sync::mpsc::Receiver<Vec<u8>>,
    chunk: Vec<u8>,
    pos: usize,
}

impl BlockingReader {
    /// Start a reader thread for `reader` and adapt it.
    pub fn new<R>(mut reader: R) -> Self
    where
        R: std::io::Read + Send + 'static,
    {
        let (tx, chunks) = tokio::sync::mpsc::channel(CHUNK_BUFFER);
        thread::spawn(move || loop {
            let mut buf = vec![0u8; READ_CHUNK];
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf.truncate(n);
                    // `blocking_send` panics inside a runtime; this is a plain
                    // std thread, and it also gives us backpressure.
                    if tx.blocking_send(buf).is_err() {
                        break;
                    }
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        });
        Self {
            chunks,
            chunk: Vec::new(),
            pos: 0,
        }
    }
}

impl AsyncRead for BlockingReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        loop {
            if me.pos < me.chunk.len() {
                let n = buf.remaining().min(me.chunk.len() - me.pos);
                buf.put_slice(&me.chunk[me.pos..me.pos + n]);
                me.pos += n;
                return Poll::Ready(Ok(()));
            }
            match me.chunks.poll_recv(cx) {
                Poll::Ready(Some(chunk)) => {
                    me.chunk = chunk;
                    me.pos = 0;
                }
                // The reader thread is gone: EOF.
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Keep the last [`KEPT_ERRORS`] stderr lines in a shared ring (tokio flavour).
///
/// **Reconnected in Phase 1**: the recovered `keep_errors` (`0x10167dd7c` +
/// closure `0x10167aa64`) allocates the `Arc<Mutex<VecDeque<String>>>` itself,
/// spawns a task over the child's stderr and pops the front while the deque
/// holds `0x14 == 20` entries.  This version does the same with
/// `tokio::sync::Mutex` and a tokio task, and returns its `JoinHandle`.
pub fn keep_errors_async<R>(
    stderr: R,
) -> (
    Arc<tokio::sync::Mutex<VecDeque<String>>>,
    tokio::task::JoinHandle<()>,
)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let kept: Arc<tokio::sync::Mutex<VecDeque<String>>> =
        Arc::new(tokio::sync::Mutex::new(VecDeque::with_capacity(KEPT_ERRORS)));
    let sink = Arc::clone(&kept);
    let handle = crate::runtime::spawn(async move {
        let mut lines = read_lines_async(stderr).await;
        while let Some(line) = lines.recv().await {
            let mut ring = sink.lock().await;
            push_kept(&mut ring, line);
        }
    });
    (kept, handle)
}

/// Keep the last [`KEPT_ERRORS`] stderr lines in a shared ring (sync facade).
///
/// The signature is the one the existing tests and [`Relay::errors`] use.  The
/// line reading is the tokio one ([`read_lines_async`]); only the ring's lock
/// type differs from [`keep_errors_async`], because this ring is shared with
/// synchronous readers.
pub fn keep_errors<R>(stderr: R) -> (Arc<Mutex<VecDeque<String>>>, JoinHandle<()>)
where
    R: BufRead + Send + 'static,
{
    let kept: Arc<Mutex<VecDeque<String>>> =
        Arc::new(Mutex::new(VecDeque::with_capacity(KEPT_ERRORS)));
    let sink = Arc::clone(&kept);
    let mut lines = crate::runtime::block_on(read_lines_async(BlockingReader::new(stderr)));
    let handle = thread::spawn(move || {
        while let Some(line) = lines.blocking_recv() {
            let mut ring = sink.lock().unwrap_or_else(|poison| poison.into_inner());
            push_kept(&mut ring, line);
        }
    });
    (kept, handle)
}

/// The recovered ring rule: drop the oldest line while the deque is full, then
/// push.  Shared by both ring flavours.
fn push_kept(ring: &mut VecDeque<String>, line: String) {
    while ring.len() >= KEPT_ERRORS {
        ring.pop_front();
    }
    ring.push_back(line);
}

fn strip_line_ending(buf: &mut Vec<u8>) {
    if buf.last() == Some(&b'\n') {
        buf.pop();
        if buf.last() == Some(&b'\r') {
            buf.pop();
        }
    }
}

/// Everything `spawn` needs to launch Lich.
///
/// UNKNOWN: the original `Launch` is a private type whose exact fields are
/// only visible through `spawn` offsets: ruby at +0x8, the `lich.rbw` path
/// around +0x20/+0x28, extra args at +0x38/+0x40 and environment pairs at
/// +0x50/+0x58.  We keep those four pieces and mark the rest unknown.
#[derive(Debug, Clone)]
pub struct Launch {
    /// Ruby interpreter to execute.
    pub ruby: PathBuf,
    /// Path to `lich.rbw`; its parent becomes the child's working directory.
    pub lich: PathBuf,
    /// Extra game/character arguments appended after the fixed Lich args.
    pub args: Vec<String>,
    /// Extra environment variables set on the child (after the password
    /// filtering pass).
    pub env: Vec<(OsString, OsString)>,
}

impl Launch {
    pub fn new(ruby: impl Into<PathBuf>, lich: impl Into<PathBuf>) -> Self {
        Self {
            ruby: ruby.into(),
            lich: lich.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }
}

/// Build the exact child command recovered from `spawn`.
///
/// Order (from `spawn` @ `0x10167e860`):
/// `child::command(ruby) -e <78-byte prelude> -e "$0 = ARGV.shift; load $0"
///  <lich.rbw> --pipe --stormfront --sentinel -g 127.0.0.1:<port>
///  <launch.args...>`
/// then `env` pairs, stdin/stdout/stderr piped, cwd = parent of `lich`.
pub fn build_command(launch: &Launch, port: u16) -> Command {
    let mut cmd = crate::child::command(&launch.ruby);
    cmd.args(launch_args(launch, port));
    for (key, value) in &launch.env {
        cmd.env(key, value);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(parent) = working_directory(launch) {
        cmd.current_dir(parent);
    }
    cmd
}

/// [`build_command`] for `tokio::process`.
///
/// **Reconnected in Phase 1**: the recovered `spawn` built a
/// `tokio::process::Command` and returned a tokio `Child`; the async relay
/// ([`spawn_async`]) now uses exactly that.
pub fn build_command_async(launch: &Launch, port: u16) -> tokio::process::Command {
    let mut cmd = crate::child::command_async(&launch.ruby);
    cmd.args(launch_args(launch, port));
    for (key, value) in &launch.env {
        cmd.env(key, value);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(parent) = working_directory(launch) {
        cmd.current_dir(parent);
    }
    cmd
}

/// The argv after the Ruby program, in recovered order.
fn launch_args(launch: &Launch, port: u16) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::with_capacity(6 + LICH_ARGS.len() + launch.args.len());
    for arg in ["-e", RUBY_PRELUDE, "-e", RUBY_LOAD_LICH] {
        args.push(OsString::from(arg));
    }
    args.push(launch.lich.clone().into_os_string());
    for arg in LICH_ARGS {
        args.push(OsString::from(arg));
    }
    args.push(OsString::from(format!("127.0.0.1:{port}")));
    for arg in &launch.args {
        args.push(OsString::from(arg));
    }
    args
}

/// `Path::parent(lich.rbw)` when it is non-empty (the recovered `cwd`).
fn working_directory(launch: &Launch) -> Option<&std::path::Path> {
    match launch.lich.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => Some(parent),
        _ => None,
    }
}


/// A spawned child with its piped streams taken out of `Child`.
pub struct Spawned {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
}

/// Start the Lich child.
///
/// Recovered: `spawn` returns `Result<Child…, io::Error>` (its sret tag 3 is
/// the error branch, storing the `std::io::Error`).  `ChildStdin`,
/// `ChildStdout` and `ChildStderr` are split out so the relay can wire a
/// line reader, an error ring and a writer thread.
pub fn spawn(launch: &Launch, port: u16) -> io::Result<Spawned> {
    let mut command = build_command(launch, port);
    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("child stdin missing"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr missing"))?;
    Ok(Spawned {
        child,
        stdin,
        stdout,
        stderr,
    })
}

/// A spawned tokio child with its piped streams taken out of `Child`.
///
/// **Reconnected in Phase 1**: `tokio::process` instead of `std::process`.
pub struct SpawnedAsync {
    pub child: tokio::process::Child,
    pub stdin: tokio::process::ChildStdin,
    pub stdout: tokio::process::ChildStdout,
    pub stderr: tokio::process::ChildStderr,
}

/// Start the Lich child on tokio.
///
/// Recovered: `spawn` built a `tokio::process::Command`, spawned it and returned
/// a `Result<Child…, io::Error>` (sret tag 3 is the error branch).  Must be
/// awaited inside a runtime — tokio's process driver needs a reactor, so call it
/// from [`run_until`]/[`show_async`] under [`crate::runtime`] or under the
/// caller's own runtime.
pub async fn spawn_async(launch: &Launch, port: u16) -> io::Result<SpawnedAsync> {
    let mut command = build_command_async(launch, port);
    // The recovered relay reaps Lich on every exit path; do the same when a
    // `RelayAsync` is dropped mid-flight, so tests and aborted relays cannot
    // leave an orphaned ruby behind.
    command.kill_on_drop(true);
    let mut child = command.spawn()?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("child stdin missing"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout missing"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr missing"))?;
    Ok(SpawnedAsync {
        child,
        stdin,
        stdout,
        stderr,
    })
}

/// A live relay: child, line receiver, kept stderr ring and pending typed
/// lines.
pub struct Relay {
    pub child: Child,
    /// Writer to the child's stdin (game commands / typed lines).
    pub stdin: ChildStdin,
    /// Child stdout lines (frame/text lines from Lich).
    pub lines: Receiver<String>,
    /// Last [`KEPT_ERRORS`] stderr lines.
    pub errors: Arc<Mutex<VecDeque<String>>>,
    /// Pending typed lines and hand-held state.
    pub waits: Waits,
    /// Every stdout line observed by [`Relay::pump`], in order.
    pub output: Vec<String>,
    stderr_handle: JoinHandle<()>,
    status: Option<ExitStatus>,
    /// Real `tokio_util::sync::CancellationToken` (`tokio-util` 0.7.19): the
    /// recovered relay loop is cancelled through one, so the synchronous facade
    /// carries one too instead of a plain stop flag.
    stop: CancellationToken,
}

/// Wire a spawned child into a relay.
///
/// Recovered `show` @ `0x10167e060` starts the reader/writer/error tasks and
/// returns a control handle pair (stored at +0x280/+0x288 by the relay
/// loop).  This version returns the whole [`Relay`], which is the equivalent
/// testable amount of state.
///
/// `show` takes the port the caller already bound; the observed protocol has
/// Hydra *listen* on it and Lich connect back.  [`start_keyed`] is the full
/// handshake sequence; `show` alone (used by the fake-Ruby tests, which never
/// speak the socket) just wires the pipes.
pub fn show(launch: &Launch, port: u16) -> io::Result<Relay> {
    Ok(relay_from_spawned(spawn(launch, port)?))
}

/// Wire an already-spawned child into a [`Relay`] (shared by [`show`] and
/// [`start_keyed`]).
fn relay_from_spawned(spawned: Spawned) -> Relay {
    // `read_lines`/`keep_errors` wrap the blocking pipe in a
    // `tokio::io::BufReader` (8192 bytes, the recovered `0x2000` allocation).
    let lines = read_lines(BufReader::with_capacity(8192, spawned.stdout));
    let (errors, stderr_handle) = keep_errors(BufReader::with_capacity(8192, spawned.stderr));
    let waits = Waits::new(false);
    Relay {
        child: spawned.child,
        stdin: spawned.stdin,
        lines,
        errors,
        waits,
        output: Vec::new(),
        stderr_handle,
        status: None,
        stop: CancellationToken::new(),
    }
}

/// A relay whose keyed handshake has completed: the live [`Relay`] plus the
/// accepted socket (the game stream Hydra and Lich share).
pub struct KeyedRelay {
    /// The wired relay (child, stdin, stdout lines, stderr ring).
    pub relay: Relay,
    /// The socket Lich connected back on, handshake already validated.  Use it
    /// to write the game stream ([`KeyedRelay::send_game`]); reads go through
    /// [`KeyedRelay::reader`] so the stream is buffered.
    pub socket: TcpStream,
    /// Buffered read side of [`KeyedRelay::socket`] (a `try_clone`, so the
    /// accept left its 60-second timeout in place).  Nothing is buffered at
    /// this point: the accept reads byte by byte and never over-reads.
    pub reader: BufReader<TcpStream>,
    /// The token this session generated and demanded on the socket's first
    /// line.  Kept for diagnostics and tests; never logged by the relay.
    pub token: String,
}

/// Start Lich and complete the observed keyed handshake.
///
/// The recovered sequence (`docs/LICH_WIRE.md` §1-2):
/// 1. bind `127.0.0.1:0` and read the ephemeral port;
/// 2. spawn Lich with `-g 127.0.0.1:<port>`;
/// 3. immediately write `<token>\n<client line>\n` to Lich's stdin;
/// 4. accept one connection with the recovered 60-second deadline and require
///    the token as its first line and the client-identification line as its
///    second, dropping the socket on any mismatch.
///
/// Every failure path kills and reaps the child before returning the error, so
/// a refused handshake never leaves an orphaned `ruby lich.rbw` behind.
pub fn start_keyed(launch: &Launch, token: &str) -> io::Result<KeyedRelay> {
    let (listener, port) = crate::wire::keyed_listener()?;
    let mut spawned = spawn(launch, port)?;

    if let Err(err) = crate::wire::write_lich_handshake(&mut spawned.stdin, token) {
        kill_and_reap(&mut spawned.child);
        return Err(err);
    }
    let socket = match crate::wire::accept_keyed_listener(&listener, token) {
        Ok(socket) => socket,
        Err(err) => {
            kill_and_reap(&mut spawned.child);
            return Err(err);
        }
    };
    // One Lich per relay: a second connection would be unauthenticated.
    drop(listener);

    Ok(KeyedRelay {
        relay: relay_from_spawned(spawned),
        reader: BufReader::new(socket.try_clone()?),
        socket,
        token: token.to_string(),
    })
}

/// [`start_keyed`] with a freshly minted [`crate::child::new_token`] token.
pub fn start_keyed_minted(launch: &Launch) -> io::Result<KeyedRelay> {
    let token = crate::child::new_token().map_err(|err| io::Error::other(err.to_string()))?;
    start_keyed(launch, &token)
}

impl KeyedRelay {
    /// Write one line of the game stream to the accepted socket.
    ///
    /// Hydra plays the game server for Lich, so this is the socket direction
    /// that carries the game XML (and any frame Hydra injects).  A trailing
    /// `\n` is added when the line does not already carry one.
    pub fn send_game(&mut self, line: &str) -> io::Result<()> {
        self.socket.write_all(line.as_bytes())?;
        if !line.ends_with('\n') {
            self.socket.write_all(b"\n")?;
        }
        self.socket.flush()
    }

    /// Read one Lich -> Hydra line from the socket, split into a `<c>` command
    /// and a bare (typed / protocol) line.
    ///
    /// A closed socket is `UnexpectedEof`: there is no line to report.  The
    /// accept left a 60-second read timeout on the socket, so a silent Lich
    /// surfaces as `WouldBlock`/`TimedOut` rather than blocking forever.
    pub fn read_frame(&mut self) -> io::Result<crate::wire::LichFrame> {
        let mut buf = Vec::new();
        if self.reader.read_until(b'\n', &mut buf)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Lich's socket closed",
            ));
        }
        strip_line_ending(&mut buf);
        Ok(crate::wire::LichFrame::parse(
            &String::from_utf8_lossy(&buf),
        ))
    }
}

/// Kill and reap a child on a failed handshake.
fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl Relay {
    /// Write one line (without an added newline) to the child's stdin and
    /// flush it.
    ///
    /// Observed (`docs/LICH_WIRE.md` §3): stdin is the typed-input path and the
    /// line is bare (`look`), never `<c>`-prefixed — Lich forwards it to the
    /// game itself.  The game stream Hydra relays goes over the accepted
    /// socket instead ([`KeyedRelay::send_game`]).
    pub fn send(&mut self, line: &str) -> io::Result<()> {
        self.stdin.write_all(line.as_bytes())?;
        if !line.ends_with('\n') {
            self.stdin.write_all(b"\n")?;
        }
        self.stdin.flush()
    }

    /// Queue or hand a typed line according to [`Waits::typed`], writing any
    /// immediately-handed line to the child.
    pub fn type_line(&mut self, line: String) -> io::Result<Typed> {
        match self.waits.typed(line) {
            Typed::Handed(line) => {
                self.send(&line)?;
                Ok(Typed::Handed(line))
            }
            queued => Ok(queued),
        }
    }

    /// Arm the hand-held flag and immediately flush the queue to the child.
    pub fn release_hold(&mut self) -> io::Result<Vec<String>> {
        self.waits.set_hand_held(true);
        let drained = self.waits.hand_held();
        for line in &drained {
            self.send(line)?;
        }
        Ok(drained)
    }

    /// Ask Lich to acknowledge the character; returns the notice, if any.
    pub fn tell(&mut self) -> Option<Notice> {
        self.waits.tell()
    }

    /// Snapshot of the kept stderr lines.
    pub fn errors(&self) -> Vec<String> {
        self.errors
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    /// Read one stdout line without blocking longer than `timeout`.
    pub fn next_line_timeout(&mut self, timeout: Duration) -> Option<String> {
        match self.lines.recv_timeout(timeout) {
            Ok(line) => {
                self.output.push(line.clone());
                Some(line)
            }
            Err(_) => None,
        }
    }

    /// Drain stdout lines that the reader thread has already produced after
    /// the child exited.  The reader may still be between `read_until` and
    /// `send`, so poll for a short grace period instead of dropping a tail
    /// line.
    fn drain_ready_lines(&mut self) {
        let deadline = Instant::now() + Duration::from_millis(50);
        loop {
            match self.lines.try_recv() {
                Ok(line) => self.output.push(line),
                Err(TryRecvError::Disconnected) => break,
                Err(TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        break;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }

    /// Drive one relay step: wait for a stdout line, notice that the child has
    /// exited, or observe the stop token.  Returns `false` once the child is
    /// gone and the line channel has been drained, or once the token has been
    /// cancelled.
    pub fn pump(&mut self) -> io::Result<bool> {
        if self.stop.is_cancelled() {
            // Reap only a child that is still running: `Child::kill` on an
            // already-reaped process is an error on some platforms.
            let status = match self.child.try_wait()? {
                Some(status) => status,
                None => self.child.kill().and_then(|()| self.child.wait())?,
            };
            self.status = Some(status);
            self.drain_ready_lines();
            return Ok(false);
        }
        match self.lines.recv_timeout(Duration::from_millis(20)) {
            Ok(line) => {
                self.output.push(line);
                Ok(true)
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(status) = self.child.try_wait()? {
                    self.status = Some(status);
                    self.drain_ready_lines();
                    Ok(false)
                } else {
                    Ok(true)
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                if self.status.is_none() {
                    self.status = Some(self.child.wait()?);
                }
                Ok(false)
            }
        }
    }

    /// Block until EOF / child exit, draining stdout into `output`.
    pub fn run_to_end(&mut self) -> io::Result<ExitStatus> {
        while self.pump()? {}
        match self.status {
            Some(status) => Ok(status),
            None => {
                let status = self.child.wait()?;
                self.status = Some(status);
                Ok(status)
            }
        }
    }

    /// Last exit status seen by [`Relay::pump`].
    pub fn status(&self) -> Option<ExitStatus> {
        self.status
    }

    /// The relay's `CancellationToken` (`tokio-util` 0.7.19), the same handle
    /// the recovered loop is cancelled through.  Cancel it to stop the relay at
    /// the next [`Relay::pump`]; the token is cloneable and may be cancelled
    /// from another thread.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.stop.clone()
    }

    /// Has the stop token been cancelled?
    pub fn is_cancelled(&self) -> bool {
        self.stop.is_cancelled()
    }

    /// Stop the child (SIGKILL via `std`) and reap it.
    ///
    /// Also cancels the token, so a [`Relay::pump`]/[`Relay::run_to_end`] on
    /// another thread (or a later call) sees the stop.
    pub fn stop(&mut self) -> io::Result<ExitStatus> {
        self.stop.cancel();
        let status = match self.child.try_wait()? {
            Some(status) => status,
            None => self.child.kill().and_then(|()| self.child.wait())?,
        };
        self.status = Some(status);
        Ok(status)
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        // Best effort: do not leave an orphaned Lich child behind.  Cancelling
        // first matches the recovered teardown, which cancels the token before
        // reaping the child.
        self.stop.cancel();
        let _ = self.child.kill();
        let _ = self.child.wait();
        // The stderr task owns its own reader and exits at EOF; joining a
        // possibly blocked reader is unsafe, so intentionally detach it.
        let _ = &self.stderr_handle;
    }
}

/// Top-level relay entry point (synchronous facade).
///
/// Recovered `run` @ `0x10167dfa0` claims a `LichDoor`, copies the launch
/// state and then drives the select loop in `run::{{closure}}`
/// (`0x10112ec94`): spawn/show once, multiplex child stdout, stderr, typed
/// lines, symbol/tell requests and child exit.
///
/// This facade starts the child, reads every stdout line to EOF and returns the
/// mapped [`Ended`] reason; [`run_async`] is the tokio-native equivalent.
/// Frame parsing and session delivery are the caller's responsibility
/// (`Relay::output` / `Relay::lines`).
pub fn run(launch: Launch, port: u16) -> io::Result<Ended> {
    let mut relay = show(&launch, port)?;
    let status = relay.run_to_end()?;
    // `status.code()` is `None` when a signal terminated the child; the
    // recovered Ended::Exited contains the Display of the ExitStatus, which
    // also distinguishes signal termination.
    Ok(Ended::Exited(status.code()))
}

/// One step of the tokio relay loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A stdout line was read; it has been appended to `RelayAsync::output`.
    Line,
    /// The child exited with this status.
    Exited(ExitStatus),
    /// The [`CancellationToken`] was cancelled; the child was killed.
    Cancelled,
}

/// A live relay on `tokio::process`.
///
/// **Reconnected in Phase 1.**  This is the tokio-native counterpart of
/// [`Relay`]: same recovered state (child, line channel, last-20 stderr ring,
/// [`Waits`]) but with the primitive types the binary used —
/// `tokio::sync::mpsc::channel(64)` for stdout, `tokio::sync::Mutex` for the
/// ring and the queue, `tokio::sync::watch` for the hand-held flag (+0x160 in
/// the recovered `Waits::new`), and a `CancellationToken` for cancellation.
pub struct RelayAsync {
    pub child: tokio::process::Child,
    /// Writer to the child's stdin (game commands / typed lines).
    pub stdin: tokio::process::ChildStdin,
    /// Child stdout lines (frame/text lines from Lich).
    pub lines: tokio::sync::mpsc::Receiver<String>,
    /// Last [`KEPT_ERRORS`] stderr lines.
    pub errors: Arc<tokio::sync::Mutex<VecDeque<String>>>,
    /// Pending typed lines and hand-held state.
    pub waits: tokio::sync::Mutex<Waits>,
    /// The recovered `watch::Receiver<bool>`: `true` once Lich knows the
    /// character and typed lines are handed over instead of queued.
    pub hand: tokio::sync::watch::Receiver<bool>,
    /// Cancellation for the relay loop.
    pub stop: CancellationToken,
    /// Every stdout line observed by [`RelayAsync::pump`], in order.
    pub output: Vec<String>,
    status: Option<ExitStatus>,
    hand_tx: tokio::sync::watch::Sender<bool>,
    stderr_task: tokio::task::JoinHandle<()>,
}

/// Wire a spawned tokio child into a relay.
///
/// The async counterpart of [`show`]: recovered `show` @ `0x10167e060` starts
/// the reader/error tasks and returns the control handles the select loop
/// multiplexes.
pub async fn show_async(launch: &Launch, port: u16) -> io::Result<RelayAsync> {
    let spawned = spawn_async(launch, port).await?;
    let lines = read_lines_async(spawned.stdout).await;
    let (errors, stderr_task) = keep_errors_async(spawned.stderr);
    let (hand_tx, hand) = tokio::sync::watch::channel(false);
    Ok(RelayAsync {
        child: spawned.child,
        stdin: spawned.stdin,
        lines,
        errors,
        waits: tokio::sync::Mutex::new(Waits::new(false)),
        hand,
        stop: CancellationToken::new(),
        output: Vec::new(),
        status: None,
        hand_tx,
        stderr_task,
    })
}

impl RelayAsync {
    /// Write one line to the child's stdin and flush it.
    pub async fn send(&mut self, line: &str) -> io::Result<()> {
        self.stdin.write_all(line.as_bytes()).await?;
        if !line.ends_with('\n') {
            self.stdin.write_all(b"\n").await?;
        }
        self.stdin.flush().await
    }

    /// Queue or hand a typed line according to [`Waits::typed`].
    pub async fn type_line(&mut self, line: String) -> io::Result<Typed> {
        let typed = self.waits.lock().await.typed(line);
        match typed {
            Typed::Handed(line) => {
                self.send(&line).await?;
                Ok(Typed::Handed(line))
            }
            queued => Ok(queued),
        }
    }

    /// Arm the hold, flip the `watch` flag and flush the queue to the child.
    pub async fn release_hold(&mut self) -> io::Result<Vec<String>> {
        let drained = {
            let mut waits = self.waits.lock().await;
            waits.set_hand_held(true);
            waits.hand_held()
        };
        // `watch::Sender::send` only fails when there are no receivers.
        let _ = self.hand_tx.send(true);
        for line in &drained {
            self.send(line).await?;
        }
        Ok(drained)
    }

    /// Ask Lich to acknowledge the character; returns the notice, if any.
    pub async fn tell(&mut self) -> Option<Notice> {
        self.waits.lock().await.tell()
    }

    /// Snapshot of the kept stderr lines.
    pub async fn errors(&self) -> Vec<String> {
        self.errors.lock().await.iter().cloned().collect()
    }

    /// Drain stdout lines the reader task has already produced (plus a short
    /// grace period, mirroring the synchronous facade).
    async fn drain_ready_lines(&mut self) {
        while let Ok(line) = self.lines.try_recv() {
            self.output.push(line);
        }
        let deadline = tokio::time::Instant::now() + Duration::from_millis(50);
        while let Ok(Some(line)) = tokio::time::timeout_at(deadline, self.lines.recv()).await {
            self.output.push(line);
        }
    }

    /// Drive one relay step: a stdout line, the child's exit, or cancellation.
    ///
    /// This is the recovered `tokio::select!` (`run::{{closure}}` @
    /// `0x10112ec94`), reduced to the three arms this crate owns.  The other
    /// recovered arms (door commands, the `watch` channel, the `Waits`
    /// deadline) belong to `hydra_session`; see `NOTES.md` §4.
    pub async fn pump(&mut self) -> io::Result<Step> {
        enum Ready {
            Line(Option<String>),
            Exited(io::Result<ExitStatus>),
            Cancelled,
        }

        // Split the borrows so the select arms can hold `&mut child`,
        // `&mut lines` and `&stop` at the same time.
        let ready = {
            let Self {
                child,
                lines,
                stop,
                ..
            } = self;
            tokio::select! {
                line = lines.recv() => Ready::Line(line),
                waited = child.wait() => Ready::Exited(waited),
                _ = stop.cancelled() => Ready::Cancelled,
            }
        };

        match ready {
            Ready::Line(Some(line)) => {
                self.output.push(line);
                Ok(Step::Line)
            }
            Ready::Line(None) => {
                // The reader task ended at EOF; collect the exit status.
                let status = match self.status {
                    Some(status) => status,
                    None => {
                        let status = self.child.wait().await?;
                        self.status = Some(status);
                        status
                    }
                };
                self.drain_ready_lines().await;
                Ok(Step::Exited(status))
            }
            Ready::Exited(waited) => {
                let status = waited?;
                self.status = Some(status);
                self.drain_ready_lines().await;
                Ok(Step::Exited(status))
            }
            Ready::Cancelled => {
                let _ = self.child.start_kill();
                let status = self.child.wait().await?;
                self.status = Some(status);
                self.drain_ready_lines().await;
                Ok(Step::Cancelled)
            }
        }
    }

    /// Run the relay loop to its end and map it to [`Ended`].
    ///
    /// `Ended::Stopped` is the recovered "Lich stopped." branch (level 2), which
    /// is what a cancelled token produces.
    pub async fn run_to_end(&mut self) -> io::Result<Ended> {
        loop {
            match self.pump().await? {
                Step::Line => continue,
                Step::Exited(status) => return Ok(Ended::Exited(status.code())),
                Step::Cancelled => return Ok(Ended::Stopped),
            }
        }
    }

    /// Last exit status observed by [`RelayAsync::pump`].
    pub fn status(&self) -> Option<ExitStatus> {
        self.status
    }

    /// Cancel the relay loop.
    pub fn stop(&self) {
        self.stop.cancel();
    }

    /// Has the stop token been cancelled?
    pub fn is_cancelled(&self) -> bool {
        self.stop.is_cancelled()
    }

    /// Kill and reap the child now.
    pub async fn kill(&mut self) -> io::Result<ExitStatus> {
        self.stop.cancel();
        let _ = self.child.start_kill();
        let status = self.child.wait().await?;
        self.status = Some(status);
        Ok(status)
    }
}

impl Drop for RelayAsync {
    fn drop(&mut self) {
        // Never leave an orphaned Lich behind: cancel the token and start the
        // kill.  `spawn_async` sets `kill_on_drop(true)` so tokio reaps it.
        self.stop.cancel();
        let _ = self.child.start_kill();
        // The stderr task owns its reader and ends at EOF; joining it here
        // would need an executor, so it is intentionally detached.
        let _ = &self.stderr_task;
    }
}

/// Top-level relay entry point, tokio-native.
///
/// The recovered `run` @ `0x10167dfa0` claims a `LichDoor`, copies the launch
/// state and drives the select loop; this is that entry point with a fresh
/// [`CancellationToken`] (see [`run_until`] to supply one).
pub async fn run_async(launch: Launch, port: u16) -> io::Result<Ended> {
    run_until(launch, port, CancellationToken::new()).await
}

/// [`run_async`] with a caller-supplied [`CancellationToken`].
///
/// Cancelling the token while the relay runs kills the child and answers
/// `Ended::Stopped`, the recovered "Lich stopped." (level 2) notice.
pub async fn run_until(
    launch: Launch,
    port: u16,
    stop: CancellationToken,
) -> io::Result<Ended> {
    let mut relay = show_async(&launch, port).await?;
    relay.stop = stop;
    relay.run_to_end().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::mpsc::RecvTimeoutError;

    fn collect_lines(rx: Receiver<String>) -> Vec<String> {
        let mut out = Vec::new();
        loop {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(line) => out.push(line),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => panic!("reader thread hung"),
            }
        }
        out
    }

    #[test]
    fn waits_new_sets_flags_and_ten_second_deadline() {
        let before = Instant::now();
        let waits = Waits::new(true);
        let deadline = waits.deadline();
        assert!(waits.is_hand_held());
        assert!(!waits.has_told());
        assert!(waits.pending().is_empty());
        // now + 10s, allowing for scheduling slop.
        assert!(deadline >= before + Duration::from_secs(9));
        assert!(deadline <= Instant::now() + Duration::from_secs(11));
    }

    #[test]
    fn typed_queues_and_notices_only_on_first_line() {
        let mut waits = Waits::new(false);
        let first = waits.typed("north".into());
        match first {
            Typed::Queued(Some(notice)) => {
                assert_eq!(notice.kind, 2);
                assert_eq!(
                    notice.text(),
                    "Lich is still logging in; what you typed follows once it knows the character."
                );
            }
            other => panic!("unexpected first typed result: {other:?}"),
        }
        assert_eq!(waits.pending(), ["north".to_string()]);

        let second = waits.typed("south".into());
        assert_eq!(second, Typed::Queued(None));
        assert_eq!(
            waits.pending(),
            ["north".to_string(), "south".to_string()]
        );
    }

    #[test]
    fn typed_hands_immediately_when_hold_armed() {
        let mut waits = Waits::new(true);
        assert_eq!(waits.typed("look".into()), Typed::Handed("look".into()));
        // Nothing was queued and no notice was emitted.
        assert!(waits.pending().is_empty());
    }

    #[test]
    fn hand_held_drains_fifo_only_when_armed() {
        let mut waits = Waits::new(false);
        waits.typed("one".into());
        waits.typed("two".into());
        waits.typed("three".into());
        assert!(waits.hand_held().is_empty());
        assert_eq!(waits.pending().len(), 3);

        waits.set_hand_held(true);
        assert_eq!(
            waits.hand_held(),
            ["one".to_string(), "two".to_string(), "three".to_string()]
        );
        assert!(waits.pending().is_empty());
        // The binary does not clear the flag; a second drain yields nothing.
        assert!(waits.hand_held().is_empty());
    }

    #[test]
    fn tell_sets_flag_and_reports_queue_length() {
        let mut waits = Waits::new(false);
        assert_eq!(waits.tell(), None);
        assert!(waits.has_told());

        waits.typed("a".into());
        waits.typed("b".into());
        let notice = waits.tell().expect("tell should speak with queued lines");
        assert_eq!(notice.kind, 1);
        assert_eq!(notice.level, 1);
        assert_eq!(
            notice.text(),
            "Lich has not said it knows the character yet: 2 line(s) wait, and go as soon as it does."
        );
    }

    #[test]
    fn never_given_concatenates_in_order() {
        let pending = vec!["queued-1".to_string(), "queued-2".to_string()];
        let recovered = vec!["from-lich".to_string()];
        assert_eq!(
            never_given(pending, recovered),
            [
                "queued-1".to_string(),
                "queued-2".to_string(),
                "from-lich".to_string()
            ]
        );
        assert_eq!(never_given(Vec::new(), vec!["only".to_string()]), ["only"]);
        assert!(never_given(Vec::new(), Vec::new()).is_empty());
    }

    #[test]
    fn shared_symbol_produces_warning_only_on_collision() {
        assert_eq!(shared_symbol('.', ';'), None);
        let notice = shared_symbol('.', '.').expect("collision should warn");
        assert_eq!(notice.kind, 1);
        assert_eq!(
            notice.text(),
            "Hydra's commands start with '.', as Lich's do, so a line typed with '.' is Hydra's and Lich's commands can't be reached. Give Hydra another command symbol, such as ., on the settings menu's General page."
        );
        assert!(shared_symbol('!', '!').unwrap().text().contains("'!'"));
    }

    #[test]
    fn ended_notices_are_exact() {
        assert_eq!(
            Ended::Stopped.notice().unwrap().text(),
            "Lich stopped."
        );
        assert_eq!(Ended::Stopped.notice().unwrap().level, 2);
        assert_eq!(
            Ended::Exited(Some(7)).notice().unwrap().text(),
            "Lich exited (7)."
        );
        assert_eq!(
            Ended::Exited(None).notice().unwrap().text(),
            "Lich exited (signal)."
        );
        assert_eq!(
            Ended::DidNotConnect { seconds: 60 }.notice().unwrap().text(),
            "Lich did not connect within 60 seconds, and was stopped."
        );
        assert_eq!(
            Ended::FellBehind { chunks: 95 }.notice().unwrap().text(),
            "Lich fell 95 chunks behind the game, and was stopped: a stream with a hole in it would mislead its scripts."
        );
        assert_eq!(Ended::Finished.notice(), None);
        assert_eq!(
            Ended::AlreadyRunning.notice().unwrap().text(),
            "Lich already runs for this character."
        );
        assert_eq!(
            Ended::CouldNotStart("No such file".into())
                .notice()
                .unwrap()
                .text(),
            "Lich could not start: No such file"
        );
    }

    #[test]
    fn read_lines_preserves_order_and_strips_crlf() {
        let input = Cursor::new(b"first\r\nsecond\n\nfourth\n");
        let rx = read_lines(input);
        assert_eq!(
            collect_lines(rx),
            ["first", "second", "", "fourth"]
        );
    }

    #[test]
    fn read_lines_replaces_invalid_utf8() {
        let input = Cursor::new(b"ok\nbad\xffbyte\n");
        let rx = read_lines(input);
        assert_eq!(collect_lines(rx), ["ok", "bad\u{fffd}byte"]);
    }

    #[test]
    fn read_lines_eof_disconnects_empty() {
        let rx = read_lines(Cursor::new(Vec::new()));
        assert_eq!(collect_lines(rx), Vec::<String>::new());
    }

    #[test]
    fn keep_errors_keeps_last_twenty_in_order() {
        let mut input = String::new();
        for i in 0..25 {
            input.push_str(&format!("err-{i}\n"));
        }
        let (ring, handle) = keep_errors(Cursor::new(input.into_bytes()));
        handle.join().unwrap();
        let ring = ring.lock().unwrap();
        assert_eq!(ring.len(), KEPT_ERRORS);
        assert_eq!(ring.front().unwrap(), "err-5");
        assert_eq!(ring.back().unwrap(), "err-24");
        assert_eq!(
            ring.iter().cloned().collect::<Vec<_>>(),
            (5..25).map(|i| format!("err-{i}")).collect::<Vec<_>>()
        );
    }

    #[test]
    fn keep_errors_under_limit_keeps_all() {
        let (ring, handle) = keep_errors(Cursor::new(b"one\ntwo\n".to_vec()));
        handle.join().unwrap();
        let ring = ring.lock().unwrap();
        assert_eq!(ring.iter().cloned().collect::<Vec<_>>(), ["one", "two"]);
    }

    #[test]
    fn build_command_has_recovered_argument_order() {
        let launch = Launch {
            ruby: PathBuf::from("/usr/bin/ruby"),
            lich: PathBuf::from("/opt/lich/lich.rbw"),
            args: vec!["GEMSTONE".into(), "Hero".into()],
            env: vec![
                (OsString::from("LICH_DIR"), OsString::from("/opt/lich")),
                (OsString::from("GAME"), OsString::from("DR")),
            ],
        };
        let cmd = build_command(&launch, 1234);
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec![
                "-e".to_string(),
                RUBY_PRELUDE.to_string(),
                "-e".to_string(),
                RUBY_LOAD_LICH.to_string(),
                "/opt/lich/lich.rbw".to_string(),
                "--pipe".to_string(),
                "--stormfront".to_string(),
                "--sentinel".to_string(),
                "-g".to_string(),
                "127.0.0.1:1234".to_string(),
                "GEMSTONE".to_string(),
                "Hero".to_string(),
            ]
        );
        assert_eq!(cmd.get_program(), "/usr/bin/ruby");
        assert_eq!(cmd.get_current_dir(), Some(std::path::Path::new("/opt/lich")));

        let env: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert!(env.contains(&("LICH_DIR".into(), Some("/opt/lich".into()))));
        assert!(env.contains(&("GAME".into(), Some("DR".into()))));
    }

    #[test]
    fn build_command_uses_absolute_port_and_no_cwd_without_parent() {
        let launch = Launch::new("ruby", "lich.rbw");
        let cmd = build_command(&launch, 65535);
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"127.0.0.1:65535".to_string()));
        // "lich.rbw" has an empty parent, so no cwd is set.
        assert_eq!(cmd.get_current_dir(), None);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_and_run_relay_with_fake_ruby() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "hydra-lich-relay-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        // A fake ruby that echoes two lines, reads one stdin line, echoes it,
        // writes a stderr line and exits 3.
        let ruby = root.join("ruby");
        let script = format!(
            "#!/bin/sh\n\
             echo 'first line'\n\
             echo 'second line'\n\
             read line\n\
             echo \"got:$line\"\n\
             echo 'stderr line' 1>&2\n\
             exit 3\n"
        );
        fs::write(&ruby, script).unwrap();
        let mut perms = fs::metadata(&ruby).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ruby, perms).unwrap();

        let lich = root.join("lich.rbw");
        fs::write(&lich, "# fake\n").unwrap();

        let mut relay = show(&Launch::new(&ruby, &lich), 4000).unwrap();
        // A typed line waits for the hand-held release; the first one emits
        // the recovered login notice.
        let typed = relay.type_line("north".into()).unwrap();
        match typed {
            Typed::Queued(Some(notice)) => assert_eq!(
                notice.text(),
                "Lich is still logging in; what you typed follows once it knows the character."
            ),
            other => panic!("unexpected typed result: {other:?}"),
        }
        assert!(relay.output.is_empty(), "line must not be written yet");
        assert_eq!(relay.release_hold().unwrap(), ["north".to_string()]);
        let status = relay.run_to_end().unwrap();
        assert_eq!(status.code(), Some(3));
        assert_eq!(
            relay.output,
            ["first line", "second line", "got:north"]
        );
        // The stderr reader thread needs a moment to finish; poll briefly.
        let mut errors = relay.errors();
        for _ in 0..100 {
            if errors == ["stderr line"] {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            errors = relay.errors();
        }
        assert_eq!(errors, ["stderr line"]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn run_maps_child_exit_status_to_ended() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "hydra-lich-run-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let ruby = root.join("ruby");
        fs::write(&ruby, "#!/bin/sh\necho started\nexit 5\n").unwrap();
        let mut perms = fs::metadata(&ruby).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ruby, perms).unwrap();
        let lich = root.join("lich.rbw");
        fs::write(&lich, "# fake\n").unwrap();

        let ended = run(Launch::new(&ruby, &lich), 4001).unwrap();
        assert_eq!(ended, Ended::Exited(Some(5)));
        assert_eq!(
            ended.notice().unwrap().text(),
            "Lich exited (5)."
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn spawn_missing_ruby_is_an_error() {
        let launch = Launch::new("/definitely/not/ruby", "/definitely/not/lich.rbw");
        let err = spawn(&launch, 1).err().expect("spawn should fail");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    // ---------------------------------------------------------------------
    // Phase 1: tests for the tokio-native layer.
    // ---------------------------------------------------------------------

    /// A fake `ruby` in a fresh temp directory: `script` is written as the
    /// interpreter and an empty `lich.rbw` sits beside it (so the recovered
    /// `cwd = parent(lich)` is set).
    #[cfg(unix)]
    fn fake_ruby(tag: &str, script: &str) -> (PathBuf, PathBuf, PathBuf) {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "hydra-lich-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let ruby = root.join("ruby");
        fs::write(&ruby, script).unwrap();
        let mut perms = fs::metadata(&ruby).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ruby, perms).unwrap();
        let lich = root.join("lich.rbw");
        fs::write(&lich, "# fake\n").unwrap();
        (root, ruby, lich)
    }

    /// Drain a tokio line channel that is already closed at `capacity` lines,
    /// but read it asynchronously.
    async fn drain_async(mut rx: tokio::sync::mpsc::Receiver<String>) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(line) = rx.recv().await {
            out.push(line);
        }
        out
    }

    #[test]
    fn line_channel_capacity_matches_the_recovered_semaphore() {
        // `Semaphore::new(0x40)` in `read_lines`.
        assert_eq!(LINE_CHANNEL_CAPACITY, 64);
        assert_eq!(KEPT_ERRORS, 20);
    }

    #[tokio::test]
    async fn read_lines_async_reads_lines_in_order() {
        // `&[u8]` implements `tokio::io::AsyncRead`, so the bytes below are the
        // child's stdout as far as the relay is concerned.
        let input: &'static [u8] = b"first\r\nsecond\n\nfourth\n";
        assert_eq!(
            drain_async(read_lines_async(input).await).await,
            ["first", "second", "", "fourth"]
        );
    }

    #[tokio::test]
    async fn read_lines_async_replaces_invalid_utf8_like_from_utf8_lossy() {
        let input: &'static [u8] = b"ok\nbad\xffbyte\n";
        assert_eq!(
            drain_async(read_lines_async(input).await).await,
            ["ok", "bad\u{fffd}byte"]
        );
    }

    #[tokio::test]
    async fn read_lines_async_stops_at_eof() {
        let input: &'static [u8] = b"";
        assert!(drain_async(read_lines_async(input).await).await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn read_lines_async_applies_bounded_backpressure() {
        // A writer that keeps writing must stall once the 64-line channel is
        // full and the duplex buffer behind it is full too: that is the
        // recovered `Semaphore::new(0x40)` backpressure, not an unbounded
        // queue.
        use tokio::io::AsyncWriteExt;

        const LINES: usize = 200;
        let (mut writer, reader) = tokio::io::duplex(64);
        let mut lines = read_lines_async(reader).await;

        let writer = tokio::spawn(async move {
            for i in 0..LINES {
                let line = format!("{i:020}\n");
                if writer.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
            }
        });

        // Nothing consumes the lines: the reader task fills its 64 slots, stops
        // draining the duplex, and the writer blocks well before line 200.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !writer.is_finished(),
            "the writer finished, so the channel was not bounded"
        );

        let mut got = Vec::new();
        while let Some(line) = lines.recv().await {
            got.push(line);
            if got.len() == LINES {
                break;
            }
        }
        assert_eq!(got.len(), LINES);
        assert_eq!(got[0], format!("{:020}", 0));
        assert_eq!(got[LINES - 1], format!("{:020}", LINES - 1));
        // Draining unblocked the writer.
        tokio::time::timeout(Duration::from_secs(5), writer)
            .await
            .expect("writer stayed blocked")
            .unwrap();
    }

    #[tokio::test]
    async fn keep_errors_async_keeps_the_last_twenty() {
        let mut input = String::new();
        for i in 0..25 {
            input.push_str(&format!("err-{i}\n"));
        }
        let (ring, task) = keep_errors_async(Cursor::new(input.into_bytes()));
        task.await.unwrap();
        let ring = ring.lock().await;
        assert_eq!(ring.len(), KEPT_ERRORS);
        assert_eq!(ring.front().unwrap(), "err-5");
        assert_eq!(ring.back().unwrap(), "err-24");
    }

    #[test]
    fn build_command_async_matches_the_sync_argv() {
        let launch = Launch {
            ruby: PathBuf::from("/usr/bin/ruby"),
            lich: PathBuf::from("/opt/lich/lich.rbw"),
            args: vec!["GEMSTONE".into(), "Hero".into()],
            env: vec![
                (OsString::from("LICH_DIR"), OsString::from("/opt/lich")),
                (OsString::from("GAME"), OsString::from("DR")),
            ],
        };
        let sync_args: Vec<OsString> = build_command(&launch, 1234)
            .get_args()
            .map(|a| a.to_os_string())
            .collect();
        let async_cmd = build_command_async(&launch, 1234);
        let async_args: Vec<OsString> = async_cmd
            .as_std()
            .get_args()
            .map(|a| a.to_os_string())
            .collect();
        assert_eq!(sync_args, async_args);
        assert_eq!(
            async_cmd.as_std().get_program(),
            build_command(&launch, 1234).get_program()
        );
        assert_eq!(
            async_cmd.as_std().get_current_dir(),
            Some(std::path::Path::new("/opt/lich"))
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn show_async_relays_stdout_stdin_stderr_and_exit_status() {
        use std::fs;

        let (root, ruby, lich) = fake_ruby(
            "async-relay",
            "#!/bin/sh\n\
             echo 'first line'\n\
             echo 'second line'\n\
             read line\n\
             echo \"got:$line\"\n\
             echo 'stderr line' 1>&2\n\
             exit 3\n",
        );

        let mut relay = show_async(&Launch::new(&ruby, &lich), 4002).await.unwrap();

        // The typed line is queued (the hand is not held) and says the recovered
        // login notice.
        match relay.type_line("north".into()).await.unwrap() {
            Typed::Queued(Some(notice)) => assert_eq!(
                notice.text(),
                "Lich is still logging in; what you typed follows once it knows the character."
            ),
            other => panic!("unexpected typed result: {other:?}"),
        }
        assert!(relay.output.is_empty(), "line must not be written yet");
        assert!(!*relay.hand.borrow());

        assert_eq!(
            relay.release_hold().await.unwrap(),
            ["north".to_string()]
        );
        // `release_hold` flips the recovered `watch` flag.
        assert!(*relay.hand.borrow());

        assert_eq!(relay.run_to_end().await.unwrap(), Ended::Exited(Some(3)));
        assert_eq!(relay.output, ["first line", "second line", "got:north"]);

        // The stderr task ends at EOF; give it a moment.
        let mut errors = relay.errors().await;
        for _ in 0..200 {
            if errors == ["stderr line"] {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
            errors = relay.errors().await;
        }
        assert_eq!(errors, ["stderr line"]);
        assert_eq!(relay.status().and_then(|s| s.code()), Some(3));

        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_async_maps_child_exit_status_to_ended() {
        use std::fs;

        let (root, ruby, lich) = fake_ruby("async-run", "#!/bin/sh\necho started\nexit 5\n");
        let ended = run_async(Launch::new(&ruby, &lich), 4003).await.unwrap();
        assert_eq!(ended, Ended::Exited(Some(5)));
        assert_eq!(ended.notice().unwrap().text(), "Lich exited (5).");
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_until_stops_the_relay_when_the_token_is_cancelled() {
        use std::fs;

        // A Lich that prints one line and then hangs: only cancellation can end
        // it.
        let (root, ruby, lich) = fake_ruby(
            "async-cancel",
            "#!/bin/sh\necho 'Lich v5.0'\nwhile true; do sleep 1; done\n",
        );

        let stop = CancellationToken::new();
        let canceller = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            canceller.cancel();
        });

        let ended = tokio::time::timeout(
            Duration::from_secs(10),
            run_until(Launch::new(&ruby, &lich), 4004, stop),
        )
        .await
        .expect("cancellation did not stop the relay")
        .unwrap();

        assert_eq!(ended, Ended::Stopped);
        assert_eq!(ended.notice().unwrap().text(), "Lich stopped.");
        assert_eq!(ended.notice().unwrap().level, 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_already_cancelled_token_never_runs_the_child() {
        use std::fs;

        // The child would print a marker; a token cancelled up front must kill it
        // before any output is collected.
        let (root, ruby, lich) = fake_ruby(
            "async-precancel",
            "#!/bin/sh\necho marker\nsleep 5\n",
        );

        let stop = CancellationToken::new();
        stop.cancel();
        let ended = tokio::time::timeout(
            Duration::from_secs(10),
            run_until(Launch::new(&ruby, &lich), 4005, stop),
        )
        .await
        .expect("the relay hung on a pre-cancelled token")
        .unwrap();
        assert_eq!(ended, Ended::Stopped);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_sync_facade_stop_cancels_the_token_and_kills_the_child() {
        use std::fs;

        // A child that ignores nothing and sleeps: it only ends when stopped.
        let (root, ruby, lich) = fake_ruby("sync-stop", "#!/bin/sh\necho started\nsleep 30\n");
        let mut relay = show(&Launch::new(&ruby, &lich), 4006).unwrap();

        // Consume the start line so the test is not racing the reader.
        assert_eq!(relay.next_line_timeout(Duration::from_secs(5)), Some("started".to_string()));
        assert!(!relay.is_cancelled());

        let status = relay.stop().unwrap();
        assert!(relay.is_cancelled());
        assert!(status.code().is_none() || status.code() != Some(0));
        // A later pump observes the stop and reports the end.
        assert!(!relay.pump().unwrap());

        fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn a_relay_async_drop_cancels_and_kills_the_child() {
        // `spawn_async` uses `kill_on_drop(true)` and `RelayAsync::drop` cancels
        // the token, so a failing spawn is the only observable here; the live
        // child case is covered by `run_until_stops_the_relay_when_the_token_is
        // _cancelled`.
        let launch = Launch::new("/definitely/not/ruby", "/definitely/not/lich.rbw");
        let err = spawn_async(&launch, 1).await.err().expect("spawn should fail");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn cancelling_the_token_from_another_thread_stops_run_to_end() {
        use std::fs;

        let (root, ruby, lich) = fake_ruby("sync-token", "#!/bin/sh\necho started\nsleep 30\n");
        let mut relay = show(&Launch::new(&ruby, &lich), 4007).unwrap();
        // The token is the relay's real `CancellationToken`, not a stop flag:
        // cancelling it from another thread ends `run_to_end`.
        let token = relay.cancellation_token();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            token.cancel();
        });
        let status = relay.run_to_end().unwrap();
        canceller.join().unwrap();
        assert!(relay.is_cancelled());
        // Killed by SIGKILL: no exit code on Unix.
        assert_eq!(status.code(), None);
        fs::remove_dir_all(&root).unwrap();
    }

    // ---------------------------------------------------------------------
    // Phase 3: the observed wire protocol (docs/LICH_WIRE.md).
    // ---------------------------------------------------------------------

    #[test]
    fn read_lines_strips_the_game_chunk_prefix() {
        // Lich prefixes a forwarded game chunk with 0x1F; its own lines are
        // unprefixed.  The chunk byte must not reach a protocol parser.
        let input = Cursor::new(
            b"\x1f<room>1</room>\r\nLich is active.\n\x1f<prompt>\n".to_vec(),
        );
        assert_eq!(
            collect_lines(read_lines(input)),
            ["<room>1</room>", "Lich is active.", "<prompt>"]
        );
    }

    #[tokio::test]
    async fn read_lines_async_strips_the_game_chunk_prefix() {
        let input: &'static [u8] = b"\x1f<room>1</room>\r\n--- Lich v5.22.0\n";
        assert_eq!(
            drain_async(read_lines_async(input).await).await,
            ["<room>1</room>", "--- Lich v5.22.0"]
        );
    }

    #[test]
    fn read_lich_out_classifies_the_stdout_lines() {
        // `\r\n` is the observed game-chunk ending; Lich's own wrapped lines
        // end `\r\r\n`, and (like `Lines`) only the delimiter and one `\r`
        // are stripped, so the extra `\r` stays part of the line.
        let input = Cursor::new(
            b"\x1f<room>1</room>\r\n<output class=\"mono\"/>hi<output class=\"\"/>\r\r\n".to_vec(),
        );
        let mut lines = read_lich_out(input);
        let first = lines.recv().unwrap();
        assert_eq!(first, crate::wire::LichOut::Game("<room>1</room>".into()));
        let second = lines.recv().unwrap();
        assert_eq!(
            second,
            crate::wire::LichOut::Own(
                "<output class=\"mono\"/>hi<output class=\"\"/>\r".into()
            )
        );
        assert!(lines.recv().is_err(), "the channel closes at EOF");
    }

    #[tokio::test]
    async fn read_lich_out_async_classifies_the_stdout_lines() {
        let input: &'static [u8] = b"\x1f<prompt>\nLich is running.\n";
        let mut lines = read_lich_out_async(input).await;
        assert_eq!(
            lines.recv().await.unwrap(),
            crate::wire::LichOut::Game("<prompt>".into())
        );
        assert_eq!(
            lines.recv().await.unwrap(),
            crate::wire::LichOut::Own("Lich is running.".into())
        );
        assert!(lines.recv().await.is_none());
    }

    /// A fake Lich written as a bash script: it speaks the observed protocol
    /// over `/dev/tcp` (no Ruby is involved, and no game text is used).
    ///
    /// It reads the two stdin handshake lines, records its pid next to the
    /// script (so a test can check it was reaped) and runs `body` with `$token`,
    /// `$client` and `$port` in scope.
    #[cfg(unix)]
    fn fake_lich_script(body: &str) -> (PathBuf, PathBuf, PathBuf) {
        // The `-g 127.0.0.1:<port>` argument is always the last one for these
        // launches; the script extracts it with bash parameter expansion.
        let script = format!(
            "#!/bin/bash\n\
             read -r token\n\
             read -r client\n\
             last=${{@: -1}}\n\
             port=${{last#127.0.0.1:}}\n\
             echo \"$$\" > \"$(dirname \"$0\")/pid\"\n\
             {body}\n"
        );
        fake_ruby("keyed", &script)
    }

    #[cfg(unix)]
    #[test]
    fn start_keyed_completes_the_two_channel_handshake() {
        let (root, ruby, lich) = fake_lich_script(
            "exec 3<>/dev/tcp/127.0.0.1/$port\n\
             printf '%s\\n' \"$token\" >&3\n\
             printf '%s\\n' \"$client\" >&3\n\
             echo \"seen:$token:$client\"\n\
             read -r game <&3\n\
             printf '%s\\n' \"$game\" >&3",
        );
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let mut keyed = start_keyed(&Launch::new(&ruby, &lich), token).unwrap();
        assert_eq!(keyed.token, token);

        // Hydra's side of the game stream is the socket; the fake Lich echoes
        // the line back as a bare (typed) line.
        keyed.send_game("look").unwrap();
        assert_eq!(
            keyed.read_frame().unwrap(),
            crate::wire::LichFrame::Bare("look".into())
        );

        let status = keyed.relay.run_to_end().unwrap();
        assert!(status.success());
        // The stdin handshake carried the same two lines, and the child echoed
        // them so the test sees exactly what it read.
        let seen = format!("seen:{token}:{}", crate::wire::CLIENT_IDENTIFICATION);
        assert!(
            keyed.relay.output.iter().any(|line| line == &seen),
            "stdin handshake not observed: {:?}",
            keyed.relay.output
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// `kill -0 <pid>`: is the process still there?
    #[cfg(unix)]
    fn process_alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(unix)]
    #[test]
    fn start_keyed_refuses_a_wrong_token_and_reaps_the_child() {
        let (root, ruby, lich) = fake_lich_script(
            "exec 3<>/dev/tcp/127.0.0.1/$port\n\
             printf '%s\\n' \"not-the-token\" >&3\n\
             printf '%s\\n' \"$client\" >&3\n\
             cat <&3 >/dev/null\n\
             sleep 30",
        );
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let err = match start_keyed(&Launch::new(&ruby, &lich), token) {
            Ok(_) => panic!("a wrong token must not be accepted"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert!(!err.to_string().contains(token), "the token must not leak");

        // The failure path killed and reaped the child.
        let pid = std::fs::read_to_string(root.join("pid")).unwrap_or_default();
        assert!(!pid.trim().is_empty(), "the fake Lich never started");
        assert!(
            !process_alive(pid.trim()),
            "the failed handshake left child {} behind",
            pid.trim()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn start_keyed_refuses_a_wrong_client_line() {
        let (root, ruby, lich) = fake_lich_script(
            "exec 3<>/dev/tcp/127.0.0.1/$port\n\
             printf '%s\\n' \"$token\" >&3\n\
             printf '%s\\n' \"/FE:WRAYTH /VERSION:0.0.0 /XML\" >&3\n\
             sleep 30",
        );
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let err = match start_keyed(&Launch::new(&ruby, &lich), token) {
            Ok(_) => panic!("a wrong client line must not be accepted"),
            Err(err) => err,
        };
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let pid = std::fs::read_to_string(root.join("pid")).unwrap_or_default();
        assert!(!process_alive(pid.trim()), "a child was left behind");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn blocking_reader_adapts_a_std_reader_to_async() {
        // The synchronous facade feeds `BlockingReader` into the tokio reader;
        // check the adapter on its own too.
        let mut reader = crate::runtime::block_on(read_lines_async(BlockingReader::new(
            Cursor::new(b"alpha\nbeta\n".to_vec()),
        )));
        let mut out = Vec::new();
        while let Some(line) = crate::runtime::block_on(reader.recv()) {
            out.push(line);
        }
        assert_eq!(out, ["alpha", "beta"]);
    }
}

