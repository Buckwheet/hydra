//! Status-flag store.
//!
//! Recovered from `hydra_model::status` in the arm64 binary:
//!
//! * `StatusInfo` is a single `BTreeMap<String, bool>` (recovered from the
//!   `BTreeMap::insert` / `BTreeMap::remove` calls in `set` / `forget` and the
//!   inlined node walk in `get` / `is_known`: node payloads are `String` keys
//!   with a one-byte value and a `0x120`-offset child pointer array).
//! * every key is normalised first: a leading `Icon` is stripped
//!   (`cmp` against `0x49636f6e`, i.e. `"Icon"`) and the rest is
//!   ASCII-lowercased with the vectorised `make_ascii_lowercase` loop
//!   (`sub w11, w10, #0x41; cmp w11, #0x1a; orr w10, w11|0x20`).
//! * `StatusReading` is a newtype over a reference to the map
//!   (`ldr x19, [x0]` at every reader entry) whose accessors return
//!   `Option<bool>`; `None` is the `2` tag that rustc uses for
//!   `Option<bool>`.

use std::collections::BTreeMap;

/// The prefix the game puts on status-bar icons in `<indicator>` streams.
///
/// Evidence: `StatusInfo::get` compares the first four bytes against
/// `0x49636f6e` (`"Icon"` big-endian) and then re-runs the lookup on the
/// remainder.
pub const ICON_PREFIX: &str = "Icon";

/// A map of status-flag name to "is on".
#[derive(Default, Clone, Debug, PartialEq)]
pub struct StatusInfo {
    pub flags: BTreeMap<String, bool>,
}

impl StatusInfo {
    pub fn new() -> Self {
        StatusInfo::default()
    }

    /// Normalise a status name: strip a leading `Icon`, then ASCII-lowercase.
    ///
    /// `// UNKNOWN:` the binary does not lowercase non-ASCII bytes; the
    /// vectorised loop only folds `A`..`Z`.
    pub fn normalize(name: &str) -> String {
        let name = name.strip_prefix(ICON_PREFIX).unwrap_or(name);
        name.to_ascii_lowercase()
    }

    /// Look up a flag; a missing flag reads as `false`.
    ///
    /// Recovered body: normalise, walk the map, `Option::unwrap_or(false)` —
    /// the disassembly ends in `and w0, w21, #1` on the found value and
    /// returns `0` when the map root is null.
    pub fn get(&self, name: &str) -> bool {
        let key = StatusInfo::normalize(name);
        self.flags.get(&key).copied().unwrap_or(false)
    }

    /// Is this flag known at all (present in the map, even as `false`)?
    ///
    /// Recovered body: identical walk to [`StatusInfo::get`] but the result is
    /// the *presence* bit (`mov w21, #1` on a hit, `0` on a miss).
    pub fn is_known(&self, name: &str) -> bool {
        let key = StatusInfo::normalize(name);
        self.flags.contains_key(&key)
    }

    /// Set a flag; returns whether the stored value actually changed.
    ///
    /// Recovered body: `BTreeMap::insert` returns `Option<bool>`; the result is
    /// `insert(...).map_or(true, |old| old != value)` — the disassembly does
    /// `cmp w8, #2` (the `None` tag), `eor w8, w19, w8`, `csinc ... ne`, so a
    /// first-time insert reports `true`.
    pub fn set(&mut self, name: &str, value: bool) -> bool {
        let key = StatusInfo::normalize(name);
        match self.flags.insert(key, value) {
            Some(old) => old != value,
            None => true,
        }
    }

    /// Forget a flag (`BTreeMap::remove`); the removed value is discarded.
    pub fn forget(&mut self, name: &str) {
        let key = StatusInfo::normalize(name);
        self.flags.remove(&key);
    }

    /// `self.get("dead")` — a 16-byte thunk in the binary.
    pub fn dead(&self) -> bool {
        self.get("dead")
    }
}

/// A read-only view of [`StatusInfo`].
#[derive(Clone, Copy, Debug)]
pub struct StatusReading<'a>(pub &'a StatusInfo);

impl<'a> StatusReading<'a> {
    pub fn new(info: &'a StatusInfo) -> Self {
        StatusReading(info)
    }

    /// `Some(info.get(name))` when the flag is known, else `None`.
    ///
    /// Every accessor in the binary is this same 80-byte shape: an
    /// `is_known` test, then a `get` call, with `2` (the `None` tag of
    /// `Option<bool>`) on the unknown path.
    fn read(&self, name: &str) -> Option<bool> {
        if self.0.is_known(name) {
            Some(self.0.get(name))
        } else {
            None
        }
    }

    pub fn dead(&self) -> Option<bool> {
        self.read("dead")
    }
    pub fn prone(&self) -> Option<bool> {
        self.read("prone")
    }
    pub fn hidden(&self) -> Option<bool> {
        self.read("hidden")
    }
    pub fn sitting(&self) -> Option<bool> {
        self.read("sitting")
    }
    pub fn stunned(&self) -> Option<bool> {
        self.read("stunned")
    }
    pub fn kneeling(&self) -> Option<bool> {
        self.read("kneeling")
    }
    pub fn standing(&self) -> Option<bool> {
        self.read("standing")
    }
    pub fn invisible(&self) -> Option<bool> {
        self.read("invisible")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_icon_prefix() {
        assert_eq!(StatusInfo::normalize("IconDEAD"), "dead");
        assert_eq!(StatusInfo::normalize("Iconstunned"), "stunned");
        // only the exact four bytes are stripped
        assert_eq!(StatusInfo::normalize("ICONDEAD"), "icondead");
        assert_eq!(StatusInfo::normalize("icons"), "icons");
        assert_eq!(StatusInfo::normalize(""), "");
    }

    #[test]
    fn get_and_is_known_differ_for_missing_flags() {
        let mut info = StatusInfo::new();
        assert!(!info.get("dead"));
        assert!(!info.is_known("dead"));
        assert!(info.set("IconDEAD", true));
        assert!(info.get("dead"));
        assert!(info.is_known("Icondead"));
    }

    #[test]
    fn set_reports_change_only_when_value_differs() {
        let mut info = StatusInfo::new();
        assert!(info.set("hidden", true));
        assert!(!info.set("HIDDEN", true));
        assert!(info.set("hidden", false));
        assert!(!info.get("hidden"));
        assert!(info.is_known("hidden"));
    }

    #[test]
    fn forget_removes_key() {
        let mut info = StatusInfo::new();
        info.set("prone", true);
        assert!(info.is_known("prone"));
        info.forget("Iconprone");
        assert!(!info.is_known("prone"));
        assert!(!info.get("prone"));
    }

    #[test]
    fn reading_is_three_state() {
        let mut info = StatusInfo::new();
        let reading = StatusReading::new(&info);
        assert_eq!(reading.dead(), None);
        info.set("dead", true);
        assert_eq!(StatusReading::new(&info).dead(), Some(true));
        info.set("dead", false);
        assert_eq!(StatusReading::new(&info).dead(), Some(false));
    }

    #[test]
    fn dead_accessor_reads_dead_flag() {
        let mut info = StatusInfo::new();
        assert!(!info.dead());
        info.set("dead", true);
        assert!(info.dead());
    }

    #[test]
    fn every_reading_accessor_uses_its_own_name() {
        let mut info = StatusInfo::new();
        for name in [
            "dead", "prone", "hidden", "sitting", "stunned", "kneeling", "standing", "invisible",
        ] {
            info.set(name, true);
        }
        let r = StatusReading::new(&info);
        assert_eq!(r.dead(), Some(true));
        assert_eq!(r.prone(), Some(true));
        assert_eq!(r.hidden(), Some(true));
        assert_eq!(r.sitting(), Some(true));
        assert_eq!(r.stunned(), Some(true));
        assert_eq!(r.kneeling(), Some(true));
        assert_eq!(r.standing(), Some(true));
        assert_eq!(r.invisible(), Some(true));
    }
}
