//! Login / `char` block handling.
//!
//! `GameState::login` (17268 B at `0x101bd096c`) applies the character-select
//! output. Recovered literals: `"playerID"`, `"instance"`, `"settingsInfo"`,
//! `"char"`, `"app"`, `"minivitals"`, `"pbarStance"`, `"stance"`, `"id"`,
//! `"value"`, `"yourLvl"`, `"game<c>\nFREETRUE"`; calls `StatusInfo::get` and
//! the `hydra_protocol::write` helpers.
//!
//! `// UNKNOWN:` the full attribute mapping — the recovered body walks the
//! `settings`/`char` attribute tree, which is protocol-shaped.

impl crate::state::GameState {
    /// @0x0000000101bd096c (17268 B)
    ///
    /// Applies one line of the login block. Returns `true` when something the
    /// model tracks changed.
    pub fn login(&mut self, line: &str) -> bool {
        let mut changed = false;
        if let Some(value) = attribute(line, "yourLvl") {
            if let Ok(level) = value.parse::<u32>() {
                if self.character.level != level {
                    self.character.level = level;
                    changed = true;
                }
            }
        }
        if let Some(value) = attribute(line, "stance") {
            changed |= self.character.set_stance_text(&value);
        }
        if let Some(value) = attribute(line, "pbarStance") {
            changed |= self.character.set_stance_text(&value);
        }
        if let Some(value) = attribute(line, "playerID") {
            if let Ok(id) = value.parse::<u32>() {
                if self.character.exist_id != id {
                    self.character.exist_id = id;
                    changed = true;
                }
            }
        }
        if let Some(rest) = line.split_once("FREETRUE") {
            let _ = rest;
            if self.account.is_none() {
                self.account = Some(crate::state::character::vocabulary::AccountType::Free);
                changed = true;
            }
        }
        changed
    }
}

fn attribute(text: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::character::stance::Stance;

    #[test]
    fn level_and_stance_are_applied() {
        let mut state = crate::state::GameState::default();
        assert!(state.login("<settingsInfo yourLvl=\"42\" stance=\"Defensive (100%)\"/>"));
        assert_eq!(state.character.level, 42);
        assert_eq!(state.character.stance, Stance::Defensive);
        assert!(!state.login("<settingsInfo yourLvl=\"42\" stance=\"Defensive (100%)\"/>"));
    }

    #[test]
    fn player_id_is_stored() {
        let mut state = crate::state::GameState::default();
        assert!(state.login("<char playerID=\"7\"/>"));
        assert_eq!(state.character.exist_id, 7);
    }

    #[test]
    fn account_type_is_free_once() {
        let mut state = crate::state::GameState::default();
        assert!(state.login("game<c>\nFREETRUE"));
        assert!(!state.login("game<c>\nFREETRUE"));
        assert_eq!(
            state.account,
            Some(crate::state::character::vocabulary::AccountType::Free)
        );
    }

    #[test]
    fn unrelated_lines_change_nothing() {
        let mut state = crate::state::GameState::default();
        assert!(!state.login("You stand still."));
    }
}
