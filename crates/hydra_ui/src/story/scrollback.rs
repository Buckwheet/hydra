//! What the story window keeps.
//!
//! `Scrollback` is a serde type (`story::scrollback`'s `__Visitor` is 8.3 KB)
//! with two accessors, `story` @0x10194752c and `streams` @0x101947540 — both
//! 20-byte field reads.  It holds the main [`Story`] and the [`Streams`].

use crate::story::Story;
use crate::story::streams::Streams;

/// The main scrollback plus the named streams.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scrollback {
    pub story: Story,
    pub streams: Streams,
}

impl Scrollback {
    pub fn new() -> Scrollback {
        Scrollback::default()
    }

    /// `@0x10194752c` — the main story.
    pub fn story(&self) -> &Story {
        &self.story
    }

    /// `@0x10194752c` — the main story, mutably.
    pub fn story_mut(&mut self) -> &mut Story {
        &mut self.story
    }

    /// `@0x101947540` — the streams.
    pub fn streams(&self) -> &Streams {
        &self.streams
    }

    /// `@0x101947540` — the streams, mutably.
    pub fn streams_mut(&mut self) -> &mut Streams {
        &mut self.streams
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_reach_the_parts() {
        let mut s = Scrollback::new();
        assert!(s.story().is_empty());
        assert!(s.streams().is_empty());
        s.story_mut().see("hello");
        assert!(!s.story().is_empty());
        assert!(s.streams_mut().ids().is_empty());
    }
}
