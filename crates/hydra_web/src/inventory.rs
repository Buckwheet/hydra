//! The inventory panel.
//!
//! Recovered from `inventory::inventory` (`0x1002c3f38`), `inventory::offered`
//! (`0x1002c3e60`), `inventory::add` (`0x1002c3708`) and the serde glue of
//! `InventoryOut` (`0x1002c3434`), `GroupOut` (`0x1002c31c4`), `ItemOut`
//! (`0x1002c2fe4`), `DetailOut` (`0x1002c32fc`) and `ActOut` (`0x1002c2ec8`).

use hydra_model::state::inventory_snapshot::InventorySnapshot;

use crate::json::Value;

/// The actions the page may offer for an item.
///
/// **ESTABLISHED** command words from the literal pool (`0x102071900`):
/// `inspect/look/right/left/drop/wear/open/close`, with labels `Inspect`,
/// `Look`, `To right hand`, `To left hand`, `Drop`, `Wear`, `Open`, `Close`.
/// The original enum lives in the unrecovered `hydra_ui::inventory`, so it is
/// modelled here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Act {
    Inspect,
    Look,
    Right,
    Left,
    Drop,
    Wear,
    Open,
    Close,
}

impl Act {
    /// Every act, in the recovered order.
    pub const ALL: [Act; 8] = [
        Act::Inspect,
        Act::Look,
        Act::Right,
        Act::Left,
        Act::Drop,
        Act::Wear,
        Act::Open,
        Act::Close,
    ];

    /// The command word sent to the game.
    pub fn command(self) -> &'static str {
        match self {
            Act::Inspect => "inspect",
            Act::Look => "look",
            Act::Right => "right",
            Act::Left => "left",
            Act::Drop => "drop",
            Act::Wear => "wear",
            Act::Open => "open",
            Act::Close => "close",
        }
    }

    /// The label shown on the button.
    pub fn label(self) -> &'static str {
        match self {
            Act::Inspect => "Inspect",
            Act::Look => "Look",
            Act::Right => "To right hand",
            Act::Left => "To left hand",
            Act::Drop => "Drop",
            Act::Wear => "Wear",
            Act::Open => "Open",
            Act::Close => "Close",
        }
    }

    /// The act for a wire name (`hydra_ui::inventory::Act::named`).
    pub fn named(name: &str) -> Option<Act> {
        let name = name.trim();
        Act::ALL
            .into_iter()
            .find(|a| a.command().eq_ignore_ascii_case(name) || a.label().eq_ignore_ascii_case(name))
    }

    /// Does this act apply to an item in `snapshot`?  (`hydra_ui::inventory::
    /// Act::offered` + `container`/`shut` in the original.)
    pub fn offered(self, item: Option<&Item>) -> bool {
        let Some(item) = item else { return false };
        let noun = item.detail.get("noun").map(String::as_str).unwrap_or("");
        match self {
            // Anything can be inspected, looked at and dropped.
            Act::Inspect | Act::Look | Act::Drop => true,
            // Anything carried can be moved to a hand.
            Act::Right | Act::Left => true,
            // Only wearables can be worn.
            Act::Wear => noun != "box" && noun != "container",
            // Only containers can be opened or closed.
            Act::Open | Act::Close => {
                matches!(noun, "box" | "container" | "backpack" | "satchel" | "case")
            }
        }
    }
}

/// A container group (`GroupOut`): `ids, heading, tab`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupOut {
    pub ids: Vec<String>,
    pub heading: String,
    pub tab: String,
}

impl GroupOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            (
                "ids",
                Value::Arr(self.ids.iter().map(|s| Value::str(s.clone())).collect()),
            ),
            ("heading", Value::str(self.heading.clone())),
            ("tab", Value::str(self.tab.clone())),
        ]
    }
}

/// One item row (`ItemOut`): `within, id, parent, noun, name, pounds, acts,
/// count, fill, container, locked`.
///
/// `// UNKNOWN:` the original `ItemOut` is larger than the recovered field set
/// (its `name` sits at `+0x60` and `pounds` at `+0x120`), so a few unread fields
/// live in between; only the serialized keys are modelled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemOut {
    pub within: String,
    pub id: String,
    pub parent: String,
    pub noun: String,
    pub name: String,
    pub pounds: String,
    pub acts: Vec<ActOut>,
    pub count: u32,
    pub fill: String,
    pub container: bool,
    pub locked: bool,
}

impl ItemOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("within", Value::str(self.within.clone())),
            ("id", Value::str(self.id.clone())),
            ("parent", Value::str(self.parent.clone())),
            ("noun", Value::str(self.noun.clone())),
            ("name", Value::str(self.name.clone())),
            ("pounds", Value::str(self.pounds.clone())),
            (
                "acts",
                Value::Arr(self.acts.iter().map(|a| crate::wire::object(a.fields())).collect()),
            ),
            ("count", Value::int(self.count)),
            ("fill", Value::str(self.fill.clone())),
            ("container", Value::Bool(self.container)),
            ("locked", Value::Bool(self.locked)),
        ]
    }
}

/// A detail section (`DetailOut`): `id, name, sections`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DetailOut {
    pub id: String,
    pub name: String,
    pub sections: Vec<GroupOut>,
}

impl DetailOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("id", Value::str(self.id.clone())),
            ("name", Value::str(self.name.clone())),
            (
                "sections",
                Value::Arr(
                    self.sections
                        .iter()
                        .map(|g| crate::wire::object(g.fields()))
                        .collect(),
                ),
            ),
        ]
    }
}

/// A single offered act (`ActOut`): `act, label`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActOut {
    pub act: String,
    pub label: String,
}

impl ActOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("act", Value::str(self.act.clone())),
            ("label", Value::str(self.label.clone())),
        ]
    }

    /// The acts offered for one item.
    pub fn offered(item: Option<&Item>) -> Vec<ActOut> {
        Act::ALL
            .into_iter()
            .filter(|a| a.offered(item))
            .map(|a| ActOut {
                act: a.command().to_string(),
                label: a.label().to_string(),
            })
            .collect()
    }
}

/// The panel payload (`InventoryOut`): `summary, groups, items, item, known,
/// room_left`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InventoryOut {
    pub summary: String,
    pub groups: Vec<GroupOut>,
    pub items: Vec<ItemOut>,
    pub item: Option<DetailOut>,
    pub known: bool,
    pub room_left: bool,
}

impl InventoryOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("summary", Value::str(self.summary.clone())),
            (
                "groups",
                Value::Arr(
                    self.groups
                        .iter()
                        .map(|g| crate::wire::object(g.fields()))
                        .collect(),
                ),
            ),
            (
                "items",
                Value::Arr(self.items.iter().map(|i| crate::wire::object(i.fields())).collect()),
            ),
            (
                "item",
                match &self.item {
                    Some(d) => crate::wire::object(d.fields()),
                    None => Value::Null,
                },
            ),
            ("known", Value::Bool(self.known)),
            ("room_left", Value::Bool(self.room_left)),
        ]
    }
}

type Item = hydra_model::state::inventory_snapshot::Item;

/// Where an item lives, as the page groups it.
///
/// **ESTABLISHED** captions from the pool (`0x102072380`): `Hands`, `Worn`,
/// `Worn containers`, `At your feet`, `Reserved`, `Room containers`,
/// `On the ground`.
pub fn within(item: &Item, snapshot: &InventorySnapshot) -> &'static str {
    // `hydra_ui::inventory::container` / `shut` / `roots` in the original.
    if item.detail.contains_key("worn") {
        "Worn"
    } else if item.detail.contains_key("held") {
        "Hands"
    } else if item.detail.contains_key("in") {
        "Worn containers"
    } else if snapshot.closed {
        "On the ground"
    } else {
        "At your feet"
    }
}

/// Convert one item (`inventory::add`, `0x1002c3708`).
///
/// Recovered: the row is built from `hydra_ui::inventory::{container, shut,
/// filled, locked, pounds, weight_within, gather}` plus `Act::offered`, and the
/// fill word is one of the recovered `full/critical/filling` strings.
pub fn add(snapshot: &InventorySnapshot, parent: &str, item: &Item) -> ItemOut {
    let fill = item
        .detail
        .get("fill")
        .cloned()
        .unwrap_or_else(|| String::from("filling"));
    ItemOut {
        within: within(item, snapshot).to_string(),
        id: item.name.clone(),
        parent: parent.to_string(),
        noun: item
            .detail
            .get("noun")
            .cloned()
            .unwrap_or_default(),
        name: item.name.clone(),
        pounds: format!("{}", item.weight),
        acts: ActOut::offered(Some(item)),
        count: item.count,
        fill,
        container: item.detail.contains_key("in") || item.detail.contains_key("container"),
        locked: item.detail.get("locked").map(|v| v == "yes").unwrap_or(false),
    }
}

/// The whole panel (`0x1002c3f38`).
///
/// Recovered: `hydra_ui::inventory::{roots, summary, left_room}` provide the
/// grouping and the summary line, `InventorySnapshot::{get, details}` the item
/// detail, and each row goes through [`add`].
pub fn inventory(snapshot: &InventorySnapshot, summary: &str, room_left: bool) -> InventoryOut {
    let mut groups: Vec<GroupOut> = Vec::new();
    let mut items = Vec::new();
    for item in &snapshot.items {
        let heading = within(item, snapshot);
        match groups.iter_mut().find(|g| g.heading == heading) {
            Some(group) => group.ids.push(item.name.clone()),
            None => groups.push(GroupOut {
                ids: vec![item.name.clone()],
                heading: heading.to_string(),
                tab: "items".to_string(),
            }),
        }
        items.push(add(snapshot, "", item));
    }
    InventoryOut {
        summary: summary.to_string(),
        groups,
        items,
        item: None,
        known: !snapshot.items.is_empty(),
        room_left,
    }
}

/// Does the item offer that act (`0x1002c3e60`)?
///
/// Recovered: `InventorySnapshot::get` for the item, `Act::named` for the act
/// name, then `hydra_ui::inventory::container`/`shut` and `Act::offered`.
pub fn offered(snapshot: &InventorySnapshot, item: &str, act: &str) -> bool {
    let Some(act) = Act::named(act) else {
        return false;
    };
    act.offered(snapshot.get(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_model::state::inventory_snapshot::Item;

    fn snapshot() -> InventorySnapshot {
        let mut snap = InventorySnapshot::default();
        snap.items.push(Item {
            name: "a leather backpack".into(),
            count: 1,
            weight: 5,
            detail: [("noun".to_string(), "backpack".to_string())]
                .into_iter()
                .collect(),
        });
        snap.items.push(Item {
            name: "a rusty sword".into(),
            count: 1,
            weight: 8,
            detail: [("worn".to_string(), "yes".to_string())]
                .into_iter()
                .collect(),
        });
        snap
    }

    #[test]
    fn act_words_and_labels() {
        assert_eq!(Act::Inspect.command(), "inspect");
        assert_eq!(Act::Right.label(), "To right hand");
        assert_eq!(Act::named("wear"), Some(Act::Wear));
        assert_eq!(Act::named("To left hand"), Some(Act::Left));
        assert_eq!(Act::named("nope"), None);
    }

    #[test]
    fn acts_offered_depend_on_the_item() {
        let sword = Item {
            name: "a rusty sword".into(),
            count: 1,
            weight: 8,
            detail: Default::default(),
        };
        let acts = ActOut::offered(Some(&sword));
        let names: Vec<&str> = acts.iter().map(|a| a.act.as_str()).collect();
        assert!(names.contains(&"inspect"));
        assert!(!names.contains(&"open"));
        assert_eq!(ActOut::offered(None), vec![]);
        assert!(offered(&snapshot(), "a leather backpack", "open"));
        assert!(offered(&snapshot(), "a leather backpack", "close"));
        assert!(!offered(&snapshot(), "a rusty sword", "close"));
        // A missing item offers nothing.
        assert!(!offered(&snapshot(), "nothing here", "look"));
    }

    #[test]
    fn panel_groups_and_json() {
        let out = inventory(&snapshot(), "You are carrying 2 items.", false);
        assert_eq!(out.groups.len(), 2);
        assert_eq!(out.groups[0].heading, "At your feet");
        assert_eq!(out.groups[1].heading, "Worn");
        assert!(out.known);
        let json = crate::wire::object(out.fields()).to_json();
        assert!(json.starts_with(r#"{"summary":"You are carrying 2 items.","groups":[{"ids":["a leather backpack"],"heading":"At your feet","tab":"items"}"#), "{json}");
        assert!(json.contains(r#""item":null,"known":true,"room_left":false}"#));
        // `add` fills the row.
        let row = add(&snapshot(), "", &snapshot().items[0]);
        assert_eq!(row.pounds, "5");
        assert_eq!(row.fill, "filling");
    }
}
