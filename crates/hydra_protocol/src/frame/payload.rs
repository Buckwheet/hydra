//! Frame payload types recovered from the binary.

use crate::runs::Run;

/// The target of a clickable `<a>`/`<d>` link.
///
/// `// UNKNOWN:` the original layout is a niche-optimised union at offset
/// `0x30` of `Link`: an `exist` string and optional `noun` in the object
/// case, or a discriminant word `0x8000000000000002` followed by a `cmd` /
/// `href` string in the other cases.  The variants here model that union.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    /// `<a exist="123" noun="gem">`
    Object {
        exist: String,
        noun: Option<String>,
    },
    /// `<d cmd="look">`
    Command { cmd: String },
    /// `<a href="...">`
    Href { href: String },
}

impl Default for LinkTarget {
    fn default() -> Self {
        LinkTarget::Object {
            exist: String::new(),
            noun: None,
        }
    }
}

/// A clickable link attached to a run of text.
///
/// Recovered layout (96 bytes):
///   * `+0x00`: `Vec<(String, String)>` of extra attributes (always empty in
///     `link_from_tag`; `// UNKNOWN:` exact element type)
///   * `+0x18`: `Option<String>` `coord`
///   * `+0x30`: `LinkTarget` (48 bytes)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// `// UNKNOWN:` the binary only ever writes an empty `Vec` here.
    pub attrs: Vec<(String, String)>,
    pub coord: Option<String>,
    pub target: LinkTarget,
}

impl Link {
    /// An object link (`exist`/`noun`).
    pub fn object(exist: impl Into<String>, noun: Option<impl Into<String>>) -> Self {
        Link {
            target: LinkTarget::Object {
                exist: exist.into(),
                noun: noun.map(Into::into),
            },
            ..Link::default()
        }
    }

    /// A command link (`cmd`).
    pub fn command(cmd: impl Into<String>) -> Self {
        Link {
            target: LinkTarget::Command { cmd: cmd.into() },
            ..Link::default()
        }
    }

    /// A hyperlink (`href`).
    pub fn hyperlink(href: impl Into<String>) -> Self {
        Link {
            target: LinkTarget::Href { href: href.into() },
            ..Link::default()
        }
    }

    /// True for the `exist`/`noun` variant.
    ///
    /// The binary tests bit 63 of the word at `Link + 0x30`; object links are
    /// the only variant with that bit clear.
    pub fn is_object(&self) -> bool {
        matches!(self.target, LinkTarget::Object { .. })
    }

    pub fn exist(&self) -> Option<&str> {
        match &self.target {
            LinkTarget::Object { exist, .. } => Some(exist),
            _ => None,
        }
    }

    pub fn noun(&self) -> Option<&str> {
        match &self.target {
            LinkTarget::Object { noun, .. } => noun.as_deref(),
            _ => None,
        }
    }

    pub fn cmd(&self) -> Option<&str> {
        match &self.target {
            LinkTarget::Command { cmd } => Some(cmd),
            _ => None,
        }
    }

    pub fn href(&self) -> Option<&str> {
        match &self.target {
            LinkTarget::Href { href } => Some(href),
            _ => None,
        }
    }
}

/// Merchant-window status reported by `<merchantStatus>`.
///
/// `// UNKNOWN:` the recovered return type is the enum itself (24 bytes), not
/// `Option<MerchantStatus>`: the binary's first discriminant value is also
/// `isize::MIN`, so wrapping it in an `Option` would make `Start`
/// indistinguishable from `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum MerchantStatus {
    /// `"start"` (encoded as `isize::MIN` in the binary)
    #[default]
    Start,
    /// `"rules"`
    Rules,
    /// `"stop"`
    Stop,
    /// Any other status text, lowercased.
    Other(String),
}

impl MerchantStatus {
    /// Trim and lowercase the input, then recognise fixed status words.
    ///
    /// Unrecognised non-empty text becomes `Other(lowercased)`.
    pub fn parse(text: &str) -> MerchantStatus {
        let lowered = text.trim().to_ascii_lowercase();
        match lowered.as_str() {
            "start" => MerchantStatus::Start,
            "rules" => MerchantStatus::Rules,
            "stop" => MerchantStatus::Stop,
            _ => MerchantStatus::Other(lowered),
        }
    }
}

/// The capacity/quantity display for an inventory item.
///
/// Recovered from `in_capacity`:
///   * `(1, 0)` reads as [`Capacity::Unknown`] (tag 2);
///   * otherwise the second `u32` is split into `div = value / 10` and
///     `rem = value % 10`;
///   * `rem == 0` -> [`Capacity::Whole`], else [`Capacity::Fraction`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capacity {
    /// An exact whole number.
    Whole(u32),
    /// A fractional display: `whole.remainder`.
    Fraction { remainder: u32, whole: u32 },
    /// Slot encoding `(1, 0)`, used when the game sends no capacity.
    Unknown,
}

impl Default for Capacity {
    fn default() -> Self {
        Capacity::Whole(0)
    }
}

/// An item shown in an inventory/container list.
///
/// `// UNKNOWN:` only the capacity words at offsets `0x118`/`0x11c` were
/// recovered from `in_capacity`; the leading fields are a plausible model of
/// an inventory row, not byte-for-byte recovered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InventoryItem {
    pub name: String,
    pub noun: String,
    pub quantity: u32,
    /// First capacity word (binary offset `0x118`).
    pub capacity: u32,
    /// Second capacity word (binary offset `0x11c`).
    pub capacity_value: u32,
}

impl InventoryItem {
    pub fn new(name: impl Into<String>) -> Self {
        InventoryItem {
            name: name.into(),
            ..InventoryItem::default()
        }
    }

    /// Construct an item with the two raw capacity words supplied.
    pub fn with_capacity_words(
        name: impl Into<String>,
        capacity: u32,
        capacity_value: u32,
    ) -> Self {
        InventoryItem {
            name: name.into(),
            capacity,
            capacity_value,
            ..InventoryItem::default()
        }
    }

    /// Decode the item's capacity display.
    ///
    /// The exact disassembly is:
    /// ```text
    /// if first == 1 && second == 0 { return Unknown; }
    /// let div = second / 10;
    /// let rem = second % 10;
    /// if rem != 0 { Fraction(rem, div) } else { Whole(div) }
    /// ```
    pub fn in_capacity(&self) -> Capacity {
        if self.capacity == 1 && self.capacity_value == 0 {
            return Capacity::Unknown;
        }
        let whole = self.capacity_value / 10;
        let remainder = self.capacity_value % 10;
        if remainder != 0 {
            Capacity::Fraction { remainder, whole }
        } else {
            Capacity::Whole(whole)
        }
    }
}

/// A single line of styled text, ready to be turned into a [`Run`].
///
/// Recovered layout (binary offsets):
///   * `+0x00`: `String` text
///   * `+0x30`: `Option<String>` preset (style/preset id)
///   * `+0x48`: `u16` style flags
///   * `+0x4a`: `u8` bold-ish flag (copied to `Run + 0x32`)
///   * `+0x50`: `Option<Link>` first link
///   * `+0xb0`: `Option<Link>` second link
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextFrame {
    pub text: String,
    pub preset: Option<String>,
    /// `// UNKNOWN:` only "non-zero means bold" is established.
    pub style: u16,
    pub bold: bool,
    pub link: Option<Link>,
    pub object: Option<Link>,
}

impl TextFrame {
    pub fn new(text: impl Into<String>) -> Self {
        TextFrame {
            text: text.into(),
            ..TextFrame::default()
        }
    }

    /// Convert this frame to a text run.
    ///
    /// The binary's implementation is a straight field-by-field clone into
    /// the 248-byte `Run`; there is no additional computation.
    pub fn as_run(&self) -> Run {
        Run {
            text: self.text.clone(),
            preset: self.preset.clone(),
            style: self.style,
            bold: self.bold,
            link: self.link.clone(),
            object: self.object.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merchant_status_is_lowercased() {
        assert_eq!(MerchantStatus::parse(" start "), MerchantStatus::Start);
        assert_eq!(MerchantStatus::parse("RULES"), MerchantStatus::Rules);
        assert_eq!(MerchantStatus::parse("Stop"), MerchantStatus::Stop);
        assert_eq!(
            MerchantStatus::parse("  Haggling  "),
            MerchantStatus::Other("haggling".into())
        );
        assert_eq!(
            MerchantStatus::parse(""),
            MerchantStatus::Other(String::new())
        );
    }

    #[test]
    fn capacity_decodes_binary_words() {
        let unknown = InventoryItem::with_capacity_words("gem", 1, 0);
        assert_eq!(unknown.in_capacity(), Capacity::Unknown);

        let whole = InventoryItem::with_capacity_words("gem", 0, 90);
        assert_eq!(whole.in_capacity(), Capacity::Whole(9));

        let frac = InventoryItem::with_capacity_words("gem", 0, 95);
        assert_eq!(
            frac.in_capacity(),
            Capacity::Fraction {
                remainder: 5,
                whole: 9
            }
        );
    }

    #[test]
    fn text_frame_as_run_copies_fields() {
        let frame = TextFrame {
            text: "a blue gem".into(),
            preset: Some("roomDesc".into()),
            style: 1,
            bold: true,
            link: Some(Link::object("123", Some("gem"))),
            object: None,
        };
        let run = frame.as_run();
        assert_eq!(run.text, "a blue gem");
        assert_eq!(run.preset.as_deref(), Some("roomDesc"));
        assert!(run.is_bold());
        assert_eq!(run.link.as_ref().and_then(Link::exist), Some("123"));
    }
}
