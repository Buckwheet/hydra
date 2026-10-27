//! Objectives / task tracking.
//!
//! `Objectives::apply` (2548 B) folds an objective block in.
//! `// UNKNOWN:` the block grammar; the recovered body stores key/value rows.

use std::collections::BTreeMap;

/// The player's objective list.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Objectives {
    pub entries: BTreeMap<String, String>,
    pub new_tasks: bool,
}

impl Objectives {
    /// @0x0000000101bec0b0 (2548 B)
    pub fn apply(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        let Some((key, value)) = line.split_once(": ") else {
            return false;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            return false;
        }
        if self.entries.get(key) == Some(&value.to_string()) {
            return false;
        }
        self.entries.insert(key.to_string(), value.to_string());
        if key.eq_ignore_ascii_case("new task") {
            self.new_tasks = true;
        }
        true
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_stored_once() {
        let mut objectives = Objectives::default();
        assert!(objectives.apply("Task: clear the kobolds"));
        assert_eq!(objectives.get("Task"), Some("clear the kobolds"));
        assert!(!objectives.apply("Task: clear the kobolds"));
    }

    #[test]
    fn new_task_sets_the_flag() {
        let mut objectives = Objectives::default();
        assert!(objectives.apply("New task: kill 10 kobolds"));
        assert!(objectives.new_tasks);
    }

    #[test]
    fn prose_is_ignored() {
        let mut objectives = Objectives::default();
        assert!(!objectives.apply("You have no objectives."));
    }
}
