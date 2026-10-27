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
//! * [`relay`] is the Lich-only relay `hydra serve` still uses: Lich's stdout is
//!   relayed to this process's stdout with the 0x1F chunk byte stripped
//!   ([`hydra_lich::wire::LichOut`]) until Lich exits or the session stops, and
//!   nothing is written to the accepted socket or read back from it.
//! * [`relay_game`] is the whole session `hydra play` runs once a real login has
//!   named the game socket: the game's lines go to Lich, Lich's frames go to the
//!   game, and Lich's stdout still reaches this process's stdout.  See its own
//!   documentation for what ends it.

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
/// It stops when the child is gone, the game reaches EOF, or the session stops
/// ([`SessionHandle::stopping`]); then it stops Lich, shuts both sockets down
/// (which is what unblocks the reader threads) and joins them.  A write that
/// fails because the other side has already closed is *not* an error: that is
/// what a Lich that exits immediately looks like.
pub fn relay_game(
    keyed: &mut KeyedRelay,
    game: &mut TcpStream,
    out: &mut dyn Write,
    session: &SessionHandle,
) -> io::Result<()> {
    // The keyed accept left a 60-second read timeout on Lich's socket, and this
    // loop reads for as long as the session lasts: clear it on both sockets.
    let _ = keyed.socket.set_read_timeout(None);
    let _ = game.set_read_timeout(None);

    let (game_sender, game_lines) = mpsc::channel::<String>();
    let game_socket = game.try_clone()?;
    let game_reader = thread::Builder::new()
        .name("hydra-game-reader".to_string())
        .spawn(move || read_into(game_socket, game_sender, |line| line))?;

    let (lich_sender, lich_frames) = mpsc::channel::<LichFrame>();
    let lich_socket = keyed.socket.try_clone()?;
    let lich_reader = thread::Builder::new()
        .name("hydra-lich-reader".to_string())
        .spawn(move || read_into(lich_socket, lich_sender, |line| LichFrame::parse(&line)))?;

    let mut printed = 0usize;
    let mut game_open = true;
    let mut lich_open = true;
    let outcome = loop {
        if session.stopping() {
            break Ok(());
        }
        let alive = match keyed.relay.pump() {
            Ok(alive) => alive,
            Err(error) => break Err(error),
        };
        if let Err(error) = drain(keyed, out, &mut printed) {
            break Err(error);
        }
        // game -> Lich
        loop {
            match game_lines.try_recv() {
                Ok(line) => {
                    let _ = keyed.send_game(&line);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    game_open = false;
                    break;
                }
            }
        }
        // Lich -> game.  A Lich that has closed its socket is not a reason to
        // stop on its own: the child's exit shows up in `pump`, and its stdout
        // may still be arriving.
        loop {
            if !lich_open {
                break;
            }
            match lich_frames.try_recv() {
                Ok(frame) => {
                    let _ = write_game_line(game, &frame);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    lich_open = false;
                    break;
                }
            }
        }
        if !alive || !game_open {
            break Ok(());
        }
    };

    // Teardown.  Lich may still have stdout in flight, so give it a bounded
    // grace period before the child is stopped (pump drains what has arrived).
    if outcome.is_ok() && keyed.relay.status().is_none() {
        let deadline = Instant::now() + TEARDOWN_GRACE;
        while keyed.relay.status().is_none() && Instant::now() < deadline {
            match keyed.relay.pump() {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => break,
            }
            // A stdout write that fails here ends the grace period; the child
            // is stopped below either way.
            if drain(keyed, out, &mut printed).is_err() {
                break;
            }
        }
    }
    let _ = keyed.relay.stop();
    let _ = drain(keyed, out, &mut printed);
    let _ = keyed.socket.shutdown(Shutdown::Both);
    let _ = game.shutdown(Shutdown::Both);
    let _ = game_reader.join();
    let _ = lich_reader.join();
    outcome
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
    let text = frame.text();
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

/// What `hydra play`/`hydra serve` say when `.lich` is off: there is nothing
/// for this headless phase to do, and it says so instead of doing nothing.
pub fn off_message(label: &str) -> String {
    format!(
        "{label}: Lich is off for this character, and this headless phase has no game \
         connection; it needs a live game connection (hydra_table's connector, hydra_net's \
         login) and Lich set to on (`lich.json` key \"on\", or `.lich on` in a session). \
         See crates/hydra/NOTES.md."
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
        assert!(message.contains("no game connection"), "{message}");
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
}
