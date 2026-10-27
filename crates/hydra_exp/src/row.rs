//! The row types the reports consume.
//!
//! The binary's reports take slices of `hydra_ledger::row::Pulse` (stride
//! `0xd0` = 208 bytes — visible as the `mov w9, #0xd0; madd` address arithmetic
//! in every report) and of `hydra_ledger::row::Snapshot` (stride `0x90` = 144).
//!
//! Those rows are also declared here, because this crate's public API is these
//! types (every `report::*` takes `&[Pulse]`/`&[Snapshot]`) and they are *not*
//! the same types: `hydra_exp`'s copy names the readings off the reports
//! (`bonus`, `label`, `Who { name }`), while `hydra_ledger`'s names them off its
//! tables (`instant`, `name`, `Who { instance, character }`) and types the
//! snapshot kind. [`crate::ledger`] maps between the two, so the one 208-byte
//! row is read and written through the real `hydra_ledger`; every field below is
//! at the byte offset the binary actually reads, which is the offset the
//! ledger's copy uses too.
//!
//! Offsets inside the 208-byte `Pulse` that were observed: `+0x30` (`rpa`,
//! `Option<String>` — its capacity word is the `isize::MIN` niche), `+0x68`
//! (`at`), `+0x80`/`+0x84` (`until_next`, tag + `u32`), `+0x88`/`+0x8c`
//! (`lumnis`, tag + `u32`), `+0x90`/`+0x94` (`fashlonae`), `+0xa0` (`exp`),
//! `+0xa8` (`axp`), `+0xb0` (`exp_gained`), `+0xb8` (`axp_gained`), `+0xc0`
//! (the `brooch`/`instant` amount) and `+0xc8` (`source`, `u8`). Field order
//! inside the row is therefore *not* the declaration order below; only the
//! offsets are established.

/// Where a pulse's experience came from.
///
/// Table at `0x10442dae0` (pointers) / `0x10240cee8` (lengths), five entries:
/// `normal`, `bounty`, `brooch`, `instant`, `manual`. Indexed by the byte at
/// `Pulse + 0xc8` (`report::last`, `report::daily`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Source {
    /// A normal experience pulse.
    #[default]
    Normal = 0,
    /// A bounty reward.
    Bounty = 1,
    /// A brooch rub.
    Brooch = 2,
    /// A "lumnis"/lump style instant gain.
    Instant = 3,
    /// A gain typed in by hand (`exp addgain`).
    Manual = 4,
}

impl Source {
    /// The five names, in discriminant order (`0x10442dae0`).
    pub const NAMES: [&'static str; 5] = ["normal", "bounty", "brooch", "instant", "manual"];

    /// The name for this source, as printed in the `Type` column.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    /// The discriminant as stored in the row. Unknown bytes fall back to
    /// [`Source::Normal`]; the binary indexes the name table with the raw byte,
    /// so a row produced by this build never holds anything else.
    pub fn from_u8(byte: u8) -> Source {
        match byte {
            0 => Source::Normal,
            1 => Source::Bounty,
            2 => Source::Brooch,
            3 => Source::Instant,
            4 => Source::Manual,
            _ => Source::Normal,
        }
    }
}

/// One experience pulse (`hydra_ledger::row::Pulse`, 208 bytes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pulse {
    /// When the pulse was seen (unix seconds) — row offset `+0x68`.
    pub at: i64,
    /// The `rpa` bonus as stored (`+0x30`, `Option<String>`).
    pub rpa: Option<String>,
    /// Experience still needed for the next level (`+0x80`, `Option<u32>`).
    pub until_next: Option<u32>,
    /// The `lumnis` multiplier (`+0x88`, `Option<u32>`).
    pub lumnis: Option<u32>,
    /// The `fashlonae` bonus (`+0x90`). `report::bonuses` prints `fashlonae`
    /// when the tag is `1` and the value word is `2`.
    pub fashlonae: Option<u8>,
    /// Absorbed experience (`+0xa0`).
    pub exp: i64,
    /// Absorbed ascension experience (`+0xa8`).
    pub axp: i64,
    /// Experience gained by this pulse (`+0xb0`).
    pub exp_gained: i64,
    /// Ascension experience gained by this pulse (`+0xb8`).
    pub axp_gained: i64,
    /// The single amount a `brooch`/`instant` row records (`+0xc0`).
    pub bonus: i64,
    /// Where the gain came from (`+0xc8`).
    pub source: Source,
}

/// One snapshot reading (`hydra_ledger::row::Snapshot`, 144 bytes, stride
/// `0x90`).
///
/// Every offset below is established from `report::snapshots`
/// (`0x1012b4d94`) and its two closures (`0x1012bcf64`, `0x1012bcd1c`):
///
/// | offset | field |
/// |---|---|
/// | `+0x00` tag (`u32` = 1) / `+0x08` | [`Snapshot::value`] |
/// | `+0x10` tag (== 1) / `+0x18` | [`Snapshot::weekly`] |
/// | `+0x20` tag (== 1) / `+0x28` | [`Snapshot::total`] |
/// | `+0x60` cap (`isize::MIN` = `None`), `+0x68` ptr, `+0x70` len | [`Snapshot::label`] |
/// | `+0x80` | [`Snapshot::at`] |
/// | `+0x88` | [`Snapshot::kind`] |
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `+0x00`/`+0x08` — the reading itself (Voln favor, suffused experience).
    pub value: Option<i64>,
    /// `+0x10`/`+0x18` — the weekly figure (`{}/50,000 weekly`).
    pub weekly: Option<i64>,
    /// `+0x20`/`+0x28` — the lifetime figure (`{}/200,000 total`).
    pub total: Option<i64>,
    /// `+0x60` — the reading's own name; `None` falls back to the kind's
    /// capitalised name (`"Resource"`) or `"resource"`.
    pub label: Option<String>,
    /// `+0x80` — unix seconds.
    pub at: i64,
    /// `+0x88` — which kind this row is; matches the `which` byte
    /// [`crate::report::snapshots`] is called with (`0` favor, `1` resource,
    /// `2` suffused).
    pub kind: u8,
}

/// The character a report is about (`hydra_ledger::row::Who`).
///
/// `command::read` reads the name as a `&str` at `+0x20`/`+0x28` of the second
/// argument and passes it to every report as the character name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    /// The character's name (`+0x20`).
    pub name: String,
}

impl Who {
    /// Build a `Who` for `name`.
    pub fn new(name: impl Into<String>) -> Who {
        Who { name: name.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names_match_the_table() {
        assert_eq!(Source::NAMES, ["normal", "bounty", "brooch", "instant", "manual"]);
        assert_eq!(Source::Normal.name(), "normal");
        assert_eq!(Source::Instant.name(), "instant");
        assert_eq!(Source::from_u8(2), Source::Brooch);
        assert_eq!(Source::from_u8(4), Source::Manual);
    }
}
