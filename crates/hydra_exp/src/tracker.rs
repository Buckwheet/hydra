//! `hydra_exp::tracker` — the per-character experience tracker.
//!
//! The tracker folds the numbers the game prints into snapshot rows and
//! remembers the last values it saw so it only records a row when something
//! actually moved. Its shape was partly recovered from the five entry points:
//!
//! * [`Tracker::heard`] (`0x1012ddf00`, 100 B) is **fully** recovered: it calls
//!   `hydra_model::state::character::experience::Experience::apply_bar` and
//!   returns `changed && reading.text == "mindState"`.
//! * [`Tracker::knows`] (`0x1012ddf64`) is recovered as far as the loop and the
//!   `HashMap` go: it inserts one entry per snapshot into the map at `self +
//!   0x1e8`, keyed by the byte at snapshot `+0x88` and the `Option<String>` at
//!   `+0x60`, with the snapshot's leading `0x30` bytes as the value.
//! * [`Tracker::new`] (`0x1012dcf60`), [`Tracker::look`] (`0x1012dd0dc`) and
//!   [`Tracker::seed`] (`0x1012dd768`) are **partly** recovered: the field
//!   offsets, the comparisons and the literals are known, the exact set of
//!   snapshot labels is not (`// UNKNOWN:` at each site).
//!
//! `Tracker` is 0x228+ bytes in the binary. Only the fields below are backed by
//! evidence; `Look`'s offsets are noted on each field.

use std::collections::HashMap;

use hydra_model::state::character::experience::Experience;

use crate::row::{Snapshot, Who};

/// A bar reading handed to [`Tracker::heard`].
///
/// The first field is a `String` in the binary: `heard` reads the length at
/// `+0x10` and the bytes at `+0x08`, i.e. the classic `String` layout
/// (capacity `+0x00`, pointer `+0x08`, length `+0x10`), and compares the nine
/// bytes `mindState`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bar {
    /// The reading's name (`"mindState"`, …).
    pub text: String,
    /// The current value.
    pub current: u64,
    /// The bar's maximum.
    pub maximum: u64,
}

/// What [`Tracker::look`] compares against: the numbers the game state holds.
///
/// Offsets are the ones `look` reads from its first argument (`0x1012dd0dc`):
/// `exp +0xb90`, `axp +0x214`, `until_next +0x1f8`, the two `Option<u32>` mind
/// fields at `+0x5e0`/`+0x5e4` and `+0x608`/`+0x60c`/`+0x610`, and the mind
/// wording byte at `+0x61a`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Watch {
    /// Absorbed experience (`+0xb90`, `u32`).
    pub exp: u32,
    /// Absorbed ascension experience (`+0x214`, `u32`).
    pub axp: u32,
    /// Experience still needed for the next level (`+0x1f8`).
    pub until_next: i64,
    /// Mind state percentage (`+0x5e0` tag / `+0x5e4` value).
    pub mind_percent: Option<u32>,
    /// Pool percentage (`+0x608` tag / `+0x60c`+`+0x610` values).
    pub pool_percent: Option<u32>,
    /// The mind-state wording index (`+0x61a`), used as a snapshot label.
    pub mind_state: u8,
}

/// Key of the map [`Tracker::knows`] fills.
///
/// The binary keys on the snapshot's `+0x88` byte and the `Option<String>` at
/// `+0x60`; this build keeps the same pair.
pub type SnapshotKey = (u8, Option<String>);

/// Value of the map [`Tracker::knows`] fills: the snapshot's leading `0x30`
/// bytes, i.e. the two tagged figures it carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    /// First tagged figure (the snapshot's `+0x10`).
    pub weekly: Option<u64>,
    /// Second tagged figure (the snapshot's `+0x20`).
    pub total: Option<u64>,
    /// The reading's timestamp.
    pub at: i64,
}

/// The experience tracker.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    /// `+0x00` — first field, which is why `heard` can hand `self` straight to
    /// `Experience::apply_bar`.
    pub experience: Experience,
    /// `+0x1b8`/`+0x1d0` — the two `String`s `new` copies out of its input
    /// (0x30 bytes) and `look` clones into the snapshot it builds.
    pub who: Who,
    /// `+0x1e8` — the `HashMap` `knows` inserts into.
    pub known: HashMap<SnapshotKey, Known>,
    /// `+0x220` — last absorbed experience `look` saw.
    pub seen_exp: u32,
    /// `+0x224` — last absorbed ascension `look` saw.
    pub seen_axp: u32,
    /// `+0x218` — last `until_next` `look` saw.
    pub seen_until_next: i64,
    /// Snapshots produced by [`Tracker::seed`] that `look` has not returned yet.
    pub pending: Vec<Snapshot>,
}

impl Tracker {
    /// `tracker::Tracker::new(who) -> Tracker` — `@0x1012dcf60` (**partly**).
    ///
    /// `new` builds a 0x180-byte zeroed template whose `Option<String>` slots
    /// hold the `isize::MIN` niche, copies it into the result, copies 0x30 bytes
    /// of its argument into `+0x1b8` (two `String`s — the character), then
    /// installs a `RandomState` (it calls `std::sys::random::hashmap_random_keys`
    /// and bumps a process-wide counter) for the `HashMap` at `+0x1e8` and
    /// memcpy's 0x20 bytes of `__const` (two `&'static str`) into `+0x1e8`.
    ///
    /// `// UNKNOWN:` what the two static strings at `0x10442ddf8` are (their
    /// bytes are `a8 9a 06 02 01 00 00 00 …`, i.e. pointers into `__const`, not
    /// printable text); this build starts with an empty [`Tracker::who`].
    pub fn new(who: Who) -> Tracker {
        Tracker {
            experience: Experience::default(),
            who,
            known: HashMap::new(),
            seen_exp: 0,
            seen_axp: 0,
            seen_until_next: 0,
            pending: Vec::new(),
        }
    }

    /// `tracker::Tracker::heard(&mut self, reading: &Bar) -> bool` —
    /// `@0x1012ddf00` (**full** for the control flow).
    ///
    /// Folds the reading into [`Tracker::experience`] through
    /// `Experience::apply_bar`; when that reports a change *and* the reading's
    /// name is exactly `"mindState"` (nine bytes, compared with two `movk`-built
    /// immediates `0x696d`/`0x646e`/`0x7453`/`0x7461`), returns `true`. A reading
    /// whose name has any other length never matches (the length check is
    /// `cmp x8, #0x9`).
    ///
    /// `// UNKNOWN:` the original passes the whole `&Bar` to `apply_bar`; the
    /// recovered `hydra_model` signature takes `(current, maximum)`, so this
    /// build forwards those two numbers.
    pub fn heard(&mut self, reading: &Bar) -> bool {
        let changed = self
            .experience
            .apply_bar(reading.current, reading.maximum);
        if !changed {
            return false;
        }
        reading.text == "mindState"
    }

    /// `tracker::Tracker::knows(&mut self, snaps: &[Snapshot])` —
    /// `@0x1012ddf64` (**partly**).
    ///
    /// Early-returns on an empty slice, then for every snapshot (stride `0x90`)
    /// inserts `(byte at +0x88, Option<String> at +0x60) -> first 0x30 bytes`
    /// into the map at `self + 0x1e8` (`hashbrown`'s `HashMap::insert`, which
    /// returns the previous value — the binary discards it).
    ///
    /// `// UNKNOWN:` the byte at `+0x88` is the snapshot kind; its enum is not in
    /// the binary, so the raw byte is kept.
    pub fn knows(&mut self, snaps: &[Snapshot]) {
        for snap in snaps {
            let kind = snapshot_kind(snap);
            self.known.insert(
                (kind, snap.text.clone()),
                Known {
                    weekly: snap.weekly,
                    total: snap.total,
                    at: snap.at,
                },
            );
        }
    }

    /// `tracker::Tracker::seed(&mut self, out, who, kind, label, values)` —
    /// `@0x1012dd768` (**partly**).
    ///
    /// `look` calls this three times (`w3` = `0`, `1`, `2`) with a `&mut Vec`,
    /// the character, a kind byte, a label (`Option<String>`, cloned) and two
    /// tagged numbers. It appends a snapshot row to the vector.
    ///
    /// `// UNKNOWN:` the label strings are picked from tables this build could
    /// not identify (the calls pass `Option<String>` values straight out of the
    /// game state), so the caller supplies the label.
    pub fn seed(
        &mut self,
        out: &mut Vec<Snapshot>,
        kind: u8,
        label: Option<&str>,
        weekly: Option<u64>,
        total: Option<u64>,
        at: i64,
    ) {
        let _ = kind;
        out.push(Snapshot {
            weekly,
            total,
            text: label.map(|text| text.to_string()),
            at,
        });
    }

    /// `tracker::Tracker::look(&mut self, watch: &Watch, at) -> Vec<Snapshot>` —
    /// `@0x1012dd0dc` (**partly**).
    ///
    /// Compares the watch's absorbed experience with `self + 0x220` and the
    /// ascension with `self + 0x224` (`u32`), remembering the newest values in
    /// the two pending `(flag, at)` slots at `+0x198`/`+0x1a8`. When the absorbed
    /// figures move it hands the last-seen `until_next` (`+0x218`) and the
    /// character strings to [`Tracker::seed`] and returns the snapshots it built
    /// (`report::snapshots`' first helper is then applied to the vector,
    /// `0x1012c0390`).
    ///
    /// `// UNKNOWN:` which of the three `seed` calls fire for a given combination
    /// of mind/pool state; this build emits one snapshot per change of either
    /// absorbed figure, which is the condition the two `u32` comparisons
    /// establish.
    pub fn look(&mut self, watch: &Watch, at: i64) -> Vec<Snapshot> {
        let moved = watch.exp > self.seen_exp || watch.axp > self.seen_axp;
        self.seen_exp = watch.exp;
        self.seen_axp = watch.axp;
        self.seen_until_next = watch.until_next;
        let mut out: Vec<Snapshot> = Vec::new();
        if moved {
            let name = self.who.name.clone();
            self.seed(
                &mut out,
                0,
                Some(&name),
                Some(u64::from(watch.exp)),
                Some(u64::from(watch.axp)),
                at,
            );
            if let Some(mind) = watch.mind_percent {
                self.seed(
                    &mut out,
                    1,
                    Some(&name),
                    Some(u64::from(mind)),
                    None,
                    at,
                );
            }
            if let Some(pool) = watch.pool_percent {
                self.seed(
                    &mut out,
                    2,
                    Some(&name),
                    Some(u64::from(pool)),
                    None,
                    at,
                );
            }
        }
        self.pending.extend(out.iter().cloned());
        out
    }
}

/// The snapshot kind byte the binary reads at `+0x88`.
///
/// `// UNKNOWN:` the row's kind enum is not in the binary; this build derives a
/// stable byte from the snapshot text so `knows` still keys deterministically.
fn snapshot_kind(snap: &Snapshot) -> u8 {
    match snap.text.as_deref() {
        Some("favor") => 0,
        Some("resource") => 1,
        Some("suffused") => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heard_only_reports_changes_of_the_mind_state() {
        let mut tracker = Tracker::new(Who::new("Sindar"));
        let mut bar = Bar {
            text: "mindState".to_string(),
            current: 10,
            maximum: 100,
        };
        assert!(tracker.heard(&bar));
        // Same numbers again: nothing changed.
        assert!(!tracker.heard(&bar));
        bar.current = 20;
        assert!(tracker.heard(&bar));
        // A different name reports the change as "not interesting".
        bar.text = "health".to_string();
        bar.current = 30;
        assert!(!tracker.heard(&bar));
        // Nine characters that are not "mindState".
        bar.text = "mindState".to_string();
        bar.text = "mindstate".to_string();
        bar.current = 40;
        assert!(!tracker.heard(&bar));
    }

    #[test]
    fn knows_indexes_snapshots_by_kind_and_text() {
        let mut tracker = Tracker::new(Who::new("Sindar"));
        tracker.knows(&[Snapshot {
            weekly: Some(12_000),
            total: Some(240_000),
            text: Some("resource".to_string()),
            at: 1_784_567_152,
        }]);
        let known = tracker
            .known
            .get(&(1, Some("resource".to_string())))
            .expect("entry");
        assert_eq!(known.weekly, Some(12_000));
        assert_eq!(known.total, Some(240_000));
        assert_eq!(known.at, 1_784_567_152);
        // An empty slice leaves the map alone.
        tracker.knows(&[]);
        assert_eq!(tracker.known.len(), 1);
    }

    #[test]
    fn look_emits_a_snapshot_only_when_the_figures_move() {
        let mut tracker = Tracker::new(Who::new("Sindar"));
        let quiet = Watch::default();
        assert!(tracker.look(&quiet, 1_784_567_152).is_empty());
        let moved = Watch {
            exp: 3_750,
            axp: 44,
            until_next: 1_100,
            mind_percent: Some(80),
            pool_percent: Some(60),
            mind_state: 2,
        };
        let snaps = tracker.look(&moved, 1_784_567_152);
        assert_eq!(snaps.len(), 3);
        assert_eq!(snaps[0].weekly, Some(3_750));
        assert_eq!(snaps[0].total, Some(44));
        assert_eq!(snaps[1].weekly, Some(80));
        // The same watch again is quiet.
        assert!(tracker.look(&moved, 1_784_567_212).is_empty());
    }
}
