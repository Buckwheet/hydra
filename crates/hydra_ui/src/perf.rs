//! Timing instrumentation.
//!
//! `Perf::stages` @0x10195e9dc returns the stage table; the recovered names and
//! descriptions are one packed literal at 0x103994328:
//!
//! ```text
//! Line   "The game's text arriving, to it on the screen.  Feed"
//! feed   "Of that: the text arriving, to the window having it.  Wait"
//! wait   "Of that: the window having it, to the window starting to draw."
//! Lich   "The game's text arriving, to Lich passing it back: Lich's own time."
//! sent   "You press Enter, to the line written to the game (through Lich, when it runs)."
//! reply  "Lag: the line written, to the game's answer arriving. The network and the game, not Hydra."
//! screen "The game's answer arriving, to it on the screen."
//! Paint  "Drawing the play window, once."
//! Frame  "Drawing every window once: the hub, each play window, the rest."
//! ```
//!
//! `Perf::collect` @0x10195ec54 records one sample, `Perf::log_header`
//! @0x10195e4a4 and `Perf::log_row` @0x10195ee08 write the CSV log, and
//! `perf::took` @0x10195f430 formats a duration.

use std::time::Duration;

/// One instrumentation stage: `(id, label, description)`.
pub const STAGES: [(&str, &str, &str); 8] = [
    (
        "feed",
        "Line",
        "The game's text arriving, to it on the screen.  Feed",
    ),
    (
        "wait",
        "Wait",
        "Of that: the text arriving, to the window having it.  Wait",
    ),
    (
        "lich",
        "Lich",
        "Of that: the window having it, to the window starting to draw.",
    ),
    (
        "sent",
        "sent",
        "You press Enter, to the line written to the game (through Lich, when it runs).",
    ),
    (
        "reply",
        "reply",
        "Lag: the line written, to the game's answer arriving. The network and the game, not Hydra.",
    ),
    (
        "screen",
        "screen",
        "The game's answer arriving, to it on the screen.",
    ),
    ("paint", "Paint", "Drawing the play window, once."),
    (
        "frame",
        "Frame",
        "Drawing every window once: the hub, each play window, the rest.",
    ),
];

/// `@0x10195f430` — format a duration the way the log does.
///
/// Microseconds below a millisecond, milliseconds below a second, seconds
/// above; at most two decimals.
pub fn took(d: Duration) -> String {
    let us = d.as_micros();
    if us < 1_000 {
        format!("{us}us")
    } else if us < 1_000_000 {
        format!("{:.2}ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2}s", us as f64 / 1_000_000.0)
    }
}

/// A rolling window of samples for one stage.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Samples {
    values: Vec<f64>,
    max: usize,
}

impl Samples {
    /// A window holding `max` samples (at least one).
    pub fn new(max: usize) -> Samples {
        Samples {
            values: Vec::new(),
            max: max.max(1),
        }
    }

    /// `@0x10195f6a0` — record a sample in milliseconds.
    pub fn push(&mut self, ms: f64) {
        if self.values.len() == self.max {
            self.values.remove(0);
        }
        self.values.push(ms);
    }

    /// `@0x10195f790` — the (min, max) over the whole window.
    pub fn spread(&self) -> (f64, f64) {
        self.values.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        })
    }

    /// `@0x10195f4f4` — the (min, max) over the last `n` samples.
    pub fn last_spread(&self, n: usize) -> (f64, f64) {
        let n = n.max(1).min(self.values.len());
        let tail = &self.values[self.values.len() - n..];
        tail.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| {
            (lo.min(*v), hi.max(*v))
        })
    }

    /// How many samples are held.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` when nothing has been sampled.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// The whole instrumentation set.
#[derive(Clone, Debug, PartialEq)]
pub struct Perf {
    stages: Vec<Samples>,
}

impl Default for Perf {
    fn default() -> Self {
        Perf {
            stages: vec![Samples::new(240); STAGES.len()],
        }
    }
}

impl Perf {
    pub fn new() -> Perf {
        Perf::default()
    }

    /// `@0x10195e9dc` — the stage table.
    pub fn stages() -> Vec<(&'static str, &'static str, &'static str)> {
        STAGES.to_vec()
    }

    /// `@0x10195ec54` — record one sample for a stage id.
    ///
    /// Unknown ids are ignored (the binary indexes the table directly).
    pub fn collect(&mut self, stage: &str, ms: f64) {
        if let Some(i) = STAGES.iter().position(|(id, _, _)| *id == stage) {
            self.stages[i].push(ms);
        }
    }

    /// The samples for one stage.
    pub fn samples(&self, stage: &str) -> Option<&Samples> {
        STAGES
            .iter()
            .position(|(id, _, _)| *id == stage)
            .map(|i| &self.stages[i])
    }

    /// `@0x10195e4a4` — the CSV header line.
    pub fn log_header() -> String {
        let mut out = String::from("at");
        for (id, _, _) in STAGES {
            out.push(',');
            out.push_str(id);
        }
        out.push('\n');
        out
    }

    /// `@0x10195ee08` — one CSV row: the last sample of every stage.
    pub fn log_row(&self) -> String {
        let mut out = String::new();
        for (i, (id, _, _)) in STAGES.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = id;
            match self.stages[i].values.last() {
                Some(v) => out.push_str(&format!("{v:.3}")),
                None => out.push_str(""),
            }
        }
        out.push('\n');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_table_is_the_recovered_one() {
        let stages = Perf::stages();
        assert_eq!(stages.len(), 8);
        assert_eq!(
            stages.iter().map(|s| s.0).collect::<Vec<_>>(),
            vec!["feed", "wait", "lich", "sent", "reply", "screen", "paint", "frame"]
        );
        assert_eq!(stages[0].1, "Line");
        assert!(stages[0].2.starts_with("The game's text arriving"));
    }

    #[test]
    fn took_formats_three_scales() {
        assert_eq!(took(Duration::from_micros(999)), "999us");
        assert_eq!(took(Duration::from_millis(1)), "1.00ms");
        assert_eq!(took(Duration::from_micros(1500)), "1.50ms");
        assert_eq!(took(Duration::from_millis(1500)), "1.50s");
    }

    #[test]
    fn samples_keep_a_window() {
        let mut s = Samples::new(3);
        assert!(s.is_empty());
        for v in [1.0, 2.0, 3.0, 10.0] {
            s.push(v);
        }
        assert_eq!(s.len(), 3);
        assert_eq!(s.spread(), (2.0, 10.0));
        assert_eq!(s.last_spread(1), (10.0, 10.0));
        assert_eq!(s.last_spread(2), (3.0, 10.0));
    }

    #[test]
    fn collect_and_log() {
        let mut perf = Perf::new();
        perf.collect("feed", 1.5);
        perf.collect("frame", 9.0);
        perf.collect("nope", 3.0);
        assert_eq!(perf.samples("feed").unwrap().spread(), (1.5, 1.5));
        assert!(perf.samples("nope").is_none());
        assert_eq!(
            Perf::log_header(),
            "at,feed,wait,lich,sent,reply,screen,paint,frame\n"
        );
        assert_eq!(perf.log_row(), "1.500,,,,,,,9.000\n");
    }
}
