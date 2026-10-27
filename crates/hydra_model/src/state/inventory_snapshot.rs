//! The `INVENTORY` snapshot.
//!
//! Recovered: `apply_snapshot` (1592 B) / `apply_page` (768 B) /
//! `apply_detail` (736 B) fold the three block kinds in; `details` (224 B),
//! `get` (796 B), `total_weight` (368 B) and `is_closed` (3568 B) are
//! accessors. `is_closed` splits on `": "` (recovered literal).

use std::collections::BTreeMap;

/// One inventory item.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub count: u32,
    pub weight: u32,
    /// Extra detail lines from the `detail` block.
    pub detail: BTreeMap<String, String>,
}

/// The whole inventory snapshot.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct InventorySnapshot {
    pub items: Vec<Item>,
    /// `false` while the block is still arriving.
    pub closed: bool,
    /// Current page number, when paged.
    pub page: Option<u32>,
}

impl InventorySnapshot {
    /// @0x0000000101bed478 (1592 B)
    pub fn apply_snapshot(&mut self, line: &str) -> bool {
        let line = line.trim();
        if line.is_empty() {
            return false;
        }
        let (count, name) = split_count(line);
        if name.is_empty() {
            return false;
        }
        if let Some(existing) = self.items.iter_mut().find(|i| i.name == name) {
            if existing.count == count {
                return false;
            }
            existing.count = count;
            return true;
        }
        self.items.push(Item {
            name,
            count,
            ..Item::default()
        });
        true
    }

    /// @0x0000000101becd28 (768 B)
    pub fn apply_page(&mut self, line: &str) -> bool {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Page ") {
            let page = rest
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse().ok());
            if page != self.page && page.is_some() {
                self.page = page;
                return true;
            }
            return false;
        }
        if line.contains("Items in your inventory:") {
            return self.apply_snapshot(line);
        }
        false
    }

    /// @0x0000000101bed028 (736 B) — a detail line for one item.
    pub fn apply_detail(&mut self, line: &str) -> bool {
        let line = line.trim();
        let Some((item, rest)) = line.split_once(": ") else {
            return false;
        };
        let Some((key, value)) = rest.split_once(": ") else {
            return false;
        };
        let Some(entry) = self.items.iter_mut().find(|i| i.name == item.trim()) else {
            return false;
        };
        if entry.detail.get(key) == Some(&value.to_string()) {
            return false;
        }
        entry.detail.insert(key.to_string(), value.to_string());
        true
    }

    /// @0x0000000101bed? (224 B)
    pub fn details(&self, name: &str) -> Option<&BTreeMap<String, String>> {
        self.items
            .iter()
            .find(|i| i.name == name)
            .map(|i| &i.detail)
    }

    /// @0x0000000101bed? (796 B)
    pub fn get(&self, name: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.name == name)
    }

    /// @0x0000000101bed? (368 B)
    pub fn total_weight(&self) -> u32 {
        self.items
            .iter()
            .map(|item| item.weight.saturating_mul(item.count.max(1)))
            .sum()
    }

    /// @0x0000000101bedeac (3568 B) — has the block finished?
    pub fn is_closed(&self, line: &str) -> bool {
        line.contains("</inv>") || line.contains("Total items:") || line.contains("closed")
    }
}

fn split_count(line: &str) -> (u32, String) {
    let (count, name) = match line.split_once(' ') {
        Some((count, name)) => match count.parse::<u32>() {
            Ok(count) => (count, name),
            Err(_) => (1, line),
        },
        None => (1, line),
    };
    (count, name.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_rows_carry_counts() {
        let mut inventory = InventorySnapshot::default();
        assert!(inventory.apply_snapshot("3 a rusty sword"));
        assert_eq!(inventory.get("a rusty sword").unwrap().count, 3);
        assert!(!inventory.apply_snapshot("3 a rusty sword"));
        assert!(inventory.apply_snapshot("4 a rusty sword"));
        assert_eq!(inventory.get("a rusty sword").unwrap().count, 4);
    }

    #[test]
    fn detail_lines_attach_to_their_item() {
        let mut inventory = InventorySnapshot::default();
        inventory.apply_snapshot("1 a rusty sword");
        assert!(inventory.apply_detail("a rusty sword: Weight: 4"));
        assert_eq!(
            inventory.details("a rusty sword").unwrap().get("Weight"),
            Some(&"4".to_string())
        );
        assert!(!inventory.apply_detail("a rusty sword: Weight: 4"));
    }

    #[test]
    fn pages_are_tracked() {
        let mut inventory = InventorySnapshot::default();
        assert!(inventory.apply_page("Page 2"));
        assert_eq!(inventory.page, Some(2));
        assert!(!inventory.apply_page("Page 2"));
    }

    #[test]
    fn total_weight_sums_counts() {
        let mut inventory = InventorySnapshot::default();
        inventory.items.push(Item {
            name: "a".into(),
            count: 2,
            weight: 3,
            detail: BTreeMap::new(),
        });
        inventory.items.push(Item {
            name: "b".into(),
            count: 1,
            weight: 5,
            detail: BTreeMap::new(),
        });
        assert_eq!(inventory.total_weight(), 11);
    }

    #[test]
    fn close_markers() {
        let inventory = InventorySnapshot::default();
        assert!(inventory.is_closed("</inv>"));
        assert!(inventory.is_closed("Total items: 12"));
        assert!(!inventory.is_closed("a kobold"));
    }
}
