//! The Lich side of `hydra play` and `hydra serve`.
//!
//! The `.lich` commands themselves live in [`hydra_session::lich_command`];
//! this module is the half that only a binary can do — read `lich.json`, decide
//! whether this start may spawn Lich, spawn it with the observed semantics
//! (`hydra_lich::relay::start_keyed_minted`: bind loopback, spawn
//! `ruby -e … -e … <folder>/lich.rbw --pipe --stormfront --sentinel -g
//! 127.0.0.1:<port>`, write the two-line stdin handshake, accept the keyed
//! socket connection — see `docs/LICH_WIRE.md`) and relay Lich's stdout.
//!
//! # Where `lich.json` is
//!
//! [`settings_path`]: `$HYDRA_LICH_SETTINGS` when set (the test seam, a file
//! path), otherwise `$HYDRA_DATA_DIR/lich.json`, otherwise `./lich.json`.  The
//! data directory variable is the recovered one (`hydra_table::account`), and
//! the file itself is global — "for every character" — exactly as
//! [`hydra_session::lich_command`] documents.
//!
//! # The messages
//!
//! Starting mirrors the reference build's two lines, verbatim:
//!
//! ```text
//! Lich is at <dir>, for every character.
//! Lich is starting, from <dir>\lich.rbw.
//! ```
//!
//! The recovered literal spells the script `<dir>\lich.rbw` with a backslash
//! because the reference build is a Windows executable; the message keeps that
//! spelling while the path actually spawned is `<dir>/lich.rbw` on this
//! platform ([`hydra_session::lich_command::script_path`]).  A refusal reports
//! one recovered sentence instead: the missing folder
//! (`Where Lich is has not been said: lich folder <folder>.`), a folder with no
//! `lich.rbw` in it (`<dir> has no lich.rbw in it`), or no Ruby
//! (`Lich needs Ruby, …`).
//!
//! # Two relays
//!
//! * [`relay`] is the Lich-only relay: Lich's stdout is relayed to this
//!   process's stdout with the 0x1F chunk byte stripped
//!   ([`hydra_lich::wire::LichOut`]) until Lich exits or the session stops, and
//!   nothing is written to the accepted socket or read back from it.  Neither
//!   `play` nor `serve` uses it any more (both run [`relay_game`]); it is kept
//!   because it is the smallest faithful start-and-print loop.
//! * [`relay_game`] is the whole session `hydra play` and `hydra serve` run once
//!   a real login has named the game socket: the game's lines go to Lich, Lich's
//!   frames go to the game, and Lich's stdout still reaches this process's
//!   stdout.  See its own documentation for what ends it.
//!
//! # Starting Lich again in a running session
//!
//! Since Phase 9 the Lich half of [`relay_game`] is swappable ([`LichRun`]): a
//! `.lich off` typed mid-session stops the child and closes the door, and a
//! later `.lich on` spawns a second Lich on the same game socket —
//! [`switches`] is the pure bookkeeping that turns what was typed into the one
//! request the loop acts on, and [`start_lich`] is the start itself (plan,
//! spawn, the two recovered start lines, a fresh door claim).  Whether the
//! reference build really spawns there is **UNKNOWN**; see
//! `crates/hydra/NOTES.md` §8.3 and §9.

use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use hydra_lich::{find_ruby, start_keyed_minted, KeyedRelay, Launch, LichFrame, LichOut};
use hydra_session::command::SessionHandle;
use hydra_session::lich_command::{
    has_no_lich_rbw, lich_at_for_every_character, lich_starting, script_path, LichSettings,
    NEEDS_RUBY, WHERE_NOT_SAID_SENTENCE,
};

/// The recovered data-directory variable (`hydra_table::account::DATA_DIR_ENV`).
pub const DATA_DIR_ENV: &str = "HYDRA_DATA_DIR";

/// Point `lich.json` somewhere else.  A test seam: the integration tests run
/// the binary against a temp data directory, and this keeps a developer's own
/// `data/lich.json` out of the run.
pub const SETTINGS_ENV: &str = "HYDRA_LICH_SETTINGS";

/// `lich.json` given the two environment values, so tests never have to mutate
/// the process environment.
pub fn settings_path_from(settings: Option<OsString>, data_dir: Option<OsString>) -> PathBuf {
    if let Some(settings) = settings {
        return PathBuf::from(settings);
    }
    let dir = match data_dir {
        Some(dir) => PathBuf::from(dir),
        None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    LichSettings::path(&dir)
}

/// Where this run reads `lich.json`: `$HYDRA_LICH_SETTINGS`, else
/// `$HYDRA_DATA_DIR/lich.json`, else `./lich.json`.
pub fn settings_path() -> PathBuf {
    settings_path_from(env::var_os(SETTINGS_ENV), env::var_os(DATA_DIR_ENV))
}

/// Read the settings from `path` (a missing file is the defaults).
pub fn load_from(path: &Path) -> io::Result<LichSettings> {
    LichSettings::load(path)
}

/// Read the settings from [`settings_path`].
pub fn load_settings() -> io::Result<LichSettings> {
    load_from(&settings_path())
}

/// Everything decided before any process exists: the folder, the script in it
/// and the Ruby that will run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// the Lich folder (the child's working directory)
    pub folder: PathBuf,
    /// `<folder>/lich.rbw`, the path the child loads
    pub script: PathBuf,
    /// the interpreter `find_ruby` chose
    pub ruby: PathBuf,
}

/// Why a start cannot happen.  Each one is one of the recovered sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// no `.lich folder` has been said
    FolderUnsaid,
    /// the folder is known but holds no `lich.rbw`
    ScriptMissing(PathBuf),
    /// no Ruby on the PATH (and none under `C:\Ruby4Lich5`)
    RubyMissing,
}

impl Refusal {
    /// The recovered sentence for this refusal, verbatim.
    pub fn message(&self) -> String {
        match self {
            Refusal::FolderUnsaid => WHERE_NOT_SAID_SENTENCE.to_string(),
            Refusal::ScriptMissing(folder) => has_no_lich_rbw(folder),
            Refusal::RubyMissing => NEEDS_RUBY.to_string(),
        }
    }
}

/// Plan a start from `settings` and a Ruby found by the caller (the test seam:
/// no PATH lookup, no spawn).
pub fn plan_with(settings: &LichSettings, ruby: Option<PathBuf>) -> Result<Plan, Refusal> {
    let Some(folder) = settings.folder.clone() else {
        return Err(Refusal::FolderUnsaid);
    };
    let script = script_path(&folder);
    if !script.is_file() {
        return Err(Refusal::ScriptMissing(folder));
    }
    let Some(ruby) = ruby else {
        return Err(Refusal::RubyMissing);
    };
    Ok(Plan {
        folder,
        script,
        ruby,
    })
}

/// Plan a start from `settings`, choosing Ruby with `hydra_lich::find_ruby`
/// (the recovered PATH-then-`C:\Ruby4Lich5` rule).
pub fn plan(settings: &LichSettings) -> Result<Plan, Refusal> {
    plan_with(settings, find_ruby())
}

/// Start Lich for `plan` with the observed semantics:
/// [`start_keyed_minted`] binds the loopback port, spawns the child, writes the
/// token handshake to its stdin and requires the token on the accepted socket.
///
/// A failed start kills and reaps the child (that is `start_keyed`'s own
/// contract), so no orphaned `ruby lich.rbw` is left behind.
pub fn start(plan: &Plan) -> io::Result<KeyedRelay> {
    start_keyed_minted(&Launch::new(&plan.ruby, &plan.script))
}

/// Say the two recovered start lines on `out`.
pub fn announce(plan: &Plan, out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "{}", lich_at_for_every_character(&plan.folder))?;
    writeln!(out, "{}", lich_starting(&plan.folder))?;
    out.flush()
}

/// Relay Lich's stdout to `out` until Lich exits or the session stops, then
/// return Lich's exit status (`None` if the platform did not report one).
///
/// Every line goes through [`LichOut`], which strips the 0x1F chunk byte Lich
/// puts in front of forwarded game chunks, so `out` never sees it.  A session
/// that stops (`SessionHandle::stopping`, the recovered cancellation token)
/// kills and reaps Lich first, so this cannot park on a silent child.
pub fn relay(
    keyed: &mut KeyedRelay,
    out: &mut dyn Write,
    session: &SessionHandle,
) -> io::Result<Option<ExitStatus>> {
    let mut printed = 0usize;
    loop {
        if session.stopping() {
            let status = keyed.relay.stop()?;
            drain(keyed, out, &mut printed)?;
            return Ok(Some(status));
        }
        let alive = keyed.relay.pump()?;
        drain(keyed, out, &mut printed)?;
        if !alive {
            return Ok(keyed.relay.status());
        }
    }
}

/// How long [`relay_game`] lets Lich's last stdout lines arrive after the game
/// has gone before it stops the child.
const TEARDOWN_GRACE: Duration = Duration::from_millis(200);

/// How long [`relay_game`] keeps carrying Lich's frames to the game after the
/// session has stopped.
///
/// The recovered teardown sends `quit` and *then* cancels
/// (`docs/LICH_WIRE.md` §5: a typed `quit` flows through Lich to the game, the
/// game closes): the line Hydra handed to Lich on the way out has to reach the
/// game before the sockets are dropped.
const STOP_FLUSH: Duration = Duration::from_millis(400);

/// One Lich while it runs: the child, its stdout, its accepted socket and the
/// thread that reads that socket.
///
/// **INFERRED** shape, not recovered: a `.lich off` typed mid-session and a
/// later `.lich on` take one of these down and put another up without touching
/// the game socket ([`relay_game`]), so everything one Lich owns lives here and
/// nothing of it can outlive [`LichRun::stop`].  What it wraps is the observed
/// start unchanged ([`start`], `docs/LICH_WIRE.md` §1-3).
struct LichRun {
    /// the child, its stdin and its stdout lines (`hydra_lich::Relay`)
    keyed: KeyedRelay,
    /// Lich's frames off the accepted socket, parsed (`<c>` or bare)
    frames: mpsc::Receiver<LichFrame>,
    /// the thread that fills `frames`; joined once the socket is shut down
    reader: Option<thread::JoinHandle<io::Result<()>>>,
    /// how many of `keyed.relay.output` have already been printed
    printed: usize,
}

impl LichRun {
    /// Take a completed [`KeyedRelay`] and start reading Lich's side of it.
    fn attach(keyed: KeyedRelay) -> io::Result<LichRun> {
        // The keyed accept left a 60-second read timeout on Lich's socket, and
        // a run reads for as long as the session lasts: clear it.
        let _ = keyed.socket.set_read_timeout(None);
        let (sender, frames) = mpsc::channel::<LichFrame>();
        let socket = keyed.socket.try_clone()?;
        let reader = thread::Builder::new()
            .name("hydra-lich-reader".to_string())
            .spawn(move || read_into(socket, sender, |line| LichFrame::parse(&line)))?;
        Ok(LichRun {
            keyed,
            frames,
            reader: Some(reader),
            printed: 0,
        })
    }

    /// One relay step; `false` once the child is gone.
    fn pump(&mut self) -> io::Result<bool> {
        self.keyed.relay.pump()
    }

    /// Print Lich's stdout lines this run has not printed yet.
    fn drain(&mut self, out: &mut dyn Write) -> io::Result<()> {
        drain(&self.keyed, out, &mut self.printed)
    }

    /// One game line, to Lich's accepted socket.
    fn send_game(&mut self, line: &str) {
        let _ = self.keyed.send_game(line);
    }

    /// One typed line, to Lich's **stdin** (`docs/LICH_WIRE.md` §3).
    fn send_typed(&mut self, line: &str) {
        let _ = self.keyed.relay.send(line);
    }

    /// Carry Lich's frames to the game; `false` once Lich's socket has closed.
    ///
    /// A closed socket is not a reason to stop on its own: the child's exit
    /// shows up in [`LichRun::pump`], and its stdout may still be arriving.
    fn carry_frames(&mut self, game: &mut TcpStream) -> bool {
        loop {
            match self.frames.try_recv() {
                Ok(frame) => {
                    let _ = write_game_line(game, &frame);
                }
                Err(TryRecvError::Empty) => return true,
                Err(TryRecvError::Disconnected) => return false,
            }
        }
    }

    /// Let Lich's last stdout lines arrive, for at most [`TEARDOWN_GRACE`].
    fn grace(&mut self, out: &mut dyn Write) {
        if self.keyed.relay.status().is_some() {
            return;
        }
        let deadline = Instant::now() + TEARDOWN_GRACE;
        while self.keyed.relay.status().is_none() && Instant::now() < deadline {
            match self.keyed.relay.pump() {
                Ok(true) => {}
                Ok(false) | Err(_) => break,
            }
            // A stdout write that fails here ends the grace period; the child
            // is stopped below either way.
            if self.drain(out).is_err() {
                break;
            }
        }
    }

    /// Carry Lich's frames to the game for at most `grace`, then give up.
    ///
    /// Used on the way out (the session has stopped): what Lich forwards in
    /// that last moment — the `quit` Hydra just handed it, above all — still has
    /// to reach the game, and nothing else will carry it once the loop is gone.
    fn flush(&mut self, game: &mut TcpStream, out: &mut dyn Write, grace: Duration) {
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            let _ = self.keyed.relay.pump();
            let _ = self.drain(out);
            if !self.carry_frames(game) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Stop the child, carry what it had already sent, close its socket and
    /// join the reader thread.
    ///
    /// Bounded and idempotent: every step is safe to repeat, so a `.lich off`
    /// and the teardown can both call it and [`LichRun`]'s own `Drop` call it
    /// again.  The frames Lich had already put on its socket still go to the
    /// game — a `<c>` command it sent a moment before the stop is not ours to
    /// drop — for at most [`TEARDOWN_GRACE`], and the loop ends as soon as the
    /// child's death has closed the socket.
    fn stop(&mut self, game: &mut TcpStream, out: &mut dyn Write) {
        self.carry_frames(game);
        let _ = self.keyed.relay.stop();
        let _ = self.drain(out);
        let deadline = Instant::now() + TEARDOWN_GRACE;
        while Instant::now() < deadline {
            if !self.carry_frames(game) {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
        let _ = self.keyed.socket.shutdown(Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            // Shutting the socket down is what unblocks the reader.
            let _ = reader.join();
        }
        self.carry_frames(game);
    }
}

impl Drop for LichRun {
    fn drop(&mut self) {
        // A run that was never stopped (an error path, a panic) still reaps its
        // child and unblocks its reader here; `Relay::drop` kills and reaps
        // whatever `stop` left, and ignores the errors of a second kill.
        let _ = self.keyed.relay.stop();
        let _ = self.keyed.socket.shutdown(Shutdown::Both);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

/// Relay a real game connection and this session's Lich to each other until one
/// of them ends.
///
/// This is the last hop of `hydra play`: the login named the game socket
/// ([`hydra_net::eaccess::game::connect`]) and Lich has already connected back
/// on the keyed socket ([`start`]).  Two reader threads feed channels — one over
/// a clone of the game socket (line by line), one over a clone of Lich's socket
/// ([`LichFrame`], so a `<c>` command and a bare line stay distinguishable) —
/// and the loop below moves what is pending each way while Lich's stdout is
/// still printed through [`drain`]:
///
/// * game → Lich: [`KeyedRelay::send_game`], one line at a time;
/// * Lich → game: [`LichFrame::text`] (the `<c>` prefix stripped, a bare line
///   unchanged) + `\n`, flushed.
///
/// It also carries what the player types: `queues` is the
/// [`crate::typing::Queues`] the stdin thread fills.  A line the Lich door took
/// (`SessionHandle::hand_to_lich`) is written to Lich's **stdin** — the observed
/// "Lich takes typing" path of `docs/LICH_WIRE.md` §3 — and a line the session's
/// taker took (`SessionHandle::to_game`) is written to the game socket, which is
/// what typing does when Lich does not take it.
///
/// # `.lich off` and `.lich on` mid-session (Phase 9)
///
/// The Lich side of the loop is swappable ([`LichRun`]), so both recovered
/// commands can be honoured while the game socket stays exactly where it is
/// (the game's own reader thread is never touched):
///
/// * `.lich off` stops the child ([`LichRun::stop`], which also joins its
///   reader) and closes the door (`LichDoor::over`), so later typing goes to the
///   game through the session's taker.  The session keeps running;
/// * `.lich on` starts a **second** Lich: the folder and the rest of the
///   settings are the session's own ([`SessionHandle::lich_settings`], which
///   `hydra_session::lich_command` has just updated), it is planned with
///   [`plan`], spawned with [`start`] — the same observed handshake as the
///   initial start — announced with [`announce`], and its door is claimed
///   (`LichDoor::claim`) so typing goes to the new child's stdin.  A start that
///   is refused (no folder, no `lich.rbw`, no Ruby) or that fails prints the
///   recovered sentence (or a `[hydra] …` error) and leaves the session running
///   with typing still usable;
/// * `.lich on` while a Lich already runs spawns nothing (`ALREADY_ON`).
///
/// **INFERRED** — this is the Phase 9 reading of the reference build, which
/// does not settle whether it spawns at `.lich on` mid-session or only at the
/// next character start (`crates/hydra/NOTES.md` §8.3, §9).
///
/// It stops when the child is gone, the game reaches EOF, or the session stops
/// ([`SessionHandle::stopping`]); then it stops Lich, shuts the game socket down
/// (which is what unblocks the game reader) and joins it.  A write that fails
/// because the other side has already closed is *not* an error: that is what a
/// Lich that exits immediately looks like.
pub fn relay_game(
    keyed: KeyedRelay,
    game: &mut TcpStream,
    out: &mut dyn Write,
    session: &SessionHandle,
    queues: crate::typing::Queues,
) -> io::Result<()> {
    // The game socket is read for as long as the session lasts.
    let _ = game.set_read_timeout(None);

    let (game_sender, game_lines) = mpsc::channel::<String>();
    let game_socket = game.try_clone()?;
    let game_reader = thread::Builder::new()
        .name("hydra-game-reader".to_string())
        .spawn(move || read_into(game_socket, game_sender, |line| line))?;

    let mut queues = queues;
    let mut lich = match LichRun::attach(keyed) {
        Ok(run) => Some(run),
        Err(error) => {
            // Nothing would read the game socket: close it, so the reader
            // above is not left parked on it.  `keyed` was dropped inside
            // `attach`, and `Relay::drop` kills and reaps its child.
            let _ = game.shutdown(Shutdown::Both);
            let _ = game_reader.join();
            return Err(error);
        }
    };

    let mut game_open = true;
    let outcome = loop {
        if session.stopping() {
            // What was typed in the last instant still goes — the `quit` that
            // Ctrl-C sends above all — and then Lich gets its bounded moment to
            // carry that line on to the game.
            carry_typed(&mut lich, &mut queues, game, out, session);
            if let Some(run) = lich.as_mut() {
                run.flush(game, out, STOP_FLUSH);
            }
            break Ok(());
        }
        // Lich's own output, and whether the child is still there.
        let mut lich_alive = true;
        if let Some(run) = lich.as_mut() {
            lich_alive = match run.pump() {
                Ok(alive) => alive,
                Err(error) => break Err(error),
            };
            if let Err(error) = run.drain(out) {
                break Err(error);
            }
        }
        // game -> Lich
        loop {
            match game_lines.try_recv() {
                Ok(line) => {
                    if let Some(run) = lich.as_mut() {
                        run.send_game(&line);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    game_open = false;
                    break;
                }
            }
        }
        carry_typed(&mut lich, &mut queues, game, out, session);
        // Lich -> game
        if let Some(run) = lich.as_mut() {
            run.carry_frames(game);
        }
        if !lich_alive || !game_open {
            if !game_open {
                // The recovered literal `\n[hydra] the game closed the
                // connection\n` (`hydra::play::run::{{closure}}`'s pool).
                let _ = write!(out, "{}", crate::typing::GAME_CLOSED);
                let _ = out.flush();
            }
            break Ok(());
        }
    };

    // Teardown.  Lich may still have stdout in flight, so give it a bounded
    // grace period before the child is stopped (pump drains what has arrived).
    if outcome.is_ok() {
        if let Some(run) = lich.as_mut() {
            run.grace(out);
        }
    }
    if let Some(run) = lich.as_mut() {
        run.stop(game, out);
    }
    drop(lich);
    let _ = game.shutdown(Shutdown::Both);
    let _ = game_reader.join();
    outcome
}

/// What a typed `.lich` line asks the relay loop to do, in the order typed.
///
/// Pure bookkeeping: [`crate::typing::dispatch`] reports the request
/// ([`crate::typing::Handled`]) on the stdin thread, and this turns it into the
/// one thing the loop can act on.  A single typed line sets at most one of the
/// two, so the order inside one [`crate::typing::Handled`] is never ambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    /// `.lich off`: stop the running child and close the door
    Stop,
    /// `.lich on`: start a Lich from the session's own settings
    Start,
}

/// The switches `asked` asks for, in order.
///
/// The folder and every other setting are **not** carried here: the session
/// holds the one copy ([`SessionHandle::lich_settings`], updated by
/// `hydra_session::lich_command` when the line was typed) and the start reads it
/// from there, so a `.lich folder …` typed in the same breath as a `.lich on`
/// cannot be lost.
pub fn switches(asked: &[crate::typing::Handled]) -> Vec<Switch> {
    let mut switches = Vec::new();
    for request in asked {
        if request.stop {
            switches.push(Switch::Stop);
        }
        if request.start.is_some() {
            switches.push(Switch::Start);
        }
    }
    switches
}

/// Start a Lich while the session is already running.
///
/// The recovered start, unchanged: [`plan`] from the session's settings,
/// [`start`] (the observed spawn and keyed handshake), the two recovered start
/// lines ([`announce`]) and a fresh door claim so typing goes to the new child's
/// stdin.  `None` means nothing is running now, and the reason has been printed:
/// a refusal's recovered sentence, or a `[hydra] …` line for a start that failed.
///
/// A failure leaves the game session exactly as it was — no child, no claim, and
/// typing still goes to the game — and a child that did start is never orphaned
/// (a dropped [`KeyedRelay`] kills and reaps it, `Relay::drop`).
fn start_lich(
    session: &SessionHandle,
    queues: &mut crate::typing::Queues,
    out: &mut dyn Write,
) -> Option<LichRun> {
    let settings = session.lich_settings();
    let plan = match plan(&settings) {
        Ok(plan) => plan,
        Err(refusal) => {
            say(out, &refusal.message());
            return None;
        }
    };
    let keyed = match start(&plan) {
        Ok(keyed) => keyed,
        Err(error) => {
            say(out, &format!("[hydra] Lich could not be started: {error}"));
            return None;
        }
    };
    if let Err(error) = announce(&plan, out) {
        say(
            out,
            &format!("[hydra] Lich's start lines could not be written: {error}"),
        );
        // `keyed` is dropped here, and `Relay::drop` kills and reaps the child.
        return None;
    }
    let run = match LichRun::attach(keyed) {
        Ok(run) => run,
        Err(error) => {
            say(
                out,
                &format!("[hydra] Lich's socket could not be read: {error}"),
            );
            return None;
        }
    };
    // A fresh claim is what makes `lich_takes_typing()` true again, and its
    // `typed` end is the queue the loop below writes to the new child's stdin.
    queues.to_lich = session.lich_door().claim().typed;
    Some(run)
}

/// Print one line to `out`, ignoring a write error: a closed stdout must not
/// end a session that is otherwise still usable.
fn say(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Move what the player typed this pass: to Lich, to the game, or into a `.lich`
/// request only this loop can honour.
///
/// What the door took ([`crate::typing::Queues::to_lich`]) goes to Lich's
/// **stdin** — the observed "Lich takes typing" path (`docs/LICH_WIRE.md` §3: a
/// bare line, never `<c>`-prefixed, which Lich forwards to the game itself) —
/// and what the session's taker took goes straight to the game socket, which is
/// what typing does when Lich does not take it.
///
/// A line the door took when no Lich is running goes to the game instead: the
/// routing decision was made a moment earlier, and dropping the line would lose
/// what the player typed.
fn carry_typed(
    lich: &mut Option<LichRun>,
    queues: &mut crate::typing::Queues,
    game: &mut TcpStream,
    out: &mut dyn Write,
    session: &SessionHandle,
) {
    let (to_lich, to_game, asked) = queues.take();
    // The `.lich` requests first: they were typed through the same stdin as the
    // lines below, and the door they close (or open) is what those lines are
    // written through.  A line typed just before a `.lich off` and still waiting
    // in the door's queue is not lost: with no Lich running it goes to the game.
    for request in switches(&asked) {
        match request {
            Switch::Stop => {
                // `.lich off`: stop the child and close the door, so later
                // typing goes to the game.  The recovered `Lich is stopping,
                // and stays off…` was already said by
                // `hydra_session::lich_command`.
                if let Some(mut run) = lich.take() {
                    run.stop(game, out);
                }
                session.lich_door().over();
            }
            Switch::Start => {
                if session.stopping() {
                    // The run is ending: starting a child here would only mean
                    // stopping it again.
                    continue;
                }
                if lich.is_some() {
                    // Belt and braces: `hydra_session::lich_command` answers
                    // `Lich already runs for this character.` and asks for no
                    // start while one Lich runs.  Two children would each own a
                    // door claim, and the second would never see a line.
                    say(out, hydra_session::lich_command::ALREADY_ON);
                    continue;
                }
                *lich = start_lich(session, queues, out);
            }
        }
    }
    for line in to_lich {
        match lich.as_mut() {
            Some(run) => run.send_typed(&line),
            None => {
                // The door took it a moment before Lich stopped: the game is
                // where a typed line goes when Lich is not running.
                let _ = write_text(game, &line);
            }
        }
    }
    for line in to_game {
        let _ = write_text(game, &line);
    }
}

/// Read CRLF-stripped lines from `socket` into `sender` until EOF, mapping each
/// line with `map` (a plain game line, or a parsed [`LichFrame`]).
fn read_into<T: Send + 'static>(
    socket: TcpStream,
    sender: mpsc::Sender<T>,
    map: impl Fn(String) -> T,
) -> io::Result<()> {
    let mut reader = BufReader::new(socket);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => break,
            Ok(_) => {
                while bytes.last() == Some(&b'\n') || bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                let line = String::from_utf8_lossy(&bytes).into_owned();
                if sender.send(map(line)).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    Ok(())
}

/// Write one Lich frame to the game socket: the `<c>` prefix stripped, the bare
/// line unchanged, then a newline and a flush.
fn write_game_line(game: &mut TcpStream, frame: &LichFrame) -> io::Result<()> {
    write_text(game, frame.text())
}

/// Write one text line to the game socket, then a newline and a flush.
///
/// This is the path a typed line takes when Lich does not take typing: Hydra
/// writes to the game itself (`docs/LICH_WIRE.md` §3).
fn write_text(game: &mut TcpStream, text: &str) -> io::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    game.write_all(text.as_bytes())?;
    game.write_all(b"\n")?;
    game.flush()
}

/// Print the relay's stdout lines this call has not printed yet.
fn drain(keyed: &KeyedRelay, out: &mut dyn Write, printed: &mut usize) -> io::Result<()> {
    while *printed < keyed.relay.output.len() {
        let line = LichOut::parse(&keyed.relay.output[*printed]).into_text();
        *printed += 1;
        writeln!(out, "{line}")?;
    }
    out.flush()
}

/// What `hydra play`/`hydra serve` say when `.lich` is off: the one thing
/// missing is Lich, so the sentence names only that.
pub fn off_message(label: &str) -> String {
    format!(
        "{label}: Lich is off for this character; this command needs Lich set to on \
         (`lich.json` key \"on\", or `.lich on` in a session). See crates/hydra/NOTES.md."
    )
}

/// The options `hydra play`/`hydra serve` take that carry a value, so the word
/// after them is not a character.  `serve`'s `--detach` takes none.
pub const VALUE_OPTIONS: [&str; 4] = ["--account", "--game", "--in", "--web"];

/// The character a `play`/`serve` command line names: the first token that is
/// neither an option nor an option's value (`hydra play Aelith --game GSF` and
/// `hydra serve --game GSF Aelith` both answer `Aelith`).
pub fn character_of(tokens: &[String]) -> String {
    let mut skip_value = false;
    for token in tokens {
        if skip_value {
            skip_value = false;
            continue;
        }
        if token.starts_with('-') {
            skip_value = VALUE_OPTIONS.contains(&token.as_str());
            continue;
        }
        return token.clone();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typing::Handled;
    use hydra_session::claimant::Desk;
    use hydra_session::lich_command::{LICH_SCRIPT, SETTINGS_FILE};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("the clock is before the epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hydra-lich-{tag}-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("cannot create the temp folder");
            TempDir { path }
        }

        fn with_script(tag: &str) -> Self {
            let dir = TempDir::new(tag);
            fs::write(dir.path.join(LICH_SCRIPT), "").expect("cannot write lich.rbw");
            dir
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn settings(folder: Option<PathBuf>, on: bool) -> LichSettings {
        LichSettings {
            folder,
            on,
            ..LichSettings::default()
        }
    }

    #[test]
    fn the_settings_path_follows_the_two_variables() {
        assert_eq!(
            settings_path_from(Some(OsString::from("/synthetic/lich.json")), None),
            PathBuf::from("/synthetic/lich.json")
        );
        // The explicit file wins over the data directory.
        assert_eq!(
            settings_path_from(
                Some(OsString::from("/synthetic/other.json")),
                Some(OsString::from("/synthetic/data"))
            ),
            PathBuf::from("/synthetic/other.json")
        );
        assert_eq!(
            settings_path_from(None, Some(OsString::from("/synthetic/data"))),
            PathBuf::from("/synthetic/data").join(SETTINGS_FILE)
        );
        // ... and the fallback is this directory.
        let cwd = settings_path_from(None, None);
        assert!(cwd.is_absolute(), "{}", cwd.display());
        assert_eq!(cwd.file_name().unwrap(), "lich.json");
    }

    #[test]
    fn a_missing_settings_file_is_the_defaults() {
        let dir = TempDir::new("missing");
        assert_eq!(
            load_from(&LichSettings::path(&dir.path)).unwrap(),
            LichSettings::default()
        );
        assert_eq!(DATA_DIR_ENV, "HYDRA_DATA_DIR");
        assert_eq!(SETTINGS_ENV, "HYDRA_LICH_SETTINGS");
    }

    #[test]
    fn a_plan_needs_a_folder_a_script_and_a_ruby() {
        // No folder at all.
        let refusal = plan_with(&settings(None, true), Some(PathBuf::from("/synthetic/ruby")))
            .unwrap_err();
        assert_eq!(refusal, Refusal::FolderUnsaid);
        assert_eq!(
            refusal.message(),
            "Where Lich is has not been said: lich folder <folder>."
        );

        // A folder with no lich.rbw in it.
        let empty = TempDir::new("no-script");
        let refusal = plan_with(
            &settings(Some(empty.path.clone()), true),
            Some(PathBuf::from("/synthetic/ruby")),
        )
        .unwrap_err();
        assert_eq!(refusal, Refusal::ScriptMissing(empty.path.clone()));
        assert_eq!(
            refusal.message(),
            format!("{} has no lich.rbw in it", empty.path.display())
        );

        // Everything but Ruby.
        let folder = TempDir::with_script("no-ruby");
        let refusal = plan_with(&settings(Some(folder.path.clone()), true), None).unwrap_err();
        assert_eq!(refusal, Refusal::RubyMissing);
        assert_eq!(
            refusal.message(),
            r"Lich needs Ruby, which Lich's installer puts in C:\Ruby4Lich5; none was found there or on the PATH"
        );
    }

    #[test]
    fn a_good_plan_names_the_folder_the_script_and_the_ruby() {
        let folder = TempDir::with_script("good");
        let ruby = PathBuf::from("/synthetic/bin/ruby");
        let plan = plan_with(&settings(Some(folder.path.clone()), true), Some(ruby.clone()))
            .expect("a plan");
        assert_eq!(plan.folder, folder.path);
        assert_eq!(plan.script, folder.path.join("lich.rbw"));
        assert_eq!(plan.ruby, ruby);
        // The script is checked, not trusted: `lich.rbw` must be a file.
        assert!(plan.script.is_file());
    }

    #[test]
    fn the_start_messages_are_the_recovered_pair() {
        let folder = TempDir::with_script("messages");
        let plan = plan_with(
            &settings(Some(folder.path.clone()), true),
            Some(PathBuf::from("/synthetic/bin/ruby")),
        )
        .expect("a plan");
        let mut out = Vec::new();
        announce(&plan, &mut out).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            format!(
                "Lich is at {}, for every character.\nLich is starting, from {}\\lich.rbw.\n",
                folder.path.display(),
                folder.path.display()
            )
        );
    }

    #[test]
    fn the_off_message_says_what_is_missing() {
        let message = off_message("hydra play");
        assert!(message.starts_with("hydra play: Lich is off for this character"), "{message}");
        // The one thing missing is Lich itself: `play` and `serve` both log in
        // and connect a game since Phase 6/7, so the sentence must not claim
        // otherwise.
        assert!(message.contains("Lich set to on"), "{message}");
        assert!(message.contains("`.lich on`"), "{message}");
        assert!(!message.contains("no game connection"), "{message}");
        assert!(message.contains("NOTES.md"), "{message}");
        assert!(off_message("hydra serve").starts_with("hydra serve: "));
    }

    #[test]
    fn the_character_is_the_first_word_that_is_not_an_option_or_its_value() {
        let tokens = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        assert_eq!(character_of(&tokens(&["Aelith"])), "Aelith");
        assert_eq!(character_of(&tokens(&["Aelith", "--game", "GSF"])), "Aelith");
        // An option's value is not a character.
        assert_eq!(
            character_of(&tokens(&["--game", "GSF", "Aelith"])),
            "Aelith"
        );
        assert_eq!(
            character_of(&tokens(&["--detach", "--in", "/synthetic", "Aelith"])),
            "Aelith"
        );
        // `--detach` takes no value, so the next word is the character.
        assert_eq!(character_of(&tokens(&["--detach", "Aelith"])), "Aelith");
        assert_eq!(character_of(&tokens(&["--detach"])), "");
        assert_eq!(character_of(&tokens(&[])), "");
    }

    // ---------------------------------------------------------------
    // `.lich off` / `.lich on` in a running session (Phase 9)
    // ---------------------------------------------------------------

    /// A loopback pair standing in for the game socket: what the relay writes
    /// is what the test reads.
    fn game_pair() -> (TcpStream, TcpStream) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
        let addr = listener.local_addr().expect("a loopback address");
        let client = TcpStream::connect(addr).expect("the client connects");
        let (server, _) = listener.accept().expect("the server accepts");
        (client, server)
    }

    /// The lines `socket` has received, without their endings.
    fn lines_from(socket: &mut TcpStream) -> Vec<String> {
        socket
            .set_read_timeout(Some(Duration::from_millis(300)))
            .expect("a read timeout");
        let mut reader = BufReader::new(&mut *socket);
        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => lines.push(line.trim_end().to_string()),
                // The timeout: nothing more came, which is the answer.
                Err(_) => break,
            }
        }
        lines
    }

    /// The two queues a live run fills, with the writing ends kept so a test can
    /// push what the stdin thread would have pushed.
    struct Pipes {
        queues: crate::typing::Queues,
        door: mpsc::Sender<String>,
        taker: mpsc::Sender<String>,
        requests: mpsc::Sender<Handled>,
    }

    fn pipes() -> Pipes {
        let (door, to_lich) = mpsc::channel();
        let (taker, to_game) = mpsc::channel();
        let (requests, asked) = mpsc::channel();
        Pipes {
            queues: crate::typing::Queues {
                to_lich,
                to_game,
                requests: asked,
            },
            door,
            taker,
            requests,
        }
    }

    fn text(buffer: &[u8]) -> String {
        String::from_utf8(buffer.to_vec()).unwrap()
    }

    #[test]
    fn a_typed_lich_request_becomes_one_switch_and_the_order_is_kept() {
        assert_eq!(switches(&[]), Vec::new());
        // A line that is not a `.lich` request asks for nothing.
        assert_eq!(switches(&[Handled::default()]), Vec::new());
        assert_eq!(
            switches(&[Handled {
                stop: true,
                ..Handled::default()
            }]),
            vec![Switch::Stop]
        );
        assert_eq!(
            switches(&[Handled {
                start: Some(PathBuf::from("/synthetic/lich")),
                ..Handled::default()
            }]),
            vec![Switch::Start]
        );
        // Everything one pass of typing asked for is honoured in the order it
        // was typed: off, on, off again really is three switches.
        let asked = vec![
            Handled {
                stop: true,
                ..Handled::default()
            },
            Handled {
                start: Some(PathBuf::from("/synthetic/lich")),
                ..Handled::default()
            },
            Handled {
                stop: true,
                ..Handled::default()
            },
        ];
        assert_eq!(
            switches(&asked),
            vec![Switch::Stop, Switch::Start, Switch::Stop]
        );
    }

    #[test]
    fn a_lich_off_closes_the_door_so_later_typing_goes_to_the_game() {
        let (mut game, mut server) = game_pair();
        let session = SessionHandle::new(Desk::new(None, "Aelith"));
        // A running Lich: the door is claimed, so typing goes to it.
        let claim = session.lich_door().claim();
        assert!(session.lich_takes_typing());
        let (taker, to_game) = hydra_session::command::taker_pair();
        session.set_taker(taker);
        let (request_sender, requests) = mpsc::channel::<Handled>();
        let mut queues = crate::typing::Queues {
            to_lich: claim.typed,
            to_game,
            requests,
        };
        let mut lich: Option<LichRun> = None;
        let mut out = Vec::new();

        // The request the stdin thread sends for a typed `.lich off`.
        request_sender
            .send(Handled {
                stop: true,
                ..Handled::default()
            })
            .expect("the request queue is open");
        carry_typed(&mut lich, &mut queues, &mut game, &mut out, &session);

        // Nothing is running now, and the doorway is shut, so the next typed
        // line goes to the game instead of to a Lich that is not there.
        assert!(lich.is_none());
        assert!(!session.lich_takes_typing());
        assert!(!session.lich_running());
        // The stop itself is local, and says nothing: the recovered `Lich is
        // stopping, and stays off…` was said by `hydra_session::lich_command`.
        assert_eq!(text(&out), "");
        assert!(lines_from(&mut server).is_empty());

        session.to_game("look");
        carry_typed(&mut lich, &mut queues, &mut game, &mut out, &session);
        assert_eq!(lines_from(&mut server), vec!["look"]);
    }

    #[test]
    fn a_lich_on_that_cannot_be_planned_says_the_recovered_sentence_and_starts_nothing() {
        let (mut game, mut server) = game_pair();
        let mut p = pipes();
        let session = SessionHandle::new(Desk::new(None, "Aelith"));
        // A folder that is known but holds no `lich.rbw`: the recovered
        // refusal, which is decided before any process, Ruby or `PATH` exists.
        let empty = TempDir::new("refused");
        session.set_lich_settings(settings(Some(empty.path.clone()), true));
        p.requests
            .send(Handled {
                start: Some(empty.path.clone()),
                ..Handled::default()
            })
            .expect("the request queue is open");

        let mut lich: Option<LichRun> = None;
        let mut out = Vec::new();
        carry_typed(&mut lich, &mut p.queues, &mut game, &mut out, &session);

        assert!(lich.is_none());
        assert_eq!(
            text(&out).trim_end(),
            format!("{} has no lich.rbw in it", empty.path.display())
        );
        // The session is untouched: typing still goes to the game.
        p.taker
            .send("look".to_string())
            .expect("the taker queue is open");
        carry_typed(&mut lich, &mut p.queues, &mut game, &mut out, &session);
        assert_eq!(lines_from(&mut server), vec!["look"]);
    }

    #[test]
    fn a_lich_on_typed_as_the_session_ends_starts_nothing() {
        let (mut game, _server) = game_pair();
        let mut p = pipes();
        let session = SessionHandle::new(Desk::new(None, "Aelith"));
        let folder = TempDir::with_script("ending");
        session.set_lich_settings(settings(Some(folder.path.clone()), true));
        session.stop("the test ended it");
        p.requests
            .send(Handled {
                start: Some(folder.path.clone()),
                ..Handled::default()
            })
            .expect("the request queue is open");

        let mut lich: Option<LichRun> = None;
        let mut out = Vec::new();
        carry_typed(&mut lich, &mut p.queues, &mut game, &mut out, &session);
        // No child was spawned for a session that is over, and nothing was said.
        assert!(lich.is_none());
        assert_eq!(text(&out), "");
    }
}
