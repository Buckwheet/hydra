//! The eframe entry point and the hub window.
//!
//! `hydra_gui::app::run` @`0x100512f78` is **partly** recovered. The ABI is
//! seven arguments plus an `sret` (`x8`) and a 72-byte return value, and the
//! body does, in order:
//!
//! | what | evidence |
//! |---|---|
//! | load the per-character placements from a `&Path` | `Placements::load(x3, x4)` @`0x100512fd0` |
//! | build the viewport for the placement named **`hub`** | `Placements::builder(.., "hub", 3, ..)` @`0x100512ff0`, literal at `0x10207ecc3` |
//! | title the window **`Hydra`** | a 5-byte `String` written byte by byte: `mov w8, #0x7948; movk w8, #0x7264, lsl #16; strb w8=0x61` @`0x100513014` |
//! | take the graphics folder | `hydra_board::own::graphics(&Path)` @`0x1005130a4` |
//! | `egui_wgpu::WgpuConfiguration::default()` | @`0x1005130ac` |
//! | `eframe::epi::NativeOptions::default()` | @`0x1005130b8` |
//! | box a 112-byte app value and call `eframe::run_native("Hydra", 5, &options, app, vtable, sret)` | @`0x1005132a4` |
//! | wait for the seat to close, then copy the `Result` out | `hydra_board::seat::Wake::closed` @`0x1005132ac` |
//!
//! Phase 4 splits that body at the `run_native` call, which is exactly where the
//! binary's control flow splits too:
//!
//! * [`plan`] returns everything the original had *before* the call, as a value
//!   ([`RunPlan`]) — the recovered constants, testable without a window;
//! * [`build`] builds the real [`eframe::NativeOptions`] and boxes the real
//!   app value ([`Hub`]) — pure value construction, no device, no window;
//! * [`run`] is the recovered call: `eframe::run_native(&title, options, app)`.
//!   Nothing in this crate's tests calls it.
//!
//! [`Hub::ui_contents`] is the recovered `<App as eframe::epi::App>::ui`
//! @`0x1004f79a0`: the hub's chrome — the title-bar strip
//! (`play::titlebar::Item`s as real `egui` buttons), the play area inside the
//! drawer zones, and the four drawers themselves.

use crate::badge;
use crate::keys::Keys;
use crate::layout::drawers::{Drawers, Mode, Zone};
use crate::live::LiveSession;
use crate::play::titlebar::{Item, TitleBar};
use crate::seam::{pos, rect_from_min_max, Rect, Vec2};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

/// The window title: `"Hydra"`, from the 5-byte `String` built at
/// `0x100513014` (`0x79 0x48` = `"Hy"`, `0x7264` = `"dr"`, `0x61` = `"a"`) and
/// passed to `eframe::run_native` as `(&str, len = 5)` @`0x1005132a0`.
pub const TITLE: &str = "Hydra";

/// The placement profile the window is built from: `"hub"` (3 bytes at
/// `0x10207ecc3`, length in `w2` @`0x100512fe4`).
pub const PLACEMENT: &str = "hub";

/// The window width: `mov x3, #0x44250000` @`0x100512fe8` = `660.0f32`.
pub const WIDTH: f32 = 660.0;

/// The window height: the `movk x3, #0x4402, lsl #48` half of the same packed
/// pair = `520.0f32`.
// UNKNOWN: which half of the packed register is the width; 660 wide is assumed
// (see `RunPlan::size`).
pub const HEIGHT: f32 = 520.0;

/// The renderer the original asked eframe for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Renderer {
    /// `egui_wgpu::WgpuConfiguration::default()` is built at `0x1005130ac`, and
    /// `eframe`'s default renderer in this build is the wgpu one
    /// (`Renderer::Glow` does not even exist: `egui_glow` is not linked).
    #[default]
    Wgpu,
    Glow,
}

/// What `app::run` hands to the window system.
#[derive(Clone, Debug, PartialEq)]
pub struct RunPlan {
    /// [`TITLE`].
    pub title: String,
    /// [`PLACEMENT`].
    pub placement: String,
    /// The size the placement builder was given.
    ///
    /// `mov x3, #0x44250000; movk x3, #0x4402, lsl #48` @`0x100512fe8` packs two
    /// `f32`s into one register: `0x44250000` = `660.0` and `0x44020000` =
    /// `520.0`.
    // UNKNOWN: which of the two is the width; `660 × 520` is assumed.
    pub size: Vec2,
    /// The graphics folder the app would display (`hydra_board::own::graphics`).
    pub graphics: std::path::PathBuf,
    /// The per-character data folder the placements were read from.
    pub placements_path: std::path::PathBuf,
    /// The fifth argument, `cmp w5, #0x0; cset w26, ne` @`0x100512fbc` — carried
    /// into the app value as a bool (`strb w26, [sp, #0x338]` @`0x10051324c`).
    pub logged_on: bool,
    pub renderer: Renderer,
    /// The app value is boxed with `_rust_alloc(0x70, 8)`; 112 bytes.
    ///
    /// Note: this is the *recovered* size of the original app value. The
    /// reconstruction's [`Hub`] is larger (it owns the model inline rather than
    /// through the `hydra_board` handles the original shared), so the constant
    /// records the original, not `size_of::<Hub>()`.
    pub app_size: usize,
}

impl RunPlan {
    /// The recovered title and placement, with everything else defaulted.
    pub fn new() -> RunPlan {
        RunPlan::default()
    }
}

impl Default for RunPlan {
    fn default() -> Self {
        RunPlan {
            title: TITLE.to_string(),
            placement: PLACEMENT.to_string(),
            size: Vec2::new(WIDTH, HEIGHT),
            graphics: std::path::PathBuf::new(),
            placements_path: std::path::PathBuf::new(),
            logged_on: false,
            renderer: Renderer::Wgpu,
            app_size: 0x70,
        }
    }
}

/// The inputs the original took by reference: a slice/array of shared handles
/// (`x0` points at eight pointers, `ldp q0, q1, [x19]` / `[x19, #0x20]` copy 64
/// bytes), a pair of strings (`x1`, `x2`), the data folder (`x3`, `x4`) and two
/// flags (`w5`, `x6`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Start {
    /// `x1`/`x2`: the character name pair stored at the front of the app value
    /// (`str x22, [sp, #0x2d0]; str x21, [sp, #0x2d8]` @`0x100513244`).
    pub character: String,
    /// `x3`/`x4`: the per-character folder (`Placements::load` and
    /// `Path::to_path_buf` are both called with it).
    pub data_dir: std::path::PathBuf,
    /// `w5`.
    pub logged_on: bool,
}

impl Start {
    pub fn new(character: impl Into<String>, data_dir: impl Into<std::path::PathBuf>) -> Start {
        Start {
            character: character.into(),
            data_dir: data_dir.into(),
            logged_on: false,
        }
    }
}

/// Everything `eframe::run_native` had been given once `app::run` reached the
/// call, as a value.
pub fn plan(start: &Start) -> RunPlan {
    RunPlan {
        title: TITLE.to_string(),
        placement: PLACEMENT.to_string(),
        graphics: graphics_dir(&start.data_dir),
        placements_path: start.data_dir.clone(),
        logged_on: start.logged_on,
        ..RunPlan::default()
    }
}

/// The former name of [`plan`]; kept so callers of the Phase-1 API keep working.
pub fn run_plan(start: &Start) -> RunPlan {
    plan(start)
}

/// `hydra_board::own::graphics` @`0x10147042c` — the graphics folder that lives
/// beside the character data.
///
/// The callee is in `hydra_board`, which is being reconstructed in parallel, so
/// the recovered *call shape* (`graphics(&data_dir)`) is kept and the join is
/// the documented stand-in.
pub fn graphics_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("graphics")
}

/// The room state the play area shows.
///
/// This crate has no `hydra_ui` dependency (the original reached the room
/// through the shared handles in that slice argument), so the model is plain
/// state the caller fills in. Nothing here is invented game data: every field
/// starts empty and is only drawn when it holds something.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    /// The room's name (`RoomTitle` in the title bar).
    pub title: String,
    /// The room's number (`RoomNumber` in the title bar), as written.
    pub number: String,
    /// The room description, one entry per line.
    pub lines: Vec<String>,
    /// The command prompt line.
    pub prompt: String,
}

/// The app value the original boxed (112 bytes) and eframe calls `ui` on.
///
/// Recovered layout: the two `String`s at the front (`0x2d0`/`0x2d8`), the
/// logged-on bool at `0x338` (`strb w26` @`0x10051324c`). The reconstruction
/// owns the model inline instead of the `hydra_board` handles.
pub struct Hub {
    /// What `app::run` was called with.
    pub start: Start,
    /// The flag the original carried into the app value at `0x338`.
    pub logged_on: bool,
    /// The title bar items (`play::titlebar`).
    pub title_bar: TitleBar,
    /// The four drawers (`layout::drawers`).
    pub drawers: Drawers,
    /// The key bindings (`keys::Keys::take` consumes this frame's events).
    pub keys: Keys,
    /// The room text the play area shows when there is no live session.
    pub room: Room,
    /// The live session (`hydra gui <character>`): `None` for the hub window,
    /// which is `hydra gui` with no character and draws the empty room model.
    ///
    /// The model is behind a lock because a **reader thread** owns the game
    /// socket and folds lines into it while the window paints.  Nothing here
    /// ever holds that lock across a socket operation: see [`Hub::live_ui`].
    pub live: Option<Arc<Mutex<LiveSession>>>,
    /// Where a typed line goes: the writer thread's channel
    /// (`hydra::gui`'s writer, which owns the game socket).  `None` for the hub
    /// window.
    pub input: Option<Sender<String>>,
}

impl Hub {
    /// The height of the title-bar strip inside the play area.
    // UNKNOWN: no literal for the strip height was recovered next to
    // `TitleBar::set`/`page`; 24.0 is a "one 13 px button row plus padding"
    // choice and is only used to split the play area, never to size a widget.
    pub const TITLE_BAR_HEIGHT: f32 = 24.0;

    /// The padding the play area's text is laid out with.
    // UNKNOWN: likewise unrecovered; 8.0 is used.
    pub const PAD: f32 = 8.0;

    /// The height of the live status strip (the character and the connection
    /// indicator) at the top of the play area.
    // UNKNOWN: a drawing decision; 18.0 is one body-text row.
    pub const STATUS_HEIGHT: f32 = 18.0;

    /// The height of the live input row at the bottom of the play area.
    // UNKNOWN: a drawing decision; 24.0 fits one `TextEdit` and one button.
    pub const INPUT_HEIGHT: f32 = 24.0;

    /// The share of the play area's width the story gets when a session is
    /// live; the room/vitals/hands/compass column gets the rest.
    // UNKNOWN: the binary's hub splits are in `layout::drawers` (the four
    // drawers); this two-column split is inferred for the live play area.
    pub const STORY_FRACTION: f32 = 0.6;

    /// The height of the vitals rings below their words.
    // UNKNOWN: a drawing decision; the stack's geometry is in
    // `bar::stack::Stack::square` and only needs a non-empty box.
    pub const VITALS_HEIGHT: f32 = 64.0;

    /// The id the live input line is given, so a test (or a future "focus the
    /// input" shortcut) can name it without a click.
    pub const INPUT_ID: &'static str = "hydra-live-input";

    pub fn new(start: Start, logged_on: bool) -> Hub {
        Hub {
            start,
            logged_on,
            // `TitleBar as Default` @`0x1003ccb64`: the first-paint entries.
            title_bar: TitleBar::default(),
            // `Drawers::default()` — the recovered 160/160/240/240 table.
            drawers: Drawers::default(),
            keys: Keys::with_defaults(),
            room: Room::default(),
            live: None,
            input: None,
        }
    }

    /// A hub around a **live** session: the window `hydra gui <character>`
    /// opens.
    ///
    /// `live` is shared with the reader thread that folds the game's lines into
    /// it, and `input` is the channel to the writer thread that owns the game
    /// socket.  Everything else is [`Hub::new`]'s state, so the chrome is the
    /// same window either way — only the play area's contents differ
    /// ([`Hub::live_ui`] instead of [`Hub::room_ui`]).
    pub fn with_live(
        start: Start,
        logged_on: bool,
        live: Arc<Mutex<LiveSession>>,
        input: Sender<String>,
    ) -> Hub {
        let mut hub = Hub::new(start, logged_on);
        hub.live = Some(live);
        hub.input = Some(input);
        hub
    }

    /// The rects the hub lays out inside `window`.
    ///
    /// Everything comes out of the recovered [`crate::layout::drawers::Zones`]
    /// maths (`Zones::of` @`0x1007cef40`: the 40.0 floor, the 60/144 reserves
    /// and the proportional scaling), so the drawer strips and the main area are
    /// the recovered ones; the title-bar strip and the play area are the two
    /// pieces inside the main area.
    pub fn chrome(&self, window: Rect) -> Chrome {
        Chrome::of(&self.drawers, window)
    }

    /// Apply a clicked title-bar item's effect.
    ///
    /// The four drawer icons toggle their drawer (`Item::TopDrawer` …
    /// `Item::RightDrawer`), which is the state transition the recovered
    /// `TitleBar::set` @`0x1003da938` performs before repainting. Every other
    /// item is left to the caller (menus, zoom, lock, arrange are not
    /// reconstructed).
    pub fn press(&mut self, item: &Item) -> bool {
        match item {
            Item::TopDrawer => self.drawers.top.open = !self.drawers.top.open,
            Item::BottomDrawer => self.drawers.bottom.open = !self.drawers.bottom.open,
            Item::LeftDrawer => self.drawers.left.open = !self.drawers.left.open,
            Item::RightDrawer => self.drawers.right.open = !self.drawers.right.open,
            _ => return false,
        }
        true
    }

    /// `<App as eframe::epi::App>::ui` @`0x1004f79a0` — paint the hub.
    pub fn ui_contents(&mut self, ui: &mut egui::Ui) {
        let window = ui.max_rect();
        let chrome = self.chrome(window);
        let visuals = ui.visuals().clone();

        // 1. The drawers that take room, behind everything: `Drawn` and
        //    `Pushing` are part of the window layout (`Zones::of` subtracts
        //    them), so they are painted first.
        self.drawers.paint(
            ui.painter(),
            window,
            visuals.panel_fill,
            &[Mode::Drawn, Mode::Pushing],
        );

        // 2. The play area, on the theme's background.
        if chrome.play.width() > 0.0 && chrome.play.height() > 0.0 {
            ui.painter()
                .rect_filled(chrome.play, egui::CornerRadius::ZERO, visuals.extreme_bg_color);
            if self.live.is_some() {
                self.live_ui(ui, chrome.play);
                // A background reader thread is what makes new game lines
                // arrive, and nothing in the window changes when it does: ask
                // for a repaint so the story keeps up.  This is a repaint hint,
                // never a poll of the socket (that is the reader thread's job).
                ui.ctx().request_repaint_after(crate::live::REPAINT);
            } else {
                self.room_ui(ui, chrome.play, &visuals);
            }
        }

        // 3. The title-bar strip: real widgets, one per recovered `Item`.
        if chrome.title_bar.width() > 0.0 && chrome.title_bar.height() > 0.0 {
            ui.painter().rect_filled(
                chrome.title_bar,
                egui::CornerRadius::ZERO,
                visuals.widgets.inactive.bg_fill,
            );
            let responses = ui
                .scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(chrome.title_bar)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    |ui| self.title_bar.ui(ui),
                )
                .inner;
            for (item, response) in responses {
                if response.clicked() {
                    self.press(&item);
                }
            }
        }

        // 4. `Clipping` drawers lie *over* the play area, so they go last.
        self.drawers.paint(
            ui.painter(),
            window,
            visuals.panel_fill,
            &[Mode::Clipping],
        );
    }

    /// The room text and prompt, painted into `area` (clipped to it, so a long
    /// room description never spills over a drawer).
    fn room_ui(&self, ui: &mut egui::Ui, area: Rect, visuals: &egui::Visuals) {
        let painter = ui.painter().with_clip_rect(area);
        let heading = egui::TextStyle::Heading.resolve(ui.style());
        let body = egui::TextStyle::Body.resolve(ui.style());
        let mut y = area.min.y + Hub::PAD;
        if !self.room.title.is_empty() {
            let used = painter.text(
                pos(Vec2::new(area.min.x + Hub::PAD, y)),
                egui::Align2::LEFT_TOP,
                &self.room.title,
                heading,
                visuals.strong_text_color(),
            );
            y = used.max.y;
        }
        if !self.room.number.is_empty() {
            let used = painter.text(
                pos(Vec2::new(area.min.x + Hub::PAD, y)),
                egui::Align2::LEFT_TOP,
                format!("({})", self.room.number),
                body.clone(),
                visuals.weak_text_color(),
            );
            y = used.max.y;
        }
        if !self.room.lines.is_empty() {
            y += Hub::PAD;
            for line in &self.room.lines {
                let used = painter.text(
                    pos(Vec2::new(area.min.x + Hub::PAD, y)),
                    egui::Align2::LEFT_TOP,
                    line,
                    body.clone(),
                    visuals.text_color(),
                );
                y = used.max.y;
            }
        }
        if !self.room.prompt.is_empty() {
            painter.text(
                pos(Vec2::new(area.min.x + Hub::PAD, area.max.y - Hub::PAD)),
                egui::Align2::LEFT_BOTTOM,
                &self.room.prompt,
                body,
                visuals.text_color(),
            );
        }
    }

    /// The live play area: story, room/vitals/hands/compass, status and input.
    ///
    /// **INFERRED layout.** The recovered chrome (`Chrome`) is untouched — the
    /// title bar and the four drawers are the recovered ones — and this only
    /// decides what goes inside the play rectangle, which the binary's hub draws
    /// with the `hydra_board` panels that are not reconstructed.  Four bands:
    ///
    /// ```text
    /// +----------------------------------------------+
    /// | Aelith — connected                           |  STATUS_HEIGHT
    /// +-------------------------------+--------------+
    /// | story: the scrollback, the    | room title   |  the rest
    /// | prompt as its last line       | description  |
    /// |                               | exits        |
    /// |                               | Compass: …   |
    /// |                               | Left/Right   |
    /// |                               | Sp … HP …    |
    /// |                               | (rings)      |
    /// +-------------------------------+--------------+
    /// | [ type a command……………… ] [Send]             |  INPUT_HEIGHT
    /// +----------------------------------------------+
    /// ```
    ///
    /// The story gets [`Hub::STORY_FRACTION`] of the width and the side column
    /// the rest.  The prompt is painted once, as the story's last line (that is
    /// where the recovered `Story::paint_into` puts it), so the room pane is
    /// drawn from a copy of the view with its prompt cleared.
    ///
    /// **The model lock is never held across a socket operation.** The typed
    /// line is taken out of the model, validated, and only then handed to the
    /// writer thread's channel — after the lock has been dropped.  The lock is
    /// also never held twice: `std::sync::Mutex` is not reentrant, so each band
    /// takes its own short lock.
    fn live_ui(&mut self, ui: &mut egui::Ui, area: Rect) {
        let Some(model) = self.live.clone() else {
            // No session: the empty room model is what the hub draws, and
            // `ui_contents` never calls this then.
            return;
        };
        let sender = self.input.clone();
        let visuals = ui.visuals().clone();
        let body = egui::TextStyle::Body.resolve(ui.style());

        // The three bands.  A play area too short for them still gets the story.
        let status_height = Hub::STATUS_HEIGHT.min(area.height() * 0.5);
        let input_height = Hub::INPUT_HEIGHT.min((area.height() - status_height) * 0.5);
        let status_rect = rect_from_min_max(
            area.min.to_vec2(),
            Vec2::new(area.max.x, area.min.y + status_height),
        );
        let input_rect = rect_from_min_max(
            Vec2::new(area.min.x, area.max.y - input_height),
            area.max.to_vec2(),
        );
        let body_rect = rect_from_min_max(
            Vec2::new(area.min.x, status_rect.max.y),
            Vec2::new(area.max.x, input_rect.min.y),
        );
        if body_rect.width() <= 0.0 || body_rect.height() <= 0.0 {
            return;
        }

        // 1. The status strip: who this window is for, and whether the game is
        //    still connected.
        {
            let live = locked(&model);
            let colour = if live.connected {
                visuals.strong_text_color()
            } else {
                visuals.weak_text_color()
            };
            ui.painter().with_clip_rect(status_rect).text(
                pos(Vec2::new(status_rect.min.x + Hub::PAD, status_rect.center().y)),
                egui::Align2::LEFT_CENTER,
                format!("{} \u{2014} {}", live.character, live.status()),
                body.clone(),
                colour,
            );
        }

        // 2. The input row: a real single-line `TextEdit` and a Send button.
        //    Enter is the recovered `TextEdit` pattern — a single-line edit
        //    loses focus when Enter is pressed.
        let mut buffer = {
            let mut live = locked(&model);
            std::mem::take(&mut live.input)
        };
        let connected = locked(&model).connected;
        let mut submitted = false;
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(input_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                let edit = ui.add_enabled(
                    connected,
                    egui::TextEdit::singleline(&mut buffer)
                        .id(egui::Id::new(Hub::INPUT_ID))
                        .hint_text("type a command"),
                );
                if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submitted = true;
                    // A single-line edit gives focus up on Enter; the player
                    // needs it back to type the next command, so ask for it
                    // again in the same frame (the request is honoured at the
                    // start of the next one).
                    edit.request_focus();
                }
                if ui
                    .add_enabled(connected, egui::Button::new("Send"))
                    .clicked()
                {
                    submitted = true;
                }
            },
        );
        locked(&model).input = buffer;
        if submitted {
            let line = locked(&model).submit();
            if let Some(line) = line {
                if let Some(sender) = &sender {
                    // The lock is gone by now: the writer thread owns the socket.
                    let _ = sender.send(line);
                }
            }
        }

        // 3. The story: the scrollback with the prompt as its last line.
        let split = body_rect.width() * Hub::STORY_FRACTION;
        let story_rect = rect_from_min_max(
            body_rect.min.to_vec2(),
            Vec2::new(body_rect.min.x + split, body_rect.max.y),
        );
        let side_rect = rect_from_min_max(
            Vec2::new(story_rect.max.x, body_rect.min.y),
            body_rect.max.to_vec2(),
        );
        if story_rect.width() > 0.0 {
            let live = locked(&model);
            let painter = ui.painter().with_clip_rect(story_rect);
            let ink = hydra_ui::story::Ink::of(&live.theme, ui).prompt(&live.view.prompt);
            live.story.paint_into(&painter, story_rect, &ink);
        }

        // 4. The side column: the room, the compass, the hands and the vitals,
        //    drawn with the real `hydra_ui`/`bar` widgets.
        if side_rect.width() > 0.0 {
            let live = locked(&model);
            ui.scope_builder(
                egui::UiBuilder::new()
                    .max_rect(side_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
                |ui| {
                    ui.set_clip_rect(side_rect);
                    // The prompt is the story's last line; the pane skips it so
                    // it is not said twice.
                    let mut view = live.view.clone();
                    view.prompt.clear();
                    view.ui_with(ui, &live.theme);
                    if !live.room_number.is_empty() {
                        ui.label(format!("({})", live.room_number));
                    }
                    if !live.compass.is_empty() {
                        ui.separator();
                        ui.label(format!("Compass: {}", live.compass.join(", ")));
                    }
                    if live.hands.left().is_some() || live.hands.right().is_some() {
                        ui.separator();
                        if let Some(item) = live.hands.left() {
                            ui.label(format!("Left hand: {item}"));
                        }
                        if let Some(item) = live.hands.right() {
                            ui.label(format!("Right hand: {item}"));
                        }
                    }
                    ui.separator();
                    ui.label(live.vitals.words());
                    let (rings, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), Hub::VITALS_HEIGHT),
                        egui::Sense::hover(),
                    );
                    live.vitals.paint(
                        &ui.painter().with_clip_rect(rings),
                        rings,
                        ui.visuals().strong_text_color(),
                    );
                },
            );
        }
    }
}

/// Lock a live session, surviving a poisoned lock.
///
/// A panicking reader thread must not turn every later frame into a second
/// panic: the model is still the best one available.
fn locked(model: &Arc<Mutex<LiveSession>>) -> MutexGuard<'_, LiveSession> {
    model
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl eframe::App for Hub {
    /// This rev's trait method is `ui`, not `update` (`eframe::epi::App::ui`,
    /// the app's impl at `0x1004f79a0`).
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.ui_contents(ui);
    }
}

/// The rects a hub lays out in a window.
///
/// The four drawer strips are the recovered zones; [`Chrome::main`] is
/// `Zones::main_rect`, and the strip splits into [`Chrome::title_bar`] and
/// [`Chrome::play`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chrome {
    /// The whole window.
    pub window: Rect,
    /// The top drawer strip (`Drawers::default().top.size` = 160.0).
    pub top: Rect,
    /// The bottom drawer strip (160.0).
    pub bottom: Rect,
    /// The left drawer strip (240.0).
    pub left: Rect,
    /// The right drawer strip (240.0).
    pub right: Rect,
    /// The area the drawers leave: `Zones::main_rect` offset by the window.
    pub main: Rect,
    /// The title-bar strip at the top of [`Chrome::main`].
    pub title_bar: Rect,
    /// The play area below the title bar.
    pub play: Rect,
}

impl Chrome {
    pub fn of(drawers: &Drawers, window: Rect) -> Chrome {
        use crate::layout::drawers::Zones;
        let zones = Zones::of(drawers, window.size());
        let main = zones.main_rect_in(window);
        let strip = Hub::TITLE_BAR_HEIGHT.min(main.height());
        let title_bar = rect_from_min_max(
            main.min.to_vec2(),
            Vec2::new(main.max.x, main.min.y + strip),
        );
        let play = rect_from_min_max(
            Vec2::new(main.min.x, title_bar.max.y),
            main.max.to_vec2(),
        );
        Chrome {
            window,
            top: zones.strip(Zone::Top, window),
            bottom: zones.strip(Zone::Bottom, window),
            left: zones.strip(Zone::Left, window),
            right: zones.strip(Zone::Right, window),
            main,
            title_bar,
            play,
        }
    }

    /// Which of the four strips are open, in zone order.
    pub fn open(&self) -> Vec<(Zone, Rect)> {
        [
            (Zone::Top, self.top),
            (Zone::Bottom, self.bottom),
            (Zone::Left, self.left),
            (Zone::Right, self.right),
        ]
        .into_iter()
        .filter(|(_, rect)| rect.width() > 0.0 && rect.height() > 0.0)
        .collect()
    }
}

/// Everything `eframe::run_native` needs, as a value.
///
/// Building this touches no window and no GPU device: it is structs and the
/// badge bytes only, which is what lets the tests assert the recovered
/// options without ever starting an event loop.
pub struct Launch {
    pub title: String,
    pub options: eframe::NativeOptions,
    pub app: Box<dyn eframe::App>,
}

impl Launch {
    /// The viewport size the launch was built with, as `(width, height)`.
    ///
    /// `NativeOptions::viewport.inner_size` is the recovered 660 × 520 pair
    /// (`app::RunPlan::size`); this accessor keeps a caller from having to name
    /// an `egui` type to read it.
    pub fn size(&self) -> (f32, f32) {
        match self.options.viewport.inner_size {
            Some(size) => (size.x, size.y),
            None => (0.0, 0.0),
        }
    }

    /// Hand this launch to eframe's native event loop.
    ///
    /// This is one of only two functions in the crate (with [`run`]) that start
    /// the platform event loop, i.e. the only ones that open a window.
    pub fn run(self) -> eframe::Result {
        eframe::run_native(&self.title, self.options, Box::new(move |_cc| Ok(self.app)))
    }
}

/// Build the recovered `eframe` launch value: [`RunPlan`] → `NativeOptions` +
/// the boxed [`Hub`].
pub fn build(start: &Start, logged_on: bool) -> Launch {
    let plan = plan(start);
    // `badge::pixels(false, _)` returns the 256 × 256 RGBA rows; the two
    // recovered call sites copy the width into both `IconData` fields
    // (`stp w22, w22, [x0, #0x28]` @`0x100546d3c`), so the icon is square.
    let (rgba, side) = badge::pixels(false, badge::SIDE);
    let icon = egui::IconData {
        rgba,
        width: side,
        height: side,
    };
    let mut options = eframe::NativeOptions::default();
    // `WgpuConfiguration::default()` @`0x1005130ac`, then eframe's wgpu
    // renderer (the only one this build has).
    options.renderer = eframe::Renderer::Wgpu;
    options.viewport = options
        .viewport
        .clone()
        .with_title(plan.title.clone())
        .with_inner_size([plan.size.x, plan.size.y])
        .with_icon(icon);
    options.wgpu_options = egui_wgpu::WgpuConfiguration::default();
    Launch {
        title: plan.title,
        options,
        app: Box::new(Hub::new(start.clone(), logged_on)),
    }
}

/// The recovered call: `eframe::run_native("Hydra", options, app)`.
///
/// This starts the platform event loop and opens the window, so nothing in this
/// crate's test suite calls it.
pub fn run(start: &Start, logged_on: bool) -> eframe::Result {
    build(start, logged_on).run()
}

/// [`build`], but the boxed app is a [`Hub`] around a **live** session.
///
/// Same recovered window (title, placement, size, icon, wgpu renderer); only
/// the play area's contents differ.  `live` is shared with the reader thread and
/// `input` is the writer thread's channel — see [`Hub::with_live`] and
/// `hydra::gui`, which owns both threads.  This is still a pure value: no
/// device, no event loop, no socket.
pub fn build_live(
    start: &Start,
    logged_on: bool,
    live: Arc<Mutex<LiveSession>>,
    input: Sender<String>,
) -> Launch {
    let mut launch = build(start, logged_on);
    launch.app = Box::new(Hub::with_live(start.clone(), logged_on, live, input));
    launch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless;
    use crate::seam::rect;

    fn start() -> Start {
        Start::new("Aelith", "/tmp/hydra")
    }

    #[test]
    fn the_window_constants_are_the_recovered_ones() {
        assert_eq!(TITLE, "Hydra");
        assert_eq!(TITLE.len(), 5);
        assert_eq!(PLACEMENT, "hub");
        assert_eq!(PLACEMENT.len(), 3);
        assert_eq!(WIDTH, f32::from_bits(0x4425_0000));
        assert_eq!(HEIGHT, f32::from_bits(0x4402_0000));
    }

    #[test]
    fn default_size_is_the_packed_pair() {
        let plan = RunPlan::default();
        // 0x44250000 = 660.0, 0x44020000 = 520.0.
        assert_eq!(f32::from_bits(0x4425_0000), 660.0);
        assert_eq!(f32::from_bits(0x4402_0000), 520.0);
        assert_eq!(plan.size, Vec2::new(660.0, 520.0));
        assert_eq!(plan.renderer, Renderer::Wgpu);
        assert_eq!(plan.app_size, 0x70);
    }

    #[test]
    fn plan_carries_the_start_into_the_run_plan() {
        let start = Start {
            character: "Aelish".into(),
            data_dir: "/tmp/hydra".into(),
            logged_on: true,
        };
        let plan = plan(&start);
        assert_eq!(plan.title, "Hydra");
        assert_eq!(plan.placement, "hub");
        assert_eq!(plan.placements_path, std::path::PathBuf::from("/tmp/hydra"));
        assert_eq!(plan.graphics, std::path::PathBuf::from("/tmp/hydra/graphics"));
        assert!(plan.logged_on);
        // The Phase-1 name still works and returns the same plan.
        assert_eq!(run_plan(&start), plan);
    }

    #[test]
    fn build_hands_eframe_the_recovered_options() {
        let launch = build(&start(), false);
        assert_eq!(launch.title, "Hydra");
        // `egui_wgpu::WgpuConfiguration::default()` @0x1005130ac.
        assert_eq!(launch.options.renderer, eframe::Renderer::Wgpu);
        assert_eq!(
            launch.options.viewport.inner_size,
            Some(Vec2::new(660.0, 520.0))
        );
        assert_eq!(launch.options.viewport.title.as_deref(), Some("Hydra"));
        let icon = launch
            .options
            .viewport
            .icon
            .as_ref()
            .expect("the badge is the window icon");
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        // `badge::pixels` returns the pre-decoded asset; the icon is its bytes.
        assert_eq!(icon.rgba, badge::decode().0);
    }

    #[test]
    fn build_boxes_a_hub_with_the_recovered_state() {
        let launch = build(&start(), true);
        // The app value is a `Hub`; `eframe::Frame` has no public constructor at
        // this rev, so the frame is driven through `ui_contents` — which is
        // exactly what `Hub::ui` forwards to.
        assert_eq!(launch.app.auto_save_interval().as_secs(), 30);
        let mut hub = Hub::new(start(), true);
        hub.room.title = "The Hub".into();
        let (_, output) = headless::one_frame(|ui| hub.ui_contents(ui));
        assert!(!headless::shapes(&output).is_empty());
    }

    #[test]
    fn chrome_splits_the_window_into_the_recovered_zones() {
        let hub = Hub::new(start(), false);
        let window = rect(Vec2::ZERO, Vec2::new(660.0, 520.0));
        let chrome = hub.chrome(window);
        // Shut drawers: the whole window is main, and the title bar is 24 high.
        assert_eq!(chrome.main, window);
        assert_eq!(
            chrome.title_bar,
            rect_from_min_max(Vec2::new(0.0, 0.0), Vec2::new(660.0, 24.0))
        );
        assert_eq!(
            chrome.play,
            rect_from_min_max(Vec2::new(0.0, 24.0), Vec2::new(660.0, 520.0))
        );
        assert!(chrome.open().is_empty());
    }

    #[test]
    fn chrome_follows_the_drawer_extents() {
        let mut hub = Hub::new(start(), false);
        hub.drawers.top.open = true;
        hub.drawers.left.open = true;
        let window = rect(Vec2::ZERO, Vec2::new(660.0, 520.0));
        let chrome = hub.chrome(window);
        // Top 160, left 240 (the recovered defaults).
        assert_eq!(chrome.top, rect(Vec2::ZERO, Vec2::new(660.0, 160.0)));
        assert_eq!(chrome.left, rect(Vec2::ZERO, Vec2::new(240.0, 520.0)));
        assert_eq!(
            chrome.main,
            rect_from_min_max(Vec2::new(240.0, 160.0), Vec2::new(660.0, 520.0))
        );
        assert_eq!(
            chrome.title_bar,
            rect_from_min_max(Vec2::new(240.0, 160.0), Vec2::new(660.0, 184.0))
        );
        assert_eq!(
            chrome.play,
            rect_from_min_max(Vec2::new(240.0, 184.0), Vec2::new(660.0, 520.0))
        );
        assert_eq!(chrome.open().len(), 2);
    }

    #[test]
    fn the_hub_paints_the_play_area_and_the_title_bar_items() {
        let mut hub = Hub::new(start(), false);
        hub.room.title = "The Hub".into();
        hub.room.lines = vec!["A quiet crossing of roads.".into()];
        hub.room.prompt = "> ".into();
        let mut seen = None;
        let (_, out) = headless::frame(&egui::Context::default(), headless::SIZE, |ui| {
            hub.ui_contents(ui);
            seen = Some(ui.visuals().clone());
        });
        let visuals = seen.unwrap();
        // The play area is filled with the theme's window background…
        let play = hub.chrome(rect(Vec2::ZERO, headless::SIZE)).play;
        assert_eq!(
            headless::rects_of(&out, visuals.extreme_bg_color),
            vec![play]
        );
        // …and the title-bar strip with the button background, then one such
        // rect per button (the four recovered items).
        let strip = hub.chrome(rect(Vec2::ZERO, headless::SIZE)).title_bar;
        let buttons = headless::rects_of(&out, visuals.widgets.inactive.bg_fill);
        assert_eq!(buttons.len(), 5, "the strip then four buttons");
        assert_eq!(buttons[0], strip);
        for button in &buttons[1..] {
            assert!(strip.contains_rect(*button), "{button:?} is in the strip");
        }
        // The four recovered items are drawn as buttons, then the room text.
        let texts = headless::texts(&out);
        for label in ["Character", "Connection", "Menu", "Zoom \u{2212} and +"] {
            assert!(
                texts.iter().any(|text| text == label),
                "{label} is in the title bar: {texts:?}"
            );
        }
        assert!(texts.iter().any(|text| text == "The Hub"));
        assert!(texts
            .iter()
            .any(|text| text == "A quiet crossing of roads."));
        assert!(texts.iter().any(|text| text == "> "));
    }

    #[test]
    fn an_open_drawer_is_painted_in_its_strip() {
        let mut hub = Hub::new(start(), false);
        hub.drawers.top.open = true;
        hub.drawers.bottom.open = true;
        hub.drawers.left.open = true;
        hub.drawers.right.open = true;
        let mut seen = None;
        let (_, out) = headless::frame(&egui::Context::default(), headless::SIZE, |ui| {
            hub.ui_contents(ui);
            seen = Some(ui.visuals().panel_fill);
        });
        let fill = seen.unwrap();
        let window = rect(Vec2::ZERO, headless::SIZE);
        let chrome = hub.chrome(window);
        // 660 × 520: the 160-high strips fit, the 240-wide ones fit under the
        // recovered 144 reserve, so nothing is scaled.
        assert_eq!(
            headless::rects_of(&out, fill),
            vec![chrome.top, chrome.bottom, chrome.left, chrome.right]
        );
        assert_eq!(chrome.top.height(), 160.0);
        assert_eq!(chrome.left.width(), 240.0);
        // The play area sits inside what the drawers left.
        assert_eq!(chrome.play.min, egui::Pos2::new(240.0, 160.0 + 24.0));
    }

    #[test]
    fn a_clipping_drawer_is_painted_over_the_play_area() {
        let mut hub = Hub::new(start(), false);
        hub.drawers.top.open = true;
        hub.drawers.top.mode = Mode::Clipping;
        let window = rect(Vec2::ZERO, headless::SIZE);
        let (_, out) = headless::frame(&egui::Context::default(), headless::SIZE, |ui| {
            hub.ui_contents(ui)
        });
        let chrome = hub.chrome(window);
        // Painted last, so it covers the play area rather than sitting under it.
        let fills = headless::rect_fills(&out);
        assert_eq!(fills.last().map(|rect| rect.rect), Some(chrome.top));
        // Without the clipping mode the same drawer is painted before the play
        // area instead.
        hub.drawers.top.mode = Mode::Drawn;
        let (_, out) = headless::frame(&egui::Context::default(), headless::SIZE, |ui| {
            hub.ui_contents(ui)
        });
        let fills = headless::rect_fills(&out);
        assert_eq!(fills[0].rect, chrome.top);
    }

    #[test]
    fn pressing_a_drawer_item_toggles_that_drawer() {
        let mut hub = Hub::new(start(), false);
        assert!(!hub.drawers.top.open);
        assert!(hub.press(&Item::TopDrawer));
        assert!(hub.drawers.top.open);
        assert!(hub.press(&Item::TopDrawer));
        assert!(!hub.drawers.top.open);
        assert!(!hub.press(&Item::Lock));
        assert!(hub.press(&Item::LeftDrawer));
        assert!(hub.drawers.left.open);
        assert!(hub.press(&Item::RightDrawer));
        assert!(hub.drawers.right.open);
        assert!(hub.press(&Item::BottomDrawer));
        assert!(hub.drawers.bottom.open);
    }

    // ----------------------------------------------------------------------
    // Phase 9: the live play area.
    // ----------------------------------------------------------------------

    /// The synthetic XML stream the live tests play.
    ///
    /// Every value is invented for the test; no captured game text, name or key
    /// appears here.  It carries each tag the live model reads — the
    /// `component`/`nav` attributes, the `preset` room form, a plain story line,
    /// the four `progressBar`s, both hands, a `compass` and a `prompt` — so one
    /// frame proves the whole tag mapping reaches the window.
    const LIVE_STREAM: &str = concat!(
        "<settings/>\r\n",
        "<component id=\"roomName\" value=\"A Synthetic Crossing\"/>\r\n",
        "<component id=\"roomDesc\" value=\"A wide synthetic road.&#10;Obvious exits: north, southeast.\"/>\r\n",
        "<nav rm=\"4242\"/>\r\n",
        "<preset id=\"roomName\">A Synthetic Crossing</preset>\r\n",
        "<preset id=\"roomDesc\">A wide synthetic road.</preset>\r\n",
        "<preset id=\"roomDesc\">Obvious exits: north, southeast.</preset>\r\n",
        "A synthetic breeze crosses the road.\r\n",
        "<progressBar id=\"health\" value=\"80\"/>\r\n",
        "<progressBar id=\"mana\" value=\"90\"/>\r\n",
        "<progressBar id=\"spirit\" value=\"70\"/>\r\n",
        "<progressBar id=\"stamina\" value=\"60\"/>\r\n",
        "<left>a synthetic shield</left>\r\n",
        "<right>a synthetic sword</right>\r\n",
        "<compass><dir value=\"n\"/><dir value=\"se\"/></compass>\r\n",
        "<prompt>synthetic&gt;</prompt>\r\n",
    );

    /// Play [`LIVE_STREAM`] into a fresh [`LiveSession`] over a **real**
    /// loopback socket: `MockGame` on `127.0.0.1:0` → `TcpStream` → the
    /// two-line handshake → the parser → the model.
    ///
    /// No window, no login and no real host: the mock's stream is written
    /// verbatim and the mock closes, so the read ends at EOF.
    fn played_session() -> LiveSession {
        use std::io::{BufRead, BufReader, Write};

        let game = hydra_mocks::MockGame::start(LIVE_STREAM).expect("the mock game server starts");
        let mut socket =
            std::net::TcpStream::connect(game.addr()).expect("the loopback game socket opens");
        // The two handshake lines the game socket takes before any XML: the
        // launch KEY, then the front-end identification (docs/LICH_WIRE.md §2).
        let handshake = format!("synthetic-key\n{}\n", hydra_mocks::CLIENT_IDENTIFICATION);
        socket
            .write_all(handshake.as_bytes())
            .expect("the handshake is written");
        socket.flush().expect("the handshake is flushed");

        let mut session = LiveSession::new("Aelith");
        let mut reader = BufReader::new(socket);
        let mut line = String::new();
        while reader.read_line(&mut line).expect("the game stream reads") > 0 {
            // `read_line` keeps the `\n`; the model strips it (and a `\r`).
            session.feed_line(&line);
            line.clear();
        }
        // The mock really saw the two handshake lines.
        assert_eq!(
            game.received(),
            vec![
                "synthetic-key".to_string(),
                hydra_mocks::CLIENT_IDENTIFICATION.to_string()
            ]
        );
        game.join().expect("the mock game server ends cleanly");
        session
    }

    #[test]
    fn a_live_session_is_painted_in_the_play_area() {
        let session = played_session();
        // The model itself first, so a painting failure is not mistaken for a
        // parsing one.
        assert_eq!(session.view.room, "A Synthetic Crossing");
        assert_eq!(session.view.exits, vec!["n".to_string(), "se".to_string()]);
        assert_eq!(session.vitals.words(), "Sp 70%, HP 80%, MP 90%, SP 60%");
        assert_eq!(session.hands.left(), Some("a synthetic shield"));
        assert_eq!(session.hands.right(), Some("a synthetic sword"));
        assert_eq!(session.compass, vec!["n".to_string(), "se".to_string()]);
        assert_eq!(session.view.prompt, "synthetic>");

        let live = Arc::new(Mutex::new(session));
        // The text the player has typed but not sent: it must be in the frame.
        locked(&live).input = "inventory".to_string();
        let (input, inbox) = std::sync::mpsc::channel::<String>();
        let mut hub = Hub::with_live(start(), true, Arc::clone(&live), input);

        let (_, out) = headless::one_frame(|ui| hub.ui_contents(ui));
        let texts = headless::texts(&out);
        let found = |needle: &str| texts.iter().any(|text| text.contains(needle));

        // The story: the plain line and the raw/preset room text.
        assert!(found("A synthetic breeze crosses the road."), "{texts:?}");
        assert!(found("A Synthetic Crossing"), "{texts:?}");
        assert!(found("A wide synthetic road."), "{texts:?}");
        // The room pane does not repeat the exit caption as body text: the exits
        // are a row of buttons (`n`, `se`) instead, which is what `exits_in` and
        // `PlayView::described` are for.
        assert_eq!(
            live.lock().unwrap().view.described(),
            vec!["A wide synthetic road."]
        );
        // The prompt, painted as the story's last line.
        assert!(found("synthetic>"), "{texts:?}");
        // The vitals: `HP 80%` is one of the four recovered words.
        assert!(found("HP 80%"), "{texts:?}");
        assert!(found("Sp 70%, HP 80%, MP 90%, SP 60%"), "{texts:?}");
        // Both hands.
        assert!(found("Left hand: a synthetic shield"), "{texts:?}");
        assert!(found("Right hand: a synthetic sword"), "{texts:?}");
        // A compass direction.
        assert!(found("Compass: n, se"), "{texts:?}");
        // The room number.
        assert!(found("(4242)"), "{texts:?}");
        // The status strip: the character and the connection indicator.
        assert!(found("Aelith \u{2014} connected"), "{texts:?}");
        // The input line's text.
        assert!(found("inventory"), "{texts:?}");
        // Nothing was sent: the frame only painted.
        assert!(inbox.try_recv().is_err());
        // The input line is still the model's buffer, unchanged by the frame.
        assert_eq!(locked(&live).input, "inventory");
    }

    #[test]
    fn enter_sends_the_input_line_out_of_the_window() {
        let live = Arc::new(Mutex::new(LiveSession::new("Aelith")));
        locked(&live).input = "look".to_string();
        let (input, inbox) = std::sync::mpsc::channel::<String>();
        let mut hub = Hub::with_live(start(), true, Arc::clone(&live), input);

        // Frame 1: the input row is painted with its text and the Send button.
        let ctx = egui::Context::default();
        let (_, out) = headless::frame(&ctx, headless::SIZE, |ui| hub.ui_contents(ui));
        assert!(headless::texts(&out).iter().any(|text| text == "look"));
        assert!(headless::texts(&out).iter().any(|text| text == "Send"));

        // Frame 2: the input has focus and the player pressed Enter — a
        // single-line `TextEdit` loses focus on Enter, which is the send.
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(Hub::INPUT_ID)));
        let mut raw = headless::input(headless::SIZE);
        raw.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run_ui(raw, |ui| hub.ui_contents(ui));

        // The bare line went to the writer thread, the echo is in the story and
        // the input line is empty again.
        assert_eq!(inbox.try_recv().ok(), Some("look".to_string()));
        {
            let live = locked(&live);
            assert_eq!(live.input, "");
            assert_eq!(live.story.text(1), vec!["> look".to_string()]);
            assert!(live.connected);
        }

        // The input line kept the focus, so the next command needs no click:
        // a third frame types and sends without asking for focus again.
        locked(&live).input = "north".to_string();
        let mut raw = headless::input(headless::SIZE);
        raw.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run_ui(raw, |ui| hub.ui_contents(ui));
        assert_eq!(inbox.try_recv().ok(), Some("north".to_string()));
    }

    #[test]
    fn an_empty_input_line_is_not_sent() {
        let live = Arc::new(Mutex::new(LiveSession::new("Aelith")));
        let (input, inbox) = std::sync::mpsc::channel::<String>();
        let mut hub = Hub::with_live(start(), true, Arc::clone(&live), input);

        let ctx = egui::Context::default();
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(Hub::INPUT_ID)));
        let mut raw = headless::input(headless::SIZE);
        raw.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run_ui(raw, |ui| hub.ui_contents(ui));

        assert!(inbox.try_recv().is_err());
        assert!(locked(&live).story.is_empty());
    }

    #[test]
    fn a_disconnected_session_says_so() {
        let live = Arc::new(Mutex::new(LiveSession::new("Aelith")));
        locked(&live).closed();
        let (input, _inbox) = std::sync::mpsc::channel::<String>();
        let mut hub = Hub::with_live(start(), true, Arc::clone(&live), input);

        let (_, out) = headless::one_frame(|ui| hub.ui_contents(ui));
        let texts = headless::texts(&out);
        assert!(
            texts
                .iter()
                .any(|text| text.contains("Aelith \u{2014} disconnected")),
            "{texts:?}"
        );
        // The recovered wording of the end of a session, in the story.
        assert!(
            texts.iter().any(|text| text == crate::live::GAME_CLOSED),
            "{texts:?}"
        );
        // The input row is still painted (the window is not empty); it is
        // disabled, which `add_enabled(false, …)` shows by greying the widgets.
        assert!(texts.iter().any(|text| text == "Send"), "{texts:?}");
    }

    #[test]
    fn the_hub_window_still_draws_the_empty_room_model() {
        // No session: the pre-Phase-9 play area, unchanged.
        let mut hub = Hub::new(start(), false);
        assert!(hub.live.is_none());
        assert!(hub.input.is_none());
        hub.room.title = "The Hub".into();
        let (_, out) = headless::one_frame(|ui| hub.ui_contents(ui));
        let texts = headless::texts(&out);
        assert!(texts.iter().any(|text| text == "The Hub"));
        // The live-only pieces are not drawn.
        assert!(!texts.iter().any(|text| text.contains("connected")));
        assert!(!texts.iter().any(|text| text == "Send"));
    }

    #[test]
    fn build_live_is_the_same_window_around_a_live_model() {
        let live = Arc::new(Mutex::new(LiveSession::new("Aelith")));
        let (input, _inbox) = std::sync::mpsc::channel::<String>();
        let launch = build_live(&start(), true, Arc::clone(&live), input);
        assert_eq!(launch.title, "Hydra");
        assert_eq!(launch.size(), (660.0, 520.0));
        assert_eq!(launch.options.renderer, eframe::Renderer::Wgpu);
        assert_eq!(launch.options.viewport.title.as_deref(), Some("Hydra"));
        // The badge icon is still the window icon.
        let icon = launch.options.viewport.icon.as_ref().expect("the icon");
        assert_eq!((icon.width, icon.height), (256, 256));
    }
}
