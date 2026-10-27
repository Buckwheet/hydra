//! Character status effects.
//!
//! Recovered from:
//! * `gs_base::status::Status::from_name` @0x100a8bac4
//! * `gs_base::status::Statuses::has_name` @0x100a8c8a4
//! * `gs_base::status::Statuses::resolved` @0x100a8c8e0
//!
//! ## Shape
//!
//! `Status` is a `#[repr(u8)]` fieldless enum with discriminants `0..=32`;
//! `Option<Status>` is `u8` with `33` (`0x21`) as the `None` byte — `has_name`
//! tests exactly `idx != 0x21`.  `Statuses` is a `u64` bitmask: bit *n* is set
//! when status *n* is active.
//!
//! `from_name` `trim()`s the input, ASCII-lowercases it, and dispatches on the
//! length (a jump over 10 entries at 0x1021f0bb6 covers lengths 3..=12, longer
//! names fall through a linear chain).  The name table below was rebuilt by
//! decoding every comparison constant in that function, grouped by the length
//! the compiler tested.
//!
//! `resolved` is three instructions and completely unambiguous:
//!
//! ```text
//! if self.0 & (1 << 26) == 0 { self.0 &= !(1 << 21) }
//! ```
//!
//! i.e. [`Status::Staggered`] (bit 26) keeps [`Status::Calm`] (bit 21).

/// A single status effect.  Discriminants are the recovered ones and double as
/// the bit index inside [`Statuses`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Status {
    Kneel = 0,
    Sit = 1,
    Prone = 2,
    Stun = 3,
    Sleep = 4,
    Unconscious = 5,
    Web = 6,
    Dead = 7,
    Frozen = 8,
    Immobile = 9,
    Poisoned = 10,
    Diseased = 11,
    Bleeding = 12,
    Rooted = 13,
    Bound = 14,
    Hidden = 15,
    Invisible = 16,
    Possessed = 17,
    Blind = 18,
    Cutthroat = 19,
    Silenced = 20,
    Calm = 21,
    Horrified = 22,
    Demoralized = 23,
    Feeble = 24,
    Crippled = 25,
    Staggered = 26,
    Dazed = 27,
    Confused = 28,
    Frenzied = 29,
    Overexerted = 30,
    Slowed = 31,
    Vulnerable = 32,
}

/// The `None` byte of `Option<Status>` (`0x21`).
pub const STATUS_NONE: u8 = 33;

/// Every accepted spelling, in the order the disassembly compares them, with the
/// discriminant it maps to.
///
/// INFERRED: a handful of the 5-character group's *heads* were only recoverable
/// through `add`/`orr` chains rather than a literal; those entries are the ones
/// covered by `("kneel", Kneel)`, `("prone", Prone)`, `("sleep", Sleep)`,
/// `("bleed", Bleeding)`, `("bound", Bound)`, `("blind", Blind)`,
/// `("dazed", Dazed)` — each proven by the trailing-byte compare but reconstructed
/// by hand.  Everything else is a direct 8- or 4-byte literal comparison.
pub static STATUS_NAMES: &[(&str, Status)] = &[
    // length 3
    ("sit", Status::Sit),
    ("web", Status::Web),
    // length 5
    ("kneel", Status::Kneel),
    ("prone", Status::Prone),
    ("sleep", Status::Sleep),
    ("bleed", Status::Bleeding),
    ("bound", Status::Bound),
    ("blind", Status::Blind),
    ("dazed", Status::Dazed),
    // length 4
    ("stun", Status::Stun),
    ("dead", Status::Dead),
    ("bind", Status::Bound),
    ("calm", Status::Calm),
    // length 6
    ("asleep", Status::Sleep),
    ("webbed", Status::Web),
    ("frozen", Status::Frozen),
    ("poison", Status::Poisoned),
    ("rooted", Status::Rooted),
    ("hidden", Status::Hidden),
    ("calmed", Status::Calm),
    ("feeble", Status::Feeble),
    ("frenzy", Status::Frenzied),
    ("slowed", Status::Slowed),
    // length 7 / mixed group
    ("sitting", Status::Sit),
    ("stunned", Status::Stun),
    ("thorned", Status::Poisoned),
    ("diseased", Status::Diseased),
    ("blinded", Status::Blind),
    ("horrified", Status::Horrified),
    ("major poison", Status::Poisoned),
    ("thorn poison", Status::Poisoned),
    // length 8
    ("kneeling", Status::Kneel),
    ("sleeping", Status::Sleep),
    ("immobile", Status::Immobile),
    ("poisoned", Status::Poisoned),
    ("mind rot", Status::Diseased),
    ("bleeding", Status::Bleeding),
    ("shrouded", Status::Hidden),
    ("silenced", Status::Silenced),
    ("weakened", Status::Feeble),
    ("crippled", Status::Crippled),
    ("confused", Status::Confused),
    ("frenzied", Status::Frenzied),
    // length 9
    ("invisible", Status::Invisible),
    ("possessed", Status::Possessed),
    ("cutthroat", Status::Cutthroat),
    ("terrified", Status::Horrified),
    ("staggered", Status::Staggered),
    // length 10
    ("lying down", Status::Prone),
    ("sheer fear", Status::Horrified),
    ("frightened", Status::Horrified),
    ("vulnerable", Status::Vulnerable),
    // length >= 11
    ("unconscious", Status::Unconscious),
    ("immobilized", Status::Immobile),
    ("major bleeding", Status::Bleeding),
    ("demoralized", Status::Demoralized),
    ("disoriented", Status::Confused),
    ("overexerted", Status::Overexerted),
];

impl Status {
    /// Bit index inside [`Statuses`].
    pub const fn index(self) -> u32 {
        self as u32
    }

    /// Mask with just this status' bit.
    pub const fn bit(self) -> u64 {
        1u64 << (self as u32)
    }

    /// Parse a status name (trimmed, ASCII-case-insensitive).
    ///
    /// `@0x100a8bac4` — `Status::from_name(name: &str) -> Option<Status>`.
    pub fn from_name(name: &str) -> Option<Status> {
        let trimmed = name.trim();
        for (candidate, status) in STATUS_NAMES {
            if candidate.len() == trimmed.len() && candidate.eq_ignore_ascii_case(trimmed) {
                return Some(*status);
            }
        }
        None
    }

    /// The canonical spelling for this status (first match in the table).
    pub fn name(self) -> &'static str {
        STATUS_NAMES
            .iter()
            .find(|(_, s)| *s == self)
            .map(|(n, _)| *n)
            .unwrap_or("")
    }

    /// All 33 statuses in discriminant order.
    pub fn all() -> impl Iterator<Item = Status> {
        (0..33u8).map(|i| Status::from_index(i).unwrap())
    }

    fn from_index(i: u8) -> Option<Status> {
        STATUS_NAMES.iter().find(|(_, s)| s.index() == i as u32).map(|(_, s)| *s)
    }
}

/// A set of active statuses: `u64` bitmask over [`Status`] discriminants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Statuses(pub u64);

impl Statuses {
    pub const fn new(bits: u64) -> Self {
        Statuses(bits)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, s: Status) -> bool {
        self.0 & s.bit() != 0
    }

    pub fn insert(&mut self, s: Status) {
        self.0 |= s.bit();
    }

    pub fn remove(&mut self, s: Status) {
        self.0 &= !s.bit();
    }

    /// `true` if the named status is present.
    ///
    /// `@0x100a8c8a4`:
    /// ```text
    /// idx = Status::from_name(name) as u8      // 33 == None
    /// (idx != 33) & ((self.0 >> idx) as u32 & 1)
    /// ```
    pub fn has_name(&self, name: &str) -> bool {
        match Status::from_name(name) {
            Some(s) => self.0 >> s.index() & 1 != 0,
            None => false,
        }
    }

    /// Drop [`Status::Calm`] unless [`Status::Staggered`] is also set.
    ///
    /// `@0x100a8c8e0` — `and x8, x0, #0xffffffffffdfffff` clears bit 21
    /// (`0x0020_0000`), `tst x0, #0x4000000` tests bit 26, and the `csel` keeps
    /// the original when bit 26 is set.
    pub fn resolved(self) -> Statuses {
        if self.0 & (1 << Status::Staggered.index()) == 0 {
            Statuses(self.0 & !(1 << Status::Calm.index()))
        } else {
            self
        }
    }

    /// Iterate the set statuses in discriminant order.
    pub fn iter(self) -> impl Iterator<Item = Status> {
        Status::all().filter(move |s| self.contains(*s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_and_none_sentinel() {
        assert_eq!(Status::Kneel as u8, 0);
        assert_eq!(Status::Calm as u8, 21);
        assert_eq!(Status::Staggered as u8, 26);
        assert_eq!(Status::Vulnerable as u8, 32);
        assert_eq!(STATUS_NONE, 33);
        assert_eq!(Status::Horrified.bit(), 1 << 22);
        assert_eq!(Status::Calm.bit(), 0x0020_0000);
        assert_eq!(Status::Staggered.bit(), 0x0400_0000);
    }

    #[test]
    fn from_name_folds_case_and_whitespace() {
        assert_eq!(Status::from_name("stunned"), Some(Status::Stun));
        assert_eq!(Status::from_name("  STUNNED\n"), Some(Status::Stun));
        assert_eq!(Status::from_name("Stun"), Some(Status::Stun));
        assert_eq!(Status::from_name("mind rot"), Some(Status::Diseased));
        assert_eq!(Status::from_name("major bleeding"), Some(Status::Bleeding));
        assert_eq!(Status::from_name("not a status"), None);
        assert_eq!(Status::from_name(""), None);
        // Prefixes are not accepted.
        assert_eq!(Status::from_name("stunne"), None);
    }

    #[test]
    fn every_discriminant_has_a_name() {
        let mut covered = [false; 33];
        for (_, s) in STATUS_NAMES {
            covered[s.index() as usize] = true;
        }
        assert!(covered.iter().all(|&c| c), "a discriminant has no name");
        assert_eq!(Status::all().count(), 33);
        // `from_name` round-trips every spelling.
        for (name, s) in STATUS_NAMES {
            assert_eq!(Status::from_name(name), Some(*s), "name={name}");
        }
    }

    #[test]
    fn has_name() {
        let mut st = Statuses::default();
        assert!(!st.has_name("stunned"));
        st.insert(Status::Stun);
        assert!(st.has_name("stunned"));
        assert!(st.has_name("STUN"));
        assert!(!st.has_name("dead"));
        assert!(!st.has_name("nonsense"));
        st.insert(Status::Staggered);
        assert_eq!(st.iter().count(), 2);
    }

    #[test]
    fn resolved_drops_calm_without_staggered() {
        let calm_and_stagger = Statuses::new(Status::Calm.bit() | Status::Staggered.bit());
        assert_eq!(calm_and_stagger.resolved(), calm_and_stagger);

        let calm_only = Statuses::new(Status::Calm.bit() | Status::Stun.bit());
        assert_eq!(
            calm_only.resolved(),
            Statuses::new(Status::Stun.bit()),
            "calm must be cleared when staggered is absent"
        );

        // Bit 26 alone is untouched; bit 21 alone disappears.
        assert_eq!(Statuses::new(Status::Staggered.bit()).resolved().0, 0x0400_0000);
        assert_eq!(Statuses::new(Status::Calm.bit()).resolved().0, 0);
        // Other bits survive either way.
        assert_eq!(
            Statuses::new(u64::MAX & !Status::Calm.bit()).resolved().0,
            u64::MAX & !Status::Calm.bit()
        );
    }
}
