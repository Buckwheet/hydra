//! Editing a [`HighlightFile`] from the settings page.
//!
//! Recovered: `category_names` @0x101848134 (216 B), `named` @0x101848584
//! (768 B), `put` @0x10184820c (888 B), `remove` @0x101848884 (336 B) and
//! `switch` @0x1018489d4 (1.3 KB).  `switch` is the interesting one: it flips a
//! category on or off and re-derives every rule's `off` flag from it.

use super::file::{HighlightFile, Mark, Rule};

impl HighlightFile {
    /// `@0x101848134` — every category that has at least one rule.
    pub fn category_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .categories
            .iter()
            .filter(|(_, rules)| !rules.is_empty())
            .map(|(name, _)| name.clone())
            .collect();
        for rule in self.highlight.values() {
            if let Mark::Named(name) = &rule.category {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names.sort();
        names
    }

    /// `@0x101848584` — the rules whose pattern contains `needle`.
    pub fn named(&self, needle: &str) -> Vec<(&str, &Rule)> {
        let needle = needle.to_lowercase();
        self.highlight
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&needle))
            .map(|(name, rule)| (name.as_str(), rule))
            .collect()
    }

    /// `@0x10184820c` — add or replace a rule.
    pub fn put(&mut self, name: &str, rule: Rule) -> Option<Rule> {
        let category = rule.category.name().to_string();
        if !category.is_empty() {
            let list = self.categories.entry(category).or_default();
            list.retain(|r| r.pattern != rule.pattern);
            list.push(rule.clone());
        }
        self.highlight.insert(name.to_string(), rule)
    }

    /// `@0x101848884` — drop a rule by name.
    pub fn remove(&mut self, name: &str) -> Option<Rule> {
        let rule = self.highlight.remove(name)?;
        let category = rule.category.name().to_string();
        if let Some(list) = self.categories.get_mut(&category) {
            list.retain(|r| r.pattern != rule.pattern);
        }
        Some(rule)
    }

    /// `@0x1018489d4` — switch a category on or off.
    ///
    /// Every rule in the category gets its `off` flag set to `!on`; rules in
    /// other categories are untouched.
    pub fn switch(&mut self, category: &str, on: bool) -> usize {
        let mut changed = 0;
        if let Some(rules) = self.categories.get_mut(category) {
            for rule in rules.iter_mut() {
                if rule.off == on {
                    rule.off = !on;
                    changed += 1;
                }
            }
        }
        let patterns: Vec<String> = self
            .categories
            .get(category)
            .map(|rules| rules.iter().map(|r| r.pattern.clone()).collect())
            .unwrap_or_default();
        for pattern in patterns {
            if let Some(rule) = self.highlight.get_mut(&pattern) {
                rule.off = !on;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, category: &str) -> Rule {
        Rule {
            pattern: pattern.to_string(),
            category: Mark::named(category),
            ..Rule::default()
        }
    }

    #[test]
    fn category_names_are_sorted_and_complete() {
        let mut f = HighlightFile::default();
        f.put("a", rule("a", "room"));
        f.put("b", rule("b", "combat"));
        assert_eq!(f.category_names(), vec!["combat", "room"]);
    }

    #[test]
    fn named_searches_case_insensitively() {
        let mut f = HighlightFile::default();
        f.put("Burn", rule("burning", "status"));
        f.put("Freeze", rule("frozen", "status"));
        assert_eq!(f.named("burn").len(), 1);
        assert_eq!(f.named("BURN").len(), 1);
        assert_eq!(f.named("zzz").len(), 0);
    }

    #[test]
    fn put_and_remove_keep_the_categories_in_step() {
        let mut f = HighlightFile::default();
        assert!(f.put("burn", rule("burning", "status")).is_none());
        assert_eq!(f.categories["status"].len(), 1);
        // Putting it again replaces rather than appending.
        f.put("burn", rule("burning", "status"));
        assert_eq!(f.categories["status"].len(), 1);
        let removed = f.remove("burn").unwrap();
        assert_eq!(removed.pattern, "burning");
        assert!(f.categories["status"].is_empty());
        assert!(f.remove("burn").is_none());
    }

    #[test]
    fn switching_a_category_flips_its_rules() {
        let mut f = HighlightFile::default();
        f.put("a", rule("a", "room"));
        f.put("b", rule("b", "room"));
        f.put("c", rule("c", "combat"));
        assert!(f.category_on("room"));
        let changed = f.switch("room", false);
        assert_eq!(changed, 2);
        assert!(!f.category_on("room"));
        assert!(f.highlight["a"].off);
        assert!(f.highlight["b"].off);
        // Other categories are untouched.
        assert!(!f.highlight["c"].off);
        f.switch("room", true);
        assert!(f.category_on("room"));
        assert!(!f.highlight["a"].off);
    }
}
