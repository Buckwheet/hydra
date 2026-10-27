//! Picking which character a relay line belongs to.
//!
//! Recovered from `hydra_table::relay::pick` (`0x10127d530`, `0x10127d9fc`).
//! `pick` (1.2 KB) resolves a name against the roster; `chosen` (84 KB) is the
//! window that shows the candidates — it is `egui` code plus four inlined
//! `hydra_session` state machines, so only its contract is recovered here.

use crate::roster;

/// A candidate the picker offers.
///
/// **ESTABLISHED** (`relay::pick::pick`, `0x10127d530`): the candidates are roster
/// entries (`0x68`-byte stride, `hydra_table::roster::Entry`) filtered by the name
/// typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The character the line would go to.
    pub character: String,
    /// The account it belongs to.
    pub account: String,
    /// The game it is played in.
    pub game_code: String,
    /// Its nickname, if it has one.
    pub nickname: Option<String>,
}

impl Candidate {
    /// The name the picker shows: the nickname when it has one.
    ///
    /// **INFERRED**: `Entry::card`'s nickname handling (`0x10129d344`) is what the
    /// picker displays, and the roster stores a nickname precisely to rename a
    /// character in the lists.
    pub fn label(&self) -> &str {
        self.nickname.as_deref().unwrap_or(&self.character)
    }
}

/// The characters whose name (or nickname, or account) holds `typed`.
///
/// **ESTABLISHED** (`relay::pick::pick`): the roster is read, then each entry is
/// compared case-insensitively against the typed name — the same ASCII fold
/// `roster::find_in` uses (`0x10129dc2c`).
pub fn pick(entries: &[roster::Entry], typed: &str) -> Vec<Candidate> {
    let typed = typed.trim();
    if typed.is_empty() {
        return Vec::new();
    }
    entries
        .iter()
        .filter(|entry| {
            entry.character.eq_ignore_ascii_case(typed)
                || entry.account.eq_ignore_ascii_case(typed)
                || entry
                    .nickname
                    .as_ref()
                    .is_some_and(|nickname| nickname.eq_ignore_ascii_case(typed))
        })
        .map(|entry| Candidate {
            character: entry.character.clone(),
            account: entry.account.clone(),
            game_code: entry.game_code.clone(),
            nickname: entry.nickname.clone(),
        })
        .collect()
}

/// Which candidate was chosen out of `candidates`.
///
/// **ESTABLISHED** (`relay::pick::chosen`, `0x10127d9fc`): the 84 KB body renders
/// the list with `egui` and answers the picked row; the two inlined
/// `chosen::{{closure}}` symbols (`0x10127ddbc`, `0x10127dec0`) are the row
/// callbacks.
///
/// **Departure**: no drawing here.  With exactly one candidate that candidate is
/// chosen, which is the case the window would auto-pick; with none or several
/// there is no answer.
pub fn chosen(candidates: &[Candidate]) -> Option<&Candidate> {
    match candidates {
        [only] => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(character: &str, account: &str, nickname: Option<&str>) -> roster::Entry {
        roster::Entry {
            character: character.to_string(),
            account: account.to_string(),
            game_code: "GS4".to_string(),
            favourite: false,
            nickname: nickname.map(|nickname| nickname.to_string()),
        }
    }

    #[test]
    fn a_name_matches_the_character_the_account_or_the_nickname() {
        let entries = vec![
            entry("Aelith", "main", Some("Aely")),
            entry("Wynter", "other", None),
        ];
        assert_eq!(pick(&entries, "aelith").len(), 1);
        assert_eq!(pick(&entries, "MAIN").len(), 1);
        assert_eq!(pick(&entries, "aely")[0].character, "Aelith");
        assert_eq!(pick(&entries, "nobody").len(), 0);
        assert_eq!(pick(&entries, "  ").len(), 0, "an empty name picks nothing");
    }

    #[test]
    fn a_lone_candidate_is_chosen() {
        let entries = vec![entry("Aelith", "main", Some("Aely"))];
        let candidates = pick(&entries, "aelith");
        let one = chosen(&candidates).expect("one candidate");
        assert_eq!(one.label(), "Aely");
        assert_eq!(one.account, "main");
        assert_eq!(one.game_code, "GS4");
        assert!(chosen(&[]).is_none());
        let many = pick(&vec![entry("Aelith", "main", None), entry("Aelith", "alt", None)], "aelith");
        assert!(chosen(&many).is_none(), "several candidates need a click");
    }
}
