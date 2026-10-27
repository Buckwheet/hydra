//! A seat: one character's place in the board — its session, its story, its
//! goals, its hotbars and the windows kept for it.
//!
//! **ESTABLISHED**:
//!
//!  * The session handle is the `Option<SessionHandle>` at `+0x7c0`
//!    ([`Seat::command_words`], [`Seat::symbol`], [`Seat::lich_symbol`]).
//!  * `hunts_closed` is a plain word at `+0xa70` (`0x1013e663c`).
//!  * `keep_window`/`window_kept` are the `bool` at `+0xaa4`
//!    (`0x1013e651c`, `0x1013e653c`).
//!  * The log is a `OnceLock<PlayerLog>` at `+0x770` (`0x1013e69b0` returns
//!    `Option<&PlayerLog>`, `0x1013e7a98` initialises it).
//!  * The team is a `OnceLock<String>` at `+0x7a0` (`0x1013e6bd8` returns
//!    `Option<&str>`, `0x1013e776c` sets it).
//!  * The rumbles are a `Mutex<Vec<…>>` reached through a `LazyLock` at `+0xa48`
//!    (`0x1013e77f8` drains it).
//!  * `press_hotbar` says `Not pressed: a hotbar is pressed by a window, and none
//!    is open.` (`0x102418xxx`).
//!
//! **std/UI departure**: the original held a `hydra_ui::story::Story` behind a
//! mutex and spawned tokio tasks for the delays.  That crate is a sibling this
//! one may not depend on, so [`Story`] here is the smallest model of what the
//! board needs, and every delay is synchronous.

use std::sync::{Mutex, MutexGuard, OnceLock};

use hydra_session::command::SessionHandle;
use hydra_session::notice::Notice;
use hydra_session::player_log::channel::PlayerLog;
use hydra_session::steps::Step;

use crate::pad::rumble::Pattern;
use crate::push::Alert;
use crate::trainer::{Change, SkillGoals, Status, Trainer};

/// The line the board says when a hotbar press has nowhere to go.
///
/// **ESTABLISHED** (`0x102418xxx`): `Not pressed: a hotbar is pressed by a
/// window, and none is open.`
pub const NO_WINDOW: &str =
    "Not pressed: a hotbar is pressed by a window, and none is open.";

/// Whether a typed line is a hotbar press.
///
/// **PARTLY** (`0x1013e5f0`, which trims a leader and compares): UNKNOWN which
/// spellings the binary accepted; `hotbar` is the word the board's own command
/// uses ([`crate::hotbars::HOTBAR`]).
pub fn is_hotbar_press(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with(crate::hotbars::HOTBAR)
        && line[crate::hotbars::HOTBAR.len()..]
            .starts_with([' ', '\t'])
}

/// The story the seat shows: the smallest model of `hydra_ui::story::Story` the
/// board needs.
///
/// **std/UI departure**: the original's type is in a sibling crate that this one
/// may not depend on.  The lines, alerts and typing are all kept here so the
/// board's own tests can read them back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Story {
    /// The lines, in order.
    pub lines: Vec<String>,
    /// The alerts, with how long each was asked to stay.
    pub alerts: Vec<(String, u32)>,
    /// What the player is typing.
    pub typing: String,
}

impl Story {
    /// Add a line.
    pub fn push(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    /// Show an alert for `ms`.
    pub fn alert(&mut self, text: &str, ms: u32) {
        self.alerts.push((text.to_string(), ms));
    }

    /// Set what the player is typing.
    pub fn typing(&mut self, text: &str) {
        self.typing = text.to_string();
    }

    /// The alert last shown.
    pub fn last_alert(&self) -> Option<&(String, u32)> {
        self.alerts.last()
    }
}

/// What the board keeps about a seat's windows.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::seat::Windows`): the set of
/// windows raised for this seat.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Windows {
    /// The windows that are open, by name.
    pub open: Vec<String>,
}

/// How far a seat has got in its life.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Lifecycle {
    /// Not connected.
    #[default]
    Idle,
    /// Connected, and the game has said hello.
    Started,
    /// The character changed.
    Changed(String),
    /// It is over, and why.
    Over(String),
}

/// One character's place in the board.
#[derive(Debug, Default)]
pub struct Seat {
    /// The character's name.
    pub name: String,
    /// The session this seat drives.
    pub session: Option<SessionHandle>,
    /// The story the seat shows.
    pub story: Mutex<Story>,
    /// The windows kept for this seat.
    pub windows: Mutex<Windows>,
    /// The trainer's goals.
    pub goals: Mutex<Option<SkillGoals>>,
    /// The trainer.
    pub trainer: Mutex<Trainer>,
    /// The hotbar presses waiting to be done, `(bar, button)`.
    pub hotbar_presses: Mutex<Vec<(String, String)>>,
    /// The rumbles waiting to be played.
    pub rumbles: Mutex<Vec<Pattern>>,
    /// The player log, once it is known.
    pub log: OnceLock<PlayerLog>,
    /// The team the character is on, once it is known.
    pub team: OnceLock<String>,
    /// The seat's life so far.
    pub lifecycle: Mutex<Lifecycle>,
    /// The page this seat's character can be read at.
    pub page_link: Mutex<String>,
    /// The lines the story has been told, for the snapshot.
    pub snapshot: Mutex<Vec<String>>,
    /// The alerts pushed to a phone.
    pub pushed: Mutex<Vec<Alert>>,
    /// How many hunts have closed here.
    pub hunts_closed: u64,
    /// Whether the seat's window is kept when the character goes.
    pub keep_window: bool,
}

impl Seat {
    /// A seat for `name`.
    pub fn new(name: &str) -> Seat {
        Seat {
            name: name.to_string(),
            ..Seat::default()
        }
    }

    /// The seat's session.
    pub fn session(&self) -> Option<&SessionHandle> {
        self.session.as_ref()
    }

    /// The words a typed line breaks into, or an empty list when there is no
    /// session.
    ///
    /// **ESTABLISHED** (`0x1013e6894`): `SessionHandle::command_words`.
    pub fn command_words(&self, line: &str) -> Vec<String> {
        match &self.session {
            Some(session) => session.command_words(line),
            None => Vec::new(),
        }
    }

    /// The command symbol, or none when there is no session.
    ///
    /// **ESTABLISHED** (`0x1013e7754`): `0x110000` is the answer for "no
    /// session", which is `None` for an `Option<char>`.
    pub fn symbol(&self) -> Option<char> {
        self.session.as_ref().and_then(SessionHandle::symbol)
    }

    /// The Lich symbol, or none.
    ///
    /// **ESTABLISHED** (`0x1013e6524`).
    pub fn lich_symbol(&self) -> Option<char> {
        self.session.as_ref().and_then(SessionHandle::lich_symbol)
    }

    /// How many hunts have closed on this seat.
    ///
    /// **ESTABLISHED** (`0x1013e663c`): the word at `+0xa70`.
    pub fn hunts_closed(&self) -> u64 {
        self.hunts_closed
    }

    /// Count a hunt as closed.
    pub fn hunt_closed(&mut self) {
        self.hunts_closed = self.hunts_closed.saturating_add(1);
    }

    /// Keep the seat's window when the character goes.
    ///
    /// **ESTABLISHED** (`0x1013e651c`): the byte at `+0xaa4`.
    pub fn keep_window(&mut self, on: bool) {
        self.keep_window = on;
    }

    /// Is the seat's window kept?
    ///
    /// **ESTABLISHED** (`0x1013e653c`).
    pub fn window_kept(&self) -> bool {
        self.keep_window
    }

    /// Put the character on a team.
    ///
    /// **ESTABLISHED** (`0x1013e776c`): a once-only set, so the first team wins.
    pub fn in_team(&self, team: &str) {
        if self.team.get().is_none() {
            let _ = self.team.set(team.to_string());
        }
    }

    /// The team, once it is known.
    ///
    /// **ESTABLISHED** (`0x1013e6bd8`).
    pub fn team(&self) -> Option<&str> {
        self.team.get().map(String::as_str)
    }

    /// Keep the player log.
    ///
    /// **ESTABLISHED** (`0x1013e7a98`): a once-only set; a second log is let go.
    pub fn keep_log(&self, log: PlayerLog) {
        if self.log.get().is_none() {
            let _ = self.log.set(log);
        }
    }

    /// The player log, once it is known.
    ///
    /// **ESTABLISHED** (`0x1013e69b0`).
    pub fn log(&self) -> Option<&PlayerLog> {
        self.log.get()
    }

    /// The story, locked.
    ///
    /// **ESTABLISHED** (`0x1013e741c`): a mutex.
    pub fn story(&mut self) -> MutexGuard<'_, Story> {
        self.story.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// What the seat's window is for.
    ///
    /// **ESTABLISHED** (`0x1013e83c8`): a mutex-guarded string.
    pub fn page_link(&self) -> String {
        self.page_link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Let go of the windows.
    pub fn close_windows(&self) -> Vec<String> {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut windows.open)
    }

    /// The windows that are open.
    pub fn open_windows(&self) -> Vec<String> {
        self.windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .open
            .clone()
    }

    /// Raise a window.
    pub fn raise_window(&self, window: &str) {
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        if !windows.open.iter().any(|open| open == window) {
            windows.open.push(window.to_string());
        }
    }

    /// How far the seat has got.
    ///
    /// **ESTABLISHED** (`0x1013e8308`).
    pub fn lifecycle(&self) -> Lifecycle {
        self.lifecycle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Move the seat's life along.
    pub fn set_lifecycle(&self, state: Lifecycle) {
        *self.lifecycle.lock().unwrap_or_else(|e| e.into_inner()) = state;
    }

    /// What the seat knows, all at once.
    ///
    /// **ESTABLISHED** (`0x1013e813c`): a mutex-guarded copy.
    pub fn snapshot(&self) -> Vec<String> {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The hotbar presses waiting.
    ///
    /// **ESTABLISHED** (`0x1013e68b4`): drained, like [`Seat::rumbles`].
    pub fn hotbar_presses(&self) -> Vec<(String, String)> {
        let mut presses = self
            .hotbar_presses
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *presses)
    }

    /// Press a hotbar button.
    ///
    /// **ESTABLISHED** (`0x1013e6644`): with no window open the board says
    /// [`NO_WINDOW`]; otherwise the press is queued and the window woken.
    pub fn press_hotbar(&self, bar: &str, button: &str) -> Result<(), String> {
        if self.open_windows().is_empty() {
            let mut story = self.story.lock().unwrap_or_else(|e| e.into_inner());
            story.push(NO_WINDOW);
            return Err(NO_WINDOW.to_string());
        }
        self.hotbar_presses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((bar.to_string(), button.to_string()));
        Ok(())
    }

    /// Queue a rumble.
    ///
    /// **ESTABLISHED** (`0x1013e7618`).
    pub fn rumble(&self, pattern: Pattern) {
        self.rumbles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(pattern);
    }

    /// Take the rumbles that are waiting.
    ///
    /// **ESTABLISHED** (`0x1013e77f8`): the list is drained.
    pub fn rumbles(&self) -> Vec<Pattern> {
        let mut rumbles = self.rumbles.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *rumbles)
    }

    /// Send a line to the game.
    ///
    /// **ESTABLISHED** (`0x1013e69c8`): the line is shown as typing and handed
    /// to the session's queue.
    pub fn send(&self, line: &str) {
        if let Some(session) = &self.session {
            let mut story = self.story.lock().unwrap_or_else(|e| e.into_inner());
            story.typing(line);
            session.log_typed(line);
            session.to_game(line);
        }
    }

    /// Type a line: start the trainer if it is a trainer command, else send it.
    ///
    /// **ESTABLISHED** (`0x1013e7490`): [`crate::trainer::typed`] is asked first.
    pub fn typed(&self, line: &str) -> bool {
        match crate::trainer::typed(line) {
            Some(change) => {
                let mut trainer = self.trainer.lock().unwrap_or_else(|e| e.into_inner());
                let _ = trainer.change(change);
                true
            }
            None => {
                self.send(line);
                false
            }
        }
    }

    /// The trainer's status.
    ///
    /// **ESTABLISHED** (`0x1013e78f4`): a mutex-guarded clone.
    pub fn trainer(&self) -> Status {
        self.trainer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .status
            .clone()
    }

    /// The goals the seat is training towards.
    ///
    /// **ESTABLISHED** (`0x1013e8218`): a mutex-guarded clone.
    pub fn ask_goals(&self) -> Option<SkillGoals> {
        self.goals
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Give the seat new goals.
    ///
    /// **ESTABLISHED** (`0x1013e61d0`): the goals are cloned in and the trainer
    /// is told.
    pub fn apply_goals(&self, goals: SkillGoals) {
        *self.goals.lock().unwrap_or_else(|e| e.into_inner()) = Some(goals.clone());
        let mut trainer = self.trainer.lock().unwrap_or_else(|e| e.into_inner());
        let _ = trainer.change(Change::Keep(goals));
    }

    /// Change the goals.
    ///
    /// **ESTABLISHED** (`0x1013e654c`): `trainer::Trainer::change`.
    pub fn change_goals(&self, change: Change) -> Result<(), String> {
        let mut trainer = self.trainer.lock().unwrap_or_else(|e| e.into_inner());
        trainer.change(change)
    }

    /// The first command the trainer wants typed.
    ///
    /// **ESTABLISHED** (`0x1013e7b20`): the trainer launches and its command is
    /// typed.  The tokio task is the seam: this returns the line instead.
    pub fn launched(&self) -> Option<String> {
        let mut trainer = self.trainer.lock().unwrap_or_else(|e| e.into_inner());
        trainer.launched()
    }

    /// Run a set of steps.
    ///
    /// **ESTABLISHED** (`0x1013e84ac`): each step's line is typed and its
    /// duration waited for; the waits were tokio timers, so this returns the
    /// total duration instead of sleeping.
    pub fn run_steps(&self, steps: &[Step]) -> std::time::Duration {
        let mut total = std::time::Duration::ZERO;
        for step in steps {
            if let Some(line) = step.line() {
                self.send(line);
            }
            if let Some(duration) = step.duration() {
                total += duration;
            }
        }
        total
    }

    /// Note that a line was said, for the snapshot.
    pub fn told(&self, line: &str) {
        self.snapshot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line.to_string());
        self.story
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(line);
    }

    /// Say something to the seat's story.
    pub fn say(&self, notice: &Notice) {
        if let Some(session) = &self.session {
            session.say(notice);
        }
        self.told(&notice.first_line());
    }

    /// Push an alert to the character's phone.
    ///
    /// **ESTABLISHED** (`0x1013e95d8`): the alert is cloned and handed to a task
    /// that sends it.  There is no network here, so it is kept for
    /// [`Seat::pushed`].
    pub fn pushed(&self, alert: Alert) {
        self.pushed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(alert);
    }

    /// The alerts that were pushed.
    pub fn pushed_alerts(&self) -> Vec<Alert> {
        self.pushed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Start the seat: a session and a story.
    ///
    /// **ESTABLISHED** (`0x1013e6bfc`): a `Story::default()` and the session's
    /// channels.  The channels and the tokio runtime are the seam, so this hands
    /// the session over to the caller.
    pub fn start(name: &str, session: SessionHandle) -> Seat {
        let seat = Seat {
            name: name.to_string(),
            session: Some(session),
            ..Seat::default()
        };
        seat.set_lifecycle(Lifecycle::Started);
        seat
    }
}

/// At most one window is raised for a seat at a time, and this is how the board
/// asks for it.
///
/// **ESTABLISHED** (`0x1013e8990`, `0x1013e9080`, `0x1013e9330`, `0x1013e9208`,
/// `0x1013e8c90`, `0x1013e8f2c`, `0x1013e8ba8`, `0x1013e8dd4`): a shared list of
/// raised seats behind a mutex.
#[derive(Debug, Default)]
pub struct Wake {
    /// The seats that have been raised, with the window they were raised for.
    pub raised: Mutex<Vec<(String, String)>>,
    /// The window that is open, if any.
    pub open: Mutex<Option<String>>,
    /// The hub window's name, if it is open.
    pub hub: Mutex<Option<String>>,
}

impl Wake {
    /// Raise the window `window` for the seat `seat`.
    ///
    /// **ESTABLISHED** (`0x1013e8990`): the pair is added to the list.
    pub fn raise_seat(&self, seat: &str, window: &str) {
        let mut raised = self.raised.lock().unwrap_or_else(|e| e.into_inner());
        if !raised.iter().any(|(s, w)| s == seat && w == window) {
            raised.push((seat.to_string(), window.to_string()));
        }
    }

    /// The seats that have been raised.
    ///
    /// **ESTABLISHED** (`0x1013e8ba8`).
    pub fn raised_seats(&self) -> Vec<(String, String)> {
        self.raised
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Raise the window that is open.
    ///
    /// **ESTABLISHED** (`0x1013e8f2c`): nothing to raise is not an error.
    pub fn raise(&self) -> bool {
        let open = self.open.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match open {
            Some(window) => {
                let mut raised = self.raised.lock().unwrap_or_else(|e| e.into_inner());
                if !raised.iter().any(|(_, w)| *w == window) {
                    raised.push((String::new(), window));
                }
                true
            }
            None => false,
        }
    }

    /// Wake the board up.
    ///
    /// **ESTABLISHED** (`0x1013e8dd4`): the open window is dropped.
    pub fn wake(&self) -> bool {
        self.open
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some()
    }

    /// A window was opened.
    ///
    /// **ESTABLISHED** (`0x1013e9208`).
    pub fn opened(&self, window: &str) {
        *self.open.lock().unwrap_or_else(|e| e.into_inner()) = Some(window.to_string());
    }

    /// A window was closed.
    ///
    /// **ESTABLISHED** (`0x1013e9080`): only the window that is open is let go.
    pub fn closed(&self, window: &str) -> bool {
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if open.as_deref() == Some(window) {
            *open = None;
            return true;
        }
        false
    }

    /// Is a window open?
    ///
    /// **ESTABLISHED** (`0x1013e9330`).
    pub fn is_open(&self) -> bool {
        self.open
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }

    /// The hub window's name, when it is open.
    ///
    /// **ESTABLISHED** (`0x1013e8c90`).
    pub fn hub(&self) -> Option<String> {
        self.hub.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// What a seat's windows ask of the front end that owns them.
///
/// **ESTABLISHED** (the three vtable entries `hydra_gui::EguiWindows` fills —
/// `<hydra_gui::EguiWindows as hydra_board::seat::Windows>::{open_url, repaint,
/// repaint_seat}` at `0x1005846b4`, `0x1005846a8`, `0x10058462c`):
///
///  * `open_url` allocates a copy of the url and hands `egui::Context::open_url`
///    an `OpenUrl` with `new_tab` **true**;
///  * `repaint` is `egui::Context::request_repaint`;
///  * `repaint_seat` is `request_repaint` and then
///    `request_repaint_of(ViewportId::from_hash_of(("play", seat)))` — the
///    literal `"play"` (four bytes at `0x10208216b`) salted with the seat, so one
///    seat's window can be woken without the others.
///
/// The binary spells this trait `hydra_board::seat::Windows`.  That name is taken
/// here by the recovered per-seat set of open windows ([`Windows`], above), and
/// public items are not renamed, so the trait lives in this module.
pub mod windows {
    /// What a seat's windows ask of the front end that owns them.
    ///
    /// See the module's own note: this is the binary's
    /// `hydra_board::seat::Windows` trait.
    pub trait Windows {
        /// Open `url` in the player's browser.
        fn open_url(&self, url: &str);

        /// Repaint the whole window.
        fn repaint(&self);

        /// Repaint the window `seat` is played in.
        fn repaint_seat(&self, seat: u32);
    }
}

/// The word `repaint_seat` salts a seat's viewport id with.
///
/// **ESTABLISHED** (`0x10208216b`, four bytes).
pub const PLAY: &str = "play";

/// [`windows::Windows`] over a real [`egui::Context`] — what `hydra_gui`'s
/// `EguiWindows` was.
///
/// **ESTABLISHED** (`0x1005846b4`, `0x1005846a8`, `0x10058462c`): all three
/// forward to the context, and `open_url` opens a new tab.
#[derive(Clone, Copy)]
pub struct EguiWindows<'a>(pub &'a egui::Context);

impl windows::Windows for EguiWindows<'_> {
    fn open_url(&self, url: &str) {
        self.0.open_url(egui::OpenUrl::new_tab(url));
    }

    fn repaint(&self) {
        self.0.request_repaint();
    }

    fn repaint_seat(&self, seat: u32) {
        self.0.request_repaint();
        self.0
            .request_repaint_of(egui::ViewportId::from_hash_of((PLAY, seat)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hotbar_press_is_recognised() {
        assert!(is_hotbar_press("hotbar main look"));
        assert!(is_hotbar_press("  hotbar main look"));
        assert!(!is_hotbar_press("hotbars"));
        assert!(!is_hotbar_press("look"));
    }

    #[test]
    fn the_once_only_fields_keep_the_first_answer() {
        let seat = Seat::new("Al");
        assert!(seat.team().is_none());
        seat.in_team("the wolves");
        seat.in_team("someone else");
        assert_eq!(seat.team(), Some("the wolves"));
        assert!(seat.log().is_none());
    }

    #[test]
    fn pressing_a_hotbar_needs_a_window() {
        let seat = Seat::new("Al");
        assert_eq!(
            seat.press_hotbar("main", "look").unwrap_err(),
            NO_WINDOW.to_string()
        );
        seat.raise_window("story");
        assert!(seat.open_windows().contains(&"story".to_string()));
        assert!(seat.press_hotbar("main", "look").is_ok());
        // The press is drained.
        assert_eq!(
            seat.hotbar_presses(),
            vec![("main".to_string(), "look".to_string())]
        );
        assert!(seat.hotbar_presses().is_empty());
    }

    #[test]
    fn counters_and_flags() {
        let mut seat = Seat::new("Al");
        assert_eq!(seat.hunts_closed(), 0);
        seat.hunt_closed();
        assert_eq!(seat.hunts_closed(), 1);
        assert!(!seat.window_kept());
        seat.keep_window(true);
        assert!(seat.window_kept());
    }

    #[test]
    fn rumbles_are_drained_and_the_trainer_can_be_told() {
        let seat = Seat::new("Al");
        seat.rumble(Pattern::built_in("short").unwrap());
        assert_eq!(seat.rumbles().len(), 1);
        assert!(seat.rumbles().is_empty());
        seat.apply_goals(SkillGoals {
            name: "combat".into(),
            goals: vec![crate::trainer::Goal {
                skill: "Dodging".into(),
                to: 20,
            }],
        });
        assert_eq!(seat.ask_goals().unwrap().goals.len(), 1);
        assert_eq!(seat.launched(), Some("train Dodging".to_string()));
        assert_eq!(seat.trainer(), Status::Training);
    }

    #[test]
    fn the_wake_list_holds_one_entry_per_window() {
        let wake = Wake::default();
        assert!(!wake.is_open());
        wake.opened("story");
        assert!(wake.is_open());
        wake.raise();
        wake.raise();
        assert_eq!(wake.raised_seats().len(), 1);
        assert!(wake.closed("story"));
        assert!(!wake.is_open());
        assert!(!wake.closed("story"));
        assert!(!wake.raise());
        wake.raise_seat("Al", "story");
        assert_eq!(wake.raised_seats().len(), 2);
        assert!(wake.hub().is_none());
    }

    #[test]
    fn egui_windows_opens_urls_in_a_new_tab() {
        use windows::Windows as _;

        let ctx = egui::Context::default();
        let out = ctx.run_ui(crate::testkit::input(crate::testkit::SCREEN), |ui| {
            let _ = ui;
            EguiWindows(&ctx).open_url("https://example.com/play");
        });
        // `open_url` becomes an OutputCommand for the integration to run.
        let said = out.platform_output.commands.clone();
        assert_eq!(said.len(), 1, "{said:?}");
        match &said[0] {
            egui::OutputCommand::OpenUrl(open) => {
                assert_eq!(open.url, "https://example.com/play");
                assert!(open.new_tab, "the recovered call opens a new tab");
            }
            other => panic!("{other:?}"),
        }
    }

    /// How many of the last frame's repaint causes were asked for from
    /// `seat.rs`, i.e. by [`EguiWindows`].
    ///
    /// `Context::repaint_causes` reports the *previous* pass, so one more empty
    /// frame has to be run before reading it.
    fn asked_here(ctx: &egui::Context) -> usize {
        let _ = ctx.run_ui(crate::testkit::input(crate::testkit::SCREEN), |ui| {
            let _ = ui;
        });
        ctx.repaint_causes()
            .iter()
            .filter(|cause| cause.file.ends_with("seat.rs"))
            .count()
    }

    #[test]
    fn egui_windows_repaints_a_seat_and_everything() {
        use windows::Windows as _;

        let seat = egui::ViewportId::from_hash_of((PLAY, 3u32));
        let other = egui::ViewportId::from_hash_of((PLAY, 4u32));
        let ctx = egui::Context::default();
        assert!(!ctx.has_requested_repaint_for(&seat));
        // What the context was told, read back inside the pass: egui drops a
        // child viewport that was never shown, so the request is only observable
        // while the frame that made it is running.
        let mut woke = (false, false, false);
        let _ = ctx.run_ui(crate::testkit::input(crate::testkit::SCREEN), |ui| {
            let _ = ui;
            EguiWindows(&ctx).repaint_seat(3);
            woke = (
                ctx.has_requested_repaint_for(&seat),
                ctx.has_requested_repaint_for(&other),
                ctx.has_requested_repaint(),
            );
        });
        assert!(woke.0, "the seat's own viewport was woken");
        assert!(!woke.1, "no other seat was");
        assert!(woke.2, "the whole window was too");
        // And the plain request came from this file's `repaint`.
        assert_eq!(asked_here(&ctx), 1);

        // A plain `repaint` wakes only the window it is in.
        let ctx = egui::Context::default();
        let mut woke = (false, false);
        let _ = ctx.run_ui(crate::testkit::input(crate::testkit::SCREEN), |ui| {
            let _ = ui;
            EguiWindows(&ctx).repaint();
            woke = (
                ctx.has_requested_repaint_for(&seat),
                ctx.has_requested_repaint(),
            );
        });
        assert!(woke.1);
        assert!(!woke.0, "no seat's viewport was named");
        assert_eq!(asked_here(&ctx), 1);
    }
}

