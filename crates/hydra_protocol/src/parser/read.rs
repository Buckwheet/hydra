//! Byte-oriented streaming front end for [`Parser`].
//!
//! Recovered from `parser::read`:
//!   * `push_bytes` at `0x101c1a288` (1028 bytes), `finish` at `0x101c1a68c`.
//!   * Bytes are split on `\n`; every completed line is decoded (valid UTF-8,
//!     or byte-to-`char` fallback) and handed to [`Parser::parse_line`].
//!   * A line longer than `0x40000` bytes is emitted as a [`Frame::Raw`] and
//!     the rest of that line is discarded (the skip flag at `Parser + 0x168`;
//!     the check is `len >= 0x40000` *before* appending, so the emitted line
//!     is `0x40001` bytes long).
//!   * Inside a `<settings ...>` block the bytes never reach the line buffer.
//!     `push_bytes` keeps a rolling window of the last 11 bytes (the
//!     `Vec<u8>` at `Parser + 0x60`) and waits for `</settings>`; seeing it
//!     re-enables the line buffer for the remainder of the line (state byte
//!     `Parser + 0x169` goes `1` -> `2`, and the next newline strips leading
//!     `\r` from the buffered remainder before parsing it).
//!   * A `<prompt` seen while inside an unterminated settings block aborts the
//!     block: the state returns to normal and `<prompt` is seeded into the line
//!     buffer, so the prompt itself is still parsed.
//!
//! The window comparison is exact — the binary compares the full 11 bytes
//! against `</settings>` and the last 7 bytes against `<prompt`.

use super::{decode_wire_line, Parser, MAX_LINE_BYTES};
use crate::frame::Frame;

/// The 11 bytes that close a `<settings ...>` block.
const SETTINGS_END: &[u8] = b"</settings>";

/// The 7-byte marker that aborts an unterminated settings block.
const PROMPT_START: &[u8] = b"<prompt";

impl Parser {
    /// Feed a chunk of bytes and return all frames completed by this chunk.
    ///
    /// Chunk boundaries may fall anywhere, including inside a UTF-8 sequence,
    /// an XML tag or a `<settings>` block; the parser buffers until a newline
    /// (or [`Self::finish`]).
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.push_bytes_lines(bytes).into_iter().flatten().collect()
    }

    /// Feed a chunk of bytes and return the frames **per completed input line**.
    ///
    /// This is the same state machine as [`Self::push_bytes`] (which is this
    /// method flattened); only the grouping is new.  The reference build's
    /// `--text` renderer coalesces the `Text` frames of one input line into a
    /// line buffer and breaks the line when the input line ends, so the
    /// grouping has to survive the flat `Frame` stream.  Every returned group
    /// is one line that produced at least one frame; a line that produced none
    /// (a `<settings …>` block line, the tail of an over-long line) has no
    /// group, and neither has a line that is still incomplete at the end of the
    /// chunk — [`Self::finish`] returns that one.
    pub fn push_bytes_lines(&mut self, bytes: &[u8]) -> Vec<Vec<Frame>> {
        let mut lines: Vec<Vec<Frame>> = Vec::new();
        let mut frames = Vec::new();
        for &byte in bytes {
            if self.skip_until_newline {
                if byte == b'\n' {
                    self.skip_until_newline = false;
                    self.buffer.clear();
                    self.settings_window.clear();
                }
                continue;
            }

            if byte == b'\n' {
                self.settings_window.clear();
                let closed = std::mem::take(&mut self.settings_closed);
                let data = std::mem::take(&mut self.buffer);
                let data = if closed {
                    strip_leading_cr(&data)
                } else {
                    &data[..]
                };
                self.frames_for_line(data, &mut frames);
                if !frames.is_empty() {
                    lines.push(std::mem::take(&mut frames));
                }
                continue;
            }

            if self.in_settings {
                self.settings_window.push(byte);
                if self.settings_window.len() > SETTINGS_END.len() {
                    self.settings_window.remove(0);
                }
                if self.settings_window == SETTINGS_END {
                    self.in_settings = false;
                    self.settings_closed = true;
                    self.settings_window.clear();
                } else if self.settings_window.ends_with(PROMPT_START) {
                    self.in_settings = false;
                    self.settings_window.clear();
                    self.buffer.extend_from_slice(PROMPT_START);
                }
                continue;
            }

            self.buffer.push(byte);
            if self.buffer.len() > MAX_LINE_BYTES {
                let data = std::mem::take(&mut self.buffer);
                let line = decode_wire_line(&data);
                // `// UNKNOWN:` the binary emitted a frame here whose tag word
                // is `0x8000_0000_0000_0039`, i.e. a raw frame; whether a
                // settings block was active at this point is not visible.
                frames.push(Frame::Raw(line.trim_end_matches('\r').to_string()));
                self.skip_until_newline = true;
            }
        }
        if !frames.is_empty() {
            lines.push(frames);
        }
        lines
    }

    /// Flush any unterminated final line.
    ///
    /// Returns an empty vector when the buffer is empty or when the parser is
    /// discarding the tail of an over-long line.
    pub fn finish(&mut self) -> Vec<Frame> {
        self.skip_until_newline = false;
        self.settings_window.clear();
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let data = std::mem::take(&mut self.buffer);
        let mut frames = Vec::new();
        self.frames_for_line(strip_leading_cr(&data), &mut frames);
        frames
    }

    fn frames_for_line(&mut self, data: &[u8], out: &mut Vec<Frame>) {
        let line = decode_wire_line(data);
        let line = line.trim_end_matches('\r');
        out.extend(self.parse_line(line));
    }
}

/// Drop leading `\r` bytes, as `finish` and the post-settings newline do.
fn strip_leading_cr(data: &[u8]) -> &[u8] {
    let skip = data
        .iter()
        .position(|&byte| byte != b'\r')
        .unwrap_or(data.len());
    &data[skip..]
}
