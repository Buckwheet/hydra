//! Chunked output lines.
//!
//! `Chunk::push_line` (376 B) appends and trims to a capacity
//! (`<alloc::vec::Vec<_, _>>::remove::assert_failed` is the inlined `remove(0)`
//! path, so the chunk is a bounded ring). `ChunkLine::text` (4 B) is a
//! tail-call to `Runs::plain`; `ChunkLine::is_spoken` (636 B) decides whether a
//! line came from speech.

use hydra_protocol::runs::Runs;

/// One line of output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkLine {
    pub runs: Runs,
}

impl ChunkLine {
    /// @0x0000000101bfe51c (4 B) — `self.runs.plain()`
    pub fn text(&self) -> String {
        self.runs.plain()
    }

    /// @0x0000000101bfe520 (636 B) — did this line come from speech?
    ///
    /// `// UNKNOWN:` the exact style test; the recovered body only inspects the
    /// runs. Speech presets are the protocol's `speech`/`whisper` presets.
    pub fn is_spoken(&self) -> bool {
        self.runs.iter().any(|run| {
            run.preset
                .as_deref()
                .map(|preset| {
                    let preset = preset.to_ascii_lowercase();
                    preset.contains("speech") || preset.contains("whisper")
                })
                .unwrap_or(false)
        })
    }
}

/// A bounded chunk of lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chunk {
    pub lines: Vec<ChunkLine>,
    /// The ring capacity.
    pub capacity: usize,
}

impl Chunk {
    pub fn with_capacity(capacity: usize) -> Chunk {
        Chunk {
            lines: Vec::new(),
            capacity,
        }
    }

    /// @0x0000000101bfe3a4 (376 B) — append, dropping the oldest lines when the
    /// chunk is full.
    pub fn push_line(&mut self, line: ChunkLine) -> Option<ChunkLine> {
        self.lines.push(line);
        if self.capacity > 0 && self.lines.len() > self.capacity {
            Some(self.lines.remove(0))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_protocol::runs::Run;

    fn line(text: &str) -> ChunkLine {
        ChunkLine {
            runs: Runs::from(vec![Run::new(text)]),
        }
    }

    #[test]
    fn chunk_line_text_is_the_plain_text() {
        assert_eq!(line("hello").text(), "hello");
    }

    #[test]
    fn push_line_drops_the_oldest_when_full() {
        let mut chunk = Chunk::with_capacity(2);
        assert!(chunk.push_line(line("one")).is_none());
        assert!(chunk.push_line(line("two")).is_none());
        let dropped = chunk.push_line(line("three")).unwrap();
        assert_eq!(dropped.text(), "one");
        assert_eq!(chunk.lines.len(), 2);
    }

    #[test]
    fn spoken_detection_uses_the_preset() {
        let spoken = ChunkLine {
            runs: Runs::from(vec![Run::with_preset("hello", "speech")]),
        };
        assert!(spoken.is_spoken());
        assert!(!line("hello").is_spoken());
    }
}
