//! The `.lich` command family: whether this character's own Lich runs, where
//! Lich is, and what the window shows while it runs.
//!
//! # What is recovered and what is composed
//!
//! The command's own words are **RECOVERED** — taken verbatim from the local
//! reference binary's string table, at the addresses the surrounding `lich`
//! family in `hydra_table::lich_command` points at (`0x1011f0cd8` …
//! `0x1011f1944`).  Everything below is used exactly as recovered:
//!
//! ```text
//! usage:      say lich on, lich off, lich folder <folder>, lich view lich|game, or lich help
//! help:       lich                 whether this character's own Lich runs, and where Lich is
//!             lich on              start it, and start it whenever this character starts
//!             lich off             stop it, and leave it off
//!             lich folder <folder>  where your Lich is: the folder with lich.rbw in it, for every character
//!             lich view lich       while it runs, show what Lich shows: its squelches hold, its scripts' words show
//!             lich view game       while it runs, show the game's own text, and Lich's own lines beside it
//! intro:      With Lich on, a line you type goes to Lich, and one starting with Lich's command symbol
//!             (; unless chosen) is Lich's own command; one starting with Hydra's (.) is Hydra's.
//! ```
//!
//! The status and notice sentences are recovered too:
//!
//! ```text
//! Lich already runs for this character.
//! Lich stays off for this character.
//! Lich is stopping, and stays off for this character.
//! where Lich is has not been said: lich folder <folder>
//! Where Lich is has not been said: lich folder <folder>.
//! Lich needs Ruby, which Lich's installer puts in C:\Ruby4Lich5; none was found there or on the PATH
//! Lich is not running for this character
//! Lich runs for this character, and is off, and starts with it
//! While Lich runs, this window shows what Lich shows.
//! While Lich runs, this window shows the game's own text, and Lich's own lines beside it.
//! {} has no lich.rbw in it
//! Lich is at {}, for every character.
//! Lich is starting, from {}.
//! Lich is at {}.
//! ```
//!
//! **INFERRED** is only *which* recovered sentence answers *which* situation:
//! the binary keeps one string table, not one call site per sentence, so the
//! composition (a status notice of two lines, the lower-case fragment inside a
//! line and the capitalised sentence standing alone) is this crate's reading.
//! [`status_lines`] documents the pair it builds.
//!
//! # The settings file — `lich.json`
//!
//! Global, not per character, with three keys (**RECOVERED**): `folder`, `on`
//! and `view` (`lich` or `game`).  [`LichSettings`] is that file;
//! [`LichSettings::load`]/[`LichSettings::save`] read and write it and a
//! missing file is the defaults (`on: false`, `view: game`, no folder).
//!
//! # The commands
//!
//! [`LichCommand::parse`] accepts `.lich …` and the bare word `lich …`; the
//! bodies live in [`execute`] (and [`SessionHandle::lich_command`], which is
//! what the binary calls).  `.lich` is never sent to the game: this module
//! only ever says notices on the session — nothing here touches the taker.
//!
//! The session cannot start or stop a process (that is `hydra_lich`, which
//! depends on nothing here), so a command that *asks* for a start or a stop
//! records the settings and reports the recovered notice; [`LichOutcome`]
//! carries the request (`start`, `stop`) and the fact that the settings must be
//! written back (`changed`).  `hydra`'s binary does both.
//!
//! **UNKNOWN**: whether the reference build starts Lich on `.lich on`
//! mid-session or only at the next character start (both readings fit "start
//! it, and start it whenever this character starts"); this crate reports
//! `Lich is starting, from {}` and leaves the spawn to the caller.

use crate::command::SessionHandle;
use crate::notice::{Kind, Notice, LEVEL_INFO, LEVEL_WARN};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The word the family answers to: `.lich` (Hydra's symbol) and bare `lich`.
pub const LICH_WORD: &str = "lich";

/// The settings file's name (**RECOVERED**), beside the other global files in
/// the data directory.
pub const SETTINGS_FILE: &str = "lich.json";

/// The script a Lich folder must hold (**RECOVERED**: "the folder with
/// `lich.rbw` in it").
pub const LICH_SCRIPT: &str = "lich.rbw";

/// The usage line (**RECOVERED**), the answer to an unknown sub-command.
pub const USAGE: &str = "say lich on, lich off, lich folder <folder>, lich view lich|game, or lich help";

/// The help block (**RECOVERED**), one line per sub-command.
pub const HELP: [&str; 6] = [
    "lich                 whether this character's own Lich runs, and where Lich is",
    "lich on              start it, and start it whenever this character starts",
    "lich off             stop it, and leave it off",
    "lich folder <folder>  where your Lich is: the folder with lich.rbw in it, for every character",
    "lich view lich       while it runs, show what Lich shows: its squelches hold, its scripts' words show",
    "lich view game       while it runs, show the game's own text, and Lich's own lines beside it",
];

/// The paragraph above the help block (**RECOVERED**).
pub const INTRO: &str = "With Lich on, a line you type goes to Lich, and one starting with Lich's command symbol (; unless chosen) is Lich's own command; one starting with Hydra's (.) is Hydra's.";

/// `.lich on` when it is already on (**RECOVERED**).
pub const ALREADY_ON: &str = "Lich already runs for this character.";
/// `.lich off` when Lich was not running (**RECOVERED**).
pub const STAYS_OFF: &str = "Lich stays off for this character.";
/// `.lich off` while Lich runs (**RECOVERED**).
pub const STOPPING_STAYS_OFF: &str = "Lich is stopping, and stays off for this character.";
/// The fragment inside a status line (**RECOVERED**, lower case, no stop).
pub const WHERE_NOT_SAID: &str = "where Lich is has not been said: lich folder <folder>";
/// The same as a sentence of its own (**RECOVERED**, capitalised, with a stop)
pub const WHERE_NOT_SAID_SENTENCE: &str = "Where Lich is has not been said: lich folder <folder>.";
/// No Ruby anywhere (**RECOVERED**; the sentence has no final stop).
pub const NEEDS_RUBY: &str =
    r"Lich needs Ruby, which Lich's installer puts in C:\Ruby4Lich5; none was found there or on the PATH";
/// The status line of a character whose Lich is off (**RECOVERED**).
pub const NOT_RUNNING: &str = "Lich is not running for this character";
/// The status line of a character whose Lich is on but not running here
/// (**RECOVERED**).
pub const RUNS_AND_STARTS: &str = "Lich runs for this character, and is off, and starts with it";
/// `.lich view lich` (**RECOVERED**).
pub const VIEW_LICH: &str = "While Lich runs, this window shows what Lich shows.";
/// `.lich view game` (**RECOVERED**).
pub const VIEW_GAME: &str =
    "While Lich runs, this window shows the game's own text, and Lich's own lines beside it.";

/// `{} has no lich.rbw in it` (**RECOVERED**).
pub fn has_no_lich_rbw(folder: &Path) -> String {
    format!("{} has no lich.rbw in it", folder.display())
}

/// `Lich is at {}, for every character.` (**RECOVERED**: the folder is global).
pub fn lich_at_for_every_character(folder: &Path) -> String {
    format!("Lich is at {}, for every character.", folder.display())
}

/// `Lich is at {}.` (**RECOVERED**: the status answer for a folder that is set).
pub fn lich_at(folder: &Path) -> String {
    format!("Lich is at {}.", folder.display())
}

/// `Lich is starting, from {}.` (**RECOVERED**).
///
/// The recovered literal spells the path `<folder>\lich.rbw` with a literal
/// backslash because the reference build is a Windows executable; the message
/// keeps that spelling, while the path actually spawned is
/// [`Path::join`]`("lich.rbw")` ([`script_path`]).
pub fn lich_starting(folder: &Path) -> String {
    format!(r"Lich is starting, from {}\{}.", folder.display(), LICH_SCRIPT)
}

/// The path of the script in `folder` (`<folder>/lich.rbw` on this platform).
pub fn script_path(folder: &Path) -> PathBuf {
    folder.join(LICH_SCRIPT)
}

/// Does `folder` hold a `lich.rbw`?
pub fn has_script(folder: &Path) -> bool {
    script_path(folder).is_file()
}

/// Which text the window shows while Lich runs (**RECOVERED** values: `lich`,
/// `game`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LichView {
    /// Show what Lich shows: its squelches hold, its scripts' words show.
    Lich,
    /// Show the game's own text, and Lich's own lines beside it.
    #[default]
    Game,
}

impl LichView {
    /// The word in `lich.json` and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            LichView::Lich => "lich",
            LichView::Game => "game",
        }
    }

    /// Parse the word; anything else is not a view.
    pub fn parse(word: &str) -> Option<LichView> {
        match word {
            "lich" => Some(LichView::Lich),
            "game" => Some(LichView::Game),
            _ => None,
        }
    }

    /// The mapping [`SessionHandle::set_lich_view`] uses: Lich view = `true`,
    /// Game view = `false`.
    pub fn from_on(on: bool) -> LichView {
        if on {
            LichView::Lich
        } else {
            LichView::Game
        }
    }

    /// Is this the Lich view?  (`set_lich_view(true)`)
    pub fn is_lich(self) -> bool {
        matches!(self, LichView::Lich)
    }
}

/// The contents of `lich.json`: the folder Lich lives in, whether it runs, and
/// what the window shows.  Global, not per character.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LichSettings {
    /// the folder with `lich.rbw` in it; `None` until `.lich folder` says
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// start Lich, and start it whenever this character starts
    #[serde(default)]
    pub on: bool,
    /// what the window shows while Lich runs
    #[serde(default)]
    pub view: LichView,
}

impl LichSettings {
    /// The defaults: off, Game view, no folder.
    pub fn new() -> Self {
        LichSettings::default()
    }

    /// `<dir>/lich.json`.
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(SETTINGS_FILE)
    }

    /// The script this settings value points at, if a folder is set.
    pub fn script(&self) -> Option<PathBuf> {
        self.folder.as_deref().map(script_path)
    }

    /// Is a folder set and does it hold `lich.rbw`?
    pub fn has_script(&self) -> bool {
        self.folder.as_deref().map(has_script).unwrap_or(false)
    }

    /// Read the document.
    pub fn from_json(text: &str) -> Result<LichSettings, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Write the document (pretty, one member per line, in the recovered key
    /// order `folder`, `on`, `view`).
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("lich settings are plain data")
    }

    /// Read `path`; a file that is not there is the defaults.
    pub fn load(path: &Path) -> io::Result<LichSettings> {
        match fs::read_to_string(path) {
            Ok(text) => LichSettings::from_json(&text).map_err(|err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} cannot be read: {err}", path.display()),
                )
            }),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(LichSettings::default()),
            Err(err) => Err(err),
        }
    }

    /// Write `path`, creating its directory.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let mut text = self.to_json();
        text.push('\n');
        fs::write(path, text)
    }
}

/// One `.lich` command line, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LichCommand {
    /// `.lich` — the status
    Status,
    /// `.lich on`
    On,
    /// `.lich off`
    Off,
    /// `.lich folder <folder>`; `None` is a bare `.lich folder` (where is it?)
    Folder(Option<String>),
    /// `.lich view lich|game`
    View(LichView),
    /// `.lich help`
    Help,
    /// Anything else after the word: the answer is the usage line.  The
    /// argument text is kept for diagnostics.
    Unknown(String),
}

impl LichCommand {
    /// Parse one line.  `None` means "not a `.lich` command at all" — the line
    /// belongs to the game (or to Lich).
    ///
    /// Both spellings are accepted: `.lich` (Hydra's command symbol) and the
    /// bare word `lich`.  The word has to be followed by the end of the line
    /// or by whitespace, so `.lichx` and `.lichy` are not commands.
    pub fn parse(line: &str) -> Option<LichCommand> {
        let line = line.trim();
        let rest = line.strip_prefix('.').unwrap_or(line);
        let rest = rest.strip_prefix(LICH_WORD)?;
        if !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
            return None;
        }
        let argument = rest.trim();
        let (word, after) = match argument.split_once(char::is_whitespace) {
            Some((word, after)) => (word, after.trim()),
            None => (argument, ""),
        };
        Some(match (word, after) {
            ("", _) => LichCommand::Status,
            ("on", "") => LichCommand::On,
            ("off", "") => LichCommand::Off,
            ("help", "") => LichCommand::Help,
            ("folder", "") => LichCommand::Folder(None),
            // A folder may contain spaces: the rest of the line is the path.
            ("folder", dir) => LichCommand::Folder(Some(dir.to_string())),
            ("view", word) => match LichView::parse(word) {
                Some(view) => LichCommand::View(view),
                None => LichCommand::Unknown(format!("view {word}")),
            },
            _ => LichCommand::Unknown(argument.to_string()),
        })
    }

    /// Is this line one of ours?  (True for an unknown sub-command too — it
    /// must still never reach the game.)
    pub fn is_lich_line(line: &str) -> bool {
        LichCommand::parse(line).is_some()
    }
}

/// What one `.lich` command did.
#[derive(Debug, Clone, PartialEq)]
pub struct LichOutcome {
    /// the notices to show, in order
    pub notices: Vec<Notice>,
    /// the settings after the command
    pub settings: LichSettings,
    /// true when the settings changed and the caller should write `lich.json`
    pub changed: bool,
    /// the folder a caller should start Lich from (`.lich on`)
    pub start: Option<PathBuf>,
    /// true when a caller should stop Lich (`.lich off` while it runs)
    pub stop: bool,
}

/// The help notice: the recovered paragraph, then the recovered table.
pub fn help_notice() -> Notice {
    let mut lines = Vec::with_capacity(HELP.len() + 1);
    lines.push(INTRO.to_string());
    lines.extend(HELP.iter().map(|line| (*line).to_string()));
    Notice {
        kind: Kind::Help,
        level: LEVEL_INFO,
        lines,
        quiet: true,
    }
}

/// The status notice: whether Lich runs, and where it is.
///
/// **INFERRED** composition of two recovered sentences: the first says whether
/// it is on ([`RUNS_AND_STARTS`] / [`NOT_RUNNING`]), the second where it is
/// ([`lich_at`] / the lower-case [`WHERE_NOT_SAID`] fragment).
pub fn status_lines(settings: &LichSettings) -> Vec<String> {
    let mut lines = vec![if settings.on {
        RUNS_AND_STARTS.to_string()
    } else {
        NOT_RUNNING.to_string()
    }];
    match &settings.folder {
        Some(folder) => lines.push(lich_at(folder)),
        None => lines.push(WHERE_NOT_SAID.to_string()),
    }
    lines
}

/// Run one `.lich` command on `session`: apply the settings and say the
/// notices.  `None` when the line is not a `.lich` command.
pub fn run(session: &SessionHandle, line: &str) -> Option<LichOutcome> {
    let command = LichCommand::parse(line)?;
    Some(execute(session, command))
}

/// Apply an already-parsed command.
pub fn execute(session: &SessionHandle, command: LichCommand) -> LichOutcome {
    let mut outcome = LichOutcome {
        notices: Vec::new(),
        settings: session.lich_settings(),
        changed: false,
        start: None,
        stop: false,
    };
    match command {
        LichCommand::Status => {
            outcome
                .notices
                .push(Notice::lines(LEVEL_INFO, status_lines(&outcome.settings)));
        }
        LichCommand::Help => outcome.notices.push(help_notice()),
        LichCommand::On => turn_on(session, &mut outcome),
        LichCommand::Off => turn_off(session, &mut outcome),
        LichCommand::Folder(folder) => set_folder(&mut outcome, folder),
        LichCommand::View(view) => set_view(&mut outcome, view),
        LichCommand::Unknown(_) => {
            outcome.notices.push(Notice::line(LEVEL_WARN, USAGE));
        }
    }
    if outcome.changed {
        session.set_lich_settings(outcome.settings.clone());
    }
    for notice in &outcome.notices {
        session.say(notice);
    }
    outcome
}

/// `.lich on`: start it, and start it whenever this character starts.
fn turn_on(session: &SessionHandle, outcome: &mut LichOutcome) {
    if outcome.settings.on {
        outcome.notices.push(Notice::line(LEVEL_INFO, ALREADY_ON));
        return;
    }
    let Some(folder) = outcome.settings.folder.clone() else {
        outcome
            .notices
            .push(Notice::line(LEVEL_WARN, WHERE_NOT_SAID_SENTENCE));
        return;
    };
    if !has_script(&folder) {
        outcome
            .notices
            .push(Notice::line(LEVEL_WARN, has_no_lich_rbw(&folder)));
        return;
    }
    outcome.settings.on = true;
    outcome.changed = true;
    // The session cannot spawn; the caller does, and the notice is the
    // recovered "starting" line either way.
    outcome.start = if session.lich_running() {
        None
    } else {
        Some(folder.clone())
    };
    outcome
        .notices
        .push(Notice::line(LEVEL_INFO, lich_starting(&folder)));
}

/// `.lich off`: stop it, and leave it off.
fn turn_off(session: &SessionHandle, outcome: &mut LichOutcome) {
    let running = session.lich_running();
    outcome.changed = outcome.settings.on;
    outcome.settings.on = false;
    outcome.stop = running;
    let line = if running { STOPPING_STAYS_OFF } else { STAYS_OFF };
    outcome.notices.push(Notice::line(LEVEL_INFO, line));
}

/// `.lich folder [<folder>]`.
fn set_folder(outcome: &mut LichOutcome, folder: Option<String>) {
    let Some(folder) = folder else {
        match &outcome.settings.folder {
            Some(folder) => outcome
                .notices
                .push(Notice::line(LEVEL_INFO, lich_at_for_every_character(folder))),
            None => outcome
                .notices
                .push(Notice::line(LEVEL_WARN, WHERE_NOT_SAID)),
        }
        return;
    };
    let folder = PathBuf::from(folder);
    if !has_script(&folder) {
        outcome
            .notices
            .push(Notice::line(LEVEL_WARN, has_no_lich_rbw(&folder)));
        return;
    }
    outcome.changed = outcome.settings.folder.as_deref() != Some(folder.as_path());
    outcome
        .notices
        .push(Notice::line(LEVEL_INFO, lich_at_for_every_character(&folder)));
    outcome.settings.folder = Some(folder);
}

/// `.lich view lich|game`.
fn set_view(outcome: &mut LichOutcome, view: LichView) {
    outcome.changed = outcome.settings.view != view;
    outcome.settings.view = view;
    let line = if view.is_lich() { VIEW_LICH } else { VIEW_GAME };
    outcome.notices.push(Notice::line(LEVEL_INFO, line));
}

impl SessionHandle {
    /// Run one line if it is a `.lich` command; `None` when it is not.
    ///
    /// The settings are applied to the session and the notices are said; a
    /// caller that owns `lich.json` writes it back when
    /// [`LichOutcome::changed`] is true, and starts/stops Lich when
    /// [`LichOutcome::start`]/[`LichOutcome::stop`] ask for it.  Nothing here
    /// reaches the taker, so a `.lich` line is never sent to the game.
    pub fn lich_command(&self, line: &str) -> Option<LichOutcome> {
        run(self, line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claimant::Desk;
    use crate::command::{taker_pair, SessionHandle};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn session() -> SessionHandle {
        SessionHandle::new(Desk::new(None, "Aelith"))
    }

    /// A temp folder; the `Drop` at the end removes it.
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
                "hydra-session-lich-{tag}-{}-{stamp}",
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

    fn lines(outcome: &LichOutcome) -> Vec<String> {
        outcome
            .notices
            .iter()
            .flat_map(|notice| notice.lines.iter().cloned())
            .collect()
    }

    // ---------------------------------------------------------------
    // The settings file
    // ---------------------------------------------------------------

    #[test]
    fn the_defaults_are_off_and_the_game_view() {
        let settings = LichSettings::new();
        assert_eq!(settings.folder, None);
        assert!(!settings.on);
        assert_eq!(settings.view, LichView::Game);
        assert!(!settings.view.is_lich());
        assert_eq!(settings.script(), None);
        assert!(!settings.has_script());
        assert_eq!(LichSettings::default(), settings);
        assert_eq!(SETTINGS_FILE, "lich.json");
        assert_eq!(LICH_SCRIPT, "lich.rbw");
    }

    #[test]
    fn the_json_round_trip_keeps_every_member() {
        let settings = LichSettings {
            folder: Some(PathBuf::from("/synthetic/lich folder")),
            on: true,
            view: LichView::Lich,
        };
        let text = settings.to_json();
        assert_eq!(
            text,
            concat!(
                "{\n",
                "  \"folder\": \"/synthetic/lich folder\",\n",
                "  \"on\": true,\n",
                "  \"view\": \"lich\"\n",
                "}"
            )
        );
        assert_eq!(LichSettings::from_json(&text).unwrap(), settings);
        // ... and the plain form too.
        let json = r#"{"folder":"C:\\Lich","on":false,"view":"game"}"#;
        let parsed = LichSettings::from_json(json).unwrap();
        assert_eq!(parsed.folder, Some(PathBuf::from(r"C:\Lich")));
        assert!(!parsed.on);
        assert_eq!(parsed.view, LichView::Game);
        assert_eq!(LichSettings::from_json(&parsed.to_json()).unwrap(), parsed);
    }

    #[test]
    fn an_empty_document_is_the_defaults_and_a_bad_one_is_an_error() {
        assert_eq!(LichSettings::from_json("{}").unwrap(), LichSettings::new());
        // A missing member falls back, an unknown one is ignored.
        let parsed = LichSettings::from_json(r#"{"on":true,"extra":1}"#).unwrap();
        assert!(parsed.on);
        assert_eq!(parsed.view, LichView::Game);
        // Unknown view values and broken JSON are refused.
        assert!(LichSettings::from_json(r#"{"view":"both"}"#).is_err());
        assert!(LichSettings::from_json("{").is_err());
    }

    #[test]
    fn a_missing_file_loads_the_defaults_and_saves_back() {
        let dir = TempDir::new("round-trip");
        let path = LichSettings::path(&dir.path);
        assert_eq!(path, dir.path.join("lich.json"));
        assert_eq!(LichSettings::load(&path).unwrap(), LichSettings::new());

        let settings = LichSettings {
            folder: Some(dir.path.clone()),
            on: true,
            view: LichView::Lich,
        };
        settings.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with("}\n"));
        assert_eq!(LichSettings::load(&path).unwrap(), settings);

        // A broken file is an error naming the path.
        fs::write(&path, "{").unwrap();
        let err = LichSettings::load(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().starts_with(&path.display().to_string()));
        assert!(err.to_string().contains("cannot be read"));
    }

    #[test]
    fn saving_creates_the_folder() {
        let dir = TempDir::new("nested");
        let path = LichSettings::path(&dir.path.join("data"));
        LichSettings::new().save(&path).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn the_view_mapping_is_lich_true_game_false() {
        assert!(LichView::from_on(true).is_lich());
        assert_eq!(LichView::from_on(true), LichView::Lich);
        assert_eq!(LichView::from_on(false), LichView::Game);
        assert_eq!(LichView::Lich.as_str(), "lich");
        assert_eq!(LichView::Game.as_str(), "game");
        assert_eq!(LichView::parse("lich"), Some(LichView::Lich));
        assert_eq!(LichView::parse("game"), Some(LichView::Game));
        assert_eq!(LichView::parse("both"), None);
    }

    #[test]
    fn the_session_view_flag_and_the_settings_view_agree() {
        let s = session();
        assert_eq!(s.lich_settings().view, LichView::Game);
        assert!(!s.lich_view());

        s.set_lich_view(true);
        assert_eq!(s.lich_settings().view, LichView::Lich);
        assert!(s.lich_view());

        // ... in both directions.
        let mut settings = s.lich_settings();
        settings.view = LichView::Game;
        s.set_lich_settings(settings);
        assert!(!s.lich_view());
        assert_eq!(s.lich_settings().view, LichView::Game);
    }

    // ---------------------------------------------------------------
    // Parsing
    // ---------------------------------------------------------------

    #[test]
    fn the_dot_and_the_bare_word_both_parse() {
        for line in [".lich", "lich", "  .lich  ", "lich\n"] {
            assert_eq!(LichCommand::parse(line), Some(LichCommand::Status), "{line:?}");
        }
        assert_eq!(LichCommand::parse(".lich on"), Some(LichCommand::On));
        assert_eq!(LichCommand::parse("lich ON"), Some(LichCommand::Unknown("ON".into())));
        assert!(LichCommand::is_lich_line(".lich bogus"));
        assert!(!LichCommand::is_lich_line(".lichs"));
        assert!(!LichCommand::is_lich_line("lichs"));
        assert!(!LichCommand::is_lich_line("..lich"));
        assert!(!LichCommand::is_lich_line("say lich on"));
        assert!(!LichCommand::is_lich_line("look"));
    }

    #[test]
    fn every_subcommand_parses() {
        assert_eq!(LichCommand::parse(".lich off"), Some(LichCommand::Off));
        assert_eq!(LichCommand::parse(".lich help"), Some(LichCommand::Help));
        assert_eq!(
            LichCommand::parse(".lich folder"),
            Some(LichCommand::Folder(None))
        );
        assert_eq!(
            LichCommand::parse(".lich folder /synthetic/lich"),
            Some(LichCommand::Folder(Some("/synthetic/lich".into())))
        );
        // A folder may contain spaces: the rest of the line is the path.
        assert_eq!(
            LichCommand::parse(".lich folder /synthetic/my lich"),
            Some(LichCommand::Folder(Some("/synthetic/my lich".into())))
        );
        assert_eq!(
            LichCommand::parse(".lich view lich"),
            Some(LichCommand::View(LichView::Lich))
        );
        assert_eq!(
            LichCommand::parse(".lich view game"),
            Some(LichCommand::View(LichView::Game))
        );
        // Unknown sub-commands, half-arguments and extra words.
        assert_eq!(
            LichCommand::parse(".lich view"),
            Some(LichCommand::Unknown("view ".into()))
        );
        assert_eq!(
            LichCommand::parse(".lich view both"),
            Some(LichCommand::Unknown("view both".into()))
        );
        assert_eq!(
            LichCommand::parse(".lich on now"),
            Some(LichCommand::Unknown("on now".into()))
        );
        assert_eq!(
            LichCommand::parse(".lich start"),
            Some(LichCommand::Unknown("start".into()))
        );
    }

    // ---------------------------------------------------------------
    // The commands and their exact text
    // ---------------------------------------------------------------

    #[test]
    fn the_usage_and_the_help_block_are_the_recovered_literals() {
        assert_eq!(
            USAGE,
            "say lich on, lich off, lich folder <folder>, lich view lich|game, or lich help"
        );
        assert_eq!(HELP.len(), 6);
        assert_eq!(
            HELP[0],
            "lich                 whether this character's own Lich runs, and where Lich is"
        );
        assert_eq!(
            HELP[1],
            "lich on              start it, and start it whenever this character starts"
        );
        assert_eq!(HELP[2], "lich off             stop it, and leave it off");
        assert_eq!(
            HELP[3],
            "lich folder <folder>  where your Lich is: the folder with lich.rbw in it, for every character"
        );
        assert_eq!(
            HELP[4],
            "lich view lich       while it runs, show what Lich shows: its squelches hold, its scripts' words show"
        );
        assert_eq!(
            HELP[5],
            "lich view game       while it runs, show the game's own text, and Lich's own lines beside it"
        );
        assert!(INTRO.starts_with("With Lich on, a line you type goes to Lich"));
        assert!(INTRO.ends_with("one starting with Hydra's (.) is Hydra's."));

        let help = help_notice();
        assert_eq!(help.kind, Kind::Help);
        assert!(help.is_quiet());
        assert_eq!(help.lines.len(), HELP.len() + 1);
        assert_eq!(help.lines[0], INTRO);
        assert_eq!(help.lines[1], HELP[0]);
    }

    #[test]
    fn an_unknown_subcommand_is_the_usage_line() {
        let s = session();
        let outcome = s.lich_command(".lich start").expect("a command");
        assert_eq!(lines(&outcome), vec![USAGE.to_string()]);
        assert_eq!(outcome.notices[0].level, LEVEL_WARN);
        assert!(!outcome.changed);
        assert_eq!(outcome.start, None);
        assert!(!outcome.stop);
    }

    #[test]
    fn the_bare_command_is_the_status() {
        let s = session();
        let outcome = s.lich_command(".lich").expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![
                NOT_RUNNING.to_string(),
                WHERE_NOT_SAID.to_string()
            ]
        );
        assert_eq!(
            s.said(),
            vec![
                "Lich is not running for this character".to_string(),
                "where Lich is has not been said: lich folder <folder>".to_string()
            ]
        );
        assert!(!outcome.changed);
        assert_eq!(s.lich_settings(), LichSettings::new());
    }

    #[test]
    fn the_status_reports_the_folder_once_it_is_known() {
        let dir = TempDir::with_script("status");
        let s = session();
        s.lich_command(&format!(".lich folder {}", dir.path.display()))
            .expect("a command");
        // `.lich on` with a folder that has lich.rbw: on, and starting.
        s.lich_command(".lich on").expect("a command");
        let outcome = s.lich_command(".lich").expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![
                RUNS_AND_STARTS.to_string(),
                format!("Lich is at {}.", dir.path.display())
            ]
        );
        assert!(s.lich_settings().on);
        assert_eq!(s.lich_settings().folder.as_deref(), Some(dir.path.as_path()));
    }

    #[test]
    fn on_without_a_folder_says_where_lich_is_was_not_said() {
        let s = session();
        let outcome = s.lich_command(".lich on").expect("a command");
        assert_eq!(lines(&outcome), vec![WHERE_NOT_SAID_SENTENCE.to_string()]);
        assert_eq!(
            WHERE_NOT_SAID_SENTENCE,
            "Where Lich is has not been said: lich folder <folder>."
        );
        assert!(!s.lich_settings().on);
        assert!(!outcome.changed);
        assert_eq!(outcome.start, None);
    }

    #[test]
    fn on_turns_it_on_and_asks_for_a_start() {
        let dir = TempDir::with_script("on");
        let s = session();
        let outcome = s
            .lich_command(&format!(".lich folder {}", dir.path.display()))
            .expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![format!("Lich is at {}, for every character.", dir.path.display())]
        );

        let outcome = s.lich_command(".lich on").expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![format!(
                "Lich is starting, from {}\\lich.rbw.",
                dir.path.display()
            )]
        );
        assert!(outcome.changed);
        assert_eq!(outcome.start.as_deref(), Some(dir.path.as_path()));
        assert!(s.lich_settings().on);

        // A second `.lich on` is the recovered already-running notice.
        let again = s.lich_command(".lich on").expect("a command");
        assert_eq!(lines(&again), vec![ALREADY_ON.to_string()]);
        assert!(!again.changed);
        assert_eq!(again.start, None);
    }

    #[test]
    fn off_leaves_it_off_and_reports_it() {
        let dir = TempDir::with_script("off");
        let s = session();
        s.lich_command(&format!(".lich folder {}", dir.path.display()))
            .expect("a command");

        // Never on: nothing changes.
        let outcome = s.lich_command(".lich off").expect("a command");
        assert_eq!(lines(&outcome), vec![STAYS_OFF.to_string()]);
        assert!(!outcome.changed);
        assert!(!outcome.stop);

        // On: it goes off, and that is a change worth saving.
        s.lich_command(".lich on").expect("a command");
        let outcome = s.lich_command(".lich off").expect("a command");
        assert_eq!(lines(&outcome), vec![STAYS_OFF.to_string()]);
        assert!(outcome.changed);
        assert!(!s.lich_settings().on);
    }

    #[test]
    fn off_while_lich_runs_stops_it_and_keeps_it_off() {
        let dir = TempDir::with_script("off-running");
        let s = session();
        s.lich_command(&format!(".lich folder {}", dir.path.display()))
            .expect("a command");
        s.lich_command(".lich on").expect("a command");
        // A claim is what "Lich runs" means to the session.
        let _claim = s.lich_door().claim();
        assert!(s.lich_running());

        let outcome = s.lich_command(".lich off").expect("a command");
        assert_eq!(lines(&outcome), vec![STOPPING_STAYS_OFF.to_string()]);
        assert!(outcome.stop);
        assert!(outcome.changed);
        assert!(!s.lich_settings().on);
        assert_eq!(
            STOPPING_STAYS_OFF,
            "Lich is stopping, and stays off for this character."
        );
    }

    #[test]
    fn the_folder_is_checked_for_lich_rbw() {
        let empty = TempDir::new("no-script");
        let s = session();
        let outcome = s
            .lich_command(&format!(".lich folder {}", empty.path.display()))
            .expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![format!("{} has no lich.rbw in it", empty.path.display())]
        );
        assert!(!outcome.changed);
        assert_eq!(s.lich_settings().folder, None);

        // A folder that does not exist at all is the same answer.
        let missing = empty.path.join("gone");
        let outcome = s
            .lich_command(&format!(".lich folder {}", missing.display()))
            .expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![format!("{} has no lich.rbw in it", missing.display())]
        );

        // The good folder is remembered, and a bare `.lich folder` reports it.
        let good = TempDir::with_script("good-folder");
        let outcome = s
            .lich_command(&format!(".lich folder {}", good.path.display()))
            .expect("a command");
        assert!(outcome.changed);
        assert_eq!(
            s.lich_settings().folder.as_deref(),
            Some(good.path.as_path())
        );
        let outcome = s.lich_command(".lich folder").expect("a command");
        assert_eq!(
            lines(&outcome),
            vec![format!(
                "Lich is at {}, for every character.",
                good.path.display()
            )]
        );
        // Repeating the same folder changes nothing.
        let outcome = s
            .lich_command(&format!(".lich folder {}", good.path.display()))
            .expect("a command");
        assert!(!outcome.changed);
    }

    #[test]
    fn a_bare_folder_with_nothing_known_is_the_lower_case_fragment() {
        let s = session();
        let outcome = s.lich_command(".lich folder").expect("a command");
        assert_eq!(lines(&outcome), vec![WHERE_NOT_SAID.to_string()]);
    }

    #[test]
    fn the_view_commands_set_the_flag_and_say_the_recovered_lines() {
        let s = session();
        let outcome = s.lich_command(".lich view lich").expect("a command");
        assert_eq!(lines(&outcome), vec![VIEW_LICH.to_string()]);
        assert!(outcome.changed);
        assert!(s.lich_view());
        assert_eq!(s.lich_settings().view, LichView::Lich);

        // Repeating it changes nothing.
        let again = s.lich_command(".lich view lich").expect("a command");
        assert!(!again.changed);
        assert_eq!(lines(&again), vec![VIEW_LICH.to_string()]);

        let outcome = s.lich_command(".lich view game").expect("a command");
        assert_eq!(lines(&outcome), vec![VIEW_GAME.to_string()]);
        assert!(outcome.changed);
        assert!(!s.lich_view());
        assert_eq!(s.lich_settings().view, LichView::Game);
        assert_eq!(
            VIEW_GAME,
            "While Lich runs, this window shows the game's own text, and Lich's own lines beside it."
        );
        assert_eq!(
            VIEW_LICH,
            "While Lich runs, this window shows what Lich shows."
        );
    }

    #[test]
    fn help_is_the_intro_and_the_six_lines() {
        let s = session();
        let outcome = s.lich_command(".lich help").expect("a command");
        assert_eq!(outcome.notices.len(), 1);
        assert_eq!(outcome.notices[0].kind, Kind::Help);
        assert_eq!(outcome.notices[0].lines.len(), 7);
        assert!(!outcome.changed);
        // The help block is quiet, so it is not written to the player log.
        assert!(s.said().is_empty());
    }

    #[test]
    fn a_lich_line_never_reaches_the_taker() {
        let s = session();
        let (taker, rx) = taker_pair();
        s.set_taker(taker);
        for line in [
            ".lich",
            ".lich on",
            ".lich off",
            ".lich folder /synthetic/none",
            ".lich view lich",
            ".lich help",
            ".lich bogus",
        ] {
            assert!(s.lich_command(line).is_some(), "{line}");
            assert!(rx.try_recv().is_err(), "{line} went to the game");
        }
        // ... while a line that is not ours is not claimed at all.
        assert_eq!(s.lich_command("look"), None);
        assert_eq!(s.lich_command(".lichx"), None);
    }

    #[test]
    fn every_notice_text_is_the_recovered_literal() {
        assert_eq!(ALREADY_ON, "Lich already runs for this character.");
        assert_eq!(STAYS_OFF, "Lich stays off for this character.");
        assert_eq!(STOPPING_STAYS_OFF, "Lich is stopping, and stays off for this character.");
        assert_eq!(
            WHERE_NOT_SAID,
            "where Lich is has not been said: lich folder <folder>"
        );
        assert_eq!(
            NEEDS_RUBY,
            r"Lich needs Ruby, which Lich's installer puts in C:\Ruby4Lich5; none was found there or on the PATH"
        );
        assert_eq!(NOT_RUNNING, "Lich is not running for this character");
        assert_eq!(
            RUNS_AND_STARTS,
            "Lich runs for this character, and is off, and starts with it"
        );
        assert_eq!(
            has_no_lich_rbw(Path::new("/synthetic/lich")),
            "/synthetic/lich has no lich.rbw in it"
        );
        assert_eq!(
            lich_at_for_every_character(Path::new("/synthetic/lich")),
            "Lich is at /synthetic/lich, for every character."
        );
        assert_eq!(
            lich_starting(Path::new("/synthetic/lich")),
            r"Lich is starting, from /synthetic/lich\lich.rbw."
        );
        assert_eq!(lich_at(Path::new("/synthetic/lich")), "Lich is at /synthetic/lich.");
        assert_eq!(
            script_path(Path::new("/synthetic/lich")),
            PathBuf::from("/synthetic/lich").join("lich.rbw")
        );
    }
}
