//! `hydra import`: recordings into the ledger.
//!
//! The recovered handler is `hydra::main::{{closure}}` (`0x1000236b8`): it takes
//! the data folder ([`hydra_table::account::data_dir`]), opens the ledger at
//! [`hydra_ledger::Ledger::path_in`] ([`hydra_ledger::Ledger::open`]), walks the
//! recordings with [`hydra_import::sessions`], records them, and prints the one
//! recovered summary line:
//!
//! ```text
//! {sessions} sessions read, {refused} refused: {hunts} hunts and {pulses} pulses added; {rewards} login rewards found.
//! ```
//!
//! (`__const` 0x103e218xx; the pieces are `{} sessions read, {} refused: {} hunts
//! and {} pulses added; {} login rewards found.`.)  The per-recording messages
//! are the recovered `: imported before; skipped.` and
//! `-Not characters Hydra knows, so not imported: `.
//!
//! # What this reconstruction records
//!
//! `hydra_import::sessions` is the recovered grouping: one [`Session`] per
//! `(name, stamp)` with its rotation files.  The original's `import_into` also
//! replayed the XML to count hunts, pulses and login rewards; that replay is not
//! reconstructed here, so the counters for a *new* recording are the row count
//! and zero.  Every file's metadata (instance, character, stamp, path, size and
//! the `span_of` time range) is recorded through the real
//! [`hydra_ledger::Ledger::note_import`], which is the recovered ledger call.
//!
//! A file that is already in the ledger is reported with the recovered
//! `: imported before; skipped.` and is not a failure.  A session whose
//! recording does not name a known instance is left out with the recovered
//! "not characters Hydra knows" message.

use std::io;
use std::path::PathBuf;

use hydra_import::Session;
use hydra_ledger::{Ledger, Who};

/// What one `hydra import` did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Sessions `hydra_import::sessions` returned.
    pub sessions: usize,
    /// Recordings that were skipped (unreadable, unknown instance, duplicate).
    pub refused: usize,
    /// Hunt rows added (0: the replay is not reconstructed).
    pub hunts: usize,
    /// Pulse rows added (0: the replay is not reconstructed).
    pub pulses: usize,
    /// Login-reward rows found (0: the replay is not reconstructed).
    pub rewards: usize,
}

impl Report {
    /// The recovered summary line.
    pub fn summary(&self) -> String {
        format!(
            "{} sessions read, {} refused: {} hunts and {} pulses added; {} login rewards found.",
            self.sessions, self.refused, self.hunts, self.pulses, self.rewards
        )
    }
}

/// `hydra import <file.bytes|folder>...`.
pub fn import(paths: &[String]) -> Result<(), String> {
    let data_dir = hydra_table::account::data_dir();
    let ledger = Ledger::open(&data_dir)
        .map_err(|error| format!("the ledger could not be opened: {error}"))?;

    // No paths means the player logs: the same recordings `lookout seed` reads.
    let paths: Vec<PathBuf> = if paths.is_empty() {
        vec![hydra_net::sink::config::log_dir()]
    } else {
        paths.iter().map(PathBuf::from).collect()
    };
    let sessions = hydra_import::sessions(&paths);
    let (report, notices) = import_into(&ledger, &sessions);
    for notice in notices {
        println!("{notice}");
    }
    println!("{}", report.summary());
    Ok(())
}

/// Record `sessions` in `ledger`, returning the counts and the per-recording
/// notices (the recovered texts above).
pub fn import_into(ledger: &Ledger, sessions: &[Session]) -> (Report, Vec<String>) {
    let mut report = Report {
        sessions: sessions.len(),
        ..Report::default()
    };
    let mut notices = Vec::new();
    let mut unknown = Vec::new();

    for session in sessions {
        // The instance is read from the first rotation file; an unreadable one
        // is refused rather than guessed at.
        let text = match session.files.first().map(|path| read_text(path)) {
            Some(Ok(text)) => text,
            _ => {
                report.refused += session.files.len().max(1);
                continue;
            }
        };
        let Some(instance) = hydra_import::instance_of(&text) else {
            unknown.push(session.name.clone());
            report.refused += session.files.len();
            continue;
        };
        let who = Who::new(instance, session.name.clone());
        let (first, last) = hydra_import::span_of(&text);
        for path in &session.files {
            // The recorded size is the file on disk, not the decoded text.
            let size = std::fs::metadata(path)
                .map(|meta| meta.len())
                .unwrap_or(text.len() as u64);
            let name = path.to_string_lossy();
            match ledger.note_import(
                &who,
                &session.stamp,
                &name,
                size,
                (first, last),
                0,
                0,
            ) {
                Ok(()) => {}
                Err(hydra_ledger::Error::Constraint(_)) => {
                    notices.push(format!("{name}: imported before; skipped."));
                    report.refused += 1;
                }
                Err(error) => {
                    notices.push(format!("{name}: {error}"));
                    report.refused += 1;
                }
            }
        }
    }

    if !unknown.is_empty() {
        notices.push(format!(
            "Not characters Hydra knows, so not imported: {}",
            unknown.join(", ")
        ));
    }
    (report, notices)
}

/// A recording is UTF-8 XML; invalid bytes are replaced rather than refusing the
/// whole file (the parser the original replayed accepts them the same way).
fn read_text(path: &std::path::Path) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> TempDir {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hydra-import-{tag}-{}-{stamp}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn recording() -> &'static str {
        "<app game=\"GS3\"/>\n<settingsInfo instance=\"GS3\"/>\n"
    }

    #[test]
    fn the_summary_is_the_recovered_line() {
        let report = Report {
            sessions: 2,
            refused: 1,
            hunts: 3,
            pulses: 4,
            rewards: 5,
        };
        assert_eq!(
            report.summary(),
            "2 sessions read, 1 refused: 3 hunts and 4 pulses added; 5 login rewards found."
        );
    }

    #[test]
    fn a_recording_is_recorded_and_a_second_import_is_skipped() {
        let dir = TempDir::new("roundtrip");
        let path = dir.0.join("GS3_Aelith-2026-10-27_10-00-00-000.bytes");
        std::fs::write(&path, recording()).unwrap();
        let sessions = hydra_import::sessions(&[path.clone()]);
        assert_eq!(sessions.len(), 1);

        let ledger = Ledger::open(dir.0.join("data")).unwrap();
        let (report, notices) = import_into(&ledger, &sessions);
        assert_eq!(report.sessions, 1);
        assert_eq!(report.refused, 0);
        assert!(notices.is_empty(), "{notices:?}");

        let (report, notices) = import_into(&ledger, &sessions);
        assert_eq!(report.refused, 1);
        assert!(notices[0].contains("imported before; skipped."), "{notices:?}");
    }

    #[test]
    fn a_recording_with_no_known_instance_is_not_imported() {
        let dir = TempDir::new("unknown");
        let path = dir.0.join("GS3_Aelith-2026-10-27_10-00-00-000.bytes");
        std::fs::write(&path, "no instance here").unwrap();
        let sessions = hydra_import::sessions(&[path]);
        let ledger = Ledger::open(dir.0.join("data")).unwrap();
        let (report, notices) = import_into(&ledger, &sessions);
        assert_eq!(report.refused, 1);
        assert!(notices
            .iter()
            .any(|notice| notice.starts_with("Not characters Hydra knows")));
    }
}
