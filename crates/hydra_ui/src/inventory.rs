//! The inventory window: containers, contents and merging.
//!
//! Recovered helpers all work over a `BTreeMap<String, …>` of containers and a
//! `Vec` of item lines.  Notable sizes: `sort_name` 7.8 KB (the item-name
//! normaliser), `filled` 1.1 KB (parsing the container look), `summary` 388 B.
//!
//! The two recovered constants that show up in the literals are `"  containing "`
//! (0x103992f3c, the container-look prefix) and `"you see "` / `"In the "` /
//! `"On the "` (0x103992f21).

use std::collections::BTreeMap;

/// One item in a container.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Item {
    /// The item's name, as the game wrote it.
    pub name: String,
    /// How many.
    pub count: u32,
    /// The `exist` id from `<a exist="..">`, if any.
    pub exist: Option<String>,
}

/// A container.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Container {
    /// The container's own name (`"a leather backpack"`).
    pub name: String,
    /// Everything inside it.
    pub items: Vec<Item>,
    /// Whether the game said it is closed.
    pub closed: bool,
    /// Whether it is locked.
    pub locked: bool,
}

/// The container look's prefix words: `you see `, `In the `, `On the `.
pub const LOOK_PREFIX: [&str; 3] = ["you see ", "In the ", "On the "];

/// The separator between a container's description and its contents.
pub const CONTAINING: &str = "  containing ";

/// `@0x1019529e0` — the "closed" marker in a container look.
pub fn shut(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("(closed)") || l.contains(" which is closed") || l.contains(" that is closed")
}

/// `@0x101952a64` — the top-level containers of a map.
pub fn roots(all: &BTreeMap<String, Container>) -> Vec<&str> {
    let mut names: Vec<&str> = all.keys().map(|s| s.as_str()).collect();
    names.sort_unstable();
    names
}

/// `@0x1019530b4` — the word that marks a container as locked.
pub fn locked(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    l.contains("(locked)") || l.contains(" locked") || l.contains("is locked")
}

/// `@0x10195311c` — the weight a `"a bag (5 lbs)"` look carries.
pub fn pounds(line: &str) -> Option<f32> {
    let open = line.rfind('(')?;
    let close = line[open..].find(')')? + open;
    let inside = line[open + 1..close].trim();
    let n = inside
        .strip_suffix("lbs")
        .or_else(|| inside.strip_suffix("lb"))
        .unwrap_or(inside)
        .trim();
    n.trim_end_matches('.').parse::<f32>().ok()
}

/// `@0x10195237c` — whether `weight` fits in `capacity`.
pub fn weight_within(weight: f32, capacity: f32) -> bool {
    capacity <= 0.0 || weight <= capacity
}

/// `@0x101952acc` — read a container look into items.
///
/// The recovered body splits on [`CONTAINING`] and then on commas, dropping the
/// leading prefix word.  `"a bag containing a gem, a rock"` yields two items.
pub fn filled(name: &str, look: &str) -> Container {
    let closed = shut(look);
    let locked = locked(look);
    let body = match look.split_once(CONTAINING) {
        Some((_, rest)) => rest,
        None => "",
    };
    let mut items = Vec::new();
    for part in body.split(',') {
        let mut part = part.trim();
        for prefix in LOOK_PREFIX {
            if let Some(rest) = part.strip_prefix(prefix) {
                part = rest.trim();
            }
        }
        let part = part.trim_end_matches('.').trim();
        if part.is_empty() {
            continue;
        }
        let count = part
            .split_whitespace()
            .find_map(|w| w.parse::<u32>().ok())
            .unwrap_or(1);
        items.push(Item {
            name: part.to_string(),
            count,
            exist: None,
        });
    }
    Container {
        name: name.to_string(),
        items,
        closed,
        locked,
    }
}

/// `@0x101952f0c` — merge two item lists, summing counts.
pub fn gather(items: &mut Vec<Item>, more: Vec<Item>) {
    for item in more {
        match items.iter_mut().find(|i| i.name == item.name) {
            Some(have) => have.count = have.count.saturating_add(item.count),
            None => items.push(item),
        }
    }
    items.sort();
}

/// `@0x101953300` — the container a look line belongs to, if it declares one.
pub fn container(line: &str) -> Option<&str> {
    let at = line.find(CONTAINING)?;
    let head = line[..at].trim();
    let mut name = head;
    for prefix in LOOK_PREFIX {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.trim();
        }
    }
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// `@0x10195317c` — the summary line: `"3 items, 12 lbs"`-ish.
pub fn summary(items: &[Item], weight: f32) -> String {
    let count: u32 = items.iter().map(|i| i.count).sum();
    let noun = if count == 1 { "item" } else { "items" };
    if weight > 0.0 {
        format!("{count} {noun}, {weight:.0} lbs")
    } else {
        format!("{count} {noun}")
    }
}

/// `@0x101953484` — whether the room the container came from was left.
pub fn left_room(container_home: &str, room: &str) -> bool {
    !container_home.is_empty() && container_home != room
}

/// `@0x1019534ec` — normalise an item name for sorting.
///
/// The binary strips the leading article, lowercases, and drops a leading
/// `"some "` / `"a "` / `"an "` / `"the "`, which is what makes
/// `"a leather backpack"` sort under `l`.
pub fn sort_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    for article in ["some ", "a ", "an ", "the "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.to_string();
        }
    }
    lower
}

/// The three inventory actions the window offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Sent,
    Named,
    Offered,
}

impl Act {
    /// `@0x1019524b0` — the command an action sends.
    ///
    /// `// UNKNOWN:` the recovered bodies build the command from the item name
    /// with `format!`; the verbs are the ones the literals next to this module
    /// name (`get`, `drop`, `put in`).
    pub fn sent(self, item: &str, container: &str) -> String {
        match self {
            Act::Sent => format!("get {item}"),
            Act::Named => format!("drop {item}"),
            Act::Offered => {
                if container.is_empty() {
                    format!("put {item} in my backpack")
                } else {
                    format!("put {item} in {container}")
                }
            }
        }
    }

    /// `@0x1019526fc` — the label the button carries.
    pub fn named(self) -> &'static str {
        match self {
            Act::Sent => "Get",
            Act::Named => "Drop",
            Act::Offered => "Put in",
        }
    }

    /// `@0x1019528a0` — the verb the action is described with.
    pub fn offered(self) -> &'static str {
        match self {
            Act::Sent => "get",
            Act::Named => "drop",
            Act::Offered => "put",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_look_parsing() {
        let look = "a leather backpack  containing a gem, some coins.";
        let c = filled("backpack", look);
        assert_eq!(c.name, "backpack");
        assert_eq!(c.items.len(), 2);
        assert_eq!(c.items[0].name, "a gem");
        assert_eq!(c.items[1].name, "some coins");
        assert!(!c.closed);
        assert_eq!(container(look), Some("a leather backpack"));
    }

    #[test]
    fn closed_and_locked_marks() {
        assert!(shut("a bag (closed)"));
        assert!(shut("a bag which is closed"));
        assert!(!shut("a bag"));
        assert!(locked("a chest (locked)"));
        assert!(!locked("a chest"));
        let c = filled("x", "a chest  containing nothing (closed) (locked)");
        assert!(c.closed);
        assert!(c.locked);
    }

    #[test]
    fn weight_parsing() {
        assert_eq!(pounds("a bag (5 lbs)"), Some(5.0));
        assert_eq!(pounds("a bag (5 lb)"), Some(5.0));
        assert_eq!(pounds("a bag (0.5 lbs)"), Some(0.5));
        assert_eq!(pounds("a bag"), None);
        assert!(weight_within(5.0, 0.0));
        assert!(weight_within(5.0, 5.0));
        assert!(!weight_within(6.0, 5.0));
    }

    #[test]
    fn sort_names_drop_articles() {
        assert_eq!(sort_name("a leather backpack"), "leather backpack");
        assert_eq!(sort_name("An Apple"), "apple");
        assert_eq!(sort_name("some coins"), "coins");
        assert_eq!(sort_name("the rock"), "rock");
        assert_eq!(sort_name("Lantern"), "lantern");
    }

    #[test]
    fn gathering_merges_counts() {
        let mut items = vec![Item {
            name: "gem".into(),
            count: 1,
            exist: None,
        }];
        gather(
            &mut items,
            vec![
                Item {
                    name: "gem".into(),
                    count: 2,
                    exist: None,
                },
                Item {
                    name: "rock".into(),
                    count: 1,
                    exist: None,
                },
            ],
        );
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "gem");
        assert_eq!(items[0].count, 3);
        assert_eq!(items[1].name, "rock");
    }

    #[test]
    fn summary_and_actions() {
        let items = vec![
            Item {
                name: "gem".into(),
                count: 2,
                exist: None,
            },
            Item {
                name: "rock".into(),
                count: 1,
                exist: None,
            },
        ];
        assert_eq!(summary(&items, 12.0), "3 items, 12 lbs");
        assert_eq!(summary(&items, 0.0), "3 items");
        assert_eq!(summary(&items[1..], 0.0), "1 item");
        assert_eq!(Act::Sent.sent("gem", ""), "get gem");
        assert_eq!(Act::Named.sent("gem", ""), "drop gem");
        assert_eq!(Act::Offered.sent("gem", "backpack"), "put gem in backpack");
        assert_eq!(Act::Sent.named(), "Get");
        assert_eq!(Act::Offered.offered(), "put");
    }

    #[test]
    fn roots_and_left_room() {
        let mut all = BTreeMap::new();
        for name in ["b", "a"] {
            all.insert(name.to_string(), Container::default());
        }
        assert_eq!(roots(&all), vec!["a", "b"]);
        assert!(left_room("room1", "room2"));
        assert!(!left_room("room1", "room1"));
        assert!(!left_room("", "room2"));
    }
}
