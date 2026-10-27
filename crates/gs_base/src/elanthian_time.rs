//! Wall-clock arithmetic for the "Eastern" timezone Hydra presents the game in.
//!
//! Recovered from:
//! * `gs_base::elanthian_time::eastern_date`     @0x100a93f10
//! * `gs_base::elanthian_time::eastern_is_dst`   @0x100a940b8 (local symbol)
//! * `gs_base::elanthian_time::elanthian_hour_f` @0x100a9443c
//!
//! Both exported functions start from a Unix timestamp, shift it into US Eastern
//! local time (`-5h`, or `-4h` while DST is in force) and then do plain calendar
//! arithmetic.
//!
//! ## eastern_date
//!
//! ```text
//! off    = if eastern_is_dst(t) { -14_400 } else { -18_000 }
//! days   = (t + off).div_euclid(86_400)
//! (y, m, d) = civil_from_days(days)
//! ```
//!
//! `civil_from_days` is Howard Hinnant's algorithm verbatim — the binary adds
//! `719_468`, divides by `146_097`, applies the `-doe/1460 + doe/36524
//! - doe/146096` correction, then `mp = (5*doy+2)/153` and
//! `m = mp + (mp < 10 ? 3 : -9)`.  Every magic multiplier checks out against
//! those divisors (e.g. `0x59C67CD9 >> 39` divides `doe>>2` by 365, i.e.
//! `doe/1460`).
//!
//! ## elanthian_hour_f
//!
//! ```text
//! hour_of_day = ((t + off).rem_euclid(86_400)) / 3600     // integer division
//! frac        = t - floor(t)                              // sub-second part
//! result      = hour_of_day + (frac + t.rem_euclid(3600)) / 3600
//! ```
//!
//! i.e. the Eastern local hour of day as `f32` in `[0, 24)`.  The extra
//! `t.rem_euclid(3600)` term only carries the minutes/seconds that the integer
//! hour division dropped.
//!
//! ## eastern_is_dst
//!
//! The post-2007 US rule, evaluated on `t - 18_000` (EST) so that the extra hour
//! only ever matters *inside* the transition hour:
//!
//! * months 4..=10 → DST; months 12, 1, 2 → standard time;
//! * March: DST from 02:00 EST on the second Sunday (offset 07:00 UTC);
//! * November: standard time from 01:00 EST on the first Sunday
//!   (i.e. 02:00 EDT, 06:00 UTC).
//!
//! The day-of-month of the first Sunday is derived with
//! `w = (7 - (days_since_epoch % 7)) % 7`, where `days_since_epoch` is the
//! Sunday-aligned day number of the 1st of the month (`civil_days(y, m, 1) + 4`,
//! because 1970-01-01 was a Thursday and Sunday sits at index 3): `w` is
//! `first_sunday - 1`.  March then does `w | 8`, which is
//! `first_sunday + 7`, i.e. the second Sunday.

/// US Eastern standard-time offset in seconds (`-5h`).  `mov x9, #-0x3840`.
pub const EST_OFFSET: i64 = -18_000;

/// US Eastern daylight-time offset in seconds (`-4h`).  `mov x8, #-0x4650`.
pub const EDT_OFFSET: i64 = -14_400;

/// Seconds in a day.
pub const SECS_PER_DAY: i64 = 86_400;

/// The transition hour within the day, in seconds: 07:00 UTC == 02:00 EST.
pub const DST_START_SECS: i64 = 7_200;

/// 01:00 EST == 02:00 EDT, the moment DST ends.
pub const DST_END_SECS: i64 = 3_600;

/// `true` if US Eastern daylight time is in force at Unix timestamp `t`.
///
/// `@0x100a940b8`.
pub fn eastern_is_dst(t: i64) -> bool {
    // Everything below works in EST; the extra hour supplied by DST is exactly
    // what the 07:00/01:00 comparisons resolve.
    let local = t + EST_OFFSET;
    let days = local.div_euclid(SECS_PER_DAY);
    let secs_of_day = local.rem_euclid(SECS_PER_DAY);
    let (year, month, day) = civil_from_days(days);

    match month {
        3 => {
            let second_sunday = first_sunday_offset(year, 3) | 8;
            if day > second_sunday {
                true
            } else if day == second_sunday {
                secs_of_day >= DST_START_SECS
            } else {
                false
            }
        }
        11 => {
            // `w` is (first Sunday - 1); the switch happens on `w + 1`.
            let first_sunday = first_sunday_offset(year, 11);
            let transition_day = first_sunday + 1;
            if day <= first_sunday {
                true
            } else if day == transition_day {
                secs_of_day < DST_END_SECS
            } else {
                false
            }
        }
        4..=10 => true,
        _ => false,
    }
}

/// `first_sunday_of_month - 1` for the given month, reproduced from the binary's
/// `(7 - days_since_epoch % 7) % 7`.
fn first_sunday_offset(year: i64, month: u32) -> u32 {
    // The binary uses `-719464` (four *less* than the usual `-719468`), which is
    // exactly `+4` days; that shift makes `% 7` align 1970-01-01 (a Thursday)
    // such that 0 means "the 1st is a Sunday".
    let days = days_from_civil(year, month, 1) + 4;
    let rem = days.rem_euclid(7) as u32;
    (7 - rem) % 7
}

/// `civil_from_days` from Howard Hinnant's date algorithms.
///
/// `days` is the number of days since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y + i64::from(m <= 2), m as u32, d as u32)
}

/// `days_from_civil` from Howard Hinnant's date algorithms.
///
/// Note the recovered `-719464` epoch shift used by `eastern_is_dst`; this
/// helper returns the *unshifted* value (i.e. `-719468`), matching
/// [`civil_from_days`].
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The Eastern-local calendar date at Unix timestamp `t`.
///
/// `@0x100a93f10` — returns `(year, month, day)`; the binary writes `i64` then
/// two `u32`s, i.e. a `(i64, u32, u32)` tuple with offsets 0/8/12.
pub fn eastern_date(t: i64) -> (i64, u32, u32) {
    let off = if eastern_is_dst(t) {
        EDT_OFFSET
    } else {
        EST_OFFSET
    };
    civil_from_days((t + off).div_euclid(SECS_PER_DAY))
}

/// Eastern-local hour of day, as `f32` in `[0, 24)`.
///
/// `@0x100a9443c` — `t` is a `f64` (`fcvtms` = floor).
///
/// The binary guards the whole hour-of-day computation with
/// `(secs + 8_000_000_000_000) as u64 <= 16_000_000_000_000`, i.e. it only
/// computes the hour term when `|t|` is inside ±8e12 seconds; outside that
/// range the hour term is `0.0` and only the sub-second fraction survives.
/// Reproduced exactly (`SAFE_SECS`).
pub fn elanthian_hour_f(t: f64) -> f32 {
    /// `8_000_000_000_000` — the binary's `add`/`cmp` pair with `16e12`.
    pub const SAFE_SECS: i64 = 8_000_000_000_000;

    let secs = t.floor() as i64;

    let hour = if (-SAFE_SECS..=SAFE_SECS).contains(&secs) {
        let off = if eastern_is_dst(secs) {
            EDT_OFFSET
        } else {
            EST_OFFSET
        };
        let secs_of_day = (secs + off).rem_euclid(SECS_PER_DAY);
        (secs_of_day / 3_600) as f64
    } else {
        0.0
    };

    let within_hour = secs.rem_euclid(3_600);
    let frac = t - secs as f64;
    (hour + (frac + within_hour as f64) / 3_600.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2024-01-15 00:00:00 UTC == 2024-01-14 19:00 EST.
    const T_2024_01_15: i64 = 1_705_276_800;

    #[test]
    fn hinnant_round_trips() {
        for days in [-1_000_000i64, -40_000, -1, 0, 1, 19_000, 20_000, 800_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days, "days={days}");
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2024, 2, 29), 19_782);
        assert_eq!(days_from_civil(2024, 3, 10), 19_792);
        assert_eq!(days_from_civil(2024, 11, 3), 20_030);
    }

    #[test]
    fn eastern_dates() {
        // 2024-01-15T00:00:00Z -> 2024-01-14 19:00 EST
        assert_eq!(eastern_date(T_2024_01_15), (2024, 1, 14));
        // 2024-07-15T00:00:00Z -> 2024-07-14 20:00 EDT
        assert_eq!(eastern_date(1_721_001_600), (2024, 7, 14));
        // Unambiguous: 2024-06-01T12:00:00Z -> 2024-06-01 08:00 EDT
        assert_eq!(eastern_date(1_717_243_200), (2024, 6, 1));
        // Epoch: 1970-01-01T00:00:00Z -> 1969-12-31 19:00 EST
        assert_eq!(eastern_date(0), (1969, 12, 31));
    }

    #[test]
    fn dst_transitions_2024() {
        // DST began 2024-03-10 07:00 UTC; ended 2024-11-03 06:00 UTC.
        let march10 = days_from_civil(2024, 3, 10) * SECS_PER_DAY;
        let nov3 = days_from_civil(2024, 11, 3) * SECS_PER_DAY;
        // `DST_START_SECS`/`DST_END_SECS` are *local* EST seconds-of-day, so the
        // matching UTC instants are 07:00 (25_200 s) and 06:00 (21_600 s).
        assert_eq!(DST_START_SECS + 18_000, 25_200);
        assert_eq!(DST_END_SECS + 18_000, 21_600);
        assert!(!eastern_is_dst(march10 + 25_200 - 1));
        assert!(eastern_is_dst(march10 + 25_200)); // 07:00 UTC = 02:00 EST
        assert!(eastern_is_dst(march10 + 25_200 + 3_600));
        // DST ends at 06:00:00 UTC (01:00 EST == 02:00 EDT).
        assert!(eastern_is_dst(nov3 + 21_600 - 1));
        assert!(!eastern_is_dst(nov3 + 21_600));
        assert!(!eastern_is_dst(nov3 + 36_000));
    }

    #[test]
    fn dst_month_rules() {
        // Whole months.
        assert!(eastern_is_dst(days_from_civil(2024, 6, 15) * SECS_PER_DAY));
        assert!(eastern_is_dst(days_from_civil(2024, 10, 31) * SECS_PER_DAY));
        assert!(!eastern_is_dst(days_from_civil(2024, 11, 30) * SECS_PER_DAY));
        assert!(!eastern_is_dst(days_from_civil(2024, 12, 31) * SECS_PER_DAY));
        assert!(!eastern_is_dst(days_from_civil(2024, 1, 31) * SECS_PER_DAY));
        assert!(!eastern_is_dst(days_from_civil(2024, 2, 29) * SECS_PER_DAY));
        // March 1st is always standard time.
        assert!(!eastern_is_dst(days_from_civil(2024, 3, 1) * SECS_PER_DAY));
        // November 1st is always DST.
        assert!(eastern_is_dst(days_from_civil(2024, 11, 1) * SECS_PER_DAY));
    }

    #[test]
    fn first_sunday_offsets() {
        // March 2024 starts on a Friday -> first Sunday is the 3rd -> offset 2.
        assert_eq!(first_sunday_offset(2024, 3), 2);
        // November 2024 starts on a Friday -> first Sunday the 3rd -> offset 2.
        assert_eq!(first_sunday_offset(2024, 11), 2);
        // March 2020 started on a Sunday -> offset 0.
        assert_eq!(first_sunday_offset(2020, 3), 0);
        // November 2020 started on a Sunday -> offset 0.
        assert_eq!(first_sunday_offset(2020, 11), 0);
    }

    #[test]
    fn hour_fraction() {
        // 2024-01-15T00:00:00Z is 19:00 EST.
        assert_eq!(elanthian_hour_f(T_2024_01_15 as f64), 19.0);
        // Half an hour later.
        assert_eq!(elanthian_hour_f((T_2024_01_15 + 1_800) as f64), 19.5);
        // 2024-06-01T12:00:00Z is 08:00 EDT.
        assert_eq!(elanthian_hour_f(1_717_243_200.0), 8.0);
        // 06:12:36 EDT == 10:12:36 UTC
        let t = 1_717_243_200 - 2 * 3_600 + 12 * 60 + 36;
        assert_eq!(elanthian_hour_f(t as f64), 6.21);
        // Out of the safe range the hour term is dropped, but the
        // `t mod 3600` term survives: 1e15 mod 3600 == 2800.
        assert!((elanthian_hour_f(1e15) - 2_800.0 / 3_600.0).abs() < 1e-6);
        assert_eq!(1e15 as i64 % 3_600, 2_800);
        // Fractional seconds are absorbed into the hour.
        let base = 1_717_243_200.0; // exactly 08:00 EDT
        assert!((elanthian_hour_f(base + 0.5) - 8.0).abs() < 1e-3);
        assert!((elanthian_hour_f(base + 1_800.5) - 8.5).abs() < 1e-3);
    }
}
