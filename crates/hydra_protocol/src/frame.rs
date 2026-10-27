//! Top-level protocol frames emitted by [`crate::parser::Parser`].
//!
//! `// UNKNOWN:` the original `Frame` is a 280-byte (`0x118`) tagged struct
//! with many payload variants.  Only the variants the recovered parser paths
//! actually emit are modelled here; the tag/name and raw text are preserved
//! so unknown protocol messages are never silently lost.

pub mod payload;

use crate::runs::Runs;

use payload::MerchantStatus;

/// A parsed protocol frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// Styled text produced by the line parser.
    Text(Runs),
    /// `<prompt>...</prompt>`
    Prompt(String),
    /// A line echoed back by the game (kept separate in the binary).
    Echo(String),
    /// A raw/oversized line that could not be parsed as markup.
    Raw(String),
    /// `<merchantStatus>...</merchantStatus>`
    MerchantStatus(MerchantStatus),
    /// A known or unknown tag the simplified parser does not model.
    Unknown {
        tag: String,
        raw: String,
        text: String,
    },
    /// A recognised structural tag: the tag name plus its raw body, and no
    /// text of its own.
    ///
    /// Built by [`Frame::structural`] — the original
    /// `frame::methods::<impl Frame>::structural` (`0x101c2f73c`, frame tag
    /// `0x8000_0000_0000_0038`).  `// UNKNOWN:` the original's field names;
    /// the shape is established: two owned `String`s, copied from the two
    /// `&str` arguments.
    Structural {
        tag: String,
        text: String,
    },
    /// `wire::client_region`'s text variant (frame tag
    /// `0x8000_0000_0000_0035`): the entity-decoded head of a line that
    /// started with `<c>` before the `<!-- ENDCLIENT -->` marker.
    ClientText(String),
    /// `wire::client_region`'s empty variant (frame tag
    /// `0x8000_0000_0000_003a`): the head did not start with `<c>`, or there
    /// was no marker at all.
    ClientRegion,
}

impl Default for Frame {
    fn default() -> Self {
        Frame::Raw(String::new())
    }
}

impl Frame {
    /// Human-readable plain text for the frame.
    pub fn plain(&self) -> String {
        match self {
            Frame::Text(runs) => runs.plain(),
            Frame::Prompt(s) | Frame::Echo(s) | Frame::Raw(s) => s.clone(),
            Frame::MerchantStatus(status) => match status {
                MerchantStatus::Start => "start".into(),
                MerchantStatus::Rules => "rules".into(),
                MerchantStatus::Stop => "stop".into(),
                MerchantStatus::Other(s) => s.clone(),
            },
            Frame::Unknown { text, .. } => text.clone(),
            // A structural frame carries no visible text of its own; the
            // binary's `structural` constructor is what `structural_only`
            // pushes after flushing the pending text frame.
            Frame::Structural { .. } | Frame::ClientRegion => String::new(),
            Frame::ClientText(text) => text.clone(),
        }
    }

    /// True when the frame carries no visible text.
    pub fn is_empty(&self) -> bool {
        match self {
            Frame::Text(runs) => runs.is_empty(),
            Frame::Prompt(s) | Frame::Echo(s) | Frame::Raw(s) => s.is_empty(),
            Frame::MerchantStatus(_) => false,
            Frame::Unknown { text, raw, .. } => text.is_empty() && raw.is_empty(),
            Frame::Structural { .. } | Frame::ClientRegion => true,
            Frame::ClientText(text) => text.is_empty(),
        }
    }

    /// The runs of a text frame, if this is one.
    pub fn runs(&self) -> Option<&Runs> {
        match self {
            Frame::Text(runs) => Some(runs),
            _ => None,
        }
    }

    /// The tag name captured for an unknown frame.
    pub fn tag(&self) -> Option<&str> {
        match self {
            Frame::Unknown { tag, .. } => Some(tag),
            _ => None,
        }
    }

    /// The raw wire text captured for an unknown frame.
    pub fn raw(&self) -> Option<&str> {
        match self {
            Frame::Unknown { raw, .. } => Some(raw),
            _ => None,
        }
    }

    /// Build a structural frame from a tag name and its raw body.
    ///
    /// Recovered from `frame::methods::<impl Frame>::structural`
    /// (`0x101c2f73c`, 272 B).  The whole body is an allocation of two
    /// `String`s — `__rust_alloc(len, 1)` + `memcpy` each, with the capacity
    /// word set to exactly the input length — and one store of the frame tag
    /// word `0x8000_0000_0000_0038`:
    ///
    /// ```text
    /// [frame + 0x00] = 0x8000_0000_0000_0038
    /// [frame + 0x08] = String::from(first)     // cap, ptr, len
    /// [frame + 0x20] = String::from(second)    // cap, ptr, len
    /// ```
    ///
    /// Its only inlined caller is `parser::dispatch::structural_only`
    /// (`0x101c1d35c`), which flushes the pending text frame into the output
    /// vector and then pushes `Frame::structural(a, b)`.
    pub fn structural(tag: &str, text: &str) -> Frame {
        Frame::Structural {
            tag: tag.to_string(),
            text: text.to_string(),
        }
    }

    /// The tag/body pair of a structural frame, if this is one.
    pub fn structural_parts(&self) -> Option<(&str, &str)> {
        match self {
            Frame::Structural { tag, text } => Some((tag, text)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_copies_both_strings() {
        let frame = Frame::structural("roomDesc", "A blue gem lies here.");
        assert_eq!(
            frame,
            Frame::Structural {
                tag: "roomDesc".into(),
                text: "A blue gem lies here.".into(),
            }
        );
        assert_eq!(
            frame.structural_parts(),
            Some(("roomDesc", "A blue gem lies here."))
        );
        // The original copies the inputs into freshly allocated `String`s
        // whose capacity equals the length; nothing is shared with them.
        match &frame {
            Frame::Structural { tag, text } => {
                assert_eq!(tag.capacity(), "roomDesc".len());
                assert_eq!(text.capacity(), "A blue gem lies here.".len());
            }
            other => panic!("expected a structural frame, got {other:?}"),
        }
    }

    #[test]
    fn a_structural_frame_carries_no_visible_text() {
        let frame = Frame::structural("streamWindow", "<b>x</b>");
        assert_eq!(frame.plain(), "");
        assert!(frame.is_empty());
        assert!(frame.runs().is_none());
        assert!(frame.tag().is_none());
        assert!(frame.raw().is_none());
    }

    #[test]
    fn client_region_frames_report_their_text() {
        assert_eq!(Frame::ClientRegion.plain(), "");
        assert!(Frame::ClientRegion.is_empty());
        assert_eq!(Frame::ClientText("hi".into()).plain(), "hi");
        assert!(!Frame::ClientText("hi".into()).is_empty());
        assert!(Frame::ClientText(String::new()).is_empty());
    }
}
