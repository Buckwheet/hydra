//! Per-target cooldowns.
//!
//! `Cooldowns::record_group` (780 B) and `record_target` (600 B) register a
//! cooldown against a group or a single target.

use std::collections::BTreeMap;

/// Cooldown state per key (a group name or a target name).
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Cooldowns {
    pub until: BTreeMap<String, u32>,
}

impl Cooldowns {
    /// @0x0000000101be6f68 (780 B)
    pub fn record_group(&mut self, group: &str, ready_at: u32) -> bool {
        self.record(group, ready_at)
    }

    /// @0x0000000101be? (600 B)
    pub fn record_target(&mut self, target: &str, ready_at: u32) -> bool {
        self.record(target, ready_at)
    }

    fn record(&mut self, key: &str, ready_at: u32) -> bool {
        if self.until.get(key) == Some(&ready_at) {
            return false;
        }
        self.until.insert(key.to_string(), ready_at);
        true
    }

    pub fn ready_at(&self, key: &str) -> Option<u32> {
        self.until.get(key).copied()
    }

    pub fn is_ready(&self, key: &str, now: u32) -> bool {
        self.until.get(key).map(|at| now >= *at).unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_and_target_cooldowns_share_the_store() {
        let mut cooldowns = Cooldowns::default();
        assert!(cooldowns.record_group("fury", 10));
        assert!(cooldowns.record_target("a kobold", 5));
        assert!(!cooldowns.record_group("fury", 10));
        assert_eq!(cooldowns.ready_at("fury"), Some(10));
        assert_eq!(cooldowns.ready_at("a kobold"), Some(5));
    }

    #[test]
    fn readiness_compares_against_now() {
        let mut cooldowns = Cooldowns::default();
        cooldowns.record_group("fury", 10);
        assert!(!cooldowns.is_ready("fury", 9));
        assert!(cooldowns.is_ready("fury", 10));
        assert!(cooldowns.is_ready("unknown", 0));
    }
}
