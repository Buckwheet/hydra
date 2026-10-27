//! Bounty/task classification.
//!
//! `classify` (30880 B) is the second-largest function in the crate. Recovered
//! literals include the task text openers
//! `"It appears they have a creature problem they'd like you t…"`,
//! `"It appears they need your help in tracking down some kind…"`,
//! `"It appears that a local resident urgently needs our help i…"`,
//! `"The local furrier (?P<npc_name>.+) has an order to fill an…"`,
//! `"one of the guardsmen just inside the (?P<town3>Ta'Illist…"`,
//! `"^You have succeeded in your task and can return to the Adve…"`,
//! `"You have located (?:an?|some) (?P<item>.+) and should brin…"`,
//! the town names (`Ataum`, `Galeb`, `Mist Harbor`, `Cold River`,
//! `Kraken's Fall`) and the completion phrasing
//! `")?of them to complete your task\.$"`.
//!
//! `Task::requirement` (224 B) is the display string for a task.

/// The kind of bounty task.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskKind {
    #[default]
    Unknown,
    Creature,
    Heirloom,
    Skin,
    Herb,
    Escort,
}

/// One bounty task.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Task {
    pub kind: TaskKind,
    /// The NPC/town the task came from.
    pub npc: Option<String>,
    pub town: Option<String>,
    /// The creature/item to fetch.
    pub target: Option<String>,
    /// How many are required.
    pub count: u32,
}

impl Task {
    /// @0x0000000101bbbff80 (224 B) — the requirement text.
    pub fn requirement(&self) -> String {
        let target = self.target.as_deref().unwrap_or("");
        match self.kind {
            TaskKind::Creature => format!("kill {count} {target}", count = self.count),
            TaskKind::Heirloom => format!("find {count} {target}", count = self.count),
            TaskKind::Skin => format!("skin {count} {target}", count = self.count),
            TaskKind::Herb => format!("gather {count} {target}", count = self.count),
            TaskKind::Escort => format!("escort {target}"),
            TaskKind::Unknown => String::new(),
        }
    }
}

/// @0x0000000101bbc060 (30880 B)
///
/// `// UNKNOWN:` the task grammar (the recovered body is regex-driven and the
/// patterns live in the same long literals); only the recovered openers are
/// classified here.
pub fn classify(text: &str) -> Option<Task> {
    let kind = if text.contains("a creature problem they'd like you") {
        TaskKind::Creature
    } else if text.contains("tracking down some kind") {
        TaskKind::Heirloom
    } else if text.contains("The local furrier") {
        TaskKind::Skin
    } else if text.contains("urgently needs our help") {
        TaskKind::Herb
    } else if text.contains("one of the guardsmen just inside") {
        TaskKind::Escort
    } else {
        return None;
    };
    Some(Task {
        kind,
        npc: None,
        town: None,
        target: None,
        count: first_number(text).unwrap_or(0),
    })
}

/// The town names recovered from the string pool.
pub const TOWNS: [&str; 5] = [
    "Ataum",
    "Galeb",
    "Mist Harbor",
    "Cold River",
    "Kraken's Fall",
];

fn first_number(text: &str) -> Option<u32> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_openers_classify() {
        assert_eq!(
            classify("It appears they have a creature problem they'd like you to solve.")
                .unwrap()
                .kind,
            TaskKind::Creature
        );
        assert_eq!(
            classify("The local furrier wants an order filled.").unwrap().kind,
            TaskKind::Skin
        );
        assert_eq!(classify("Nothing to do today."), None);
    }

    #[test]
    fn requirement_texts() {
        let task = Task {
            kind: TaskKind::Creature,
            target: Some("kobolds".into()),
            count: 10,
            ..Task::default()
        };
        assert_eq!(task.requirement(), "kill 10 kobolds");
        assert_eq!(Task::default().requirement(), "");
    }

    #[test]
    fn towns_are_recovered() {
        assert_eq!(TOWNS.len(), 5);
        assert!(TOWNS.contains(&"Mist Harbor"));
        assert!(TOWNS.contains(&"Kraken's Fall"));
    }
}
