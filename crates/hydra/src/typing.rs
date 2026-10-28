//! Interactive typing: what `hydra play` does with a line read from stdin.
//!
//! The recovered loop is `hydra::play::run::{{closure}}`
//! (`0x100023b6c` … `0x100025b34`, 8136 bytes): it spawns tokio stdin lines, an
//! `Event` broadcast receiver ([`SessionHandle::subscribe`], called at
//! `0x100023e84`), the session supervisor's `JoinHandle` and Ctrl-C in one
//! `tokio::select!`.  Its call table also names `hydra_table::relay::alone`
//! (`0x10023d7c`), `tokio::io::stdin` (`0x100024008`) and `hydra::status::line`
//! (`0x100024d48`), and the same literal pool holds the three notices repeated
//! below (`[status] the session has ended`, `\n[hydra] connected; type
//! commands, Ctrl-C to quit\n\n`, `\n[hydra] the game closed the connection\n`).
//!
//! A typed line goes through the session's inbox — the reference build's
//! `SessionHandle::send_as_typed`.  This reconstruction splits that into the two
//! ends the crate already has:
//!
//! * [`SessionHandle::hand_to_lich`] — the Lich door's typed queue, taken by
//!   [`LichDoor::claim`], which is the "Lich takes typing" path of
//!   `docs/LICH_WIRE.md` §3;
//! * [`SessionHandle::to_game`] — the session's taker
//!   ([`hydra_session::command::taker_pair`]), which is what goes to the game
//!   socket when Lich does not take typing.
//!
//! Neither end owns a socket: [`Queues`] carries both to the relay loop, which
//! writes one to Lich's stdin and the other to the game ([`crate::lich::relay_game`]).
//! That keeps the bytes on one thread each and mirrors the recovered split.
//!
//! `.lich` commands that need a process are the exception: the session cannot
//! spawn, so [`Handled`] carries the request (`.lich off`'s stop, `.lich on`'s
//! start) out on the same [`Queues`] and the relay loop carries it out —
//! [`crate::lich::switches`] turns the requests into the loop's own
//! [`crate::lich::Switch`]es, and a `.lich on` really does start a second Lich
//! on the running game socket (`crates/hydra/NOTES.md` §8.3).
//!
//! Ctrl-C sends `quit` and then cancels the session ([`spawn_ctrl_c`]).
//!
//! # The dispatch rules
//!
//! [`route`] is pure and covers the whole branching; [`dispatch`] is the
//! effectful half (it prints what is local and fills the two queues).
//!
//! | a line | goes |
//! | --- | --- |
//! | empty or whitespace only | nowhere ([`Route::Ignore`]) |
//! | `;status` (as the first word) | a local status line, never forwarded ([`Route::Status`]) |
//! | starting with the session's Hydra symbol (`.` unless chosen) | Hydra: `SessionHandle::lich_command`, then the notices ([`Route::Hydra`]) |
//! | anything else, with Lich taking typing | Lich, through the door ([`Route::Lich`]) |
//! | anything else, with Lich not running | the game socket directly ([`Route::Game`]) |
//!
//! **INFERRED** markers below: the reference build has one string table and one
//! call site per question, not one per sentence, so the status line's
//! composition, the "not a Hydra command" sentence and the placement of the
//! three recovered notices are this crate's reading of the code shape.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use hydra_session::command::SessionHandle;

use crate::session::Headless;

/// The word that asks for a local status line (**RECOVERED**: it is named in
/// the reference build's `play` usage text, `readable text, `;status``).
pub const STATUS_WORD: &str = ";status";

/// The tail of a status line for a session that has ended.
///
/// **RECOVERED** literal (`0x101f87479`, 30 bytes): `[status] the session has
/// ended`.  It is used as the last clause of one line rather than on its own, so
/// that a `;status` answer always carries the character and the Lich facts.
pub const SESSION_ENDED: &str = "the session has ended";

/// The other tail: the session is still here.
pub const SESSION_RUNNING: &str = "the session is running";

/// The line `hydra play` writes once the game socket is open.
///
/// **RECOVERED** literal, byte for byte including the leading and trailing
/// newlines: `\n[hydra] connected; type commands, Ctrl-C to quit\n\n`.  *Where*
/// it is written (**INFERRED**): after the login has opened the game socket and
/// before the relay starts, which is the only moment "connected" is true.
pub const CONNECTED: &str = "\n[hydra] connected; type commands, Ctrl-C to quit\n\n";

/// The line `hydra play` writes when the game closes the connection.
///
/// **RECOVERED** literal, including the leading newline:
/// `\n[hydra] the game closed the connection\n`.
pub const GAME_CLOSED: &str = "\n[hydra] the game closed the connection\n";

/// The line printed when a `.`-prefixed line is not a command this phase knows.
///
/// **INFERRED** wording: `docs/LICH_WIRE.md` §3 and the recovered `.lich` intro
/// both say a line starting with Hydra's symbol is Hydra's and must not be sent
/// on, so it is answered here instead of being forwarded.
pub fn not_a_command(line: &str, symbol: char) -> String {
    format!(
        "[hydra] {line}: not a command Hydra knows; a line starting with {symbol} is never sent on"
    )
}

/// Where one typed line goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// nothing: an empty or whitespace-only line
    Ignore,
    /// a Hydra command: handled locally, never sent to the game or to Lich
    Hydra(String),
    /// `;status`: one local status line, never forwarded
    Status,
    /// to Lich, because Lich takes typing
    Lich(String),
    /// to the game socket, because Lich is not running
    Game(String),
}

/// Decide where `line` goes.  Pure: the same answer for the same three values.
///
/// `symbol` is the session's Hydra command symbol (`.` unless the character has
/// chosen another).  `lich_takes_typing` is
/// [`SessionHandle::lich_takes_typing`] asked at the moment the line arrives.
pub fn route(line: &str, symbol: char, lich_takes_typing: bool) -> Route {
    // The reader strips the newline; a stray `\r` (a Windows console) is
    // trimmed here too, so `look\r` never reaches the game with the `\r`.
    let line = line.trim_end_matches(['\r', '\n']);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Route::Ignore;
    }
    // `;status`, and `;status` followed by anything: the first word decides,
    // because the word is a status request and not a game command.
    if trimmed.split_whitespace().next() == Some(STATUS_WORD) {
        return Route::Status;
    }
    if trimmed.starts_with(symbol) {
        return Route::Hydra(trimmed.to_string());
    }
    if lich_takes_typing {
        Route::Lich(trimmed.to_string())
    } else {
        Route::Game(trimmed.to_string())
    }
}

/// What handling one typed line did.
///
/// The queues themselves carry the lines ([`SessionHandle::hand_to_lich`] and
/// [`SessionHandle::to_game`] are what put them there); these fields say which
/// way the line went, and carry the two requests only the relay loop can honour.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Handled {
    /// the line was handed to Lich (the door's typed queue)
    pub to_lich: Option<String>,
    /// the line went to the session's taker, for the game socket
    pub to_game: Option<String>,
    /// `.lich on` asked for a start; the folder and the rest of the settings
    /// are the session's own (`SessionHandle::lich_settings`)
    pub start: Option<PathBuf>,
    /// `.lich off` asked for Lich to be stopped
    pub stop: bool,
}

impl Handled {
    /// Nothing happened that the relay loop has to act on.
    pub fn is_idle(&self) -> bool {
        self.to_lich.is_none() && self.to_game.is_none() && self.start.is_none() && !self.stop
    }
}

/// Handle one typed line: print what is local, and put the rest where it goes.
///
/// The `Handled` value never contains the sockets: the line is already on the
/// door's typed queue or the session's taker by the time this returns.
pub fn dispatch(line: &str, session: &Headless, out: &mut dyn Write) -> io::Result<Handled> {
    let handle: &SessionHandle = session.handle();
    let lich = handle.lich_takes_typing();
    match route(line, session.door().symbol(), lich) {
        Route::Ignore => Ok(Handled::default()),
        Route::Status => {
            writeln!(out, "{}", status_line(session, lich))?;
            out.flush()?;
            Ok(Handled::default())
        }
        Route::Hydra(text) => command(&text, session, out),
        Route::Lich(text) => {
            handle.log_typed(&text);
            if handle.hand_to_lich(&text) {
                Ok(Handled {
                    to_lich: Some(text),
                    ..Handled::default()
                })
            } else {
                // Nothing took the line: `hand_to_lich` said one of its two
                // recovered notices on the session, so it is printed here.
                say_new_notices(session, out)?;
                Ok(Handled::default())
            }
        }
        Route::Game(text) => {
            handle.log_typed(&text);
            handle.to_game(&text);
            Ok(Handled {
                to_game: Some(text),
                ..Handled::default()
            })
        }
    }
}

/// Run one `.`-prefixed line as a Hydra command.
///
/// `.lich …` is `SessionHandle::lich_command`
/// ([`hydra_session::lich_command::execute`]): it says its notices, and this
/// function prints them, writes `lich.json` back when the settings changed and
/// reports the start/stop request the relay loop has to carry out.  A
/// `.`-prefixed line that is not a command is answered locally and never sent
/// on.
fn command(text: &str, session: &Headless, out: &mut dyn Write) -> io::Result<Handled> {
    let handle = session.handle();
    let Some(outcome) = handle.lich_command(text) else {
        writeln!(out, "{}", not_a_command(text, session.door().symbol()))?;
        out.flush()?;
        return Ok(Handled::default());
    };
    let handled = Handled {
        start: outcome.start,
        stop: outcome.stop,
        ..Handled::default()
    };
    for notice in &outcome.notices {
        for line in &notice.lines {
            writeln!(out, "{line}")?;
        }
    }
    if outcome.changed {
        // The recovered settings file is global (`lich.json`, beside the other
        // data-directory files); `crate::lich::settings_path` is the same seam
        // `play` read it from.
        if let Err(error) = outcome.settings.save(&crate::lich::settings_path()) {
            writeln!(out, "[hydra] lich.json could not be written: {error}")?;
        }
    }
    out.flush()?;
    Ok(handled)
}

/// Print the notices the session has said and [`dispatch`] has not printed.
///
/// `SessionHandle::hand_to_lich` says its failure notice on the session rather
/// than returning it, so it is read back from [`SessionHandle::said`].
fn say_new_notices(session: &Headless, out: &mut dyn Write) -> io::Result<()> {
    if let Some(last) = session.handle().said().last() {
        writeln!(out, "{last}")?;
        out.flush()?;
    }
    Ok(())
}

/// One readable status line for this session.
///
/// **INFERRED** composition (see the module note): the character, whether Lich
/// runs and takes typing, how many typed lines wait, and whether the session is
/// still here — with the recovered [`SESSION_ENDED`] clause at the end.
pub fn status_line(session: &Headless, lich_takes_typing: bool) -> String {
    let handle = session.handle();
    let lich = match (handle.lich_running(), lich_takes_typing) {
        (true, true) => "Lich is running and takes typing",
        (true, false) => "Lich is running and is not taking typing",
        (false, true) => "Lich is not running, though it would take typing",
        (false, false) => "Lich is not running",
    };
    let end = if handle.stopping() {
        SESSION_ENDED
    } else {
        SESSION_RUNNING
    };
    let pending = handle.lich_pending().len();
    format!(
        "[status] {}: {lich}; {pending} typed line(s) wait; {end}",
        session.character()
    )
}

/// The two queues a live run fills, plus what the stdin thread asked for.
#[derive(Debug)]
pub struct Queues {
    /// lines the door took for Lich ([`hydra_session::lich::Claim::typed`])
    pub to_lich: Receiver<String>,
    /// lines the session's taker took for the game socket
    pub to_game: Receiver<String>,
    /// requests only the relay loop can honour (a `.lich off` stop)
    pub requests: Receiver<Handled>,
}

impl Queues {
    /// Take everything that is waiting, without blocking.
    pub fn take(&self) -> (Vec<String>, Vec<String>, Vec<Handled>) {
        (
            drain(&self.to_lich),
            drain(&self.to_game),
            drain(&self.requests),
        )
    }
}

/// Everything already on `queue`, in order.
fn drain<T>(queue: &Receiver<T>) -> Vec<T> {
    let mut out = Vec::new();
    while let Ok(item) = queue.try_recv() {
        out.push(item);
    }
    out
}

/// Hand every line of `reader` to `sink` until EOF.
///
/// The reader is a parameter so a test can drive the loop with a `&[u8]` and no
/// terminal; [`spawn_stdin`] passes this process's stdin.
pub fn pump<R: BufRead>(mut reader: R, mut sink: impl FnMut(&str)) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => sink(&line),
            // A read error ends the loop the way EOF does: a closed or broken
            // stdin must not spin.
            Err(_) => break,
        }
    }
}

/// The session a long-lived input thread dispatches to.
///
/// `hydra play` never changes its session, but a resting `hydra serve` can
/// (`--again` starts a fresh [`Headless`] when the game connection drops and
/// comes back), and the input thread outlives a connection.  The thread holds
/// this cell instead of one `Headless`, so the lines typed after the swap go to
/// the session that is running now — and no second input thread is needed.
#[derive(Clone, Debug)]
pub struct Current(Arc<Mutex<Headless>>);

impl Current {
    /// A cell holding `session`.
    pub fn new(session: Headless) -> Current {
        Current(Arc::new(Mutex::new(session)))
    }

    /// The session that is running now.
    pub fn get(&self) -> Headless {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Install `session` and hand back the one it replaced.
    pub fn replace(&self, session: Headless) -> Headless {
        let mut current = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::replace(&mut *current, session)
    }
}

/// Read this process's stdin on its own thread.
///
/// Every line goes through [`dispatch`]; the requests that need the relay loop
/// (a `.lich off`) come back on the receiver.  The thread ends at EOF or at the
/// first read error, and is deliberately not joined: it may be parked in a read
/// when the run ends, and joining a parked reader would hang the exit.
pub fn spawn_stdin(session: Current) -> io::Result<(Receiver<Handled>, thread::JoinHandle<()>)> {
    let (sender, requests) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("hydra-stdin".to_string())
        .spawn(move || {
            let stdin = io::stdin();
            let mut out = io::stdout();
            pump(stdin.lock(), |line| {
                if let Ok(handled) = dispatch(line, &session.get(), &mut out) {
                    if !handled.is_idle() {
                        // A dead receiver means the run is over.
                        let _ = sender.send(handled);
                    }
                }
            });
        })?;
    Ok((requests, handle))
}

/// How often [`spawn_spool`] looks in its folder for new files.
pub const SPOOL_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// The spool files in `dir`: every regular file whose name does not start with
/// `.`, in name order.
///
/// **ADDITION** (`hydra serve --in <dir>`, see `crates/hydra/NOTES.md` §8): the
/// reference build only *entered* the folder.  The hidden files a `.DS_Store`
/// or an editor leaves behind are skipped, so only what was meant as input is
/// read.
pub fn spooled(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| match path.file_name() {
            Some(name) => !name.to_string_lossy().starts_with('.'),
            None => false,
        })
        .collect();
    files.sort();
    files
}

/// Read typed input from a spool folder instead of this process's stdin.
///
/// **ADDITION**: `hydra serve --in <dir>` has no keyboard (it is usually
/// detached), so what would be typed is dropped into files in `dir` instead.
/// Each file is read line by line through [`dispatch`] — the same rules as
/// stdin, so `.lich`/`;status`/game-vs-Lich are identical — and removed once it
/// has been read.  The thread ends when the session stops or the receiver is
/// dropped, and is deliberately not joined: it may be asleep between polls.
pub fn spawn_spool(
    dir: PathBuf,
    session: Current,
) -> io::Result<(Receiver<Handled>, thread::JoinHandle<()>)> {
    let (sender, requests) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("hydra-spool".to_string())
        .spawn(move || {
            let mut out = io::stdout();
            loop {
                if session.get().handle().stopping() {
                    return;
                }
                for file in spooled(&dir) {
                    let Ok(handle) = std::fs::File::open(&file) else {
                        continue;
                    };
                    let mut keep_going = true;
                    pump(io::BufReader::new(handle), |line| {
                        if !keep_going {
                            return;
                        }
                        if let Ok(handled) = dispatch(line, &session.get(), &mut out) {
                            if !handled.is_idle() && sender.send(handled).is_err() {
                                // The relay loop is gone: this run is over.
                                keep_going = false;
                            }
                        }
                    });
                    // The file is consumed whether or not it was wanted: that is
                    // what "spool" means, and it keeps a failed line from being
                    // replayed forever.
                    let _ = std::fs::remove_file(&file);
                    if !keep_going {
                        return;
                    }
                }
                thread::sleep(SPOOL_POLL);
            }
        })?;
    Ok((requests, handle))
}

/// Watch for Ctrl-C while the run lasts.
///
/// **RECOVERED** shape (`0x100025b34`'s `tokio::select!` arm): Ctrl-C sends
/// `quit` and then cancels the session.  `quit` goes where every other typed
/// line goes — to Lich when it takes typing, to the game otherwise — and the
/// cancel is [`SessionHandle::stop`], which is what
/// [`crate::lich::relay_game`] watches.
pub fn spawn_ctrl_c(session: Current) -> io::Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("hydra-ctrl-c".to_string())
        .spawn(move || {
            // The signal driver needs a runtime of its own: this binary drives
            // the session synchronously, outside the runtime `main` built.
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            if runtime.block_on(tokio::signal::ctrl_c()).is_err() {
                return;
            }
            let session = session.get();
            let mut out = io::stdout();
            let _ = dispatch("quit", &session, &mut out);
            session.handle().stop("Ctrl-C");
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_session::claimant::Desk;
    use hydra_session::command::taker_pair;
    use hydra_session::lich::LichDoor;
    use hydra_session::lich_command::{LichSettings, RUNS_AND_STARTS, STAYS_OFF, USAGE};
    use std::io::Cursor;
    use std::path::PathBuf;
    use std::sync::Mutex;

    fn session() -> Headless {
        Headless::new("Aelith")
    }

    /// Point `lich.json` somewhere harmless: the `HYDRA_LICH_SETTINGS` seam.
    struct SettingsPath {
        previous: Option<std::ffi::OsString>,
    }

    impl SettingsPath {
        fn temp(tag: &str) -> (Self, PathBuf) {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|s| s.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir().join(format!(
                "hydra-typing-{tag}-{}-{stamp}.json",
                std::process::id()
            ));
            let previous = std::env::var_os(crate::lich::SETTINGS_ENV);
            std::env::set_var(crate::lich::SETTINGS_ENV, &path);
            (SettingsPath { previous }, path)
        }
    }

    impl Drop for SettingsPath {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(crate::lich::SETTINGS_ENV, value),
                None => std::env::remove_var(crate::lich::SETTINGS_ENV),
            }
        }
    }

    /// A session whose door takes typing, with the taker's receiving end.
    ///
    /// The claim is returned, not dropped: dropping it would close the door's
    /// typed channel, which is exactly the "Lich stopped reading" case.
    struct Wired {
        session: Headless,
        to_game: mpsc::Receiver<String>,
        _claim: hydra_session::lich::Claim,
    }

    fn wired() -> Wired {
        let session = session();
        let claim = session.door().claim();
        let (taker, to_game) = taker_pair();
        session.handle().set_taker(taker);
        Wired {
            session,
            to_game,
            _claim: claim,
        }
    }

    fn text(buffer: &[u8]) -> String {
        String::from_utf8(buffer.to_vec()).unwrap()
    }

    #[test]
    fn the_spool_skips_hidden_files_and_reads_the_rest() {
        let dir = std::env::temp_dir().join(format!("hydra-spool-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("b"), "second\n").unwrap();
        std::fs::write(dir.join("a"), "first\n").unwrap();
        // What an editor or the Finder leaves behind is not input.
        std::fs::write(dir.join(".DS_Store"), "not input\n").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();

        let names: Vec<String> = spooled(&dir)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_line_goes_nowhere() {
        assert_eq!(route("", '.', true), Route::Ignore);
        assert_eq!(route("\n", '.', true), Route::Ignore);
        assert_eq!(route(" \t \r\n", '.', true), Route::Ignore);
        // ... whether or not Lich takes typing.
        assert_eq!(route("   ", '.', false), Route::Ignore);
    }

    #[test]
    fn the_status_word_is_local_however_it_is_spaced() {
        assert_eq!(route(";status", '.', true), Route::Status);
        assert_eq!(route(";status", '.', false), Route::Status);
        assert_eq!(route("  ;status  ", '.', true), Route::Status);
        // The first word decides.
        assert_eq!(route(";status now", '.', true), Route::Status);
        // A line that merely starts with it is not the command.
        assert_eq!(
            route(";statuses", '.', true),
            Route::Lich(";statuses".into())
        );
        // Even when Hydra's symbol is `;`, the recovered word wins.
        assert_eq!(route(";status", ';', true), Route::Status);
    }

    #[test]
    fn a_line_with_hydras_symbol_is_hydras_alone() {
        assert_eq!(
            route(".lich on", '.', true),
            Route::Hydra(".lich on".into())
        );
        assert_eq!(
            route("  .lich off  ", '.', true),
            Route::Hydra(".lich off".into())
        );
        // A chosen symbol is respected, and `.` then means the game's own text.
        assert_eq!(
            route("!lich on", '!', false),
            Route::Hydra("!lich on".into())
        );
        assert_eq!(
            route(".lich on", '!', false),
            Route::Game(".lich on".into())
        );
    }

    #[test]
    fn everything_else_follows_who_takes_the_typing() {
        assert_eq!(route("look", '.', true), Route::Lich("look".into()));
        assert_eq!(route("look", '.', false), Route::Game("look".into()));
        // The line arrives with its newline from a reader; it never travels on
        // with one.
        assert_eq!(route("look\r\n", '.', true), Route::Lich("look".into()));
        assert_eq!(
            route("  go north  ", '.', false),
            Route::Game("go north".into())
        );
    }

    #[test]
    fn dispatch_hands_a_line_to_the_door_when_lich_takes_typing() {
        let wired = wired();
        let mut out = Vec::new();
        let handled = dispatch("look", &wired.session, &mut out).unwrap();
        assert_eq!(handled.to_lich.as_deref(), Some("look"));
        assert_eq!(handled.to_game, None);
        assert!(!handled.stop && handled.start.is_none());
        // It went to the door, and not to the taker.
        assert!(wired.to_game.try_recv().is_err());
        // ... and it is in the session's typed log (the log is on by default).
        assert_eq!(wired.session.handle().typed_log().len(), 1);
        assert!(wired.session.handle().typed_log()[0].ends_with("/look"));
        // Nothing was said to the player: the line simply left.
        assert_eq!(wired.session.handle().said(), Vec::<String>::new());
        assert_eq!(text(&out), "");
    }

    #[test]
    fn dispatch_writes_to_the_game_when_lich_does_not_take_typing() {
        // No door claimed: nothing takes typing.
        let session = session();
        assert!(!session.handle().lich_takes_typing());
        let (taker, to_game) = taker_pair();
        session.handle().set_taker(taker);

        let mut out = Vec::new();
        let handled = dispatch("look", &session, &mut out).unwrap();
        assert_eq!(handled.to_game.as_deref(), Some("look"));
        assert_eq!(handled.to_lich, None);
        assert_eq!(to_game.try_recv().unwrap(), "look");
        assert_eq!(text(&out), "");
    }

    #[test]
    fn a_lich_command_is_answered_locally_and_never_forwarded() {
        let wired = wired();
        let mut out = Vec::new();
        let handled = dispatch(".lich", &wired.session, &mut out).unwrap();

        // The recovered status sentences, both of them.
        let printed = text(&out);
        assert!(
            printed.contains("Lich is not running for this character"),
            "{printed}"
        );
        assert!(
            printed.contains("where Lich is has not been said"),
            "{printed}"
        );
        // Nothing was forwarded anywhere.
        assert!(handled.is_idle());
        assert!(wired.to_game.try_recv().is_err());
        assert!(!wired
            .session
            .handle()
            .typed_log()
            .iter()
            .any(|line| line.contains("lich")));
    }

    #[test]
    fn a_lich_command_reports_the_start_and_stop_it_asks_for() {
        let (settings, path) = SettingsPath::temp("start");
        // A folder with a `lich.rbw` in it, so `.lich on` is allowed.
        let folder = std::env::temp_dir().join(format!("hydra-typing-lich-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("lich.rbw"), "").unwrap();

        let session = session();
        let mut out = Vec::new();
        let line = format!(".lich folder {}", folder.display());
        let handled = dispatch(&line, &session, &mut out).unwrap();
        assert!(handled.is_idle());
        assert!(text(&out).contains("for every character"), "{}", text(&out));

        let handled = dispatch(".lich on", &session, &mut out).unwrap();
        assert_eq!(handled.start.as_deref(), Some(folder.as_path()));
        assert!(!handled.stop);
        assert!(
            text(&out).contains("Lich is starting, from"),
            "{}",
            text(&out)
        );
        // The settings were written to the seam, not to a developer's file.
        let saved = LichSettings::load(&path).unwrap();
        assert!(saved.on, "{saved:?}");
        assert_eq!(saved.folder.as_deref(), Some(folder.as_path()));

        // `.lich off` asks for the stop (`lich_running` is false here, so the
        // recovered `Lich stays off` answer is the one said).
        let handled = dispatch(".lich off", &session, &mut out).unwrap();
        assert!(!handled.stop);
        assert!(text(&out).contains(STAYS_OFF), "{}", text(&out));

        let _ = std::fs::remove_dir_all(&folder);
        let _ = std::fs::remove_file(&path);
        drop(settings);
    }

    #[test]
    fn a_lich_status_answers_with_the_recovered_sentences() {
        let running = session();
        running.handle().set_lich_settings(LichSettings {
            on: true,
            folder: Some(PathBuf::from("/synthetic/lich")),
            ..LichSettings::default()
        });
        let mut out = Vec::new();
        // The status is the bare `.lich`: the recovered usage table has no
        // `status` word (`hydra_session::lich_command::USAGE`).
        dispatch(".lich", &running, &mut out).unwrap();
        let printed = text(&out);
        assert!(printed.contains(RUNS_AND_STARTS), "{printed}");
        assert!(printed.contains("Lich is at /synthetic/lich."), "{printed}");

        // `.lich status` is still Hydra's, so it is answered locally with the
        // recovered usage line — and, like every `.`-prefixed line, never sent on.
        let plain = session();
        let (taker, to_game) = taker_pair();
        plain.handle().set_taker(taker);
        let mut out = Vec::new();
        let handled = dispatch(".lich status", &plain, &mut out).unwrap();
        assert!(handled.is_idle());
        assert!(to_game.try_recv().is_err());
        assert_eq!(text(&out).trim_end(), USAGE);
    }

    #[test]
    fn a_hydra_line_that_is_not_a_command_is_still_local() {
        let wired = wired();
        let mut out = Vec::new();
        let handled = dispatch(".wibble", &wired.session, &mut out).unwrap();
        assert!(handled.is_idle());
        assert!(wired.to_game.try_recv().is_err());
        let printed = text(&out);
        assert!(printed.contains("not a command Hydra knows"), "{printed}");
        assert!(printed.contains("never sent on"), "{printed}");
    }

    #[test]
    fn the_status_line_reports_the_five_facts() {
        let session = session();
        let line = status_line(&session, false);
        assert!(line.starts_with("[status] Aelith: "), "{line}");
        assert!(line.contains("Lich is not running"), "{line}");
        assert!(line.contains("0 typed line(s) wait"), "{line}");
        assert!(line.contains(SESSION_RUNNING), "{line}");
        assert!(!line.contains('\n'));

        let wired = wired();
        let line = status_line(&wired.session, true);
        assert!(line.contains("Lich is running and takes typing"), "{line}");

        // An ended session says so, with the recovered clause.
        wired.session.handle().stop("the test ended it");
        let line = status_line(&wired.session, true);
        assert!(line.contains(SESSION_ENDED), "{line}");
    }

    #[test]
    fn a_status_line_is_printed_and_forwarded_nowhere() {
        let wired = wired();
        let mut out = Vec::new();
        let handled = dispatch(";status", &wired.session, &mut out).unwrap();
        assert!(handled.is_idle());
        assert!(wired.to_game.try_recv().is_err());
        let printed = text(&out);
        assert_eq!(printed.lines().count(), 1, "{printed}");
        assert!(printed.starts_with("[status] Aelith: "), "{printed}");
    }

    #[test]
    fn an_empty_line_says_and_does_nothing() {
        let wired = wired();
        let mut out = Vec::new();
        assert!(dispatch("   \r\n", &wired.session, &mut out)
            .unwrap()
            .is_idle());
        assert_eq!(text(&out), "");
        assert!(wired.to_game.try_recv().is_err());
        assert!(wired.session.handle().typed_log().is_empty());
    }

    #[test]
    fn the_pump_runs_the_dispatch_over_every_line() {
        let wired = wired();
        let seen = Mutex::new(Vec::new());
        let mut out = Vec::new();
        pump(
            Cursor::new("look\n\n;status\r\n.lich\n".as_bytes()),
            |line| {
                let handled = dispatch(line, &wired.session, &mut out).unwrap();
                if let Some(to_lich) = handled.to_lich {
                    seen.lock().unwrap().push(to_lich);
                }
            },
        );
        assert_eq!(*seen.lock().unwrap(), vec!["look".to_string()]);
        assert!(wired.to_game.try_recv().is_err());
        let printed = text(&out);
        assert!(printed.contains("[status] Aelith: "), "{printed}");
        assert!(
            printed.contains("Lich is not running for this character"),
            "{printed}"
        );
    }

    #[test]
    fn the_queues_are_drained_in_order() {
        let (door_sender, to_lich) = mpsc::channel();
        let (game_sender, to_game) = mpsc::channel();
        let (request_sender, requests) = mpsc::channel();
        let queues = Queues {
            to_lich,
            to_game,
            requests,
        };
        door_sender.send("look".to_string()).unwrap();
        door_sender.send("quit".to_string()).unwrap();
        game_sender.send("go north".to_string()).unwrap();
        request_sender
            .send(Handled {
                stop: true,
                ..Handled::default()
            })
            .unwrap();

        let (lich, game, asked) = queues.take();
        assert_eq!(lich, vec!["look".to_string(), "quit".to_string()]);
        assert_eq!(game, vec!["go north".to_string()]);
        assert_eq!(asked.len(), 1);
        assert!(asked[0].stop);
        // A second take finds nothing.
        assert_eq!(queues.take(), (Vec::new(), Vec::new(), Vec::new()));
    }

    #[test]
    fn the_recovered_notices_are_the_binarys_bytes() {
        assert_eq!(
            CONNECTED,
            "\n[hydra] connected; type commands, Ctrl-C to quit\n\n"
        );
        assert_eq!(GAME_CLOSED, "\n[hydra] the game closed the connection\n");
        // 30 bytes in the reference build, `[status] the session has ended`;
        // this crate uses the sentence without the `[status] ` head, which the
        // status line it appears in already carries.
        assert_eq!(SESSION_ENDED.len(), "the session has ended".len());
        assert!(SESSION_ENDED.starts_with("the session has ended"));
        assert_eq!(STATUS_WORD, ";status");
        // The usage text really names the word.
        let _ = USAGE;
    }

    #[test]
    fn a_session_without_a_desk_still_has_a_symbol() {
        // `Headless::new` gives the default symbol, which is what `route` sees.
        assert_eq!(session().door().symbol(), '.');
        // ... and a chosen symbol is what dispatch passes on.
        let session = session();
        session.handle().set_symbol('!');
        assert_eq!(session.door().symbol(), '!');
        // `Desk::new` with no desk at all still answers `.` through the door.
        let bare = SessionHandle::new(Desk::new(None, "Aelith"));
        assert_eq!(LichDoor::new(bare).symbol(), '.');
    }
}
