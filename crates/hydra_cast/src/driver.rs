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

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

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
/// `Game::send`/`cast_line`/`cast_through`; nothing of that crate is reachable
/// from a synchronous loop here, so the driver takes a sink.
pub trait Sink {
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
#[derive(Clone, Debug, Default)]
pub struct Settings {
    inner: Arc<Mutex<Read>>,
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
/// [`Sink`] instead of the session's queue.
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
