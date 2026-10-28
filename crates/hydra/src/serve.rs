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
//! `hydra::gui::run`.
//!
//! # What this phase runs
//!
//! The whole session `hydra play` runs — the eaccess login, this character's
//! Lich, the game socket and [`crate::lich::relay_game`] — without a window:
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
//! * `--resume`, `--again` and `--open` are parsed (the recovered words) and
//!   reported as not wired: there is no captured-session store and no browser
//!   to open here.
//!
//! The run ends when the game or Lich does, and exits 0 — the same way `play`
//! does after the recovered `[status] the session has ended`.

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use hydra_session::command::taker_pair;
use hydra_session::lich_command::LichSettings;

use crate::lich;
use crate::login;
use crate::session;
use crate::startup;
use crate::typing;

/// The command's own name, as every message spells it.
pub const LABEL: &str = "hydra serve";

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

    /// The login arguments this command line amounts to.
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
        if !self.resumes.is_empty() {
            out.push("--resume (resuming a captured session needs hydra_table's session store)");
        }
        if self.again {
            out.push("--again (there is no captured session to start over from)");
        }
        if self.open {
            out.push("--open (there is no browser to open, and no window in this phase)");
        }
        out
    }
}

/// A web page thread, stopped and joined when this value is dropped.
struct Web {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl Web {
    /// Bind `port` and serve the synthetic page from its own thread.
    fn start(root: &std::path::Path, port: u16, character: &str) -> Result<Web, String> {
        let mut server = hydra_web::Server::new(root)?;
        let (listener, port) = server.bind(port)?;
        let page = server.page(character);
        let url = format!("http://127.0.0.1:{port}/#pairing={}", server.token);
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            let server = server.clone();
            std::thread::Builder::new()
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
fn session_run(options: &Options, settings: io::Result<LichSettings>) -> Result<(), String> {
    let settings = settings.map_err(|error| format!("{LABEL}: {error}"))?;
    if !settings.on {
        // The same first branch as `play` (and the same sentence).
        return Err(lich::off_message(LABEL));
    }
    let plan = lich::plan(&settings).map_err(|refusal| refusal.message())?;

    let arguments = options.arguments();
    let target = login::resolve(&arguments).map_err(|message| format!("{LABEL}: {message}"))?;
    let password =
        login::password(&target.account).map_err(|message| format!("{LABEL}: {message}"))?;
    let login = login::sign_in(&target, &password).map_err(|message| format!("{LABEL}: {message}"))?;
    let launch = login.launch().clone();

    let session = session::Headless::new(&arguments.character);
    session.handle().set_lich_settings(settings.clone());
    let mut keyed = lich::start(&plan)
        .map_err(|error| format!("{LABEL}: Lich could not be started: {error}"))?;
    let mut out = io::stdout();
    if let Err(error) = lich::announce(&plan, &mut out) {
        let _ = keyed.relay.stop();
        return Err(format!("{LABEL}: {error}"));
    }

    let mut socket = match login::open_game(&launch) {
        Ok(socket) => socket,
        Err(message) => {
            let _ = keyed.relay.stop();
            return Err(format!("{LABEL}: {message}"));
        }
    };

    // The recovered connected notice (`play` says the same one).
    let _ = write!(out, "{}", typing::CONNECTED);
    let _ = out.flush();

    // The page, if `--web` asked for one.  It is bound *before* the relay so a
    // port it cannot have is an error rather than a silent absence.
    let web = match options.web_port {
        Some(port) => match Web::start(
            &hydra_table::account::data_dir(),
            port,
            &arguments.character,
        ) {
            Ok(web) => Some(web),
            Err(message) => {
                // A port the page cannot have is a refusal, not a silent
                // absence: Lich is stopped on the way out (`Relay::drop` would
                // reap it too, but the recovered teardown stops first).
                let _ = keyed.relay.stop();
                return Err(format!("{LABEL}: {message}"));
            }
        },
        None => None,
    };

    // Typing: the spool folder when `--in` named one (an addition), stdin
    // otherwise.  Both fill the same two queues `relay_game` carries.
    let claim = session.door().claim();
    let (taker, to_game) = taker_pair();
    session.handle().set_taker(taker);
    let (requests, input_thread) = match &options.in_dir {
        Some(dir) => typing::spawn_spool(dir.clone(), session.clone()),
        None => typing::spawn_stdin(session.clone()),
    }
    .map_err(|error| format!("{LABEL}: the typed input could not be read: {error}"))?;
    let _ctrl_c = typing::spawn_ctrl_c(session.clone()).ok();
    let queues = typing::Queues {
        to_lich: claim.typed,
        to_game,
        requests,
    };

    let outcome = lich::relay_game(keyed, &mut socket, &mut out, session.handle(), queues);

    session.door().over();
    session.handle().stop(typing::SESSION_ENDED);
    let _ = writeln!(out, "[status] {}", typing::SESSION_ENDED);
    let _ = out.flush();
    // The spool thread ends on the stopped session; the web page is closed with
    // `web`, so nothing outlives the run.
    let _ = input_thread;
    drop(web);
    outcome.map_err(|error| format!("{LABEL}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
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
        assert_eq!(options.unwired().len(), 1, "{:?}", options.unwired());
        assert!(options.unwired()[0].starts_with("--resume"));

        // The recovered words are parsed, and the additions still say nothing.
        let options = Options::parse(&tokens(&["--open", "--again", "Aelith"])).unwrap();
        assert_eq!(options.unwired().len(), 2, "{:?}", options.unwired());
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
}
