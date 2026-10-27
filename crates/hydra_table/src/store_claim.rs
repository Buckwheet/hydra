//! Claiming what the store remembers about an account.
//!
//! Recovered from `hydra_table::store_claim` (`0x1012768a0`).  `accounts` is 27 KB:
//! it walks the roster, asks the ledger for each character's latest store visits
//! (`hydra_ledger::reader::Reader::latest_store_claims`, `0x10168c3b0`) and reports
//! who has anything to collect.  `hydra_ledger` is **not** reconstructed — see
//! [`crate::ledger`] — so this module is the roster half of that walk.

use crate::ledger;
use crate::roster;

/// Which characters of an account have something waiting at the store.
///
/// **ESTABLISHED** (`store_claim::accounts`, `0x1012768a0`): `ledger::open`
/// (`0x101225424`) is tried first and its absence is fine (`cbz x0` jumps past the
/// ledger query), then `roster::all` (`0x10129cb04`) supplies the characters and
/// the ledger answers each one's latest claims.
///
/// **Departure**: without `hydra_ledger` no claims are ever recorded, so the answer
/// is the roster with an empty claim list per character.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Claimable {
    /// The character's roster entry.
    pub entry: roster::Entry,
    /// What the ledger says it can collect.
    pub claims: Vec<String>,
}

/// The characters of `dir`'s roster, with what each can claim.
///
/// **ESTABLISHED**: the roster read and the ledger probe; the roster's I/O error is
/// flattened to `Err(error.to_string())` because the original's answer carries the
/// message text.
pub fn accounts(dir: &std::path::Path) -> Result<Vec<Claimable>, String> {
    let entries = roster::all(dir).map_err(|error| error.to_string())?;
    // The original probes the ledger once per run (`cbz x0` right after
    // `ledger::open`); with no ledger there is nothing to ask it.
    let _ledger = ledger::open();
    Ok(entries
        .into_iter()
        .map(|entry| Claimable {
            entry,
            claims: Vec::new(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra_table_store_{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn the_roster_is_walked_in_key_order() {
        let dir = dir("accounts");
        roster::record(
            &dir,
            &roster::Entry {
                character: "Wynter".to_string(),
                account: "main".to_string(),
                game_code: "GS4".to_string(),
                favourite: false,
                nickname: None,
            },
        )
        .expect("record");
        roster::record(
            &dir,
            &roster::Entry {
                character: "Aelith".to_string(),
                account: "main".to_string(),
                game_code: "GS4".to_string(),
                favourite: false,
                nickname: None,
            },
        )
        .expect("record");
        let accounts = accounts(&dir).expect("accounts");
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].entry.character, "Aelith", "gs4:aelith sorts first");
        assert_eq!(accounts[1].entry.character, "Wynter");
        assert!(accounts[0].claims.is_empty(), "no ledger, no claims");
    }

    #[test]
    fn a_missing_roster_is_an_empty_answer() {
        assert!(accounts(&dir("missing")).expect("accounts").is_empty());
    }
}
