//! Named streams (`<stream id="..">`) and their scrollback.
//!
//! Recovered: `Streams::{get, keep, last, take_last}` plus the `Kept` type, and
//! `story::streams::own_streams` @0x1029 61a4 (the predicate that decides
//! whether a stream belongs to the current window).  A stream holds
//! `Vec<Runs>`; the drop glue shows `Vec<Kept>`.

use std::collections::BTreeMap;

use hydra_protocol::runs::Runs;

/// One kept stream line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kept {
    /// The stream's id, e.g. `"thoughts"`.
    pub id: String,
    /// The plain text of the line.
    pub text: String,
}

/// The stream scrollback.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Streams {
    lines: BTreeMap<String, Vec<Runs>>,
    kept: Vec<Kept>,
}

impl Streams {
    pub fn new() -> Streams {
        Streams::default()
    }

    /// `@0x1018d30f4` — the lines of a stream, creating it on demand.
    pub fn get(&mut self, id: &str) -> &mut Vec<Runs> {
        self.lines.entry(id.to_string()).or_default()
    }

    /// `@0x1018d3a68` — trim a stream to its last `keep` lines.
    pub fn keep(&mut self, id: &str, keep: usize) {
        if let Some(list) = self.lines.get_mut(id) {
            let keep = keep.max(1);
            if list.len() > keep {
                let drop = list.len() - keep;
                list.drain(..drop);
            }
        }
    }

    /// `@0x1018d3d64` — the newest line of a stream.
    pub fn last(&self, id: &str) -> Option<&Runs> {
        self.lines.get(id).and_then(|l| l.last())
    }

    /// `@0x1018d3f30` — take the newest line out of a stream.
    pub fn take_last(&mut self, id: &str) -> Option<Runs> {
        self.lines.get_mut(id).and_then(|l| l.pop())
    }

    /// The stream ids, sorted.
    pub fn ids(&self) -> Vec<&str> {
        self.lines.keys().map(|s| s.as_str()).collect()
    }

    /// The number of lines in a stream.
    pub fn len(&self, id: &str) -> usize {
        self.lines.get(id).map(|l| l.len()).unwrap_or(0)
    }

    /// `true` when no stream has anything.
    pub fn is_empty(&self) -> bool {
        self.lines.values().all(|l| l.is_empty())
    }

    /// The flat "kept" view the window shows.
    pub fn kept(&self) -> &[Kept] {
        &self.kept
    }

    /// `@0x102961a4` — push a line onto a stream and onto the kept view.
    pub fn own_streams(&mut self, id: &str, runs: Runs) {
        let text = runs.plain();
        self.get(id).push(runs);
        self.kept.push(Kept {
            id: id.to_string(),
            text,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_protocol::runs::Run;

    #[test]
    fn streams_are_created_on_demand() {
        let mut s = Streams::new();
        assert!(s.is_empty());
        s.get("thoughts").push(Runs::from_vec(vec![Run::new("hmm")]));
        assert_eq!(s.len("thoughts"), 1);
        assert_eq!(s.len("nope"), 0);
        assert_eq!(s.ids(), vec!["thoughts"]);
    }

    #[test]
    fn keep_trims() {
        let mut s = Streams::new();
        for i in 0..5 {
            s.get("x").push(Runs::from_vec(vec![Run::new(i.to_string())]));
        }
        s.keep("x", 2);
        assert_eq!(s.len("x"), 2);
        assert_eq!(s.last("x").unwrap().plain(), "4");
    }

    #[test]
    fn last_and_take_last() {
        let mut s = Streams::new();
        s.get("x").push(Runs::from_vec(vec![Run::new("a")]));
        s.get("x").push(Runs::from_vec(vec![Run::new("b")]));
        assert_eq!(s.last("x").unwrap().plain(), "b");
        assert_eq!(s.take_last("x").unwrap().plain(), "b");
        assert_eq!(s.len("x"), 1);
        assert_eq!(s.take_last("nope"), None);
    }

    #[test]
    fn own_streams_keeps_a_flat_view() {
        let mut s = Streams::new();
        s.own_streams("thoughts", Runs::from_vec(vec![Run::new("hmm")]));
        assert_eq!(s.kept().len(), 1);
        assert_eq!(s.kept()[0].id, "thoughts");
        assert_eq!(s.kept()[0].text, "hmm");
    }
}
