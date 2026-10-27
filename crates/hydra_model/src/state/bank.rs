//! Bank state.
//!
//! `Account::local` (10572 B) parses the local-bank block; recovered literals:
//! `"Wehnimer's Landing"`, `", "`, `"Main Deck"`, `"come back to me when you
//! have enough money for our arrangeme…"`.

/// The player's bank account state.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The town the account belongs to.
    pub town: Option<String>,
    /// Balance in silver.
    pub balance: u64,
    /// Notes held.
    pub notes: u64,
}

impl Account {
    /// @0x0000000101bdf184 (10572 B)
    pub fn local(&mut self, line: &str) -> bool {
        let mut changed = false;
        if line.contains("come back to me when you have enough money for our arrangement") {
            if self.balance != 0 {
                self.balance = 0;
                changed = true;
            }
        }
        if let Some((_, rest)) = line.split_once("You have ") {
            let digits: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .filter(|c| c.is_ascii_digit())
                .collect();
            if let Ok(balance) = digits.parse::<u64>() {
                if self.balance != balance {
                    self.balance = balance;
                    changed = true;
                }
            }
        }
        if let Some((town, _)) = line.split_once(", ") {
            let town = town.trim();
            if line.contains("Wehnimer's Landing") || line.contains("Dock") {
                if self.town.as_deref() != Some(town) {
                    self.town = Some(town.to_string());
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_is_read() {
        let mut account = Account::default();
        assert!(account.local("You have 12,345 silvers in the bank."));
        assert_eq!(account.balance, 12345);
        assert!(!account.local("You have 12,345 silvers in the bank."));
    }

    #[test]
    fn insufficient_funds_resets_the_balance() {
        let mut account = Account::default();
        account.local("You have 500 silvers in the bank.");
        assert!(account.local("come back to me when you have enough money for our arrangement"));
        assert_eq!(account.balance, 0);
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut account = Account::default();
        assert!(!account.local("The banker nods."));
    }
}
