//! Character snapshots (save/restore between sessions).
//!
//! Recovered: `CharacterSnapshot::of` (1376 B) captures, `restore_into`
//! (1632 B) writes back, `is_known` (116 B) and `describes` (176 B) are
//! predicates. The serde `expecting` text `struct CharacterSnapshot` survives.
//!
//! `// UNKNOWN:` the field set. The snapshot must cover everything the
//! character block holds (the recovered `restore_into` copies the whole block),
//! so the reconstruction clones [`Character`] plus a group label.

use super::Character;
use serde::Deserialize;

/// The group label stored alongside a snapshot.
#[derive(Default, Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Group {
    pub leader: Option<String>,
    pub members: Vec<String>,
}

/// A captured character block.
#[derive(Default, Clone, Debug, PartialEq, Deserialize)]
pub struct CharacterSnapshot {
    pub character: Character,
    pub group: Group,
    /// `// UNKNOWN:` a version/marker word; `is_known` tests it.
    pub known: bool,
}

impl CharacterSnapshot {
    /// @0x0000000101c11560 (1376 B)
    pub fn of(character: &Character, group: Group) -> CharacterSnapshot {
        CharacterSnapshot {
            character: character.clone(),
            group,
            known: true,
        }
    }

    /// @0x0000000101c10f00 (1632 B) — write the snapshot back into `character`,
    /// returning the replaced block.
    pub fn restore_into(&self, character: &mut Character) -> Character {
        std::mem::replace(character, self.character.clone())
    }

    /// @0x0000000101c10? (116 B)
    pub fn is_known(&self) -> bool {
        self.known
    }

    /// @0x0000000101c11b34 (176 B) — does this snapshot describe `name`?
    pub fn describes(&self, name: &str) -> bool {
        self.character.name == name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_the_character() {
        let mut character = Character::default();
        character.set_identity("Kithar", "Human", "Wizard");
        let snapshot = CharacterSnapshot::of(&character, Group::default());
        character.set_identity("Other", "Elf", "Bard");
        let old = snapshot.restore_into(&mut character);
        assert_eq!(old.name, "Other");
        assert_eq!(character.name, "Kithar");
    }

    #[test]
    fn predicates() {
        let mut character = Character::default();
        character.name = "Kithar".into();
        let snapshot = CharacterSnapshot::of(&character, Group::default());
        assert!(snapshot.is_known());
        assert!(snapshot.describes("Kithar"));
        assert!(!snapshot.describes("kithar"));
    }

    #[test]
    fn group_label_is_kept() {
        let snapshot = CharacterSnapshot::of(
            &Character::default(),
            Group {
                leader: Some("Kithar".into()),
                members: vec!["Ael".into()],
            },
        );
        assert_eq!(snapshot.group.leader.as_deref(), Some("Kithar"));
        assert_eq!(snapshot.group.members, vec!["Ael".to_string()]);
    }
}
