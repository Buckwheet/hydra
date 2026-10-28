//! `hydra login` and `hydra play`: the real game login, and the session it
//! opens.
//!
//! # What a login is
//!
//! [`hydra_net::eaccess::handshake`] walks the eight eaccess stages
//! (`cert_pin`, `k_response`, `a_response`, `m_response`, `f_response`,
//! `g_response`, `c_response`, `l_response`) and hands back a
//! [`handshake::Login`]: the account, the chosen game and character, and the
//! launch (`GAMEHOST`/`GAMEPORT`/`KEY`).  This module is the part a binary
//! owns — where the account and the game come from, where the password and the
//! certificate pin live, and what each command does with the launch:
//!
//! * `hydra login <character> [--account A] [--game G]` prints one stage line
//!   per stage to **stderr**, opens the game socket
//!   ([`hydra_net::eaccess::game::connect`]) and copies the raw XML stream to
//!   **stdout** until the game closes;
//! * `hydra play …` prints the same stage lines, then starts this character's
//!   Lich ([`crate::lich`]), announces the two recovered start lines, opens the
//!   game socket and relays until either side ends
//!   ([`crate::lich::relay_game`]).  What the player types on stdin is read on
//!   its own thread ([`crate::typing`]): a bare line goes to Lich while it takes
//!   typing and to the game otherwise, `;status` and the `.`-prefixed commands
//!   are answered locally, and Ctrl-C sends `quit` and ends the run.
//!
//! # Where the values come from
//!
//! | what | from |
//! | --- | --- |
//! | the character | the first token that is not an option |
//! | the account | `--account`, else the roster entry for the character, else the character's own name |
//! | the game | `--game`, else the roster entry's `game_code`, else `GS3` |
//! | the data folder | `hydra_table::account::data_dir()` (`HYDRA_DATA_DIR`, else `data`), created if need be |
//! | the pin | `<data folder>/simu.pem` (trust on first use, `hydra_net::eaccess::pin`) |
//! | the eaccess endpoint | `HYDRA_EACCESS_HOST` (else `eaccess.play.net`) and `HYDRA_EACCESS_PORT` (else 7910) |
//! | the password | `hydra_session::password::kept` (the environment only), else `hydra_table::account::password` (keychain, then a prompt) |
//!
//! The two `HYDRA_EACCESS_*` variables are the test seam: every test points them
//! at a loopback mock ([`hydra_mocks`]), and nothing here ever resolves a real
//! game host on its own.
//!
//! # What is never printed
//!
//! The account holder's name, the server's key, the keyed password, the session
//! key and the launch key.  The stage lines are the original's
//! (`[login] [stage: cert_pin]`, …) and nothing more; a refusal is the login
//! crate's own sentence, which carries the stage and no key material.

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

use hydra_net::eaccess::game;
use hydra_net::eaccess::handshake::{self, Login, LoginConfig, Stage};
use hydra_session::command::taker_pair;
use hydra_session::lich_command::LichSettings;

use crate::lich;
use crate::session;
use crate::typing;

/// Where the login dials: `HYDRA_EACCESS_HOST` overrides
/// [`handshake::EACCESS_HOST`].  Tests point it at `127.0.0.1`.
pub const HOST_ENV: &str = "HYDRA_EACCESS_HOST";

/// The login's port: `HYDRA_EACCESS_PORT` overrides
/// [`handshake::EACCESS_PORT`].
pub const PORT_ENV: &str = "HYDRA_EACCESS_PORT";

/// The game played when nothing says otherwise (and when the roster is silent).
pub const DEFAULT_GAME: &str = "GS3";

/// The certificate pin's file name inside the data folder (the reference
/// build's `simu.pem`).
pub const PIN_FILE: &str = "simu.pem";

/// A parsed `hydra login`/`hydra play` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arguments {
    /// The character to play.
    pub character: String,
    /// `--account`, if it was given.
    pub account: Option<String>,
    /// `--game`, if it was given.
    pub game: Option<String>,
}

/// Parse `[<character>] [--account <account>] [--game <code>]`.
///
/// The first token that is not an option is the character; `--account` and
/// `--game` take the next token; any other option is refused rather than
/// ignored (a silently dropped `--acount` would log in as the wrong account).
pub fn parse(tokens: &[String]) -> Result<Arguments, String> {
    let mut character: Option<String> = None;
    let mut account: Option<String> = None;
    let mut game: Option<String> = None;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index].as_str();
        match token {
            "--account" | "--game" => {
                let value = tokens.get(index + 1).ok_or_else(|| {
                    format!("{token} needs a value: `hydra login <character> [--account <account>] [--game <code>]`")
                })?;
                if value.starts_with('-') {
                    return Err(format!("{token} needs a value, and {value} looks like an option"));
                }
                if token == "--account" {
                    account = Some(value.clone());
                } else {
                    game = Some(value.clone());
                }
                index += 2;
            }
            other if other.starts_with('-') => {
                return Err(format!(
                    "{other} is not an option this command takes: it takes --account <account> and --game <code>"
                ));
            }
            other => {
                if let Some(named) = &character {
                    return Err(format!(
                        "one character at a time: {other} came after {named}"
                    ));
                }
                character = Some(other.to_string());
                index += 1;
            }
        }
    }
    let character = character.ok_or_else(|| {
        "no character was named: `hydra login <character> [--account <account>] [--game <code>]`"
            .to_string()
    })?;
    Ok(Arguments {
        character,
        account,
        game,
    })
}

/// Everything the login needs resolved from the command line, the roster and the
/// environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The character to play.
    pub character: String,
    /// The account whose password is used.
    pub account: String,
    /// The game code to ask for.
    pub game: String,
    /// `<data folder>/simu.pem`.
    pub pin: PathBuf,
    /// The eaccess host.
    pub host: String,
    /// The eaccess port.
    pub port: u16,
}

/// [`Target`] from the real environment: the data folder, then
/// `HYDRA_EACCESS_HOST`/`HYDRA_EACCESS_PORT`.
pub fn resolve(arguments: &Arguments) -> Result<Target, String> {
    resolve_from(
        arguments,
        &hydra_table::account::data_dir(),
        std::env::var(HOST_ENV).ok(),
        std::env::var(PORT_ENV).ok(),
    )
}

/// [`resolve`] with the environment passed in, so the tests never mutate it.
pub fn resolve_from(
    arguments: &Arguments,
    data_dir: &Path,
    host: Option<String>,
    port: Option<String>,
) -> Result<Target, String> {
    std::fs::create_dir_all(data_dir).map_err(|error| {
        format!(
            "the data folder {} could not be made: {error}",
            data_dir.display()
        )
    })?;
    // The roster names the account and the game for a character it knows; an
    // unreadable or ambiguous roster is treated as "not known" rather than as a
    // login failure, because the command line can say both.
    let entry = hydra_table::roster::find(data_dir, &arguments.character)
        .ok()
        .flatten();
    let account = arguments
        .account
        .clone()
        .or_else(|| entry.as_ref().map(|entry| entry.account.clone()))
        .filter(|account| !account.is_empty())
        .unwrap_or_else(|| arguments.character.clone());
    let game = arguments
        .game
        .clone()
        .or_else(|| entry.as_ref().map(|entry| entry.game_code.clone()))
        .filter(|game| !game.is_empty())
        .unwrap_or_else(|| DEFAULT_GAME.to_string());
    let host = host
        .map(|host| host.trim().to_string())
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| handshake::EACCESS_HOST.to_string());
    let port = match port {
        Some(port) => port.trim().parse::<u16>().map_err(|_| {
            format!("{PORT_ENV} is not a port: {port:?}")
        })?,
        None => handshake::EACCESS_PORT,
    };
    Ok(Target {
        character: arguments.character.clone(),
        account,
        game,
        pin: data_dir.join(PIN_FILE),
        host,
        port,
    })
}

/// The account password: the environment's copy first (never printed), else the
/// keychain, else an interactive prompt.
pub fn password(account: &str) -> Result<String, String> {
    if let Some(password) = hydra_session::password::kept(account) {
        return Ok(password);
    }
    hydra_table::account::password(account)
        .map_err(|error| format!("no password for the account {account}: {error}"))
}

/// One stage line, exactly as the original prints it.
pub fn stage_line(stage: Stage) -> String {
    format!("[login] [stage: {}]", stage.name())
}

/// Say one stage on stderr (the original's progress line).
fn say_stage(stage: Stage) {
    eprintln!("{}", stage_line(stage));
}

/// Log in for `target` and print every stage on stderr.
///
/// `pub(crate)` so `hydra serve` runs exactly this login (same stages, same
/// refusals) rather than a second copy of it.
pub(crate) fn sign_in(target: &Target, password: &str) -> Result<Login, String> {
    let config = LoginConfig {
        account: &target.account,
        character: &target.character,
        game: Some(&target.game),
        password: password.as_bytes(),
        pin_path: &target.pin,
    };
    handshake::login_tls(&target.host, target.port, &config, &mut say_stage)
        .map_err(|refusal| refusal.to_string())
}

/// Open the game socket the login named.
///
/// `pub(crate)` for the same reason as [`sign_in`].
pub(crate) fn open_game(launch: &handshake::LaunchOwned) -> Result<TcpStream, String> {
    eprintln!("[login] [stage: game_connect]");
    game::connect(&launch.host, launch.port, &launch.key).map_err(|error| {
        format!(
            "the game socket at {}:{} could not be opened: {error}",
            launch.host, launch.port
        )
    })
}

/// `hydra login <character> [--account A] [--game G]`: log in and stream the
/// game's raw XML to stdout until the game closes.
///
/// A failure is one line on stderr, prefixed with the command's name; a refusal
/// from the login crate already names its stage (`[stage: cert_pin] …`).
pub fn login(tokens: &[String]) -> Result<(), String> {
    log_in(tokens).map_err(|message| format!("hydra login: {message}"))
}

/// The body of [`login`], without the label.
fn log_in(tokens: &[String]) -> Result<(), String> {
    let arguments = parse(tokens)?;
    let target = resolve(&arguments)?;
    let password = password(&target.account)?;
    let login = sign_in(&target, &password)?;
    let launch = login.launch().clone();

    let mut socket = open_game(&launch)?;
    let stdout = io::stdout();
    let mut out = stdout.lock();
    io::copy(&mut socket, &mut out)
        .map_err(|error| format!("the game stream ended badly: {error}"))?;
    out.flush()
        .map_err(|error| format!("the game stream could not be written out: {error}"))
}

/// `hydra play <character> [--account A] [--game G]`: log in, start Lich, relay.
pub fn play(tokens: &[String]) -> Result<(), String> {
    play_with("hydra play", tokens, lich::load_settings())
}

/// The body of [`play`], with the settings handed in (the Lich-only branches are
/// reachable from a unit test that way, with no file and no network).
///
/// The order is the one the reference build's `play` used and the one the
/// refusal sentences assume: `.lich` off says so and stops before anything
/// else; a Lich folder/script/Ruby that is missing is reported before a socket
/// is opened; only then is the login made and Lich started.
pub fn play_with(
    label: &str,
    tokens: &[String],
    settings: io::Result<LichSettings>,
) -> Result<(), String> {
    let settings = settings.map_err(|error| format!("{label}: {error}"))?;
    if !settings.on {
        // The off message names the command itself.
        return Err(lich::off_message(label));
    }
    let arguments = parse(tokens).map_err(|message| format!("{label}: {message}"))?;
    // A Lich that cannot be found is reported before anything is dialled, and
    // in the reference build's own words (they name the command already).
    let plan = lich::plan(&settings).map_err(|refusal| refusal.message())?;

    let target = resolve(&arguments).map_err(|message| format!("{label}: {message}"))?;
    let password = password(&target.account).map_err(|message| format!("{label}: {message}"))?;
    let login = sign_in(&target, &password).map_err(|message| format!("{label}: {message}"))?;
    let launch = login.launch().clone();

    let session = session::Headless::new(&arguments.character);
    // The session's own view of `lich.json`: `.lich status`, `.lich on` and
    // `.lich off` answer for the file this run was started from rather than for
    // the defaults (`hydra_session::lich_command` keeps the settings on the
    // session).
    session.handle().set_lich_settings(settings.clone());
    let mut keyed = lich::start(&plan)
        .map_err(|error| format!("{label}: Lich could not be started: {error}"))?;
    let mut out = io::stdout();
    if let Err(error) = lich::announce(&plan, &mut out) {
        let _ = keyed.relay.stop();
        return Err(format!("{label}: {error}"));
    }

    let mut socket = match open_game(&launch) {
        Ok(socket) => socket,
        Err(message) => {
            let _ = keyed.relay.stop();
            return Err(format!("{label}: {message}"));
        }
    };

    // The recovered connected notice (`hydra::play::run::{{closure}}`'s literal
    // pool): the game socket is open, so typing is worth announcing.
    let _ = write!(out, "{}", typing::CONNECTED);
    let _ = out.flush();

    // Typing.  Claiming the door is what makes `lich_takes_typing()` true, so
    // from here until Lich stops a typed line goes to Lich (`docs/LICH_WIRE.md`
    // §3); with no door claimed it goes to the game through the session's taker.
    // The claim's own line channel is unused: Lich's lines are read off the
    // keyed socket by `relay_game`, as before.
    let claim = session.door().claim();
    let (taker, to_game) = taker_pair();
    session.handle().set_taker(taker);
    let (requests, _stdin_thread) = typing::spawn_stdin(session.clone())
        .map_err(|error| format!("{label}: stdin could not be read: {error}"))?;
    // Ctrl-C sends `quit` and then cancels the session, which ends the relay.
    // The watcher is never joined: it may be parked in a signal read when the
    // run ends, and joining it would hold the exit.
    let _ctrl_c = typing::spawn_ctrl_c(session.clone()).ok();
    let queues = typing::Queues {
        to_lich: claim.typed,
        to_game,
        requests,
    };

    let outcome = lich::relay_game(keyed, &mut socket, &mut out, session.handle(), queues);

    // Lich does not take typing any more, and the session is over: the recovered
    // `[status] the session has ended`.
    session.door().over();
    session.handle().stop(typing::SESSION_ENDED);
    let _ = writeln!(out, "[status] {}", typing::SESSION_ENDED);
    let _ = out.flush();
    outcome.map_err(|error| format!("{label}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tokens(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    /// A temp folder, removed when the test ends.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or(0);
            let path = std::env::temp_dir()
                .join(format!("hydra-login-{tag}-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&path).expect("cannot create the temp folder");
            TempDir { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn the_character_is_the_first_token_that_is_not_an_option() {
        assert_eq!(
            parse(&tokens(&["Aelith"])).unwrap(),
            Arguments {
                character: "Aelith".into(),
                account: None,
                game: None
            }
        );
        assert_eq!(
            parse(&tokens(&["Aelith", "--account", "main", "--game", "GSF"])).unwrap(),
            Arguments {
                character: "Aelith".into(),
                account: Some("main".into()),
                game: Some("GSF".into())
            }
        );
        // The options may come first.
        assert_eq!(
            parse(&tokens(&["--game", "GS3", "Aelith"])).unwrap(),
            Arguments {
                character: "Aelith".into(),
                account: None,
                game: Some("GS3".into())
            }
        );
    }

    #[test]
    fn a_missing_or_unknown_option_is_refused() {
        // No character at all.
        assert!(parse(&[]).unwrap_err().contains("no character was named"));
        assert!(parse(&tokens(&["--account", "main"]))
            .unwrap_err()
            .contains("no character was named"));
        // An option with no value, or with another option where the value goes.
        assert!(parse(&tokens(&["Aelith", "--account"]))
            .unwrap_err()
            .contains("--account needs a value"));
        assert!(parse(&tokens(&["Aelith", "--game", "--account", "main"]))
            .unwrap_err()
            .contains("looks like an option"));
        // An option neither command takes is refused, not ignored.
        assert!(parse(&tokens(&["Aelith", "--acount", "main"]))
            .unwrap_err()
            .contains("not an option this command takes"));
        // Two characters.
        assert!(parse(&tokens(&["Aelith", "Bryn"]))
            .unwrap_err()
            .contains("one character at a time"));
    }

    #[test]
    fn the_target_falls_back_to_the_character_and_to_gs3() {
        let dir = TempDir::new("defaults");
        let target = resolve_from(
            &parse(&tokens(&["Aelith"])).unwrap(),
            &dir.path,
            None,
            None,
        )
        .unwrap();
        assert_eq!(target.character, "Aelith");
        assert_eq!(target.account, "Aelith");
        assert_eq!(target.game, DEFAULT_GAME);
        assert_eq!(target.pin, dir.path.join(PIN_FILE));
        assert_eq!(target.host, handshake::EACCESS_HOST);
        assert_eq!(target.port, handshake::EACCESS_PORT);
        assert!(dir.path.is_dir(), "the data folder is created");
    }

    #[test]
    fn the_roster_names_the_account_and_the_game() {
        let dir = TempDir::new("roster");
        // The roster is `roster.json`, keyed `game_code:character`.  A bare word
        // is matched against an entry's *account* (the recovered `roster::find`),
        // which is how a character whose account is `aelith` is found.
        fs::write(
            dir.path.join("roster.json"),
            concat!(
                "{\"schema_version\":1,\"characters\":{",
                "\"dr:aelith\":{\"character\":\"Aelith\",\"account\":\"aelith\",",
                "\"game_code\":\"DR\"}}}\n"
            ),
        )
        .unwrap();
        let target = resolve_from(
            &parse(&tokens(&["Aelith"])).unwrap(),
            &dir.path,
            None,
            None,
        )
        .unwrap();
        // Both values came from the roster: the fallbacks would be `Aelith`
        // (the character) and `GS3`.
        assert_eq!(target.account, "aelith");
        assert_eq!(target.game, "DR");

        // The command line still wins over the roster.
        let target = resolve_from(
            &parse(&tokens(&["Aelith", "--account", "main", "--game", "GSF"])).unwrap(),
            &dir.path,
            None,
            None,
        )
        .unwrap();
        assert_eq!(target.account, "main");
        assert_eq!(target.game, "GSF");
    }

    #[test]
    fn the_two_environment_variables_point_the_login_somewhere_else() {
        let dir = TempDir::new("endpoint");
        let arguments = parse(&tokens(&["Aelith"])).unwrap();
        let target = resolve_from(
            &arguments,
            &dir.path,
            Some("127.0.0.1".to_string()),
            Some(" 4242 ".to_string()),
        )
        .unwrap();
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 4242);

        // An empty host falls back to the real one; a bad port is reported.
        let target = resolve_from(&arguments, &dir.path, Some(String::new()), None).unwrap();
        assert_eq!(target.host, handshake::EACCESS_HOST);
        let refusal =
            resolve_from(&arguments, &dir.path, None, Some("eaccess".to_string())).unwrap_err();
        assert!(refusal.contains("is not a port"), "{refusal}");
    }

    #[test]
    fn the_stage_line_is_the_recovered_shape() {
        assert_eq!(stage_line(Stage::CertPin), "[login] [stage: cert_pin]");
        assert_eq!(
            stage_line(Stage::LResponse),
            "[login] [stage: l_response]"
        );
    }

    #[test]
    fn lich_off_is_said_before_anything_else_happens() {
        // The settings say off: no parse error, no folder, no network.
        let message = play_with("hydra play", &[], Ok(LichSettings::default())).unwrap_err();
        assert!(message.starts_with("hydra play: Lich is off"), "{message}");

        // On, but no folder has been said: the recovered sentence, and the
        // missing character did not get in the way.
        let on = LichSettings {
            on: true,
            ..LichSettings::default()
        };
        assert_eq!(
            play_with("hydra play", &tokens(&["Aelith"]), Ok(on)).unwrap_err(),
            "Where Lich is has not been said: lich folder <folder>."
        );

        // A settings file that cannot be read is the label's own message.
        let error = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let message = play_with("hydra play", &[], Err(error)).unwrap_err();
        assert_eq!(message, "hydra play: denied");
    }
}
