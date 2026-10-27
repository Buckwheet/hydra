//! The Lich relay: pending typed lines, notices, reader threads and the
//! child-process lifecycle.
//!
//! The original crate is inherently `tokio`-based.  Everything here is a
//! std-only equivalent with the same observable ordering:
//!
//! * `read_lines` = `BufReader<ChildStdout>` -> bounded channel (capacity 64)
//! * `keep_errors` = `BufReader<ChildStderr>` -> `Arc<Mutex<VecDeque<String>>>`
//!   keeping the last [`KEPT_ERRORS`] lines
//! * `Waits` = queue of typed lines plus `hand_held`/`tell` flags
//! * `spawn`/`show`/`run` = command construction and relay lifecycle
//!
//! See `NOTES.md` for the recovered wire protocol and the
//! established-vs-UNKNOWN split.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

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

/// Spawn a thread that reads text lines from `reader` into a bounded
/// channel of capacity [`LINE_CHANNEL_CAPACITY`].
///
/// Recovered (`read_lines` @ `0x10167dcc0`): a `tokio::io::BufReader` over
/// an `OwnedReadHalf`, a bounded `mpsc` of `String` with capacity 64, and a
/// detached task that forwards every line.  The std version uses
/// `BufRead::read_until(b'\n')` + `String::from_utf8_lossy` (the binary
/// calls `from_utf8_lossy` on the bytes) and a `sync_channel(64)` so a slow
/// consumer applies backpressure exactly like the bounded tokio channel.
///
/// The returned receiver disconnects when the reader reaches EOF or errors.
pub fn read_lines<R>(reader: R) -> Receiver<String>
where
    R: BufRead + Send + 'static,
{
    let (tx, rx) = mpsc::sync_channel(LINE_CHANNEL_CAPACITY);
    thread::spawn(move || {
        let mut reader = reader;
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    strip_line_ending(&mut buf);
                    let line = String::from_utf8_lossy(&buf).into_owned();
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

/// Spawn a thread that keeps the last [`KEPT_ERRORS`] stderr lines in a
/// shared ring.
///
/// Recovered (`keep_errors` @ `0x10167dd7c` + closure @ `0x10167aa64`):
/// allocates the `Arc<Mutex<VecDeque<String>>>` itself, spawns a task over
/// `Lines<BufReader<ChildStderr>>`, and for each line pops the front while
/// the deque length is `0x14 == 20` before pushing the new line.  Returns
/// the ring and the task handle.
pub fn keep_errors<R>(stderr: R) -> (Arc<Mutex<VecDeque<String>>>, JoinHandle<()>)
where
    R: BufRead + Send + 'static,
{
    let kept: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::with_capacity(KEPT_ERRORS)));
    let sink = Arc::clone(&kept);
    let handle = thread::spawn(move || {
        let mut reader = stderr;
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    strip_line_ending(&mut buf);
                    let line = String::from_utf8_lossy(&buf).into_owned();
                    let mut ring = sink.lock().unwrap_or_else(|poison| poison.into_inner());
                    if ring.len() >= KEPT_ERRORS {
                        ring.pop_front();
                    }
                    ring.push_back(line);
                }
                Err(_) => break,
            }
        }
    });
    (kept, handle)
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
    cmd.arg("-e")
        .arg(RUBY_PRELUDE)
        .arg("-e")
        .arg(RUBY_LOAD_LICH)
        .arg(&launch.lich);
    for arg in LICH_ARGS {
        cmd.arg(arg);
    }
    cmd.arg(format!("127.0.0.1:{port}"));
    cmd.args(&launch.args);
    for (key, value) in &launch.env {
        cmd.env(key, value);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(parent) = launch.lich.parent() {
        if !parent.as_os_str().is_empty() {
            cmd.current_dir(parent);
        }
    }
    cmd
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
}

/// Wire a spawned child into a relay.
///
/// Recovered `show` @ `0x10167e060` starts the reader/writer/error tasks and
/// returns a control handle pair (stored at +0x280/+0x288 by the relay
/// loop).  The std-only version returns the whole [`Relay`], which is the
/// equivalent testable amount of state.
pub fn show(launch: &Launch, port: u16) -> io::Result<Relay> {
    let spawned = spawn(launch, port)?;
    // BufReader's 8192-byte default matches the recovered `0x2000`
    // allocation in both reader closures.
    let lines = read_lines(BufReader::with_capacity(8192, spawned.stdout));
    let (errors, stderr_handle) =
        keep_errors(BufReader::with_capacity(8192, spawned.stderr));
    let waits = Waits::new(false);
    Ok(Relay {
        child: spawned.child,
        stdin: spawned.stdin,
        lines,
        errors,
        waits,
        output: Vec::new(),
        stderr_handle,
        status: None,
    })
}

impl Relay {
    /// Write one line (without an added newline) to the child's stdin and
    /// flush it.  This is the parent -> child game-command path.
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

    /// Drive one relay step: wait for a stdout line, or notice that the
    /// child has exited.  Returns `false` once the child is gone and the
    /// line channel has been drained.
    pub fn pump(&mut self) -> io::Result<bool> {
        match self.lines.recv_timeout(Duration::from_millis(20)) {
            Ok(line) => {
                self.output.push(line);
                Ok(true)
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(status) = self.child.try_wait()? {
                    self.status = Some(status);
                    // Drain any lines the reader thread already produced.
                    while let Ok(line) = self.lines.try_recv() {
                        self.output.push(line);
                    }
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

    /// Stop the child (SIGKILL via `std`) and reap it.
    pub fn stop(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.kill().and_then(|()| self.child.wait())?;
        self.status = Some(status);
        Ok(status)
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        // Best effort: do not leave an orphaned Lich child behind.
        let _ = self.child.kill();
        let _ = self.child.wait();
        // The stderr thread owns its own reader and exits at EOF; joining a
        // possibly blocked reader is unsafe, so intentionally detach it.
        let _ = &self.stderr_handle;
    }
}

/// Top-level relay entry point.
///
/// Recovered `run` @ `0x10167dfa0` claims a `LichDoor`, copies the launch
/// state and then drives the select loop in `run::{{closure}}`
/// (`0x10112ec94`): spawn/show once, multiplex child stdout, stderr, typed
/// lines, symbol/tell requests and child exit.
///
/// std-only equivalent: start the child, read every stdout line to EOF and
/// return the mapped [`Ended`] reason.  Frame parsing and session delivery
/// are the caller's responsibility (`Relay::output` / `Relay::lines`).
pub fn run(launch: Launch, port: u16) -> io::Result<Ended> {
    let mut relay = show(&launch, port)?;
    let status = relay.run_to_end()?;
    // `status.code()` is `None` when a signal terminated the child; the
    // recovered Ended::Exited contains the Display of the ExitStatus, which
    // also distinguishes signal termination.
    Ok(Ended::Exited(status.code()))
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
        relay.send("north").unwrap();
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
    fn spawn_missing_ruby_is_an_error() {
        let launch = Launch::new("/definitely/not/ruby", "/definitely/not/lich.rbw");
        let err = spawn(&launch, 1).err().expect("spawn should fail");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
