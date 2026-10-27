//! Where the wire bytes come from.
//!
//! Hydra has exactly two feeds, and the reference build keeps them in two
//! different places:
//!
//! * **a recording** — `hydra::replay::replay` (`0x100205ff4`) reads the whole
//!   file with `std::fs::read` (`0x10020602c`) and pushes it through
//!   `hydra_protocol::parser::read::<impl Parser>::push_bytes` (`0x100206174`)
//!   in pieces of **`0x1000` bytes** (`cmp x21, #0x1000` / `csel` at
//!   `0x100206158`..`0x100206160`, advanced by `add x26, x26, x25`), then calls
//!   `Parser::finish` (`0x1002061ac`) for the tail;
//! * **Lich's stdout** — `hydra_lich::relay::read_lines` hands the session a
//!   line channel (bounded at 64) and the relay loop feeds the lines to the
//!   frame parser; `hydra::play::run` (`0x100023b6c`) is the headless consumer.
//!
//! [`Source`] is that seam.  [`pump`] is the single driver both feeds go
//! through, so a recording and a relay run the *same* byte state machine
//! (`push_bytes`), which is the only way the replay gate can claim to exercise
//! the real parser: CRLF, UTF-8 and the `<settings>` byte rules are all handled
//! inside `push_bytes`, never by splitting the stream here.

use std::io::BufRead;
use std::sync::mpsc::Receiver;

use hydra_protocol::{Frame, Parser};

/// The size of one replay read, recovered from `hydra::replay::replay`
/// (`cmp x21, #0x1000` at `0x100206158`).
pub const REPLAY_CHUNK: usize = 0x1000;

/// One piece of the stream, in the form its producer has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chunk {
    /// Raw bytes (a recording).
    Bytes(Vec<u8>),
    /// One line without its terminator (Lich's stdout reader).
    Line(String),
}

/// A producer of [`Chunk`]s.
pub trait Source {
    /// The next piece, or `None` at the end of the stream.
    fn next_chunk(&mut self) -> Option<Chunk>;
}

/// A recorded stream held in memory, handed out in [`REPLAY_CHUNK`] pieces.
#[derive(Clone, Debug)]
pub struct Replay {
    data: Vec<u8>,
    at: usize,
}

impl Replay {
    /// Wrap a recording.
    pub fn new(data: Vec<u8>) -> Self {
        Replay { data, at: 0 }
    }

    /// The length of the whole recording, in bytes.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when the recording is empty (replaying it must succeed with no
    /// frames).
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Source for Replay {
    fn next_chunk(&mut self) -> Option<Chunk> {
        if self.at >= self.data.len() {
            return None;
        }
        let end = (self.at + REPLAY_CHUNK).min(self.data.len());
        let piece = self.data[self.at..end].to_vec();
        self.at = end;
        Some(Chunk::Bytes(piece))
    }
}

/// The relayed stream from a Lich child.
///
/// The lines come from [`hydra_lich::relay::read_lines`], i.e. the recovered
/// child-stdout reader: a `BufReader` over the child's `OwnedReadHalf`, a
/// bounded 64-line channel, invalid UTF-8 replaced lossily, and `\n` (with a
/// preceding `\r`) stripped.  Nothing here spawns Lich: [`Lich::listen`] takes
/// the reader, and the relay itself
/// ([`hydra_lich::relay::show`]/[`hydra_lich::relay::run`]) is what would own
/// the child when the live path is wired (see `NOTES.md`).
pub struct Lich {
    lines: Receiver<String>,
    /// How many lines were relayed.
    seen: usize,
}

impl Lich {
    /// Attach to an existing line channel (what the relay loop holds).
    pub fn from_lines(lines: Receiver<String>) -> Self {
        Lich { lines, seen: 0 }
    }

    /// Attach to any line source — the child's stdout in production, a fixture
    /// in tests — through `hydra_lich`'s own line reader.
    pub fn listen<R>(reader: R) -> Self
    where
        R: BufRead + Send + 'static,
    {
        Lich::from_lines(hydra_lich::relay::read_lines(reader))
    }

    /// How many lines were relayed so far.
    pub fn seen(&self) -> usize {
        self.seen
    }

    /// Blocking wait for one line (used when the caller drives the pump itself).
    pub fn next_line(&mut self) -> Option<String> {
        let line = self.lines.recv().ok();
        if line.is_some() {
            self.seen += 1;
        }
        line
    }
}

impl Source for Lich {
    fn next_chunk(&mut self) -> Option<Chunk> {
        self.next_line().map(Chunk::Line)
    }
}

/// Drive a [`Source`] through a [`Parser`], calling `on_frame` for every frame
/// in stream order, and flush the parser at the end.
///
/// Returns the number of frames seen.  A [`Chunk::Line`] is fed to the same
/// byte state machine as a [`Chunk::Bytes`] chunk (line bytes, then `'\n'`),
/// because `read_lines` strips the terminator but `push_bytes` needs it.
pub fn pump<S, F>(source: &mut S, parser: &mut Parser, mut on_frame: F) -> usize
where
    S: Source + ?Sized,
    F: FnMut(Frame),
{
    pump_groups(source, parser, |group| {
        for frame in group {
            on_frame(frame);
        }
    })
}

/// [`pump`], but one **input line** at a time: `on_line` gets the frames of a
/// single line together.
///
/// The reference build's `--text` renderer coalesces the `Text` frames of one
/// line into a line buffer and breaks the line when the input line ends, so
/// the line grouping [`Parser::push_bytes_lines`] returns has to reach the
/// renderer.
/// Lines that produce no frames have no group (nothing to render), and the tail
/// of the recording — a final line without a terminator — is delivered as the
/// last group, i.e. exactly like a completed line.
///
/// Returns the number of frames seen.
pub fn pump_groups<S, F>(source: &mut S, parser: &mut Parser, mut on_line: F) -> usize
where
    S: Source + ?Sized,
    F: FnMut(Vec<Frame>),
{
    let mut frames = 0;
    while let Some(chunk) = source.next_chunk() {
        let groups = match chunk {
            Chunk::Bytes(bytes) if bytes.is_empty() => Vec::new(),
            Chunk::Bytes(bytes) => parser.push_bytes_lines(&bytes),
            Chunk::Line(line) => {
                let mut groups = parser.push_bytes_lines(line.as_bytes());
                groups.extend(parser.push_bytes_lines(b"\n"));
                groups
            }
        };
        for group in groups {
            frames += group.len();
            on_line(group);
        }
    }
    let tail = parser.finish();
    if !tail.is_empty() {
        frames += tail.len();
        on_line(tail);
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn drain<S: Source>(source: &mut S) -> Vec<Frame> {
        let mut out = Vec::new();
        let mut parser = Parser::new();
        pump(source, &mut parser, |frame| out.push(frame));
        out
    }

    #[test]
    fn a_replay_is_handed_out_in_4k_pieces() {
        let data = vec![b'x'; 2 * REPLAY_CHUNK + 5];
        let mut source = Replay::new(data);
        let mut sizes = Vec::new();
        while let Some(Chunk::Bytes(piece)) = source.next_chunk() {
            sizes.push(piece.len());
        }
        assert_eq!(sizes, vec![REPLAY_CHUNK, REPLAY_CHUNK, 5]);
        assert!(source.next_chunk().is_none(), "the source ends once");
    }

    #[test]
    fn an_empty_recording_has_no_chunks() {
        let mut source = Replay::new(Vec::new());
        assert!(source.is_empty());
        assert_eq!(source.len(), 0);
        assert!(source.next_chunk().is_none());
        assert!(drain(&mut source).is_empty());
    }

    #[test]
    fn a_line_beyond_the_chunk_boundary_still_parses() {
        // 0x1000 is not a line boundary: the byte machine must not care.
        let mut data = vec![b'a'; REPLAY_CHUNK - 3];
        data.extend_from_slice(b"<b>bold</b>\n");
        let frames = drain(&mut Replay::new(data));
        // `<b>` and `</b>` are inline markup: one `Structural` each, and the
        // line's text arrives as one frame on either side of the first tag.
        let kinds: Vec<&str> = frames.iter().map(hydra_protocol::Frame::kind).collect();
        assert_eq!(kinds, vec!["Text", "Structural", "Text", "Structural"]);
        let text = frames[0].plain();
        assert!(text.starts_with("aaa"), "{text:?}");
        assert_eq!(frames[2].plain(), "bold", "the tail is not cut in half");
    }

    #[test]
    fn the_lich_feed_reaches_the_same_parser() {
        let script = b"<b>hello</b>\r\n<a exist=\"1\" noun=\"gem\">a gem</a>\r\n".to_vec();
        let frames = drain(&mut Lich::listen(Cursor::new(script.clone())));
        let kinds: Vec<&str> = frames.iter().map(hydra_protocol::Frame::kind).collect();
        assert_eq!(
            kinds,
            vec!["Structural", "Text", "Structural", "Structural", "Text", "Structural"]
        );
        assert_eq!(frames[1].plain(), "hello");
        assert_eq!(frames[4].plain(), "a gem");

        // The same bytes as a recording, chunked by the replay source, give the
        // same frames: the two feeds share one state machine.
        let replayed = drain(&mut Replay::new(script));
        assert_eq!(frames, replayed);
    }

    #[test]
    fn a_lich_line_without_a_terminator_is_flushed_at_the_end() {
        let frames = drain(&mut Lich::listen(Cursor::new(b"trailing words".to_vec())));
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].plain(), "trailing words");
    }
}
