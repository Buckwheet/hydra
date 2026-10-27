//! Story "following": picking up new notice lines for the page.
//!
//! Recovered from `follow::Follow::take` (`0x10029122c`, 3016 bytes).  The
//! original walked a notice queue, compared `hydra_session::notice::Body`
//! values (a `PartialEq` impl is called from the loop), cloned the styled runs
//! out of `hydra_ui::run::StyledRun` and collected `Vec<wire::Line>`.

use crate::wire::{self, Line, TextRun};

/// What kind of notice a followed line came from.
///
/// `// UNKNOWN:` the exact `hydra_session::notice::Body` variants; the recovered
/// crate does not expose that enum, so the three bodies the page follows are
/// modelled here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    /// Normal story output.
    Story,
    /// A room/arrival stream line.
    Stream,
    /// A help or prompt line.
    Help,
}

/// One notice line waiting to be shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub body: Body,
    /// The stream id the line belongs to (`"main"` for the story window).
    pub stream: String,
    pub runs: Vec<TextRun>,
}

impl Default for Body {
    fn default() -> Body {
        Body::Story
    }
}

/// The follow cursor.
///
/// The original `Follow` carries the stream filter, a `Vec<wire::Line>` of
/// pending output and the last seen positions; here the cursor is a simple
/// count over the notice list handed to [`Follow::take`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Follow {
    /// Streams the page asked to follow; empty means "everything".
    pub streams: Vec<String>,
    /// How many notices have already been taken.
    pub seen: usize,
}

impl Follow {
    /// A follow for `streams` (empty = all streams).
    pub fn new(streams: Vec<String>) -> Follow {
        Follow { streams, seen: 0 }
    }

    fn keeps(&self, notice: &Notice) -> bool {
        self.streams.is_empty() || self.streams.iter().any(|s| s == &notice.stream)
    }

    /// Take the lines that have not been shown yet (`0x10029122c`).
    ///
    /// Recovered: the loop matches on the notice body (comparing against the
    /// bodies it follows), converts the styled runs with `wire::run` and pushes
    /// a `wire::Line` per notice into the returned `Vec`.
    pub fn take(&mut self, notices: &[Notice]) -> Vec<Line> {
        let mut out = Vec::new();
        for notice in notices.iter().skip(self.seen) {
            if !self.keeps(notice) {
                continue;
            }
            let kind = match notice.body {
                Body::Story => "story",
                Body::Stream => "stream",
                Body::Help => "help",
            };
            out.push(wire::story_line(kind, &notice.stream, &notice.runs));
        }
        self.seen = notices.len();
        out
    }
}

/// Per-stream follow state (`follow::StreamFollow` in the original).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamFollow {
    pub stream: String,
    pub seen: usize,
}

impl StreamFollow {
    /// The unsent lines of one stream.
    pub fn take(&mut self, notices: &[Notice]) -> Vec<Line> {
        let mut mine = Follow {
            streams: vec![self.stream.clone()],
            seen: self.seen,
        };
        let out = mine.take(notices);
        self.seen = notices.len();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(stream: &str, text: &str, body: Body) -> Notice {
        Notice {
            body,
            stream: stream.to_string(),
            runs: vec![TextRun {
                text: text.to_string(),
                ..TextRun::default()
            }],
        }
    }

    #[test]
    fn take_returns_only_new_lines() {
        let notices = vec![
            notice("main", "one", Body::Story),
            notice("main", "two", Body::Story),
        ];
        let mut f = Follow::default();
        let first = f.take(&notices);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].runs[0].text, "one");
        assert_eq!(first[0].kind, "story");
        // Nothing new.
        assert!(f.take(&notices).is_empty());
        // One more arrives.
        let notices = vec![
            notice("main", "one", Body::Story),
            notice("main", "two", Body::Story),
            notice("help", "three", Body::Help),
        ];
        let next = f.take(&notices);
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].runs[0].text, "three");
        assert_eq!(next[0].kind, "help");
    }

    #[test]
    fn stream_filter_is_honoured() {
        let notices = vec![
            notice("main", "one", Body::Story),
            notice("thoughts", "two", Body::Stream),
        ];
        let mut f = Follow::new(vec!["thoughts".to_string()]);
        let out = f.take(&notices);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].stream, "thoughts");
        assert_eq!(out[0].kind, "stream");
    }

    #[test]
    fn stream_follow_tracks_its_own_cursor() {
        let notices = vec![notice("main", "one", Body::Story)];
        let mut sf = StreamFollow {
            stream: "main".to_string(),
            seen: 0,
        };
        assert_eq!(sf.take(&notices).len(), 1);
        assert_eq!(sf.seen, 1);
        assert!(sf.take(&notices).is_empty());
    }
}
