//! The in-game sky model.
//!
//! Recovered from `gs_base::sky::GameSky::sky` @0x100a75e78.  Everything below is
//! *established* unless marked otherwise.
//!
//! ```text
//! GameSky::sky(&self, month: u32, day: u32, time: f64, hour: f32) -> Sky
//! ```
//!
//! The three orbital constants are doubles embedded in the binary and are the
//! reference periods of the model:
//!
//! | constant | bits                 | value (s) | meaning      |
//! |----------|----------------------|-----------|--------------|
//! | `C_YEAR` | `0x417E187E00000000` | 31 557 600 | 365.25 days  |
//! | `C_SLOW` | `0x414539C000000000` |  2 782 080 | 32.2 days    |
//! | `C_FAST` | `0x41233F8000000000` |    630 720 | 7.3 days     |
//!
//! The two moon phases are
//!
//! ```text
//! phase(p) = fract( (time mod p)/p - (time mod C_YEAR)/C_YEAR + 0.5 )
//! ```
//!
//! (`fract` is `x - trunc(x)`; the second `+0.5`/wrap pair in the disassembly is
//! part of it.)  Their illumination is `(1 - cos 2π p)/2` and their transit hour
//! is `(p*24 + 13) mod 24` — the `+13` is the recovered offset.
//!
//! ## Day length
//!
//! The year folds into a triangle wave built entirely from immediate constants:
//!
//! ```text
//! s = (time as i64).rem_euclid(31_557_600)
//! folded = if s < 14_792_400      { 14_824_800 - s }
//!          else if s < 30_603_600 { s - 14_792_400 }
//!          else                   { 46_382_400 - s }
//! season = (folded * 376_287_347) >> 39            // == folded / 1461
//! ```
//!
//! `376_287_347 == round(2^39 / 1461)`, i.e. `season = folded / 1461`.  It
//! becomes the seasonal half-swing in hours; the binary stores
//! `dawn = (season + 16200)/3600 + 1` and `dusk = (70200 - season)/3600 + 1`
//! (the `+1` is why `moon_light_of` subtracts it again).  So the daylight window
//! is `4.5 + season/3600 .. 19.5 - season/3600`, centred on 12:00, shrinking from
//! 15.0 h to about 8.99 h across the year.
//!
//! ## Day fraction
//!
//! The month table at 0x1021f0964 is the ordinary non-leap Gregorian
//! `[0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334]`:
//!
//! ```text
//! day = fract((MONTH_START[month-1] + (day - 1) + hour/24) / 365.0)
//! ```
//!
//! ## Festival day (month 12)
//!
//! Two extra quantities come out of the `month == 12` special case:
//!
//! * [`Sky::festival`] is `clamp((day - 20 + hour/24) / 11, 0, 1)` when
//!   `month == 12 && day > 19`, else `0.0`;
//! * a "night floor" of exactly `1.0` on `month == 12 && day == 31`, which the
//!   binary folds straight into the slow moon's illumination with `fmaxnm`
//!   (so it is not stored separately).
//!
//! INFERRED: the *meaning* of these two fields; the arithmetic is exact.

use crate::sky_light::{illum_from_phase, FLAT_DAY};

/// Reference period: 365.25 days, in seconds.  `0x417E187E00000000`.
pub const C_YEAR: f64 = 31_557_600.0;
/// Reference period of the slow moon: 32.2 days, in seconds.  `0x414539C000000000`.
pub const C_SLOW: f64 = 2_782_080.0;
/// Reference period of the fast moon: 7.3 days, in seconds.  `0x41233F8000000000`.
pub const C_FAST: f64 = 630_720.0;

/// First fold boundary inside the year, in seconds (`0x00E1B6D0`).
pub const FOLD_HI: i64 = 14_792_400;
/// Peak of the triangle wave: `14_824_800 - s` applies below [`FOLD_HI`]
/// (`0x00E23560`).
pub const FOLD_PEAK: i64 = 14_824_800;
/// Second fold boundary (`0x01D2F950`).
pub const FOLD_LO: i64 = 30_603_600;
/// Symmetric partner: `46_382_400 - s` (`0x02C3BD40`).
pub const FOLD_END: i64 = 46_382_400;

/// `round(2^39 / 1461)` — the magic multiplier turning `folded` into
/// `folded / 1461`.  `0x166DB073`.
pub const SEASON_MAGIC: u64 = 376_287_347;
/// Shift paired with [`SEASON_MAGIC`].
pub const SEASON_SHIFT: u32 = 39;

/// Float embedded in the binary as `0x47891C00` (`70200.0`): `dusk*3600` before
/// the `-1` shift.
pub const DUSK_SUM: f32 = 70_200.0;
/// `16200` — the integer added to `season` before converting, i.e.
/// `4.5 * 3600`.
pub const DAWN_BASE: u32 = 16_200;

/// Months' start day-of-year (non-leap).  Table @0x1021f0964.
pub static MONTH_START: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// Seconds in the model's year.
pub const SECS_PER_YEAR: i64 = 31_557_600;

/// The model's input: the two fields `sky()` reads off `&self`.
///
/// In the binary `sky()` loads a `u32` from `self + 0x10` and an `f32` from
/// `self + 0x14`; the 16 bytes before them are never touched here.
///
/// UNKNOWN: the original struct is larger (its first 16 bytes are unused by
/// `sky()`), so only the two fields that participate are modelled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameSky {
    /// Copied straight into [`Sky::kind`].
    pub kind: u32,
    /// Copied straight into [`Sky::brightness`]; unused by the light model.
    pub brightness: f32,
}

impl Default for GameSky {
    fn default() -> Self {
        // `sky()` always writes `1` into `Sky::x08` and `Sky::x10`, and the
        // light model's `x10 == 1` branch is the seasonal one, so `kind = 1`
        // reproduces the shipped configuration.
        GameSky {
            kind: 1,
            brightness: 0.0,
        }
    }
}

/// The resulting sky state — the 0x34-byte `sret` of `sky()`, fields at their
/// recovered offsets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sky {
    /// 0x00 — copy of [`GameSky::kind`].
    pub kind: u32,
    /// 0x04 — copy of [`GameSky::brightness`].
    pub brightness: f32,
    /// 0x08 — always `1`.
    pub x08: u32,
    /// 0x0c — day-of-year fraction in `[0, 1)`.
    pub day: f32,
    /// 0x10 — always `1`; selects the seasonal dawn/dusk window in
    /// `sky_light::moon_light_of`.
    pub x10: u32,
    /// 0x14 — `(season + 16200) / 3600 + 1`, i.e. `dawn_end + 1`.
    pub dawn: f32,
    /// 0x18 — `(70200 - season) / 3600 + 1`, i.e. `dusk_start + 1`.
    pub dusk: f32,
    /// 0x1c — fast moon illumination in `[0, 1]`.
    pub fast_moon_phase: f32,
    /// 0x20 — always `0`; scales the `1 - 0.9x` darkening in `moon_light_of`.
    pub cloud: f32,
    /// 0x24 — fast moon transit hour in `[0, 24)`.
    pub fast_moon_hour: f32,
    /// 0x28 — slow moon illumination in `[0, 1]`, floored by the month-12
    /// night floor.
    pub slow_moon_phase: f32,
    /// 0x2c — slow moon transit hour in `[0, 24)`.
    pub slow_moon_hour: f32,
    /// 0x30 — month-12 festival blend, see the module docs.
    pub festival: f32,
}

impl Sky {
    /// The four key hours `sky_light::night_share_in` consumes.
    ///
    /// `Sky::x10 == 1` (the shipped case) uses the seasonal window: `dawn` and
    /// `dusk` are stored `+1.0` shifted, so the window is
    /// `[dawn-1, dawn-1, dusk-1, dusk-1]` — the binary builds it with
    /// `ldur d4, [self+0x14]; zip1.4s; fadd -1.0`.  Otherwise the flat
    /// `[6, 8, 18, 20]` table at 0x1021b45e0 is used.
    pub fn day_windows(&self) -> [f32; 4] {
        if self.x10 == 1 {
            let a = self.dawn - 1.0;
            let b = self.dusk - 1.0;
            [a, a, b, b]
        } else {
            FLAT_DAY
        }
    }

    /// Share of the sky that is "night" at `hour`.
    pub fn night_share(&self, hour: f32) -> f32 {
        crate::sky_light::night_share_in(&self.day_windows(), hour)
    }

    /// Light contributed by the fast moon at `hour`.
    pub fn moon_light(&self, hour: f32) -> f32 {
        crate::sky_light::moon_light(self, hour)
    }

    /// Light contributed by the slow (Lornon) moon at `hour`.
    pub fn lornon_light(&self, hour: f32) -> f32 {
        crate::sky_light::lornon_light(self, hour)
    }
}

impl GameSky {
    /// Build the [`Sky`] state.
    ///
    /// `@0x100a75e78`.  `time` is a `f64` holding seconds since the Unix epoch;
    /// `hour` is the wall-clock hour in `[0, 24)` (divided by 24 internally).
    pub fn sky(&self, month: u32, day: u32, time: f64, hour: f32) -> Sky {
        // ---- day-of-year fraction --------------------------------------------
        // `month.min(12)` with `0 -> 1`, and `day == 0 -> 0`, else `min(day,31)-1`
        // are the binary's own out-of-range clamps.
        let month_idx = if month == 0 { 1 } else { month.min(12) };
        let day0 = if day == 0 { 0 } else { day.min(31) - 1 };
        let hour_frac = hour / 24.0;
        let day_frac = ((MONTH_START[month_idx as usize - 1] + day0) as f32 + hour_frac) / 365.0;
        let day_frac = day_frac - day_frac.trunc();

        // ---- month-12 special cases ------------------------------------------
        let festival = if month == 12 && day > 19 {
            (((day - 20) as f32) + hour_frac) / 11.0
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        let night_floor = if month == 12 && day == 31 { 1.0 } else { 0.0 };

        // ---- seasonal half-swing ---------------------------------------------
        let secs = (time as i64).rem_euclid(SECS_PER_YEAR);
        let folded = if secs < FOLD_HI {
            FOLD_PEAK - secs
        } else if secs < FOLD_LO {
            secs - FOLD_HI
        } else {
            FOLD_END - secs
        };
        let season = (((folded as i128) * (SEASON_MAGIC as i128)) >> SEASON_SHIFT) as u32;
        let dawn = (season + DAWN_BASE) as f32 / 3600.0 + 1.0;
        let dusk = (DUSK_SUM - season as f32) / 3600.0 + 1.0;

        // ---- moon phases ------------------------------------------------------
        let phase_slow = moon_phase(time, C_SLOW);
        let phase_fast = moon_phase(time, C_FAST);

        Sky {
            kind: self.kind,
            brightness: self.brightness,
            x08: 1,
            day: day_frac,
            x10: 1,
            dawn,
            dusk,
            fast_moon_phase: illum_from_phase(phase_fast),
            cloud: 0.0,
            fast_moon_hour: transit_hour(phase_fast),
            slow_moon_phase: illum_from_phase(phase_slow).max(night_floor),
            slow_moon_hour: transit_hour(phase_slow),
            festival,
        }
    }
}

/// `fract((time mod period)/period - (time mod C_YEAR)/C_YEAR + 0.5)`.
fn moon_phase(time: f64, period: f64) -> f32 {
    let a = time.rem_euclid(period) / period;
    let b = time.rem_euclid(C_YEAR) / C_YEAR;
    let mut x = a - b;
    x -= x.trunc();
    x += 0.5;
    x -= x.trunc();
    x as f32
}

/// Transit hour: `(phase * 24 + 13) mod 24`.
fn transit_hour(phase: f32) -> f32 {
    (phase * 24.0 + 13.0).rem_euclid(24.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn constants_are_the_recovered_bit_patterns() {
        assert_eq!(C_YEAR.to_bits(), 0x417E187E00000000);
        assert_eq!(C_SLOW.to_bits(), 0x414539C000000000);
        assert_eq!(C_FAST.to_bits(), 0x41233F8000000000);
        // Exactly 365.25 / 32.2 / 7.3 days; the decimal forms do not round-trip,
        // so compare against the exact quotients.
        assert_eq!(C_YEAR, 365.25 * 86_400.0);
        assert!((C_SLOW - 32.2 * 86_400.0).abs() < 1e-6);
        assert!((C_FAST - 7.3 * 86_400.0).abs() < 1e-6);
        assert_eq!(SEASON_MAGIC, ((1u64 << SEASON_SHIFT) + 1460) / 1461); // ceil
        assert_eq!(DUSK_SUM, 70_200.0);
        assert_eq!(DAWN_BASE, 16_200);
        assert_eq!(DAWN_BASE as f32 / 3600.0, 4.5);
        assert_eq!(MONTH_START[0], 0);
        assert_eq!(MONTH_START[11], 334);
    }

    #[test]
    fn day_fraction_and_day_length() {
        let gs = GameSky::default();
        let sky = gs.sky(1, 1, 0.0, 0.0);
        assert!(close(sky.day, 0.0));
        // season = 14_824_800 / 1461 = 10147
        assert!(close(sky.dawn, 10_147.0 / 3600.0 + 4.5 + 1.0));
        assert!(close(sky.dusk, (70_200.0 - 10_147.0) / 3600.0 + 1.0));
        let w = sky.day_windows();
        assert!(close(w[0], 10_147.0 / 3600.0 + 4.5));
        assert!(close(w[2], (70_200.0 - 10_147.0) / 3600.0));
        // 1 July (month 7 starts at day 181), noon.
        let sky = gs.sky(7, 1, 0.0, 12.0);
        assert!(close(sky.day, (181.0 + 0.5) / 365.0));
        assert_eq!(sky.x08, 1);
        assert_eq!(sky.x10, 1);
        assert_eq!(sky.cloud, 0.0);
        assert_eq!(sky.kind, 1);
    }

    #[test]
    fn festival_and_night_floor() {
        let gs = GameSky::default();
        assert_eq!(gs.sky(11, 30, 0.0, 0.0).festival, 0.0);
        assert_eq!(gs.sky(12, 20, 0.0, 0.0).festival, 0.0);
        assert!(close(gs.sky(12, 20, 0.0, 12.0).festival, 0.5 / 11.0));
        assert!(close(gs.sky(12, 31, 0.0, 0.0).festival, 1.0));
        // The month-12 night floor forces the slow moon's illumination to 1.0.
        // At t = 0 the slow phase is 0.5, i.e. already full, so pick a time where
        // it is new (illumination 0) and check the floor takes over.
        // Solve fract((t/C_SLOW) - (t/C_YEAR) + 0.5) == 0 for the new moon.
        let new_moon_t = 0.5 / (1.0 / C_SLOW - 1.0 / C_YEAR);
        assert!(gs.sky(12, 30, new_moon_t, 0.0).slow_moon_phase < 0.01);
        assert_eq!(gs.sky(12, 31, new_moon_t, 0.0).slow_moon_phase, 1.0);
    }

    #[test]
    fn moon_phases_follow_the_periods() {
        let gs = GameSky::default();
        let sky = gs.sky(1, 1, 0.0, 0.0);
        // t = 0: phase = fract(0 - 0 + 0.5) = 0.5 -> illumination 1.0 (full).
        assert!(close(sky.fast_moon_phase, 1.0));
        assert!(close(sky.slow_moon_phase, 1.0));
        // transit hour = (0.5*24 + 13) mod 24 = 25 mod 24 = 1
        assert!(close(sky.fast_moon_hour, 1.0));
        assert!(close(sky.slow_moon_hour, 1.0));
        // One fast period later `a` resets to 0 but `b` advances by
        // C_FAST / C_YEAR, so the phase is fract(0.5 - C_FAST/C_YEAR).
        let b = gs.sky(1, 1, C_FAST, 0.0);
        let expected = (0.5 - C_FAST / C_YEAR) as f32;
        assert!((b.fast_moon_phase - illum_from_phase(expected)).abs() < 1e-3);
    }

    #[test]
    fn fold_is_a_triangle_wave() {
        let gs = GameSky::default();
        let d0 = gs.sky(1, 1, 0.0, 0.0).dawn;
        let d_mid = gs.sky(1, 1, FOLD_HI as f64, 0.0).dawn;
        let d_end = gs.sky(1, 1, (SECS_PER_YEAR - 1) as f64, 0.0).dawn;
        assert!(close(d0, d_end));
        assert!(d_mid < d0);
        // The branch boundary is a 32_400 s step == 22 units of `season`.
        assert_eq!((FOLD_PEAK - FOLD_HI) / 1461, 22);
    }
}
