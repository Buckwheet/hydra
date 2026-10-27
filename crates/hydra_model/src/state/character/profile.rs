//! The `PROFILE` report.
//!
//! `Profile::read_lines` (2984 B) splits on `": "`, `" is "` and `" has "`
//! (recovered literals) and stores the resulting key/value pairs; the caller
//! in `roundtime_remaining_after` passes each line individually.

use std::collections::BTreeMap;

/// `PROFILE` key/value pairs.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub fields: BTreeMap<String, String>,
}

impl Profile {
    /// @0x0000000101beff60 (2984 B)
    pub fn read_lines(&mut self, lines: &[&str]) -> usize {
        let mut stored = 0;
        for line in lines {
            if self.read_line(line) {
                stored += 1;
            }
        }
        stored
    }

    /// One line: `"Name: Kithar"`, `"Kithar is a Human"`, `"Kithar has ..."`.
    pub fn read_line(&mut self, line: &str) -> bool {
        let line = line.trim();
        let pair = if let Some((key, value)) = line.split_once(": ") {
            Some((key.to_string(), value.to_string()))
        } else if let Some((key, value)) = line.split_once(" is ") {
            Some((format!("is {key}"), value.to_string()))
        } else if let Some((key, value)) = line.split_once(" has ") {
            Some((format!("has {key}"), value.to_string()))
        } else {
            None
        };
        let Some((key, value)) = pair else {
            return false;
        };
        if key.is_empty() || self.fields.get(&key) == Some(&value) {
            return false;
        }
        self.fields.insert(key, value);
        true
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_value_lines_are_stored() {
        let mut profile = Profile::default();
        assert!(profile.read_line("Fame: 1,234,567"));
        assert_eq!(profile.get("Fame"), Some("1,234,567"));
        assert!(!profile.read_line("Fame: 1,234,567"));
    }

    #[test]
    fn is_and_has_lines_get_prefixed_keys() {
        let mut profile = Profile::default();
        assert!(profile.read_line("Kithar is a Human"));
        assert_eq!(profile.get("is Kithar"), Some("a Human"));
        assert!(profile.read_line("Kithar has a beard"));
        assert_eq!(profile.get("has Kithar"), Some("a beard"));
    }

    #[test]
    fn prose_lines_are_ignored() {
        let mut profile = Profile::default();
        assert!(!profile.read_line("You see nothing special."));
        assert!(profile.fields.is_empty());
    }

    #[test]
    fn read_lines_counts_stored_rows() {
        let mut profile = Profile::default();
        let lines = ["Name: Kithar", "Name: Kithar", "Fame: 5"];
        assert_eq!(profile.read_lines(&lines), 2);
    }
}
