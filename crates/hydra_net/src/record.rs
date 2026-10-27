//! Recording both directions of the wire.
//!
//! Recovered from `hydra_net::record` (`0x101c51428` .. `0x101c53550`).  The
//! recorder is the seam between the socket and the session sink: everything that
//! comes in and goes out is copied into the recording before it is parsed.

use std::io;

use crate::sink::writer::SessionSink;

/// Copies the wire into a session sink as it flows past.
pub struct Recorder {
    /// Where the bytes go; a recorder without a sink still counts.
    pub sink: Option<SessionSink>,
    /// Bytes recorded from the server.
    pub inbound_bytes: u64,
    /// Bytes recorded to the server.
    pub outbound_bytes: u64,
}

impl Recorder {
    /// A recorder with no sink: it counts but records nothing.
    pub fn new() -> Recorder {
        Recorder {
            sink: None,
            inbound_bytes: 0,
            outbound_bytes: 0,
        }
    }

    /// A recorder writing into a session sink.
    pub fn with_sink(sink: SessionSink) -> Recorder {
        Recorder {
            sink: Some(sink),
            inbound_bytes: 0,
            outbound_bytes: 0,
        }
    }

    /// Record what the server sent.
    ///
    /// Recovered from `0x101c51428`: the bytes go to the sink before anything
    /// else looks at them, so a parse failure still leaves the wire on disk.
    pub fn inbound(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.inbound_bytes += bytes.len() as u64;
        match &mut self.sink {
            Some(sink) => sink.wire('<', bytes),
            None => Ok(()),
        }
    }

    /// Record what Hydra sent.
    ///
    /// Recovered from `0x101c515cc`: the same as [`Recorder::inbound`], in the
    /// other direction.
    pub fn outbound(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.outbound_bytes += bytes.len() as u64;
        match &mut self.sink {
            Some(sink) => sink.wire('>', bytes),
            None => Ok(()),
        }
    }

    /// Note an event in the session log.
    pub fn event(&mut self, text: &str) -> io::Result<()> {
        match &mut self.sink {
            Some(sink) => sink.event(text),
            None => Ok(()),
        }
    }
}

impl Default for Recorder {
    fn default() -> Recorder {
        Recorder::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::redactions::Redactions;

    fn scratch(name: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("hydra-net-rec-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_recorder_without_a_sink_still_counts() {
        let mut recorder = Recorder::new();
        recorder.inbound(b"<c>x</c>").unwrap();
        recorder.outbound(b"look").unwrap();
        assert_eq!(recorder.inbound_bytes, 8);
        assert_eq!(recorder.outbound_bytes, 4);
        assert!(recorder.event("nothing to write").is_ok());
    }

    #[test]
    fn both_directions_land_in_the_recording() {
        let dir = scratch("both");
        let sink = SessionSink::create(&dir, "session", Redactions::new()).unwrap();
        let mut recorder = Recorder::with_sink(sink);
        recorder.inbound(b"in").unwrap();
        recorder.outbound(b"out").unwrap();
        recorder.event("connected").unwrap();

        let bytes = std::fs::read(recorder.sink.as_ref().unwrap().bytes_path()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("in"), "{text}");
        assert!(text.contains("out"), "{text}");
        let log = std::fs::read_to_string(recorder.sink.as_ref().unwrap().log_path()).unwrap();
        assert!(log.contains("connected"), "{log}");
        assert_eq!(recorder.inbound_bytes, 2);
        assert_eq!(recorder.outbound_bytes, 3);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
