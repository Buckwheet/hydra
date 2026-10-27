//! Eastern-time civil date arithmetic.
//!
//! Recovered from `hydra_net::eastern` (`0x101c57dc0` .. `0x101c59098`).  Every
//! entry point works on whole days except [`date`], [`offset`] and [`midnight`],
//! which work on unix seconds.  The original used `chrono`; this is a std-only
//! reimplementation of the same arithmetic (Howard Hinnant's `days_from_civil`,
//! which the binary has inlined verbatim).
//!
//! "Eastern" here means `America/New_York` with a *fixed* rule rather than a
//! zoneinfo lookup: DST runs from the second Sunday in March (02:00 local) to
//! the first Sunday in November (02:00 local), i.e. `EST` is UTC-5 and `EDT` is
//! UTC-4.  Both boundaries were recovered exactly from the inlined arithmetic in
//! `date`/`offset`; see NOTES.md.

/// A civil (calendar) date in Eastern time.
///
/// Field order and width are recovered from the disassembly: three 8-byte
/// signed words at offsets 0, 8 and 16.
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
/// `EST` is UTC-5 the whole year; `EDT` is UTC-4.
const EST: i64 = -18_000;
const EDT: i64 = -14_400;
/// Number of days between `0000-03-01` and `1970-01-01` (Hinnant's epoch shift).
const CIVIL_EPOCH: i64 = 719_468;

/// Convert a count of days since 1970-01-01 into a civil date.
///
/// Howard Hinnant's `civil_from_days`; the binary inlines it (0x101c58408).
pub fn civil_from_days(days: i64) -> Date {
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

/// Convert a civil date into a count of days since 1970-01-01.
///
/// Howard Hinnant's `days_from_civil`; the binary inlines it (0x101c5858c).
pub fn days_from_civil(date: &Date) -> i64 {
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

/// The current time as unix seconds.
///
/// The original is `SystemTime::now().duration_since(UNIX_EPOCH)` with the
/// seconds clamped at `i64::MAX` and a pre-epoch clock reported as `0`
/// (0x101c5866c).
pub fn now() -> i64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs().min(i64::MAX as u64) as i64,
        Err(_) => 0,
    }
}

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
    let march_first = days_from_civil(&Date::new(year, 3, 1));
    nth_sunday(march_first, 2) * DAY + 25_200
}

/// Unix second at which DST ends in `year`: 02:00 EDT on the first Sunday of
/// November, which is 06:00 UTC.
fn dst_end(year: i64) -> i64 {
    let november_first = days_from_civil(&Date::new(year, 11, 1));
    nth_sunday(november_first, 1) * DAY + 21_600
}

/// True when Eastern daylight time is in force at unix second `at`.
fn is_dst(at: i64, year: i64) -> bool {
    at >= dst_start(year) && at < dst_end(year)
}

/// The UTC offset in force at unix second `at`: `-18000` (EST) or `-14400` (EDT).
///
/// Recovered from 0x101c58dc8.  The year used for the DST rule is the Eastern
/// calendar year of `at`, exactly as the original computes it.
pub fn offset(at: i64) -> i64 {
    let year = civil_from_days((at + EST).div_euclid(DAY)).year;
    if is_dst(at, year) {
        EDT
    } else {
        EST
    }
}

/// The Eastern civil date at unix second `at`.
///
/// Recovered from 0x101c586c0: shift by the offset in force and divide down to
/// a day number.
pub fn date(at: i64) -> Date {
    civil_from_days((at + offset(at)).div_euclid(DAY))
}

/// Unix second of 00:00 Eastern on `date`.
///
/// Recovered from 0x101c59098.  Midnight can fall on either side of a DST
/// change (the change happens at 02:00 local), so the offset is evaluated at the
/// standard-time midnight before it is applied.
pub fn midnight(date: &Date) -> i64 {
    let days = days_from_civil(date);
    days * DAY - offset(days * DAY + 18_000)
}

/// The Sunday that starts the week containing `date`.
///
/// Recovered from 0x101c57dc0: the day number is floored to a multiple of seven
/// in the shifted frame the original uses, which lands on Sunday.
pub fn week_start(date: &Date) -> Date {
    let days = days_from_civil(date);
    civil_from_days(days - (days - 3).rem_euclid(7))
}

/// The Sunday that starts the "stretch" containing `date`, but never earlier
/// than the 1st of the month.
///
/// Recovered from 0x101c5803c by executing it: the original walks backwards one
/// day at a time and stops on a Sunday or on the 1st of the month, so a week
/// that straddles a month boundary is clipped to the 1st.
pub fn stretch_start(date: &Date) -> Date {
    let mut day = *date;
    loop {
        let days = days_from_civil(&day);
        if day.day <= 1 || days.rem_euclid(7) == 3 {
            return day;
        }
        day = civil_from_days(days - 1);
    }
}

/// `YYYY-MM-DD`, zero padded (`{:04}-{:02}-{:02}`; the format string is at
/// 0x103bfa729).
pub fn format(date: &Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

/// Parse `YYYY-MM-DD`, rejecting anything that is not a canonical date.
///
/// Recovered from 0x101c58abc: split on `-` into exactly three fields of widths
/// 4/2/2, parse each as a signed integer (the original accepts a leading `+` or
/// `-`, as `str::parse::<i64>` does), then round-trip through
/// [`days_from_civil`]/[`civil_from_days`] so that non-existent dates such as
/// `2023-02-30` are rejected rather than normalised.
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
    if civil_from_days(days_from_civil(&parsed)) == parsed {
        Some(parsed)
    } else {
        None
    }
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
    fn dst_boundaries_2024() {
        // 2024-03-10 07:00 UTC and 2024-11-03 06:00 UTC.
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
    fn week_start_is_sunday() {
        assert_eq!(week_start(&Date::new(2024, 3, 15)), Date::new(2024, 3, 10));
        assert_eq!(week_start(&Date::new(2024, 3, 10)), Date::new(2024, 3, 10));
        assert_eq!(week_start(&Date::new(1970, 1, 1)), Date::new(1969, 12, 28));
        assert_eq!(week_start(&Date::new(2024, 1, 1)), Date::new(2023, 12, 31));
        assert_eq!(week_start(&Date::new(1999, 12, 31)), Date::new(1999, 12, 26));
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
    }

    #[test]
    fn parse_format_round_trip() {
        for (y, m, d) in [(2024, 1, 1), (1999, 12, 31), (2000, 2, 29), (2024, 11, 3)] {
            let date = Date::new(y, m, d);
            assert_eq!(parse(&format(&date)), Some(date));
        }
    }
}
