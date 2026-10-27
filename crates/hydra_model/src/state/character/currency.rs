//! Coin/note holdings.
//!
//! Recovered literals from `Currency::absorb_chunk` (4060 B):
//! `" silver stored within your "`, `" promissory notes valued at a total of "`.
//! The serde `expecting` text `struct Currency` survives, so the type name is
//! right; the field set below is `// UNKNOWN:` beyond the recovered phrases.

/// Money as raw silver pieces plus banked notes.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Currency {
    /// Coins on hand, in silver pieces.
    pub silver: u64,
    /// Silver stored "within" a container/bank.
    pub stored: BTreeLike,
    /// Face value of promissory notes.
    pub notes: u64,
    /// The container phrase seen in the recovered message.
    pub holder: Option<String>,
}

/// A small ordered map kept separate to avoid a public `BTreeMap` in the API.
pub type BTreeLike = std::collections::BTreeMap<String, u64>;

impl Currency {
    /// @0x0000000101bf0b08 (4060 B)
    pub fn absorb_chunk(&mut self, line: &str) -> bool {
        let mut changed = false;
        if let Some((before, after)) = line.split_once(" silver stored within your ") {
            if let Some(value) = trailing_number(before) {
                changed |= self.silver != value;
                self.silver = value;
            }
            let holder = after.trim().trim_end_matches('.').to_string();
            if !holder.is_empty() && self.holder.as_deref() != Some(&holder) {
                self.holder = Some(holder.clone());
                self.stored.insert(holder, self.silver);
            }
        } else if let Some((_, after)) = line.split_once(" promissory notes valued at a total of ") {
            if let Some(value) = leading_number(after) {
                changed |= self.notes != value;
                self.notes = value;
            }
        }
        changed
    }
}

fn leading_number(text: &str) -> Option<u64> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn trailing_number(text: &str) -> Option<u64> {
    let digits: String = text
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect::<String>()
        .chars()
        .rev()
        .filter(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_silver_is_attributed_to_its_holder() {
        let mut currency = Currency::default();
        assert!(currency.absorb_chunk("You have 1,234 silver stored within your cloak."));
        assert_eq!(currency.silver, 1234);
        assert_eq!(currency.holder.as_deref(), Some("cloak"));
        assert_eq!(currency.stored.get("cloak"), Some(&1234));
        assert!(!currency.absorb_chunk("You have 1,234 silver stored within your cloak."));
    }

    #[test]
    fn promissory_notes_are_tracked() {
        let mut currency = Currency::default();
        assert!(currency.absorb_chunk("You have 40 promissory notes valued at a total of 40000"));
        assert_eq!(currency.notes, 40000);
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut currency = Currency::default();
        assert!(!currency.absorb_chunk("You have no money."));
        assert_eq!(currency, Currency::default());
    }
}
