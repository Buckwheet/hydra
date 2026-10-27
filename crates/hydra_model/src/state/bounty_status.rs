//! The `BOUNTY` status block.
//!
//! `read_line` (860 B) literals: `" expedited task reassignment vouchers
//! remaining"`, `"says, \"All done with that assignment?"` — and it calls
//! `bounty::classify` plus `classify_refusal` (928 B).
//! `stream_said` (224 B) / `stream_cleared` (80 B) / `clear` (256 B) maintain
//! the block.

use super::bounty::Task;

/// The current bounty state.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct BountyStatus {
    pub task: Option<Task>,
    /// `"<n> expedited task reassignment vouchers remaining"`.
    pub vouchers: Option<u32>,
    /// `true` while the taskmaster asks for a report.
    pub awaiting_report: bool,
    /// The refusal text, when the taskmaster refused.
    pub refusal: Option<Refusal>,
}

/// Why a bounty was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Refusal {
    NotEnoughTasks,
    TooManyTasks,
    WrongTown,
    NotReady,
    Unknown,
}

/// @0x0000000101b6c878 (928 B)
pub fn classify_refusal(text: &str) -> Option<Refusal> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("you are not in the market for a task")
        || lower.contains("you already have a task")
    {
        Some(Refusal::TooManyTasks)
    } else if lower.contains("you must complete your current task") {
        Some(Refusal::NotReady)
    } else if lower.contains("you do not have enough tasks") {
        Some(Refusal::NotEnoughTasks)
    } else if lower.contains("not from this town") {
        Some(Refusal::WrongTown)
    } else if lower.contains("taskmaster") {
        Some(Refusal::Unknown)
    } else {
        None
    }
}

impl BountyStatus {
    /// @0x0000000101b6c41c (256 B)
    pub fn clear(&mut self) -> bool {
        let changed = self.task.is_some() || self.awaiting_report || self.refusal.is_some();
        self.task = None;
        self.awaiting_report = false;
        self.refusal = None;
        changed
    }

    /// @0x0000000101b6c51c (860 B)
    pub fn read_line(&mut self, line: &str) -> bool {
        if let Some((before, _)) = line.split_once(" expedited task reassignment vouchers remaining") {
            let vouchers = before
                .split_whitespace()
                .last()
                .and_then(|n| n.parse().ok());
            if self.vouchers == vouchers {
                return false;
            }
            self.vouchers = vouchers;
            return true;
        }
        if line.contains("says, \"All done with that assignment?") {
            if self.awaiting_report {
                return false;
            }
            self.awaiting_report = true;
            return true;
        }
        if let Some(refusal) = classify_refusal(line) {
            if self.refusal == Some(refusal) {
                return false;
            }
            self.refusal = Some(refusal);
            return true;
        }
        if let Some(task) = super::bounty::classify(line) {
            if self.task.as_ref() == Some(&task) {
                return false;
            }
            self.task = Some(task);
            return true;
        }
        false
    }

    /// @0x0000000101b6c2ec (224 B)
    pub fn stream_said(&mut self, line: &str) -> bool {
        self.read_line(line)
    }

    /// @0x0000000101b6c3cc (80 B)
    pub fn stream_cleared(&mut self) -> bool {
        self.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vouchers_are_read_from_the_recovered_sentence() {
        let mut status = BountyStatus::default();
        assert!(status.read_line("You have 7 expedited task reassignment vouchers remaining."));
        assert_eq!(status.vouchers, Some(7));
        assert!(!status.read_line("You have 7 expedited task reassignment vouchers remaining."));
    }

    #[test]
    fn the_taskmaster_report_is_recorded() {
        let mut status = BountyStatus::default();
        assert!(status.read_line("The taskmaster says, \"All done with that assignment?\""));
        assert!(status.awaiting_report);
        assert!(!status.read_line("The taskmaster says, \"All done with that assignment?\""));
    }

    #[test]
    fn refusals_are_classified() {
        assert_eq!(
            classify_refusal("You already have a task."),
            Some(Refusal::TooManyTasks)
        );
        assert_eq!(classify_refusal("Nice weather."), None);
        let mut status = BountyStatus::default();
        assert!(status.read_line("You already have a task."));
        assert_eq!(status.refusal, Some(Refusal::TooManyTasks));
    }

    #[test]
    fn clear_resets_and_reports_change() {
        let mut status = BountyStatus::default();
        assert!(!status.clear());
        status.read_line("You already have a task.");
        assert!(status.clear());
        assert_eq!(status.refusal, None);
        assert!(!status.stream_cleared());
    }
}
