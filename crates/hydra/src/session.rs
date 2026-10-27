//! The session seam: what a headless Hydra does with the frames it parsed.
//!
//! The reference build's headless path is `hydra::play::run` (`0x100023b6c`),
//! which drives `hydra_session` directly: a
//! `hydra_session::supervisor::SupervisedSession<hydra_table::connector::LiveConnector>`
//! for the connection, `hydra_session::command::SessionHandle::subscribe`
//! (`0x100023b6c`'s call table) for the event stream,
//! `hydra_session::command::SessionHandle::snapshot` for the render state, and
//! `hydra::render::Renderer::frame` + `hydra::status::line` to print what the
//! game said.  The GUI hub (`hydra::gui::run`, `0x1001103f4`) uses the same
//! handle; the replay path uses none of it.
//!
//! This module is the part of that wiring Phase 2 can build without a live
//! connection and without `hydra_table` (`SupervisedSession<LiveConnector>` and
//! the connector are out of this crate's dependency set):
//!
//! * [`Headless::new`] builds a `SessionHandle` over a `claimant::Desk` — the
//!   recovered `SessionHandle::new` (`0x101a3bc9c`) shape;
//! * [`Headless::note`] says a `notice::Notice` on it (`SessionHandle::say`,
//!   `0x101a333…`), which is how stages and problems reach the player;
//! * [`Headless::fold`] hands parsed frames to the session as text
//!   (`SessionHandle::fold_in`, `0x101a3ca04`), which also emits
//!   `Event::Frames`;
//! * [`Headless::over`] closes it (`Event::Over`), like the relay's `session_over`
//!   (`0x101a88…`).
//!
//! The Lich door (`LichDoor`) comes along for free: `SessionHandle::lich_door`
//! (`0x101a38d58`-family) is what `hydra_lich::relay` would talk to.
//!
//! **Not wired here:** spawning Lich, the keyed-accept handshake and the live
//! connection.  Those need credentials, a Ruby/Lich installation and — for the
//! handshake — Phase 3's ground truth; `NOTES.md` records why they are out of
//! scope rather than faked.

use hydra_protocol::{Frame, Parser};
use hydra_session::claimant::Desk;
use hydra_session::command::{Listener, SessionHandle};
use hydra_session::lich::LichDoor;

use crate::source::{self, Source};

/// How many frames [`relay`] folds into the session at a time.
pub const FOLD_BATCH: usize = 64;

/// Drive a [`Source`] into a session: parse, fold the frames in batches, and
/// report how many frames went through.
///
/// This is the body `hydra::play::run` (`0x100023b6c`) runs once the connector
/// exists — `SessionHandle::fold_in` per batch plus `Renderer::frame`/
/// `status::line` for the window it does not have here.  Empty frames are
/// dropped, because the session log is text.
pub fn relay<S>(session: &Headless, source: &mut S) -> usize
where
    S: Source + ?Sized,
{
    let mut parser = Parser::new();
    let mut batch: Vec<Frame> = Vec::new();
    let total = source::pump(source, &mut parser, |frame| {
        if frame.is_empty() {
            return;
        }
        batch.push(frame);
        if batch.len() >= FOLD_BATCH {
            session.fold(&batch);
            batch.clear();
        }
    });
    session.fold(&batch);
    total
}

/// A session with no window and no connection.
#[derive(Clone, Debug)]
pub struct Headless {
    handle: SessionHandle,
}

impl Headless {
    /// A session for `character` whose command symbol is the default `.`.
    pub fn new(character: &str) -> Self {
        Headless {
            handle: SessionHandle::new(Desk::new(None, character)),
        }
    }

    /// The handle everything else holds.
    pub fn handle(&self) -> &SessionHandle {
        &self.handle
    }

    /// The Lich door (`SessionHandle::lich_door`).
    pub fn door(&self) -> LichDoor {
        self.handle.lich_door()
    }

    /// Say a notice at `level` (`SessionHandle::say_line`).
    pub fn note(&self, level: u8, text: impl Into<String>) {
        self.handle.say_line(level, text);
    }

    /// Fold parsed frames into the session, as text, in stream order.
    pub fn fold(&self, frames: &[Frame]) {
        let texts: Vec<String> = frames.iter().map(Frame::plain).collect();
        self.handle.fold_in(texts);
    }

    /// Close the session with `reason`.
    ///
    /// The Lich slot is closed first (`LichDoor::over` → `Slot::close`,
    /// `0x101a88408`, which drops both taps and sets `session_over`), then the
    /// session is stopped — `SessionHandle::stop` trips the cancellation token
    /// and emits `Event::Over(reason)`.
    pub fn over(&self, reason: &str) {
        self.handle.lich_door().over();
        self.handle.stop(reason);
    }

    /// Subscribe to this session's event stream.
    pub fn subscribe(&self) -> Listener {
        self.handle.subscribe()
    }

    /// The character this session belongs to.
    pub fn character(&self) -> String {
        self.handle.snapshot().character
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_session::command::Event;
    use hydra_session::notice::Notice;

    fn text_frame(text: &str) -> Frame {
        let mut runs = hydra_protocol::Runs::new();
        runs.push(hydra_protocol::Run::new(text));
        Frame::Text(runs)
    }

    #[test]
    fn a_headless_session_keeps_its_character() {
        let session = Headless::new("Aelith");
        assert_eq!(session.character(), "Aelith");
        assert_eq!(session.handle().symbol(), Some('.'));
    }

    #[test]
    fn frames_are_folded_in_as_text_and_announced() {
        let session = Headless::new("Aelith");
        let mut listener = session.subscribe();
        session.fold(&[text_frame("you feel better"), Frame::Prompt(">".to_string())]);

        let events = listener.drain();
        match &events[0] {
            Event::Frames(lines) => {
                assert_eq!(lines, &vec!["you feel better".to_string(), ">".to_string()])
            }
            other => panic!("expected Event::Frames, got {other:?}"),
        }
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn an_empty_fold_says_nothing() {
        let session = Headless::new("Aelith");
        let mut listener = session.subscribe();
        session.fold(&[]);
        assert!(listener.drain().is_empty());
    }

    #[test]
    fn notes_and_the_end_reach_the_event_stream() {
        let session = Headless::new("Aelith");
        let mut listener = session.subscribe();
        session.note(1, "Lich stopped.");
        session.over("the recording ended");

        let events = listener.drain();
        assert!(matches!(&events[0], Event::Said(n) if n.first_line() == "Lich stopped."));
        assert!(matches!(&events[1], Event::Over(reason) if reason == "the recording ended"));
        assert_eq!(session.handle().said(), vec!["Lich stopped.".to_string()]);
        let notice = Notice::line(1, "Lich stopped.");
        assert_eq!(notice.first_line(), "Lich stopped.");
    }

    #[test]
    fn a_relayed_stream_lands_in_the_session_in_order() {
        // Lich source → hydra_protocol parser → hydra_session, end to end.
        let lines = std::io::Cursor::new(
            b"you feel better\r\n<a exist=\"-1\" noun=\"gem\">a gem</a>\r\n<prompt>&gt;</prompt>\r\n"
                .to_vec(),
        );
        let mut source = crate::source::Lich::listen(lines);
        let session = Headless::new("Aelith");
        let mut listener = session.subscribe();

        // A text line, then an object link (open `Structural`, the text, close
        // `Structural`), then the prompt.
        assert_eq!(relay(&session, &mut source), 5);

        let events = listener.drain();
        match &events[0] {
            // The two `Structural` frames carry no text, so the session keeps
            // only the three text-bearing ones.
            Event::Frames(text) => assert_eq!(
                text,
                &vec!["you feel better".to_string(), "a gem".to_string(), ">".to_string()]
            ),
            other => panic!("expected Event::Frames, got {other:?}"),
        }
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn the_lich_door_belongs_to_the_same_session() {
        let session = Headless::new("Aelith");
        let door = session.door();
        assert_eq!(door.symbol(), '.');
        assert!(!door.session_over());
        session.over("done");
        assert!(session.door().session_over());
    }
}
