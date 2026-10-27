//! One connected web page: its state and every command it can send.
//!
//! The original `Page` is a large struct driven by an async websocket task
//! (`client::run`).  Each handler below was read out of the binary; the pieces
//! that talk to the game live behind the [`Seat`] seam, because the crate that
//! owns them (`hydra_board`) is not recovered.
//!
//! The driver itself is here too, on real `tokio`: [`relay`] is the recovered
//! read-a-frame → parse-`wire::In` → run-the-handler → write-`wire::Out` loop,
//! [`asked`] is its message dispatch, and [`serve_socket`] bridges a blocking
//! `tungstenite` socket onto the same loop.  The synchronous handlers and
//! builders are unchanged.

use std::path::{Path, PathBuf};

use crate::wire::{self, In, Out};
use crate::workspace;
use crate::{
    highlights, injuries, inventory, notes as notes_module, phone, tasks as tasks_module,
    trainer as trainer_module,
};

// ---------------------------------------------------------------------------
// Recovered user-facing strings
// ---------------------------------------------------------------------------

/// `0x102071fec`, 55 bytes — `Page::refused_here`.
pub const LOGGED_IN_HERE: &str = "A character is logged in from the device Hydra runs on.";
/// `0x102072023`, 41 bytes.
pub const NO_LONGER_HERE: &str = "That character is no longer on the table.";
/// `0x10207204c`, 41 bytes — `Page::inventory_act`.
pub const NOT_OFFERED: &str = "Not sent: that item does not offer that; ";
/// `0x102072050`, 22 bytes.
pub const REFRESH_INVENTORY: &str = "Refresh the inventory.";
/// `0x10207208b`, 29 bytes — `Page::highlights_merge`.
pub const TOO_BIG_TO_MERGE: &str = "The copy is too big to merge.";
/// `0x1020720ae`, 40 bytes — `Page::press`.
pub const NOT_KNOWN_YET: &str = "Not yet: the character is not known yet.";
/// `0x102071f9e`, 41 bytes — `Page::push_asked`.
pub const PUSH_NOT_PAIRED: &str = "Push: this phone was not paired for push.";
/// `0x102071fc7`, 37 bytes.
pub const PUSH_EXHAUSTED: &str = "Push: this phone gets no more pushes.";
/// `0x1020699b3`, 32 bytes — `Page::request`.
pub const NOT_READY: &str = "Hydra is not ready for that yet.";
/// The static asset `Page::request` special-cases (`0x102069a90`).
pub const WHEEL_EDITOR: &str = "/wheel-editor.js";

// ---------------------------------------------------------------------------
// The seat seam
// ---------------------------------------------------------------------------

/// Everything a page handler needs from the running character.
///
/// Stand-in for `hydra_board::seat::Seat` (unrecovered).  The recovered calls
/// are `symbol`, `lich_symbol`, `send`, `story`, `snapshot`, `trainer`,
/// `change_goals`, `apply_goals`, `run_steps`.
pub trait Seat {
    /// The command prefix the character uses (`Seat::symbol`, `'.'` when unset).
    fn symbol(&self) -> Option<char>;
    /// The Lich command prefix (`Seat::lich_symbol`, `';'` when unset).
    fn lich_symbol(&self) -> Option<char>;
    /// Send one line to the game (`Seat::send`).
    fn send(&mut self, line: &str);
    /// The character's name, when the seat is attached.
    fn name(&self) -> Option<&str>;
    /// The game code, e.g. `GS4`.
    fn game(&self) -> &str;
}

/// The prefix a handler should use: the recovered fallbacks are `.` for the
/// character's own symbol and `;` for Lich's.
///
/// `0x1002bb1d4`: `cmp w0, #0x110000` (the `Option<char>` niche) then
/// `csel '.'` / `csel ';'`.
pub fn symbol_of(seat: &dyn Seat) -> char {
    seat.symbol().unwrap_or('.')
}

/// See [`symbol_of`].
pub fn lich_symbol_of(seat: &dyn Seat) -> char {
    seat.lich_symbol().unwrap_or(';')
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

/// One attached web page.
///
/// Recovered field offsets: the outbox is a `Vec<Out>` at `+0x348` (elements are
/// 0xa8 bytes apart) and the "already refused" latch is the byte at `+0x594`.
pub struct Page {
    /// Messages waiting to be written to the socket.
    pub outbox: Vec<Out>,
    /// Set once the "a character is logged in here" notice has been sent.
    pub refused: bool,
    /// The storage root (`~/.hydra` and friends).
    pub root: PathBuf,
    /// The character this page is attached to.
    pub character: String,
    /// The account the character belongs to.
    pub account: String,
    /// The game code.
    pub game: String,
    /// The highlight file, loaded on attach.
    pub highlights: highlights::HighlightFile,
    /// The inbox id the page last acknowledged.
    pub id: u32,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("outbox", &self.outbox.len())
            .field("refused", &self.refused)
            .field("character", &self.character)
            .field("id", &self.id)
            .finish()
    }
}

impl Page {
    /// A page attached to `character` of `account`.
    pub fn new(root: impl Into<PathBuf>, account: &str, character: &str, game: &str) -> Page {
        Page {
            outbox: Vec::new(),
            refused: false,
            root: root.into(),
            character: character.to_string(),
            account: account.to_string(),
            game: game.to_string(),
            highlights: highlights::HighlightFile::default(),
            id: 0,
        }
    }

    fn push(&mut self, out: Out) {
        self.outbox.push(out);
    }

    /// Send one line to the game through `seat`.
    pub fn say(&mut self, seat: &mut dyn Seat, line: &str) {
        seat.send(line);
    }

    // -- recovered handlers -------------------------------------------------

    /// Tell the page once that another character owns the game connection.
    ///
    /// **ESTABLISHED** (`0x1002b33fc`, 272 bytes): if the latch byte at `+0x594`
    /// is already set the answer is `false`; otherwise an `Out::Refused` with
    /// [`LOGGED_IN_HERE`] is queued, the latch is set and the answer is `true`.
    pub fn refused_here(&mut self) -> bool {
        if self.refused {
            return false;
        }
        self.push(Out::Refused {
            why: LOGGED_IN_HERE.to_string(),
        });
        self.refused = true;
        true
    }

    /// The character said something in the game window.
    ///
    /// **ESTABLISHED** (`0x1002bb1d4`): the line is filtered through the story
    /// (`hydra_ui::run::game_line`) with the seat's own symbol (`'.'` when unset)
    /// and Lich's (`';'` when unset), then sent.
    pub fn game_said(&mut self, seat: &mut dyn Seat, line: &str) {
        let symbol = symbol_of(seat);
        let lich = lich_symbol_of(seat);
        // The original re-prefixed a command that names another character with
        // the seat's symbols; the plain case is sent unchanged.
        let mut text = line.to_string();
        if let Some(rest) = text.strip_prefix(lich) {
            text = format!("{symbol}{rest}");
        }
        self.say(seat, &text);
        self.push(Out::Said { text });
    }

    /// A hotbar button was pressed (`0x1002bb018`).
    ///
    /// Recovered: `hydra_board::hotbars::button_lines` for the button id, then
    /// each resulting line goes through `Seat::send`.  A button that produced no
    /// lines says nothing (the recovered code loops over the `Vec<String>` and
    /// stops).
    pub fn button(&mut self, seat: &mut dyn Seat, lines: &[String]) {
        for line in lines {
            self.say(seat, line);
        }
    }

    /// A tap on the directional pad (`0x1002b9974`).
    ///
    /// Recovered: `hydra_board::pad::touch::touch_command` builds the command
    /// from the direction, `trim_matches` cleans it and `Seat::send` sends it.
    pub fn wheel(&mut self, seat: &mut dyn Seat, direction: &str) {
        let command = wheel_command(direction);
        if command.is_empty() {
            return;
        }
        self.say(seat, &command);
        self.push(Out::Wheel { wheel: command });
    }

    /// A click on a link in the story (`0x1002b46cc`).
    ///
    /// Recovered: `hydra_ui::object_menu::link_command` turns the noun into a
    /// command; `Seat::snapshot` is consulted first and an empty command is not
    /// sent.
    pub fn link(&mut self, seat: &mut dyn Seat, noun: &str) {
        let noun = noun.trim();
        if noun.is_empty() {
            return;
        }
        let command = format!("{}{noun}", symbol_of(seat));
        self.say(seat, &command);
        self.game_said(seat, &command);
    }

    /// An inventory action (`0x1002b3734`).
    ///
    /// **ESTABLISHED** message pair: when the item does not offer the act the page
    /// gets `"Not sent: that item does not offer that; "` followed by
    /// `"Refresh the inventory."`.
    pub fn inventory_act(
        &mut self,
        seat: &mut dyn Seat,
        snapshot: &hydra_model::state::inventory_snapshot::InventorySnapshot,
        act: &str,
        item: &str,
    ) -> bool {
        if !inventory::offered(snapshot, item, act) {
            self.push(Out::Phone {
                kind: "inventory".to_string(),
                text: format!("{NOT_OFFERED}{REFRESH_INVENTORY}"),
            });
            return false;
        }
        let command = format!("{} {item}", inventory::Act::named(act).map(|a| a.command()).unwrap_or(act));
        self.say(seat, &command);
        true
    }

    /// The highlight document, for the editor (`0x1002b41c8`).
    ///
    /// Recovered: `Highlights::file`, `HighlightFile::to_toml` and
    /// `highlights::changed` are combined into `Out::HighlightsText`.
    pub fn highlights_text(&mut self) {
        let text = highlights::render(&self.highlights);
        let changed = highlights::changed(&self.root);
        self.push(Out::HighlightsText { changed, text });
    }

    /// Merge a pasted highlight document (`0x1002b43d8`).
    ///
    /// Recovered: too big → [`TOO_BIG_TO_MERGE`]; otherwise the merge runs and
    /// the editor text is re-sent.
    pub fn highlights_merge(&mut self, text: &str) -> Result<(), String> {
        match highlights::merge(&mut self.highlights, text) {
            Ok(_) => {
                self.highlights_text();
                Ok(())
            }
            Err(why) => {
                self.push(Out::Refused { why });
                Err(TOO_BIG_TO_MERGE.to_string())
            }
        }
    }

    /// The highlight editor's lists (`Page::look`'s highlights part).
    pub fn highlights_listed(&mut self) {
        let (categories, rules) = highlights::listed(&self.highlights);
        let sounds = highlights::sounds(&self.root);
        self.push(Out::Highlights {
            categories,
            rules,
            sounds,
        });
    }

    /// Push this phone's state to the page (`0x1002b2f44`).
    ///
    /// **ESTABLISHED**: an unpaired phone gets [`PUSH_NOT_PAIRED`]; a paired one
    /// gets [`PUSH_EXHAUSTED`] once it asks to forget.
    pub fn push_asked(&mut self, paired: bool, forget: bool) {
        if forget {
            self.push(Out::PushPaired);
            self.push(Out::Refused {
                why: PUSH_EXHAUSTED.to_string(),
            });
            return;
        }
        if !paired {
            self.push(Out::Refused {
                why: PUSH_NOT_PAIRED.to_string(),
            });
            return;
        }
        self.push(Out::PushPaired);
    }

    /// The phone state of one kind (`0x1002b94a4`).
    ///
    /// Recovered: `phone::kept` is read and an `Out::Phone` carries the document.
    pub fn phone(&mut self, kind: &str) {
        match phone::kept(&self.root, kind) {
            Some((text, _when)) => self.push(Out::Phone {
                kind: kind.to_string(),
                text,
            }),
            None => self.push(Out::Phone {
                kind: kind.to_string(),
                text: String::new(),
            }),
        }
    }

    /// Store one kind of phone state (`0x1002bb37c`).
    pub fn phone_put(&mut self, kind: &str, text: &str) -> Result<(), String> {
        phone::keep(&self.root, kind, text)?;
        self.phone(kind);
        Ok(())
    }

    /// Send one command and echo it in the story (`0x1002b96b4`).
    ///
    /// Recovered: `Seat::story` gives the line a place in the story, then
    /// `Seat::send`; a seat with no character answers [`NOT_KNOWN_YET`].
    pub fn press(&mut self, seat: &mut dyn Seat, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        if seat.name().is_none() {
            self.push(Out::Refused {
                why: NOT_KNOWN_YET.to_string(),
            });
            return false;
        }
        self.say(seat, text);
        self.push(Out::Said {
            text: text.to_string(),
        });
        true
    }

    /// The trainer panel (`0x1002b3ab0`).
    ///
    /// Recovered: `Seat::trainer`, `GoalProfiles::{load, change}`, the
    /// `SkillGoals::targets` map and `Seat::{change_goals, apply_goals}`.
    pub fn trainer_asked(&mut self, profiles: Vec<String>, sheet: Option<&str>, said: &str) {
        let out = trainer_module::trainer(profiles, said, sheet, false);
        self.push(Out::Trainer(out));
    }

    /// One skill-step edit (`Page::trainer_asked`'s step branch).
    pub fn trainer_step(&mut self, seat: &mut dyn Seat, skill: &str, step: i32) -> bool {
        let skill = skill.trim();
        if skill.is_empty() {
            return false;
        }
        let line = format!("{} train {skill} {step}", symbol_of(seat));
        self.say(seat, &line);
        true
    }

    /// The character's workspace file (`0x1002b350c`).
    ///
    /// Recovered: `Sessions::glance` + `workspace::path`, and the fixed
    /// `Out::Workspace` pair.  `status` is the HTTP status the page should
    /// expect for its own workspace endpoint.
    pub fn workspace_of(&mut self, ask: u32) -> PathBuf {
        let path = workspace::path(&self.root, &self.account, &self.character);
        self.push(Out::Workspace { ask, status: 200 });
        path
    }

    /// The whole game panel (`Page::look`, `0x1002b4c84`).
    pub fn look(&mut self, view: wire::View) {
        self.push(Out::View(view));
    }

    /// The initial page load (`0x1002b9c04`).
    ///
    /// Recovered: `Story::scrollback` (story + stream lines), `own_streams`,
    /// `injuries::doll`, the settings store and `pad::touch::touch_wheel` are
    /// combined into `Out::Attached`, `Out::View`, `Out::Injuries` and
    /// `Out::Hub`.
    pub fn attach(
        &mut self,
        id: u32,
        streams: Vec<wire::StreamOut>,
        story_lines: Vec<wire::Line>,
        stream_lines: Vec<Vec<wire::Line>>,
        view: Option<wire::View>,
    ) {
        self.id = id;
        self.push(Out::Attached {
            name: self.character.clone(),
            game: self.game.clone(),
            streams,
            story_lines,
            stream_lines,
            id,
        });
        self.push(Out::Injuries(injuries::doll()));
        if let Some(view) = view {
            self.push(Out::View(view));
        }
    }

    /// The notes/hub panels (`Page::panel`, `0x1002b8cd4`).
    ///
    /// Recovered: `hydra_session::notes::{write, rewrite, remove, stamp}`; the
    /// page gets `Out::Note` rows.
    pub fn panel(&mut self) {
        match notes_module::read(&self.root, &self.character) {
            notes_module::NotesOut::Notes(rows) => {
                for row in rows {
                    self.push(Out::Note(row));
                }
            }
            notes_module::NotesOut::Problem(problem) => {
                self.push(Out::Refused { why: problem });
            }
        }
    }

    /// Write a note (`Page::panel`'s `In::NoteWrite`).
    pub fn note_write(&mut self, key: &str, text: &str) -> Result<(), String> {
        hydra_session::notes::rewrite(&self.root, |notes| {
            if let Some(note) = notes.notes.iter_mut().find(|n| n.key == key) {
                note.lines = text.lines().map(str::to_string).collect();
                note.when = hydra_session::notes::stamp();
            } else {
                notes
                    .notes
                    .push(hydra_session::notes::Note::new(key, key, text));
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
    }

    /// Forget a note (`Page::panel`'s `In::NoteForget`).
    pub fn note_forget(&mut self, key: &str) -> Result<bool, String> {
        hydra_session::notes::remove(&self.root, key).map_err(|e| e.to_string())
    }

    /// The theme picker (`Page::wear`, `0x1002b8980`).
    ///
    /// Recovered: `hydra_ui::theme::file::Themes::{load, outfit}`; the page gets
    /// `Out::Themes` with the outfit names.
    pub fn wear(&mut self, names: Vec<String>) {
        self.push(Out::Themes { names });
    }

    /// The trainer's goal-file folder (`TrainerPanel::open`, `0x1002b2e78`).
    pub fn trainer_folder(&self) -> PathBuf {
        self.root.join("trainer")
    }

    /// Tasks panel (`0x1002c48c`'s sibling).
    pub fn tasks(&mut self, stream_text: &str) {
        self.push(Out::Tasks(tasks_module::tasks(stream_text)));
    }

    /// Inventory panel.
    pub fn inventory(
        &mut self,
        snapshot: &hydra_model::state::inventory_snapshot::InventorySnapshot,
        summary: &str,
        room_left: bool,
    ) {
        self.push(Out::Inventory(inventory::inventory(
            snapshot, summary, room_left,
        )));
    }
}

/// The command for a directional pad tap.
///
/// `// UNKNOWN:` the exact `hydra_board::pad::touch::touch_command` table (the
/// crate is unrecovered).  The recovered directions are the pool words
/// `Top/Middle/Bottom`, `Centre`, `Left/Right`, `across`, `down`, `up`.
pub fn wheel_command(direction: &str) -> String {
    let direction = direction.trim().to_ascii_lowercase();
    match direction.as_str() {
        "" => String::new(),
        "up" | "top" => "n".to_string(),
        "down" | "bottom" => "s".to_string(),
        "left" => "w".to_string(),
        "right" => "e".to_string(),
        "centre" | "center" | "middle" => "look".to_string(),
        other => other.to_string(),
    }
}

/// The websocket frame body for one message (`client::message`, `0x1002bb490`).
///
/// **ESTABLISHED**: `serde_json::to_string` then `bytes::Bytes::from(String)`.
/// `bytes` is not a workspace pin, so the bytes are a `Vec<u8>` here;
/// [`frame`] is the same document as the `tungstenite` message the socket
/// actually sends.
pub fn message(out: &Out) -> Vec<u8> {
    out.to_frame()
}

/// One document as the websocket text message the page receives.
///
/// The original fed `bytes::Bytes` of the JSON into `tungstenite`; this is that
/// message, built with the `tungstenite` the original linked.
pub fn frame(out: &Out) -> tungstenite::Message {
    tungstenite::Message::text(out.to_json())
}

/// The page message in a websocket message, when it is a text frame.
///
/// `None` for binary/ping/pong/close frames (they carry no `wire::In`).
pub fn told(msg: &tungstenite::Message) -> Option<Result<In, String>> {
    match msg {
        tungstenite::Message::Text(text) => Some(In::from_json(text.as_str())),
        _ => None,
    }
}

/// The trainer panel of one character.
///
/// Recovered from `client::TrainerPanel::open` (`0x1002b2e78`): the goal-file
/// folder is joined with `"trainer"` (`0x102071f8d`, 7 bytes) and
/// `hydra_board::trainer::GoalProfiles::load` reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrainerPanel {
    /// The folder the goal profiles live in.
    pub folder: PathBuf,
    /// The profile names that were found.
    pub profiles: Vec<String>,
}

impl TrainerPanel {
    /// Open the panel for `root` (`0x1002b2e78`).
    ///
    /// A missing folder yields an empty profile list, never an error — the
    /// original's `GoalProfiles::load` returns a map, so a missing directory is
    /// just an empty one.
    pub fn open(root: &Path) -> TrainerPanel {
        let folder = root.join("trainer");
        let mut profiles: Vec<String> = std::fs::read_dir(&folder)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter_map(|e| {
                        let path = e.path();
                        if path.extension().map(|x| x == "json").unwrap_or(false) {
                            path.file_stem()
                                .and_then(|s| s.to_str())
                                .map(str::to_string)
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        profiles.sort();
        TrainerPanel { folder, profiles }
    }
}

// ---------------------------------------------------------------------------
// The async relay (tokio)
// ---------------------------------------------------------------------------

/// Apply one page message to `page` — the recovered `Page::asked` dispatch.
///
/// The returned messages are the ones this handler queued (the page's outbox is
/// drained for them).  The messages whose work lives in the unrecovered
/// `hydra_board`/`hydra_ui` crates — `login`, `characters`, `launch`,
/// `launch_team`, `choose`, `reconnect`, `attach`, `streams`, `stream`, `theme`,
/// `tasks`, `inventory`, `inventory_act`, `trainer`, `trainer_profile`,
/// `hotbar`, `note_new`, `clip`, `remove` — are accepted and leave the outbox
/// empty: they need a [`Seat`] implementation to do anything.
pub fn asked(page: &mut Page, seat: &mut dyn Seat, msg: In) -> Vec<Out> {
    let before = page.outbox.len();
    match msg {
        In::Send { text } => {
            page.press(seat, &text);
        }
        In::Wheel { direction } => page.wheel(seat, &direction),
        // `hotbars::button_lines` belongs to `hydra_board`; the id is the line.
        In::Button { id } => page.button(seat, std::slice::from_ref(&id)),
        In::Link { noun } => page.link(seat, &noun),
        In::Notes => page.panel(),
        In::NoteWrite { key, text } => {
            let _ = page.note_write(&key, &text);
        }
        In::NoteForget { key } => {
            let _ = page.note_forget(&key);
        }
        In::Highlights => page.highlights_listed(),
        In::HighlightsMerge { text } => {
            let _ = page.highlights_merge(&text);
        }
        In::HighlightSave { form } => {
            highlights::save(&mut page.highlights, std::slice::from_ref(&form));
            page.highlights_text();
        }
        In::HighlightRemove { name } => {
            page.highlights.remove(&name);
            page.highlights_text();
        }
        In::HighlightSwitch { name, on } => {
            page.highlights.switch(&name, on);
            page.highlights_text();
        }
        // `In::PhoneGet` carries no kind; the page's own state is `ui`.
        In::PhoneGet => page.phone("ui"),
        In::PhonePut { kind, text } => {
            let _ = page.phone_put(&kind, &text);
        }
        In::PushPair { .. } => page.push_asked(true, false),
        In::PushForget => page.push_asked(true, true),
        In::Workspace => {
            page.workspace_of(0);
        }
        In::TrainerStep { skill, step } => {
            page.trainer_step(seat, &skill, step);
        }
        _ => {}
    }
    page.outbox.split_off(before)
}

/// The relay's end of a page socket: `wire::In` text frames in, `wire::Out`
/// text frames out.
#[derive(Debug)]
pub struct Relay {
    /// The page's messages, in arrival order (each one is the JSON body of a
    /// websocket text frame).
    pub incoming: tokio::sync::mpsc::UnboundedReceiver<String>,
    /// What the page is sent, in order.
    pub outgoing: tokio::sync::mpsc::UnboundedSender<String>,
}

/// The socket's end of a [`Relay`].
#[derive(Debug)]
pub struct Link {
    /// Feed the page a text frame.
    pub incoming: tokio::sync::mpsc::UnboundedSender<String>,
    /// What the relay wrote for the page.
    pub outgoing: tokio::sync::mpsc::UnboundedReceiver<String>,
}

/// A connected pair of text-frame channels: the relay on one side, the socket
/// on the other.
pub fn frames() -> (Relay, Link) {
    let (to_page, incoming) = tokio::sync::mpsc::unbounded_channel();
    let (outgoing, to_socket) = tokio::sync::mpsc::unbounded_channel();
    (
        Relay { incoming, outgoing },
        Link {
            incoming: to_page,
            outgoing: to_socket,
        },
    )
}

/// What a relayed session did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Relayed {
    /// Page messages that parsed and reached the handler.
    pub handled: usize,
    /// Text frames that were not `wire::In` documents (dropped, as the original's
    /// parser did).
    pub rejected: usize,
}

/// Relay a page (`client::run`'s loop).
///
/// Every text frame the page sends is parsed as a [`wire::In`] and handed to
/// `handle`; every [`Out`] the handler answers with is written back as one text
/// frame — the JSON body [`message`] produces.  The loop ends when the page's
/// side of the channel closes, or when the socket side is gone.
pub async fn relay<H>(relay: &mut Relay, mut handle: H) -> Relayed
where
    H: FnMut(In) -> Vec<Out>,
{
    let mut done = Relayed::default();
    while let Some(text) = relay.incoming.recv().await {
        let Ok(msg) = In::from_json(&text) else {
            done.rejected += 1;
            continue;
        };
        for out in handle(msg) {
            // One JSON document per websocket text frame.
            if relay.outgoing.send(out.to_json()).is_err() {
                return done;
            }
        }
        done.handled += 1;
    }
    done
}

/// How long [`serve_socket`] waits for a frame before it drains the outbox.
///
/// The blocking `tungstenite` socket cannot be interrupted, so it is read with a
/// short `SO_RCVTIMEO` and the outgoing frames are flushed between reads.
pub const TICK: std::time::Duration = std::time::Duration::from_millis(20);

/// How long a peer gets to complete its handshake in [`serve_socket`].
pub const HANDSHAKE: std::time::Duration = std::time::Duration::from_secs(10);

/// Bridge a blocking `tungstenite` server socket onto a [`Relay`].
///
/// The recovered server spoke `tungstenite` text frames over the accepted
/// socket; `tokio-tungstenite`'s `WebSocketStream` is the async equivalent, but
/// polling it needs the `futures-util` `Stream`/`Sink` traits, which are **not**
/// in the workspace pins.  So the socket is driven by `tungstenite`'s blocking
/// API on a blocking task ([`tokio::task::spawn_blocking`]) and the two
/// directions are joined with tokio channels.  The handshake is `tungstenite`'s
/// (`accept`, i.e. its own `101` bytes); [`crate::http::handshake`] is the path
/// that writes the recovered template bytes instead.
///
/// Must be called from inside a tokio runtime; the returned join handle ends
/// when the socket closes.
pub fn serve_socket(
    stream: std::net::TcpStream,
) -> std::io::Result<(Relay, tokio::task::JoinHandle<()>)> {
    stream.set_nodelay(true).ok();
    // The handshake may take as long as the peer needs …
    stream.set_read_timeout(Some(HANDSHAKE))?;
    let (relay, link) = frames();
    let Link {
        incoming: to_page,
        mut outgoing,
    } = link;
    let handle = tokio::task::spawn_blocking(move || {
        use std::io::ErrorKind;
        use tungstenite::Message;

        let Ok(mut socket) = tungstenite::accept(stream) else {
            return;
        };
        // … but a blocking `read` cannot be interrupted, so from here on the
        // socket is read with a short timeout and the outgoing frames are
        // written between reads.
        socket.get_ref().set_read_timeout(Some(TICK)).ok();
        loop {
            while let Ok(text) = outgoing.try_recv() {
                if socket.send(Message::text(text)).is_err() {
                    return;
                }
            }
            match socket.read() {
                Ok(Message::Text(text)) => {
                    if to_page.send(text.to_string()).is_err() {
                        return;
                    }
                }
                Ok(Message::Close(_)) => return,
                // Ping/pong and binary frames carry no page message.
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return,
            }
        }
    });
    Ok((relay, handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_model::state::inventory_snapshot::{InventorySnapshot, Item};

    struct FakeSeat {
        sent: Vec<String>,
        name: Option<String>,
    }

    impl FakeSeat {
        fn new(name: Option<&str>) -> FakeSeat {
            FakeSeat {
                sent: Vec::new(),
                name: name.map(str::to_string),
            }
        }
    }

    impl Seat for FakeSeat {
        fn symbol(&self) -> Option<char> {
            Some('.')
        }
        fn lich_symbol(&self) -> Option<char> {
            None
        }
        fn send(&mut self, line: &str) {
            self.sent.push(line.to_string());
        }
        fn name(&self) -> Option<&str> {
            self.name.as_deref()
        }
        fn game(&self) -> &str {
            "GS4"
        }
    }

    fn page() -> Page {
        Page::new(std::env::temp_dir(), "ACCT", "Ima", "GS4")
    }

    #[test]
    fn message_is_the_json_frame() {
        let bytes = message(&Out::Said {
            text: "hi".into(),
        });
        assert_eq!(String::from_utf8(bytes).unwrap(), r#"{"t":"said","text":"hi"}"#);
    }

    #[test]
    fn refused_here_latches() {
        let mut p = page();
        assert!(p.refused_here());
        assert_eq!(p.outbox.len(), 1);
        assert_eq!(
            p.outbox[0],
            Out::Refused {
                why: LOGGED_IN_HERE.to_string()
            }
        );
        assert!(!p.refused_here());
        assert_eq!(p.outbox.len(), 1, "only announced once");
    }

    #[test]
    fn refuses_are_the_recovered_sentences() {
        assert_eq!(LOGGED_IN_HERE.len(), 55);
        assert_eq!(NO_LONGER_HERE.len(), 41);
        assert_eq!(NOT_OFFERED.len(), 41);
        assert_eq!(REFRESH_INVENTORY.len(), 22);
        assert_eq!(TOO_BIG_TO_MERGE.len(), 29);
        assert_eq!(NOT_KNOWN_YET.len(), 40);
        assert_eq!(PUSH_NOT_PAIRED.len(), 41);
        assert_eq!(PUSH_EXHAUSTED.len(), 37);
        assert_eq!(NOT_READY.len(), 32);
        assert_eq!(WHEEL_EDITOR, "/wheel-editor.js");
    }

    #[test]
    fn push_asked_reports_pairing() {
        let mut p = page();
        p.push_asked(false, false);
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Refused {
                why: PUSH_NOT_PAIRED.to_string()
            }
        );
        let mut p = page();
        p.push_asked(true, false);
        assert_eq!(p.outbox, vec![Out::PushPaired]);
        let mut p = page();
        p.push_asked(true, true);
        assert_eq!(p.outbox.len(), 2);
        assert_eq!(
            p.outbox[1],
            Out::Refused {
                why: PUSH_EXHAUSTED.to_string()
            }
        );
    }

    #[test]
    fn game_said_uses_the_lich_fallback_symbol() {
        let mut p = page();
        let mut seat = FakeSeat::new(Some("Ima"));
        p.game_said(&mut seat, ";who");
        assert_eq!(seat.sent, vec![".who"], "';' is rewritten to the seat symbol");
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Said {
                text: ".who".to_string()
            }
        );
    }

    #[test]
    fn press_needs_a_character() {
        let mut p = page();
        let mut seat = FakeSeat::new(None);
        assert!(!p.press(&mut seat, "look"));
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Refused {
                why: NOT_KNOWN_YET.to_string()
            }
        );
        let mut seat = FakeSeat::new(Some("Ima"));
        assert!(p.press(&mut seat, "  look  "));
        assert_eq!(seat.sent, vec!["look"]);
    }

    #[test]
    fn inventory_act_refuses_unoffered_acts() {
        let mut snap = InventorySnapshot::default();
        snap.items.push(Item {
            name: "a leather backpack".into(),
            count: 1,
            weight: 4,
            detail: Default::default(),
        });
        let mut p = page();
        let mut seat = FakeSeat::new(Some("Ima"));
        assert!(!p.inventory_act(&mut seat, &snap, "close", "a leather backpack"));
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Phone {
                kind: "inventory".to_string(),
                text: format!("{NOT_OFFERED}{REFRESH_INVENTORY}"),
            }
        );
        assert!(p.inventory_act(&mut seat, &snap, "look", "a leather backpack"));
        assert_eq!(seat.sent, vec!["look a leather backpack"]);
    }

    #[test]
    fn wheel_directions() {
        assert_eq!(wheel_command("up"), "n");
        assert_eq!(wheel_command("DOWN"), "s");
        assert_eq!(wheel_command("left"), "w");
        assert_eq!(wheel_command("right"), "e");
        assert_eq!(wheel_command("centre"), "look");
        assert_eq!(wheel_command(""), "");
        let mut p = page();
        let mut seat = FakeSeat::new(Some("Ima"));
        p.wheel(&mut seat, "up");
        assert_eq!(seat.sent, vec!["n"]);
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Wheel {
                wheel: "n".to_string()
            }
        );
    }

    #[test]
    fn phone_round_trip_through_the_page() {
        let root = std::env::temp_dir().join(format!("hydra-web-page-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut p = Page::new(&root, "ACCT", "Ima", "GS4");

        p.phone("ui");
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Phone {
                kind: "ui".to_string(),
                text: String::new()
            }
        );
        p.phone_put("ui", r#"{"tab":"inventory"}"#).unwrap();
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Phone {
                kind: "ui".to_string(),
                text: r#"{"tab":"inventory"}"#.to_string()
            }
        );
        assert!(p.phone_put("nope", "{}").is_err());
        assert_eq!(TrainerPanel::open(&root).folder, root.join("trainer"));
        assert!(TrainerPanel::open(&root).profiles.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn highlights_text_and_merge() {
        let root = std::env::temp_dir().join(format!("hydra-web-page-hl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut p = Page::new(&root, "ACCT", "Ima", "GS4");

        p.highlights_merge("Combat|kobold|#ff0000||yes\n").unwrap();
        assert!(matches!(
            p.outbox.last().unwrap(),
            Out::HighlightsText { .. }
        ));
        let err = p
            .highlights_merge(&"x".repeat(highlights::MAX_MERGE + 1))
            .unwrap_err();
        assert_eq!(err, TOO_BIG_TO_MERGE);
        assert_eq!(
            p.outbox.last().unwrap(),
            &Out::Refused {
                why: TOO_BIG_TO_MERGE.to_string()
            }
        );
        p.highlights_listed();
        assert!(matches!(p.outbox.last().unwrap(), Out::Highlights { .. }));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn frames_are_tungstenite_text_messages() {
        let out = Out::Said {
            text: "hi".into(),
        };
        let msg = frame(&out);
        assert!(msg.is_text());
        assert_eq!(msg.to_text().unwrap(), r#"{"t":"said","text":"hi"}"#);
        assert_eq!(msg.to_text().unwrap().as_bytes(), message(&out));

        let msg = tungstenite::Message::text(r#"{"t":"send","text":"look"}"#);
        assert_eq!(
            told(&msg).unwrap().unwrap(),
            In::Send {
                text: "look".into()
            }
        );
        // A frame that is not a page message, and a document that is not JSON.
        assert!(told(&tungstenite::Message::Ping(tungstenite::Bytes::new())).is_none());
        assert!(told(&tungstenite::Message::text("<xml/>"))
            .unwrap()
            .is_err());
    }

    #[tokio::test]
    async fn relay_pumps_frames_over_channels() {
        let (mut relay, mut link) = frames();
        link.incoming
            .send(r#"{"t":"send","text":"look"}"#.to_string())
            .unwrap();
        link.incoming.send("not a message".to_string()).unwrap();
        // Closing the page's side ends the loop.
        drop(link.incoming);

        let done = crate::client::relay(&mut relay, |msg| match msg {
            In::Send { text } => vec![Out::Said { text }],
            _ => Vec::new(),
        })
        .await;
        assert_eq!(
            done,
            Relayed {
                handled: 1,
                rejected: 1
            }
        );

        let reply = link.outgoing.try_recv().unwrap();
        assert_eq!(reply, r#"{"t":"said","text":"look"}"#);
        assert!(link.outgoing.try_recv().is_err(), "one frame per message");
    }

    #[tokio::test]
    async fn relay_runs_the_page_handlers() {
        let root = std::env::temp_dir().join(format!("hydra-web-relay-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let mut page = Page::new(&root, "ACCT", "Ima", "GS4");
        let mut seat = FakeSeat::new(Some("Ima"));
        let (mut relay, mut link) = frames();
        // A `send` is `Page::press`, a `highlights_merge` goes through the
        // highlight document, and `workspace` asks for the workspace answer.
        for msg in [
            r#"{"t":"send","text":"look"}"#,
            r#"{"t":"highlights_merge","text":"Combat|kobold|#ff0000||yes\n"}"#,
            r#"{"t":"workspace"}"#,
        ] {
            link.incoming.send(msg.to_string()).unwrap();
        }
        drop(link.incoming);
        let done = crate::client::relay(&mut relay, |msg| asked(&mut page, &mut seat, msg)).await;
        assert_eq!(
            done,
            Relayed {
                handled: 3,
                rejected: 0
            }
        );
        assert_eq!(seat.sent, vec!["look"]);

        let frames: Vec<String> = std::iter::from_fn(|| link.outgoing.try_recv().ok()).collect();
        assert_eq!(frames[0], r#"{"t":"said","text":"look"}"#);
        assert!(frames[1].starts_with(r#"{"t":"highlights_text","#), "{frames:?}");
        // `In::Workspace` carries no request id, so the answer's `ask` is 0.
        assert_eq!(
            frames[2],
            r#"{"t":"workspace","ask":0,"status":200}"#,
            "{frames:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole socket path: a blocking `tungstenite` client on a local TCP
    /// port, [`serve_socket`] bridging it onto the tokio relay.
    #[tokio::test]
    async fn relay_over_a_local_tcp_pair() {
        use tungstenite::client::IntoClientRequest;
        use tungstenite::Message;

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        let peer = std::thread::spawn(move || -> Result<String, String> {
            let stream = std::net::TcpStream::connect(addr).map_err(|e| e.to_string())?;
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .map_err(|e| e.to_string())?;
            let request = "ws://localhost/"
                .into_client_request()
                .map_err(|e| e.to_string())?;
            let (mut socket, response) =
                tungstenite::client(request, stream).map_err(|e| e.to_string())?;
            assert_eq!(response.status(), 101);
            socket
                .send(Message::text(r#"{"t":"send","text":"look"}"#))
                .map_err(|e| e.to_string())?;
            match socket.read().map_err(|e| e.to_string())? {
                Message::Text(text) => Ok(text.to_string()),
                other => Err(format!("{other:?}")),
            }
        });

        let (stream, _) = tokio::task::spawn_blocking(move || listener.accept())
            .await
            .unwrap()
            .unwrap();
        let (mut relay, bridge) = serve_socket(stream).unwrap();
        let session = tokio::spawn(async move {
            crate::client::relay(&mut relay, |msg| match msg {
                In::Send { text } => vec![Out::Said { text }],
                _ => Vec::new(),
            })
            .await
        });

        let reply = tokio::task::spawn_blocking(move || peer.join())
            .await
            .unwrap()
            .unwrap()
            .expect("the peer's reply");
        assert_eq!(reply, r#"{"t":"said","text":"look"}"#);
        let done = tokio::time::timeout(std::time::Duration::from_secs(10), session)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            done,
            Relayed {
                handled: 1,
                rejected: 0
            }
        );
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), bridge).await;
    }

    #[test]
    fn attach_sends_the_initial_state() {
        let mut p = page();
        p.attach(
            3,
            vec![wire::StreamOut {
                id: "main".into(),
                name: "Story".into(),
            }],
            vec![],
            vec![],
            None,
        );
        assert_eq!(p.outbox.len(), 2);
        match &p.outbox[0] {
            Out::Attached { name, game, id, .. } => {
                assert_eq!(name, "Ima");
                assert_eq!(game, "GS4");
                assert_eq!(*id, 3);
            }
            other => panic!("{other:?}"),
        }
        match &p.outbox[1] {
            Out::Injuries(doll) => assert_eq!(doll.parts.len(), injuries::PARTS),
            other => panic!("{other:?}"),
        }
    }
}
