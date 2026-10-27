//! The player log: one day file per character per day.
//!
//! Recovered from `player_log/{writer, reader, links, archive, retention,
//! channel, feed}` and `player_log::{class, sender}`.
//!
//! **ESTABLISHED** file layout and formats (all from `__TEXT` literals, decoded
//! from the length-prefixed format pieces):
//!
//! ```text
//! +0x103e7bf3a   "[{}][{}] {}\n"        one log line: time, stream, text
//! +0x103e7bf4d   "{}_{}.log"            a day file name
//! +0x103e7bf48   "{}_{}"                the file key (day + character)
//! +0x103e7bf57   "{}_"                  the fallback key fragment
//! +0x103e7c12e   "{}/{}"                a day path / reader display
//! +0x103e7c133   "{}_to_{}"             an export name (from, to)
//! +0x103e7c13b   "{}.{}.partial"        the export's partial file
//! +0x103e7c43a   "{}.gz"                an archive member
//! +0x103e7c3f3   "{}\t{}\n"             a manifest line (name, size)
//! +0x103e7c432   "week-{}"              the week archive period name
//! +0x103e7c3ed   "{}: {}"               the shared error shape
//! +0x103e7c3fa   "{} did not read back as written; the day-files are kept"
//! +0x103e7c948   "not a manifest line: {}"
//! +0x103e7c960   "The next login removes {} file{} ({} KB), the days up to \
//!                 {} (the oldest {})."
//! +0x103e7c9b0   "{}:"                   (a stream name in a list)
//! ```
//!
//! **ESTABLISHED** behaviour:
//! * `writer::root` is a tail call into the endpoint configuration's `log_dir`
//!   (`hydra_net::sink::config::log_dir`), i.e. the log root is configurable;
//!   every other function takes the root (or the directory) as an argument.
//! * `writer::write` (`0x101a50f3c`) calls `date_dir`, `eastern::now`,
//!   `writer::file_key`, `writer::dir`, `DirBuilder::_create`, `dir::day_path`,
//!   `OpenOptions::_open`, `Redactions::apply`, `links::written`, and formats
//!   with `core::fmt::write` — i.e. it classifies the line, redacts it, appends
//!   it to today's day file and records a link.
//! * `writer::adopt_unscoped` (`0x101a514e8`) reads a directory, renames each
//!   entry into place and removes the now-empty directory (`remove_dir`).
//! * `archive::sweep` (`0x101a649c4`) gzips whole days into `<period>.gz`
//!   (the crate linked `flate2`, see the `flate2-1.1.10` paths in `__TEXT`),
//!   writes a manifest of `name<TAB>size` lines, verifies the archive reads
//!   back ("{} did not read back as written; the day-files are kept") and only
//!   then deletes the day files.
//! * the reader parses the same `[time][stream] text` lines and offers
//!   `tail`, `around`, `window`, `search`, `tagged`, `export`.
//! * `retention::doomed`/`preview` implement "keep N days": the preview text is
//!   the recovered sentence above.
//!
//! **Phase 1**: `flate2` is back (archives are real gzip, see [`archive`]) and
//! `regex` is back (see [`crate::pattern`]).  There is still no `chrono` or
//! `time`: [`writer::civil`] and [`crate::notes::days_from_civil`] stay.

pub mod archive;
pub mod channel;
pub mod feed;
pub mod links;
pub mod reader;
pub mod retention;
pub mod writer;

use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

/// The stream class of a log line: the text between the first `[` and the
/// closing `]` of the *second* bracket group, or `""` for a line that is not
/// in the log format.
///
/// **ESTABLISHED** shape (`0x101a58f54`, nine instructions): one split on
/// `'['`/`']'`, no allocation for the common case.
pub fn class(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('[') else {
        return "";
    };
    let Some((_time, rest)) = rest.split_once("][") else {
        return "";
    };
    match rest.split_once("] ").or_else(|| rest.split_once(']')) {
        Some((stream, _)) => stream,
        None => "",
    }
}

/// The one-line summary the session keeps for a log line.
pub fn summary(line: &str) -> String {
    match writer::parse_line(line) {
        Some(parsed) => format!("{} [{}] {}", parsed.time, parsed.stream, parsed.text),
        None => line.to_string(),
    }
}

/// The background writer: take lines from `rx` and append them to the day
/// files until the channel closes.
///
/// **ESTABLISHED** shape (`0x101a5e230`, 12 KB): a thread that owns the
/// `PlayerWriter`, locks it, writes each line, flushes on a timer, and reports
/// a write failure once on stderr (`<Stderr as Write>::write` and
/// `BufWriter::flush_buf` are both called).  We keep the thread, the lock and
/// the single stderr report.
pub fn sender(mut writer: writer::PlayerWriter, rx: Receiver<String>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reported = false;
        for line in rx {
            if let Err(e) = writer.write(&line) {
                if !reported {
                    eprintln!("{e}");
                    reported = true;
                }
            }
        }
        let _ = writer.flush();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_finds_the_stream_of_a_log_line() {
        assert_eq!(class("[10:04][speech] Aelith says, \"hello\""), "speech");
        assert_eq!(class("[23:59][combat] a kobold attacks"), "combat");
        assert_eq!(class("not a log line"), "");
        assert_eq!(
            summary("[10:04][speech] hi"),
            "10:04 [speech] hi"
        );
    }
}
