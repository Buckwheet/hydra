//! Game-object classification.
//!
//! `classify` (3572 B) uses `text::item` / `text::creature`;
//! `ObjectTypes::is` (3228 B) compiles the object-type table with
//! `pattern::compile` and the recovered column header
//! `kind\tcategory\tfield\tpattern\ntype\taggressive npc\tname\t…`.

use crate::pattern::{compile, Pattern};

/// The object kinds the client tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKind {
    AggressiveNpc,
    PassiveNpc,
    Item,
    Container,
    Gem,
    Herb,
    Weapon,
    Armor,
}

impl ObjectKind {
    pub fn key(self) -> &'static str {
        match self {
            ObjectKind::AggressiveNpc => "aggressive npc",
            ObjectKind::PassiveNpc => "passive npc",
            ObjectKind::Item => "item",
            ObjectKind::Container => "container",
            ObjectKind::Gem => "gem",
            ObjectKind::Herb => "herb",
            ObjectKind::Weapon => "weapon",
            ObjectKind::Armor => "armor",
        }
    }

    pub fn parse(text: &str) -> Option<ObjectKind> {
        let text = text.trim();
        [
            ObjectKind::AggressiveNpc,
            ObjectKind::PassiveNpc,
            ObjectKind::Item,
            ObjectKind::Container,
            ObjectKind::Gem,
            ObjectKind::Herb,
            ObjectKind::Weapon,
            ObjectKind::Armor,
        ]
        .into_iter()
        .find(|kind| kind.key().eq_ignore_ascii_case(text))
    }
}

/// One object-type rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectType {
    pub kind: Option<ObjectKind>,
    pub category: String,
    pub field: String,
    pub matcher: Pattern,
}

impl ObjectType {
    pub fn new(kind: ObjectKind, category: &str, field: &str, pattern: &str) -> ObjectType {
        ObjectType {
            kind: Some(kind),
            category: category.to_string(),
            field: field.to_string(),
            matcher: compile(&format!("(?i){pattern}")),
        }
    }
}

/// The object-type table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectTypes {
    pub types: Vec<ObjectType>,
}

impl ObjectTypes {
    /// @0x0000000101b92224 (3228 B)
    pub fn is(&self, kind: ObjectKind, text: &str) -> bool {
        self.types
            .iter()
            .any(|rule| rule.kind == Some(kind) && rule.matcher.is_match(text))
    }

    pub fn matching(&self, text: &str) -> Option<ObjectKind> {
        self.types
            .iter()
            .find(|rule| rule.matcher.is_match(text))
            .and_then(|rule| rule.kind)
    }
}

/// @0x0000000101b92ec0 (3572 B)
///
/// `// UNKNOWN:` the recovered body classifies items/creatures with the table
/// above; with no table loaded only the passive-npc prose is decided.
pub fn classify(text: &str) -> Option<ObjectKind> {
    if text.contains("passive npc") {
        return Some(ObjectKind::PassiveNpc);
    }
    if text.contains("aggressive npc") {
        return Some(ObjectKind::AggressiveNpc);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_kinds_parse_their_keys() {
        assert_eq!(ObjectKind::parse("passive npc"), Some(ObjectKind::PassiveNpc));
        assert_eq!(ObjectKind::parse("Item"), Some(ObjectKind::Item));
        assert_eq!(ObjectKind::parse("spaceship"), None);
    }

    #[test]
    fn classify_uses_the_marker_words() {
        assert_eq!(classify("the town guard is a passive npc"), Some(ObjectKind::PassiveNpc));
        assert_eq!(classify("a kobold is an aggressive npc"), Some(ObjectKind::AggressiveNpc));
        assert_eq!(classify("a rusty sword"), None);
    }

    #[test]
    fn table_lookup_matches_kind_and_pattern() {
        let table = ObjectTypes {
            types: vec![ObjectType::new(ObjectKind::Weapon, "item", "name", "^a .* sword$")],
        };
        assert!(table.is(ObjectKind::Weapon, "a rusty sword"));
        assert!(!table.is(ObjectKind::Weapon, "a rusty shield"));
        assert!(!table.is(ObjectKind::Armor, "a rusty sword"));
        assert_eq!(table.matching("a rusty sword"), Some(ObjectKind::Weapon));
    }
}
