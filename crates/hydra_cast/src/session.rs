//! The session glue: the command queue behind the driver's [`Sink`], and the
//! event stream the driver selects on.
//!
//! The recovered `Game::send` / `cast_line` / `cast_through`
//! (`0x10197ae94` and its callees) pushed the line onto `hydra_session`'s
//! command queue — `queue::CommandQueue::admit` is at `0x101a0c52c` — and
//! `driver::start` selected over the session's event broadcast.  Phase 1 makes
//! both real:
//!
//! * [`SessionSink`] implements [`Sink`] over `CommandQueue::admit`, which is
//!   the seam the crate kept while `hydra_session` was on another branch;
//! * [`watch`] hands the driver the session's real
//!   `tokio::sync::broadcast` of `hydra_session::command::Event`s.
//!
//! UNKNOWN: which `Envelope` fields `Game::send` filled in.  The recovered
//! driver did not await the answer itself (it selected over the events
//! instead), so the sink queues a plain envelope with no reply ticket.

use hydra_session::command::{Event, SessionHandle};
use hydra_session::queue::Envelope;
use tokio::sync::broadcast;

use crate::driver::Sink;

/// The driver's [`Sink`] over a session's command queue.
///
/// This is the "send a command to the game" step the recovered `Game::send`
/// did: queue it, and let the session's write path take it from there.
#[derive(Clone, Debug)]
pub struct SessionSink {
    session: SessionHandle,
}

impl SessionSink {
    /// A sink writing to `session`.
    pub fn new(session: SessionHandle) -> SessionSink {
        SessionSink { session }
    }

    /// The session behind the sink.
    pub fn session(&self) -> &SessionHandle {
        &self.session
    }

    /// How many commands are waiting to be written.
    pub fn waiting(&self) -> usize {
        self.session.queue().waiting()
    }
}

impl Sink for SessionSink {
    fn say(&mut self, line: &str) -> Result<(), ()> {
        // `0x101a0c52c` — a plain push onto the back of the queue.
        self.session.queue().admit(Envelope::new(line));
        Ok(())
    }
}

/// The session's events, as `driver::Relay` selects over them.
///
/// `SessionHandle::watch` is the real `tokio::sync::broadcast` the session
/// emits every `say`/`say_unlogged`/frame event on.
pub fn watch(session: &SessionHandle) -> broadcast::Receiver<Event> {
    session.watch()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_session::claimant::Desk;

    fn session() -> SessionHandle {
        SessionHandle::new(Desk::new(None, "Tester"))
    }

    #[test]
    fn the_sink_admits_into_the_command_queue_in_order() {
        let session = session();
        let mut sink = SessionSink::new(session.clone());
        sink.say("prepare shock").unwrap();
        sink.say("cast").unwrap();
        assert_eq!(sink.waiting(), 2);
        let queue = session.queue();
        let lines: Vec<String> = queue.lines().map(|line| line.to_string()).collect();
        assert_eq!(lines, vec!["prepare shock", "cast"]);
    }

    #[test]
    fn the_session_reports_what_it_says_on_its_broadcast() {
        let session = session();
        let mut events = watch(&session);
        session.say_line(2, "You prepare shock.");
        match events.try_recv() {
            Ok(Event::Said(notice)) => assert_eq!(notice.lines, vec!["You prepare shock."]),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
