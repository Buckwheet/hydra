//! `hydra_exp::report` — the `exp` text reports.
//!
//! Every function here returns the lines a report is made of (`Vec<String>`);
//! the caller prints them. The formats, the column widths and the arithmetic
//! below were read out of the instruction stream and the format-string blobs
//! (see `NOTES.md` for the blob encoding). `hydra_net::eastern` supplies the
//! civil-date arithmetic exactly as the original did.

use crate::levels;
use crate::row::{Pulse, Snapshot, Source};
use hydra_net::eastern;

/// Seconds in a day.
const DAY: i64 = 86_400;

/// Local (Eastern) time of day as `HH:MM:SS`.
///
/// The same inlined sequence appears in `report::stamp` (`0x1012b3a68`),
/// `report::session` and `report::last`: shift by `eastern::offset`, take a
/// Euclidean remainder against 86,400 (`0x5180`), then divide by 3,600 and 60.
fn clock(at: i64) -> String {
    let local = at + eastern::offset(at);
    let second_of_day = local.rem_euclid(DAY);
    format!(
        "{:02}:{:02}:{:02}",
        second_of_day / 3_600,
        (second_of_day % 3_600) / 60,
        second_of_day % 60
    )
}

/// `report::stamp(at: i64) -> String` — `@0x1012b3a68` (**full**).
///
/// `format!("{} {}", eastern::format(&eastern::date(at)), clock)`, i.e.
/// `2026-07-20 13:05:52`. The two format blobs are `0x103e21840`
/// (`"{} {}"`) and `0x10240cbeb` (`"{:02}:{:02}:{:02}"`).
pub fn stamp(at: i64) -> String {
    let date = eastern::date(at);
    format!("{} {}", eastern::format(&date), clock(at))
}

/// `report::bonuses(&Pulse) -> String` — `@0x1012b480c` (**full**).
///
/// The pieces are joined with `" + "` (the separator at `0x10240ce65`, three
/// bytes) and, when the pulse carries no bonus at all, the single word
/// `"none"`. Each piece is one of:
///
/// * `lumnis {n}x`   — `format!("lumnis {}x", rpa)` blob `0x103e64611`,
///   printed when `Pulse + 0x88` (tag) is `1`, value `+0x8c` as `u32`;
/// * `rpa {text}x`   — blob `0x103e6461d`, printed when `Pulse + 0x30`
///   (`Option<String>`) is `Some`;
/// * `fashlonae`     — the nine-byte literal at `0x10240ce5c`, printed when
///   `Pulse + 0x90` is `1` and `Pulse + 0x94` is `2`.
pub fn bonuses(pulse: &Pulse) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(lumnis) = pulse.lumnis {
        parts.push(format!("lumnis {}x", lumnis));
    }
    if let Some(rpa) = &pulse.rpa {
        parts.push(format!("rpa {}x", rpa));
    }
    if pulse.fashlonae == Some(2) {
        parts.push("fashlonae".to_string());
    }
    if parts.is_empty() {
        parts.push("none".to_string());
    }
    parts.join(" + ")
}

/// The totals a set of pulses add up to (`report::hour`/`today`/`session`
/// accumulate these inline before calling [`Totals::said`]).
///
/// Field order and widths are the stack layout the three callers build:
/// `+0x00` total experience gained, `+0x08` total ascension gained, `+0x10`
/// bounty, `+0x18` brooch, `+0x20` instant, `+0x28` pulse count, `+0x30` the
/// latest absorbed experience, `+0x38` the latest absorbed ascension.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Experience gained by every pulse in the range (`+0xb0`, all sources).
    pub exp: i64,
    /// Ascension experience gained (`+0xb8`, all sources).
    pub axp: i64,
    /// Bounty gains: for `source == 1` rows this adds `exp_gained + axp_gained`.
    pub bounty: i64,
    /// Brooch gains: for `source == 2` rows this adds `Pulse + 0xc0`.
    pub brooch: i64,
    /// Instant gains: for `source == 3` rows this adds `Pulse + 0xc0`.
    pub instant: i64,
    /// How many pulses were folded in.
    pub pulses: usize,
    /// The last pulse's absorbed experience.
    pub current_exp: i64,
    /// The last pulse's absorbed ascension experience.
    pub current_axp: i64,
}

impl Totals {
    /// Fold `pulses` into a `Totals`.
    ///
    /// This is the loop that `hour` (`0x1012b02b0`), `today` (`0x1012b02b0`
    /// shape) and `session` (`0x1012b4b14`) each inline: the source byte at
    /// `Pulse + 0xc8` selects the bounty/brooch/instant bucket, everything else
    /// is summed unconditionally, and the absorbed experience is taken from the
    /// last pulse (`+0xa0`/`+0xa8`).
    pub fn of(pulses: &[Pulse]) -> Totals {
        let mut totals = Totals::default();
        for pulse in pulses {
            totals.exp += pulse.exp_gained;
            totals.axp += pulse.axp_gained;
            match pulse.source {
                Source::Bounty => totals.bounty += pulse.exp_gained + pulse.axp_gained,
                Source::Brooch => totals.brooch += pulse.bonus,
                Source::Instant => totals.instant += pulse.bonus,
                _ => {}
            }
        }
        totals.pulses = pulses.len();
        if let Some(last) = pulses.last() {
            totals.current_exp = last.exp;
            totals.current_axp = last.axp;
        }
        totals
    }

    /// `report::Totals::said(&self, name: &str) -> Vec<String>` —
    /// `@0x1012b3da0` (**full**).
    ///
    /// Ten lines, which is why the binary allocates a 240-byte (10 × 24) Vec up
    /// front:
    ///
    /// ```text
    /// {name}:
    ///   Total gained: {exp + axp}
    ///     Normal:     {exp}
    ///     Ascension:  {axp}
    ///   Of which:
    ///     Bounty:     {bounty}
    ///     Brooch:     {brooch}
    ///     Instant:    {instant}
    ///   Pulses:       {pulses}
    ///   Current exp: {current_exp} (axp {current_axp})
    /// ```
    ///
    /// `Of which:` is pushed as a plain literal (the 11-byte string at
    /// `0x10240cdc7`), not through `format!`.
    pub fn said(&self, name: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(10);
        out.push(format!("{}:", name));
        out.push(format!("  Total gained: {}", self.exp + self.axp));
        out.push(format!("    Normal:     {}", self.exp));
        out.push(format!("    Ascension:  {}", self.axp));
        out.push("  Of which:".to_string());
        out.push(format!("    Bounty:     {}", self.bounty));
        out.push(format!("    Brooch:     {}", self.brooch));
        out.push(format!("    Instant:    {}", self.instant));
        out.push(format!("  Pulses:       {}", self.pulses));
        out.push(format!(
            "  Current exp: {} (axp {})",
            self.current_exp, self.current_axp
        ));
        out
    }
}

/// `report::hour(name: &str, pulses: &[Pulse]) -> Vec<String>` —
/// `@0x1012b0280` (**full**).
///
/// Empty input gives the single line `No experience data in the last hour for
/// {name}.` (blob `0x103e64248`, whose 40-byte first piece drops the opening
/// bracket of the C string it is minted from). Otherwise the report is
/// `Totals::said(&format!("Last hour for {name}"))` followed by
/// `  Rate:        {exp} exp/hour` — the "rate" is the range's total experience
/// because the caller passes exactly one hour of pulses.
pub fn hour(name: &str, pulses: &[Pulse]) -> Vec<String> {
    if pulses.is_empty() {
        return vec![format!("No experience data in the last hour for {}.", name)];
    }
    let totals = Totals::of(pulses);
    let mut out = totals.said(&format!("Last hour for {}", name));
    out.push(format!("  Rate:        {} exp/hour", totals.exp));
    out
}

/// `report::today(name: &str, pulses: &[Pulse]) -> Vec<String>` —
/// `@0x1012b3bf4` (**full**).
///
/// Empty input gives `No experience data today for {name}.` (blob
/// `0x103e64465`); otherwise `Totals::said(&format!("Today's totals for {name}
/// (since midnight)"))`.
pub fn today(name: &str, pulses: &[Pulse]) -> Vec<String> {
    if pulses.is_empty() {
        return vec![format!("No experience data today for {}.", name)];
    }
    let totals = Totals::of(pulses);
    totals.said(&format!("Today's totals for {} (since midnight)", name))
}

/// `report::session(name: &str, pulses: &[Pulse]) -> Vec<String>` —
/// `@0x1012b4ae0` (**full**).
///
/// Empty input gives `No session data yet for {name}.` (blob `0x103e64648`);
/// otherwise `Totals::said(&format!("Session report for {name} (since {first
/// pulse clock})"))`, where the clock is `HH:MM:SS` of the *first* pulse.
pub fn session(name: &str, pulses: &[Pulse]) -> Vec<String> {
    if pulses.is_empty() {
        return vec![format!("No session data yet for {}.", name)];
    }
    let totals = Totals::of(pulses);
    let since = clock(pulses[0].at);
    totals.said(&format!("Session report for {} (since {})", name, since))
}

/// `report::level(name, latest: Option<&Pulse>, pulses: &[Pulse], now: i64)
/// -> Vec<String>` — `@0x1012b12fc` (**full**).
///
/// `latest` is `Option<&Pulse>` (`None` = a null pointer, which is the branch
/// that prints `No pulses recorded yet for {name}.`). The lines are:
///
/// * `{name}: {level label}` — `levels::label(latest.exp)` (blob `0x103e21615`);
/// * `  Total exp: {exp} (axp {axp})` (blob `0x103e642d1`);
/// * `  To next level: {n}` (blob `0x103e642eb`) when `latest.until_next` is
///   `Some` and non-zero;
/// * `  ETA at today's rate: ~{eta:.1}h ({rate} exp/hour)` (blob `0x10240ccbf`)
///   when there is at least one pulse, the first pulse is at least 61 seconds
///   old and `rate = sum(exp_gained) * 3600 / (now - first.at)` is at least 1 —
///   `eta = until_next / rate`;
/// * `  Last pulse: {stamp}` (blob `0x103e642ff`), always.
pub fn level(name: &str, latest: Option<&Pulse>, pulses: &[Pulse], now: i64) -> Vec<String> {
    let pulse = match latest {
        Some(pulse) => pulse,
        None => return vec![format!("No pulses recorded yet for {}.", name)],
    };
    let mut out: Vec<String> = Vec::with_capacity(5);
    out.push(format!("{}: {}", name, levels::label(pulse.exp.max(0) as u64)));
    out.push(format!(
        "  Total exp: {} (axp {})",
        pulse.exp, pulse.axp
    ));
    if let Some(until_next) = pulse.until_next {
        out.push(format!("  To next level: {}", until_next));
        if until_next != 0 && !pulses.is_empty() {
            let gained: i64 = pulses.iter().map(|p| p.exp_gained).sum();
            let elapsed = now - pulses[0].at;
            if gained >= 1 && elapsed >= 61 {
                let rate = gained * 3_600 / elapsed;
                if rate >= 1 {
                    let eta = f64::from(until_next) / rate as f64;
                    out.push(format!(
                        "  ETA at today's rate: ~{:.1}h ({} exp/hour)",
                        eta, rate
                    ));
                }
            }
        }
    }
    out.push(format!("  Last pulse: {}", stamp(pulse.at)));
    out
}

/// `report::last(name: &str, pulses: &[Pulse], bonus_only: bool)
/// -> Vec<String>` — `@0x1012b04d0` (**full**).
///
/// `bonus_only` only relabels the report: it selects the 22-byte prefix
/// `"bounty/brooch/instant "` (at `0x10240cb40`) for the title and the empty
/// message, which is what `exp instant` passes after filtering the pulses down
/// to bounty/brooch/instant rows.
///
/// The rows are five columns (`{:<10}` time, `{:>12}` total exp, `{:>8}`
/// gained, `{:>8}` ascension, `{:>9}` type) plus the bonus column:
///
/// ```text
/// Last {n} {prefix}pulses:
/// Time         Total Exp   Gained     Asc      Type Bonuses
/// 13:05:52        1234567     2500      30    normal none
/// ```
///
/// The header literal words are `Time` (`0x102144764`), `Total Exp`
/// (`0x10240cb56`), `Gained` (`0x10240cb5f`), `Asc` (`0x10240cb65`) and `Type`
/// (`0x10220f7dc`); the type column is [`Source::name`] and the last column is
/// [`bonuses`].
pub fn last(name: &str, pulses: &[Pulse], bonus_only: bool) -> Vec<String> {
    let prefix = if bonus_only {
        "bounty/brooch/instant "
    } else {
        ""
    };
    if pulses.is_empty() {
        return vec![format!("No {}entries found for {}.", prefix, name)];
    }
    let mut out: Vec<String> = Vec::with_capacity(2 + pulses.len());
    out.push(format!("Last {} {}pulses:", pulses.len(), prefix));
    out.push(format!(
        "{:<10} {:>12} {:>8} {:>8} {:>9} Bonuses",
        "Time", "Total Exp", "Gained", "Asc", "Type"
    ));
    for pulse in pulses {
        out.push(format!(
            "{:<10} {:>12} {:>8} {:>8} {:>9} {}",
            clock(pulse.at),
            pulse.exp,
            pulse.exp_gained,
            pulse.axp_gained,
            pulse.source.name(),
            bonuses(pulse)
        ));
    }
    out
}

/// One day's totals inside [`daily`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Day {
    date: eastern::Date,
    exp: i64,
    axp: i64,
    bounty: i64,
    brooch: i64,
    instant: i64,
    pulses: usize,
}

impl Day {
    fn new(date: eastern::Date) -> Day {
        Day {
            date,
            exp: 0,
            axp: 0,
            bounty: 0,
            brooch: 0,
            instant: 0,
            pulses: 0,
        }
    }
}

/// `report::daily(pulses: &[Pulse], now: i64) -> Vec<String>` —
/// `@0x1012b09b8` (**full**).
///
/// Groups the pulses by Eastern date (a linear scan for an equal
/// `(year, month, day)` triple — no map), sorts the days, keeps the **last ten**
/// (`cmp x8, #0xa`), and prints them oldest→newest plus a totals row and a
/// legend:
///
/// ```text
/// Date              Total    Normal       Asc    Bounty    Brooch   Instant Pulses
/// 2026-07-20*      123456   120000      3456         0         0         0     42
/// Total:           123456
/// * = Today
/// ```
///
/// The `*` marks today's row (the literal `*` is the first byte of the
/// `*Total:` pool entry at `0x10240cc9e`); the `* = Today` line (9-byte literal
/// at `0x10240ccb6`) is only emitted when today is among the ten days.
/// Columns: `{:<12}` Date, `{:>10}` Total, `{:>10}` Normal, `{:>10}` Ascension,
/// `{:>10}` Bounty, `{:>10}` Brooch, `{:>10}` Instant, `{:>6}` Pulses.
pub fn daily(pulses: &[Pulse], now: i64) -> Vec<String> {
    if pulses.is_empty() {
        return vec!["No experience data found for the last 10 days.".to_string()];
    }
    let mut days: Vec<Day> = Vec::new();
    for pulse in pulses {
        let date = eastern::date(pulse.at);
        let index = match days.iter().position(|day| day.date == date) {
            Some(index) => index,
            None => {
                days.push(Day::new(date));
                days.len() - 1
            }
        };
        let day = &mut days[index];
        day.exp += pulse.exp_gained;
        day.axp += pulse.axp_gained;
        day.pulses += 1;
        match pulse.source {
            Source::Bounty => day.bounty += pulse.exp_gained + pulse.axp_gained,
            Source::Brooch => day.brooch += pulse.bonus,
            Source::Instant => day.instant += pulse.bonus,
            _ => {}
        }
    }
    days.sort_by_key(|day| day.date);
    if days.len() > 10 {
        days.drain(..days.len() - 10);
    }
    let today = eastern::date(now);
    let mut out: Vec<String> = Vec::with_capacity(days.len() + 3);
    out.push(format!(
        "{:<12} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>6}",
        "Date", "Total", "Normal", "Asc", "Bounty", "Brooch", "Instant", "Pulses"
    ));
    let mut running = 0i64;
    for day in &days {
        running += day.exp + day.axp;
        let star = if day.date == today { "*" } else { "" };
        out.push(format!(
            "{:<12} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10} {:>6}",
            format!("{}{}", eastern::format(&day.date), star),
            day.exp + day.axp,
            day.exp,
            day.axp,
            day.bounty,
            day.brooch,
            day.instant,
            day.pulses
        ));
    }
    out.push(format!("{:<12} {:>10}", "Total:", running));
    if days.iter().any(|day| day.date == today) {
        out.push("* = Today".to_string());
    }
    out
}

/// Gap limit for the timing analysis: 90 seconds.
///
/// The static `i64` at `0x10240cf28` in `__const` is `0x5a`, and the filter in
/// the gap closure (`0x1012bc678`) keeps only `diff < 0x5b`.
pub const TIMING_GAP_LIMIT: i64 = 90;

/// The timestamps the timing analysis uses: normal pulses that gained
/// experience (`filter(|p| p.source == 0 && p.exp_gained > 0)` at
/// `0x1012bcbc4`).
fn timing_stamps(pulses: &[Pulse]) -> Vec<i64> {
    pulses
        .iter()
        .filter(|p| p.source == Source::Normal && p.exp_gained > 0)
        .map(|p| p.at)
        .collect()
}

/// `report::timing(pulses: &[Pulse]) -> Vec<String>` — `@0x1012b41f0`
/// (**full**).
///
/// Fewer than two pulses gives the single 48-byte line `Not enough pulse data
/// found for timing analysis.` (`0x10240ce2c`). Otherwise the gaps between
/// consecutive normal, positive-gain pulses are taken (`|a - b|`, keeping only
/// gaps below [`TIMING_GAP_LIMIT`]) and summarised in seven lines:
///
/// ```text
/// Pulse timing analysis (last {pulses} normal pulses):
///   Min time between pulses: {min} seconds
///   Max time between pulses: {max} seconds
///   Average time: {avg:.1} seconds
///   Median time: {gaps[gaps.len() / 2]} seconds
///   Pulses analyzed: {gaps}
///   (Excluded: bounty, brooch, instant, manual, gaps >{90}s)
/// ```
///
/// Two details are odd but literal: the median is the *upper* middle element
/// (`gaps[len / 2]`, no averaging for even lengths) and "Pulses analyzed"
/// counts the gaps, not the pulses (the value formatted there is the gap-vector
/// length at `sp + 0x68`). If no gap survives the filter, the second line is
/// replaced by the single 56-byte line `Not enough consecutive pulses found for
/// timing analysis.` and nothing follows it.
pub fn timing(pulses: &[Pulse]) -> Vec<String> {
    if pulses.len() < 2 {
        return vec!["Not enough pulse data found for timing analysis.".to_string()];
    }
    let stamps = timing_stamps(pulses);
    let mut gaps: Vec<i64> = stamps
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .filter(|gap| *gap < TIMING_GAP_LIMIT + 1)
        .collect();
    let mut out: Vec<String> = Vec::with_capacity(7);
    out.push(format!(
        "Pulse timing analysis (last {} normal pulses):",
        pulses.len()
    ));
    if gaps.is_empty() {
        out.push("Not enough consecutive pulses found for timing analysis.".to_string());
        return out;
    }
    gaps.sort_unstable();
    let count = gaps.len();
    let sum: i64 = gaps.iter().sum();
    out.push(format!("  Min time between pulses: {} seconds", gaps[0]));
    out.push(format!(
        "  Max time between pulses: {} seconds",
        gaps[count - 1]
    ));
    out.push(format!(
        "  Average time: {:.1} seconds",
        sum as f64 / count as f64
    ));
    out.push(format!("  Median time: {} seconds", gaps[count / 2]));
    out.push(format!("  Pulses analyzed: {}", count));
    out.push(format!(
        "  (Excluded: bounty, brooch, instant, manual, gaps >{}s)",
        TIMING_GAP_LIMIT
    ));
    out
}

/// `report::range(name: &str, pulses: &[Pulse], from: i64, to: i64)
/// -> Vec<String>` — `@0x1012b18a4` (**partly**).
///
/// Every format string below is recovered exactly (blobs under
/// `0x103e64330`…`0x103e643d6` and `0x10240cff9`…`0x10240cd58`), and so is the
/// shape of the aggregates (they reuse the same `source`-keyed buckets as
/// [`Totals`] plus the [`timing`] gap logic). What is **not** recovered is the
/// exact third argument of `Ending Exp:   {} (axp {}) - {}` (a string, printed
/// after the axp figure) — this build prints the ending level label there and
/// marks it `// UNKNOWN:`.
pub fn range(name: &str, pulses: &[Pulse], from: i64, to: i64) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(24);
    out.push(format!("Period: {} to {}", stamp(from), stamp(to)));
    if pulses.is_empty() {
        // The binary prints the period and then nothing else for an empty span;
        // the only "no data" literal in the crate belongs to `snapshots`.
        return out;
    }
    let first = &pulses[0];
    let last = &pulses[pulses.len() - 1];
    let totals = Totals::of(pulses);
    out.push(format!("Experience Report for {}", name));
    out.push(format!("Starting Exp: {} (axp {})", first.exp, first.axp));
    // UNKNOWN: the last placeholder is a string the binary builds elsewhere;
    // the safest reading (it follows the ending axp figure) is the level label.
    out.push(format!(
        "Ending Exp:   {} (axp {}) - {}",
        last.exp,
        last.axp,
        levels::label(last.exp.max(0) as u64)
    ));
    let first_level = levels::label(first.exp.max(0) as u64);
    let last_level = levels::label(last.exp.max(0) as u64);
    let gained_levels = last_level
        .split(' ')
        .nth(1)
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0)
        - first_level
            .split(' ')
            .nth(1)
            .and_then(|n| n.parse::<i64>().ok())
            .unwrap_or(0);
    out.push(format!(
        "Levels Gained: {} ({} -> {})",
        gained_levels, first_level, last_level
    ));
    out.push(format!(
        "Total Gained: {} exp, {} ascension",
        totals.exp, totals.axp
    ));
    let total_gain = (totals.exp + totals.axp).max(1);
    let percent = |value: i64| value as f64 * 100.0 / total_gain as f64;
    out.push(format!(
        "  Bounty:   {} ({:.2}%)",
        totals.bounty,
        percent(totals.bounty)
    ));
    out.push(format!(
        "  Brooch:   {} ({:.2}%)",
        totals.brooch,
        percent(totals.brooch)
    ));
    out.push(format!(
        "  Instant:  {} ({:.2}%)",
        totals.instant,
        percent(totals.instant)
    ));
    for source in Source::NAMES {
        let rows: Vec<&Pulse> = pulses
            .iter()
            .filter(|p| p.source.name() == source)
            .collect();
        if rows.is_empty() {
            continue;
        }
        let exp: i64 = rows.iter().map(|p| p.exp_gained).sum();
        out.push(format!(
            "  {:<24} {:>6} pulses ({:>5.1}%)  {:>9} exp",
            source,
            rows.len(),
            rows.len() as f64 * 100.0 / pulses.len() as f64,
            exp
        ));
    }
    // Intervals: the same stamps/gaps as `timing` (normal, positive-gain pulses,
    // gaps below the 90 s limit).
    let stamps = timing_stamps(pulses);
    let mut gaps: Vec<i64> = stamps
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .filter(|gap| *gap < TIMING_GAP_LIMIT + 1)
        .collect();
    if !gaps.is_empty() {
        gaps.sort_unstable();
        let count = gaps.len();
        let sum: i64 = gaps.iter().sum();
        out.push(format!("  Min interval: {} seconds", gaps[0]));
        out.push(format!("  Max interval: {} seconds", gaps[count - 1]));
        out.push(format!(
            "  Avg interval: {:.1} seconds",
            sum as f64 / count as f64
        ));
        let active_seconds = sum;
        out.push(format!(
            "  Active Time:  {}h {}m (gaps >{}s excluded)",
            active_seconds / 3_600,
            (active_seconds % 3_600) / 60,
            TIMING_GAP_LIMIT
        ));
        if active_seconds >= 1 {
            let rate = totals.exp * 3_600 / active_seconds;
            out.push(format!("  Rate:         {} exp/hour (active)", rate));
            out.push(format!("  Total Pulses: {}", totals.pulses));
            if let Some(until_next) = last.until_next {
                if rate >= 1 {
                    out.push(format!(
                        "  Next Level:   {} exp to go (~{:.1}h at this rate)",
                        until_next,
                        f64::from(until_next) / rate as f64
                    ));
                }
            }
        }
    }
    // UNKNOWN: the two "fullness" lines average a 0..=100 integer column of the
    // pulse (the binary filters values with `cmp x10, #0x63`, i.e. keeps <= 99,
    // and divides the `f64` sum by the count) and add the share of pulses that
    // hit the ceiling. The row offsets of those columns were not established.
    out
}

/// The three snapshot kinds [`snapshots`] can print (labels at `0x10442db08` /
/// `0x10240cf10`): `favor`, `resource`, `suffused`.
pub const SNAPSHOT_KINDS: [&str; 3] = ["favor", "resource", "suffused"];

/// `report::snapshots(which: u8, snaps: &[Snapshot], today: &[Snapshot])
/// -> Vec<String>` — `@0x1012b4d94` (**full**).
///
/// `which` selects the kind ([`SNAPSHOT_KINDS`], used both as the label and as
/// the filter byte the first closure (`0x1012bcf64`) compares against
/// `Snapshot + 0x88`). The second slice is *today's* snapshots: the second
/// closure (`0x1012bcd1c`) walks it looking for rows of the same kind **and the
/// same label**, collects their values, and the loop then sorts them and prints
/// `max - min` as the day's movement.
///
/// Two lines per snapshot, and `No {kind} data captured yet.` (`0x103e64665`)
/// when nothing matches:
///
/// ```text
/// Voln Favor: 1234 (as of 2026-07-20 13:05:52)              // which = 0
///   Favor movement today: +234
/// Resource: 12000/50,000 weekly, 240000/200,000 total (as of …)   // which = 1
///   Resource movement today: +1000
/// Suffused Essence: 5000 (as of …)                          // which = 2
///   Suffused Essence movement today: +300
/// ```
///
/// The labels are the three blobs `0x103e6467f`, `0x103e6469a`, `0x103e646cc`
/// plus `0x10240cec5` (`"  {} movement today: {:+}"`). A snapshot without a
/// label prints `"Resource"` (`0x10213c7c8`) for `resource` and `"resource"`
/// (`0x10206b390`) for `suffused`; the movement label is the five-byte literal
/// `"Favor"` for `favor` and the snapshot's own label otherwise (empty when it
/// has none).
pub fn snapshots(which: u8, snaps: &[Snapshot], today: &[Snapshot]) -> Vec<String> {
    let kind = SNAPSHOT_KINDS
        .get(which as usize)
        .copied()
        .unwrap_or("experience");
    let matching: Vec<&Snapshot> = snaps.iter().filter(|snap| snap.kind == which).collect();
    if matching.is_empty() {
        return vec![format!("No {} data captured yet.", kind)];
    }
    let mut out: Vec<String> = Vec::with_capacity(matching.len() * 2);
    for snap in matching {
        let as_of = stamp(snap.at);
        match kind {
            "favor" => out.push(format!(
                "Voln Favor: {} (as of {})",
                snap.value.unwrap_or_default(),
                as_of
            )),
            "resource" => out.push(format!(
                "{}: {}/50,000 weekly, {}/200,000 total (as of {})",
                snap.label.as_deref().unwrap_or("Resource"),
                snap.weekly.unwrap_or_default(),
                snap.total.unwrap_or_default(),
                as_of
            )),
            _ => out.push(format!(
                "Suffused {}: {} (as of {})",
                snap.label.as_deref().unwrap_or("resource"),
                snap.value.unwrap_or_default(),
                as_of
            )),
        }
        let mut values: Vec<i64> = today
            .iter()
            .filter(|row| row.kind == which && row.label == snap.label)
            .map(|row| row.value.unwrap_or_default())
            .collect();
        values.sort_unstable();
        let movement = match (values.first(), values.last()) {
            (Some(first), Some(last)) => last - first,
            _ => 0,
        };
        let movement_label = if which == 0 {
            "Favor".to_string()
        } else {
            snap.label.clone().unwrap_or_default()
        };
        out.push(format!(
            "  {} movement today: {:+}",
            movement_label, movement
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Pulse;

    /// 2026-07-20 13:05:52 Eastern (EDT, UTC-4 == 17:05:52 UTC).
    const T: i64 = 1_784_567_152;

    fn pulse(at: i64, exp: i64, gained: i64) -> Pulse {
        Pulse {
            at,
            exp,
            axp: 30,
            exp_gained: gained,
            axp_gained: 0,
            source: Source::Normal,
            ..Pulse::default()
        }
    }

    #[test]
    fn stamp_is_date_space_clock() {
        // The clock is local time: `T` is 17:05:52 UTC.
        assert_eq!(stamp(T), "2026-07-20 13:05:52");
        assert_eq!(stamp(T + 86_400), "2026-07-21 13:05:52");
    }

    #[test]
    fn totals_fold_sources_into_their_buckets() {
        let pulses = vec![
            pulse(T, 1_000, 500),
            Pulse {
                at: T + 60,
                exp: 1_000,
                exp_gained: 200,
                axp_gained: 40,
                source: Source::Bounty,
                ..Pulse::default()
            },
            Pulse {
                at: T + 120,
                exp: 1_000,
                bonus: 75,
                source: Source::Brooch,
                ..Pulse::default()
            },
            Pulse {
                at: T + 180,
                exp: 1_000,
                bonus: 25,
                source: Source::Instant,
                ..Pulse::default()
            },
        ];
        let totals = Totals::of(&pulses);
        assert_eq!(totals.exp, 700);
        assert_eq!(totals.axp, 40);
        assert_eq!(totals.bounty, 240);
        assert_eq!(totals.brooch, 75);
        assert_eq!(totals.instant, 25);
        assert_eq!(totals.pulses, 4);
        assert_eq!(totals.current_exp, 1_000);
    }

    #[test]
    fn said_prints_ten_lines() {
        let totals = Totals {
            exp: 700,
            axp: 40,
            bounty: 240,
            brooch: 75,
            instant: 25,
            pulses: 4,
            current_exp: 1_000,
            current_axp: 30,
        };
        let lines = totals.said("Sindar");
        assert_eq!(lines.len(), 10);
        assert_eq!(
            lines,
            vec![
                "Sindar:",
                "  Total gained: 740",
                "    Normal:     700",
                "    Ascension:  40",
                "  Of which:",
                "    Bounty:     240",
                "    Brooch:     75",
                "    Instant:    25",
                "  Pulses:       4",
                "  Current exp: 1000 (axp 30)",
            ]
        );
    }

    #[test]
    fn hour_without_data_names_the_window() {
        assert_eq!(
            hour("Sindar", &[]),
            vec!["No experience data in the last hour for Sindar."]
        );
        assert_eq!(
            today("Sindar", &[]),
            vec!["No experience data today for Sindar."]
        );
        assert_eq!(
            session("Sindar", &[]),
            vec!["No session data yet for Sindar."]
        );
    }

    #[test]
    fn hour_titles_the_totals_and_adds_the_rate() {
        let pulses = vec![pulse(T, 1_000, 500), pulse(T + 60, 1_200, 200)];
        let lines = hour("Sindar", &pulses);
        assert_eq!(lines[0], "Last hour for Sindar:");
        assert_eq!(lines[1], "  Total gained: 700");
        assert_eq!(lines.len(), 11);
        assert_eq!(lines[10], "  Rate:        700 exp/hour");
    }

    #[test]
    fn today_and_session_use_their_own_headers() {
        let pulses = vec![pulse(T, 1_000, 500)];
        assert_eq!(
            today("Sindar", &pulses)[0],
            "Today's totals for Sindar (since midnight):"
        );
        assert_eq!(
            session("Sindar", &pulses)[0],
            "Session report for Sindar (since 13:05:52):"
        );
    }

    #[test]
    fn level_prints_the_label_and_the_eta() {
        let pulses = vec![pulse(T, 3_750, 900), pulse(T + 3_600, 3_900, 1_000)];
        let latest = Pulse {
            at: T + 3_600,
            exp: 3_900,
            axp: 44,
            until_next: Some(1_100),
            ..Pulse::default()
        };
        let lines = level("Sindar", Some(&latest), &pulses, T + 3_600);
        assert_eq!(lines[0], "Sindar: level 1 (56.0% through)");
        assert_eq!(lines[1], "  Total exp: 3900 (axp 44)");
        assert_eq!(lines[2], "  To next level: 1100");
        // 900 + 1000 exp in one hour -> 1900 exp/hour, eta = 1100/1900 = 0.6 h.
        assert_eq!(
            lines[3],
            "  ETA at today's rate: ~0.6h (1900 exp/hour)"
        );
        assert_eq!(lines[4], "  Last pulse: 2026-07-20 14:05:52");
        assert_eq!(level("Sindar", None, &[], T)[0], "No pulses recorded yet for Sindar.");
    }

    #[test]
    fn last_prints_a_header_and_one_row_per_pulse() {
        let pulses = vec![
            Pulse {
                at: T,
                exp: 1_234_567,
                exp_gained: 2_500,
                axp_gained: 30,
                source: Source::Normal,
                ..Pulse::default()
            },
            Pulse {
                at: T - 60,
                exp: 1_234_000,
                lumnis: Some(2),
                source: Source::Bounty,
                ..Pulse::default()
            },
        ];
        let lines = last("Sindar", &pulses, false);
        assert_eq!(lines[0], "Last 2 pulses:");
        assert_eq!(
            lines[1],
            "Time          Total Exp   Gained      Asc      Type Bonuses"
        );
        assert_eq!(
            lines[2],
            "13:05:52        1234567     2500       30    normal none"
        );
        assert_eq!(
            lines[3],
            "13:04:52        1234000        0        0    bounty lumnis 2x"
        );
        assert_eq!(
            last("Sindar", &[], true),
            vec!["No bounty/brooch/instant entries found for Sindar."]
        );
    }

    #[test]
    fn bonuses_join_with_plus() {
        let mut pulse = Pulse::default();
        assert_eq!(bonuses(&pulse), "none");
        pulse.lumnis = Some(3);
        assert_eq!(bonuses(&pulse), "lumnis 3x");
        pulse.rpa = Some("1.25".to_string());
        assert_eq!(bonuses(&pulse), "lumnis 3x + rpa 1.25x");
        pulse.fashlonae = Some(2);
        assert_eq!(bonuses(&pulse), "lumnis 3x + rpa 1.25x + fashlonae");
    }

    #[test]
    fn timing_reports_gaps_below_ninety_seconds() {
        let ats = [T, T + 60, T + 120, T + 400];
        let pulses: Vec<Pulse> = ats.iter().map(|at| pulse(*at, 1_000, 100)).collect();
        let lines = timing(&pulses);
        assert_eq!(lines[0], "Pulse timing analysis (last 4 normal pulses):");
        assert_eq!(lines[1], "  Min time between pulses: 60 seconds");
        assert_eq!(lines[2], "  Max time between pulses: 60 seconds");
        assert_eq!(lines[3], "  Average time: 60.0 seconds");
        assert_eq!(lines[4], "  Median time: 60 seconds");
        // Two gaps survive; the 280 s gap is excluded.
        assert_eq!(lines[5], "  Pulses analyzed: 2");
        assert_eq!(
            lines[6],
            "  (Excluded: bounty, brooch, instant, manual, gaps >90s)"
        );
        assert_eq!(
            timing(&pulses[..1]),
            vec!["Not enough pulse data found for timing analysis."]
        );
    }

    #[test]
    fn snapshots_prints_one_as_of_and_one_movement_line_per_row() {
        use crate::row::Snapshot;
        // 05:05:52 and 13:05:52 Eastern on the same day: +234 of Voln favor.
        let today = vec![
            Snapshot {
                value: Some(1_000),
                at: T - 8 * 3_600,
                kind: 0,
                ..Snapshot::default()
            },
            Snapshot {
                value: Some(1_234),
                at: T,
                kind: 0,
                ..Snapshot::default()
            },
        ];
        let lines = snapshots(0, &today[1..], &today);
        assert_eq!(
            lines,
            vec![
                "Voln Favor: 1234 (as of 2026-07-20 13:05:52)",
                "  Favor movement today: +234",
            ]
        );
        // No rows of this kind at all.
        assert_eq!(
            snapshots(1, &today, &today),
            vec!["No resource data captured yet."]
        );
    }

    #[test]
    fn snapshots_resource_and_suffused_use_their_label_or_the_default() {
        use crate::row::Snapshot;
        let owned = vec![Snapshot {
            value: Some(5_000),
            weekly: Some(12_000),
            total: Some(240_000),
            label: Some("Guild".to_string()),
            at: T,
            kind: 1,
        }];
        assert_eq!(
            snapshots(1, &owned, &owned)[0],
            "Guild: 12000/50,000 weekly, 240000/200,000 total (as of 2026-07-20 13:05:52)"
        );
        assert_eq!(snapshots(1, &owned, &owned)[1], "  Guild movement today: +0");
        // Without a label the resource row says "Resource", the suffused row
        // "resource" (the two fallback literals differ in case on purpose).
        let bare = vec![Snapshot {
            value: Some(7),
            at: T,
            kind: 1,
            ..Snapshot::default()
        }];
        assert!(snapshots(1, &bare, &bare)[0].starts_with("Resource: 0/50,000 weekly"));
        let suffused = vec![Snapshot {
            value: Some(7),
            at: T,
            kind: 2,
            ..Snapshot::default()
        }];
        assert_eq!(
            snapshots(2, &suffused, &suffused)[0],
            "Suffused resource: 7 (as of 2026-07-20 13:05:52)"
        );
    }

    #[test]
    fn daily_groups_by_eastern_date_and_marks_today() {
        // 13:05 and 23:00 on 2026-07-20, then 09:00 on 2026-07-21.
        let pulses = vec![
            pulse(T, 1_000, 500),
            pulse(T + 9_000, 1_200, 300),
            Pulse {
                at: T + 71_000,
                exp: 1_500,
                exp_gained: 700,
                source: Source::Brooch,
                bonus: 50,
                ..Pulse::default()
            },
        ];
        let lines = daily(&pulses, T + 9_000);
        assert_eq!(
            lines[0],
            "Date              Total     Normal        Asc     Bounty     Brooch    Instant Pulses"
        );
        assert_eq!(
            lines[1],
            "2026-07-20*         800        800          0          0          0          0      2"
        );
        assert_eq!(
            lines[2],
            "2026-07-21          700        700          0          0         50          0      1"
        );
        assert_eq!(lines[3], "Total:             1500");
        assert_eq!(lines[4], "* = Today");
        assert_eq!(
            daily(&[], T),
            vec!["No experience data found for the last 10 days."]
        );
    }
}
