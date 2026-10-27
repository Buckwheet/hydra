//! The async driver: the tokio loop the recovered `driver::start` is.
//!
//! These tests use the fixture model below (the real spell table lives in
//! `hydra_model` and is exercised in `tests/model.rs`); every message and line
//! asserted is one `plan::plan` builds from recovered literals.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use hydra_cast::driver::{self, Game, Halt, Relay, Settings, Sink};
use hydra_cast::game::World;
use hydra_cast::request::Request;
use hydra_cast::session::SessionSink;
use hydra_cast::settings::CasterSettings;

/// A tiny model: two spells, the second one an attack.
struct Fixture;

const SHOCK: u16 = 901;
const BALL: u16 = 902;

impl World for Fixture {
    fn spell_name(&self, number: u16) -> Option<&str> {
        match number {
            SHOCK => Some("shock"),
            BALL => Some("ball"),
            _ => None,
        }
    }

    fn knows(&self, _number: u16) -> bool {
        true
    }

    fn is_attack(&self, number: u16) -> bool {
        number == BALL
    }

    fn spell_cost(&self, number: u16, resource: &str) -> Option<(f64, i64)> {
        match (number, resource) {
            (SHOCK, "mana") => Some((1.0, 3)),
            (SHOCK, "second") => Some((1.0, 2)),
            _ => None,
        }
    }

    fn has_live_target(&self, target: &str) -> bool {
        target == "kobold"
    }
}

static FIXTURE: Fixture = Fixture;

/// Install the fixture as the crate's global model too: the settings commands
/// (`ec verb`, `ec alias`) resolve spell numbers through it.
fn fixture() {
    hydra_cast::game::install(&FIXTURE);
}

/// A sink that remembers every line.
#[derive(Default)]
struct Lines(Vec<String>);

impl Sink for Lines {
    fn say(&mut self, line: &str) -> Result<(), ()> {
        self.0.push(line.to_string());
        Ok(())
    }
}

fn request(spell: u16, target: Option<&str>) -> Request {
    Request {
        spell,
        target: target.map(|target| target.to_string()),
        verb: None,
    }
}

#[test]
fn the_driver_relays_requests_in_order() {
    // The sync adapter: the tokio loop runs on a runtime it owns.
    let (handle, requests) = driver::channel();
    let settings = Settings::new();
    let mut game = Game::new(CasterSettings::default());
    let mut sink = Lines::default();

    handle.request(request(SHOCK, None)).unwrap();
    handle.request(request(BALL, Some("kobold"))).unwrap();
    drop(handle);

    let halted = Relay::new(requests, settings, &mut game, &FIXTURE, &mut sink).start();

    assert_eq!(halted, Err(Halt::Closed));
    // `prepare`/`target`/`cast`, in the order the builder writes them, for
    // each request in turn.
    assert_eq!(
        sink.0,
        vec![
            "prepare shock",
            "cast",
            "prepare ball",
            "target kobold",
            "cast kobold",
        ]
    );
}

#[test]
fn a_refused_cast_stops_the_driver() {
    let (handle, requests) = driver::channel();
    let settings = Settings::new();
    let mut game = Game::new(CasterSettings::default());
    let mut sink = Lines::default();

    handle.request(Request::new(999)).unwrap();
    drop(handle);

    let halted = Relay::new(requests, settings, &mut game, &FIXTURE, &mut sink).start();

    // 0x103e7b1ee "{} could not be prepared."
    assert_eq!(halted, Err(Halt::Unprepared("999".to_string())));
    assert!(sink.0.is_empty());
}

#[tokio::test]
async fn a_settings_change_lands_on_the_next_cast() {
    fixture();
    let (handle, requests) = driver::channel();
    let settings = Settings::new();
    let mut game = Game::new(CasterSettings::default());
    let (said, mut lines) = mpsc::unbounded_channel();
    let mut sink = Channel(said);

    let mut relay = Relay::new(requests, settings.clone(), &mut game, &FIXTURE, &mut sink);
    let run = relay.run();
    let drive = async {
        // The first cast uses the default verb: `prepare`.
        handle.request(request(SHOCK, None)).unwrap();
        assert_eq!(lines.recv().await, Some("prepare shock".to_string()));
        assert_eq!(lines.recv().await, Some("cast".to_string()));

        // A settings change (ec verb 901 channel) must reach the next cast.
        let mut changed = settings.get();
        changed.set_verb("901", "channel").unwrap();
        settings.put(changed);
        handle.request(request(SHOCK, None)).unwrap();
        assert_eq!(lines.recv().await, Some("incant shockchannel".to_string()));
        assert_eq!(lines.recv().await, Some("cast".to_string()));

        drop(handle);
    };

    let (halted, ()) = tokio::join!(run, drive);
    assert_eq!(halted, Err(Halt::Closed));
}

#[tokio::test]
async fn a_settings_change_reaches_the_game_while_it_is_idle() {
    fixture();
    let (_handle, requests) = driver::channel();
    let settings = Settings::new();
    let mut game = Game::new(CasterSettings::default());
    let mut sink = Lines::default();

    let mut relay = Relay::new(requests, settings.clone(), &mut game, &FIXTURE, &mut sink);
    let run = relay.run();
    let changer = async {
        let mut changed = settings.get();
        changed.switch(true);
        settings.put(changed);
        // Let the watch arm fire before the request channel closes.
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(_handle);
    };

    let (halted, ()) = tokio::join!(run, changer);
    assert_eq!(halted, Err(Halt::Closed));
    // 0x1019b4598: `switch(true)` — "Typed spells are cast."
    assert!(game.settings.on);
}

#[tokio::test]
async fn the_driver_folds_the_session_events_it_selects_on() {
    let session = hydra_session::command::SessionHandle::new(hydra_session::claimant::Desk::new(
        None, "Tester",
    ));
    let events = hydra_cast::session::watch(&session);
    // The sink is the real session queue, so both sides of the session are
    // exercised: commands in, events out.
    let mut sink = SessionSink::new(session.clone());

    let (handle, requests) = driver::channel();
    let settings = Settings::new();
    let mut game = Game::new(CasterSettings::default());

    let mut relay =
        Relay::new(requests, settings.clone(), &mut game, &FIXTURE, &mut sink).with_events(events);
    let run = relay.run();
    let drive = async {
        handle.request(request(SHOCK, None)).unwrap();
        session.say_line(2, "You prepare shock.");
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(handle);
    };

    let (halted, ()) = tokio::join!(run, drive);
    assert_eq!(halted, Err(Halt::Closed));
    // The event was folded into the model's line list ...
    assert_eq!(
        game.state.lines,
        vec!["You prepare shock.".to_string()],
        "the event arm folded the session's line"
    );
    // ... and the request went into the session's command queue.
    let queue = session.queue();
    let queued: Vec<String> = queue.lines().map(|line| line.to_string()).collect();
    assert_eq!(queued, vec!["prepare shock", "cast"]);
}

#[tokio::test]
async fn the_driver_can_run_as_a_task() {
    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<String>>>);

    impl Sink for Shared {
        fn say(&mut self, line: &str) -> Result<(), ()> {
            self.0.lock().unwrap().push(line.to_string());
            Ok(())
        }
    }

    let said = Shared::default();
    let (handle, requests) = driver::channel();
    let task = Relay::spawn(
        requests,
        Settings::new(),
        None,
        Game::new(CasterSettings::default()),
        &FIXTURE,
        said.clone(),
    );
    handle.request(request(SHOCK, None)).unwrap();
    drop(handle);
    assert_eq!(task.await.unwrap(), Err(Halt::Closed));
    assert_eq!(
        *said.0.lock().unwrap(),
        vec!["prepare shock".to_string(), "cast".to_string()]
    );
}

/// A sink that hands its lines to the test.
struct Channel(mpsc::UnboundedSender<String>);

impl Sink for Channel {
    fn say(&mut self, line: &str) -> Result<(), ()> {
        self.0.send(line.to_string()).map_err(|_| ())
    }
}
