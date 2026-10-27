//! Keeping the disk from falling behind the wire.
//!
//! Recovered from `hydra_net::behind` (`0x101c60da8` .. `0x101c62348`).  A session
//! writes everything it reads to disk; when the disk cannot keep up the hydra
//! says so and drops what comes until it has caught up, rather than growing a
//! queue without bound.
//!
//! Recovered messages:
//! * `[hydra] {}: the disk is too far behind; what more comes is left out until it
//!   catches up\n` (0x103e7fdc1)
//! * `[hydra] {}: no thread to write it: {}\n` (0x103e7fe1b)

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// How many bytes may be outstanding before the recording gives up on them.
pub const DEFAULT_LIMIT: u64 = 8 * 1024 * 1024;

/// How far behind the writer is, and how much has been dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Behind {
    /// Bytes handed over that are not written yet.
    pub outstanding: u64,
    /// Bytes dropped because the writer fell too far behind.
    pub skipped: u64,
    /// Bytes accepted since the last settle.
    pub accepted: u64,
    /// The point at which recording gives up until it catches up.
    pub limit: u64,
    /// True once the writer has been told to stop taking more.
    pub giving_up: bool,
}

impl Behind {
    /// A tracker with a limit.
    pub fn new(limit: u64) -> Behind {
        Behind {
            limit,
            ..Behind::default()
        }
    }

    /// Note that `bytes` more were handed to the writer.
    ///
    /// Returns the warning to print when the writer has fallen too far behind
    /// (once: the flag is sticky until [`Behind::settle`] catches up).
    pub fn queue(&mut self, bytes: u64, label: &str) -> Option<String> {
        self.outstanding += bytes;
        if self.outstanding > self.limit {
            if self.giving_up {
                self.skipped += bytes;
                return None;
            }
            self.giving_up = true;
            self.skipped += bytes;
            return Some(format!(
                "[hydra] {}: the disk is too far behind; what more comes is left out until it \
                 catches up\n",
                label
            ));
        }
        self.accepted += bytes;
        None
    }

    /// Note that `bytes` reached the disk; returns what is still outstanding.
    pub fn settle(&mut self, bytes: u64) -> u64 {
        self.outstanding = self.outstanding.saturating_sub(bytes);
        if self.outstanding <= self.limit / 2 {
            self.giving_up = false;
        }
        self.outstanding
    }

    /// True while the recording is dropping what comes.
    pub fn is_dropping(&self) -> bool {
        self.giving_up
    }
}

/// Settle every tracker, returning the total still outstanding.
///
/// Recovered from `0x101c60da8`.
pub fn settle_all(behinds: &mut [Behind]) -> u64 {
    let mut outstanding = 0;
    for behind in behinds.iter_mut() {
        outstanding += behind.settle(0);
    }
    outstanding
}

/// State a session's recording shares with the rest of Hydra.
#[derive(Debug, Default)]
pub struct Shared {
    ended: AtomicBool,
    behind: Mutex<Behind>,
}

impl Shared {
    /// Fresh state with the default limit.
    pub fn new(limit: u64) -> Arc<Shared> {
        Arc::new(Shared {
            ended: AtomicBool::new(false),
            behind: Mutex::new(Behind::new(limit)),
        })
    }

    /// True once the session is over and nothing more will be written.
    ///
    /// Recovered from `0x101c618b4`: all the queued work has been settled and
    /// the writer has stopped.
    pub fn ended(&self) -> bool {
        self.ended.load(Ordering::Acquire)
    }

    /// Say that the session is over.
    pub fn end(&self) {
        self.ended.store(true, Ordering::Release);
    }

    /// The lag tracker behind the shared state.
    pub fn behind(&self) -> std::sync::MutexGuard<'_, Behind> {
        self.behind.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A recording thread and the channel into it.
pub struct Live {
    label: String,
    sender: Option<Sender<Vec<u8>>>,
    handle: Option<JoinHandle<u64>>,
    pub shared: Arc<Shared>,
}

impl Live {
    /// Start a recording thread that writes everything it is given to `sink`.
    ///
    /// Recovered from `0x101c60f84`: the thread is spawned with the sink and a
    /// limit; when it cannot be spawned the recording is marked ended and the
    /// caller is told `[hydra] {}: no thread to write it: {}`.
    pub fn new(
        label: &str,
        mut sink: Box<dyn Write + Send>,
        limit: u64,
    ) -> Result<Live, String> {
        let shared = Shared::new(limit);
        let (sender, receiver) = mpsc::channel::<Vec<u8>>();
        let thread_shared = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name("hydra-record".to_string())
            .spawn(move || {
                let mut written = 0u64;
                while let Ok(bytes) = receiver.recv() {
                    if sink.write_all(&bytes).is_err() {
                        break;
                    }
                    written += bytes.len() as u64;
                    thread_shared.behind().settle(bytes.len() as u64);
                }
                let _ = sink.flush();
                written
            })
            .map_err(|error| {
                shared.end();
                format!("[hydra] {}: no thread to write it: {}\n", label, error)
            })?;
        Ok(Live {
            label: label.to_string(),
            sender: Some(sender),
            handle: Some(handle),
            shared,
        })
    }

    /// Hand bytes to the recording thread.
    ///
    /// Returns the warning to print when the disk has fallen too far behind, or
    /// `None` when the bytes were taken.
    pub fn write(&self, bytes: &[u8]) -> Option<String> {
        let warning = self.shared.behind().queue(bytes.len() as u64, &self.label);
        if self.shared.behind().is_dropping() {
            return warning;
        }
        match &self.sender {
            Some(sender) if sender.send(bytes.to_vec()).is_ok() => warning,
            _ => {
                self.shared.end();
                warning.or_else(|| {
                    Some(format!(
                        "[hydra] {}: no thread to write it: {}",
                        self.label, "the recording thread has stopped"
                    ))
                })
            }
        }
    }

    /// Stop taking bytes, wait for the writer and report how much reached disk.
    pub fn finish(mut self) -> u64 {
        self.sender = None;
        self.shared.end();
        match self.handle.take() {
            Some(handle) => handle.join().unwrap_or(0),
            None => 0,
        }
    }

    /// The label the warnings carry.
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_behind_tracker_notices_the_disk_falling_behind() {
        let mut behind = Behind::new(10);
        assert_eq!(behind.queue(4, "log"), None);
        assert_eq!(behind.outstanding, 4);
        behind.settle(4);
        assert_eq!(behind.outstanding, 0);

        assert!(behind.queue(11, "log").is_some());
        assert!(behind.is_dropping());
        assert_eq!(
            behind.skipped, 11,
            "the bytes that could not be taken are counted"
        );
        // The warning is said once, not once per write.
        assert_eq!(behind.queue(1, "log"), None);
        assert_eq!(behind.skipped, 12);
        // Catching up turns recording back on.
        behind.settle(12);
        assert!(!behind.is_dropping());
        assert_eq!(behind.queue(1, "log"), None);
    }

    #[test]
    fn the_warning_matches_the_recovered_literal() {
        let mut behind = Behind::new(0);
        let warning = behind.queue(1, "Dridarn").unwrap();
        assert_eq!(
            warning,
            "[hydra] Dridarn: the disk is too far behind; what more comes is left out until it \
             catches up\n"
        );
    }

    #[test]
    fn settle_all_adds_up_what_is_outstanding() {
        let mut behinds = [Behind::new(100), Behind::new(100)];
        behinds[0].queue(10, "a");
        behinds[1].queue(5, "b");
        assert_eq!(settle_all(&mut behinds), 15);
        behinds[0].settle(10);
        assert_eq!(settle_all(&mut behinds), 5);
    }

    #[test]
    fn the_recording_thread_takes_bytes_and_settles_them() {
        let live = Live::new("record", Box::new(Vec::new()), DEFAULT_LIMIT).unwrap();
        assert_eq!(live.write(b"hello"), None);
        assert_eq!(live.write(b"world"), None);
        assert_eq!(live.finish(), 10);
    }

    #[test]
    fn shared_state_starts_live_and_can_end() {
        let shared = Shared::new(1024);
        assert!(!shared.ended());
        shared.end();
        assert!(shared.ended());
    }
}
