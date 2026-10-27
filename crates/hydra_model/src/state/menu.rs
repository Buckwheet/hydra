//! The learned-command menu tables.
//!
//! Recovered: `LearnedCommands::absorb` (5380 B) parses the versioned TSV
//! (`# cmd` / `timestamp` / `1788300900.1.1.1` header, then
//! `coord\tlabel\tcommand\tcategory` rows such as
//! `2524,1541\tambush @\tambush #\t6`), `apply` (188 B) folds a
//! `MenuCommands` value in, `entry` (220 B), `novel` (132 B) and
//! `set_version` (156 B) are accessors; `MenuCommands::resolve_item` (1748 B) /
//! `resolve_grouped` (1576 B) expand `#`/`@` placeholders and `category_path`
//! (76 B) is the category key.

use std::collections::BTreeMap;

/// One menu entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Command {
    /// `"2524,1541"` in the recovered table.
    pub coord: String,
    /// The label shown in the menu (`"ambush @"`).
    pub label: String,
    /// The expanded command (`"ambush #"`).
    pub command: String,
    /// The category id (`"6"`, `"7_befriend"`, `"6_combat maneuvers"`).
    pub category: String,
    /// `false` until the entry is learned.
    pub learned: bool,
}

/// The menu, keyed by coord.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct MenuCommands {
    pub entries: BTreeMap<String, Command>,
}

impl MenuCommands {
    /// @0x0000000101b84988 (220 B)
    pub fn entry(&self, coord: &str) -> Option<&Command> {
        self.entries.get(coord)
    }

    /// @0x0000000101b84948 (64 B)
    pub fn get(&self, coord: &str) -> Option<&Command> {
        self.entries.get(coord)
    }

    /// @0x0000000101b83c4c (1748 B) — expand one entry's command.
    ///
    /// `#` is the free-text placeholder and `@` the target placeholder.
    pub fn resolve_item(&self, coord: &str, text: &str, target: &str) -> Option<String> {
        let entry = self.entries.get(coord)?;
        Some(
            entry
                .command
                .replace('#', text)
                .replace('@', target)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    /// @0x0000000101b84320 (1576 B) — expand every entry of a category.
    pub fn resolve_grouped(&self, category: &str, text: &str, target: &str) -> Vec<String> {
        self.entries
            .values()
            .filter(|entry| entry.category == category)
            .map(|entry| {
                entry
                    .command
                    .replace('#', text)
                    .replace('@', target)
            })
            .collect()
    }
}

/// @0x0000000101b84a64 (76 B) — the category path of an entry.
pub fn category_path(category: &str) -> Vec<&str> {
    category.split('_').collect()
}

/// The learned-command store.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct LearnedCommands {
    pub commands: MenuCommands,
    /// `# cmd` / `timestamp` header value.
    pub version: Option<String>,
    pub timestamp: Option<String>,
}

impl LearnedCommands {
    /// @0x0000000101b84988 (220 B)
    pub fn entry(&self, coord: &str) -> Option<&Command> {
        self.commands.entry(coord)
    }

    /// @0x0000000101b84c08 (132 B) — is this coord new?
    pub fn novel(&self, coord: &str) -> bool {
        !self.commands.entries.contains_key(coord)
    }

    /// @0x0000000101b84ab0 (156 B)
    pub fn set_version(&mut self, version: &str) -> bool {
        if self.version.as_deref() == Some(version) {
            return false;
        }
        self.version = Some(version.to_string());
        true
    }

    /// @0x0000000101b84b4c (188 B) — fold a whole menu in.
    pub fn apply(&mut self, menu: &MenuCommands) -> usize {
        self.absorb(&menu.entries.values().cloned().collect::<Vec<_>>())
    }

    /// @0x0000000101b84c8c (5380 B) — parse `coord\tlabel\tcommand\tcategory`
    /// rows; `#`-prefixed lines carry the version/timestamp header.
    pub fn absorb(&mut self, rows: &[Command]) -> usize {
        let mut stored = 0;
        for row in rows {
            if self.commands.entries.get(&row.coord) == Some(row) {
                continue;
            }
            self.commands.entries.insert(row.coord.clone(), row.clone());
            stored += 1;
        }
        stored
    }

    /// Parse the recovered on-disk format.
    pub fn read(&mut self, text: &str) -> usize {
        let mut rows = Vec::new();
        for line in text.split('\n') {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("# ") {
                if let Some(version) = rest.strip_prefix("cmd") {
                    self.set_version(version.trim());
                }
                continue;
            }
            if let Some(ts) = line.strip_prefix("timestamp") {
                self.timestamp = Some(ts.trim().to_string());
                continue;
            }
            let cells: Vec<&str> = line.split('\t').collect();
            if cells.len() < 4 || cells[0] == "coord" {
                continue;
            }
            rows.push(Command {
                coord: cells[0].to_string(),
                label: cells[1].to_string(),
                command: cells[2].to_string(),
                category: cells[3].to_string(),
                learned: true,
            });
        }
        self.absorb(&rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# cmd
timestamp\t1788300900.1.1.1
coord\tlabel\tcommand\tcategory
2524,1541\tambush @\tambush #\t6
2524,1546\tclear from befriend list\tbefriend clear #\t7_befriend
";

    fn learned() -> LearnedCommands {
        let mut learned = LearnedCommands::default();
        assert_eq!(learned.read(SAMPLE), 2);
        learned
    }

    #[test]
    fn version_and_rows_are_recovered_from_the_sample() {
        let learned = learned();
        assert_eq!(learned.version.as_deref(), Some(""));
        assert_eq!(learned.timestamp.as_deref(), Some("1788300900.1.1.1"));
        let entry = learned.entry("2524,1541").unwrap();
        assert_eq!(entry.label, "ambush @");
        assert_eq!(entry.command, "ambush #");
        assert_eq!(entry.category, "6");
    }

    #[test]
    fn resolve_substitutes_both_placeholders() {
        let learned = learned();
        assert_eq!(
            learned
                .commands
                .resolve_item("2524,1541", "kobold", "the kobold")
                .as_deref(),
            Some("ambush kobold")
        );
        assert_eq!(learned.commands.resolve_item("nope", "a", "b"), None);
    }

    #[test]
    fn grouped_resolution_filters_by_category() {
        let learned = learned();
        let resolved = learned.commands.resolve_grouped("7_befriend", "", "x");
        assert_eq!(resolved, vec!["befriend clear ".to_string()]);
    }

    #[test]
    fn novel_and_apply() {
        let learned = learned();
        assert!(!learned.novel("2524,1541"));
        assert!(learned.novel("2524,9999"));
        let mut other = LearnedCommands::default();
        assert_eq!(other.apply(&learned.commands), 2);
        assert_eq!(other.apply(&learned.commands), 0);
    }

    #[test]
    fn category_path_splits_on_underscore() {
        assert_eq!(category_path("6_combat maneuvers"), vec!["6", "combat maneuvers"]);
        assert_eq!(category_path("6"), vec!["6"]);
    }

    #[test]
    fn absorb_is_idempotent() {
        let mut learned = LearnedCommands::default();
        let row = Command {
            coord: "1,1".into(),
            label: "x".into(),
            command: "x #".into(),
            category: "6".into(),
            learned: true,
        };
        assert_eq!(learned.absorb(std::slice::from_ref(&row)), 1);
        assert_eq!(learned.absorb(&[row]), 0);
    }
}
