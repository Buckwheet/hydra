//! Line-level wire handling.
//!
//! Recovered from `crates/hydra-protocol/src/parser/wire.rs`:
//!
//!   * `decode_wire_line` (276 B, `0x101c39d24`) — lives in [`super`] here;
//!     it is `[super::decode_wire_line]`.
//!   * `client_region` (528 B, `0x101c39b14`) — [`client_region`] below.
//!
//! `client_region` is called once, from `parse_line` (`0x101c1b614`), on the
//! raw line.  It splits the line at Lich's `<!-- ENDCLIENT -->` marker and
//! turns the part before it into a frame.

use crate::frame::Frame;
use crate::text::entities;

/// The marker Lich inserts to separate its own output from the game's.
///
/// The literal sits at `0x103bf919d`, is 18 bytes long, and `client_region`
/// passes it straight to `str::find` (`x3 = 0x12`).
pub const END_CLIENT: &str = "<!-- ENDCLIENT -->";

/// The number of leading bytes `client_region` skips before it looks for
/// [`END_CLIENT`].
///
/// Established: every slice in the function uses `#0xf` — the boundary check
/// (`ldrsb w8, [x22, #0xf]!`), the `str::find` window (`x20 + 0xf`,
/// `len - 0xf`) and all three `slice_error_fail` sites.  What those 15 bytes
/// mean is **not** recoverable from this binary (`// UNKNOWN:`).
pub const CLIENT_PREFIX_LEN: usize = 15;

/// The prefix that marks the head of a client region as carrying text.
///
/// `client_region` compares the first three bytes of the head against the two
/// immediates `0x633c` (`"<c"`) and `'>'`.
const TEXT_PREFIX: &str = "<c>";

/// Split a line at [`END_CLIENT`] and describe the client region it bounds.
///
/// Returns the frame for the region and the remainder of the line *after* the
/// marker (an empty `&str` when the marker is absent).
///
/// Recovered control flow (`0x101c39b14`):
///
/// ```text
/// require line.len() >= 15 and a char boundary at 15   // else slice panic
/// rest = &line[15..]
/// match rest.find("<!-- ENDCLIENT -->") {
///     Some(i) => { head = &rest[..i]; tail = &rest[i + 18..] }
///     None    => { head = rest;       tail = ""             }
/// }
/// if head.starts_with("<c>") {
///     Frame::ClientText(decode_entities + strip_control_chars of &head[3..])
/// } else {
///     Frame::ClientRegion
/// }
/// ```
///
/// The two frame tags in the binary are `0x8000_0000_0000_0035` (the text
/// variant, `0x3a - 5`) and `0x8000_0000_0000_003a` (the empty variant).
///
/// # Panics
///
/// The original indexes `&line[15..]`, so a line shorter than 15 bytes (or
/// one with a non-boundary byte at 15) panics inside `core::str`.  This
/// reconstruction returns the empty-frame form instead of panicking, which is
/// the only deliberate departure — see `NOTES.md`.
pub fn client_region(line: &str) -> (Frame, &str) {
    let Some(rest) = line.get(CLIENT_PREFIX_LEN..) else {
        return (Frame::ClientRegion, "");
    };
    let (head, tail) = match rest.find(END_CLIENT) {
        Some(i) => (&rest[..i], &rest[i + END_CLIENT.len()..]),
        None => (rest, ""),
    };
    match head.strip_prefix(TEXT_PREFIX) {
        Some(text) => (
            Frame::ClientText(entities::strip_control_chars(&entities::decode_entities(
                text,
            ))),
            tail,
        ),
        None => (Frame::ClientRegion, tail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `<prompt time="1">` is 17 bytes, so pad to reach the 15-byte prefix.
    const PAD: &str = "0123456789abcde";

    #[test]
    fn the_prefix_length_and_marker_are_the_recovered_literals() {
        assert_eq!(END_CLIENT.len(), 18);
        assert_eq!(CLIENT_PREFIX_LEN, 15);
        assert_eq!(PAD.len(), CLIENT_PREFIX_LEN);
    }

    #[test]
    fn a_line_without_the_marker_returns_everything_after_the_prefix() {
        let line = format!("{PAD}hello");
        let (frame, tail) = client_region(&line);
        assert_eq!(frame, Frame::ClientRegion);
        assert_eq!(tail, "");
    }

    #[test]
    fn the_text_after_the_marker_becomes_the_tail() {
        let line = format!("{PAD}{END_CLIENT}<dir value=\"north\"/>");
        let (frame, tail) = client_region(&line);
        assert_eq!(frame, Frame::ClientRegion);
        assert_eq!(tail, "<dir value=\"north\"/>");
    }

    #[test]
    fn the_head_is_only_decoded_when_it_starts_with_the_text_prefix() {
        let line = format!("{PAD}<c>a &amp; b&lt;{END_CLIENT}rest");
        let (frame, tail) = client_region(&line);
        assert_eq!(frame, Frame::ClientText("a & b<".into()));
        assert_eq!(tail, "rest");
    }

    #[test]
    fn a_head_without_the_prefix_carries_no_text() {
        let line = format!("{PAD}x<c>ignored{END_CLIENT}rest");
        let (frame, tail) = client_region(&line);
        assert_eq!(frame, Frame::ClientRegion);
        assert_eq!(tail, "rest");
    }

    #[test]
    fn control_characters_are_stripped_from_the_decoded_head() {
        let line = format!("{PAD}<c>a\u{7}b{END_CLIENT}");
        let (frame, _) = client_region(&line);
        assert_eq!(frame, Frame::ClientText("ab".into()));
    }

    #[test]
    fn a_short_line_yields_the_empty_frame_and_no_tail() {
        // The original panics here; this reconstruction does not.
        assert_eq!(client_region("short"), (Frame::ClientRegion, ""));
        assert_eq!(
            client_region("0123456789abcde"),
            (Frame::ClientRegion, "")
        );
    }
}
