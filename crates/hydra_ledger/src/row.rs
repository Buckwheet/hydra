//! `hydra_ledger::row` — the ledger's row types.
//!
//! Every struct below is laid out at the byte offsets the binary actually
//! reads/writes; the layouts were recovered from the `reader::*` row mappers
//! (`reader::pulse` `0x10168aca8`, `reader::reward` `0x10168d0ec`,
//! `reader::snapshot` `0x10168d620`, `reader::store_visit` `0x10168a8d4`) and
//! cross-checked against `core::ptr::drop_in_place<…>` for the same types
//! (which shows exactly which words are `String`/`Option<String>` capacities).
//!
//! Two ABI facts that the offsets depend on (both established from the drop
//! glue and the mappers, and both *different from the textbook layout*):
//!
//! * a `String`/`Vec` in this build is `{ cap @0, ptr @8, len @0x10 }`, and
//!   `Option<String>` uses `isize::MIN` in the capacity word for `None`
//!   (e.g. `cmp x1, #0x8000000000000000` in `report::snapshots`' closures);
//! * `Option<u32>` is `{ tag: u32, value: u32 }` (8 bytes) and
//!   `Option<i64>` is `{ tag: u64, value: i64 }` (16 bytes), i.e. no niche.
//!
//! The declaration order below is the *offset* order, so the structs are the
//! recovered rows; the original's declaration order is not recoverable.

/// Where a pulse's experience came from — `Pulse + 0xc8`.
///
/// Read by `report::last`/`report::daily` from the name table at
/// `0x10442dae0` (pointers) / `0x10240cee8` (lengths): `normal`, `bounty`,
/// `brooch`, `instant`, `manual`. `row::Source::parse` (`0x10168a808`) turns
/// the `source` TEXT column back into this value: a six-byte compare against
/// `bounty`/`brooch`/`manual` and a seven-byte compare against `instant`;
/// anything else (including `normal`) is [`Source::Normal`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Source {
    /// A normal experience pulse.
    #[default]
    Normal = 0,
    /// A bounty reward.
    Bounty = 1,
    /// A brooch rub.
    Brooch = 2,
    /// A lump ("instant") gain.
    Instant = 3,
    /// A gain typed in by hand (`exp addgain`).
    Manual = 4,
}

impl Source {
    /// The five names, in discriminant order (table `0x10442dae0`).
    pub const NAMES: [&'static str; 5] = ["normal", "bounty", "brooch", "instant", "manual"];

    /// The name for this source, as stored in `pulses.source`.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    /// `row::Source::parse(&str) -> Source` — `@0x10168a808` (**full**).
    ///
    /// The body is length-first: `len == 7` compares four bytes at `+0` and
    /// four at `+3` with `instant`; `len == 6` compares four bytes at `+0` and
    /// two at `+4` with `bounty`, then `brooch`, then `manual`. Every miss
    /// (including `len == 5`, `normal`) returns `Normal`, which is what the
    /// `csel w0, wzr, w9, ne` tails encode.
    pub fn parse(text: &str) -> Source {
        match text {
            "bounty" => Source::Bounty,
            "brooch" => Source::Brooch,
            "instant" => Source::Instant,
            "manual" => Source::Manual,
            // `normal`, a short word, or anything unrecognised.
            _ => Source::Normal,
        }
    }

    /// The stored byte, with the same fallback as [`Source::parse`].
    pub fn from_u8(byte: u8) -> Source {
        match byte {
            1 => Source::Bounty,
            2 => Source::Brooch,
            3 => Source::Instant,
            4 => Source::Manual,
            _ => Source::Normal,
        }
    }

    /// The stored byte.
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Which kind a snapshot row is.
///
/// `Reader::snapshots` (`0x10168cdac`) indexes three parallel tables with the
/// `kind` argument — pointers at `0x1044426c8`, lengths at `0x103983690` —
/// and binds the text as `?3`, so the stored column is TEXT:
///
/// | value | text | pointer |
/// |---|---|---|
/// | 0 | `favor` | `0x103982e72` (5) |
/// | 1 | `resource` | `0x10206b390` (8) |
/// | 2 | `suffused` | `0x10213c758` (8) |
///
/// The three-entry table is indexed without a bounds check, which is only sound
/// for a three-variant fieldless enum — hence this type rather than a `u8`.
/// `reader::snapshot` parses the column back with an inlined byte-compare chain
/// (`favo`/`reso`/`suff` + the tail words) at `0x10168d758`/`0x10168d868`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SnapshotKind {
    /// Voln favour.
    #[default]
    Favor = 0,
    /// The profession's resource points.
    Resource = 1,
    /// Suffused experience.
    Suffused = 2,
}

impl SnapshotKind {
    /// The three names, in discriminant order (`0x1044426c8`).
    pub const NAMES: [&'static str; 3] = ["favor", "resource", "suffused"];

    /// The text stored in `snapshots.kind`.
    pub fn name(self) -> &'static str {
        Self::NAMES[self as usize]
    }

    /// The stored byte.
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// `snapshots.kind` text → kind. An unknown word falls back to
    /// [`SnapshotKind::Favor`]: the binary's inlined compare chain has no other
    /// arm, so a stray word can only leave the default in place
    /// (`// UNKNOWN:` which arm that is, but the effect is the default).
    pub fn parse(text: &str) -> SnapshotKind {
        match text {
            "resource" => SnapshotKind::Resource,
            "suffused" => SnapshotKind::Suffused,
            _ => SnapshotKind::Favor,
        }
    }
}

/// One experience pulse — `hydra_ledger::row::Pulse`, 208 bytes (`stride 0xd0`,
/// which is also the size of `writer::Writer::try_record`'s argument).
///
/// Column order of the recovered `SELECT` is
/// `at, game_time, instance, character, exp, axp, exp_gained, axp_gained, fxp,
/// fxp_max, until_next, lumnis, rpa, fashlonae, mind_percent, mind_text,
/// source, instant`; the offsets below come from the stores at the tail of
/// `reader::pulse` (`0x10168b000`…`0x10168b0ac`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pulse {
    /// `+0x00` — the instance the row belongs to.
    pub instance: String,
    /// `+0x18` — the character the row belongs to.
    pub character: String,
    /// `+0x30` — the `rpa` bonus as stored (TEXT, so `Option<String>`).
    pub rpa: Option<String>,
    /// `+0x48` — the mind-state text.
    pub mind_text: Option<String>,
    /// `+0x60` — the game clock.
    pub game_time: Option<u32>,
    /// `+0x68` — when the pulse was seen (unix seconds).
    pub at: i64,
    /// `+0x70` — field experience.
    pub fxp: Option<u32>,
    /// `+0x78` — field experience maximum.
    pub fxp_max: Option<u32>,
    /// `+0x80` — experience still needed for the next level.
    pub until_next: Option<u32>,
    /// `+0x88` — the `lumnis` multiplier.
    pub lumnis: Option<u32>,
    /// `+0x90` — the `fashlonae` bonus.
    pub fashlonae: Option<u32>,
    /// `+0x98` — how full the mind pool was, in percent.
    pub mind_percent: Option<u32>,
    /// `+0xa0` — absorbed experience (`max(v, 0)`, see [`Pulse::absorbed`]).
    pub exp: i64,
    /// `+0xa8` — absorbed ascension experience.
    pub axp: i64,
    /// `+0xb0` — experience gained by this pulse.
    pub exp_gained: i64,
    /// `+0xb8` — ascension experience gained by this pulse.
    pub axp_gained: i64,
    /// `+0xc0` — the single amount a brooch/instant row carries. This is the
    /// column `pulses.instant`; `hydra_exp`'s copy of the row calls it `bonus`
    /// because that is how the reports use it.
    pub instant: i64,
    /// `+0xc8` — where the gain came from.
    pub source: Source,
}

impl Pulse {
    /// `reader::pulse` clamps the two absorbed figures with `bic x10, x23,
    /// x23, asr #63` — `v & !(v >> 63)`, i.e. `max(v, 0)`.
    pub fn absorbed(exp: i64, axp: i64) -> (i64, i64) {
        (exp.max(0), axp.max(0))
    }
}

/// One snapshot reading — `hydra_ledger::row::Snapshot`, 144 bytes (`0x90`).
///
/// Offsets from `reader::snapshot` (`0x10168d620`) and the drop glue of the
/// `Row` variant that wraps it (three string capacities at `0x38`, `0x50`,
/// `0x68`, i.e. `+8` into a payload starting at `0x30`, `0x48`, `0x60`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `+0x00` — the reading itself (Voln favour, suffused experience).
    pub value: Option<i64>,
    /// `+0x10` — the weekly figure.
    pub weekly: Option<i64>,
    /// `+0x20` — the lifetime figure.
    pub total: Option<i64>,
    /// `+0x30` — instance.
    pub instance: String,
    /// `+0x48` — character.
    pub character: String,
    /// `+0x60` — the reading's own name (`snapshots.name`, nullable).
    /// `hydra_exp`'s copy of the row calls it `label`.
    pub name: Option<String>,
    /// `+0x78` — the game clock.
    pub game_time: Option<u32>,
    /// `+0x80` — unix seconds.
    pub at: i64,
    /// `+0x88` — which kind this row is (one byte, then padding).
    pub kind: SnapshotKind,
}

/// One `login_rewards` row — `hydra_ledger::row::Reward`, 176 bytes (`0xb0`).
///
/// Six string capacities at `0x20`, `0x38`, `0x50`, `0x68`, `0x80`, `0x98`
/// (`drop_in_place<Reward>`), so `at`/`game_time`/`streak`/`bridges` fill
/// `0x00`…`0x20`. The field set and types come from the column list
/// `at, game_time, instance, character, day, streak, bridges, reward, thanks,
/// line` and the per-column `Row::get` monomorphizations; which of the six
/// strings sits at which of the six offsets is **partly recovered**
/// (`NOTES.md` §5.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reward {
    /// `+0x00` — unix seconds.
    pub at: i64,
    /// `+0x08` — the game clock.
    pub game_time: Option<u32>,
    /// `+0x10` — the streak counter.
    pub streak: Option<i64>,
    /// `+0x20` — the bridges counter.
    pub bridges: Option<i64>,
    /// `+0x30` — instance.
    pub instance: String,
    /// `+0x48` — character.
    pub character: String,
    /// `+0x60` — the day (`YYYY-MM-DD`), unique per instance+character.
    pub day: String,
    /// `+0x78` — the reward text.
    pub reward: String,
    /// `+0x90` — the thanks text.
    pub thanks: String,
    /// `+0xa8` — the raw log line.
    pub line: String,
}

/// One `store_visits` row — `hydra_ledger::row::StoreVisit`, 128 bytes
/// (`0x80`). Four string capacities at `0x10`, `0x28`, `0x40`, `0x58`
/// (`drop_in_place<StoreVisit>`); `next_at` (16 bytes) comes first, then
/// `at`/`claimed` last (`reader::store_visit`'s tail:
/// `stp x10, x28, [x23, #0x68]` and `strb w22, [x23, #0x78]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoreVisit {
    /// `+0x00` — when the next claim is due (nullable).
    pub next_at: Option<i64>,
    /// `+0x10` — the account name.
    pub account: String,
    /// `+0x28` — the game name.
    pub game: String,
    /// `+0x40` — the message the store printed.
    pub message: String,
    /// `+0x58` — the balance text.
    pub balance: String,
    /// `+0x70` — unix seconds.
    pub at: i64,
    /// `+0x78` — whether the visit's reward was claimed.
    pub claimed: bool,
}

/// Who a row belongs to — `hydra_ledger::row::Who`, 48 bytes (`0x30`).
///
/// Every reader takes `&Who` and binds `?1 = &who.instance`, `?2 =
/// &who.character` (`add x9, x20, #0x18` in `Reader::last_pulses`: the second
/// `String` starts at `+0x18`).
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Who {
    /// `+0x00` — instance.
    pub instance: String,
    /// `+0x18` — character.
    pub character: String,
}

impl Who {
    /// `row::Who::new` — `@0x1006ce96c` / `@0x10129e93c` (**full**: two
    /// `String` clones into a 48-byte value).
    pub fn new(instance: impl Into<String>, character: impl Into<String>) -> Who {
        Who {
            instance: instance.into(),
            character: character.into(),
        }
    }

    /// The character alone, for the reports that only need the name.
    pub fn name(&self) -> &str {
        &self.character
    }
}

/// One row handed to the writer.
///
/// The binary's `Row` is a 208-byte (`0xd0`) enum — its size is exactly the
/// `ldp`/`stp` block at the head of `Writer::try_record` (`0x1016939a4`) — with
/// the four variants of the writer's four `INSERT`s: `pulses`, `snapshots`,
/// `login_rewards`, `store_visits`. The discriminant lives in the first payload
/// word using `usize` sentinels (`0` = the pulse, then `isize::MIN`,
/// `MIN+1`, `MIN+2`), which is why `drop_in_place<Row>` (`0x1012bd28c`) tests
/// that word against `0x8000_0000_0000_000n` before dispatching. The encoding
/// is a codegen detail and is not reproduced here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// A `pulses` row.
    Pulse(Pulse),
    /// A `snapshots` row.
    Snapshot(Snapshot),
    /// A `login_rewards` row.
    Reward(Reward),
    /// A `store_visits` row.
    StoreVisit(StoreVisit),
}

/// A wall-clock reading — `hydra_ledger::row::Stamp`, 16 bytes.
///
/// `row::Stamp::now` (`0x1016995a4`, **full**) is
/// `SystemTime::now().duration_since(UNIX_EPOCH)`, saturating to `i64::MAX`
/// and 0 on the error arm (`cmp x9, #0x7fff…; csel x9, x9, …; cmp w8, #0;
/// csel x8, xzr, x9, ne`), stored at `+0x08`; the two `u32` arguments are
/// stored at `+0x00`/`+0x04` (`stp w20, w19, [x21]`).
///
/// Which two numbers those are is **not** recoverable from the signature: one
/// caller passes the pair of adjacent `u32`s a game-state struct keeps next to
/// each other (`0x1012d5c54`: `ldr w0, [x19, #0x2728]; ldr w1, [x19, #0x272c]`)
/// and another passes a literal `0` (`0x100072e4c`). The only consumer in the
/// binary reads `+0x08` alone (`hydra_rewards::watcher::Heard::at`:
/// `ldr x0, [x21, #0x8]` → `eastern::date`), so the first two fields are
/// carried as `hour`/`minute` — `// UNKNOWN:` the names, not the offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stamp {
    /// `+0x00` — first of the pair `Stamp::now` is handed.
    pub hour: u32,
    /// `+0x04` — second of the pair.
    pub minute: u32,
    /// `+0x08` — unix seconds.
    pub at: i64,
}

impl Stamp {
    /// `row::Stamp::now(hour, minute) -> Stamp` — `@0x1016995a4` (**full**).
    ///
    /// `SystemTime::now().duration_since(UNIX_EPOCH)`; the `Duration`'s seconds
    /// are clamped to `i64::MAX` and a clock before the epoch yields `0` (the
    /// second `csel`). The two arguments land in the first two words untouched.
    pub fn now(hour: u32, minute: u32) -> Stamp {
        use std::time::{SystemTime, UNIX_EPOCH};

        let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(elapsed) => i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
            Err(_) => 0,
        };
        Stamp {
            hour,
            minute,
            at: seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names_and_parse_match_the_table() {
        assert_eq!(
            Source::NAMES,
            ["normal", "bounty", "brooch", "instant", "manual"]
        );
        assert_eq!(Source::parse("normal"), Source::Normal);
        assert_eq!(Source::parse("bounty"), Source::Bounty);
        assert_eq!(Source::parse("brooch"), Source::Brooch);
        assert_eq!(Source::parse("instant"), Source::Instant);
        assert_eq!(Source::parse("manual"), Source::Manual);
        // Anything else is `normal`: the binary has no separate miss arm.
        assert_eq!(Source::parse(""), Source::Normal);
        assert_eq!(Source::parse("bountyy"), Source::Normal);
        assert_eq!(Source::parse("boun"), Source::Normal);
        assert_eq!(Source::Bounty.name(), "bounty");
        assert_eq!(Source::from_u8(3), Source::Instant);
        assert_eq!(Source::from_u8(9), Source::Normal);
        assert_eq!(Source::Manual.to_u8(), 4);
    }

    #[test]
    fn snapshot_kinds_are_the_three_text_values() {
        assert_eq!(SnapshotKind::NAMES, ["favor", "resource", "suffused"]);
        assert_eq!(SnapshotKind::Favor.name(), "favor");
        assert_eq!(SnapshotKind::parse("resource"), SnapshotKind::Resource);
        assert_eq!(SnapshotKind::parse("suffused"), SnapshotKind::Suffused);
        assert_eq!(SnapshotKind::parse("favor"), SnapshotKind::Favor);
        assert_eq!(SnapshotKind::Suffused.to_u8(), 2);
    }

    #[test]
    fn absorbed_figures_are_clamped_at_zero() {
        assert_eq!(Pulse::absorbed(1_234_567, -8), (1_234_567, 0));
        assert_eq!(Pulse::absorbed(0, 0), (0, 0));
    }

    #[test]
    fn who_keeps_instance_and_character_apart() {
        let who = Who::new("prime", "Sindar");
        assert_eq!(who.instance, "prime");
        assert_eq!(who.character, "Sindar");
        assert_eq!(who.name(), "Sindar");
    }

    #[test]
    fn stamp_now_reads_the_system_clock_and_keeps_its_pair() {
        let before = Stamp::now(0, 0).at;
        let stamp = Stamp::now(13, 5);
        assert_eq!(stamp.hour, 13);
        assert_eq!(stamp.minute, 5);
        // Loose bounds: after 2020 and at most the i64 ceiling.
        assert!(stamp.at >= 1_577_836_800, "at = {}", stamp.at);
        assert!(stamp.at <= i64::MAX);
        assert!(stamp.at >= before);
    }
}
