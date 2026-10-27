//! The session sink: the private development log and the raw wire recording.
//!
//! Recovered from `hydra_net::sink::writer` (`0x101c48df8` .. `0x101c4af00`).
//! A session writes two files into a date directory:
//!
//! * `<stem>.log` — human lines, one per event, stamped with the moment,
//! * `<stem>-000.bytes` — the wire itself, rotated to `-001`, `-002`, ... after
//!   `HYDRA_LOG_LINES` lines (see [`crate::sink::config`]).
//!
//! Recovered literals: `{}.log` (0x103e7f49b), `{}-000.bytes` (0x103e7f48e),
//! `{}-{}_{}` (0x103e7f486), `{}.tmp` (0x103e7f8d3), the header block
//! (0x103e7f4a2), `recording gap ended: {} bytes of the wire are missing from
//! the .bytes file before here` (0x103e7f64e) and the `[hydra] {}: not written:
//! {}` warning (0x103e7ff32).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::sink::config;
use crate::sink::redactions::Redactions;

/// The header every session log opens with (recovered verbatim).
pub const HEADER: &str = "# Hydra session log -- PRIVATE DEVELOPMENT LOG, not automatically shareable.\n\
# Credentials registered BEFORE the first byte: {}.\n\
# The session key is registered LATER, when the connect mints it --\n\
# look for `redaction registered (session key)` below. Everything\n\
# before that line predates its redaction.\n\
# OTHER PLAYERS' NAMES ARE NOT REDACTED. Cut fixtures through\n\
# hydra_protocol::scrub::Scrubber with the names named.\n";

/// A session's log and wire recording.
pub struct SessionSink {
    /// The date directory the files live in.
    pub dir: PathBuf,
    /// The stem both files are named from.
    pub stem: String,
    log: File,
    bytes: File,
    /// Which `.bytes` file is open (`-000`, `-001`, ...).
    rotation: u32,
    /// How many lines may go into one `.bytes` file.
    rotate_after: usize,
    /// How many lines the open `.bytes` file already has.
    lines: usize,
    /// The secrets registered for masking.
    pub redactions: Redactions,
    /// Bytes that could not be written because the disk was behind.
    pub gap: u64,
}

impl SessionSink {
    /// Open a session sink with the default rotation.
    ///
    /// Recovered from `0x101c4ad50`: the files are created, the header is
    /// written, and a recording gap carried over from a previous sink is
    /// reported (`recording gap ended: {} bytes ...`, 0x103e7f64e).
    pub fn create(dir: &Path, stem: &str, redactions: Redactions) -> io::Result<SessionSink> {
        SessionSink::create_with_rotation(dir, stem, redactions, config::rotate_after_lines(), 0)
    }

    /// Open a session sink, rotating the `.bytes` file every `rotate_after`
    /// lines, and report a `gap` of bytes that went unrecorded before it.
    ///
    /// Recovered from `0x101c498a0`.
    pub fn create_with_rotation(
        dir: &Path,
        stem: &str,
        redactions: Redactions,
        rotate_after: usize,
        gap: u64,
    ) -> io::Result<SessionSink> {
        std::fs::create_dir_all(dir)?;
        let log_path = dir.join(format!("{}.log", stem));
        let mut log = OpenOptions::new().create(true).append(true).open(&log_path)?;
        let bytes_path = bytes_path(dir, stem, 0);
        let bytes = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&bytes_path)?;
        let registered = if redactions.is_empty() {
            "none".to_string()
        } else {
            format!("{} entries", redactions_count(&redactions))
        };
        let mut sink = SessionSink {
            dir: dir.to_path_buf(),
            stem: stem.to_string(),
            log,
            bytes,
            rotation: 0,
            rotate_after,
            lines: 0,
            redactions,
            gap,
        };
        sink.log.write_all(HEADER.replacen("{}", &registered, 1).as_bytes())?;
        if gap > 0 {
            sink.log.write_all(
                format!(
                    "recording gap ended: {} bytes of the wire are missing from the .bytes \
                     file before here\n",
                    gap
                )
                .as_bytes(),
            )?;
        }
        Ok(sink)
    }

    /// Note one log line, stamped with the moment it happened.
    ///
    /// Recovered from `0x101c4ac04`: `{} {}\n` (0x103e23ba0) — the line time and
    /// the text, with every registered secret masked.
    pub fn event(&mut self, text: &str) -> io::Result<()> {
        let masked = self.redactions.apply(text);
        let line = format!("{} {}\n", config::line_time(), masked);
        self.log.write_all(line.as_bytes())
    }

    /// Record wire bytes, masked, rotating the file when it is full.
    ///
    /// Recovered from `0x101c4a3f4`: the recording is prefixed with `{}: `
    /// (0x103e2ac33) — the moment and a colon — and the bytes themselves are the
    /// masked wire.
    pub fn wire(&mut self, direction: char, bytes: &[u8]) -> io::Result<()> {
        let masked = self.redactions.apply_bytes_after(bytes, 0);
        writeln!(self.bytes, "{} {}: ", config::line_time(), direction)?;
        self.bytes.write_all(&masked)?;
        self.lines += 1;
        if self.rotate_after > 0 && self.lines >= self.rotate_after {
            self.rotate()?;
        }
        Ok(())
    }

    /// Close the current `.bytes` file and open the next one.
    fn rotate(&mut self) -> io::Result<()> {
        self.bytes.flush()?;
        self.rotation += 1;
        self.lines = 0;
        let path = bytes_path(&self.dir, &self.stem, self.rotation);
        self.bytes = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(())
    }

    /// Register the session key so later bytes are masked.
    ///
    /// Recovered from `0x101c48df8`: the key is registered with the mask and the
    /// log carries `{}: ` (0x103e2ac33) plus the registration note, so a reader
    /// can tell where the redaction starts.
    pub fn redact_key(&mut self, key: &[u8]) -> io::Result<()> {
        self.redactions
            .add(key, crate::sink::redactions::KEY_MASK);
        self.event(&format!("{}: redaction registered (session key)", config::line_time()))
    }

    /// Take the files back out of the sink, for a caller that wants to own them.
    ///
    /// Recovered from `0x101c4af00`: the sink is consumed and the paths are
    /// handed over.
    pub fn hand_over(self) -> (PathBuf, PathBuf) {
        let log = self.dir.join(format!("{}.log", self.stem));
        let bytes = bytes_path(&self.dir, &self.stem, self.rotation);
        (log, bytes)
    }

    /// The path of the `.bytes` file currently open.
    pub fn bytes_path(&self) -> PathBuf {
        bytes_path(&self.dir, &self.stem, self.rotation)
    }

    /// The path of the `.log` file.
    pub fn log_path(&self) -> PathBuf {
        self.dir.join(format!("{}.log", self.stem))
    }
}

fn redactions_count(redactions: &Redactions) -> usize {
    // `is_empty` is the only accessor the sink needs; the count is for the
    // header line only.
    if redactions.is_empty() {
        0
    } else {
        redactions.len()
    }
}

fn bytes_path(dir: &Path, stem: &str, rotation: u32) -> PathBuf {
    dir.join(format!("{}-{:03}.bytes", stem, rotation))
}

/// The stem a session's files are named from: date, character and instance.
///
/// Recovered from the literal `{}-{}_{}` (0x103e7f486).
pub fn stem(date: &str, character: &str, instance: &str) -> String {
    format!("{}-{}_{}", date, character, instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::redactions::Redactions;

    fn scratch(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("hydra-net-sink-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_session_writes_a_log_and_a_wire_file() {
        let dir = scratch("basic");
        let mut sink = SessionSink::create(&dir, "2024-03-10-Dridarn_GS4", Redactions::new()).unwrap();
        sink.event("connected").unwrap();
        sink.wire('>', b"<c>look</c>").unwrap();

        let log = std::fs::read_to_string(sink.log_path()).unwrap();
        assert!(log.starts_with("# Hydra session log -- PRIVATE DEVELOPMENT LOG"), "{log}");
        assert!(log.contains("none."), "the header names the registered credentials: {log}");
        assert!(log.contains("connected"), "{log}");

        let bytes = std::fs::read(sink.bytes_path()).unwrap();
        assert!(bytes.ends_with(b"<c>look</c>"), "{:?}", String::from_utf8_lossy(&bytes));
        assert!(String::from_utf8_lossy(&bytes).contains(">: "));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_wire_file_rotates_on_the_line_count() {
        let dir = scratch("rotate");
        let mut sink =
            SessionSink::create_with_rotation(&dir, "session", Redactions::new(), 2, 0).unwrap();
        assert!(sink.bytes_path().ends_with("session-000.bytes"));
        sink.wire('>', b"one").unwrap();
        sink.wire('>', b"two").unwrap();
        assert!(sink.bytes_path().ends_with("session-001.bytes"), "{:?}", sink.bytes_path());
        sink.wire('>', b"three").unwrap();
        assert_eq!(
            std::fs::read(dir.join("session-000.bytes")).unwrap().len() > 0,
            true
        );
        let second = std::fs::read(dir.join("session-001.bytes")).unwrap();
        assert!(second.ends_with(b"three"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_registered_key_is_masked_in_later_bytes() {
        let dir = scratch("redact");
        let mut sink = SessionSink::create(&dir, "session", Redactions::new()).unwrap();
        sink.wire('>', b"key=secretkey").unwrap();
        sink.redact_key(b"secretkey").unwrap();
        sink.wire('>', b"key=secretkey").unwrap();
        let bytes = std::fs::read(sink.bytes_path()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert_eq!(text.matches("secretkey").count(), 1, "{text}");
        assert!(text.contains("<session key>"), "{text}");
        let log = std::fs::read_to_string(sink.log_path()).unwrap();
        assert!(log.contains("redaction registered (session key)"), "{log}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_gap_is_reported_in_the_header() {
        let dir = scratch("gap");
        let sink = SessionSink::create_with_rotation(&dir, "session", Redactions::new(), 10, 4096)
            .unwrap();
        let log = std::fs::read_to_string(sink.log_path()).unwrap();
        assert!(
            log.contains("recording gap ended: 4096 bytes of the wire are missing"),
            "{log}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_stem_joins_date_character_and_instance() {
        assert_eq!(stem("2024-03-10", "Dridarn", "GS4"), "2024-03-10-Dridarn_GS4");
    }

    #[test]
    fn hand_over_names_the_files_it_wrote() {
        let dir = scratch("handover");
        let sink = SessionSink::create(&dir, "session", Redactions::new()).unwrap();
        let (log, bytes) = sink.hand_over();
        assert!(log.ends_with("session.log"));
        assert!(bytes.ends_with("session-000.bytes"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
