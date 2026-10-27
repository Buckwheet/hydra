//! The board's registry of characters: who is attached, what is closing, and
//! what the web side is showing.
//!
//! **ESTABLISHED** shapes:
//!
//!  * `Sessions` is the registry (`hydra_board::sessions::Sessions`, dropped at
//!    `0x10004ea24`) and `Board` the thing it hands the window.
//!  * `Glance` (`0x10008c1e4`), `Open`, `Update` and `WebAt` are the answers it
//!    gives the chrome.
//!  * `new` (`0x1013ea110`) allocates the seats, `attach` (`0x1013ea94c`) and
//!    `detach` (`0x1013eab08`) move a character in and out, `close`
//!    (`0x1013ea648`) and `closing` (`0x1013eb780`) end one, `characters`
//!    (`0x1013e99b8`) lists them and `offer` (`0x1013ea674`) is what the game
//!    hands over.
//!
//! **std/tokio departure**: the original kept `tokio::sync::mpsc` channels for
//! the seats it opened and a `broadcast` receiver for events.  This build has no
//! async runtime, so the channels are plain vectors the caller drains.

use hydra_session::command::SessionHandle;

use crate::seat::{Seat, Wake};
use crate::trainer::SkillGoals;

/// A character the board has seen but not attached.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::sessions::Open`).
#[derive(Debug, Clone, Default)]
pub struct Open {
    /// The character's name.
    pub character: String,
    /// The session it would be attached with.
    pub session: Option<SessionHandle>,
    /// Where it came from, for the board to sort it by.
    pub instance: String,
}

/// What one character is doing, in a glance.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::sessions::Glance`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Glance {
    /// The character's name.
    pub character: String,
    /// The team it is on, when it is on one.
    pub team: Option<String>,
    /// The lines it has been told, most recent first.
    pub lines: Vec<String>,
    /// Whether its window is open.
    pub open: bool,
}

/// A change to one seat, as the chrome reports it.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::sessions::Update`).
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    /// A character was attached.
    Attached(String),
    /// A character was detached.
    Detached(String),
    /// A character's goals changed.
    Goals(String, SkillGoals),
    /// A character's window was raised.
    Raised(String),
    /// Something went wrong, and why.
    Failed(String),
}

/// Where the web side is listening.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::sessions::WebAt`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WebAt {
    /// The address it listens on.
    pub address: String,
    /// The port.
    pub port: u16,
    /// The team the page is showing, when one was asked for.
    pub team: Option<String>,
}

impl WebAt {
    /// The address the page is served at.
    pub fn url(&self) -> String {
        format!("http://{}:{}", self.address, self.port)
    }
}

/// The board's answer to "what is going on".
///
/// **ESTABLISHED** (the drop glue for `hydra_board::sessions::Board`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Board {
    /// What has changed since the chrome last asked.
    pub updates: Vec<Update>,
    /// The characters and what they are doing.
    pub glances: Vec<Glance>,
}

/// What a character is wearing, as the board reports it.
///
/// **ESTABLISHED** (`0x1013ea4d8`, `wore`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Worn {
    /// The character.
    pub character: String,
    /// What it put on.
    pub item: String,
}

/// The board's settings.
///
/// **ESTABLISHED** (`0x1013ebb5c`, `settings`): a small set of flags the chrome
/// edits.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Whether the web page is served at all.
    pub serve_web: bool,
    /// Whether the board follows the computer's light or dark theme.
    pub follow_computer: bool,
    /// The theme's name.
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            serve_web: false,
            follow_computer: false,
            theme: String::new(),
        }
    }
}

/// The registry of characters.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::sessions::Sessions`): the
/// seats, the ones that are opening, the ones that are closing and the web side.
#[derive(Debug, Default)]
pub struct Sessions {
    /// The attached characters.
    pub seats: Vec<Seat>,
    /// The characters that have been offered but not attached.
    pub offered: Vec<Open>,
    /// The characters that are closing.
    pub closes: Vec<String>,
    /// The window that was last opened.
    pub opener: Option<String>,
    /// The web side, when it is listening.
    pub web_at: Option<WebAt>,
    /// Whether the web page was asked for.
    pub want_web: bool,
    /// The team the web page is showing.
    pub web_team: Option<String>,
    /// The settings the chrome edits.
    pub settings: Settings,
    /// The palette's chosen entry.
    pub palette: Option<String>,
    /// The wake list the windows share.
    pub wake: Wake,
    /// What has changed since the chrome last asked.
    pub updates: Vec<Update>,
    /// The questions the board is waiting for an answer to.
    pub asked: Vec<String>,
    /// The question being answered.
    pub answerer: Option<String>,
    /// The raw game streams, by character.
    pub streams: Vec<(String, SessionHandle)>,
    /// The roster's order.
    pub roster: Vec<String>,
    /// The teams, by name.
    pub teams: Vec<(String, Vec<String>)>,
    /// The thing that is being worn.
    pub wore: Vec<Worn>,
    /// The update the folder is waiting for.
    pub update: Option<String>,
}

impl Sessions {
    /// A registry with nothing in it.
    ///
    /// **ESTABLISHED** (`0x1013ea110`): empty, with the wake list and the
    /// settings made.
    pub fn new() -> Sessions {
        Sessions::default()
    }

    /// The characters that are attached, in roster order.
    ///
    /// **ESTABLISHED** (`0x1013e99b8`): the seat names, ordered by the roster.
    pub fn characters(&self) -> Vec<String> {
        let mut names: Vec<String> = self.seats.iter().map(|seat| seat.name.clone()).collect();
        names.sort_by_key(|name| {
            self.roster
                .iter()
                .position(|roster| roster == name)
                .unwrap_or(usize::MAX)
        });
        names
    }

    /// The seat for a character.
    pub fn seat(&self, character: &str) -> Option<&Seat> {
        self.seats.iter().find(|seat| seat.name == character)
    }

    /// The seat for a character, mutably.
    pub fn seat_mut(&mut self, character: &str) -> Option<&mut Seat> {
        self.seats.iter_mut().find(|seat| seat.name == character)
    }

    /// The game offered a character.
    ///
    /// **ESTABLISHED** (`0x1013ea674`): the character is put at the front of the
    /// offered list so the chrome can attach it.
    pub fn offer(&mut self, open: Open) {
        self.offered.retain(|other| other.character != open.character);
        self.offered.insert(0, open);
    }

    /// Attach a character.
    ///
    /// **ESTABLISHED** (`0x1013ea94c`): a seat is made and put in the list; a
    /// character that is already attached is not attached twice.
    pub fn attach(&mut self, mut open: Open) -> Result<String, String> {
        if self.seats.iter().any(|seat| seat.name == open.character) {
            return Err(format!("{} is already here.", open.character));
        }
        self.offered.retain(|other| other.character != open.character);
        let session = open.session.take().ok_or_else(|| {
            format!("{} came without a session.", open.character)
        })?;
        let seat = Seat::start(&open.character.clone(), session);
        self.seats.push(seat);
        self.updates.push(Update::Attached(open.character.clone()));
        Ok(open.character)
    }

    /// Let a character go without ending it.
    ///
    /// **ESTABLISHED** (`0x1013eab08`): the seat is taken out of the list.
    pub fn detach(&mut self, character: &str) -> Option<Seat> {
        match self.seats.iter().position(|seat| seat.name == character) {
            Some(index) => {
                self.updates.push(Update::Detached(character.to_string()));
                Some(self.seats.remove(index))
            }
            None => None,
        }
    }

    /// End a character's session.
    ///
    /// **ESTABLISHED** (`0x1013ea648`): the session is told to stop and the
    /// character goes on the closing list.
    pub fn close(&mut self, character: &str, reason: &str) -> bool {
        let Some(seat) = self.seat(character) else {
            return false;
        };
        if let Some(session) = seat.session() {
            session.stop(reason);
        }
        if !self.closes.iter().any(|closing| closing == character) {
            self.closes.push(character.to_string());
        }
        true
    }

    /// The characters that are closing.
    ///
    /// **ESTABLISHED** (`0x1013eaa88`): the list itself.
    pub fn closes(&self) -> Vec<String> {
        self.closes.clone()
    }

    /// Is a character closing?
    ///
    /// **ESTABLISHED** (`0x1013eb780`).
    pub fn closing(&self, character: &str) -> bool {
        self.closes.iter().any(|closing| closing == character)
    }

    /// The sessions the folder owns, for the takeover handshake.
    ///
    /// **ESTABLISHED** (`0x1013e9b10`): the sessions behind each seat.
    pub fn own_streams(&self) -> Vec<String> {
        self.seats.iter().map(|seat| seat.name.clone()).collect()
    }

    /// Where the web page is listening.
    ///
    /// **ESTABLISHED** (`0x1013eb664`).
    pub fn web_at(&self) -> Option<&WebAt> {
        self.web_at.as_ref()
    }

    /// Ask for the web page.
    ///
    /// **ESTABLISHED** (`0x1013ebc70`): the flag is set and the address worked
    /// out from the folder's settings.
    pub fn want_web(&mut self, want: bool) -> Option<WebAt> {
        self.want_web = want;
        if !want {
            self.web_at = None;
            return None;
        }
        let web = self
            .web_at
            .clone()
            .unwrap_or_else(|| WebAt {
                address: "127.0.0.1".to_string(),
                port: crate::own::web_page_port(None),
                team: self.web_team.clone(),
            });
        self.web_at = Some(web.clone());
        Some(web)
    }

    /// Whether the web page was asked for.
    ///
    /// **ESTABLISHED** (`0x1013e9a90`).
    pub fn web_wanted(&self) -> bool {
        self.want_web
    }

    /// Serve the web page, which is the seam: no network here.
    ///
    /// **ESTABLISHED** (`0x1013ebe70`): the server is started with the address
    /// and the team.  **Departure**: the address is returned rather than bound.
    pub fn serve_web(&mut self, address: &str, port: u16) -> WebAt {
        let web = WebAt {
            address: address.to_string(),
            port,
            team: self.web_team.clone(),
        };
        self.web_at = Some(web.clone());
        self.want_web = true;
        web
    }

    /// The team the web page shows.
    ///
    /// **ESTABLISHED** (`0x1013ebda0`): setting it also sets the page's copy.
    pub fn web_team(&mut self, team: Option<String>) -> Option<String> {
        self.web_team = team.clone();
        if let Some(web) = &mut self.web_at {
            web.team = team.clone();
        }
        team
    }

    /// The teams the board knows.
    ///
    /// **ESTABLISHED** (`0x1013ea7a0`): the teams, each with its characters.
    pub fn teams(&self) -> Vec<(String, Vec<String>)> {
        let mut teams = self.teams.clone();
        teams.sort_by(|a, b| a.0.cmp(&b.0));
        teams
    }

    /// The roster, in the order the board shows it.
    ///
    /// **ESTABLISHED** (`0x1013eb3fc`): the attached characters, then the ones
    /// that are offered.
    pub fn roster(&self) -> Vec<String> {
        let mut names = self.characters();
        for open in &self.offered {
            if !names.contains(&open.character) {
                names.push(open.character.clone());
            }
        }
        names
    }

    /// What one character is doing.
    ///
    /// **ESTABLISHED** (`0x1013eacf0`): a small summary the chrome can draw
    /// without touching the seat.
    pub fn glance(&self, character: &str) -> Option<Glance> {
        let seat = self.seat(character)?;
        let mut lines = seat.snapshot();
        lines.reverse();
        Some(Glance {
            character: seat.name.clone(),
            team: seat.team().map(str::to_string),
            lines,
            open: !seat.open_windows().is_empty(),
        })
    }

    /// The windows that are open across the board.
    ///
    /// **ESTABLISHED** (`0x1013e9cc4`): the windows of every seat.
    pub fn open_windows(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for seat in &self.seats {
            for window in seat.open_windows() {
                out.push((seat.name.clone(), window));
            }
        }
        out
    }

    /// The window that was last opened.
    ///
    /// **ESTABLISHED** (`0x1013eb2d0`).
    pub fn opener(&self) -> Option<&str> {
        self.opener.as_deref()
    }

    /// Note the window that was opened.
    pub fn opened(&mut self, window: &str) {
        self.opener = Some(window.to_string());
        self.wake.opened(window);
    }

    /// The palette's chosen entry.
    ///
    /// **ESTABLISHED** (`0x1013eb964`).
    pub fn palette(&self) -> Option<&str> {
        self.palette.as_deref()
    }

    /// Choose a palette entry.
    pub fn choose(&mut self, entry: &str) {
        self.palette = Some(entry.to_string());
    }

    /// The question the board is waiting on.
    ///
    /// **ESTABLISHED** (`0x1013eba80`): the answerer, which is the first question
    /// asked.
    pub fn answerer(&self) -> Option<&str> {
        self.answerer.as_deref().or_else(|| self.asked.first().map(String::as_str))
    }

    /// Ask something.
    ///
    /// **ESTABLISHED** (`0x1013e9e58`): the question is queued for the chrome.
    pub fn ask(&mut self, question: &str) {
        self.asked.push(question.to_string());
        if self.answerer.is_none() {
            self.answerer = Some(question.to_string());
        }
    }

    /// Answer the question that is being asked.
    pub fn answer(&mut self, answer: &str) -> Option<String> {
        let question = self.answerer.take()?;
        self.asked.retain(|asked| *asked != question);
        let _ = answer;
        if self.answerer.is_none() {
            self.answerer = self.asked.first().cloned();
        }
        Some(question)
    }

    /// The board's settings.
    ///
    /// **ESTABLISHED** (`0x1013ebb5c`).
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Change the settings.
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    /// The control the chrome last asked for.
    ///
    /// **ESTABLISHED** (`0x1013eb840`): a string the window interprets.
    pub fn control(&self) -> Option<&str> {
        self.palette.as_deref()
    }

    /// The update the folder is waiting for.
    ///
    /// **ESTABLISHED** (`0x1013e9cb8`).
    pub fn fetch_update(&mut self) -> Option<String> {
        self.update.take()
    }

    /// Note that a character put something on.
    ///
    /// **ESTABLISHED** (`0x1013ea4d8`).
    pub fn wore(&mut self, character: &str, item: &str) {
        self.wore.push(Worn {
            character: character.to_string(),
            item: item.to_string(),
        });
    }

    /// The changes since the chrome last asked.
    ///
    /// **ESTABLISHED** (`0x1013eb52c`): the updates are drained and the roster
    /// handed back with them.
    pub fn update(&mut self) -> Board {
        Board {
            updates: std::mem::take(&mut self.updates),
            glances: self
                .characters()
                .iter()
                .filter_map(|name| self.glance(name))
                .collect(),
        }
    }

    /// Run a trainer command for a character.
    ///
    /// **ESTABLISHED** (`0x1013e...`): the seat's trainer is asked.
    pub fn train(&self, character: &str, change: crate::trainer::Change) -> Result<(), String> {
        match self.seat(character) {
            Some(seat) => seat.change_goals(change),
            None => Err(format!("{character} is not here.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_session::claimant::Desk;

    fn session() -> SessionHandle {
        SessionHandle::new(Desk::new(Some('.'), "Al"))
    }

    fn open(name: &str) -> Open {
        Open {
            character: name.to_string(),
            session: Some(session()),
            instance: "Hydra".to_string(),
        }
    }

    #[test]
    fn a_character_can_be_offered_and_attached() {
        let mut sessions = Sessions::new();
        sessions.offer(open("Al"));
        sessions.offer(open("Bo"));
        assert_eq!(sessions.roster(), vec!["Bo", "Al"]);
        assert_eq!(sessions.attach(open("Al")).unwrap(), "Al");
        assert_eq!(sessions.characters(), vec!["Al"]);
        assert!(sessions.attach(open("Al")).is_err());
        assert_eq!(sessions.roster(), vec!["Al", "Bo"]);
        assert!(matches!(sessions.update().updates[0], Update::Attached(_)));
    }

    #[test]
    fn attaching_needs_a_session() {
        let mut sessions = Sessions::new();
        let err = sessions
            .attach(Open {
                character: "Al".into(),
                session: None,
                instance: String::new(),
            })
            .unwrap_err();
        assert!(err.contains("without a session"), "{err}");
    }

    #[test]
    fn detaching_and_closing() {
        let mut sessions = Sessions::new();
        sessions.attach(open("Al")).unwrap();
        assert!(sessions.close("Al", "bye"));
        assert!(sessions.closing("Al"));
        assert_eq!(sessions.closes(), vec!["Al"]);
        assert!(sessions.detach("Al").is_some());
        assert!(sessions.detach("Al").is_none());
        assert!(!sessions.close("Nobody", "bye"));
    }

    #[test]
    fn the_web_page_is_a_seam() {
        let mut sessions = Sessions::new();
        assert!(!sessions.web_wanted());
        let web = sessions.want_web(true).unwrap();
        assert_eq!(web.address, "127.0.0.1");
        assert_eq!(web.port, crate::own::WEB_PORT);
        assert!(sessions.web_wanted());
        assert_eq!(web.url(), format!("http://127.0.0.1:{}", crate::own::WEB_PORT));
        assert!(sessions.want_web(false).is_none());
        let served = sessions.serve_web("0.0.0.0", 80);
        assert_eq!(served.url(), "http://0.0.0.0:80");
        assert_eq!(sessions.web_team(Some("the wolves".into())), Some("the wolves".into()));
        assert_eq!(sessions.web_at().unwrap().team.as_deref(), Some("the wolves"));
    }

    #[test]
    fn questions_are_answered_in_order() {
        let mut sessions = Sessions::new();
        assert!(sessions.answerer().is_none());
        sessions.ask("Which profile?");
        sessions.ask("How much?");
        assert_eq!(sessions.answerer(), Some("Which profile?"));
        assert_eq!(sessions.answer("combat"), Some("Which profile?".to_string()));
        assert_eq!(sessions.answerer(), Some("How much?"));
    }

    #[test]
    fn a_glance_shows_the_recent_lines_newest_first() {
        let mut sessions = Sessions::new();
        sessions.attach(open("Al")).unwrap();
        let seat = sessions.seat("Al").unwrap();
        seat.told("first");
        seat.told("second");
        seat.in_team("the wolves");
        let glance = sessions.glance("Al").unwrap();
        assert_eq!(glance.character, "Al");
        assert_eq!(glance.lines, vec!["second", "first"]);
        assert_eq!(glance.team.as_deref(), Some("the wolves"));
        assert!(!glance.open);
        assert!(sessions.glance("Bo").is_none());
    }
}
