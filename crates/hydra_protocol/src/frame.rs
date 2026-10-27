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
        }
    }

    /// True when the frame carries no visible text.
    pub fn is_empty(&self) -> bool {
        match self {
            Frame::Text(runs) => runs.is_empty(),
            Frame::Prompt(s) | Frame::Echo(s) | Frame::Raw(s) => s.is_empty(),
            Frame::MerchantStatus(_) => false,
            Frame::Unknown { text, raw, .. } => text.is_empty() && raw.is_empty(),
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
}
