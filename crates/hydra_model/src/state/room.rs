//! Room state.
//!
//! `Room::saw_players` (4196 B) and `Room::component` (1848 B) fold the room
//! block in; recovered literals `"room players"`, `"id"`, `"value"`, `"text"`,
//! `"progressBar"`, `"Indefinite"`. `PlayerStatus` has a recovered `Display`
//! impl (5632 B) whose literals include `"Creature"`, `"Attacker"`,
//! `"Absorbed"`, the body parts and `"None"`.

use std::collections::BTreeMap;

/// A player/creature seen in the room block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerStatus {
    pub name: String,
    pub noun: String,
    /// The status words, as recovered from the `<player>` block.
    pub statuses: Vec<String>,
}

impl PlayerStatus {
    /// `<player id="123" name="Kithar">...`
    pub fn read(line: &str) -> Option<PlayerStatus> {
        let rest = line.split_once("<player")?.1;
        let name = attr(rest, "name")?;
        let noun = attr(rest, "noun").unwrap_or_default();
        let statuses = rest
            .split('|')
            .skip(1)
            .flat_map(|chunk| chunk.split(','))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Some(PlayerStatus {
            name,
            noun,
            statuses,
        })
    }
}

impl std::fmt::Display for PlayerStatus {
    /// @0x0000000101b6aa14 (5632 B)
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        if !self.noun.is_empty() {
            write!(f, " ({})", self.noun)?;
        }
        if !self.statuses.is_empty() {
            write!(f, ": {}", self.statuses.join(", "))?;
        }
        Ok(())
    }
}

/// The current room.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub id: Option<String>,
    pub title: String,
    pub description: String,
    pub players: Vec<PlayerStatus>,
    /// `progressBar`/`Indefinite` components, keyed by id.
    pub components: BTreeMap<String, String>,
}

impl Room {
    /// @0x0000000101b61fb4 (4196 B)
    pub fn saw_players(&mut self, line: &str) -> bool {
        let Some(rest) = line.strip_prefix("room players") else {
            return false;
        };
        let mut changed = false;
        for chunk in rest.split('|') {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            if let Some(player) = PlayerStatus::read(&format!("<player {chunk}")) {
                if !self.players.iter().any(|p| p.name == player.name) {
                    self.players.push(player);
                    changed = true;
                }
            }
        }
        changed
    }

    /// @0x0000000101b63018 (1848 B)
    pub fn component(&mut self, line: &str) -> bool {
        let id = attr(line, "id");
        let value = attr(line, "value").or_else(|| attr(line, "text"));
        let Some((id, value)) = id.zip(value) else {
            return false;
        };
        if self.components.get(&id) == Some(&value) {
            return false;
        }
        self.components.insert(id, value);
        true
    }

    pub fn forget_players(&mut self) -> bool {
        if self.players.is_empty() {
            return false;
        }
        self.players.clear();
        true
    }
}

fn attr(text: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = text.find(&needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_are_collected_once() {
        let mut room = Room::default();
        assert!(room.saw_players("room players id=\"1\" name=\"Kithar\" noun=\"wizard\""));
        assert_eq!(room.players.len(), 1);
        assert_eq!(room.players[0].name, "Kithar");
        assert!(!room.saw_players("room players id=\"1\" name=\"Kithar\" noun=\"wizard\""));
        assert!(room.forget_players());
        assert!(!room.forget_players());
    }

    #[test]
    fn player_status_displays_name_and_noun() {
        let status = PlayerStatus {
            name: "Kithar".into(),
            noun: "wizard".into(),
            statuses: vec!["kneeling".into()],
        };
        assert_eq!(status.to_string(), "Kithar (wizard): kneeling");
    }

    #[test]
    fn components_are_keyed_by_id() {
        let mut room = Room::default();
        assert!(room.component("<component id=\"roomDesc\" value=\"A dark cave.\"/>"));
        assert_eq!(room.components.get("roomDesc").unwrap(), "A dark cave.");
        assert!(!room.component("<component id=\"roomDesc\" value=\"A dark cave.\"/>"));
    }
}
