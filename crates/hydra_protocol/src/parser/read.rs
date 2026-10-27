//! Byte-oriented streaming front end for [`Parser`].
//!
//! The original split incoming bytes on `\n`, decoded each line (valid UTF-8,
//! or byte-to-`char` fallback), and fed it to
//! [`Parser::parse_line`].  Lines longer than `0x40000` bytes were emitted as
//! a raw frame and the rest of the line discarded.

use super::{decode_wire_line, Parser, MAX_LINE_BYTES};
use crate::frame::Frame;

impl Parser {
    /// Feed a chunk of bytes and return all frames completed by this chunk.
    ///
    /// Chunk boundaries may fall anywhere, including inside a UTF-8 sequence
    /// or an XML tag; the parser buffers until a newline (or [`Self::finish`]).
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for &byte in bytes {
            if self.skip_until_newline {
                if byte == b'\n' {
                    self.skip_until_newline = false;
                    self.buffer.clear();
                }
                continue;
            }

            if byte == b'\n' {
                let data = std::mem::take(&mut self.buffer);
                self.frames_for_line(&data, &mut frames);
                continue;
            }

            self.buffer.push(byte);
            if self.buffer.len() > MAX_LINE_BYTES {
                let data = std::mem::take(&mut self.buffer);
                let line = decode_wire_line(&data);
                let line = line.trim_end_matches('\r').to_string();
                // `// UNKNOWN:` the binary suppressed a settings frame here;
                // dropping the whole over-long line is the safest choice.
                frames.push(Frame::Raw(line));
                self.skip_until_newline = true;
            }
        }
        frames
    }

    /// Flush any unterminated final line.
    ///
    /// Returns an empty vector when the buffer is empty or when the parser is
    /// discarding the tail of an over-long line.
    pub fn finish(&mut self) -> Vec<Frame> {
        if self.skip_until_newline || self.buffer.is_empty() {
            self.buffer.clear();
            return Vec::new();
        }
        let data = std::mem::take(&mut self.buffer);
        let mut frames = Vec::new();
        self.frames_for_line(&data, &mut frames);
        frames
    }

    fn frames_for_line(&mut self, data: &[u8], out: &mut Vec<Frame>) {
        let line = decode_wire_line(data);
        let line = line.trim_end_matches('\r');
        out.extend(self.parse_line(line));
    }
}
