//! The background loop that casts, and the state machine under it.
//!
//! `hydra_cast::driver::start` (0x1019a7d14, 37304 bytes) is the crate's tokio
//! task: it owns an unbounded channel of [`crate::request::Request`]s, a
//! `Mutex`-guarded settings cell and a broadcast of the session's
//! `hydra_session::event::Event`s, and it drives the async methods of
//! `driver::Game`.  Those methods are in the symbol table as their own closures:
//!
//! ```text
//! 0x10197a58c  Game::out_of_roundtime::{{closure}}     496 B
//! 0x10197a77c  Game::idle::{{closure}}                 460 B
//! 0x10197a948  Game::next::{{closure}}                1356 B
//! 0x10197ae94  Game::send::{{closure}}                2012 B
//! 0x10197b670  Game::stance::{{closure}}              1756 B
//! 0x10197bd4c  Game::release::{{closure}}              988 B
//! 0x10197c128  driver::start::{{closure}}            11604 B
//! 0x1019a80f8  driver::start::{{closure}}              484 B
//! 0x1019a82dc  driver::Settings::get                    1760 B
//! ```
//!
//! The call graph inside them is established:
//!
//! ```text
//! out_of_roundtime -> idle -> next -> fold
//! stance           -> next, send, answers::answer
//! release          -> send, next, answers::answer
//! ```
//!
//! Only [`Game::fold`] (0x1019a7c70, 164 bytes) is small enough to be read
//! whole; it is implemented exactly.  Everything else is a synchronous
//! reconstruction with the unknowns marked.
//!
//! # Phase 1
//!
//! The loop is a real tokio task again.  [`Relay`] is the recovered
//! `driver::start` body: it owns the unbounded request channel
//! (`tokio::sync::mpsc`), a `watch` of the settings cell and a `broadcast` of
//! the session's events, and selects over the three with `tokio::select!`.
//! [`Relay::start`] is the synchronous adapter that owns a current-thread
//! runtime, and [`Relay::spawn`] is the task form the original was started as.
//!
//! The synchronous [`start`] over `std::sync::mpsc` stays: it is the API the
//! crate shipped before the reconnection and the one the existing tests use.
//! The transport seam [`Sink`] stays too; [`crate::session::SessionSink`] is
//! the `hydra_session` command-queue implementation of it.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use hydra_session::command::Event as SessionEvent;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;

use crate::game::World;
use crate::plan::{self, Cast};
use crate::request::Request;
use crate::settings::CasterSettings;

/// Why the driver stopped.
///
/// UNKNOWN: `hydra_cast::driver::Halt` is a type in the binary (its
/// `drop_in_place` glue is at 0x1019857dc) but its variants are not recovered;
/// the one message the driver builds about stopping is
/// "{} could not be prepared." (0x103e7b1ee).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Halt {
    /// The game could not be reached, or the channel closed.
    Closed,
    /// A cast could not be prepared.
    Unprepared(String),
}

/// The model state the driver keeps between events.
///
/// ESTABLISHED: the first field is the `hydra_model::state::GameState` that
/// `fold` applies events to (0x1019a7c8c), `+0x1368` counts the lines already
/// handed out, and the line list it slices is `{ptr @+0xf18, len @+0xf20,
/// produced @+0xf28}`.  UNKNOWN: everything at +0xf30 .. +0x1368, which
/// belongs to `hydra_model`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// Every line the game has said since the driver started.
    pub lines: Vec<String>,
    /// How many of them the caller has seen (the counter at +0x1368).
    pub said: usize,
}

/// One thing that happened outside the caster.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The game said a line.
    Said(String),
    /// Everything the game knew was replaced, so the line list starts over
    /// (the folded event whose tag clears the counter at 0x1019a7cac).
    Reset,
}

/// The caster's state machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Game {
    /// The game model, advanced by every event.
    pub state: State,
    /// The settings in force.
    pub settings: CasterSettings,
}

impl Game {
    /// A game with these settings.
    pub fn new(settings: CasterSettings) -> Game {
        Game {
            state: State::default(),
            settings,
        }
    }

    /// Fold one event in and return the lines that are new since last time.
    ///
    /// Recovered whole from 0x1019a7c70:
    ///
    /// * the event is applied to the model state first,
    /// * an event that replaces the model clears the "already said" counter and
    ///   returns an empty `Vec`,
    /// * otherwise the tail of the line list from the counter to its end is
    ///   copied out and the counter is moved to the end.
    pub fn fold(&mut self, event: &Event) -> Vec<String> {
        match event {
            Event::Reset => {
                self.state.lines.clear();
                self.state.said = 0;
                Vec::new()
            }
            Event::Said(line) => {
                self.state.lines.push(line.clone());
                let fresh = self.state.lines[self.state.said..].to_vec();
                self.state.said = self.state.lines.len();
                fresh
            }
        }
    }

    /// Ask for the next cast to make: build the plan for one spell.
    ///
    /// UNKNOWN: `Game::next` (0x10197a948) picks the spell from the model's
    /// own state and the settings; only its calls are recovered (`fold`, then
    /// `plan::plan`).  Here the caller supplies the cast.
    pub fn next(
        &mut self,
        cast: &Cast,
        world: &dyn World,
    ) -> Result<plan::Plan, String> {
        plan::plan(cast, &self.settings, world)
    }

    /// Send one line to the game.
    ///
    /// UNKNOWN: `Game::send` (0x10197ae94) wrote into the session's command
    /// queue and could wait for the answer; here the seam is the [`Sink`].
    pub fn send(sink: &mut dyn Sink, line: &str) -> Result<(), Halt> {
        sink.say(line).map_err(|_| Halt::Closed)
    }
}

/// Seam: "send a command to the game".
///
/// The original pushed onto `hydra_session`'s command queue from
/// `Game::send`/`cast_line`/`cast_through`; the seam keeps the driver free of
/// the session, and [`crate::session::SessionSink`] is the queue behind it.
///
/// Phase 1: `Send` is required because the driver now lives in a tokio task.
pub trait Sink: Send {
    /// Send one line.  An error means the connection is gone.
    fn say(&mut self, line: &str) -> Result<(), ()>;
}

/// The driver's settings cell.
///
/// ESTABLISHED: `driver::Settings` is an `Arc<ArcInner<…>>` (drop glue at
/// 0x1019a9cd4) read through a `MutexGuard<driver::Read>`
/// (0x1019a9de4), and `Settings::get` is at 0x1019a82dc.  UNKNOWN: what
/// `Read` holds besides the settings; the generation counter is a guess kept
/// so callers can tell two reads apart.
///
/// Phase 1: the cell also owns the `tokio::sync::watch` the async driver
/// selects on, so a settings change wakes the loop instead of waiting for the
/// next request.  The mutex and its `get`/`put` shape are unchanged.
#[derive(Clone, Debug)]
pub struct Settings {
    inner: Arc<Mutex<Read>>,
    changed: watch::Sender<CasterSettings>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            inner: Arc::new(Mutex::new(Read::default())),
            changed: watch::Sender::new(CasterSettings::default()),
        }
    }
}

/// What the settings mutex guards.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Read {
    /// The caster settings in force.
    pub settings: CasterSettings,
    /// Bumped by every write.
    pub generation: u64,
}

impl Settings {
    /// An empty cell.
    pub fn new() -> Settings {
        Settings::default()
    }

    /// The settings as they are now.
    pub fn get(&self) -> CasterSettings {
        self.inner
            .lock()
            .map(|read| read.settings.clone())
            .unwrap_or_default()
    }

    /// Replace the settings; returns the new generation.
    pub fn put(&self, settings: CasterSettings) -> u64 {
        // The watch comes first so no reader can miss the change; a receiver
        // that is gone is not an error (`send_replace` keeps the value).
        self.changed.send_replace(settings.clone());
        match self.inner.lock() {
            Ok(mut read) => {
                read.settings = settings;
                read.generation += 1;
                read.generation
            }
            Err(poisoned) => {
                let mut read = poisoned.into_inner();
                read.settings = settings;
                read.generation += 1;
                read.generation
            }
        }
    }

    /// A receiver that is told about every write — the async driver's third
    /// `select!` arm.
    pub fn watch(&self) -> watch::Receiver<CasterSettings> {
        self.changed.subscribe()
    }
}

/// Run the caster until the request channel closes.
///
/// ESTABLISHED: the loop takes [`Request`]s, keeps one [`Game`], plans each
/// cast against the settings and sends the resulting lines to the game; it
/// reads the settings from a shared cell so a settings change lands on the
/// next cast.
///
/// UNKNOWN / departure: the original is a tokio task that selects over three
/// sources (requests, session events, settings changes) and drives async
/// methods with waits (`Game::answer`, `Game::release`,
/// `Game::out_of_roundtime`).  This is the synchronous equivalent: requests are
/// consumed one at a time, no waiting happens, and the command sink is a
/// [`Sink`] instead of the session's queue.  [`Relay`] is the tokio form of the
/// same loop.
pub fn start(
    requests: Receiver<Request>,
    settings: Settings,
    game: &mut Game,
    world: &dyn World,
    sink: &mut dyn Sink,
) -> Result<(), Halt> {
    for request in requests {
        game.settings = settings.get();
        let cast = Cast {
            spell: request.spell,
            target: request.target.clone(),
        };
        let plan = match game.next(&cast, world) {
            Ok(plan) => plan,
            // 0x103e7b1ee "{} could not be prepared." — the driver's own
            // wrapping of a refused cast.
            Err(_) => return Err(Halt::Unprepared(cast.spell.to_string())),
        };
        for line in &plan.lines {
            Game::send(sink, line)?;
        }
    }
    Err(Halt::Closed)
}

/// Plan one request and send its lines, in order.
///
/// ESTABLISHED: the recovered loop plans against the settings in force and
/// sends the plan's lines one after the other; a refused plan stops the driver
/// with "{} could not be prepared." (0x103e7b1ee).
fn cast_one(
    game: &mut Game,
    world: &dyn World,
    sink: &mut dyn Sink,
    request: &Request,
) -> Result<(), Halt> {
    let cast = Cast {
        spell: request.spell,
        target: request.target.clone(),
    };
    let plan = match game.next(&cast, world) {
        Ok(plan) => plan,
        Err(_) => return Err(Halt::Unprepared(cast.spell.to_string())),
    };
    for line in &plan.lines {
        Game::send(sink, line)?;
    }
    Ok(())
}

/// The request side of the driver.
///
/// ESTABLISHED: `driver::start` holds
/// `tokio::sync::mpsc::unbounded::UnboundedReceiver<Request>` (its drop glue is
/// at 0x10197f5b4), so the producing side is the unbounded sender.
#[derive(Clone, Debug)]
pub struct Handle {
    tx: UnboundedSender<Request>,
}

impl Handle {
    /// Hand one request to the driver.
    pub fn request(&self, request: Request) -> Result<(), ()> {
        self.tx.send(request).map_err(|_| ())
    }

    /// Read a line of the caster's own grammar and hand it over.
    ///
    /// `None` when the line is not a request or the driver is gone.
    pub fn line(&self, line: &str) -> Option<Request> {
        let request = crate::request::request(line)?;
        self.request(request.clone()).ok()?;
        Some(request)
    }

    /// Whether the driver is still listening.
    pub fn is_open(&self) -> bool {
        !self.tx.is_closed()
    }
}

/// A request channel: the tokio unbounded pair the recovered driver reads.
pub fn channel() -> (Handle, UnboundedReceiver<Request>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (Handle { tx }, rx)
}

/// Fold one session event into the game.
///
/// ESTABLISHED: the driver's `select!` had an arm for the session's events and
/// handed the game text to `Game::fold` (which is why `fold` receives whole
/// lines).  `Over` is the session ending, which replaces the model and clears
/// the line counter, so it folds as [`Event::Reset`] (INFERRED).
pub fn fold_session(game: &mut Game, event: &SessionEvent) {
    match event {
        SessionEvent::Frames(lines) => {
            for line in lines {
                game.fold(&Event::Said(line.clone()));
            }
        }
        SessionEvent::Said(notice) => {
            for line in &notice.lines {
                game.fold(&Event::Said(line.clone()));
            }
        }
        SessionEvent::Over(_) => {
            game.fold(&Event::Reset);
        }
        SessionEvent::Typed(_) | SessionEvent::Symbol(_) | SessionEvent::LichSymbol(_) => {}
    }
}

/// The session event arm: the next event, or wait forever when there is no
/// subscription.
async fn next_event(
    events: &mut Option<broadcast::Receiver<SessionEvent>>,
) -> Option<SessionEvent> {
    loop {
        if events.is_none() {
            return std::future::pending().await;
        }
        let received = events.as_mut().expect("checked above").recv().await;
        match received {
            Ok(event) => return Some(event),
            // A slow reader missed events; the session keeps the newest ones.
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => {
                *events = None;
                return None;
            }
        }
    }
}

/// The tokio driver: the body of the recovered `driver::start`
/// (0x10197c128, the 11604-byte `start::{{closure}}`).
///
/// It selects over three sources, which is what the symbol list shows the
/// original owning:
///
/// 1. the unbounded channel of [`Request`]s ([`Handle`], [`channel`]);
/// 2. the session's event broadcast (`hydra_session::command::Event`, through
///    [`crate::session::watch`]), folded into the [`Game`];
/// 3. changes to the settings cell ([`Settings::watch`]).
///
/// The transport seam stays [`Sink`]; [`crate::session::SessionSink`] is the
/// `hydra_session` command queue behind it.
///
/// UNKNOWN: the bodies of `Game::{next, send, stance, release, answer,
/// out_of_roundtime}` are async state machines in the binary and only their
/// call graph was recovered, so the loop here plans and sends synchronously
/// inside each arm.  What *is* recovered is what the task waits on, and that is
/// what this type is.
pub struct Relay<'a> {
    /// The casts to carry out.
    pub requests: UnboundedReceiver<Request>,
    /// The settings cell in force.
    pub settings: Settings,
    /// The session's events, when the driver was given a subscription.
    pub events: Option<broadcast::Receiver<SessionEvent>>,
    /// The state machine.
    pub game: &'a mut Game,
    /// The game model.
    pub world: &'a dyn World,
    /// Where the commands go.
    pub sink: &'a mut dyn Sink,
}

impl<'a> Relay<'a> {
    /// The four inputs of the recovered task, minus the event subscription.
    pub fn new(
        requests: UnboundedReceiver<Request>,
        settings: Settings,
        game: &'a mut Game,
        world: &'a dyn World,
        sink: &'a mut dyn Sink,
    ) -> Relay<'a> {
        Relay {
            requests,
            settings,
            events: None,
            game,
            world,
            sink,
        }
    }

    /// Also select over the session's events.
    pub fn with_events(mut self, events: broadcast::Receiver<SessionEvent>) -> Relay<'a> {
        self.events = Some(events);
        self
    }

    /// Run until the request channel closes or a cast is refused.
    ///
    /// The settings are re-read before every cast (the recovered loop read the
    /// cell on every send) and kept current while idle by the `watch` arm.
    pub async fn run(&mut self) -> Result<(), Halt> {
        let Relay {
            requests,
            settings,
            events,
            game,
            world,
            sink,
        } = self;
        let mut changes = settings.watch();
        game.settings = changes.borrow_and_update().clone();

        loop {
            tokio::select! {
                request = requests.recv() => {
                    match request {
                        Some(request) => {
                            // A change that has not been seen yet lands before
                            // this cast, whichever arm fires first.
                            if changes.has_changed().unwrap_or(false) {
                                game.settings = changes.borrow_and_update().clone();
                            }
                            cast_one(game, *world, &mut **sink, &request)?;
                        }
                        // Every sender is gone: the recovered driver stops.
                        None => return Err(Halt::Closed),
                    }
                }
                changed = changes.changed() => {
                    if changed.is_ok() {
                        game.settings = changes.borrow_and_update().clone();
                    }
                }
                event = next_event(events) => {
                    if let Some(event) = event {
                        fold_session(game, &event);
                    }
                }
            }
        }
    }

    /// Run to completion on a private current-thread runtime.
    ///
    /// This is the synchronous adapter: the recovered `driver::start` was
    /// called from non-async code, so the tokio loop is driven by a runtime the
    /// call owns.
    pub fn start(self) -> Result<(), Halt> {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            // No runtime means no driver; "the game could not be reached".
            Err(_) => return Err(Halt::Closed),
        };
        runtime.block_on(async move {
            let mut relay = self;
            relay.run().await
        })
    }
}

impl Relay<'static> {
    /// Start the driver as a tokio task — the shape the original had.
    ///
    /// The [`Game`], the model reference and the sink move into the task; the
    /// task's result is the same [`Halt`] the synchronous loop returns.
    pub fn spawn<S>(
        requests: UnboundedReceiver<Request>,
        settings: Settings,
        events: Option<broadcast::Receiver<SessionEvent>>,
        mut game: Game,
        world: &'static dyn World,
        mut sink: S,
    ) -> tokio::task::JoinHandle<Result<(), Halt>>
    where
        S: Sink + Send + 'static,
    {
        tokio::spawn(async move {
            let mut relay = Relay {
                requests,
                settings,
                events,
                game: &mut game,
                world,
                sink: &mut sink,
            };
            relay.run().await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Nothing;

    #[test]
    fn fold_returns_only_the_new_lines() {
        let mut game = Game::new(CasterSettings::default());
        assert_eq!(
            game.fold(&Event::Said("one".to_string())),
            vec!["one".to_string()]
        );
        assert_eq!(
            game.fold(&Event::Said("two".to_string())),
            vec!["two".to_string()]
        );
        assert!(game.fold(&Event::Reset).is_empty());
        assert_eq!(
            game.fold(&Event::Said("three".to_string())),
            vec!["three".to_string()]
        );
        assert_eq!(game.state.lines, vec!["three".to_string()]);
    }

    #[test]
    fn start_plans_and_sends() {
        struct Say(Vec<String>);
        impl Sink for Say {
            fn say(&mut self, line: &str) -> Result<(), ()> {
                self.0.push(line.to_string());
                Ok(())
            }
        }
        let settings = Settings::new();
        let mut game = Game::new(CasterSettings::default());
        let mut sink = Say(Vec::new());
        // `Nothing` knows no spells, so the loop stops with a refusal and
        // nothing is sent.
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Request::new(901)).unwrap();
        drop(tx);
        assert_eq!(
            start(rx, settings, &mut game, &Nothing, &mut sink),
            Err(Halt::Unprepared("901".to_string()))
        );
        assert!(sink.0.is_empty());
    }

    #[test]
    fn settings_cell_hands_out_copies() {
        let settings = Settings::new();
        assert_eq!(settings.get(), CasterSettings::default());
        let mut wanted = CasterSettings::default();
        wanted.switch(true);
        assert_eq!(settings.put(wanted.clone()), 1);
        assert_eq!(settings.get(), wanted);
        assert_eq!(settings.put(CasterSettings::default()), 2);
    }
}
