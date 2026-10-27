//! The `table` — Hydra's hub of seats, and the commands that change the roster.
//!
//! Recovered from `hydra_table::table` (`0x101292970` … `0x101294c54`).
//!
//! The recovered `Table` layout, from the fields the methods touch:
//!
//! | offset | what |
//! | --- | --- |
//! | `+0x28` | `hydra_board::sessions::Sessions` (`Table::show`, `0x101293404`) |
//! | `+0x108` | a `LazyLock<Mutex<Option<String>>>` (`Table::restarting`, `0x101293284`) |
//! | `+0x118` | the restart message itself (`str x8, [x19, #0x118]`) |
//! | `+0x160` | the account name `Table::start` clones (`0x100136874`) |
//!
//! `hydra_board` — the seat/session board — is **not** part of this
//! reconstruction, so [`crate::seat`] holds the two stand-ins this module needs
//! (`Sessions`, `Seat`) and the drawing seams are documented.

use std::path::Path;
use std::sync::Mutex;

use crate::account;
use crate::pages;
use crate::roster;
use crate::seat::{Seat, Sessions};

/// "The roster could not be changed: …"
///
/// **ESTABLISHED**: the format string at `0x103e63e18`, used by
/// `table::nickname_set` (`0x101292e34`), `table::forget` (`0x10129483c`) and
/// `table::remember` (`0x1012949c8`).
pub const ROSTER_NOT_CHANGED: &str = "The roster could not be changed: ";

/// "{character} is off the roster."
///
/// **ESTABLISHED**: the format string at `0x103e63ece`.
pub const OFF_ROSTER: &str = " is off the roster.";

/// "{character} is on the roster."
///
/// **ESTABLISHED**: the format string at `0x103e63ee6`.
pub const ON_ROSTER: &str = " is on the roster.";

/// "{character} is no longer a favourite."
///
/// **ESTABLISHED**: the format string at `0x103e63ef9`.
pub const NO_LONGER_FAVOURITE: &str = " is no longer a favourite.";

/// "{character} is a favourite."
///
/// **ESTABLISHED**: the format string at `0x103e63f19`.
pub const IS_FAVOURITE: &str = " is a favourite.";

/// "{what} is not on the roster."
///
/// **ESTABLISHED**: the format string at `0x103e228a7`, shared with
/// `table::nickname_set`.
pub const NOT_ON_ROSTER: &str = " is not on the roster.";

/// "{character} goes by its name."
///
/// **ESTABLISHED**: the format string at `0x103e63e03`.
pub const GOES_BY_ITS_NAME: &str = " goes by its name.";

/// The hub's seat table.
///
/// **ESTABLISHED** (see the module table): a `Sessions` at `+0x28`, the restart
/// message behind a `Mutex` at `+0x108`/`+0x118`, and the account name at
/// `+0x160`.
#[derive(Debug, Default)]
pub struct Table {
    /// The sessions the board knows about (`+0x28`).
    pub sessions: Sessions,
    /// The seats, one per character.
    pub seats: Vec<Seat>,
    /// The account this Hydra is logged in as (`+0x160`).
    pub account: String,
    /// The message a restart left behind (`+0x118`, behind the mutex at `+0x108`).
    restarting: Mutex<Option<String>>,
}

/// A resume request left by the last run.
///
/// **ESTABLISHED** (`table::Resume::arg`, `0x101294630`): the argument lives in a
/// `String` at `+0x18` of the `Resume` value (`ldr x10, [x9, #0x18]!`) and is
/// cloned when it is not the `Option` niche (`isize::MIN`).
/// `Resume::read` (`0x101294690`) reads it back from the resume file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resume {
    /// The resume file's path, if it was looked for.
    pub path: Option<std::path::PathBuf>,
    /// The character to resume, if any.
    pub arg: Option<String>,
}

impl Resume {
    /// The character this resume names.
    ///
    /// **ESTABLISHED** (`0x101294630`): `self.arg.clone()`, i.e. the `String` at
    /// `+0x18` when it is not `None`.
    pub fn arg(&self) -> Option<String> {
        self.arg.clone()
    }

    /// Read the resume request out of `dir`.
    ///
    /// **ESTABLISHED** (`Resume::read`, `0x101294690`): reads a file from the
    /// data directory and keeps its trimmed content as [`Resume::arg`].
    ///
    /// **UNKNOWN**: the file's name — the original passes it in, so the caller
    /// names it here.
    pub fn read(dir: &Path, file: &str) -> Resume {
        let path = dir.join(file);
        let arg = std::fs::read_to_string(&path)
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        Resume {
            path: Some(path),
            arg,
        }
    }
}

impl Table {
    /// Take the message a restart left behind.
    ///
    /// **ESTABLISHED** (`Table::restarting`, `0x10129326c`): lock the `Mutex` at
    /// `+0x108`, take the `Option<String>` at `+0x118` (`std::mem::take`), and
    /// write the `None` niche (`isize::MIN`) back; the poison flag is handled by
    /// the standard `Mutex`.
    pub fn restarting(&self) -> Option<String> {
        self.restarting.lock().unwrap().take()
    }

    /// Record the message a restart left behind.
    pub fn set_restarting(&self, message: impl Into<String>) {
        *self.restarting.lock().unwrap() = Some(message.into());
    }

    /// Whether `who` is seated in this hub.
    ///
    /// **ESTABLISHED** (`Table::seated`, `0x101293410`): a walk over the seats;
    /// **INFERRED**: the comparison is the lower-cased character name, the same
    /// key the rest of the crate uses.
    pub fn seated(&self, who: &str) -> bool {
        let who = who.trim().to_lowercase();
        self.seats.iter().any(|seat| seat.name.to_lowercase() == who)
    }

    /// Whether `who` is running in this hub.
    ///
    /// **ESTABLISHED** (`Table::running`, `0x1012937f8`): a walk over the seats
    /// that also checks the live sessions; read through [`Sessions::glance`].
    pub fn running(&self, who: &str) -> bool {
        let who = who.trim().to_lowercase();
        self.sessions
            .glance(std::time::Instant::now())
            .iter()
            .any(|live| live.character.to_lowercase() == who)
    }

    /// Raise `seat`'s window and tell the board which window is open.
    ///
    /// **ESTABLISHED** (`Table::show`, `0x1012933d8`):
    /// `hydra_board::seat::Seat::keep_window(seat, true)` (`0x1013e651c`) then
    /// `Sessions::open_windows(&self.sessions, 2, seat.id)` (`0x1013e9cc4`, the id
    /// read from `[seat, #0xaa0]`).
    pub fn show(&self, seat: &mut Seat) {
        seat.kept = true;
        self.sessions.open_windows(2, seat.id);
    }

    /// The settings file for `who`.
    ///
    /// **ESTABLISHED** (`Table::settings`, `0x101293e6c`): the character's
    /// settings file, resolved through [`pages`].
    pub fn settings(&self, root: &Path, who: &str) -> Result<std::path::PathBuf, String> {
        pages::apply(root, who)
    }

    /// Build a table around a game runtime.
    ///
    /// **ESTABLISHED** (`Table::with_game_runtime`, `0x101293358`): copies the
    /// `Sessions` (0x40 bytes at `+0`), takes the runtime `Handle` (16 bytes) and
    /// calls `hydra_host::Host::on` (`0x1013b054c`) before `Table::with_host`
    /// (`0x101293f78`).
    ///
    /// **Departure**: the runtime handle is `hydra_host::Handle`, since `tokio` is
    /// not available.
    pub fn with_game_runtime(sessions: Sessions, account: &str) -> Table {
        Table {
            sessions,
            seats: Vec::new(),
            account: account.to_string(),
            restarting: Mutex::new(None),
        }
    }

    /// Build a table for a host that already has seats (`Table::with_host`,
    /// `0x101293f78`).
    pub fn with_host(sessions: Sessions, seats: Vec<Seat>, account: &str) -> Table {
        Table {
            sessions,
            seats,
            account: account.to_string(),
            restarting: Mutex::new(None),
        }
    }
}

/// Remember a character on the roster.
///
/// **ESTABLISHED** (`table::remember`, `0x1012949c8`): two `trim_matches` calls
/// (the character and the game code), `roster::record` (`0x10129d814`) with an
/// entry built like [`roster::Entry::of`], an I/O failure formatted as
/// [`ROSTER_NOT_CHANGED`] and the answer `format!("{} is on the roster.")`.
pub fn remember(dir: &Path, character: &str, account: &str, game_code: &str) -> String {
    let entry = roster::Entry {
        character: character.trim().to_string(),
        account: account.trim().to_string(),
        game_code: game_code.trim().to_string(),
        favourite: false,
        nickname: None,
    };
    match roster::record(dir, &entry) {
        Ok(()) => format!("{} is on the roster.", entry.character),
        Err(error) => format!("{ROSTER_NOT_CHANGED}{error}"),
    }
}

/// Take a character off the roster.
///
/// **ESTABLISHED** (`table::forget`, `0x10129483c`): `roster::forget`
/// (`0x10129d45c`), then [`OFF_ROSTER`] or, when the name was not there,
/// [`NOT_ON_ROSTER`] — both formats are in the same `0x103e63ece` blob.
pub fn forget(dir: &Path, what: &str) -> String {
    match roster::find(dir, what) {
        Ok(None) => format!("{what}{NOT_ON_ROSTER}"),
        Ok(Some(entry)) => match roster::forget(dir, what) {
            Ok(()) => format!("{} is off the roster.", entry.character),
            Err(error) => format!("{ROSTER_NOT_CHANGED}{error}"),
        },
        Err(error) => format!("{ROSTER_NOT_CHANGED}{error}"),
    }
}

/// Set the nickname of a character.
///
/// **ESTABLISHED** (`table::nickname_set`, `0x101292e34`): the nickname is
/// trimmed; empty means "no nickname" (`isize::MIN` written over the slice at
/// `sp+0x90`), then `roster::nickname`, then one of three answers — an I/O error
/// ([`ROSTER_NOT_CHANGED`]), a miss ([`NOT_ON_ROSTER`]), or
/// `format!("{} goes by {}.", character, nickname)` / [`GOES_BY_ITS_NAME`].
pub fn nickname_set(dir: &Path, what: &str, nickname: &str) -> String {
    let nickname = nickname.trim();
    let wanted = if nickname.is_empty() {
        None
    } else {
        Some(nickname.to_string())
    };
    match roster::nickname(dir, what, wanted.clone()) {
        Ok(Some(entry)) => match wanted {
            Some(nickname) => format!("{} goes by {nickname}.", entry.character),
            None => format!("{}{GOES_BY_ITS_NAME}", entry.character),
        },
        Ok(None) => format!("{what}{NOT_ON_ROSTER}"),
        Err(error) => format!("{ROSTER_NOT_CHANGED}{error}"),
    }
}

/// Mark a character as a favourite, or not.
///
/// **ESTABLISHED** (`table::favourite`, `0x101294c54` — the 19 KB body is
/// `roster::favourite` plus its two answers): `roster::favourite`
/// (`0x10129e584`) then [`IS_FAVOURITE`] or [`NO_LONGER_FAVOURITE`].
pub fn favourite(dir: &Path, what: &str, on: bool) -> String {
    match roster::favourite(dir, what, on) {
        Ok(Some(entry)) => {
            if on {
                format!("{} is a favourite.", entry.character)
            } else {
                format!("{} is no longer a favourite.", entry.character)
            }
        }
        Ok(None) => format!("{what}{NOT_ON_ROSTER}"),
        Err(error) => format!("{ROSTER_NOT_CHANGED}{error}"),
    }
}

/// Forget the password kept for a character's account.
///
/// **ESTABLISHED** (`table::forget_password`, `0x1012930e8`): `account::forget`
/// (`0x1011f77d0`) and `account::env_name` (`0x1011f7a6c`) — the answer names the
/// environment variable, never the password.
pub fn forget_password(what: &str) -> String {
    let variable = account::env_name(what);
    match account::forget(what) {
        Ok(()) => format!("{variable} is forgotten."),
        // UNKNOWN: `{variable} is forgotten.` is recovered; the failure
        // wording is not in the pool.
        Err(error) => format!("{variable} is not forgotten: {error}"),
    }
}

/// The characters a typed line is given to.
///
/// **ESTABLISHED** (`table::give_typing`, `0x101292970`): reads `HYDRA_DATA_DIR`
/// (the 14-byte literal at `0x10240740e`) and calls `sync::start`
/// (`0x101239238`) — the "type this on everyone" broadcast.
///
/// **Departure**: `sync::start` is not reconstructed, so the normalised names are
/// handed back for the caller to send the line to.
pub fn give_typing(who: &[String]) -> Vec<String> {
    who.iter()
        .map(|who| who.trim().to_lowercase())
        .filter(|who| !who.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_table_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn the_restart_message_is_taken_once() {
        let table = Table::default();
        assert_eq!(table.restarting(), None);
        table.set_restarting("starting again");
        assert_eq!(table.restarting().as_deref(), Some("starting again"));
        assert_eq!(table.restarting(), None, "taking it clears it");
    }

    #[test]
    fn a_remembered_character_is_on_the_roster() {
        let dir = dir("remember");
        assert_eq!(
            remember(&dir, " Aelith ", " main ", " GS4 "),
            "Aelith is on the roster."
        );
        assert_eq!(
            roster::find(&dir, "gs4:aelith")
                .expect("find")
                .map(|e| e.character),
            Some("Aelith".to_string())
        );
        assert_eq!(forget(&dir, "gs4:aelith"), "Aelith is off the roster.");
        assert_eq!(
            forget(&dir, "gs4:aelith"),
            "gs4:aelith is not on the roster."
        );
    }

    #[test]
    fn a_nickname_is_set_and_cleared() {
        let dir = dir("nickname");
        remember(&dir, "Aelith", "main", "GS4");
        assert_eq!(nickname_set(&dir, "main", " Aely "), "Aelith goes by Aely.");
        assert_eq!(
            roster::find(&dir, "main")
                .expect("find")
                .and_then(|e| e.nickname)
                .as_deref(),
            Some("Aely")
        );
        assert_eq!(nickname_set(&dir, "main", "  "), "Aelith goes by its name.");
        assert_eq!(
            nickname_set(&dir, "nobody", "x"),
            "nobody is not on the roster."
        );
    }

    #[test]
    fn a_favourite_is_marked_and_unmarked() {
        let dir = dir("favourite");
        remember(&dir, "Aelith", "main", "GS4");
        assert_eq!(favourite(&dir, "main", true), "Aelith is a favourite.");
        assert!(roster::find(&dir, "main").expect("find").unwrap().favourite);
        assert_eq!(
            favourite(&dir, "main", false),
            "Aelith is no longer a favourite."
        );
    }

    #[test]
    fn a_resume_names_its_character() {
        let dir = dir("resume");
        std::fs::write(dir.join("resume"), " Aelith \n").expect("write");
        let resume = Resume::read(&dir, "resume");
        assert_eq!(resume.arg().as_deref(), Some("Aelith"));
        assert_eq!(
            Resume::read(&dir, "missing").arg(),
            None,
            "a missing resume file names nobody"
        );
    }

    #[test]
    fn the_seat_predicates_use_lower_cased_names() {
        let mut table = Table::default();
        table.seats.push(Seat {
            name: "Aelith".to_string(),
            id: 4312,
            kept: false,
        });
        assert!(table.seated(" aelith "));
        assert!(!table.running("aelith"), "no session is live in this build");
    }

    #[test]
    fn the_recovered_answers_are_exact() {
        assert_eq!(OFF_ROSTER, " is off the roster.");
        assert_eq!(ON_ROSTER, " is on the roster.");
        assert_eq!(IS_FAVOURITE, " is a favourite.");
        assert_eq!(NO_LONGER_FAVOURITE, " is no longer a favourite.");
        assert_eq!(NOT_ON_ROSTER, " is not on the roster.");
        assert_eq!(GOES_BY_ITS_NAME, " goes by its name.");
        assert_eq!(ROSTER_NOT_CHANGED, "The roster could not be changed: ");
    }
}
