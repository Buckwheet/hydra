//! Eastern-time civil date arithmetic.
//!
//! Recovered from `hydra_net::eastern` (`0x101c57dc0` .. `0x101c59098`).  Every
//! entry point works on whole days except [`date`], [`offset`] and [`midnight`],
//! which work on unix seconds.
//!
//! The original used `jiff`: civil dates are `jiff::civil::Date`, the zone is
//! `America/New_York` from the time zone database, and the DST rule is the
//! database's own.  That is what this module does again — the handlers below are
//! thin translations of the recovered signatures (`&Date` / `i64` in, the same
//! out), so the public API is unchanged from the std-only reconstruction.
//!
//! Two of the recovered functions had Howard Hinnant's day-number arithmetic
//! inlined (`0x101c58408`, `0x101c5858c`); those day numbers are now jiff's
//! epoch day, with the Hinnant form kept as the *fallback* for years that jiff's
//! `-9999..=9999` range cannot represent.  The recovered fixed US rule (second
//! Sunday in March to first Sunday in November) likewise survives only as the
//! fallback for a machine with no time zone database at all; `jiff`'s rules are
//! the ones that run, and they reproduce the recovered 2023/2024 boundaries
//! exactly (see the tests) while also getting the pre-2007 history right, which
//! the recovered rule could not.

use std::sync::OnceLock;

use jiff::civil::{Date as CivilDate, Weekday};
use jiff::tz::{Offset, TimeZone};
use jiff::{SignedDuration, Span, Timestamp};

/// A civil (calendar) date in Eastern time.
///
/// Field order and width are recovered from the disassembly: three 8-byte
/// signed words at offsets 0, 8 and 16.  The fields stay `i64` (not jiff's
/// `i16`/`i8`) because that is the recovered shape; they are range-checked when
/// the date is handed to jiff.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

impl Date {
    pub const fn new(year: i64, month: i64, day: i64) -> Date {
        Date { year, month, day }
    }

    /// Seconds east of UTC that apply at the start of this date.
    pub fn offset(&self) -> i64 {
        offset(midnight(self))
    }

    /// Days since 1970-01-01.
    pub fn day_number(&self) -> i64 {
        days_from_civil(self)
    }

    /// `YYYY-MM-DD`, zero padded exactly as the original's format string.
    pub fn as_text(&self) -> String {
        format(self)
    }
}

/// Seconds in a day.
const DAY: i64 = 86_400;
/// `EST` is UTC-5 the whole year; `EDT` is UTC-4.  Used by the no-database
/// fallback only.
const EST: i64 = -18_000;
const EDT: i64 = -14_400;
/// Number of days between `0000-03-01` and `1970-01-01` (Hinnant's epoch shift).
const CIVIL_EPOCH: i64 = 719_468;

// ---------------------------------------------------------------------------
// jiff plumbing
// ---------------------------------------------------------------------------

/// `America/New_York`, looked up once.
///
/// `TimeZone` is reference counted, so the clone per call is cheap.  If the
/// database has no `America/New_York`, `EST5EDT` (the same modern rule, without
/// the history) is tried; if neither resolves — a machine with no time zone
/// database at all — this is `None` and the callers fall back to the recovered
/// fixed rule ([`recovered_rule_offset`]), so the crate still works.
fn eastern_zone() -> Option<TimeZone> {
    static ZONE: OnceLock<Option<TimeZone>> = OnceLock::new();
    ZONE.get_or_init(|| {
        TimeZone::get("America/New_York")
            .ok()
            .or_else(|| TimeZone::get("EST5EDT").ok())
    })
    .clone()
}

/// True when the real `America/New_York` zone was found (the tests use it).
fn eastern_zone_is_available() -> bool {
    TimeZone::get("America/New_York").is_ok()
}

/// The same date as a jiff civil date, or `None` when it is out of range.
fn civil_date(date: &Date) -> Option<CivilDate> {
    let year = i16::try_from(date.year).ok()?;
    let month = i8::try_from(date.month).ok()?;
    let day = i8::try_from(date.day).ok()?;
    CivilDate::new(year, month, day).ok()
}

/// The recovered `Date` for a jiff civil date.
fn from_civil(date: CivilDate) -> Date {
    Date {
        year: i64::from(date.year()),
        month: i64::from(date.month()),
        day: i64::from(date.day()),
    }
}

/// Convert a count of days since 1970-01-01 into a civil date.
///
/// The original inlines Howard Hinnant's `civil_from_days` (0x101c58408); jiff
/// answers it directly as an epoch day, and the Hinnant form is the fallback for
/// day numbers outside jiff's year range.
pub fn civil_from_days(days: i64) -> Date {
    civil_from_days_via_jiff(days).unwrap_or_else(|| hinnant_civil_from_days(days))
}

fn civil_from_days_via_jiff(days: i64) -> Option<Date> {
    let seconds = days.checked_mul(DAY)?;
    let stamp = Timestamp::from_second(seconds).ok()?;
    Some(from_civil(stamp.to_zoned(TimeZone::UTC).date()))
}

/// Convert a civil date into a count of days since 1970-01-01.
///
/// The original inlines Howard Hinnant's `days_from_civil` (0x101c5858c); jiff
/// answers it as an epoch day, and the Hinnant form is the fallback for dates
/// outside jiff's year range.
pub fn days_from_civil(date: &Date) -> i64 {
    days_from_civil_via_jiff(date).unwrap_or_else(|| hinnant_days_from_civil(date))
}

fn days_from_civil_via_jiff(date: &Date) -> Option<i64> {
    let civil = civil_date(date)?;
    let zoned = civil.to_zoned(TimeZone::UTC).ok()?;
    Some(zoned.timestamp().as_second().div_euclid(DAY))
}

/// The current time as unix seconds.
///
/// The original is `now()` clamped at zero (a pre-epoch clock reports `0`),
/// recovered from 0x101c5866c; jiff's `Timestamp::now` is the same clock read.
pub fn now() -> i64 {
    Timestamp::now().as_second().max(0)
}

// ---------------------------------------------------------------------------
// The time zone
// ---------------------------------------------------------------------------

/// The UTC offset in force at unix second `at`: `-18000` (EST) or `-14400` (EDT).
///
/// Recovered from 0x101c58dc8.  The offset is jiff's for `America/New_York`, so
/// the whole zoneinfo history is in force (war time, the 1974 experiment, the
/// pre-2007 rule), not just the two boundaries the recovered code compared
/// against.
pub fn offset(at: i64) -> i64 {
    offset_via_jiff(at).unwrap_or_else(|| recovered_rule_offset(at))
}

fn offset_via_jiff(at: i64) -> Option<i64> {
    let stamp = Timestamp::from_second(at).ok()?;
    Some(offset_seconds(eastern_zone()?.to_offset(stamp)))
}

/// The number of seconds east of UTC that `offset` describes.
fn offset_seconds(offset: Offset) -> i64 {
    let span: SignedDuration = offset.duration_since(Offset::UTC);
    span.as_secs()
}

/// The Eastern civil date at unix second `at`.
///
/// Recovered from 0x101c586c0: shift by the offset in force and divide down to
/// a day number.  jiff does both steps (`Timestamp::to_zoned` → `Zoned::date`).
pub fn date(at: i64) -> Date {
    date_via_jiff(at).unwrap_or_else(|| {
        civil_from_days((at + recovered_rule_offset(at)).div_euclid(DAY))
    })
}

fn date_via_jiff(at: i64) -> Option<Date> {
    let stamp = Timestamp::from_second(at).ok()?;
    Some(from_civil(stamp.to_zoned(eastern_zone()?).date()))
}

/// Unix second of 00:00 Eastern on `date`.
///
/// Recovered from 0x101c59098.  Midnight is resolved in the zone, which is the
/// step that makes the switch days come out right: 2024-03-10 is still 05:00
/// UTC (the change is at 02:00 local) and 2024-11-03 is 04:00 UTC.
pub fn midnight(date: &Date) -> i64 {
    midnight_via_jiff(date).unwrap_or_else(|| {
        let days = days_from_civil(date);
        days * DAY - recovered_rule_offset(days * DAY + 18_000)
    })
}

fn midnight_via_jiff(date: &Date) -> Option<i64> {
    let civil = civil_date(date)?;
    // A civil date is never ambiguous or skipped at 00:00 in this zone, so
    // jiff's default resolution (the earliest instant) is the recovered answer.
    let zoned = civil.to_zoned(eastern_zone()?).ok()?;
    Some(zoned.timestamp().as_second())
}

/// The Sunday that starts the week containing `date`.
///
/// Recovered from 0x101c57dc0: the day number was floored to a multiple of
/// seven in the frame the original uses, which lands on Sunday.  jiff names the
/// weekday, so the floor is spelled as "how many days since Sunday".
pub fn week_start(date: &Date) -> Date {
    week_start_via_jiff(date).unwrap_or_else(|| {
        let days = days_from_civil(date);
        civil_from_days(days - (days - 3).rem_euclid(7))
    })
}

fn week_start_via_jiff(date: &Date) -> Option<Date> {
    let civil = civil_date(date)?;
    let back = civil.weekday().since(Weekday::Sunday);
    let sunday = civil.checked_sub(Span::new().days(back)).ok()?;
    Some(from_civil(sunday))
}

/// The Sunday that starts the "stretch" containing `date`, but never earlier
/// than the 1st of the month.
///
/// Recovered from 0x101c5803c by executing it: the original walks backwards one
/// day at a time and stops on a Sunday or on the 1st of the month, so a week
/// that straddles a month boundary is clipped to the 1st.  The walk is kept —
/// jiff has no "Sunday or the 1st" step — but each step is a jiff date.
pub fn stretch_start(date: &Date) -> Date {
    let mut day = *date;
    loop {
        if day.day <= 1 || is_sunday(&day) {
            return day;
        }
        day = civil_from_days(days_from_civil(&day) - 1);
    }
}

/// True when `date` falls on a Sunday.
fn is_sunday(date: &Date) -> bool {
    match civil_date(date) {
        Some(civil) => civil.weekday() == Weekday::Sunday,
        None => days_from_civil(date).rem_euclid(7) == 3,
    }
}

/// `YYYY-MM-DD`, zero padded (`{:04}-{:02}-{:02}`; the format string is at
/// 0x103bfa729).
pub fn format(date: &Date) -> String {
    match civil_date(date) {
        // jiff's own printer: ISO 8601, years padded to four digits.
        Some(civil) => civil.to_string(),
        None => format!("{:04}-{:02}-{:02}", date.year, date.month, date.day),
    }
}

/// Parse `YYYY-MM-DD`, rejecting anything that is not a canonical date.
///
/// Recovered from 0x101c58abc: split on `-` into exactly three fields of widths
/// 4/2/2, parse each as a signed integer (the original accepts a leading `+` or
/// `-`, as `str::parse::<i64>` does), then round-trip so that non-existent dates
/// such as `2023-02-30` are rejected rather than normalised.  The round trip is
/// now jiff's own range check, which is what rejects `2023-02-30`, `2023-02-29`
/// and `2024-13-01`.
pub fn parse(text: &str) -> Option<Date> {
    let mut fields = text.split('-');
    let year = fields.next()?;
    let month = fields.next()?;
    let day = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    if year.len() != 4 || month.len() != 2 || day.len() != 2 {
        return None;
    }
    let parsed = Date {
        year: year.parse().ok()?,
        month: month.parse().ok()?,
        day: day.parse().ok()?,
    };
    // `civil_date` is `jiff::civil::Date::new`, which rejects a date that does
    // not exist; the round trip catches a jiff date that does not survive the
    // trip through the `i64` fields.
    let civil = civil_date(&parsed)?;
    (from_civil(civil) == parsed).then_some(parsed)
}

// ---------------------------------------------------------------------------
// Fallbacks for what jiff cannot represent
// ---------------------------------------------------------------------------

/// Day number of the n-th Sunday (1-based) of a month, as days since 1970-01-01.
///
/// The binary computes this inline; `first` is the day number of the 1st of the
/// month and `nth` counts Sundays from 1.
fn nth_sunday(first: i64, nth: i64) -> i64 {
    // 1970-01-01 was a Thursday, so day numbers congruent to 3 mod 7 are Sundays.
    let to_sunday = (3 - first).rem_euclid(7);
    first + to_sunday + (nth - 1) * 7
}

/// Unix second at which DST starts in `year`: 02:00 EST on the second Sunday of
/// March, which is 07:00 UTC.
fn dst_start(year: i64) -> i64 {
    let march_first = hinnant_days_from_civil(&Date::new(year, 3, 1));
    nth_sunday(march_first, 2) * DAY + 25_200
}

/// Unix second at which DST ends in `year`: 02:00 EDT on the first Sunday of
/// November, which is 06:00 UTC.
fn dst_end(year: i64) -> i64 {
    let november_first = hinnant_days_from_civil(&Date::new(year, 11, 1));
    nth_sunday(november_first, 1) * DAY + 21_600
}

/// True when Eastern daylight time is in force at unix second `at`.
fn is_dst(at: i64, year: i64) -> bool {
    at >= dst_start(year) && at < dst_end(year)
}

/// The recovered fixed US rule: `-14400` between the second Sunday of March and
/// the first Sunday of November, `-18000` otherwise.
///
/// This is the fallback for a timestamp jiff cannot represent (outside
/// `-9999..=9999`) or a machine with no time zone database.  It is exactly what
/// the std-only reconstruction computed; the year used for the rule is the
/// Eastern calendar year of `at`.
fn recovered_rule_offset(at: i64) -> i64 {
    let year = hinnant_civil_from_days((at + EST).div_euclid(DAY)).year;
    if is_dst(at, year) {
        EDT
    } else {
        EST
    }
}

/// Howard Hinnant's `civil_from_days`, kept as the out-of-range fallback.
fn hinnant_civil_from_days(days: i64) -> Date {
    let z = days + CIVIL_EPOCH;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = mp + if mp < 10 { 3 } else { -9 }; // [1, 12]
    Date {
        year: y + if m <= 2 { 1 } else { 0 },
        month: m,
        day: d,
    }
}

/// Howard Hinnant's `days_from_civil`, kept as the out-of-range fallback.
fn hinnant_days_from_civil(date: &Date) -> i64 {
    let (mut y, m, d) = (date.year, date.month, date.day);
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - CIVIL_EPOCH
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Days since 1970-01-01.
    fn day(year: i64, month: i64, day: i64) -> i64 {
        days_from_civil(&Date::new(year, month, day))
    }

    #[test]
    fn civil_round_trips() {
        for (y, m, d) in [
            (1970, 1, 1),
            (1969, 12, 31),
            (2000, 2, 29),
            (2024, 3, 10),
            (1999, 12, 31),
            (1900, 1, 1),
            (2100, 3, 1),
        ] {
            let date = Date::new(y, m, d);
            assert_eq!(civil_from_days(days_from_civil(&date)), date, "{y}-{m}-{d}");
        }
    }

    #[test]
    fn known_day_numbers() {
        assert_eq!(day(1970, 1, 1), 0);
        assert_eq!(day(1970, 1, 2), 1);
        assert_eq!(day(1969, 12, 31), -1);
        assert_eq!(day(2000, 1, 1), 10_957);
        assert_eq!(day(2024, 3, 10), 19_792);
        assert_eq!(day(2024, 11, 3), 20_030);
        assert_eq!(civil_from_days(19_797), Date::new(2024, 3, 15));
        assert_eq!(civil_from_days(-1000), Date::new(1967, 4, 7));
    }

    #[test]
    fn weekday_frame_is_sunday_at_three() {
        // 1970-01-04 was the first Sunday after the epoch.
        assert_eq!(day(1970, 1, 4).rem_euclid(7), 3);
        assert_eq!(day(2024, 3, 10).rem_euclid(7), 3);
    }

    #[test]
    fn the_time_zone_database_is_the_one_answering() {
        // jiff resolves `America/New_York` from the bundled/on-disk database; if
        // it ever stopped, every offset below would come from the fallback rule
        // instead and this guard would say so.
        assert!(eastern_zone_is_available(), "no America/New_York in the tzdb");
        // War Time: the whole of 1942-02-09 .. 1945-09-30 was UTC-4, which the
        // recovered post-2007 rule cannot reproduce — only the database can.
        assert_eq!(offset(day(1945, 1, 15) * DAY + 43_200), EDT);
        assert_eq!(offset(day(1945, 12, 15) * DAY + 43_200), EST);
        // The 1974 year-round experiment started on 1974-01-06.
        assert_eq!(offset(day(1974, 2, 1) * DAY + 43_200), EDT);
        assert_eq!(offset(day(1974, 12, 1) * DAY + 43_200), EST);
    }

    #[test]
    fn dst_boundaries_2024() {
        // 2024-03-10 07:00 UTC and 2024-11-03 06:00 UTC, the recovered boundaries.
        assert_eq!(dst_start(2024), 19_792 * DAY + 25_200);
        assert_eq!(dst_end(2024), 20_030 * DAY + 21_600);
        // 2023: DST ran 2023-03-12 .. 2023-11-05.
        assert_eq!(dst_start(2023), day(2023, 3, 12) * DAY + 25_200);
        assert_eq!(dst_end(2023), day(2023, 11, 5) * DAY + 21_600);
    }

    #[test]
    fn offset_flips_at_the_boundaries() {
        assert_eq!(offset(dst_start(2024)), EDT);
        assert_eq!(offset(dst_start(2024) - 1), EST);
        assert_eq!(offset(dst_end(2024) - 1), EDT);
        assert_eq!(offset(dst_end(2024)), EST);
        // Mid-summer and mid-winter.
        assert_eq!(offset(day(2024, 7, 1) * DAY + 43_200), EDT);
        assert_eq!(offset(day(2024, 1, 15) * DAY + 43_200), EST);
    }

    #[test]
    fn jiff_and_the_recovered_rule_agree_on_the_modern_era() {
        // The recovered rule *is* the post-2007 US rule, so from 2007 on every
        // hour must agree with the database; before it, the database has the
        // history and the rule does not, which is why the rule is only ever a
        // fallback.
        let mut at = day(2007, 1, 1) * DAY;
        let end = day(2027, 1, 1) * DAY;
        while at < end {
            assert_eq!(
                offset(at),
                recovered_rule_offset(at),
                "offset disagreement at {at} ({})",
                format(&date(at))
            );
            at += 3600;
        }
    }

    #[test]
    fn dates_at_noon_utc() {
        assert_eq!(date(day(2024, 1, 15) * DAY + 43_200), Date::new(2024, 1, 15));
        assert_eq!(date(day(2024, 7, 1) * DAY + 43_200), Date::new(2024, 7, 1));
        // 2024-03-10 12:00 UTC is already DST (the switch is at 07:00 UTC).
        assert_eq!(offset(day(2024, 3, 10) * DAY + 43_200), EDT);
        assert_eq!(date(day(2024, 3, 10) * DAY + 43_200), Date::new(2024, 3, 10));
        // 04:30 UTC on 2024-03-10 is still 23:30 EST on 2024-03-09.
        assert_eq!(date(day(2024, 3, 10) * DAY + 16_200), Date::new(2024, 3, 9));
    }

    #[test]
    fn midnight_is_local_midnight() {
        // Winter: 00:00 EST is 05:00 UTC.
        assert_eq!(midnight(&Date::new(2024, 1, 15)), day(2024, 1, 15) * DAY + 18_000);
        // Summer: 00:00 EDT is 04:00 UTC.
        assert_eq!(midnight(&Date::new(2024, 7, 1)), day(2024, 7, 1) * DAY + 14_400);
        // The switch day itself: midnight is still standard time ...
        assert_eq!(midnight(&Date::new(2024, 3, 10)), day(2024, 3, 10) * DAY + 18_000);
        // ... and the day DST ends still starts in daylight time.
        assert_eq!(midnight(&Date::new(2024, 11, 3)), day(2024, 11, 3) * DAY + 14_400);
        assert_eq!(midnight(&Date::new(1970, 1, 1)), 18_000);
    }

    #[test]
    fn midnight_inverts_date() {
        for (y, m, d) in [(2024, 3, 10), (2024, 11, 3), (2024, 7, 4), (1999, 12, 31)] {
            let date = Date::new(y, m, d);
            assert_eq!(super::date(midnight(&date)), date);
            assert_eq!(
                super::date(midnight(&date) - 1),
                civil_from_days(days_from_civil(&date) - 1)
            );
        }
    }

    #[test]
    fn midnight_agrees_with_the_recovered_arithmetic() {
        // The recovered `days * DAY - offset(days * DAY + 18000)`, checked
        // against the zone on every day of a year.
        let mut days = day(2024, 1, 1);
        while days < day(2025, 1, 1) {
            let date = civil_from_days(days);
            assert_eq!(
                midnight(&date),
                days * DAY - recovered_rule_offset(days * DAY + 18_000),
                "{}",
                format(&date)
            );
            days += 1;
        }
    }

    #[test]
    fn week_start_is_sunday() {
        assert_eq!(week_start(&Date::new(2024, 3, 15)), Date::new(2024, 3, 10));
        assert_eq!(week_start(&Date::new(2024, 3, 10)), Date::new(2024, 3, 10));
        assert_eq!(week_start(&Date::new(1970, 1, 1)), Date::new(1969, 12, 28));
        assert_eq!(week_start(&Date::new(2024, 1, 1)), Date::new(2023, 12, 31));
        assert_eq!(week_start(&Date::new(1999, 12, 31)), Date::new(1999, 12, 26));
    }

    #[test]
    fn week_start_matches_the_floored_day_number() {
        let mut days = day(1990, 1, 1);
        while days < day(2030, 1, 1) {
            let date = civil_from_days(days);
            let start = week_start(&date);
            assert!(is_sunday(&start), "{} is not a Sunday", format(&start));
            let back = days - days_from_civil(&start);
            assert!((0..7).contains(&back), "{} -> {} is {back} days", format(&date), format(&start));
            days += 1;
        }
    }

    #[test]
    fn stretch_start_clips_to_the_month() {
        // A week inside one month behaves like week_start.
        assert_eq!(stretch_start(&Date::new(2024, 3, 15)), Date::new(2024, 3, 10));
        assert_eq!(stretch_start(&Date::new(2024, 3, 9)), Date::new(2024, 3, 3));
        // A week that straddles a month boundary stops at the 1st.
        assert_eq!(stretch_start(&Date::new(2024, 3, 1)), Date::new(2024, 3, 1));
        assert_eq!(stretch_start(&Date::new(2024, 3, 2)), Date::new(2024, 3, 1));
        assert_eq!(stretch_start(&Date::new(2024, 4, 6)), Date::new(2024, 4, 1));
        assert_eq!(stretch_start(&Date::new(2025, 1, 1)), Date::new(2025, 1, 1));
        // Sundays are their own stretch start.
        assert_eq!(stretch_start(&Date::new(2024, 4, 7)), Date::new(2024, 4, 7));
        assert_eq!(stretch_start(&Date::new(2024, 9, 1)), Date::new(2024, 9, 1));
    }

    #[test]
    fn format_is_zero_padded() {
        assert_eq!(format(&Date::new(2024, 3, 5)), "2024-03-05");
        assert_eq!(format(&Date::new(999, 12, 31)), "0999-12-31");
        // jiff's own printer agrees for every day of a year.
        let mut days = day(2024, 1, 1);
        while days < day(2025, 1, 1) {
            let date = civil_from_days(days);
            assert_eq!(
                format(&date),
                format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
            );
            days += 1;
        }
    }

    #[test]
    fn parse_accepts_canonical_dates_only() {
        assert_eq!(parse("2024-03-05"), Some(Date::new(2024, 3, 5)));
        assert_eq!(parse("2024-3-05"), None); // field width is exact
        assert_eq!(parse("2024-03-5"), None);
        assert_eq!(parse("2024-03-05 "), None);
        assert_eq!(parse("2024-13-01"), None); // normalises, so rejected
        assert_eq!(parse("2023-02-30"), None);
        assert_eq!(parse("2023-02-29"), None); // not a leap year
        assert_eq!(parse("2024-02-29"), Some(Date::new(2024, 2, 29)));
        assert_eq!(parse("2024-03-05-06"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse("abcd-03-05"), None);
        // The signed-integer spelling the original accepted.
        assert_eq!(parse("+999-01-01"), Some(Date::new(999, 1, 1)));
        assert_eq!(parse("2024-+3-05"), Some(Date::new(2024, 3, 5)));
    }

    #[test]
    fn parse_format_round_trip() {
        for (y, m, d) in [(2024, 1, 1), (1999, 12, 31), (2000, 2, 29), (2024, 11, 3)] {
            let date = Date::new(y, m, d);
            assert_eq!(parse(&format(&date)), Some(date));
        }
    }

    #[test]
    fn out_of_range_years_still_use_the_hinnant_fallback() {
        // jiff stops at year 9999; the recovered arithmetic does not.
        let far = Date::new(20_000, 3, 1);
        let days = days_from_civil(&far);
        assert_eq!(days, hinnant_days_from_civil(&far));
        assert_eq!(civil_from_days(days), far);
        // A day number far past jiff's range still round-trips.
        assert_eq!(days_from_civil(&civil_from_days(4_000_000)), 4_000_000);
        // And a timestamp past jiff's range still answers an offset.
        assert_eq!(offset(400_000_000_000), recovered_rule_offset(400_000_000_000));
    }
}
