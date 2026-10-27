//! Ship look/report helpers.
//!
//! `ShipInfo::look` (660 B) and `ShipInfo::is_mine` (2644 B) work on a looked-at
//! ship description; recovered literals `" is "` and `": "`.

/// A ship observed in the room.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct ShipInfo {
    pub name: Option<String>,
    pub captain: Option<String>,
    pub description: String,
}

impl ShipInfo {
    /// @0x0000000101beec9c (660 B) — the displayed "look" text.
    pub fn look(&self) -> String {
        let mut out = String::new();
        if let Some(name) = &self.name {
            out.push_str(name);
        }
        if let Some(captain) = &self.captain {
            out.push_str(": ");
            out.push_str(captain);
        }
        if !self.description.is_empty() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&self.description);
        }
        out
    }

    /// @0x0000000101beef30 (2644 B) — is this the player's own ship?
    pub fn is_mine(&self, player: &str) -> bool {
        match &self.captain {
            Some(captain) => captain.eq_ignore_ascii_case(player),
            None => false,
        }
    }

    /// Parse a `"<name> is <captain>"` style line.
    pub fn read(&mut self, line: &str) -> bool {
        let line = line.trim();
        let Some((name, rest)) = line.split_once(" is ") else {
            return false;
        };
        let changed = self.name.as_deref() != Some(name.trim());
        self.name = Some(name.trim().to_string());
        self.description = rest.trim().to_string();
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn look_composes_the_text() {
        let info = ShipInfo {
            name: Some("the Sea Sprite".into()),
            captain: Some("Kithar".into()),
            description: "A swift sloop.".into(),
        };
        assert_eq!(info.look(), "the Sea Sprite: Kithar A swift sloop.");
    }

    #[test]
    fn ownership_uses_the_captain() {
        let info = ShipInfo {
            captain: Some("Kithar".into()),
            ..ShipInfo::default()
        };
        assert!(info.is_mine("kithar"));
        assert!(!info.is_mine("Ael"));
        assert!(!ShipInfo::default().is_mine("Kithar"));
    }

    #[test]
    fn read_splits_on_is() {
        let mut info = ShipInfo::default();
        assert!(info.read("the Sea Sprite is a swift sloop"));
        assert_eq!(info.name.as_deref(), Some("the Sea Sprite"));
        assert_eq!(info.description, "a swift sloop");
    }
}
