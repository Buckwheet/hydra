//! The Ebon Gate merchants: what they say, and the banner that announces them.
//!
//! **ESTABLISHED**:
//!
//!  * [`told`] (`0x101471b1c`) trims the plain runs and answers `Now open.`
//!    (`0x1024189b2`) when there are none and the room is empty, `Now working in
//!    {}.` when there is a room, and `Ebon Gate merchant {}: {}` otherwise.
//!  * [`heard`] (`0x101471ce0`) lower-cases what the merchant said, looks the
//!    word up as `egMerchant:{}:{}`, and calls
//!    `Highlights::call_once(…, 600, …)` so the same line is not announced
//!    twice; it also alerts the story when `merchant_notices` is on.

use crate::highlights::Highlights;

/// The word the merchant announcements are highlighted under.
///
/// **ESTABLISHED** (`0x103e6a79b`): `egMerchant:` followed by two `:`-separated
/// values.
pub const KEY: &str = "egMerchant";

/// What the banner says when no merchant is being served.
///
/// **ESTABLISHED** (`0x1024189b2`): `Now open.`
pub const OPEN: &str = "Now open.";

/// How long a merchant announcement stays up, in milliseconds.
///
/// **ESTABLISHED** (`0x101471e3c`): `mov w3, #0x258` = 600.
pub const SHOWN_MS: u32 = 600;

/// The name the merchant announcements are filed under.
///
/// **ESTABLISHED** (`0x1024189bb`): `Ebon Gate merchant`.
pub const TITLE: &str = "Ebon Gate merchant";

/// One merchant's current state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Merchant {
    /// What the merchant is called, as the banner shows it.
    pub name: String,
    /// Where it is working, when it is.
    pub room: String,
    /// The lines it is saying.
    pub runs: String,
    /// The number the announcement is keyed by, when there is one.
    pub end: Option<u32>,
}

impl Merchant {
    /// The banner's first line.
    ///
    /// **ESTABLISHED** (`0x101471b1c`): the three cases above, with the plain
    /// runs trimmed first.
    pub fn told(&self) -> String {
        let plain = self.runs.trim();
        if plain.is_empty() {
            if self.room.is_empty() {
                OPEN.to_string()
            } else {
                format!("Now working in {}.", self.room)
            }
        } else {
            format!("{TITLE} {}: {plain}", self.name)
        }
    }

    /// The key the announcement is highlighted and de-duplicated by.
    ///
    /// **ESTABLISHED** (`0x103e6a79b`): `egMerchant:<end>:<lower-cased words>`.
    pub fn key_of(&self, said: &str) -> String {
        format!(
            "{KEY}:{}:{}",
            self.end.unwrap_or(0),
            said.to_lowercase()
        )
    }
}

/// What a merchant told the character.
///
/// **ESTABLISHED** (`0x101471b1c`).
pub fn told(merchant: &Merchant) -> String {
    merchant.told()
}

/// The merchant was heard: highlight the line, once.
///
/// **ESTABLISHED** (`0x101471ce0`): the announcement is only made when the
/// highlights are not hushed and there is something new; the key is the one
/// [`Merchant::key_of`] builds.
pub fn heard(merchant: &Merchant, highlights: &mut Highlights) -> Result<String, String> {
    let plain = merchant.runs.trim();
    if plain.is_empty() {
        return Err("nothing was said".to_string());
    }
    if highlights.hushed() {
        return Ok(String::new());
    }
    let key = merchant.key_of(plain);
    Ok(highlights.call_once(&key, &merchant.told(), SHOWN_MS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_says_which_merchant_and_where() {
        let open = Merchant::default();
        assert_eq!(told(&open), "Now open.");
        let working = Merchant {
            room: "the wagon".into(),
            ..Merchant::default()
        };
        assert_eq!(told(&working), "Now working in the wagon.");
        let talking = Merchant {
            name: "a gnome".into(),
            runs: "  buy my wares  ".into(),
            ..Merchant::default()
        };
        assert_eq!(
            told(&talking),
            "Ebon Gate merchant a gnome: buy my wares"
        );
    }

    #[test]
    fn the_key_lower_cases_and_carries_the_number() {
        let merchant = Merchant {
            end: Some(12),
            ..Merchant::default()
        };
        assert_eq!(
            merchant.key_of("Buy My Wares"),
            "egMerchant:12:buy my wares"
        );
        assert_eq!(Merchant::default().key_of("x"), "egMerchant:0:x");
    }

    #[test]
    fn a_hushed_board_does_not_announce() {
        let mut highlights = Highlights::default();
        highlights.set_volume(0.0);
        let merchant = Merchant {
            name: "a gnome".into(),
            runs: "hello".into(),
            ..Merchant::default()
        };
        assert_eq!(heard(&merchant, &mut highlights).unwrap(), "");
        highlights.set_volume(1.0);
        assert!(heard(&merchant, &mut highlights).unwrap().contains("gnome"));
        assert!(heard(&Merchant::default(), &mut highlights).is_err());
    }
}
