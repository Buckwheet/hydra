//! `hydra serve`: the resting session — login, Lich, the game socket, the
//! two-way relay, and (headless) an input spool and a web page.
//!
//! # The recovered command
//!
//! `hydra::serve` (`0x10021e6d0` … `0x10021eec4`) parses its own arguments
//! before it does anything else (the tokens arrive as pointer + length in
//! `x2`/`x3`):
//!
//! | token | meaning |
//! | --- | --- |
//! | `--detach` | rest in the background (`hydra::startup::detach`) |
//! | `--resume <arg>` | a captured session to resume |
//! | `--web <port>` | serve the page on this port (a `u16`) |
//! | `--open` | open the page in the browser |
//! | `--again` | start over rather than continue |
//! | `--in <dir>` | `std::sys::paths::unix::chdir` into `dir` |
//! | anything else | a target (a character), pushed onto the target list |
//!
//! With `--detach` it builds the child's argv (`hydra::startup::Serve::args`,
//! `0x10021efac`, which appends `--again`, `--web <port>`, `--open` and one
//! `--resume <arg>` per resume) and calls `hydra::startup::detach`
//! (`0x10021f40c`); without it, `hydra::connect::Target::many` and then
//! `hydra::gui::run`.  The original `serve` **is** the resting window; this
//! headless `serve` is the same session without a window.
//!
//! # Resting: the game may go away and come back
//!
//! The recovered session is a supervisor: `SupervisedSession::transition`,
//! `wait_before_retry` and `finish` log `\nlifecycle {state}`,
//! `reconnecting in {ms}ms (attempt {n})` and `\nsession stopped: {reason}`, and
//! `supervisor::backoff` is the recovered `[1, 2, 5, 10, 30]`-second table with
//! multiplicative jitter.  A **detached** `serve` that exited on a game EOF
//! would not be resting at all, so this phase makes the connection — not the
//! session — the thing that comes and goes:
//!
//! * the session ([`session::Headless`]), its Lich ([`lich::Resting`]), the
//!   typed input thread, the web page and the settings are made **once**;
//! * each game connection is one relay ([`lich::Resting::play`]); when the game
//!   reaches EOF the run says the recovered [`crate::rest::DROPPED`] notice,
//!   waits the recovered `supervisor::wait_before_retry(attempt)`, logs in again
//!   for the same resolved target and password (never prompting) and dials
//!   again;
//! * a login or connect that fails only means the next attempt, with the next
//!   step of the recovered table;
//! * only a deliberate end stops it: a typed `quit`/`exit`
//!   (`hydra_session::actor::is_exit_intent`), Ctrl-C, or Lich exiting by
//!   itself.  The run then says `\nsession stopped: {reason}` and
//!   `[status] the session has ended`, and exits 0.
//!
//! # What this phase runs
//!
//! * `--in <dir>` is the recovered `chdir`, **plus** an addition: `dir` is also
//!   watched as an *input spool*.  Every regular file whose name does not start
//!   with `.` is read line by line through [`crate::typing::dispatch`] (so
//!   `.lich`, `;status` and the Lich-vs-game rule are exactly `hydra play`'s)
//!   and then removed.  Without `--in`, typing comes from stdin as in `play`;
//! * `--web <port>` serves one synthetic page on `0.0.0.0:<port>` through
//!   [`hydra_web`] (bound like the original, reachable on `127.0.0.1`), saying
//!   `Hydra`, the character and the pairing token; `GET /` answers `200` with
//!   `text/html; charset=utf-8`.  It is *not* the recovered phone client, and
//!   nothing is announced over mDNS;
//! * `--detach` spawns `hydra serve` again with `--detach` filtered out,
//!   stdin closed and stdout/stderr in `<data>/hydra.log`
//!   ([`crate::startup::detach`]), prints the recovered resting message and
//!   returns `Ok`, so the parent exits 0;
//! * `--resume <path>` is read with `hydra_table::table::Resume::read`, and its
//!   `arg()` becomes the character when the command line named none;
//! * `--again` starts a **fresh** `Headless` when the game connection comes
//!   back (see [`start_over`]); without it the session is reused;
//! * `--open` stays reported as not wired: there is no browser to open here.
//!
//! **INFERRED**, and marked so at each point: that `--again` means a fresh
//! session per reconnect, that the Lich child survives a game reconnect, and
//! the channel the lifecycle lines are written on.  What the reference build's
//! `rest::doing` counted as "playing" and the memory figure it printed are
//! **UNKNOWN**; see [`crate::rest`].

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hydra_session::command::{taker_pair, Event, Listener, SessionHandle, Taker};
use hydra_session::lich_command::LichSettings;
use hydra_session::supervisor::{self, Lifecycle};
use hydra_table::table::Resume;

use crate::lich;
use crate::login;
use crate::rest;
use crate::session;
use crate::startup;
use crate::typing;

/// The command's own name, as every message spells it.
pub const LABEL: &str = "hydra serve";

/// The reason a run ends with when the player typed `quit`/`exit`
/// (`hydra_session::actor::is_exit_intent`).
pub const QUIT: &str = "quit";

/// The reason a run ends with when Lich exited by itself.
pub const LICH_ENDED: &str = "Lich ended";

/// How long a retry's wait is slept in one piece.
///
/// The recovered backoff is up to 30 seconds per attempt, and Ctrl-C has to be
/// felt while it runs: the wait is taken in slices this short, each one checking
/// the session's cancellation token.  It is not a busy loop — one retry is a few
/// dozen of these.
const RETRY_SLICE: Duration = Duration::from_millis(25);

/// A parsed `hydra serve` command line.
///
/// The flags are the recovered ones; `account` and `game` are **additions** so a
/// headless run can be pointed at a roster entry that does not exist yet (the
/// reference build took them from the roster alone).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// every token that was not an option: the characters (the first is used)
    pub targets: Vec<String>,
    /// `--account <account>` (**addition**)
    pub account: Option<String>,
    /// `--game <code>` (**addition**)
    pub game: Option<String>,
    /// `--detach`
    pub detach: bool,
    /// `--in <dir>`
    pub in_dir: Option<PathBuf>,
    /// `--web <port>`
    pub web_port: Option<u16>,
    /// `--open`
    pub open: bool,
    /// `--again`
    pub again: bool,
    /// `--resume <arg>`, in order
    pub resumes: Vec<String>,
}

impl Options {
    /// Parse the recovered `hydra serve` words.
    ///
    /// `--detach`, `--open` and `--again` are flags; `--resume`, `--web`, `--in`
    /// (and the two additions) take the next token.  Missing or malformed values
    /// are refused rather than ignored — a dropped `--web` would leave a caller
    /// waiting on a port nobody serves.
    pub fn parse(tokens: &[String]) -> Result<Options, String> {
        let mut options = Options::default();
        let mut index = 0;
        while index < tokens.len() {
            let token = tokens[index].as_str();
            match token {
                "--detach" => {
                    options.detach = true;
                    index += 1;
                }
                "--open" => {
                    options.open = true;
                    index += 1;
                }
                "--again" => {
                    options.again = true;
                    index += 1;
                }
                "--resume" | "--web" | "--in" | "--account" | "--game" => {
                    let value = tokens.get(index + 1).ok_or_else(|| {
                        format!("{token} needs a value (see `hydra serve --help`)")
                    })?;
                    if value.starts_with('-') {
                        return Err(format!(
                            "{token} needs a value, and {value} looks like an option"
                        ));
                    }
                    match token {
                        "--resume" => options.resumes.push(value.clone()),
                        "--web" => {
                            options.web_port = Some(value.parse::<u16>().map_err(|_| {
                                format!("--web is not a port: {value:?}")
                            })?)
                        }
                        "--in" => options.in_dir = Some(PathBuf::from(value)),
                        "--account" => options.account = Some(value.clone()),
                        _ => options.game = Some(value.clone()),
                    }
                    index += 2;
                }
                other if other.starts_with('-') => {
                    return Err(format!(
                        "{other} is not an option this command takes: it takes --detach, \
                         --resume <session>, --web <port>, --open, --again, --in <dir>, \
                         --account <account> and --game <code>"
                    ));
                }
                other => {
                    options.targets.push(other.to_string());
                    index += 1;
                }
            }
        }
        Ok(options)
    }

    /// The character this run plays: the first target, or nothing.
    pub fn character(&self) -> String {
        self.targets.first().cloned().unwrap_or_default()
    }

    /// The login arguments this command line amounts to, before `--resume` has
    /// been read ([`resume_arguments`] reads it).
    pub fn arguments(&self) -> login::Arguments {
        login::Arguments {
            character: self.character(),
            account: self.account.clone(),
            game: self.game.clone(),
        }
    }

    /// The recovered words this phase parses but cannot honour yet.
    pub fn unwired(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.open {
            out.push("--open (there is no browser to open, and no window in this phase)");
        }
        out
    }
}

/// The character a resume file names, through `hydra_table::table::Resume::read`.
///
/// The recovered reader takes the data directory and the file *name*
/// (`Resume::read(dir, file)`), while `--resume` is one path, so the path is
/// split here.  `Ok(None)` means the file was there but named nobody (or was not
/// there at all): the recovered reader answers an empty `arg()` for both.
pub fn resume_character(path: &str) -> Result<Option<String>, String> {
    let path = Path::new(path);
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{}: not a resume file path", path.display()))?;
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    Ok(Resume::read(dir, file).arg())
}

/// The login arguments for this run, with `--resume` filling in a character.
///
/// **INFERRED** (the recovered `Resume::arg` is a `String` and `serve` pushes
/// its `--resume` values onto the resume list, but which of them names a
/// character is this phase's reading): the first resume file that names one is
/// the character, and only when the command line named none.  A resume that
/// cannot be read is reported and changes nothing, so a named target still runs.
pub fn resume_arguments(options: &Options) -> login::Arguments {
    let mut arguments = options.arguments();
    if !arguments.character.is_empty() {
        return arguments;
    }
    for path in &options.resumes {
        match resume_character(path) {
            Ok(Some(character)) => {
                arguments.character = character;
                break;
            }
            Ok(None) => eprintln!(
                "[hydra] serve: {path} names no character to resume; keeping the target this \
                 command line named"
            ),
            Err(message) => eprintln!("[hydra] serve: {message}"),
        }
    }
    arguments
}

/// Say one lifecycle state on stdout.
///
/// **INFERRED** channel: the recovered `\nlifecycle {state}` is a log line, and
/// this headless session writes what it says about itself on stdout (the
/// connected, dropped and session-ended notices are there too); the login's
/// stage lines stay on stderr.
fn life(out: &mut dyn Write, state: Lifecycle) {
    let _ = writeln!(out, "{}", supervisor::lifecycle_line(state));
    let _ = out.flush();
}

/// Say one line, ignoring a write error: a closed stdout must not end a run that
/// is otherwise still usable.
fn say(out: &mut dyn Write, line: &str) {
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// A web page thread, stopped and joined when this value is dropped.
struct Web {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl Web {
    /// Bind `port` and serve the synthetic page from its own thread.
    fn start(root: &Path, port: u16, character: &str) -> Result<Web, String> {
        let mut server = hydra_web::Server::new(root)?;
        let (listener, port) = server.bind(port)?;
        let page = server.page(character);
        let url = format!("http://127.0.0.1:{port}/#pairing={}", server.token);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            let server = server.clone();
            thread::Builder::new()
                .name("hydra-web".to_string())
                .spawn(move || server.serve_blocking(listener, page, &stop))
                .map_err(|error| format!("the web page could not be served: {error}"))?
        };
        eprintln!("[hydra] the web page is at {url}");
        Ok(Web {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The parts of a resting run that outlive one game connection.
struct Wiring {
    /// the session itself: reused across reconnects, replaced by `--again`
    session: session::Headless,
    /// the game end of the session's taker, kept so a replacement session
    /// ([`start_over`]) can be given the same one
    taker: Taker,
    /// what the input thread fills and the relay loop drains
    queues: typing::Queues,
    /// this session's events, from before the run started: `Event::Over` is how
    /// a stopped session says *why* it stopped
    events: Listener,
    /// the spool/stdin thread; deliberately not joined (it may be parked in a
    /// read when the run ends)
    _input: JoinHandle<()>,
}

impl Wiring {
    /// One session's typing: the Lich door claimed (so Lich takes typing again),
    /// the taker installed, and the input thread started.
    fn new(
        session: session::Headless,
        options: &Options,
        current: &typing::Current,
    ) -> Result<Wiring, String> {
        let claim = session.door().claim();
        let (taker, to_game) = taker_pair();
        session.handle().set_taker(taker.clone());
        // Subscribed before the run: `SessionHandle::subscribe` only reports
        // events from now on, and the one this is here for (`Event::Over`) is
        // the last thing a run produces.
        let events = session.subscribe();
        let (requests, input) = match &options.in_dir {
            Some(dir) => typing::spawn_spool(dir.clone(), current.clone()),
            None => typing::spawn_stdin(current.clone()),
        }
        .map_err(|error| format!("{LABEL}: the typed input could not be read: {error}"))?;
        Ok(Wiring {
            session,
            taker,
            queues: typing::Queues {
                to_lich: claim.typed,
                to_game,
                requests,
            },
            events,
            _input: input,
        })
    }

    /// Why the session stopped, if it said so (`Event::Over`): Ctrl-C above all.
    fn over_reason(&mut self) -> Option<String> {
        self.events.drain().into_iter().rev().find_map(|event| match event {
            Event::Over(reason) => Some(reason),
            _ => None,
        })
    }
}

/// `hydra serve [<character>] [--detach] [--in <dir>] [--web <port>] …`.
pub fn run(tokens: &[String]) -> Result<(), String> {
    run_with(tokens, lich::load_settings())
}

/// [`run`] with the Lich settings handed in, so the Lich-only branches are
/// reachable from a unit test with no file and no network.
pub fn run_with(tokens: &[String], settings: io::Result<LichSettings>) -> Result<(), String> {
    let options = Options::parse(tokens).map_err(|message| format!("{LABEL}: {message}"))?;

    // The recovered `chdir` into `--in`: relative paths after it are the
    // folder's, and a failure is `{path}: {err}` with no label.
    if let Some(dir) = &options.in_dir {
        if let Err(error) = std::env::set_current_dir(dir) {
            return Err(format!("{}: {error}", dir.display()));
        }
    }

    // The recovered words this phase cannot honour yet are said rather than
    // silently dropped (`NOTES.md` §8.4).
    for notice in options.unwired() {
        eprintln!("[hydra] serve: {notice} is not wired in this phase; see crates/hydra/NOTES.md");
    }

    if options.detach {
        // The child runs `serve` again without `--detach`, from the same
        // folder, with its output in the data folder's log.
        let log = startup::detach(tokens, options.in_dir.as_deref())
            .map_err(|message| format!("{LABEL}: {message}"))?;
        // The recovered line, on stderr like every other message `main` prints.
        eprintln!("{}", startup::resting(&log));
        return Ok(());
    }

    session_run(&options, settings)
}

/// The session itself: settings, login, Lich, game socket, relay, web page.
///
/// The whole thing is made once and lives as long as the run; only the game
/// connection is made again when it drops.
fn session_run(options: &Options, settings: io::Result<LichSettings>) -> Result<(), String> {
    let settings = settings.map_err(|error| format!("{LABEL}: {error}"))?;
    if !settings.on {
        // The same first branch as `play` (and the same sentence).
        return Err(lich::off_message(LABEL));
    }
    let plan = lich::plan(&settings).map_err(|refusal| refusal.message())?;

    // `--resume` may name the character; the target and the password are
    // resolved once, and a reconnect reuses both (a password is never asked for
    // twice, and never printed).
    let arguments = resume_arguments(options);
    let target = login::resolve(&arguments).map_err(|message| format!("{LABEL}: {message}"))?;
    let password =
        login::password(&target.account).map_err(|message| format!("{LABEL}: {message}"))?;

    let mut out = io::stdout();
    life(&mut out, Lifecycle::Connecting);
    life(&mut out, Lifecycle::LoggingIn);
    let login = login::sign_in(&target, &password).map_err(|message| format!("{LABEL}: {message}"))?;
    let launch = login.launch().clone();

    let session = session::Headless::new(&target.character);
    session.handle().set_lich_settings(settings.clone());
    let keyed = match lich::start(&plan) {
        Ok(keyed) => keyed,
        Err(error) => return Err(format!("{LABEL}: Lich could not be started: {error}")),
    };
    // Everything one Lich owns rides in `Resting`, so the child and its keyed
    // socket survive a game reconnect; a failed attach drops `keyed`, and
    // `Relay::drop` kills and reaps the child.
    let mut resting = match lich::Resting::start(keyed) {
        Ok(resting) => resting,
        Err(error) => {
            return Err(format!("{LABEL}: Lich's socket could not be read: {error}"));
        }
    };
    if let Err(error) = lich::announce(&plan, &mut out) {
        resting.stop(&mut out);
        return Err(format!("{LABEL}: {error}"));
    }

    let mut socket = match login::open_game(&launch) {
        Ok(socket) => socket,
        Err(message) => {
            resting.stop(&mut out);
            return Err(format!("{LABEL}: {message}"));
        }
    };
    life(&mut out, Lifecycle::Connected);
    // The recovered connected notice (`play` says the same one).
    let _ = write!(out, "{}", typing::CONNECTED);
    let _ = out.flush();

    // The page, if `--web` asked for one.  It is bound *before* the relay so a
    // port it cannot have is an error rather than a silent absence, and it lives
    // as long as the run — not as long as one connection.
    let web = match options.web_port {
        Some(port) => match Web::start(
            &hydra_table::account::data_dir(),
            port,
            &target.character,
        ) {
            Ok(web) => Some(web),
            Err(message) => {
                resting.stop(&mut out);
                return Err(format!("{LABEL}: {message}"));
            }
        },
        None => None,
    };

    // Typing: the spool folder when `--in` named one (an addition), stdin
    // otherwise.  Both fill the same two queues the relay loop carries, and both
    // are made once: they follow the session (`typing::Current`) if `--again`
    // replaces it.
    let current = typing::Current::new(session.clone());
    let mut wiring = match Wiring::new(session, options, &current) {
        Ok(wiring) => wiring,
        Err(message) => {
            resting.stop(&mut out);
            return Err(message);
        }
    };
    // Ctrl-C sends `quit` and then cancels the session, which is what ends a
    // resting run.  The watcher is never joined: it may be parked in a signal
    // read when the run ends.
    let _ctrl_c = typing::spawn_ctrl_c(current.clone()).ok();

    // The resting status (`crate::rest`): one character plays while the game is
    // connected, none while a retry is pending; this build has no map and no
    // window.
    let mut status = rest::Status::new(rest::doing(1, 0, 0));
    let mut attempt: u32 = 0;

    let outcome: Result<(), String> = loop {
        match resting.play(
            socket,
            &mut out,
            wiring.session.handle(),
            &mut wiring.queues,
            Some(&mut status),
        ) {
            // Lich exiting by itself ends the session, as it does for `play`:
            // the `\nsession stopped: {reason}` line below says why, and nothing
            // reconnects a game no Lich is left to read.
            Ok(lich::Ended::Lich) => break Ok(()),
            Ok(lich::Ended::Stopped) => break Ok(()),
            Ok(lich::Ended::Game) => {
                // A `quit` the player typed is why the game closed: this is the
                // end they asked for, not a connection to reconnect.
                if resting.quit_requested() {
                    break Ok(());
                }
                // The recovered `\n\n[hydra] the connection dropped;
                // reconnecting\n\n`, then the recovered lifecycle state and the
                // recovered backoff wait.
                let _ = write!(out, "{}", rest::DROPPED);
                let _ = out.flush();
                life(&mut out, Lifecycle::Reconnecting);
                status.set(rest::doing(0, 0, 0));
                match reconnect(
                    options,
                    &target,
                    &password,
                    &mut resting,
                    &mut wiring,
                    &current,
                    &settings,
                    &mut out,
                    &mut attempt,
                ) {
                    Some(next) => {
                        socket = next;
                        status.set(rest::doing(1, 0, 0));
                        continue;
                    }
                    None => break Ok(()),
                }
            }
            Err(error) => break Err(format!("{LABEL}: {error}")),
        }
    };

    // The recovered `finish`: stop Lich, close the Lich slot, transition to
    // `Closed`, say `\nsession stopped: {reason}`, then the status tail `play`
    // prints too.
    let reason = match (wiring.over_reason(), resting.quit_requested()) {
        (Some(reason), _) => reason,
        (None, true) => QUIT.to_string(),
        (None, false) => LICH_ENDED.to_string(),
    };
    resting.stop(&mut out);
    wiring.session.over(&reason);
    life(&mut out, Lifecycle::Closed);
    let _ = writeln!(out, "{}", supervisor::stopped_line(&reason));
    let _ = writeln!(out, "[status] {}", typing::SESSION_ENDED);
    let _ = out.flush();
    // The spool thread ends on the stopped session; the web page is closed with
    // `web`, so nothing outlives the run.
    drop(web);
    outcome
}

/// Sleep out one recovered backoff, answering `false` if the session stopped.
///
/// The wait is taken in [`RETRY_SLICE`] slices so Ctrl-C (which cancels the
/// session) is felt in a fraction of a second rather than after the whole
/// recovered backoff (up to 36 s).  The wait itself comes from
/// `hydra_session::supervisor::wait_before_retry`.
fn sleep_while_waiting(session: &SessionHandle, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    loop {
        if session.stopping() {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep((deadline - now).min(RETRY_SLICE));
    }
}

/// Reconnect after the game dropped: the recovered backoff, the same login, and
/// `--again`'s fresh session.
///
/// `None` means the run is over — the session was stopped (Ctrl-C), or Lich
/// exited by itself while the game was away.  A failed login or dial is *not*
/// the end: the next step of the recovered table is waited out and tried, so a
/// game that is down for a minute is waited for, not given up on.
#[allow(clippy::too_many_arguments)]
fn reconnect(
    options: &Options,
    target: &login::Target,
    password: &str,
    resting: &mut lich::Resting,
    wiring: &mut Wiring,
    current: &typing::Current,
    settings: &LichSettings,
    out: &mut dyn Write,
    attempt: &mut u32,
) -> Option<TcpStream> {
    loop {
        if wiring.session.handle().stopping() {
            return None;
        }
        // A Lich that exited by itself ends the session, exactly as it does
        // inside the relay loop: waiting for a game it can no longer serve would
        // leave a run that never ends.
        match resting.alive(out) {
            Ok(true) => {}
            Ok(false) => return None,
            Err(error) => {
                say(out, &format!("[hydra] Lich's output could not be read: {error}"));
                return None;
            }
        }
        let wait = supervisor::wait_before_retry(*attempt);
        let _ = writeln!(
            out,
            "{}",
            supervisor::reconnecting_line(wait, attempt.saturating_add(1))
        );
        let _ = out.flush();
        *attempt = attempt.saturating_add(1);
        if !sleep_while_waiting(wiring.session.handle(), wait) {
            return None;
        }
        life(out, Lifecycle::LoggingIn);
        let launch = match login::sign_in(target, password) {
            Ok(login) => login.launch().clone(),
            Err(message) => {
                say(out, &format!("[hydra] the login did not go through: {message}"));
                continue;
            }
        };
        match login::open_game(&launch) {
            Ok(socket) => {
                // A connection that works resets the recovered table.
                *attempt = 0;
                if options.again {
                    start_over(wiring, current, settings, target, out);
                }
                life(out, Lifecycle::Connected);
                let _ = write!(out, "{}", typing::CONNECTED);
                let _ = out.flush();
                return Some(socket);
            }
            Err(message) => say(
                out,
                &format!("[hydra] the game socket could not be opened again: {message}"),
            ),
        }
    }
}

/// `--again`: start over with a fresh session instead of reusing this one.
///
/// **INFERRED**: the recovered `--again` is "start over rather than continue",
/// and this phase reads it as "a reconnected session is a new session" — the
/// fresh `Headless` carries no typed log, no settings and no Lich door of its
/// own, so `;status` and the `.lich` commands begin from nothing.  What is *not*
/// restarted is the run: the Lich child, the input thread and the web page
/// belong to the run, not to the session, and starting a second child would be a
/// `.lich off`/`.lich on` the player never asked for.  Whether the reference
/// build meant a session, a character or a captured recording by "start over" is
/// **UNKNOWN** (`crates/hydra/NOTES.md` §9).
fn start_over(
    wiring: &mut Wiring,
    current: &typing::Current,
    settings: &LichSettings,
    target: &login::Target,
    out: &mut dyn Write,
) {
    let fresh = session::Headless::new(&target.character);
    fresh.handle().set_lich_settings(settings.clone());
    // The same taker, so what the player types still reaches the game socket.
    fresh.handle().set_taker(wiring.taker.clone());
    wiring.queues.to_lich = fresh.door().claim().typed;
    wiring.events = fresh.subscribe();
    current.replace(fresh.clone());
    let old = std::mem::replace(&mut wiring.session, fresh);
    // The replaced session is over; nothing reads it any more.
    old.handle().stop("--again");
    say(
        out,
        &format!(
            "[hydra] serve: --again: started a new session for {}",
            target.character
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_session::claimant::Desk;

    fn tokens(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// A temp folder, removed when the test ends.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "hydra-serve-unit-{tag}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("cannot create the temp folder");
            TempDir { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn nothing_at_all_is_a_plain_run() {
        let options = Options::parse(&[]).unwrap();
        assert_eq!(options, Options::default());
        assert_eq!(options.character(), "");
        assert!(options.unwired().is_empty());
    }

    #[test]
    fn every_flag_is_recognised() {
        let options = Options::parse(&tokens(&[
            "--detach", "--open", "--again", "Aelith", "--again",
        ]))
        .unwrap();
        assert!(options.detach && options.open && options.again);
        assert_eq!(options.targets, vec!["Aelith".to_string()]);
        assert_eq!(options.web_port, None);
    }

    #[test]
    fn the_value_options_take_their_value() {
        let options = Options::parse(&tokens(&[
            "Aelith", "--web", "7788", "--in", "/synthetic/in", "--resume", "a", "--resume", "b",
        ]))
        .unwrap();
        assert_eq!(options.character(), "Aelith");
        assert_eq!(options.web_port, Some(7788));
        assert_eq!(options.in_dir, Some(PathBuf::from("/synthetic/in")));
        assert_eq!(options.resumes, vec!["a".to_string(), "b".to_string()]);
        // `--resume` and `--again` are wired now; `--open` is still reported
        // rather than ignored.
        assert!(options.unwired().is_empty(), "{:?}", options.unwired());
        let options = Options::parse(&tokens(&["--open", "--again", "Aelith"])).unwrap();
        assert_eq!(options.unwired().len(), 1, "{:?}", options.unwired());
        assert!(options.unwired()[0].starts_with("--open"));
    }

    #[test]
    fn the_added_account_and_game_options_are_the_login_arguments() {
        let options = Options::parse(&tokens(&[
            "Aelith", "--account", "main", "--game", "GSF",
        ]))
        .unwrap();
        assert_eq!(
            options.arguments(),
            login::Arguments {
                character: "Aelith".into(),
                account: Some("main".into()),
                game: Some("GSF".into()),
            }
        );
    }

    #[test]
    fn a_missing_or_bad_value_is_refused() {
        for flag in ["--web", "--in", "--resume", "--account", "--game"] {
            let message = Options::parse(&tokens(&["Aelith", flag])).unwrap_err();
            assert!(message.contains("needs a value"), "{flag}: {message}");
        }
        // A value that is really another option.
        let message = Options::parse(&tokens(&["--web", "--detach"])).unwrap_err();
        assert!(message.contains("looks like an option"), "{message}");
        // A port that is not a port.
        let message = Options::parse(&tokens(&["--web", "eaccess"])).unwrap_err();
        assert!(message.contains("is not a port"), "{message}");
        let message = Options::parse(&tokens(&["--web", "70000"])).unwrap_err();
        assert!(message.contains("is not a port"), "{message}");
        // An option the command does not take is refused, not ignored.
        let message = Options::parse(&tokens(&["--wibble"])).unwrap_err();
        assert!(message.contains("is not an option this command takes"), "{message}");
    }

    #[test]
    fn a_resume_file_names_the_character_when_the_command_line_names_none() {
        let dir = TempDir::new("resume");
        let file = dir.path.join("resume");
        std::fs::write(&file, "  Aelith\n").unwrap();

        // The recovered reader trims the file's content.
        assert_eq!(
            resume_character(&file.display().to_string()).unwrap(),
            Some("Aelith".to_string())
        );
        // A file that is not there names nobody (and is not an error).
        assert_eq!(
            resume_character(&dir.path.join("missing").display().to_string()).unwrap(),
            None
        );
        // A path with no file name is reported.
        let message = resume_character("/").unwrap_err();
        assert!(message.contains("not a resume file path"), "{message}");

        // The character comes from the resume, and only when none was named.
        let options = Options::parse(&tokens(&[
            "--resume",
            &file.display().to_string(),
            "--account",
            "main",
        ]))
        .unwrap();
        let arguments = resume_arguments(&options);
        assert_eq!(arguments.character, "Aelith");
        assert_eq!(arguments.account, Some("main".to_string()));

        // A named target wins over the resume file.
        let options = Options::parse(&tokens(&["Bryn", "--resume", &file.display().to_string()]))
            .unwrap();
        assert_eq!(resume_arguments(&options).character, "Bryn");

        // A resume that cannot be read keeps the target and says so.
        let options = Options::parse(&tokens(&[
            "--resume",
            &dir.path.join("missing").display().to_string(),
        ]))
        .unwrap();
        assert_eq!(resume_arguments(&options).character, "");
    }

    #[test]
    fn lich_off_and_the_lich_refusals_are_said_before_any_login() {
        // Off: the same first branch (and sentence) as `play`, with no network.
        let message = run_with(&tokens(&["Aelith"]), Ok(LichSettings::default())).unwrap_err();
        assert!(message.starts_with("hydra serve: Lich is off"), "{message}");

        // On, but no folder: the recovered sentence.
        let on = LichSettings {
            on: true,
            ..LichSettings::default()
        };
        assert_eq!(
            run_with(&tokens(&["Aelith"]), Ok(on)).unwrap_err(),
            hydra_session::lich_command::WHERE_NOT_SAID_SENTENCE
        );

        // A settings file that cannot be read is the label's own message.
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        assert_eq!(
            run_with(&tokens(&[]), Err(error)).unwrap_err(),
            "hydra serve: denied"
        );
    }

    #[test]
    fn a_bad_command_line_is_refused_before_the_settings() {
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let message = run_with(&tokens(&["--wibble"]), Err(error)).unwrap_err();
        assert!(message.starts_with("hydra serve: --wibble is not an option"), "{message}");
    }

    #[test]
    fn a_backoff_sleep_ends_at_once_when_the_session_stops() {
        let session = SessionHandle::new(Desk::new(None, "Aelith"));
        // Not stopped: the wait really takes its time.
        let started = Instant::now();
        assert!(sleep_while_waiting(&session, Duration::from_millis(120)));
        assert!(started.elapsed() >= Duration::from_millis(110));

        // Stopped: it answers at once rather than sleeping the backoff out.
        session.stop("Ctrl-C");
        let started = Instant::now();
        assert!(!sleep_while_waiting(&session, Duration::from_secs(30)));
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
    }
}
