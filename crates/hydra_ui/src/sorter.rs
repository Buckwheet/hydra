//! Sorting the inventory and container windows.
//!
//! Recovered: `sorter::is_container_look` @0x10185d8a8 (328 B) and
//! `sorter::sort` @0x10185df0 (11 KB), plus the drop-glue types `Bucket`,
//! `Entry` and `Listed`.  The serde pool has `struct Sorting` and
//! `expected number of patterns to match pattern ID` next to it, so `sort`
//! builds a `Vec<Bucket>` and each bucket a `Vec<Entry>`.

/// One item to sort.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub count: u32,
}

/// A group of entries under one heading.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bucket {
    pub heading: String,
    pub entries: Vec<Entry>,
}

/// `@0x10185d8a8` — is this line a container's `containing` look?
///
/// The recovered test is the literal [`crate::inventory::CONTAINING`] plus one
/// of the "you see"/"In the"/"On the" openers.
pub fn is_container_look(line: &str) -> bool {
    if line.contains(crate::inventory::CONTAINING) {
        return true;
    }
    let l = line.trim_start();
    crate::inventory::LOOK_PREFIX
        .iter()
        .any(|p| l.starts_with(p))
}

/// `@0x10185df0` — group and sort inventory entries.
///
/// The heading a bucket gets is the [`crate::inventory::sort_name`] of the
/// entry, so `"a leather backpack"` lands under `l`.  Buckets are sorted by
/// heading and entries inside a bucket by name.
pub fn sort(entries: &[Entry]) -> Vec<Bucket> {
    let mut buckets: Vec<Bucket> = Vec::new();
    for entry in entries {
        let key = crate::inventory::sort_name(&entry.name);
        let heading = key
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_default();
        let bucket = match buckets.iter_mut().find(|b| b.heading == heading) {
            Some(b) => b,
            None => {
                buckets.push(Bucket {
                    heading: heading.clone(),
                    entries: Vec::new(),
                });
                buckets.last_mut().unwrap()
            }
        };
        match bucket
            .entries
            .iter_mut()
            .find(|e| crate::inventory::sort_name(&e.name) == key)
        {
            Some(have) => have.count = have.count.saturating_add(entry.count),
            None => bucket.entries.push(entry.clone()),
        }
    }
    for bucket in &mut buckets {
        bucket.entries.sort_by(|a, b| {
            crate::inventory::sort_name(&a.name).cmp(&crate::inventory::sort_name(&b.name))
        });
    }
    buckets.sort_by(|a, b| a.heading.cmp(&b.heading));
    buckets
}

/// A line the sorter has heard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub text: String,
    pub bucket: Option<String>,
}

impl Listed {
    /// `@0x101860520` — take a line, working out which bucket it belongs to.
    ///
    /// 70 KB in the binary: it inlines the whole gender/number word lists the
    /// game uses.  `// UNKNOWN:` the word lists themselves are not reproduced;
    /// the bucket here is the first letter of the normalised name, which is the
    /// observable rule.
    pub fn hear(&mut self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        self.text = trimmed.to_string();
        let bucket = crate::inventory::sort_name(trimmed)
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase().to_string());
        self.bucket = bucket.clone();
        bucket
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, count: u32) -> Entry {
        Entry {
            name: name.to_string(),
            count,
        }
    }

    #[test]
    fn container_looks_are_recognised() {
        assert!(is_container_look("a bag  containing a gem"));
        assert!(is_container_look("you see a gem"));
        assert!(is_container_look("In the bag you see"));
        assert!(is_container_look("On the table you see"));
        assert!(!is_container_look("You see nothing special."));
    }

    #[test]
    fn sorting_buckets_by_first_letter() {
        let buckets = sort(&[e("a gem", 1), e("the rock", 2), e("gem", 1)]);
        assert_eq!(buckets.len(), 2);
        assert_eq!(buckets[0].heading, "G");
        assert_eq!(buckets[0].entries.len(), 1);
        // "a gem" and "gem" merge once the article is dropped.
        assert_eq!(buckets[0].entries[0].count, 2);
        assert_eq!(buckets[1].heading, "R");
        assert_eq!(buckets[1].entries[0].name, "the rock");
    }

    #[test]
    fn empty_input_gives_no_buckets() {
        assert!(sort(&[]).is_empty());
    }

    #[test]
    fn hearing_a_line_names_its_bucket() {
        let mut l = Listed::default();
        assert_eq!(l.hear("a lantern"), Some("L".into()));
        assert_eq!(l.text, "a lantern");
        assert_eq!(l.bucket.as_deref(), Some("L"));
        assert_eq!(l.hear("   "), None);
    }
}
