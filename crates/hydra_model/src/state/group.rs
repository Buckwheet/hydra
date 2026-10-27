//! The player's group.
//!
//! `Group::apply` (2144 B) folds group lines in, `Group::contains` (772 B) tests
//! membership, `Group::emptied` (244 B) reports that the group is now empty and
//! `classify` (128 B) is the line dispatcher. Recovered literal: `"leader"`
//! (the group leader marker) and `"your group"` (from the experience/group
//! messages).

/// A group member.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub leader: bool,
}

/// The player's group.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub members: Vec<Member>,
}

impl Group {
    /// @0x0000000101bba204 (2144 B)
    pub fn apply(&mut self, line: &str) -> bool {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix("You are now in a group with ")
            .or_else(|| line.strip_prefix("You join "))
            .or_else(|| line.strip_prefix("Your group now contains "))
        else {
            return false;
        };
        let mut changed = false;
        for name in rest.trim_end_matches('.').split(',') {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let leader = name.ends_with(" (leader)");
            let name = name.trim_end_matches(" (leader)").trim().to_string();
            if self.members.iter().any(|m| m.name == name) {
                continue;
            }
            self.members.push(Member { name, leader });
            changed = true;
        }
        changed
    }

    /// @0x0000000101bbab58 (772 B)
    pub fn contains(&self, name: &str) -> bool {
        self.members.iter().any(|m| m.name == name)
    }

    /// @0x0000000101b? (244 B) — the group has just been emptied.
    pub fn emptied(&mut self, line: &str) -> bool {
        let line = line.trim().to_ascii_lowercase();
        if line.contains("your group is now empty") || line.contains("you are no longer in a group")
        {
            if self.members.is_empty() {
                return false;
            }
            self.members.clear();
            return true;
        }
        false
    }

    pub fn leader(&self) -> Option<&str> {
        self.members
            .iter()
            .find(|m| m.leader)
            .map(|m| m.name.as_str())
    }
}

/// @0x0000000101bbae5c (128 B) — the line dispatcher.
pub fn classify(line: &str) -> Option<&'static str> {
    let line = line.trim();
    if line.starts_with("You are now in a group with ") {
        Some("group")
    } else if line.starts_with("You join ") {
        Some("join")
    } else if line.to_ascii_lowercase().contains("your group is now empty") {
        Some("empty")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_are_added_with_the_leader_flag() {
        let mut group = Group::default();
        assert!(group.apply("You are now in a group with Kithar (leader), Ael."));
        assert!(group.contains("Kithar"));
        assert!(group.contains("Ael"));
        assert_eq!(group.leader(), Some("Kithar"));
        assert!(!group.apply("You are now in a group with Kithar (leader), Ael."));
    }

    #[test]
    fn emptying_clears_the_group_once() {
        let mut group = Group::default();
        group.apply("You are now in a group with Ael");
        assert!(group.emptied("Your group is now empty."));
        assert!(!group.emptied("Your group is now empty."));
        assert!(group.members.is_empty());
    }

    #[test]
    fn classify_recognises_the_recovered_prefixes() {
        assert_eq!(classify("You join Ael"), Some("join"));
        assert_eq!(classify("Your group is now empty."), Some("empty"));
        assert_eq!(classify("nothing"), None);
    }
}
