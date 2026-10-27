//! Targeting state.
//!
//! `Targeting::read` (1108 B) folds a `TARGET` output block in.
//!
//! `// UNKNOWN:` the exact fields; the recovered body stores a list of target
//! names plus the selected index.

/// The player's target list.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Targeting {
    pub targets: Vec<String>,
    pub selected: Option<usize>,
}

impl Targeting {
    /// @0x0000000101b5bebc (1108 B)
    pub fn read(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        // A target block lists "1. a kobold" style entries.
        let numbered = line
            .split_once('.')
            .map(|(n, rest)| (n.trim().parse::<usize>().ok(), rest.trim()))
            .and_then(|(n, rest)| n.map(|n| (n, rest)));
        if let Some((n, name)) = numbered {
            if name.is_empty() {
                return false;
            }
            if n >= 1 {
                while self.targets.len() < n {
                    self.targets.push(String::new());
                }
                if self.targets[n - 1] == name {
                    return false;
                }
                self.targets[n - 1] = name.to_string();
                return true;
            }
        }
        false
    }

    pub fn select(&mut self, index: usize) -> bool {
        if index < self.targets.len() && self.selected != Some(index) {
            self.selected = Some(index);
            return true;
        }
        false
    }

    pub fn selected_target(&self) -> Option<&str> {
        self.selected
            .and_then(|index| self.targets.get(index))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_entries_are_stored_in_order() {
        let mut targeting = Targeting::default();
        assert!(targeting.read("1. a kobold"));
        assert!(targeting.read("2. a goblin"));
        assert_eq!(targeting.targets, vec!["a kobold", "a goblin"]);
        assert!(!targeting.read("2. a goblin"));
    }

    #[test]
    fn selection_is_bounds_checked() {
        let mut targeting = Targeting::default();
        targeting.read("1. a kobold");
        assert!(targeting.select(0));
        assert!(!targeting.select(0));
        assert!(!targeting.select(3));
        assert_eq!(targeting.selected_target(), Some("a kobold"));
    }

    #[test]
    fn prose_is_ignored() {
        let mut targeting = Targeting::default();
        assert!(!targeting.read("You have no targets."));
    }
}
