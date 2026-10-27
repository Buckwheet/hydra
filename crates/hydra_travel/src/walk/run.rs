//! A run: a batch of jobs a walk does in one room.
//!
//! Recovered from `hydra_travel::walk::run::{Run::tick, Run::work, said,
//! filled}` (`0x10155adf4`, `0x10155dcb0`, `0x10155fbf8`, `0x10156040c`) and
//! the recovered types `run::{Job, Out, Deed, Pick, Until}`
//! (`drop_in_place` symbols at `0x1015626c0`..`0x101562be4`).

/// When a job is done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Until {
    /// After this many tries.
    Tries(u32),
    /// When this line is seen.
    Said(String),
    /// When the room changes.
    Rung,
}

/// What a job does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// The command to send.
    pub command: String,
    /// Whether the hands have to be free first.
    pub hands_free: bool,
    /// When to stop trying.
    pub until: Until,
}

/// The answer of one turn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Out {
    pub said: Vec<String>,
    pub done: bool,
}

/// A run of jobs.
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub jobs: Vec<Job>,
    pub at: usize,
    pub tries: u32,
}

impl Run {
    /// A run of one command.
    ///
    /// `walk::run::filled` (2620 bytes at `0x10156040c`) fills a run from the
    /// recovered `Race`/`See`/`then`/`Rung`/`Exits`/`Are`/`Unported` words
    /// (`0x10241e560`).
    pub fn filled(commands: &[&str]) -> Run {
        Run {
            jobs: commands
                .iter()
                .map(|command| Job {
                    command: (*command).to_string(),
                    hands_free: false,
                    until: Until::Rung,
                })
                .collect(),
            at: 0,
            tries: 0,
        }
    }

    /// The next command (`Run::tick`, `0x10155adf4`).
    pub fn tick(&mut self) -> Option<String> {
        let job = self.jobs.get(self.at)?;
        self.tries += 1;
        Some(job.command.clone())
    }

    /// Take a line of output in (`Run::work`, `0x10155dcb0`).
    pub fn work(&mut self, line: &str) -> Out {
        let Some(job) = self.jobs.get(self.at).cloned() else {
            return Out {
                said: vec![],
                done: true,
            };
        };
        let stop = match &job.until {
            Until::Tries(n) => self.tries >= *n,
            Until::Said(needle) => line.contains(needle.as_str()),
            Until::Rung => line.contains("Obvious exits") || line.contains("Obvious paths"),
        };
        if stop {
            self.at += 1;
            self.tries = 0;
        }
        Out {
            said: vec![],
            done: self.at >= self.jobs.len(),
        }
    }
}

/// The line a job prints when it is done (`run::said`, 2380 bytes at
/// `0x10155fbf8`).
pub fn said(done: &str) -> String {
    done.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_walks_its_jobs() {
        let mut run = Run::filled(&["look", "go gate"]);
        assert_eq!(run.tick().as_deref(), Some("look"));
        let out = run.work("Obvious exits: north");
        assert!(!out.done);
        assert_eq!(run.tick().as_deref(), Some("go gate"));
        let out = run.work("Obvious paths: none");
        assert!(out.done);
    }

    #[test]
    fn a_said_job_waits_for_its_line() {
        let mut run = Run {
            jobs: vec![Job {
                command: "open gate".into(),
                hands_free: false,
                until: Until::Said("creak loudly".into()),
            }],
            ..Run::default()
        };
        assert!(!run.work("The gate is locked.").done);
        assert!(run.work("The hinges creak loudly as they give way").done);
        assert_eq!(said("done"), "done");
    }
}
