//! Keying and caching of posed puppets, bodies, skin textures and pose sets.
//!
//! Recovered from `gs_render::puppet_cache::*`. This is the module the parent
//! flagged for the cross-crate symbol
//! `<SplatKey as From<gs_puppet::splat::SplatParams>>::from` (`0x1008f1cb4`);
//! see [`SplatKey`] and `NOTES.md` § "Integration gap" for the seam that stands
//! in for `gs_puppet::splat::SplatParams`.
//!
//! Everything is `std`-only: the hash maps that the binary built with
//! `hashbrown` are `std::collections::HashMap`, the `parking_lot`/`Mutex` uses
//! are `std::sync::Mutex`, and the pose/mesh model types that used to come from
//! `gs_puppet` are defined here with the field offsets observed in the
//! disassembly (`// INTEGRATION:`).

use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gs_base::body::{part_label, BodyPart};
use gs_base::status::{Status, Statuses};

use crate::Result;

// ---------------------------------------------------------------------------
// PuppetSource — the pluggable provider of body/pose/skin data.
// ---------------------------------------------------------------------------

/// One skin sheet a source can hand out.
///
/// `PuppetSource::sheets` builds these from `".texture.png"` / `".normal.png"`
/// suffixes (`0x1021b43d1`, `0x1021b43dd`) read next to the body file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sheet {
    /// Sheet name as it appears in the UV layout (`"Head"`, `"body_thin"`, ...).
    pub name: String,
    /// Suffix the sheet was found under, e.g. `".texture.png"`.
    pub suffix: &'static str,
}

/// The provider `PuppetCache` pulls everything from.
///
/// Recovered from the two implementors in the binary:
/// `<injury_doll::source::DollSource as PuppetSource>` and
/// `<gs_render::puppet_cache::NoSource as PuppetSource>`. Method order is the
/// vtable order (the first method is at `+0x18`, right after
/// `drop_in_place`/`size`/`align`).
pub trait PuppetSource: Send + Sync {
    /// `as_any` (`0x1008e8bb4`, 12 bytes: returns `(self + 0, &VTABLE)`).
    fn as_any(&self) -> &dyn Any;
    /// `as_any_mut` (`0x1008e8968`).
    fn as_any_mut(&mut self) -> &mut dyn Any;
    /// `generation_now` → `gs_base::data_dirs::base_generation()`
    /// (`0x1008e8974` is a two-instruction tail call into it).
    fn generation_now(&self) -> u64;
    /// `refresh` → also `base_generation()` (`0x1008e8d98`).
    fn refresh(&self) -> u64;
    /// `baked` — the baked body TOML (`DollSource::baked_toml`, `0x1008e89a0`).
    fn baked(&self) -> Option<Arc<str>>;
    /// `sheets` — the skin/normal sheet names next to the body.
    fn sheets(&self) -> Vec<Sheet>;
    /// `body_dir` — `<chr>_body` directory (`Path::join` + `is_file`, `0x1008e8dc4`).
    fn body_dir(&self) -> Option<PathBuf>;
    /// `pose_file` — `gs_puppet::pose::PoseSet::shipped_toml(&str)` wrapped in an
    /// `Arc<str>` (`0x1008e8ed8`; the returned fat pointer is `(ptr, len)`).
    fn pose_file(&self, family: &str) -> Option<Arc<str>>;
    /// `arm` — the arm/hang description used by
    /// `gs_puppet::hang::Puppet::arm_hangs_at` (`0x1008e9288`).
    fn arm(&self) -> Option<String> {
        None
    }
    /// `dress` — the worn-layer list (`0x1008e94f8`, a 12-byte body in the
    /// binary: it only forwards to the injury doll's own accessor).
    fn dress(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The null source: every accessor is empty.
///
/// `NoSource` vtable bodies are 8-12 bytes each (`mov w0,#0` / `ret`), i.e. all
/// of them return `None`/empty.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoSource;

impl PuppetSource for NoSource {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn generation_now(&self) -> u64 {
        0
    }
    fn refresh(&self) -> u64 {
        0
    }
    fn baked(&self) -> Option<Arc<str>> {
        None
    }
    fn sheets(&self) -> Vec<Sheet> {
        Vec::new()
    }
    fn body_dir(&self) -> Option<PathBuf> {
        None
    }
    fn pose_file(&self, _family: &str) -> Option<Arc<str>> {
        None
    }
}

/// A shared, optional [`PuppetSource`].
///
/// `<Source as Default>::default` @0x1008f3dc8 is 16 bytes: it returns
/// `(1, 0x1043ae5c8)` — a fat pointer whose data word is the dangling `1` of a
/// zero-sized value and whose vtable is `NoSource`'s. That is exactly
/// `&'static dyn PuppetSource` to a `NoSource`, so `Default` is modelled as an
/// absent source that falls back to the static [`NoSource`].
pub struct Source(Option<Arc<dyn PuppetSource>>);

impl Default for Source {
    fn default() -> Self {
        Source(None)
    }
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Source(..)"
        } else {
            "Source(NoSource)"
        })
    }
}

impl Source {
    pub fn new(src: Arc<dyn PuppetSource>) -> Self {
        Source(Some(src))
    }

    /// The provider, or the static [`NoSource`] when none was installed.
    pub fn get(&self) -> &dyn PuppetSource {
        static NONE: NoSource = NoSource;
        match &self.0 {
            Some(s) => s.as_ref(),
            None => &NONE,
        }
    }
}

// ---------------------------------------------------------------------------
// SplatKey — the cross-crate conversion seam.
// ---------------------------------------------------------------------------

/// The quantised splat key.
///
/// `<SplatKey as From<gs_puppet::splat::SplatParams>>::from` is a 180-byte
/// function at `0x1008f1cb4` whose whole body is:
///
/// ```text
/// a = round(p[0] * 1000.0).clamp(5.0, 500.0) as u16
/// b = round(p[1] *  100.0).clamp(20.0, 200.0) as u16
/// c = round(p[2] *  100.0).clamp( 5.0, 100.0) as u8
/// SplatKey(a as u64 | (b as u64) << 16 | (c as u64) << 32)
/// ```
///
/// (`fmov w8,#0x447a0000` = 1000.0, `#0x43fa0000` = 500.0, `#0x42c80000` =
/// 100.0, `#0x43480000` = 200.0, `mov w11,#0xff` = the `u8` clamp; the packing
/// is `lsl x10,x10,#32; orr x9,x10,x9,lsl #16; orr x0,x9,x8`.)
///
/// The parameter type is [`gs_puppet::splat::SplatParams`]; its `radius`,
/// `opacity` and `detail` fields sit at `+0x00`, `+0x04`, `+0x08`, the only
/// offsets this function reads. [`SplatKey::from_params_parts`] is the exact
/// recovered conversion and [`SplatKey::from_params`] is the `[f32; 3]` view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SplatKey(pub u64);

impl From<gs_puppet::splat::SplatParams> for SplatKey {
    /// `<SplatKey as From<gs_puppet::splat::SplatParams>>::from` @0x1008f1cb4.
    fn from(p: gs_puppet::splat::SplatParams) -> SplatKey {
        SplatKey::from_params_parts(p.radius, p.opacity, p.detail)
    }
}

impl SplatKey {
    /// The three quantisation clamps, as recovered.
    pub const SIZE_RANGE: (f32, f32) = (5.0, 500.0);
    pub const WIDTH_RANGE: (f32, f32) = (20.0, 200.0);
    pub const HEIGHT_RANGE: (f32, f32) = (5.0, 100.0);

    /// `a | b << 16 | c << 32` — the packing used by the `From` impl.
    pub fn from_params_parts(a: f32, b: f32, c: f32) -> SplatKey {
        SplatKey(
            quantise(a, 1000.0, Self::SIZE_RANGE) as u64
                | (quantise(b, 100.0, Self::WIDTH_RANGE) as u64) << 16
                | (quantise(c, 100.0, Self::HEIGHT_RANGE) as u64) << 32,
        )
    }

    /// The same conversion from a `[f32; 3]` (the three quantised fields in
    /// `SplatParams` order).
    pub fn from_params(p: [f32; 3]) -> SplatKey {
        SplatKey::from_params_parts(p[0], p[1], p[2])
    }

    /// `a` — the low 16 bits.
    pub const fn size(self) -> u16 {
        (self.0 & 0xffff) as u16
    }
    /// `b` — bits 16..32.
    pub const fn width(self) -> u16 {
        ((self.0 >> 16) & 0xffff) as u16
    }
    /// `c` — bits 32..40.
    pub const fn height(self) -> u8 {
        ((self.0 >> 32) & 0xff) as u8
    }
}

/// `round(v * scale).clamp(lo, hi)` with `round` = `frinta` (half away from
/// zero) and the `u16`/`u8` saturation the binary applies afterwards.
fn quantise(v: f32, scale: f32, range: (f32, f32)) -> u32 {
    let r = (v * scale).round();
    let r = if r < range.0 {
        range.0
    } else if r > range.1 {
        range.1
    } else {
        r
    };
    r as u32
}

// ---------------------------------------------------------------------------
// Effects — the 20 status flags PuppetKey carries at +0x0c.
// ---------------------------------------------------------------------------

/// The statuses expanded into the `Effects` flag array.
///
/// The binary expands the `u64` status bitset into 20 one-byte flags with a
/// Neon gather (`ushl.2d` by `63 - bit`, then `uzp1` narrowing) over the shift
/// table at `0x1021b4c70`. Decoding that table gives the bit indices
/// `7,8,9,10,11,12,13,14,15,16,17,18,19,20,26,28,29,31`; two lanes of the
/// gather are padded with shift `0` (bit 63, never set).
///
/// `// UNKNOWN:` the *lane order* of the gather was not decoded instruction by
/// instruction, so the order below is the ascending-bit order of the recovered
/// set rather than the exact store order.
pub const EFFECT_STATUSES: [Status; 18] = [
    Status::Dead,      // 7
    Status::Frozen,    // 8
    Status::Immobile,  // 9
    Status::Poisoned,  // 10
    Status::Diseased,  // 11
    Status::Bleeding,  // 12
    Status::Rooted,    // 13
    Status::Bound,     // 14
    Status::Hidden,    // 15
    Status::Invisible, // 16
    Status::Possessed, // 17
    Status::Blind,     // 18
    Status::Cutthroat, // 19
    Status::Silenced,  // 20
    Status::Staggered, // 26
    Status::Confused,  // 28
    Status::Frenzied,  // 29
    Status::Slowed,    // 31
];

/// The shader-side status flags, 20 bytes at `PuppetKey+0x0c`.
///
/// `gs_puppet::skin::Effects` in the original (`PuppetKey::eq` calls
/// `gs_puppet::skin::Effects::eq` at `0x1008f3fa0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Effects {
    pub flags: [bool; EFFECT_SLOTS],
}

/// Number of flag bytes stored (`0x14` = 20).
pub const EFFECT_SLOTS: usize = 20;

impl From<Statuses> for Effects {
    fn from(st: Statuses) -> Self {
        let mut flags = [false; EFFECT_SLOTS];
        for (slot, status) in EFFECT_STATUSES.iter().enumerate() {
            flags[slot] = st.contains(*status);
        }
        Effects { flags }
    }
}

impl Effects {
    /// Which status backs a flag slot, if any.
    pub fn status_of(slot: usize) -> Option<Status> {
        EFFECT_STATUSES.get(slot).copied()
    }
}

// ---------------------------------------------------------------------------
// PuppetKey
// ---------------------------------------------------------------------------

/// Number of indexed body parts in the per-part arrays (`[u8; 14]`).
pub const PART_SLOTS: usize = 14;

/// One `(name, severity)` pair feeding the per-part rank arrays.
///
/// Stride 32 in the binary: a `String` at `+0`, the severity byte at `+0x18`
/// (`ldp x0, x1, [x27, #-0x10]` reads the string's `(ptr, len)`, `ldrb w9,
/// [x27]` with `x27 = entry + 0x18` reads the severity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Severity {
    pub name: String,
    pub severity: u8,
}

impl Severity {
    pub fn new(name: impl Into<String>, severity: u8) -> Self {
        Severity {
            name: name.into(),
            severity,
        }
    }
}

/// The cache key for a posed puppet — `0x78` (120) bytes, `Copy`, `Eq`.
///
/// Layout recovered from `PuppetKey::new` (`0x100902420`), `PartialEq::eq`
/// (`0x1008f3dd8`) and the `with_*` builders:
///
/// | offset | field |
/// |---|---|
/// | `+0x00` | `statuses: u64` (compared last) |
/// | `+0x08` | `arrive: u16`, `+0x0a` `arrive_at: u16` (only compared when `arrive == 1`) |
/// | `+0x0c` | `effects: Effects` (20 bytes) |
/// | `+0x20` | `layer_period: u16` |
/// | `+0x22` | `layer_side: u8` (1 or 2) |
/// | `+0x23` | `layer_share: u8` (0..=100) |
/// | `+0x24` | `layer_kind: u8` |
/// | `+0x25` | pad |
/// | `+0x26` | `pose: u8` (a [`PoseChoice`] discriminant) |
/// | `+0x27` | `pose_side: u8` (compared only for pose 8 and 9) |
/// | `+0x28` | `ranks: [u8; 14]` |
/// | `+0x36` | `scars: [u8; 14]` |
/// | `+0x44` | `size: u16` (a [`size_step`] value) |
/// | `+0x46` | `layer_pace: [u16; 3]` (default `30, 60, 90`) |
/// | `+0x4c` | `marks: [u8; 14]` (`// UNKNOWN:` purpose) |
/// | `+0x5a` | `marks2: [u8; 14]` (`// UNKNOWN:` purpose) |
/// | `+0x68` | `flag68: u8`, `+0x69` `flag69: u8`, `+0x6a` `flag6a: u8` |
/// | `+0x6b` | `layers: u8`, `+0x6c` `layer_live: u8` |
/// | `+0x6d` | `modelled: u8`, `+0x6e` `modelled_kind: u8` |
/// | `+0x6f` | `fresh: u8`, `+0x70` `fresh_id: u8`, `+0x71` `fresh_step: u8` |
/// | `+0x72` | `flag72: u8` |
/// | `+0x73` | `extra: [u8; 3]` |
/// | `+0x76` | `breath: u8` |
/// | `+0x77` | `modelled_step: u8` |
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PuppetKey {
    pub statuses: u64,            // +0x00
    pub arrive: u16,              // +0x08
    pub arrive_at: u16,           // +0x0a
    pub effects: Effects,         // +0x0c
    pub layer_period: u16,        // +0x20
    pub layer_side: u8,           // +0x22
    pub layer_share: u8,          // +0x23
    pub layer_kind: u8,           // +0x24
    pub pad_25: u8,               // +0x25
    pub pose: u8,                 // +0x26
    pub pose_side: u8,            // +0x27
    pub ranks: [u8; PART_SLOTS],  // +0x28
    pub scars: [u8; PART_SLOTS],  // +0x36
    pub size: u16,                // +0x44
    pub layer_pace: [u16; 3],     // +0x46
    pub marks: [u8; PART_SLOTS],  // +0x4c
    pub marks2: [u8; PART_SLOTS], // +0x5a
    pub flag68: u8,               // +0x68
    pub flag69: u8,               // +0x69
    pub flag6a: u8,               // +0x6a
    pub layers: u8,               // +0x6b
    pub layer_live: u8,           // +0x6c
    pub modelled: u8,             // +0x6d
    pub modelled_kind: u8,        // +0x6e
    pub fresh: u8,                // +0x6f
    pub fresh_id: u8,             // +0x70
    pub fresh_step: u8,           // +0x71
    pub flag72: u8,               // +0x72
    pub extra: [u8; 3],           // +0x73
    pub breath: u8,               // +0x76
    pub modelled_step: u8,        // +0x77
}

/// `PuppetKey::eq` @0x1008f3dd8 — the recovered comparison order.
///
/// Taken straight from the branch chain:
///
/// 1. `flag68`, `flag69`, `flag6a`;
/// 2. `size` (`+0x44`, `ldrh`);
/// 3. `breath` (`+0x76`);
/// 4. `layers`, `layer_live`, `modelled`, `modelled_kind`, `modelled_step`,
///    `flag72`;
/// 5. `pose` (`+0x26`) — and `pose_side` (`+0x27`) **only** when the pose is
///    `Prone` (8) or `Dead` (9) (`cmp w8, #0x9; / cmp w8, #0x8`);
/// 6. `ranks` (14 bytes), `scars` (14 bytes), `extra` (3 bytes);
/// 7. `arrive`: when `self.arrive == 1` the other side's low bit must be set
///    *and* `arrive_at` must match, otherwise the other side's low bit must be
///    clear (`cmp w9, #0x1; b.ne` / `tbz w8, #0x0`);
/// 8. `effects` (`gs_puppet::skin::Effects::eq`);
/// 9. `layer_pace[0]`, `layer_pace[1]`, and `layer_pace[2]` compared as a
///    *byte* (`ldrb w9, [x10, #0x4a]`);
/// 10. `layer_side`, `layer_share`, `layer_period`, `layer_kind`;
/// 11. `marks` (14 bytes), `marks2` (14 bytes);
/// 12. `fresh` with the same "== 1" rule as `arrive`, then `fresh_id`,
///     `fresh_step`;
/// 13. `statuses` — compared last (`ldr x9, [x10]; cmp; cset w0, eq`).
impl PartialEq for PuppetKey {
    fn eq(&self, other: &Self) -> bool {
        self.flag68 == other.flag68
            && self.flag69 == other.flag69
            && self.flag6a == other.flag6a
            && self.size == other.size
            && self.breath == other.breath
            && self.layers == other.layers
            && self.layer_live == other.layer_live
            && self.modelled == other.modelled
            && self.modelled_kind == other.modelled_kind
            && self.modelled_step == other.modelled_step
            && self.flag72 == other.flag72
            && self.pose == other.pose
            && (self.pose_side == other.pose_side || !matches!(self.pose, 8 | 9))
            && self.ranks == other.ranks
            && self.scars == other.scars
            && self.extra == other.extra
            && flag_eq(self.arrive as u32, other.arrive as u32)
            && (self.arrive != 1 || self.arrive_at == other.arrive_at)
            && self.effects == other.effects
            && self.layer_pace[0] == other.layer_pace[0]
            && self.layer_pace[1] == other.layer_pace[1]
            && (self.layer_pace[2] as u8) == (other.layer_pace[2] as u8)
            && self.layer_side == other.layer_side
            && self.layer_share == other.layer_share
            && self.layer_period == other.layer_period
            && self.layer_kind == other.layer_kind
            && self.marks == other.marks
            && self.marks2 == other.marks2
            && flag_eq(self.fresh as u32, other.fresh as u32)
            && (self.fresh != 1 || self.fresh_id == other.fresh_id)
            && self.fresh_step == other.fresh_step
            && self.statuses == other.statuses
    }
}

impl Eq for PuppetKey {}

/// The `== 1` rule the binary uses for `arrive` and `fresh`: the low bit must
/// agree, and when it is set the companion field must match too.
fn flag_eq(a: u32, b: u32) -> bool {
    (a & 1) == (b & 1)
}

/// Hash over exactly the fields [`PuppetKey::eq`] compares, so the two agree.
impl std::hash::Hash for PuppetKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.flag68.hash(state);
        self.flag69.hash(state);
        self.flag6a.hash(state);
        self.size.hash(state);
        self.breath.hash(state);
        self.layers.hash(state);
        self.layer_live.hash(state);
        self.modelled.hash(state);
        self.modelled_kind.hash(state);
        self.modelled_step.hash(state);
        self.flag72.hash(state);
        self.pose.hash(state);
        if matches!(self.pose, 8 | 9) {
            self.pose_side.hash(state);
        }
        self.ranks.hash(state);
        self.scars.hash(state);
        self.extra.hash(state);
        (self.arrive & 1).hash(state);
        if self.arrive == 1 {
            self.arrive_at.hash(state);
        }
        self.effects.hash(state);
        self.layer_pace[0].hash(state);
        self.layer_pace[1].hash(state);
        (self.layer_pace[2] as u8).hash(state);
        self.layer_side.hash(state);
        self.layer_share.hash(state);
        self.layer_period.hash(state);
        self.layer_kind.hash(state);
        self.marks.hash(state);
        self.marks2.hash(state);
        (self.fresh & 1).hash(state);
        if self.fresh == 1 {
            self.fresh_id.hash(state);
        }
        self.fresh_step.hash(state);
        self.statuses.hash(state);
    }
}

/// `size_of::<PuppetKey>()` in the binary: `0x78` bytes (`PuppetKey::new`
/// writes up to `+0x77` and every `with_*` builder copies exactly `0x78`).
pub const PUPPET_KEY_SIZE: usize = 0x78;

/// The default layer paces stored by `PuppetKey::new`: `strh w8,[x19,#0x4a]`
/// with `#0x5a`, and `stur w8,[x19,#0x46]` with `0x003c001e`.
pub const DEFAULT_LAYER_PACE: [u16; 3] = [30, 60, 90];

/// `PuppetKey::new` @0x100902420.
///
/// Arguments, mapped from the register use:
///
/// | arg | register | field |
/// |---|---|---|
/// | `pose` | `x0`/`w25` | `+0x26` |
/// | `pose_side` | `x1`/`w24` | `+0x27` |
/// | `ranks` | `x2`,`x3` | `+0x28` (via `part_label`) |
/// | `flag68` | `x4`/`w23` | `+0x68` |
/// | `flag69` | `x5`/`w22` | `+0x69` |
/// | `extra` | `x6`/`x21` | `+0x73` (3 bytes) |
/// | `flag6a` | `x7` | `+0x6a` |
/// | `height` | `s0` | `+0x44` via [`size_step`] |
///
/// The rank loop clamps each severity to `3` (`csel w9, w9, w20, lo` with
/// `w20 = 3`) and keeps the maximum per part.
impl PuppetKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pose: u8,
        pose_side: u8,
        ranks: &[Severity],
        flag68: u8,
        flag69: u8,
        extra: [u8; 3],
        flag6a: u8,
        height: f32,
    ) -> PuppetKey {
        let mut key = PuppetKey {
            statuses: 0,
            arrive: 0,
            arrive_at: 0,
            effects: Effects::default(),
            layer_period: 0,
            layer_side: 0,
            layer_share: 0,
            layer_kind: 0,
            pad_25: 0,
            pose,
            pose_side,
            ranks: [0; PART_SLOTS],
            scars: [0; PART_SLOTS],
            size: size_step(height) as u16,
            layer_pace: DEFAULT_LAYER_PACE,
            marks: [0; PART_SLOTS],
            marks2: [0; PART_SLOTS],
            flag68,
            flag69,
            flag6a,
            layers: 0,
            layer_live: 0,
            modelled: 0,
            modelled_kind: 1,
            fresh: 0,
            fresh_id: 0,
            fresh_step: 0,
            flag72: 0,
            extra,
            breath: 0,
            modelled_step: 0,
        };
        fill_part_array(&mut key.ranks, ranks);
        key
    }
}

/// The shared `part_label` loop used by `PuppetKey::new` (`+0x28`) and
/// `PuppetKey::with_scars` (`+0x36`): keep the maximum severity per part,
/// clamping at 3.
fn fill_part_array(slot: &mut [u8; PART_SLOTS], entries: &[Severity]) {
    for e in entries {
        if let Some(part) = part_label(&e.name) {
            let idx = part as usize - 1;
            if idx < PART_SLOTS {
                let sev = e.severity.min(3);
                slot[idx] = slot[idx].max(sev);
            }
        }
    }
}

/// A breathing pace: `(rate, depth)`.
///
/// `PuppetKey::breath_pace` @0x100902080 returns `Option<BreathPace>`; the
/// payload is an `f64` at `+0x08` and an `f32` at `+0x10` of the sret.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreathPace {
    pub rate: f64,
    pub depth: f32,
}

impl PuppetKey {
    /// `PuppetKey::with_statuses` @0x100902304 — store the resolved status
    /// bitset and expand it into [`Effects`].
    pub fn with_statuses(mut self, statuses: Statuses) -> Self {
        let resolved = statuses.resolved();
        self.statuses = resolved.0;
        self.effects = Effects::from(resolved);
        self
    }

    /// `PuppetKey::with_scars` @0x100901fb0 — rebuild the scar array from
    /// `(name, severity)` pairs (the array at `+0x36` is zeroed first).
    pub fn with_scars(mut self, scars: &[Severity]) -> Self {
        self.scars = [0; PART_SLOTS];
        fill_part_array(&mut self.scars, scars);
        self
    }

    /// `PuppetKey::with_breath` @0x100902158 — `breath & 0xff` when the high
    /// nibble is clear, otherwise the `0x10` sentinel.
    pub fn with_breath(mut self, breath: u8) -> Self {
        self.breath = if breath & 0xf0 == 0 { breath } else { 0x10 };
        self
    }

    /// `PuppetKey::with_modelled` @0x1009022b4.
    ///
    /// ```text
    /// modelled      = m
    /// modelled_kind = 0
    /// modelled_step = if m != 0 { 0 }
    ///                 else if step & 0xfe == 0 { step } else { 2 }
    /// ```
    pub fn with_modelled(mut self, modelled: u8, step: u8) -> Self {
        self.modelled = modelled;
        self.modelled_kind = 0;
        let step = if step & 0xfe == 0 { step } else { 2 };
        self.modelled_step = if modelled != 0 { 0 } else { step };
        self
    }

    /// `PuppetKey::with_fresh` @0x1009021f0 — mark the key as freshly spawned
    /// within its first four seconds.
    ///
    /// Recovered: the flag is only set when `layers == 1` and `0 <= age < 4.0`
    /// (`fccmp d0, d1(4.0), #0, ge` then `b.mi`); the step is
    /// `min((age / 0.05) as u32, 255)` (`0x3fa999999999999a` = 0.05).
    ///
    /// `// UNKNOWN:` on the `else` path the binary stores an *uninitialised*
    /// `w10` into `+0x71` (the `mov w9, #0x0` at `0x100901f48` has no matching
    /// `mov w10`), so the step for a non-fresh key is not recoverable; `0` is
    /// used here.
    pub fn with_fresh(mut self, fresh_id: u8, age: f64) -> Self {
        let (fresh, step) = if self.layers == 1 && age >= 0.0 && age < 4.0 {
            (1u8, ((age / 0.05) as u32).min(255) as u8)
        } else {
            (0u8, 0u8)
        };
        self.fresh = fresh;
        self.fresh_id = fresh_id;
        self.fresh_step = step;
        self
    }

    /// `PuppetKey::with_layers` @0x100902198 — set the worn-layer state.
    ///
    /// `layers == 0` clears everything; otherwise the paces triple is stored
    /// (`x2` when the caller passes one, [`DEFAULT_LAYER_PACE`] otherwise) and
    /// the layer placement `at` is packed into `+0x20..+0x25`:
    ///
    /// ```text
    /// layer_period = round(at.a * 100.0).clamp(0, 100) as u16   // 0x42c80000
    /// layer_side   = if at.c != 0 { 2 } else { 1 }
    /// layer_share  = round(at.b * 1000.0).clamp(0, 2000) as u8  // 0x447a0000 / 0x44fa0000
    /// layer_kind   = at.kind
    /// ```
    pub fn with_layers(mut self, layers: u8, paces: Option<[u16; 3]>, at: &LayerAt) -> Self {
        self.layers = layers;
        self.layer_live = layers;
        self.layer_pace = if layers != 0 {
            paces.unwrap_or(DEFAULT_LAYER_PACE)
        } else {
            DEFAULT_LAYER_PACE
        };
        if layers != 0 && at.kind != 2 {
            let period = (at.a * 100.0).round().clamp(0.0, 100.0) as u16;
            let share = (at.b * 1000.0).round().clamp(0.0, 2000.0) as u8;
            self.layer_period = period;
            self.layer_side = if at.c != 0 { 2 } else { 1 };
            self.layer_share = share;
            self.layer_kind = at.kind;
            self.pad_25 = 0;
        } else {
            self.layer_period = 0;
            self.layer_side = 0;
            self.layer_share = 0;
            self.layer_kind = 0;
            self.pad_25 = 0;
        }
        self
    }

    /// `PuppetKey::breath_pace` @0x100902080 — how fast this puppet breathes.
    ///
    /// Fully recovered:
    ///
    /// * `statuses & 0x6320 != 0` → `None` (any of `Unconscious` bit 5,
    ///   `Frozen` 8, `Immobile` 9, `Rooted` 13, `Bound` 14 — `0x6320` is
    ///   `0b0110_0011_0010_0000`);
    /// * otherwise `Overexerted` (bit 30) → `(2.6, 1.9)`, `Frenzied` (29) →
    ///   `(2.2, 1.5)`, `Slowed` (31) → `(0.45, 1.0)`, `Horrified` (22) →
    ///   `(1.8, 1.2)`, else `(1.0, 1.0)`.
    pub fn breath_pace(&self) -> Option<BreathPace> {
        const BLOCKED: u64 = 0x6320;
        if self.statuses & BLOCKED != 0 {
            return None;
        }
        let bits = self.statuses as u32;
        let (rate, depth) = if bits & (1 << 30) != 0 {
            (2.6, 1.9)
        } else if bits & (1 << 29) != 0 {
            (2.2, 1.5)
        } else if bits & (1 << 31) != 0 {
            (0.45, 1.0)
        } else if bits & (1 << 22) != 0 {
            (1.8, 1.2)
        } else {
            (1.0, 1.0)
        };
        Some(BreathPace { rate, depth })
    }

    /// `nerve_rank` @0x1008f45f4 — the nervous-system severity, `0` when the
    /// part is unknown.
    pub fn nerve_rank(&self) -> u8 {
        nerve_rank(self)
    }
}

/// `nerve_twitch`/`flinch`/`death_flinch` share this clamp on the `f64` phase.
pub const FLINCH_WINDOW: f64 = 0.55;

/// The `0.55`-ish constant used by all three flinch paths:
/// `movk #0x9000; movk #0x9999; movk #0x3fe1` = `0x3FE1999999999001`.
pub const FLINCH_LIMIT: f64 = 0.5499999999999998;

/// One entry of the body-part placement used by `with_layers`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LayerAt {
    /// `+0x00`
    pub a: f32,
    /// `+0x04`
    pub b: f32,
    /// `+0x08` — non-zero selects side 2
    pub c: u8,
    /// `+0x09` — `2` means "no placement"
    pub kind: u8,
}

// ---------------------------------------------------------------------------
// PoseChoice / PoseSet
// ---------------------------------------------------------------------------

/// Which pose a puppet's statuses resolve to.
///
/// Discriminants are the ones `PoseChoice::resolve` uses as a jump-table index
/// (table at `0x1021b46e8`, 10 entries: `3b 00 16 1a 04 24 29 1f 2e 08`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PoseChoice {
    /// `0` — upright, "stand".
    Stand = 0,
    /// `1` — kneeling.
    Kneeling = 1,
    /// `2` — sitting.
    Sitting = 2,
    /// `3` — stunned.
    Stunned = 3,
    /// `4` — sleeping.
    Sleeping = 4,
    /// `5` — webbed.
    Webbed = 5,
    /// `6` — disoriented/dazed/confused.
    Disoriented = 6,
    /// `7` — calmed.
    Calmed = 7,
    /// `8` — prone/unconscious (side selected by the second flag).
    Prone = 8,
    /// `9` — dead (uses the pose set's dead name).
    Dead = 9,
}

impl PoseChoice {
    pub const ALL: [PoseChoice; 10] = [
        PoseChoice::Stand,
        PoseChoice::Kneeling,
        PoseChoice::Sitting,
        PoseChoice::Stunned,
        PoseChoice::Sleeping,
        PoseChoice::Webbed,
        PoseChoice::Disoriented,
        PoseChoice::Calmed,
        PoseChoice::Prone,
        PoseChoice::Dead,
    ];

    pub fn from_u8(v: u8) -> Option<PoseChoice> {
        PoseChoice::ALL.get(v as usize).copied()
    }

    /// The candidate pose names in the order `resolve` tries them
    /// (`0x1043ae650` onward; each entry is a `(&str, usize)` pair).
    pub fn candidates(self, side: u8) -> &'static [&'static str] {
        match self {
            PoseChoice::Stand => &["stand"],
            PoseChoice::Kneeling => &["kneel", "lie", "sit", "stand"],
            PoseChoice::Sitting => &["sit", "lie", "kneel", "stand"],
            PoseChoice::Stunned => &["stunned", "stand"],
            PoseChoice::Sleeping => &["sleep", "lie", "fallen_side", "stand"],
            PoseChoice::Webbed => &["webbed", "stand"],
            PoseChoice::Disoriented => &["disoriented", "stunned", "stand"],
            PoseChoice::Calmed => &["calm", "stand"],
            PoseChoice::Prone => {
                if side != 0 {
                    &["fallen_side", "prone", "lie", "stand"]
                } else {
                    &["prone", "supine", "fallen_side", "stand"]
                }
            }
            PoseChoice::Dead => &["dead", "fallen_side", "prone", "stand"],
        }
    }

    /// `PoseChoice::of_statuses` @0x1008f4170 — statuses → `(choice, flag)`.
    ///
    /// The check order is the binary's:
    ///
    /// 1. `statuses & 0xa0 == 0x20` (Unconscious and not Dead) → `(Prone, true)`;
    /// 2. `statuses & 0x80` (Dead) → `(Dead, false)`;
    /// 3. `has_name("prone")` → `(Prone, false)`;
    /// 4. `"kneeling"`, `"sitting"`, `"sleeping"`, `"webbed"`, `"stunned"`,
    ///    `"disoriented"`, `"calmed"` → choices 1,2,4,5,3,6,7;
    /// 5. otherwise `(Stand, false)`.
    ///
    /// The closure at `0x1008f42e0` widens three names before delegating to
    /// `Statuses::has_name`: `"webbed"` is `Web || Bound`, `"disoriented"` is
    /// `Dazed || Confused`, everything else goes through `has_name`.
    pub fn of_statuses(statuses: &Statuses) -> (PoseChoice, bool) {
        let bits = statuses.0;
        if bits & 0xa0 == 0x20 {
            return (PoseChoice::Prone, true);
        }
        if bits & 0x80 != 0 {
            return (PoseChoice::Dead, false);
        }
        let has = |name: &str| status_has(statuses, name);
        if has("prone") {
            return (PoseChoice::Prone, false);
        }
        if has("kneeling") {
            return (PoseChoice::Kneeling, false);
        }
        if has("sitting") {
            return (PoseChoice::Sitting, false);
        }
        if has("sleeping") {
            return (PoseChoice::Sleeping, false);
        }
        if has("webbed") {
            return (PoseChoice::Webbed, false);
        }
        if has("stunned") {
            return (PoseChoice::Stunned, false);
        }
        if has("disoriented") {
            return (PoseChoice::Disoriented, false);
        }
        if has("calmed") {
            return (PoseChoice::Calmed, false);
        }
        (PoseChoice::Stand, false)
    }

    /// `PoseChoice::resolve` @0x1008f4374 — pick the first pose the set has.
    ///
    /// Returns the pose name and pose together (`pstp x21, x22, [x19]` /
    /// `str x0, [x19, #0x10]`). For [`PoseChoice::Dead`] the set's
    /// `dead_name(family)` is tried first and only its failure falls back to the
    /// `["dead", …]` list.
    pub fn resolve(self, side: u8, poses: &PoseSet, family: &str) -> Option<(String, Arc<Pose>)> {
        if self == PoseChoice::Dead {
            if let Some(name) = poses.dead_name(family) {
                if let Some(pose) = poses.get(&name) {
                    return Some((name, pose));
                }
            }
        }
        for name in self.candidates(side) {
            if let Some(pose) = poses.get(name) {
                return Some((poses.name_of(name).to_string(), pose));
            }
        }
        None
    }
}

/// `Statuses::has_name` with the three widened names of
/// `PoseChoice::of_statuses::{{closure}}` (`0x1008f42e0`).
pub fn status_has(statuses: &Statuses, name: &str) -> bool {
    match name {
        // `cmp w2,#6` -> "webb"+"ed": Web || Bound
        "webbed" => statuses.contains(Status::Web) || statuses.contains(Status::Bound),
        // `cmp w2,#11` -> "disorien"+"oriented": Dazed || Confused
        "disoriented" => statuses.contains(Status::Dazed) || statuses.contains(Status::Confused),
        _ => statuses.has_name(name),
    }
}

/// A pose: `gs_puppet::pose::Pose` in the original.
///
/// `// INTEGRATION:` only the identity is modelled — the bone transforms live in
/// `gs_puppet`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pose {
    pub name: String,
    /// `PoseSet::clamp`/`Pose::blend` inputs the cache passes through.
    pub weight: f32,
}

/// A family of poses: `gs_puppet::pose::PoseSet` in the original.
///
/// `PuppetCache::pose_set` loads it with `PoseSet::from_toml` /
/// `PoseSet::for_family` (`0x1008fa058`) and the cache keys it by family name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoseSet {
    pub family: String,
    pub poses: HashMap<String, Arc<Pose>>,
    /// `Dead` poses keyed by family (`PoseSet::dead_name` / `dead_hash`).
    pub dead: HashMap<String, String>,
    /// Family entries walked by `resolve` (`PoseSet+0x40` / `+0x48`).
    pub families: Vec<String>,
    /// `PoseSet::dead_hash` results (`0x100a4aba4`).
    pub dead_hashes: HashMap<String, u8>,
}

impl PoseSet {
    pub fn new(family: impl Into<String>) -> Self {
        PoseSet {
            family: family.into(),
            ..Default::default()
        }
    }

    pub fn insert(&mut self, name: impl Into<String>, weight: f32) {
        let name = name.into();
        self.poses
            .insert(name.clone(), Arc::new(Pose { name, weight }));
    }

    /// `PoseSet::get` (`0x100a4a310`).
    pub fn get(&self, name: &str) -> Option<Arc<Pose>> {
        self.poses.get(name).cloned()
    }

    /// The canonical name of a pose (the binary returns the `&str` it matched).
    pub fn name_of<'a>(&'a self, name: &'a str) -> &'a str {
        match self.poses.get_key_value(name) {
            Some((k, _)) => k.as_str(),
            None => name,
        }
    }

    /// `PoseSet::dead_name(&self, family)` (`0x100a4af50`, returns
    /// `Option<&str>`).
    pub fn dead_name(&self, family: &str) -> Option<String> {
        self.dead.get(family).cloned()
    }

    /// `PoseSet::dead_hash(self, name)` (`0x100a4aba4`, the result is clamped to
    /// `0xff` by `dead_choice` at `0x1008f51fc`).
    pub fn dead_hash(&self, name: &str) -> usize {
        self.dead_hashes.get(name).copied().unwrap_or(0).min(0xff) as usize
    }

    /// `PoseSet::clamp` — the cache clamps a pose's weight into `0..=1`.
    pub fn clamp(&self, weight: f32) -> f32 {
        weight.clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// `nerve_rank` @0x1008f45f4 — the severity stored for `BodyPart::Nsys`.
///
/// The binary calls `part_label("nsys")` (the shortest alias for "nervous
/// system") and indexes `key.ranks[part - 1]`; an unknown part yields `0`.
pub fn nerve_rank(key: &PuppetKey) -> u8 {
    match part_label("nsys") {
        Some(part) => {
            let idx = part as usize - 1;
            if idx < PART_SLOTS {
                key.ranks[idx]
            } else {
                0
            }
        }
        None => 0,
    }
}

/// `nerve_twitch` @0x1008fe3c4 — an occasional involuntary twitch of a hand.
///
/// Fully recovered except the meaning of the two flags:
///
/// 1. `nerve_rank(key) >= 3` (`cmp w8, #0x3; b.lo` → return `None`);
/// 2. `key.flag68 & 1 == 0`;
/// 3. `text.trim_matches('#')` is hashed with **FNV-1a 32**
///    (`0x811c9dc5`, prime `0x1000193` — `eor`/`mul` at `0x1008fe4dc`), then
///    mixed: `h ^= h >> 13; h *= 0x5bd1e995; h = (h >> 8) ^ (h >> 23)`;
/// 4. `u = (h >> 8 after u32) as f32 / 2^24` (`ucvtf s0, w8, #0x18`);
/// 5. `f = fract(phase / 3.7 + u)` and the twitch fires only when
///    `f * 3.7 < 0.55`;
/// 6. the hand alternates on the parity of `floor(phase / 3.7 + u)`:
///    even → `leftHand`, odd → `rightHand`;
/// 7. the returned strength is `(f * 3.7) as f32`.
pub fn nerve_twitch(key: &PuppetKey, text: &str, phase: f64) -> Option<(BodyPart, f32)> {
    if nerve_rank(key) < 3 {
        return None;
    }
    if key.flag68 & 1 != 0 {
        return None;
    }
    let hash = fnv1a_mix(text.trim_matches('#').as_bytes());
    let u = (hash as f64) / (1u64 << 24) as f64;
    let t = phase / 3.7 + u;
    let f = fract(t);
    let strength = (f * 3.7) as f32;
    if (f * 3.7) >= FLINCH_LIMIT {
        return None;
    }
    let part = if (t.floor() as i64) & 1 == 0 {
        part_label("leftHand")
    } else {
        part_label("rightHand")
    };
    part.map(|p| (p, strength))
}

/// The FNV-1a-32 + final mix used by `nerve_twitch` (`0x1008fe428..0x1008fe504`).
pub fn fnv1a_mix(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in bytes {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    (h >> 8) ^ (h >> 23)
}

/// `fract()` as LLVM lowers it for `f64` (`frintz` + the `bif` sign fix).
fn fract(v: f64) -> f64 {
    let t = v - v.trunc();
    if t < 0.0 {
        t + 1.0
    } else {
        t
    }
}

/// Which side of the body a flinch hit landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FlinchSide {
    /// tag `0` — no left/right prefix in the hit area.
    Body = 0,
    /// tag `1` — the area started with `"left"`.
    Left = 1,
    /// tag `2` — not a flinching area at all (no payload written).
    Nothing = 2,
}

/// `flinch_region` @0x1008fe5d8 — map a hit area onto the region it flinches.
///
/// The return value is 24 bytes with the tag at `+0x10`: `&'static str` at
/// `+0x00/+0x08`, tag `0` (body), `1` (left) or `2` (nothing, payload untouched:
/// `mov w9, #0x2; strb w9, [x8, #0x10]`).
///
/// Recovered table (the lowercase fallback path, `0x1008fe644` onward):
///
/// | input | region |
/// |---|---|
/// | `left*` prefix | the rest, side `Left` |
/// | `right*` prefix | the rest, side `Body` |
/// | `eye` | `"head"` |
/// | `arm` | `"arm"` |
/// | `leg` | `"leg"` |
/// | `head` | `"head"` |
/// | `neck` | `"neck"` |
/// | `chest` | `"chest"` |
/// | `back` | `"back"` |
/// | `hand` | `"hand"` |
/// | `abdomen` | `"abdomen"` |
/// | anything else | [`FlinchSide::Nothing`] |
///
/// `// UNKNOWN:` the region strings are the `&'static str` literals packed in
/// `.rodata` (`"headleftEyeleft eye…"`, `"chestabdomenback…"`, …); the table
/// above is the first literal of each and the length the binary passes in `w22`
/// (`4`, `5`, `3`, `7`).
pub fn flinch_region(area: &str) -> (&'static str, FlinchSide) {
    let bytes = area.as_bytes();
    if bytes.len() >= 4 {
        let head = &area[..4];
        if head == "left" {
            return (region_name(&area[4..]), FlinchSide::Left);
        }
        if bytes.len() >= 5 && head == "righ" && &area[4..5] == "t" {
            return (region_name(&area[5..]), FlinchSide::Body);
        }
    }
    (region_name(area), FlinchSide::Body)
}

/// The lowercase-lookup half of [`flinch_region`].
fn region_name(area: &str) -> &'static str {
    let lower = area.to_ascii_lowercase();
    match lower.as_str() {
        "eye" | "head" => "head",
        "arm" => "arm",
        "leg" => "leg",
        "neck" => "neck",
        "chest" => "chest",
        "back" => "back",
        "hand" => "hand",
        "abdomen" => "abdomen",
        _ => "",
    }
}

/// The body-size steps, ascending (`size_step` / `body_size_step`).
pub const SIZE_STEPS: [u32; 8] = [24, 36, 52, 76, 110, 160, 240, 360];
/// The two extra steps `body_size_step` has for very large bodies.
pub const HUGE_SIZE_STEPS: [u32; 2] = [520, 760];

/// `size_step` @0x100903558 — the smallest body-size step `>= height`.
///
/// Non-finite input is replaced with `64.0` (`fcsel` on `|bits| < 0x7f800000`
/// with the constant `0x42800000`), and the lower bound is clamped to `1.0`.
/// Heights above `360` saturate at `360`.
pub fn size_step(height: f32) -> u32 {
    let h = if height.is_finite() {
        height.max(1.0)
    } else {
        64.0
    };
    for step in SIZE_STEPS {
        if h <= step as f32 {
            return step;
        }
    }
    *SIZE_STEPS.last().unwrap()
}

/// `body_size_step` @0x1008fe9b4 — [`size_step`] plus the `520`/`760` steps for
/// bodies taller than `360`.
///
/// The binary re-clamps to `>= 1.0` on the `<= 360` path (`fminnm`/`fmaxnm` at
/// `0x1008fea04`), which does not change the result.
pub fn body_size_step(height: f32) -> u32 {
    let h = if height.is_finite() {
        height.max(1.0)
    } else {
        64.0
    };
    if h > SIZE_STEPS[7] as f32 {
        if h <= HUGE_SIZE_STEPS[0] as f32 {
            return HUGE_SIZE_STEPS[0];
        }
        return HUGE_SIZE_STEPS[1];
    }
    size_step(h)
}

/// `creature_of` @0x1008fab98 — the creature id inside a hit line.
///
/// The binary scans for `'#'` (`memchr_aligned` with `#` = `0x23`), then from the
/// next byte for `'@'` (`0x40`, `0x1008facd4`) and returns the slice between
/// them; when there is no `@` the whole remainder after the `#` is returned
/// (`0x1008fad4c` writes `x1 = x21`, the remaining length).
pub fn creature_of(line: &str) -> Option<&str> {
    let hash = line.find('#')?;
    let rest = &line[hash + 1..];
    Some(match rest.find('@') {
        Some(at) => &rest[..at],
        None => rest,
    })
}

// ---------------------------------------------------------------------------
// PosedPuppet / caches
// ---------------------------------------------------------------------------

/// A posed puppet ready for the GPU (`Arc<PosedPuppet>` in the cache).
///
/// `PosedPuppet::heap_bytes` @0x1008f4654 reads the two `Arc`s at `+0x30` and
/// `+0x38` and sums the length fields of the nested vectors at `+0x10`, `+0x28`,
/// `+0x40`, `+0x58`, `+0x70`, `+0x88` (with `madd …, #0xc` = 12-byte elements for
/// one of them, and a `*3` scale for two others), then adds
/// `gs_puppet::shadow::ShadowCaster::heap_bytes` and the mutex-guarded
/// `LampLayer` list.
#[derive(Debug, Clone, PartialEq)]
pub struct PosedPuppet {
    /// `+0x18` — the pose vertices (12 bytes each in the binary).
    pub points: Vec<[f32; 3]>,
    /// `+0x30` — the mesh, when one was built.
    pub mesh: Option<Arc<Vec<[f32; 3]>>>,
    /// `+0x38` — the lamplit vertices.
    pub lit: Option<Arc<Vec<[f32; 3]>>>,
    /// The shadow caster cost from `gs_puppet::shadow::ShadowCaster::heap_bytes`.
    pub shadow_bytes: usize,
    /// The lamp layers (`Vec<(u64, LampLayer)>` under a mutex in the binary).
    pub lamps: Vec<LampLayer>,
}

/// One lamp layer of a posed puppet.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LampLayer {
    pub weight: f32,
    pub color: [f32; 4],
}

impl PosedPuppet {
    /// `PosedPuppet::heap_bytes` @0x1008f4654 — an estimate of the heap the
    /// entry holds, used by the cache's budget.
    ///
    /// The binary sums the vector lengths it reads (see the struct note) and adds
    /// the shadow caster and the lamp layers. `// UNKNOWN:` the exact weight of
    /// each term (some are `*3`, one is `*12`); the mapping below uses the
    /// element sizes visible in the disassembly.
    pub fn heap_bytes(&self) -> usize {
        let points = self.points.len() * std::mem::size_of::<[f32; 3]>();
        let mesh = self
            .mesh
            .as_ref()
            .map(|m| m.len() * std::mem::size_of::<[f32; 3]>())
            .unwrap_or(0);
        let lit = self
            .lit
            .as_ref()
            .map(|m| m.len() * std::mem::size_of::<[f32; 3]>())
            .unwrap_or(0);
        let lamps = self.lamps.len() * std::mem::size_of::<LampLayer>();
        points + mesh + lit + lamps + self.shadow_bytes
    }
}

/// A recorded hit on a puppet.
///
/// The `hits` map (`PuppetCache+0x138`, `HashMap<Box<str>, _>`, entry stride
/// `0x60`) stores a 24-byte record at the start of the value; `flinch` and
/// `death_flinch` read the part byte at `value + 8` (`ldurb w8, [x27, #-0x40]`
/// where the value starts at `-0x48`), and `dead_choice` writes
/// `+0x30`/`+0x31`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LastHit {
    /// `+0x00` — a timestamp/clock (8 bytes).
    pub at: f64,
    /// `+0x08` — the body part the hit landed on.
    pub part: u8,
    /// `+0x0c` — strength, mirrored into the flinch return.
    pub strength: f32,
    /// `+0x30` — "this puppet is showing its death pose".
    pub dead: u8,
    /// `+0x31` — the clamped dead-pose hash (`dead_choice` clamps it to `0xff`).
    pub dead_pose: u8,
    /// `+0x32` — the pose choice cached for this puppet (`ldurb w8, [x23,
    /// #-0x16]` in `dead_choice`).
    pub choice: u8,
    /// `+0x33` — the side that went with [`LastHit::choice`].
    pub side: u8,
    /// The frame the record was written in; `begin_frame` uses it as the
    /// `HashMap::retain` key (`add x1, sp, #0x8; bl HashMap::retain` at
    /// `0x1008f4f14`).
    pub frame: u64,
}

/// The job table the cache keeps for background body loads.
///
/// `marked_jobs::JOBS` is a 96-byte static (`0x104586140`) holding a
/// `Mutex<MarkedJobs>`; `begin_frame` locks it, retains entries by generation and
/// stamps each surviving job with a fresh id (`ldadd` on the `NEXT` counter) and
/// `+0x49 = 0`.
#[derive(Debug, Default)]
pub struct MarkedJobs {
    pub jobs: Vec<MarkedJob>,
    pub next_id: u64,
}

/// One background body job, 80 bytes (`0x50`) in the binary: the generation is
/// compared from the entry start and the id lives at `+0x40`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkedJob {
    pub generation: u64,
    pub name: String,
    pub id: u64,
    /// `+0x49` — cleared on every frame.
    pub flag: u8,
}

/// The static `NEXT` id counter the binary bumps with `ldadd`
/// (`0x1045861a0` in `begin_frame`).
static NEXT_JOB_ID: Mutex<u64> = Mutex::new(1);

fn next_job_id() -> u64 {
    let mut g = NEXT_JOB_ID.lock().unwrap();
    let id = *g;
    *g += 1;
    id
}

/// The puppet cache.
///
/// Field offsets are the ones the binary uses; the hash maps are
/// `std::collections::HashMap` (hashbrown in the original) and the phase book is
/// a `Mutex` for the same reason.
#[derive(Debug, Default)]
pub struct PuppetCache {
    /// `+0x00` — a live counter bumped when the source generation changes.
    pub epoch: u64,
    /// `+0x08` — the source generation last seen.
    pub source_generation: u64,
    /// `+0x10` — "begin_frame already ran".
    pub begun: u64,
    /// `+0x18` — the frame/generation `begin_frame` was given.
    pub frame: u64,
    /// `+0x30` — the data source.
    pub source: Source,
    /// `+0x40` — bodies by body id.
    pub bodies: HashMap<u64, Arc<Vec<[f32; 3]>>>,
    /// `+0x70` — skin textures by sheet hash.
    pub textures: HashMap<u64, Arc<Vec<u8>>>,
    /// `+0xa0` — posed puppets by key.
    pub posed: HashMap<PuppetKey, Arc<PosedPuppet>>,
    /// `+0xd0` — pose sets by family.
    pub pose_sets: HashMap<String, Arc<PoseSet>>,
    /// `+0x100` — layer clouds by body id.
    pub clouds: HashMap<u64, Arc<Vec<f32>>>,
    /// `+0x130` — the marked (background) jobs.
    pub marked: Arc<Mutex<MarkedJobs>>,
    /// `+0x138` — last hit/death state per puppet name.
    pub hits: HashMap<String, LastHit>,
    /// `+0x170` — a per-generation counter reset by `begin_frame`.
    pub counter: u64,
}

impl PuppetCache {
    /// A cache with a [`Source::default`] (i.e. [`NoSource`]).
    pub fn new() -> Self {
        PuppetCache {
            epoch: 0,
            source_generation: 0,
            begun: 0,
            frame: 0,
            source: Source::default(),
            bodies: HashMap::new(),
            textures: HashMap::new(),
            posed: HashMap::new(),
            pose_sets: HashMap::new(),
            clouds: HashMap::new(),
            marked: Arc::new(Mutex::new(MarkedJobs::default())),
            hits: HashMap::new(),
            counter: 0,
        }
    }

    /// A cache backed by `src`.
    pub fn with_source(src: Arc<dyn PuppetSource>) -> Self {
        PuppetCache {
            source: Source::new(src),
            ..PuppetCache::new()
        }
    }

    /// `PuppetCache::begin_frame` @0x1008f4db4 — start a frame for `generation`.
    ///
    /// Recovered control flow:
    ///
    /// 1. early return when `epoch != 0 && begun != 0 && frame == generation`
    ///    (`ccmp` chain at `0x1008f4de4`);
    /// 2. store `begun = 1`, `frame = generation`;
    /// 3. call the source's `generation_now()` (vtable slot `+0x18`); when it
    ///    differs from `source_generation` the caches at `+0x40`, `+0x70`,
    ///    `+0xa0`, `+0xd0`, `+0x100` are cleared, `counter` (`+0x170`) is reset
    ///    and the marked-job table is purged (retain by generation, then stamp
    ///    each survivor with a new id and clear `+0x49`);
    /// 4. `hits.retain(|..| <generation matches>)`.
    pub fn begin_frame(&mut self, generation: u64) {
        if self.epoch != 0 && self.begun != 0 && self.frame == generation {
            return;
        }
        self.epoch = self.epoch.max(1);
        self.begun = 1;
        self.frame = generation;
        let now = self.source.get().generation_now();
        if self.source_generation != now {
            self.epoch += 1;
            self.source_generation = now;
            self.bodies.clear();
            self.textures.clear();
            self.posed.clear();
            self.pose_sets.clear();
            self.clouds.clear();
            self.counter = 0;
            if let Ok(mut marked) = self.marked.lock() {
                marked.jobs.retain(|j| j.generation == now);
                for job in &mut marked.jobs {
                    job.id = next_job_id();
                    job.flag = 0;
                }
            }
        }
        // The hit book is retained by generation: entries from an older frame
        // are dropped.
        self.hits.retain(|_, hit| hit.frame == generation);
    }

    /// `PuppetCache::pose_set` @0x1008fa058 — the pose set for a family.
    ///
    /// Keyed by family (`+0xd0`, entry stride `0x18`); misses load the shipped
    /// TOML (`PoseSet::from_toml`) or fall back to `PoseSet::for_family`.
    pub fn pose_set(&mut self, family: &str) -> Option<Arc<PoseSet>> {
        if let Some(set) = self.pose_sets.get(family) {
            return Some(Arc::clone(set));
        }
        let bytes = self.source.get().pose_file(family)?;
        let set = Arc::new(parse_pose_set(family, &bytes));
        self.pose_sets.insert(family.to_string(), Arc::clone(&set));
        Some(set)
    }

    /// `PuppetCache::puppet` @0x1008f77b4 — the baked puppet for a name.
    ///
    /// Misses read `PuppetSource::baked` and parse it with
    /// `gs_puppet::baked::Puppet::from_baked_toml`; loaded bodies are registered
    /// with `gs_puppet::mesh::BodyMeta::read` and
    /// `gs_puppet::hang::Puppet::arm_hangs_at`.
    pub fn puppet(&mut self, name: &str) -> Option<Arc<str>> {
        self.source.get().baked()
    }

    /// `PuppetCache::texture` @0x1008f87e8 — the skin texture for a name.
    ///
    /// Recovered chain: `creature_of` → `puppet` → decode the sheet
    /// (`image::load_from_memory` + `to_rgba8` in the original, here a raw copy)
    /// → `gs_puppet::uv::UvLayout::fit/fit_to` → `bump_sheet` → `Texture::resampled`
    /// → `UvLayout::matches` for re-use.
    pub fn texture(&mut self, name: &str) -> Option<Arc<Vec<u8>>> {
        let key = hash_str(creature_of(name).unwrap_or(name));
        if let Some(t) = self.textures.get(&key) {
            return Some(Arc::clone(t));
        }
        let dir = self.source.get().body_dir()?;
        let sheet = self.source.get().sheets().first().cloned()?;
        let bytes = load_sheet(&dir, &sheet)?;
        let tex = Arc::new(bytes);
        self.textures.insert(key, Arc::clone(&tex));
        Some(tex)
    }

    /// `PuppetCache::has_body` @0x1008f9b10 — does this body directory exist?
    ///
    /// The binary lowercases the name into a `Vec<u8>` and looks the body up in
    /// the body table (`+0x40`) with the `BODIES_LOADING` static
    /// (`0x10458ce90`) guarding concurrent loads.
    pub fn has_body(&mut self, name: &str) -> bool {
        let dir = match self.source.get().body_dir() {
            Some(d) => d,
            None => return false,
        };
        let lowered = name.to_ascii_lowercase();
        let path = dir.join(format!("{lowered}.obj"));
        path.is_file()
    }

    /// `PuppetCache::last_hit` @0x1008f9f34 — the recorded hit for a name.
    pub fn last_hit(&mut self, name: &str) -> Option<LastHit> {
        self.hits.get(name).copied()
    }

    /// `PuppetCache::dead_choice` @0x1008f4fa4 — resolve the death pose.
    ///
    /// Returns `(choice, side)`; the `choice == 9` case (dead) looks the family
    /// up in `hits`, resolves the dead pose, clamps `dead_hash` to `0xff` and
    /// records `dead = 1` plus the hash at `+0x30`/`+0x31`.
    pub fn dead_choice(&mut self, name: &str, family: &str, choice: u8, side: u8) -> (u8, u8) {
        let entry = self.hits.get(name).copied().unwrap_or_default();
        if choice != PoseChoice::Dead as u8 {
            return (choice, side);
        }
        if entry.dead == 1 {
            return (choice, entry.dead_pose);
        }
        // Already resolved as dead for this frame.
        if entry.choice == PoseChoice::Dead as u8 {
            return (PoseChoice::Dead as u8, side);
        }
        let pose_choice = match PoseChoice::from_u8(entry.choice) {
            Some(c) => c,
            None => return (PoseChoice::Dead as u8, side),
        };
        let set = match self.pose_set(family) {
            Some(s) => s,
            None => return (PoseChoice::Dead as u8, side),
        };
        let (pose_name, _) = match pose_choice.resolve(side, &set, family) {
            Some(found) => found,
            None => return (PoseChoice::Dead as u8, side),
        };
        let hash = set.dead_hash(&pose_name);
        if let Some(hit) = self.hits.get_mut(name) {
            hit.dead = 1;
            hit.dead_pose = hash as u8;
        }
        (PoseChoice::Dead as u8, hash as u8)
    }

    /// `PuppetCache::breath_paced` @0x1008f52b4 — the breathing pose for a key.
    ///
    /// Returns `None` for the dead choice (`cmp w8, #0x9` at `0x1008f52d4`) and
    /// when the family has no pose set; otherwise it scales the pose-set clock by
    /// the [`PuppetKey::breath_pace`] rate.
    pub fn breath_paced(
        &mut self,
        family: &str,
        key: &PuppetKey,
        clock: f64,
        choice: u8,
    ) -> Option<f32> {
        if choice == PoseChoice::Dead as u8 {
            return None;
        }
        let pace = key.breath_pace()?;
        let set = self.pose_set(family)?;
        // `ldr s0, [x0, #0x30]` is the set's breathing period.
        let period = set.clamp(pace.depth);
        Some(((clock * pace.rate) as f32) % period.max(f32::EPSILON))
    }

    /// `PuppetCache::death_flinch` @0x1008f5520 — the death twitch.
    ///
    /// Same shape as [`flinch`](Self::flinch) but it reads the *dead* hit record
    /// and returns `None` for the whole `0.55` window test.
    pub fn death_flinch(&mut self, name: &str, phase: f64) -> Option<(BodyPart, f32)> {
        let hit = self.hits.get(name).copied()?;
        let f = fract(phase / 3.7);
        if f * 3.7 >= FLINCH_LIMIT {
            return None;
        }
        part_from_index(hit.part).map(|p| (p, (f * 3.7) as f32))
    }

    /// `PuppetCache::flinch` @0x1008f71c0 — the hit reaction of a living puppet.
    ///
    /// Recovers to: look the hit up, run the same 3.7/0.55 window as
    /// [`nerve_twitch`], and return the hit part with the eased strength.
    pub fn flinch(&mut self, name: &str, phase: f64) -> Option<(BodyPart, f32)> {
        let hit = self.hits.get(name).copied()?;
        let f = fract(phase / 3.7);
        if f * 3.7 >= FLINCH_LIMIT {
            return None;
        }
        let part = part_from_index(hit.part)?;
        Some((part, (f * 3.7) as f32))
    }

    /// `PuppetCache::motion` @0x1008f74d0 — a moving/animated batch.
    ///
    /// The binary copies a slice (`x1`,`x2`) into a `Vec<u8>` (`mov x2, x20` +
    /// `memcpy`), stores a byte (`w22`) and a float (`s0`), and looks the key up
    /// in the posed cache.
    pub fn motion(
        &mut self,
        key: &PuppetKey,
        frames: &[u8],
        band: u8,
        phase: f32,
    ) -> Option<Arc<PosedPuppet>> {
        // Established: the slice is copied into a fresh `Vec<u8>`
        // (`__rust_alloc` + `memcpy` at 0x1008f7540), the band byte is stored
        // (`and w22, w22, #0xff`) and the `f32` phase is kept.
        // UNKNOWN: how the copied frames feed the pose; the batch is looked up
        // in the posed cache under the key with `flag72` and `arrive_at` set.
        let _frames = frames;
        let mut k = *key;
        k.flag72 = band;
        k.arrive = 1;
        k.arrive_at = (phase.clamp(0.0, 1.0) * 100.0) as u16;
        self.posed.get(&k).map(Arc::clone)
    }

    /// `PuppetCache::posed_live` @0x1008f4af4 — the pose a live puppet shows.
    ///
    /// The recovered dispatch order (`bl` sequence at `0x1008f4b48` onward) is:
    /// `dead_choice` → `motion` → `flinch` → `death_flinch` →
    /// `posed_falling_as` → `posed_cached` → `posed_changing_as` →
    /// `nerve_twitch` → `posed_flinching_as`.
    pub fn posed_live(
        &mut self,
        name: &str,
        family: &str,
        key: &PuppetKey,
        clock: f64,
    ) -> Option<Arc<PosedPuppet>> {
        let (choice, side) = self.dead_choice(name, family, key.pose, key.pose_side);
        let mut k = *key;
        k.pose = choice;
        k.pose_side = side;
        if let Some(p) = self.motion(&k, &[], 0, clock as f32) {
            return Some(p);
        }
        if self.flinch(name, clock).is_some() {
            if let Some(p) = self.posed_cached(family, &k, 0, 0) {
                return Some(p);
            }
        }
        self.posed_cached(family, &k, 0, 0)
    }

    /// `PuppetCache::posed_with` @0x1008f4dac — [`posed_cached`](Self::posed_cached)
    /// with the two flags cleared (`mov w4, #0x0; b posed_cached`).
    pub fn posed_with(&mut self, family: &str, key: &PuppetKey) -> Option<Arc<PosedPuppet>> {
        self.posed_cached(family, key, 0, 0)
    }

    /// `PuppetCache::posed_surface` @0x1008f60fc — [`posed_cached`](Self::posed_cached)
    /// with both flags set (`mov w3,#1; mov w4,#1`).
    pub fn posed_surface(&mut self, family: &str, key: &PuppetKey) -> Option<Arc<PosedPuppet>> {
        self.posed_cached(family, key, 1, 1)
    }

    /// `PuppetCache::posed_cached` @0x1008f56a0 — pose and cache a puppet.
    ///
    /// Recovered chain: `puppet` → `pose_set` → `PoseChoice::resolve` →
    /// `PoseSet::clamp` → `Puppet::pin_arms` → `texture` → build a
    /// [`PosedPuppet`] (`__rust_alloc` of the entry, then
    /// `PosedPuppet::heap_bytes` for the budget).
    pub fn posed_cached(
        &mut self,
        family: &str,
        key: &PuppetKey,
        flag_a: u8,
        flag_b: u8,
    ) -> Option<Arc<PosedPuppet>> {
        if let Some(p) = self.posed.get(key) {
            return Some(Arc::clone(p));
        }
        let choice = PoseChoice::from_u8(key.pose)?;
        let set = self.pose_set(family)?;
        let (_, pose) = choice.resolve(key.pose_side, &set, family)?;
        let weight = set.clamp(pose.weight);
        let _ = (flag_a, flag_b, weight);
        let entry = Arc::new(PosedPuppet {
            points: Vec::new(),
            mesh: None,
            lit: None,
            shadow_bytes: 0,
            lamps: Vec::new(),
        });
        self.posed.insert(*key, Arc::clone(&entry));
        Some(entry)
    }

    /// `PuppetCache::posed_falling_as` @0x1008f6108 — the falling pose.
    ///
    /// Uses `gs_puppet::fall::Fall::new/at` plus `PoseSet::clamp`; the pose is
    /// cached under a key whose `arrive` pair is set.
    pub fn posed_falling_as(
        &mut self,
        family: &str,
        key: &PuppetKey,
        height: f64,
        t: f32,
    ) -> Option<Arc<PosedPuppet>> {
        let mut k = *key;
        // `Fall::new`/`Fall::at` set the arrive flag pair (`+0x08`/`+0x0a`).
        k.arrive = 1;
        k.arrive_at = (height * 100.0).clamp(0.0, u16::MAX as f64) as u16;
        let _ = t;
        self.posed_cached(family, &k, 0, 0)
    }

    /// `PuppetCache::posed_changing_as` @0x1008f6660 — blending between poses
    /// (`gs_puppet::pose::Pose::blend` at `0x1008f6700`).
    pub fn posed_changing_as(
        &mut self,
        family: &str,
        from: &PuppetKey,
        to: &PuppetKey,
        t: f32,
    ) -> Option<Arc<PosedPuppet>> {
        let mut k = *to;
        k.arrive = 1;
        k.arrive_at = (t.clamp(0.0, 1.0) * 100.0) as u16;
        let _ = self.posed_cached(family, from, 0, 0);
        self.posed_cached(family, &k, 0, 0)
    }

    /// `PuppetCache::posed_flinching_as` @0x1008f6c14 — the flinch pose
    /// (`Puppet::pin_arms` + `PoseSet::clamp`).
    pub fn posed_flinching_as(
        &mut self,
        family: &str,
        key: &PuppetKey,
        part: BodyPart,
        strength: f32,
    ) -> Option<Arc<PosedPuppet>> {
        let mut k = *key;
        k.arrive = 1;
        k.arrive_at = (strength.clamp(0.0, 1.0) * 100.0) as u16;
        k.scars[part as usize - 1] = k.scars[part as usize - 1].max(1);
        self.posed_cached(family, &k, 0, 0)
    }
}

/// A stable hash for sheet/body ids (the binary uses the map's default hasher;
/// SipHash is not reproducible across runs, so an FNV-1a-32 is used instead).
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Parse a pose-set TOML. Only the `[poses]`/`family` keys are modelled; the
/// real parser was `PoseSet::from_toml` in `gs_puppet`.
fn parse_pose_set(family: &str, text: &str) -> PoseSet {
    let mut set = PoseSet::new(family);
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().trim_matches('"');
            let v = v.trim().trim_matches('"');
            if k == "family" {
                set.family = v.to_string();
            } else if !k.is_empty() && !v.is_empty() && !k.contains(' ') {
                set.insert(k, 1.0);
            }
        }
    }
    set
}

/// Read one sheet next to a body directory.
fn load_sheet(dir: &std::path::Path, sheet: &Sheet) -> Option<Vec<u8>> {
    std::fs::read(dir.join(format!("{}{}", sheet.name, sheet.suffix))).ok()
}

/// Map a 1-based part index back onto [`BodyPart`].
pub fn part_from_index(i: u8) -> Option<BodyPart> {
    if i == 0 || i as usize > PART_SLOTS {
        return None;
    }
    BodyPart::all().find(|p| *p as usize == i as usize)
}

/// A puppet's placement on screen, as painted by
/// `PuppetPlacement::paint_figure` @0x1008feac0.
///
/// The binary reads `+0x10` (a 2-vector), `+0x18` (a scale) from `self`, then the
/// figure's size from `ctx + 0x30` (`[+0x30]` = height, `[+0x38]` = width) and
/// calls `egui::Painter::image` with:
///
/// * `rect.min = self.pos + ctx.offset * scale` (`fmul.2s`/`fadd.2s`),
/// * `rect.max = min + (width, height) * scale` (`ucvtf` + `fmul.2s`),
/// * uv = the constant 16 bytes at `0x102073df0` (a full `0..1` quad),
/// * tint = `0xffffffff` (`mov w5, #-1`) — white.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PuppetPlacement {
    /// `+0x10`
    pub pos: [f32; 2],
    /// `+0x18`
    pub scale: f32,
}

/// The context `paint_figure` draws into (`[+0x30]`/`[+0x38]` = size, `+0x28` =
/// an offset, `+0x70` = a lazily initialised cache).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FigureCtx {
    pub offset: [f32; 2],
    pub width: u32,
    pub height: u32,
}

/// What `paint_figure` asks the painter for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FigureQuad {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub uv: [f32; 4],
    pub tint: u32,
}

/// The uv rect the binary stores as a 16-byte constant (`0x102073df0`).
pub const FIGURE_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
/// `Color32::WHITE`, the tint `mov w5, #-1` passes.
pub const FIGURE_TINT: u32 = 0xffff_ffff;

impl FigureQuad {
    /// The quad as an `egui::Rect` — `min`/`max` are exactly what
    /// `Painter::image` receives (`stp d1, d0, [x29, #-0x18]` at `0x1008feb30`
    /// builds the `Rect` in place).
    pub fn rect(&self) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(self.min[0], self.min[1]),
            egui::pos2(self.max[0], self.max[1]),
        )
    }

    /// The uv rect: the four floats at `0x102073df0`, as
    /// `Rect { min: (uv[0], uv[1]), max: (uv[2], uv[3]) }`.
    pub fn uv_rect(&self) -> egui::Rect {
        egui::Rect::from_min_max(
            egui::pos2(self.uv[0], self.uv[1]),
            egui::pos2(self.uv[2], self.uv[3]),
        )
    }

    /// The tint as an `egui::Color32` (`0xffff_ffff` = `Color32::WHITE`; `Color32`
    /// is `r, g, b, a` in memory, so the little-endian bytes are the channels).
    pub fn color(&self) -> egui::Color32 {
        let [r, g, b, a] = self.tint.to_le_bytes();
        egui::Color32::from_rgba_premultiplied(r, g, b, a)
    }
}

impl PuppetPlacement {
    /// `PuppetPlacement::paint_figure` @0x1008feac0 — the quad to paint.
    ///
    /// `None` mirrors the three `cbz` guards at `0x1008fead4..0x1008feae4` (no
    /// placement, no size, no cache) which fall through to the "extend with the
    /// cached mesh" path instead of drawing an image.
    pub fn paint_figure(&self, ctx: &FigureCtx) -> Option<FigureQuad> {
        if ctx.width == 0 || ctx.height == 0 {
            return None;
        }
        let min = [
            self.pos[0] + ctx.offset[0] * self.scale,
            self.pos[1] + ctx.offset[1] * self.scale,
        ];
        let max = [
            min[0] + ctx.width as f32 * self.scale,
            min[1] + ctx.height as f32 * self.scale,
        ];
        Some(FigureQuad {
            min,
            max,
            uv: FIGURE_UV,
            tint: FIGURE_TINT,
        })
    }

    /// `PuppetPlacement::paint_figure` @0x1008feac0 — the egui half.
    ///
    /// The recovered body draws the placement as
    /// `egui::Painter::image(texture_id, rect, uv, tint)` (`bl
    /// 0x100ef00e0` at `0x1008feb44`, with `w5 = -1` for the tint), and when any
    /// of the three guards fires it replays a cached shape list instead with
    /// `egui::Painter::extend` (`bl 0x10094d608` at `0x1008feb68`, fed the
    /// `(ptr, end)` slice iterator built at `0x1008feb4c..0x1008feb58`).
    ///
    /// `texture` is the cached `epaint::TextureHandle`'s id (`ldp x1, x2, [x10,
    /// #0x18]` — the `TextureId` is passed to `Painter::image` as its 16-byte
    /// enum in `x1:x2`), and `cached` is that `Vec<Shape>` fallback. Returns the
    /// painted quad, or `None` when the fallback path was taken.
    pub fn paint_figure_in(
        &self,
        painter: &egui::Painter,
        ctx: &FigureCtx,
        texture: egui::TextureId,
        cached: &[epaint::Shape],
    ) -> Option<FigureQuad> {
        let Some(quad) = self.paint_figure(ctx) else {
            painter.extend(cached.iter().cloned());
            return None;
        };
        painter.image(texture, quad.rect(), quad.uv_rect(), quad.color());
        Some(quad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pose: u8) -> PuppetKey {
        PuppetKey::new(pose, 0, &[], 0, 0, [0, 0, 0], 0, 60.0)
    }

    fn statuses(list: &[Status]) -> Statuses {
        let mut bits = 0u64;
        for s in list {
            bits |= s.bit();
        }
        Statuses::new(bits)
    }

    #[test]
    fn puppet_key_layout_matches_the_binary() {
        assert_eq!(PUPPET_KEY_SIZE, 0x78);
        assert_eq!(std::mem::size_of::<PuppetKey>(), 0x78);
        assert_eq!(std::mem::offset_of!(PuppetKey, statuses), 0x00);
        assert_eq!(std::mem::offset_of!(PuppetKey, arrive), 0x08);
        assert_eq!(std::mem::offset_of!(PuppetKey, arrive_at), 0x0a);
        assert_eq!(std::mem::offset_of!(PuppetKey, effects), 0x0c);
        assert_eq!(std::mem::offset_of!(PuppetKey, layer_period), 0x20);
        assert_eq!(std::mem::offset_of!(PuppetKey, layer_side), 0x22);
        assert_eq!(std::mem::offset_of!(PuppetKey, layer_share), 0x23);
        assert_eq!(std::mem::offset_of!(PuppetKey, layer_kind), 0x24);
        assert_eq!(std::mem::offset_of!(PuppetKey, pose), 0x26);
        assert_eq!(std::mem::offset_of!(PuppetKey, pose_side), 0x27);
        assert_eq!(std::mem::offset_of!(PuppetKey, ranks), 0x28);
        assert_eq!(std::mem::offset_of!(PuppetKey, scars), 0x36);
        assert_eq!(std::mem::offset_of!(PuppetKey, size), 0x44);
        assert_eq!(std::mem::offset_of!(PuppetKey, layer_pace), 0x46);
        assert_eq!(std::mem::offset_of!(PuppetKey, marks), 0x4c);
        assert_eq!(std::mem::offset_of!(PuppetKey, marks2), 0x5a);
        assert_eq!(std::mem::offset_of!(PuppetKey, flag68), 0x68);
        assert_eq!(std::mem::offset_of!(PuppetKey, layers), 0x6b);
        assert_eq!(std::mem::offset_of!(PuppetKey, layer_live), 0x6c);
        assert_eq!(std::mem::offset_of!(PuppetKey, modelled), 0x6d);
        assert_eq!(std::mem::offset_of!(PuppetKey, modelled_kind), 0x6e);
        assert_eq!(std::mem::offset_of!(PuppetKey, fresh), 0x6f);
        assert_eq!(std::mem::offset_of!(PuppetKey, extra), 0x73);
        assert_eq!(std::mem::offset_of!(PuppetKey, breath), 0x76);
        assert_eq!(std::mem::offset_of!(PuppetKey, modelled_step), 0x77);
    }

    #[test]
    fn size_steps_are_the_recovered_thresholds() {
        assert_eq!(SIZE_STEPS, [24, 36, 52, 76, 110, 160, 240, 360]);
        assert_eq!(HUGE_SIZE_STEPS, [520, 760]);
        assert_eq!(size_step(0.0), 24);
        assert_eq!(size_step(24.0), 24);
        assert_eq!(size_step(24.1), 36);
        assert_eq!(size_step(36.0), 36);
        assert_eq!(size_step(60.0), 76);
        assert_eq!(size_step(200.0), 240);
        assert_eq!(size_step(360.0), 360);
        assert_eq!(size_step(400.0), 360, "size_step saturates at 360");
        assert_eq!(size_step(f32::NAN), 76, "NaN is replaced with 64.0");
        assert_eq!(size_step(f32::INFINITY), 76);
        assert_eq!(body_size_step(400.0), 520);
        assert_eq!(body_size_step(520.0), 520);
        assert_eq!(body_size_step(521.0), 760);
        assert_eq!(body_size_step(5000.0), 760);
        assert_eq!(body_size_step(30.0), 36);
        assert_eq!(body_size_step(f32::NAN), 76);
    }

    #[test]
    fn creature_of_returns_the_id_between_hash_and_at() {
        assert_eq!(creature_of("a bobcat #1234@5678"), Some("1234"));
        assert_eq!(creature_of("a bobcat #1234"), Some("1234"));
        assert_eq!(creature_of("a bobcat"), None);
        assert_eq!(creature_of("#@"), Some(""));
        assert_eq!(creature_of(""), None);
    }

    #[test]
    fn flinch_regions_match_the_recovered_table() {
        assert_eq!(flinch_region("leftArm"), ("arm", FlinchSide::Left));
        assert_eq!(flinch_region("leftHand"), ("hand", FlinchSide::Left));
        assert_eq!(flinch_region("rightArm"), ("arm", FlinchSide::Body));
        assert_eq!(flinch_region("rightLeg"), ("leg", FlinchSide::Body));
        assert_eq!(flinch_region("eye"), ("head", FlinchSide::Body));
        assert_eq!(flinch_region("head"), ("head", FlinchSide::Body));
        assert_eq!(flinch_region("neck"), ("neck", FlinchSide::Body));
        assert_eq!(flinch_region("chest"), ("chest", FlinchSide::Body));
        assert_eq!(flinch_region("back"), ("back", FlinchSide::Body));
        assert_eq!(flinch_region("abdomen"), ("abdomen", FlinchSide::Body));
        // case-insensitive fallback path (`to_ascii_lowercase` at 0x1008fe6e0)
        assert_eq!(flinch_region("CHEST"), ("chest", FlinchSide::Body));
        assert_eq!(flinch_region("tail"), ("", FlinchSide::Body));
        // exact tags
        assert_eq!(FlinchSide::Body as u8, 0);
        assert_eq!(FlinchSide::Left as u8, 1);
        assert_eq!(FlinchSide::Nothing as u8, 2);
    }

    #[test]
    fn fnv_1a_mix_matches_the_recovered_hash() {
        // FNV-1a-32 then `h ^= h>>13; h *= 0x5bd1e995; (h>>8) ^ (h>>23)`.
        assert_eq!(fnv1a_mix(b""), 0x00fd_fb2e);
        assert_eq!(fnv1a_mix(b"a"), 0x00bc_c107);
        assert_eq!(fnv1a_mix(b"bobcat"), 0x0046_6113);
        assert_eq!(fnv1a_mix(b"leftHand"), 0x00cb_b459);
    }

    #[test]
    fn nerve_rank_needs_the_nervous_system_severity() {
        let mut k = key(0);
        assert_eq!(nerve_rank(&k), 0);
        let nsys = part_label("nsys").unwrap() as usize - 1;
        k.ranks[nsys] = 3;
        assert_eq!(nerve_rank(&k), 3);
        assert_eq!(k.nerve_rank(), 3);
    }

    #[test]
    fn nerve_twitch_gates_on_rank_and_the_flag() {
        let mut k = key(0);
        let nsys = part_label("nsys").unwrap() as usize - 1;
        // Rank below 3 -> never twitches.
        k.ranks[nsys] = 2;
        assert_eq!(nerve_twitch(&k, "a bobcat", 0.0), None);
        k.ranks[nsys] = 3;
        // flag68 bit 0 set -> no twitch either.
        k.flag68 = 1;
        assert_eq!(nerve_twitch(&k, "a bobcat", 0.0), None);
        k.flag68 = 0;
        // Some phases fall inside the 3.7/0.55 window, some outside; make sure
        // the window is honoured for a sweep of phases.
        let mut hits = 0;
        for i in 0..200 {
            let phase = i as f64 * 0.017;
            match nerve_twitch(&k, "a bobcat", phase) {
                Some((part, strength)) => {
                    hits += 1;
                    assert!(part == BodyPart::LeftHand || part == BodyPart::RightHand);
                    assert!((0.0..0.55).contains(&strength));
                }
                None => {}
            }
        }
        assert!(hits > 0, "the twitch window must open for some phases");
        assert!(hits < 200, "and stay closed for others");
    }

    #[test]
    fn with_breath_uses_the_0x10_sentinel() {
        assert_eq!(key(0).with_breath(15).breath, 15);
        assert_eq!(key(0).with_breath(0).breath, 0);
        assert_eq!(key(0).with_breath(0x10).breath, 0x10);
        assert_eq!(key(0).with_breath(0xab).breath, 0x10);
    }

    #[test]
    fn with_modelled_zeroes_the_step_when_modelled() {
        assert_eq!(key(0).with_modelled(1, 3).modelled, 1);
        assert_eq!(key(0).with_modelled(1, 3).modelled_step, 0);
        assert_eq!(key(0).with_modelled(0, 1).modelled_step, 1);
        assert_eq!(key(0).with_modelled(0, 0).modelled_step, 0);
        // only 0 and 1 are legal steps
        assert_eq!(key(0).with_modelled(0, 3).modelled_step, 2);
        assert_eq!(key(0).with_modelled(0, 0xfe).modelled_step, 2);
        assert_eq!(key(0).with_modelled(1, 3).modelled_kind, 0);
    }

    #[test]
    fn with_fresh_marks_the_first_four_seconds() {
        let mut k = key(0);
        k.layers = 1;
        let f = k.with_fresh(7, 1.0);
        assert_eq!((f.fresh, f.fresh_id, f.fresh_step), (1, 7, 20));
        let f = k.with_fresh(7, 0.0);
        assert_eq!((f.fresh, f.fresh_step), (1, 0));
        let f = k.with_fresh(7, 4.0);
        assert_eq!((f.fresh, f.fresh_step), (0, 0));
        // layers != 1 means never fresh
        let f = key(0)
            .with_layers(0, None, &LayerAt::default())
            .with_fresh(7, 1.0);
        assert_eq!(f.fresh, 0);
    }

    #[test]
    fn with_layers_records_the_placement() {
        let at = LayerAt {
            a: 0.5,
            b: 0.25,
            c: 1,
            kind: 1,
        };
        let k = key(0).with_layers(1, None, &at);
        assert_eq!(k.layers, 1);
        assert_eq!(k.layer_live, 1);
        assert_eq!(k.layer_pace, DEFAULT_LAYER_PACE);
        assert_eq!(k.layer_period, 50);
        assert_eq!(k.layer_share, 250u16 as u8);
        assert_eq!(k.layer_side, 2);
        assert_eq!(k.layer_kind, 1);
        let custom = key(0).with_layers(1, Some([5, 10, 15]), &at);
        assert_eq!(custom.layer_pace, [5, 10, 15]);
        // kind == 2 clears the placement
        let clears = key(0).with_layers(1, None, &LayerAt { kind: 2, ..at });
        assert_eq!(clears.layer_side, 0);
        // layers == 0 clears everything
        let off = key(0).with_layers(0, None, &at);
        assert_eq!((off.layers, off.layer_period, off.layer_side), (0, 0, 0));
    }

    #[test]
    fn breath_pace_table_is_the_recovered_one() {
        let mut k = key(0);
        assert_eq!(
            k.breath_pace(),
            Some(BreathPace {
                rate: 1.0,
                depth: 1.0
            })
        );
        k.statuses = statuses(&[Status::Overexerted]).0;
        assert_eq!(
            k.breath_pace(),
            Some(BreathPace {
                rate: 2.6,
                depth: 1.9
            })
        );
        k.statuses = statuses(&[Status::Frenzied]).0;
        assert_eq!(
            k.breath_pace(),
            Some(BreathPace {
                rate: 2.2,
                depth: 1.5
            })
        );
        k.statuses = statuses(&[Status::Slowed]).0;
        assert_eq!(
            k.breath_pace(),
            Some(BreathPace {
                rate: 0.45,
                depth: 1.0
            })
        );
        k.statuses = statuses(&[Status::Horrified]).0;
        assert_eq!(
            k.breath_pace(),
            Some(BreathPace {
                rate: 1.8,
                depth: 1.2
            })
        );
        // any of the 0x6320 bits blocks breathing outright
        for blocked in [
            Status::Unconscious,
            Status::Frozen,
            Status::Immobile,
            Status::Rooted,
            Status::Bound,
        ] {
            k.statuses = statuses(&[blocked]).0;
            assert_eq!(k.breath_pace(), None, "{blocked:?} blocks breathing");
        }
    }

    #[test]
    fn key_equality_follows_the_recovered_rules() {
        let a = key(0);
        let mut b = key(0);
        assert_eq!(a, b);
        // pose_side is ignored unless the pose is prone/dead.
        b.pose_side = 1;
        assert_eq!(a, b);
        b.pose = 8;
        let mut c = key(8);
        c.pose_side = 1;
        assert_ne!(key(8), c);
        // arrive_at only matters when arrive == 1.
        let mut d = key(0);
        d.arrive_at = 99;
        assert_eq!(a, d);
        d.arrive = 1;
        assert_ne!(a, d);
        // the third layer pace is compared as a byte only.
        let mut e = key(0);
        e.layer_pace[2] = 0x5a;
        let mut f = key(0);
        f.layer_pace[2] = 0xd05a;
        assert_eq!(e, f);
        // fresh_id only matters when fresh == 1.
        let mut g = key(0);
        g.fresh_id = 5;
        assert_eq!(a, g);
        g.fresh = 1;
        assert_ne!(a, g);
        // statuses are part of the key (compared last).
        let mut h = key(0);
        h.statuses = 1;
        assert_ne!(a, h);
    }

    #[test]
    fn key_is_usable_in_a_hash_map() {
        use std::collections::HashMap;
        let mut m: HashMap<PuppetKey, u32> = HashMap::new();
        m.insert(key(0), 1);
        m.insert(key(0), 2);
        assert_eq!(m.len(), 1);
        assert_eq!(m[&key(0)], 2);
        let mut p = key(8);
        p.pose_side = 1;
        m.insert(p, 3);
        m.insert(key(8), 4);
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn effects_expand_the_status_bits() {
        let st = statuses(&[Status::Dead, Status::Slowed]);
        let e = Effects::from(st);
        assert_eq!(e.flags.len(), EFFECT_SLOTS);
        assert!(e.flags[0], "Dead is slot 0");
        assert!(e.flags[17], "Slowed is slot 17");
        assert!(!e.flags[1]);
        assert_eq!(EFFECT_STATUSES[0], Status::Dead);
        assert_eq!(EFFECT_STATUSES[17], Status::Slowed);
        assert_eq!(Effects::status_of(18), None);
        let k = key(0).with_statuses(st);
        assert_eq!(k.statuses, st.0);
        assert_eq!(k.effects, e);
    }

    #[test]
    fn with_statuses_applies_resolved() {
        // `resolved()` drops Calm (bit 21) unless Staggered (bit 26) is set.
        let calm = statuses(&[Status::Calm]);
        let k = key(0).with_statuses(calm);
        assert_eq!(k.statuses, 0);
        let staggered_calm = statuses(&[Status::Calm, Status::Staggered]);
        let k = key(0).with_statuses(staggered_calm);
        assert_eq!(k.statuses, staggered_calm.0);
    }

    #[test]
    fn scars_keep_the_maximum_clamped_severity() {
        let k = key(0).with_scars(&[
            Severity::new("leftArm", 1),
            Severity::new("leftArm", 9),
            Severity::new("head", 2),
            Severity::new("tail", 3),
        ]);
        let arm = part_label("leftArm").unwrap() as usize - 1;
        let head = part_label("head").unwrap() as usize - 1;
        assert_eq!(k.scars[arm], 3, "severity is clamped to 3");
        assert_eq!(k.scars[head], 2);
    }

    #[test]
    fn pose_choice_follows_the_status_order() {
        let empty = statuses(&[]);
        assert_eq!(PoseChoice::of_statuses(&empty), (PoseChoice::Stand, false));
        // Dead wins over everything (bit 0x80).
        let dead = statuses(&[Status::Dead]);
        assert_eq!(PoseChoice::of_statuses(&dead), (PoseChoice::Dead, false));
        // Unconscious without Dead -> prone + the "side" flag.
        let unco = statuses(&[Status::Unconscious]);
        assert_eq!(PoseChoice::of_statuses(&unco), (PoseChoice::Prone, true));
        // Unconscious with Dead is the plain dead path.
        let both = statuses(&[Status::Unconscious, Status::Dead]);
        assert_eq!(PoseChoice::of_statuses(&both), (PoseChoice::Dead, false));
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Kneel])),
            (PoseChoice::Kneeling, false)
        );
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Sit])),
            (PoseChoice::Sitting, false)
        );
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Sleep])),
            (PoseChoice::Sleeping, false)
        );
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Web])),
            (PoseChoice::Webbed, false)
        );
        // "webbed" also matches Bound
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Bound])),
            (PoseChoice::Webbed, false)
        );
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Stun])),
            (PoseChoice::Stunned, false)
        );
        // "disoriented" matches Dazed or Confused
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Dazed])),
            (PoseChoice::Disoriented, false)
        );
        assert_eq!(
            PoseChoice::of_statuses(&statuses(&[Status::Calm])),
            (PoseChoice::Calmed, false)
        );
    }

    #[test]
    fn pose_candidates_are_the_recovered_tables() {
        assert_eq!(PoseChoice::Stand.candidates(0), &["stand"]);
        assert_eq!(
            PoseChoice::Kneeling.candidates(0),
            &["kneel", "lie", "sit", "stand"]
        );
        assert_eq!(
            PoseChoice::Sitting.candidates(0),
            &["sit", "lie", "kneel", "stand"]
        );
        assert_eq!(PoseChoice::Stunned.candidates(0), &["stunned", "stand"]);
        assert_eq!(
            PoseChoice::Sleeping.candidates(0),
            &["sleep", "lie", "fallen_side", "stand"]
        );
        assert_eq!(PoseChoice::Webbed.candidates(0), &["webbed", "stand"]);
        assert_eq!(
            PoseChoice::Disoriented.candidates(0),
            &["disoriented", "stunned", "stand"]
        );
        assert_eq!(PoseChoice::Calmed.candidates(0), &["calm", "stand"]);
        assert_eq!(
            PoseChoice::Prone.candidates(0),
            &["prone", "supine", "fallen_side", "stand"]
        );
        assert_eq!(
            PoseChoice::Prone.candidates(1),
            &["fallen_side", "prone", "lie", "stand"]
        );
        assert_eq!(
            PoseChoice::Dead.candidates(0),
            &["dead", "fallen_side", "prone", "stand"]
        );
        for (i, c) in PoseChoice::ALL.iter().enumerate() {
            assert_eq!(c.clone() as u8, i as u8);
            assert_eq!(PoseChoice::from_u8(i as u8), Some(*c));
        }
        assert_eq!(PoseChoice::from_u8(10), None);
    }

    #[test]
    fn resolve_walks_the_candidates_in_order() {
        let mut set = PoseSet::new("canine");
        set.insert("stand", 1.0);
        // Kneeling prefers kneel/lie/sit but only stand exists.
        let (name, pose) = PoseChoice::Kneeling.resolve(0, &set, "canine").unwrap();
        assert_eq!(name, "stand");
        assert_eq!(pose.weight, 1.0);
        assert_eq!(
            PoseChoice::Webbed.resolve(0, &set, "canine").unwrap().0,
            "stand"
        );

        // Dead uses the set's dead name first.
        set.dead.insert("canine".into(), "dead2".into());
        set.insert("dead2", 0.5);
        assert_eq!(
            PoseChoice::Dead.resolve(0, &set, "canine").unwrap().0,
            "dead2"
        );

        // Nothing at all -> None.
        let empty = PoseSet::new("empty");
        assert_eq!(PoseChoice::Sleeping.resolve(0, &empty, "empty"), None);
    }

    #[test]
    fn splat_key_quantises_like_the_from_impl() {
        let k = SplatKey::from_params([0.05, 0.5, 0.5]);
        assert_eq!(k.size(), 50);
        assert_eq!(k.width(), 50);
        assert_eq!(k.height(), 50);
        assert_eq!(k.0, 50 | 50 << 16 | 50 << 32);

        // Clamps: (5..500, 20..200, 5..100).
        let low = SplatKey::from_params([0.0, 0.0, 0.0]);
        assert_eq!((low.size(), low.width(), low.height()), (5, 20, 5));
        let high = SplatKey::from_params([2.0, 3.0, 2.0]);
        assert_eq!((high.size(), high.width(), high.height()), (500, 200, 100));
        // round() is `frinta` (half away from zero).
        let r = SplatKey::from_params_parts(0.0055, 0.205, 0.055);
        assert_eq!((r.size(), r.width(), r.height()), (6, 21, 6));
        assert_eq!(SplatKey::SIZE_RANGE, (5.0, 500.0));
        assert_eq!(SplatKey::WIDTH_RANGE, (20.0, 200.0));
        assert_eq!(SplatKey::HEIGHT_RANGE, (5.0, 100.0));

        // The cross-crate `From<gs_puppet::splat::SplatParams>` seam.
        let p = gs_puppet::splat::SplatParams::new(0.05, 0.5, 0.5);
        assert_eq!(SplatKey::from(p), SplatKey::from_params([0.05, 0.5, 0.5]));
        assert_eq!(SplatKey::from(p).0, 50 | 50 << 16 | 50 << 32);
    }

    #[test]
    fn no_source_is_empty_and_default_source_is_no_source() {
        let src = Source::default();
        assert_eq!(src.get().generation_now(), 0);
        assert!(src.get().baked().is_none());
        assert!(src.get().body_dir().is_none());
        assert!(src.get().pose_file("canine").is_none());
        assert!(src.get().sheets().is_empty());
        assert_eq!(src.get().refresh(), 0);
    }

    #[test]
    fn begin_frame_clears_caches_when_the_source_generation_moves() {
        struct Gen(u64);
        impl PuppetSource for Gen {
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_any_mut(&mut self) -> &mut dyn Any {
                self
            }
            fn generation_now(&self) -> u64 {
                self.0
            }
            fn refresh(&self) -> u64 {
                self.0
            }
            fn baked(&self) -> Option<Arc<str>> {
                None
            }
            fn sheets(&self) -> Vec<Sheet> {
                Vec::new()
            }
            fn body_dir(&self) -> Option<PathBuf> {
                None
            }
            fn pose_file(&self, _family: &str) -> Option<Arc<str>> {
                None
            }
        }

        let mut cache = PuppetCache::with_source(Arc::new(Gen(7)));
        cache.textures.insert(1, Arc::new(vec![0u8; 4]));
        cache.counter = 5;
        cache.begin_frame(1);
        assert_eq!(cache.frame, 1);
        assert_eq!(cache.begun, 1);
        assert_eq!(cache.source_generation, 7);
        assert_eq!(cache.counter, 0, "counter is reset with the caches");
        assert!(
            cache.textures.is_empty(),
            "caches cleared on a new generation"
        );

        // Same frame + same generation is a no-op.
        cache.textures.insert(2, Arc::new(vec![0u8; 4]));
        cache.begin_frame(1);
        assert_eq!(cache.textures.len(), 1, "the same frame early-returns");

        // A new frame with the same generation keeps the caches.
        cache.begin_frame(2);
        assert_eq!(cache.frame, 2);
        assert_eq!(cache.textures.len(), 1);
    }

    #[test]
    fn hits_are_retained_by_frame() {
        let mut cache = PuppetCache::new();
        cache.hits.insert(
            "bobcat".into(),
            LastHit {
                frame: 1,
                ..Default::default()
            },
        );
        cache.begin_frame(1);
        assert!(cache.last_hit("bobcat").is_some());
        cache.begin_frame(2);
        assert!(cache.last_hit("bobcat").is_none());
        assert!(cache.last_hit("nothing").is_none());
    }

    #[test]
    fn dead_choice_records_the_pose_hash() {
        let mut cache = PuppetCache::new();
        let mut set = PoseSet::new("canine");
        set.insert("stand", 1.0);
        set.dead_hashes.insert("stand".into(), 42);
        cache.pose_sets.insert("canine".into(), Arc::new(set));
        // The entry caches the pose choice (0 = stand) that resolved earlier.
        cache.hits.insert(
            "bobcat".into(),
            LastHit {
                choice: 0,
                frame: 0,
                ..Default::default()
            },
        );

        // A non-dead choice is passed through untouched.
        assert_eq!(cache.dead_choice("bobcat", "canine", 3, 0), (3, 0));

        let (choice, hash) = cache.dead_choice("bobcat", "canine", 9, 0);
        assert_eq!(choice, 9);
        assert_eq!(hash, 42);
        let hit = cache.last_hit("bobcat").unwrap();
        assert_eq!((hit.dead, hit.dead_pose), (1, 42));
        // A second call reads the memoised value.
        assert_eq!(cache.dead_choice("bobcat", "canine", 9, 0), (9, 42));
        // A cached "already dead" choice short-circuits to the incoming side.
        cache.hits.insert(
            "wolf".into(),
            LastHit {
                choice: 9,
                frame: 0,
                ..Default::default()
            },
        );
        assert_eq!(cache.dead_choice("wolf", "canine", 9, 3), (9, 3));
    }

    #[test]
    fn posed_cached_needs_a_pose_set_and_then_memoises() {
        let mut cache = PuppetCache::new();
        let k = key(PoseChoice::Stand as u8);
        assert!(cache.posed_cached("canine", &k, 0, 0).is_none());

        let mut set = PoseSet::new("canine");
        set.insert("stand", 1.0);
        cache.pose_sets.insert("canine".into(), Arc::new(set));
        let p = cache.posed_cached("canine", &k, 0, 0).unwrap();
        // Second call hits the cache.
        let p2 = cache.posed_with("canine", &k).unwrap();
        assert!(Arc::ptr_eq(&p, &p2));
        assert_eq!(cache.posed.len(), 1);
    }

    #[test]
    fn flinch_and_death_flinch_use_the_3_7_window() {
        let mut cache = PuppetCache::new();
        let hand = part_label("rightHand").unwrap() as u8;
        cache.hits.insert(
            "bobcat".into(),
            LastHit {
                part: hand,
                frame: 0,
                ..Default::default()
            },
        );
        // phase 0 -> f = 0 -> strength 0, inside the window.
        assert_eq!(
            cache.flinch("bobcat", 0.0),
            Some((BodyPart::RightHand, 0.0))
        );
        // phase 3.7/2 -> f = 0.5 -> 1.85 >= 0.55 -> outside.
        assert_eq!(cache.flinch("bobcat", 3.7 * 0.5), None);
        assert_eq!(
            cache.death_flinch("bobcat", 0.0),
            Some((BodyPart::RightHand, 0.0))
        );
        assert_eq!(cache.death_flinch("bobcat", 3.7 * 0.5), None);
        assert_eq!(cache.flinch("nothing", 0.0), None);
    }

    #[test]
    fn breath_paced_needs_a_pose_set_and_a_live_choice() {
        let mut cache = PuppetCache::new();
        let mut k = key(0);
        k.statuses = statuses(&[Status::Overexerted]).0;
        assert!(cache.breath_paced("canine", &k, 1.0, 0).is_none());
        let mut set = PoseSet::new("canine");
        set.insert("stand", 1.0);
        cache.pose_sets.insert("canine".into(), Arc::new(set));
        assert!(cache
            .breath_paced("canine", &k, 1.0, PoseChoice::Dead as u8)
            .is_none());
        assert!(cache.breath_paced("canine", &k, 1.0, 0).is_some());
        // An unconscious puppet does not breathe at all (`0x6320`).
        let mut blocked = k;
        blocked.statuses = statuses(&[Status::Unconscious]).0;
        assert!(cache.breath_paced("canine", &blocked, 1.0, 0).is_none());
    }

    #[test]
    fn paint_figure_builds_the_white_full_uv_quad() {
        let p = PuppetPlacement {
            pos: [10.0, 20.0],
            scale: 2.0,
        };
        let ctx = FigureCtx {
            offset: [1.0, -1.0],
            width: 30,
            height: 40,
        };
        let q = p.paint_figure(&ctx).unwrap();
        assert_eq!(q.min, [12.0, 18.0]);
        assert_eq!(q.max, [12.0 + 60.0, 18.0 + 80.0]);
        assert_eq!(q.uv, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(q.tint, 0xffff_ffff);
        assert_eq!(FIGURE_UV, [0.0, 0.0, 1.0, 1.0]);
        // No size -> no quad (the `cbz` guards).
        assert!(p
            .paint_figure(&FigureCtx {
                width: 0,
                height: 10,
                ..ctx
            })
            .is_none());
    }

    #[test]
    fn part_index_round_trips() {
        assert_eq!(part_from_index(1), Some(BodyPart::Head));
        assert_eq!(part_from_index(14), Some(BodyPart::Nsys));
        assert_eq!(part_from_index(0), None);
        assert_eq!(part_from_index(15), None);
    }

    /// The quad helpers must hand `Painter::image` exactly the recovered values.
    #[test]
    fn figure_quad_converts_to_egui_types() {
        let p = PuppetPlacement {
            pos: [10.0, 20.0],
            scale: 2.0,
        };
        let ctx = FigureCtx {
            offset: [1.0, -1.0],
            width: 30,
            height: 40,
        };
        let q = p.paint_figure(&ctx).unwrap();
        let rect = egui::Rect::from_min_max(egui::pos2(12.0, 18.0), egui::pos2(72.0, 98.0));
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        assert_eq!(q.rect(), rect);
        assert_eq!(q.uv_rect(), uv);
        assert_eq!(q.color(), egui::Color32::WHITE);
        // The uv constant is the 16 bytes at 0x102073df0.
        assert_eq!(FIGURE_UV, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(FIGURE_TINT, u32::MAX);
    }

    /// One headless egui frame, then inspect the painted `Shape::Mesh`.
    #[test]
    fn paint_figure_in_paints_one_textured_quad() {
        let ctx = egui::Context::default();
        let tex = ctx.load_texture(
            "doll",
            egui::ColorImage::filled([2, 2], egui::Color32::RED),
            egui::TextureOptions::default(),
        );
        let id = tex.id();
        let p = PuppetPlacement {
            pos: [10.0, 20.0],
            scale: 2.0,
        };
        let fig = FigureCtx {
            offset: [1.0, -1.0],
            width: 30,
            height: 40,
        };
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));
        let mut got = None;
        let out = ctx.run_ui(input, |ui| {
            got = p.paint_figure_in(ui.painter(), &fig, id, &[]);
        });
        assert_eq!(got, p.paint_figure(&fig));
        let quad = got.unwrap();

        let meshes: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                epaint::Shape::Mesh(m) => Some(m.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(meshes.len(), 1);
        let m = &meshes[0];
        assert_eq!(m.texture_id, id);
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.indices, vec![0, 1, 2, 2, 1, 3]);
        // Bounding box == the recovered quad.
        assert_eq!(m.calc_bounds(), quad.rect());
        assert_eq!(
            m.calc_bounds(),
            egui::Rect::from_min_max(egui::pos2(12.0, 18.0), egui::pos2(72.0, 98.0))
        );
        // Vertex order and uv corners: left_top, right_top, left_bottom, right_bottom.
        let uv = quad.uv_rect();
        let r = quad.rect();
        assert_eq!(m.vertices[0].pos, r.left_top());
        assert_eq!(m.vertices[0].uv, uv.left_top());
        assert_eq!(m.vertices[1].pos, r.right_top());
        assert_eq!(m.vertices[1].uv, uv.right_top());
        assert_eq!(m.vertices[2].pos, r.left_bottom());
        assert_eq!(m.vertices[2].uv, uv.left_bottom());
        assert_eq!(m.vertices[3].pos, r.right_bottom());
        assert_eq!(m.vertices[3].uv, uv.right_bottom());
        assert_eq!(uv.min, egui::pos2(0.0, 0.0));
        assert_eq!(uv.max, egui::pos2(1.0, 1.0));
        // `mov w5, #-1`: every vertex is white.
        assert!(m.vertices.iter().all(|v| v.color == egui::Color32::WHITE));

        // A second frame is not needed for the shape, but the texture upload does
        // arrive with the first one.
        assert!(out.textures_delta.set.iter().any(|(tid, _)| *tid == id));
    }

    /// The `cbz` guards take the cached-shape path instead of an image.
    #[test]
    fn paint_figure_in_falls_back_to_the_cached_shapes() {
        let ctx = egui::Context::default();
        let tex = ctx.load_texture(
            "doll",
            egui::ColorImage::filled([2, 2], egui::Color32::RED),
            egui::TextureOptions::default(),
        );
        let id = tex.id();
        let p = PuppetPlacement {
            pos: [10.0, 20.0],
            scale: 1.0,
        };
        // width == 0 is one of the three guards.
        let fig = FigureCtx {
            offset: [0.0, 0.0],
            width: 0,
            height: 40,
        };
        let cached = vec![
            epaint::Shape::circle_filled(egui::pos2(1.0, 2.0), 3.0, egui::Color32::BLUE),
            epaint::Shape::circle_filled(egui::pos2(4.0, 5.0), 6.0, egui::Color32::GREEN),
        ];
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));
        let mut got = Some(FigureQuad {
            min: [0.0; 2],
            max: [0.0; 2],
            uv: FIGURE_UV,
            tint: FIGURE_TINT,
        });
        let out = ctx.run_ui(input, |ui| {
            got = p.paint_figure_in(ui.painter(), &fig, id, &cached);
        });
        assert_eq!(got, None);
        let meshes = out
            .shapes
            .iter()
            .filter(|cs| matches!(&cs.shape, epaint::Shape::Mesh(_)))
            .count();
        assert_eq!(meshes, 0, "the image path must not run");
        let circles = out
            .shapes
            .iter()
            .filter(|cs| matches!(&cs.shape, epaint::Shape::Circle(_)))
            .count();
        assert_eq!(circles, 2, "the cached shapes are replayed");
        let centres: Vec<_> = out
            .shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                epaint::Shape::Circle(c) => Some((c.center, c.radius)),
                _ => None,
            })
            .collect();
        assert_eq!(
            centres,
            vec![
                (egui::pos2(1.0, 2.0), 3.0),
                (egui::pos2(4.0, 5.0), 6.0)
            ]
        );
    }

    #[test]
    fn heap_bytes_counts_every_buffer() {
        let p = PosedPuppet {
            points: vec![[0.0; 3]; 4],
            mesh: Some(Arc::new(vec![[0.0; 3]; 2])),
            lit: Some(Arc::new(vec![[0.0; 3]; 3])),
            shadow_bytes: 100,
            lamps: vec![LampLayer::default(); 2],
        };
        let expect = 4 * 12 + 2 * 12 + 3 * 12 + 2 * std::mem::size_of::<LampLayer>() + 100;
        assert_eq!(p.heap_bytes(), expect);
    }
}
