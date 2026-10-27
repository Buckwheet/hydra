//! `rewards.toml` — when the login reward is claimed, and which characters are
//! left alone.
//!
//! Recovered from `hydra_rewards::schedule::{Settings::{read, parse, write,
//! to_toml, today, today_now}, Character::skipped}` (`0x1014c857c`,
//! `0x1014c87c8`, `0x1014c92b8`, `0x1014c946c`, `0x1014c8d08`, `0x1014c9648`,
//! `0x1014c9c24`) and the serde `Raw` visitor (`0x1014db690`).
//!
//! The file is `<data dir>/rewards.toml` (`Path::join` with the twelve-byte
//! literal at `0x10241aef5`) and holds three keys — the serde visitor's field
//! names are `time`, `skip` and `store` (the `struct Raw` expecting at
//! `0x10241af27`, the three names at `0x10241aee8` … `0x10241aef5`):
//!
//! ```toml
//! time = "19:15"
//! skip = ["Aelith", "GS4:Bob"]
//! store = true
//! ```
//!
//! `time` is a `hh:mm` string (`to_toml` builds it with the `{:02}:{:02}`
//! blob at `0x10241af01`), `skip` is the list of characters not to claim for
//! and `store` turns the store claim on.
//!
//! **Phase 1 reconnection**: the document is read and written by `serde` +
//! `toml` ([`crate::toml`]) and the claim time is resolved through `jiff`'s
//! time-zone database, at the pinned crate versions.  Two of the recovered
//! signatures differ from the public ones this crate keeps for its siblings:
//!
//! * `Settings::today` is `(&self, now: &jiff::Zoned) -> Option<jiff::Timestamp>`
//!   — kept as [`Settings::today_at`], with the no-argument
//!   [`Settings::today`] answering unix seconds;
//! * `Settings::today_now` answers a `jiff::Zoned` — kept as [`zoned_now`],
//!   with [`Settings::today_now`] answering unix seconds.

use std::path::Path;
use std::sync::OnceLock;

use jiff::civil::Date as CivilDate;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

use crate::row::Who;
use crate::toml::{self as manifest, Raw};
use hydra_net::eastern;
use hydra_session::store::save_bytes;

/// The game's time zone, from the zone database.
///
/// `America/New_York`, with `EST5EDT` (the same modern rule, without the
/// history) as the fallback `hydra_net::eastern` uses; `None` on a machine
/// with no zone database at all, which is the only case the `std`-only
/// arithmetic in [`hydra_net::eastern`] still answers.
fn game_zone() -> Option<TimeZone> {
    static ZONE: OnceLock<Option<TimeZone>> = OnceLock::new();
    ZONE.get_or_init(|| {
        TimeZone::get("America/New_York")
            .ok()
            .or_else(|| TimeZone::get("EST5EDT").ok())
    })
    .clone()
}

/// Now, as the `jiff::Zoned` the recovered `Settings::today` takes.
///
/// **Reconnected**: `Settings::today_now() -> jiff::Zoned` — `@0x1014c9648`
/// is `jiff::now::sys::system_time` (`0x101c8be40`), then
/// `SystemTime::duration_since(UNIX_EPOCH)`, `SignedDuration::try_from`, and
/// the zone from `jiff::tz::db`/`jiff::tz::system::get` — i.e. `Zoned::now()`.
/// The zone is the game's here, not the machine's: the setting is the game's
/// wall clock and every report quotes that clock
/// (`hydra_net::eastern`).  **UNKNOWN**: whether the original asked for the
/// system zone or an Eastern one — the recovered `today_now` is the inlined
/// `Zoned::now()`, which is the system zone, and the only recovered callers
/// are `hydra_table::reward_claim::run`'s task (`0x10006edd8`, inside the
/// closure at `0x10006e9c0`), which passes it straight to `today`.
pub fn zoned_now() -> Zoned {
    Timestamp::now().to_zoned(game_zone().unwrap_or(TimeZone::UTC))
}

/// The file's name (`0x10241aef5`, twelve bytes).
pub const FILE: &str = "rewards.toml";

/// The minutes-in-a-day of a `time` value, in [`Settings::time`].
pub const MINUTES: u32 = 24 * 60;

/// One entry of the `skip` list: a character's name, or `GAME:Name`.
///
/// **ESTABLISHED**: 0x18 bytes — one `String` — because the skip list is a
/// `Vec<Character>` (`drop_in_place<Vec<hydra_rewards::schedule::Character>>`
/// at `0x100057fdc`) and `Character::skipped` walks it with a `0x18` stride
/// (`madd x22, x2, x8(=0x18), x1` at `0x1014c9c54`, `add x19, x19, #0x18` at
/// `0x1014c9d40`) comparing the `String`'s `(ptr, len)` with `memcmp`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Character(pub String);

impl Character {
    pub fn new(name: impl Into<String>) -> Character {
        Character(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Is `who` on this list?
    ///
    /// `schedule::Character::skipped(who: &Who, skip: &[Character]) -> bool` —
    /// `@0x1014c9c24` (**full**).
    ///
    /// ```text
    /// let skip = [Character::new("aelith"), Character::new("GS4:Bob")];
    /// Character::skipped(&Who::new("GS4", "Aelith"), &skip) == true
    /// Character::skipped(&Who::new("GS4", "Bob"), &skip)    == true
    /// Character::skipped(&Who::new("GS4", "Cara"), &skip)   == false
    /// ```
    ///
    /// An empty list never skips anybody (`cbz x2, …` at `0x1014c9c24`
    /// answers `false`).  Each entry is compared case-insensitively (the
    /// ASCII fold `sub w13, w11, #0x41` / `cmp w13, #0x1a` / `csel w13, w26,
    /// wzr, lo` / `orr`) with `who.character` first (`0x1014c9c70`:
    /// `ldp x28, x25, [x19, #0x8]` against `ldp x24, x23, [x0, #0x20]`) and,
    /// when that fails, with `format!("{}:{}", who.instance, who.character)`
    /// (the `"{}:{}"` blob at `0x103e21670`, `0x1014c9cd8`).
    pub fn skipped(who: &Who, skip: &[Character]) -> bool {
        skip.iter().any(|entry| {
            entry.0.eq_ignore_ascii_case(&who.character)
                || entry.0.eq_ignore_ascii_case(&who.tag())
        })
    }
}

/// The claim schedule.
///
/// **ESTABLISHED**: 0x28 bytes.
///
/// | offset | field | evidence |
/// |---|---|---|
/// | `+0x00` | [`Settings::skip`] | `Vec::clone` from `x0` in `to_toml` (`0x1014c94f8`) |
/// | `+0x18`/`+0x1c` | [`Settings::time`] | the tag is tested at `0x1014c9488` (`cmp w8, #0x1`) and the minutes divided by 60 at `0x1014c9494` |
/// | `+0x20` | [`Settings::store`] | `ldrb w8, [x20, #0x20]` in `to_toml` (`0x1014c94fc`) and `write` (`0x1014c9300`) |
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Characters not to claim for.
    pub skip: Vec<Character>,
    /// Minutes from midnight at which to claim, on the game's clock.
    pub time: Option<u32>,
    /// Whether the store's SimuCoins are claimed too.
    pub store: bool,
}

impl Settings {
    /// The TOML keys, in the order the serde visitor has them
    /// (`0x10241aee8` … `0x10241aef5`).
    pub const KEYS: [&'static str; 3] = ["time", "skip", "store"];

    /// Parse a `rewards.toml`.
    ///
    /// `Settings::parse(text: &str) -> Result<Settings, String>` —
    /// `@0x1014c87c8` (**full**).
    ///
    /// The document is read as `Raw { time: Option<String>, skip:
    /// Vec<String>, store: bool }` by `serde` + `toml` (the `Raw` visitor at
    /// `0x1014db690`, now [`crate::toml::Raw`]), then `time` is parsed as
    /// `hh:mm` by hand — the `:` is found with `slice::memchr`
    /// (`0x1014c8958`), both halves are folded as integers with a sign
    /// (`cmp w8, #0x2b`/`#0x2d` at `0x1014c89b0`), the minutes must be under
    /// `60` (`cmp w10, #0x3c` at `0x1014c8be0`) and a value it cannot read
    /// answers `` `{}` is not a time of day: say it as 19:05. `` (the blob at
    /// `0x103e6b629`).
    ///
    /// ```text
    /// parse("time = \"19:15\"\nskip = [\"Aelith\"]\nstore = true\n").unwrap()
    ///     == Settings { skip: [Character("Aelith")], time: Some(19*60+15), store: true }
    /// ```
    pub fn parse(text: &str) -> Result<Settings, String> {
        let raw = manifest::parse(text)?;
        let mut settings = Settings::default();
        if let Some(time) = raw.time.as_deref() {
            settings.time = Some(parse_time(time)?);
        }
        settings.skip = raw
            .skip
            .iter()
            .map(|s| Character::new(s.trim()))
            .filter(|c| !c.0.is_empty())
            .collect();
        settings.store = raw.store;
        Ok(settings)
    }

    /// Read `<dir>/rewards.toml`.
    ///
    /// `Settings::read(dir: &Path) -> Result<Option<Settings>, String>` —
    /// `@0x1014c857c` (**full**).
    ///
    /// * the file is joined onto `dir` with the twelve-byte literal
    ///   `rewards.toml` (`0x1014c8598`), then `fs::read_to_string`
    ///   (`0x1014c85bc`);
    /// * `ErrorKind::NotFound` (`tst w0, #0xff` at `0x1014c85d8`) answers
    ///   `Ok(None)` — the "no schedule" niche word written at `0x1014c871c`;
    /// * any other I/O error, and a parse error, answer
    ///   `Err(format!("{}: {}", path, error))` (the `{}: {}` blob at
    ///   `0x103e21615`).
    pub fn read(dir: &Path) -> Result<Option<Settings>, String> {
        let path = dir.join(FILE);
        match std::fs::read_to_string(&path) {
            Ok(text) => match Settings::parse(&text) {
                Ok(settings) => Ok(Some(settings)),
                Err(e) => Err(format!("{}: {}", path.display(), e)),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {}", path.display(), e)),
        }
    }

    /// The file's text.
    ///
    /// `Settings::to_toml(&self) -> String` — `@0x1014c946c` (**full**).
    ///
    /// The `time` is rendered `{:02}:{:02}` (`0x10241af01`) from
    /// `minutes / 60` and `minutes % 60` (`0x1014c94a0` divides by 60 with the
    /// `0x88888889` magic), the skip list is cloned as-is and the `store` flag
    /// appended, then the lot is serialised by `toml::to_string`
    /// (`0x1014c952c`, now [`crate::toml::to_toml`]; the error arm at
    /// `0x1014c9538` answers an empty string, which is what this returns).
    pub fn to_toml(&self) -> String {
        let raw = Raw {
            time: self
                .time
                .map(|minutes| format!("{:02}:{:02}", minutes / 60, minutes % 60)),
            skip: self.skip.iter().map(|c| c.0.clone()).collect(),
            store: self.store,
        };
        manifest::to_toml(&raw)
    }

    /// Write `<dir>/rewards.toml`, or delete it when nothing is set.
    ///
    /// `Settings::write(&self, dir: &Path) -> std::io::Result<()>` —
    /// `@0x1014c92b8` (**full**).
    ///
    /// When `time` is `None` *and* `store` is `false` (`0x1014c92f8`,
    /// `0x1014c9300`) the file is removed instead of written
    /// (`std::sys::fs::remove_file` at `0x1014c9364`), and a
    /// `NotFound` from that removal is not an error (`tst w0, #0xff` at
    /// `0x1014c9378`).  Otherwise the text goes through
    /// `hydra_session::store::save_bytes` (`0x1014c9334`) — the durable
    /// write Hydra uses for every settings file.
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        let path = dir.join(FILE);
        if self.time.is_none() && !self.store {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            };
        }
        save_bytes(dir, &path, self.to_toml().as_bytes())
    }

    /// Now, unix seconds.
    ///
    /// `Settings::today_now() -> i64` — `@0x1014c9648` (**full**, reconnected):
    /// the emitted body is `jiff::now::sys::system_time` →
    /// `SystemTime::duration_since(UNIX_EPOCH)` → `SignedDuration::try_from`
    /// → the zone (`jiff::tz::db`, `jiff::tz::system::get`), i.e. a
    /// `jiff::Zoned`.  [`zoned_now`] is that `Zoned`; this keeps the unix
    /// second spelling the crate's other row types use.
    pub fn today_now() -> i64 {
        zoned_now().timestamp().as_second()
    }

    /// The instant today's claim time falls on, unix seconds.
    ///
    /// The recovered body is `Settings::today(&self, now: &jiff::Zoned) ->
    /// Option<jiff::Timestamp>` — `@0x1014c8d08` (**full**), kept here as
    /// [`Settings::today_at`] with this no-argument spelling answering the
    /// unix second its siblings use.
    ///
    /// ```text
    /// Settings { time: Some(19 * 60 + 15), .. }.today()
    ///     == Some(<2024-01-15 19:15 Eastern>)
    /// ```
    pub fn today(&self) -> Option<i64> {
        match game_zone() {
            Some(zone) => {
                let now = Timestamp::now().to_zoned(zone);
                self.today_at(&now).map(|ts| ts.as_second())
            }
            // No zone database at all: the game clock, as the `std`-only build
            // had it.  The original had no such fallback — jiff's
            // `TimeZone::get` would have had nothing to answer.
            None => {
                let minutes = self.time? as i64;
                Some(eastern::midnight(&eastern::date(eastern::now())) + minutes * 60)
            }
        }
    }

    /// The instant today's claim time falls on, resolved in `now`'s zone.
    ///
    /// `Settings::today(&self, now: &jiff::Zoned) -> Option<jiff::Timestamp>`
    /// — `@0x1014c8d08` (**full**).  This is the recovered signature: the
    /// wall-clock time is built on `now`'s civil date
    /// (`ldr w24, [x1, #0x20]` reads the date out of the `Zoned`'s datetime,
    /// `0x1014c8d5c`) and resolved in `now`'s zone
    /// (`ldr x19, [x1, #0x10]`, `0x1014c8d74`).
    ///
    /// | case | answer |
    /// |---|---|
    /// | unambiguous | that instant (`0x1014c8e58`: the `Unambiguous` tag falls straight through to `local - offset`) |
    /// | DST gap | the wall clock is pushed forward — 02:30 on 2024-03-10 resolves to 03:30 EDT, `0x1014c8e84` taking the `Gap`'s `before` offset |
    /// | DST fold | the earlier of the two instants — 01:30 on 2024-11-03 is 05:30Z (EDT), `0x1014c8fd0` taking the `Fold`'s `before` offset |
    ///
    /// Those two arms are `jiff`'s `AmbiguousTimestamp::compatible`, which is
    /// exactly the match the emitted code performs: the `AmbiguousOffset` tag
    /// is tested (`cmp w11, #0x1` at `0x1014c8e7c`) and both the `Gap` and the
    /// `Fold` arm subtract `before` (`sub x21, x11, w8, sxtw`, `0x1014c8ef4`).
    ///
    /// A `time` of 24:00 or later is not a civil time: it answers `None` here.
    /// The recovered code built it with jiff's panicking `Date::at` and
    /// aborted inside `jiff` (`0x1014c8d54` jumps to the `invalid time` panic
    /// at `jiff-0.2.37/src/civil/time.rs:328`), which this crate does not
    /// reproduce — `parse` and `to_toml` cannot make such a value.
    pub fn today_at(&self, now: &Zoned) -> Option<Timestamp> {
        self.instant(now.date(), now.time_zone())
    }

    /// The configured wall-clock time on `date`, in `zone`, or `None`.
    fn instant(&self, date: CivilDate, zone: &TimeZone) -> Option<Timestamp> {
        let minutes = self.time?;
        let hour = i8::try_from(minutes / 60).ok()?;
        let minute = i8::try_from(minutes % 60).ok()?;
        let dt = jiff::civil::DateTime::new(
            date.year(),
            date.month(),
            date.day(),
            hour,
            minute,
            0,
            0,
        )
        .ok()?;
        zone.to_ambiguous_timestamp(dt).compatible().ok()
    }

    /// The time of day as `hh:mm`.
    pub fn clock(&self) -> Option<String> {
        self.time
            .map(|minutes| format!("{:02}:{:02}", minutes / 60, minutes % 60))
    }

    /// Nothing to do at all?
    pub fn idle(&self) -> bool {
        self.time.is_none() && !self.store && self.skip.is_empty()
    }
}

/// When the next claim is due, unix seconds.
///
/// `hydra_rewards::schedule::next` — `@0x1010011cb08` (**partly**: the symbol
/// survives but its body is a `jiff`-based comparison this reconstruction
/// cannot read faithfully).
///
/// Today's claim time when it is still to come, tomorrow's otherwise; `None`
/// when no time is set.
pub fn next(settings: &Settings) -> Option<i64> {
    let today = settings.today()?;
    let now = Settings::today_now();
    Some(if today > now { today } else { today + 86_400 })
}

/// `hh:mm` (or `h:m`) as minutes from midnight.
fn parse_time(text: &str) -> Result<u32, String> {
    let bad = || format!("`{}` is not a time of day: say it as 19:05.", text);
    let (hour, minute) = text.trim().split_once(':').ok_or_else(bad)?;
    if hour.is_empty() || minute.is_empty() || hour.len() > 2 || minute.len() > 2 {
        return Err(bad());
    }
    let hour: u32 = hour.parse().map_err(|_| bad())?;
    let minute: u32 = minute.parse().map_err(|_| bad())?;
    if hour > 23 || minute > 59 {
        return Err(bad());
    }
    Ok(hour * 60 + minute)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "hydra-rewards-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_schedule_round_trips_through_toml() {
        let settings = Settings {
            skip: vec![Character::new("Aelith"), Character::new("GS4:Bob")],
            time: Some(19 * 60 + 15),
            store: true,
        };
        let text = settings.to_toml();
        assert_eq!(
            text,
            "time = \"19:15\"\nskip = [\"Aelith\", \"GS4:Bob\"]\nstore = true\n"
        );
        assert_eq!(Settings::parse(&text).unwrap(), settings);
    }

    #[test]
    fn an_empty_schedule_still_names_its_store_flag() {
        let settings = Settings::default();
        assert_eq!(settings.to_toml(), "store = false\n");
        assert_eq!(Settings::parse("store = false\n").unwrap(), Settings::default());
        assert!(settings.idle());
    }

    #[test]
    fn a_bad_time_is_refused_with_the_recovered_message() {
        assert_eq!(
            Settings::parse("time = \"19.25\"\n").unwrap_err(),
            "`19.25` is not a time of day: say it as 19:05."
        );
        assert_eq!(
            Settings::parse("time = \"24:00\"\n").unwrap_err(),
            "`24:00` is not a time of day: say it as 19:05."
        );
        assert_eq!(
            Settings::parse("time = \"19:60\"\n").unwrap_err(),
            "`19:60` is not a time of day: say it as 19:05."
        );
    }

    #[test]
    fn the_time_is_minutes_from_midnight() {
        let s = Settings::parse("time = \"00:00\"\n").unwrap();
        assert_eq!(s.time, Some(0));
        let s = Settings::parse("time = \"23:59\"\n").unwrap();
        assert_eq!(s.time, Some(23 * 60 + 59));
        assert_eq!(s.clock().as_deref(), Some("23:59"));
        let s = Settings::parse("time = \"9:05\"\n").unwrap();
        assert_eq!(s.time, Some(9 * 60 + 5));
    }

    #[test]
    fn skipped_matches_a_name_or_a_game_and_a_name() {
        let skip = [Character::new("Aelith"), Character::new("GS4:Bob")];
        assert!(Character::skipped(&Who::new("GS4", "Aelith"), &skip));
        assert!(Character::skipped(&Who::new("GS4", "aelith"), &skip));
        assert!(Character::skipped(&Who::new("GS4", "Bob"), &skip));
        assert!(Character::skipped(&Who::new("GS4", "BOB"), &skip));
        assert!(!Character::skipped(&Who::new("GS4", "Cara"), &skip));
        // the GAME part has to match too
        assert!(!Character::skipped(&Who::new("DR", "Bob"), &skip));
        // and an empty list skips nobody
        assert!(!Character::skipped(&Who::new("GS4", "Aelith"), &[]));
    }

    #[test]
    fn write_and_read_round_trip() {
        let dir = tempdir("roundtrip");
        let settings = Settings {
            skip: vec![Character::new("Aelith")],
            time: Some(19 * 60 + 15),
            store: true,
        };
        settings.write(&dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE)).unwrap(),
            settings.to_toml()
        );
        assert_eq!(Settings::read(&dir).unwrap(), Some(settings));
        // writing an idle schedule removes the file again
        Settings::default().write(&dir).unwrap();
        assert!(!dir.join(FILE).exists());
        assert_eq!(Settings::read(&dir).unwrap(), None);
        // and removing an absent file is not an error
        Settings::default().write(&dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_read_error_names_the_file() {
        let dir = tempdir("badtime");
        std::fs::write(dir.join(FILE), "time = \"nope\"\n").unwrap();
        let err = Settings::read(&dir).unwrap_err();
        assert!(err.ends_with(": `nope` is not a time of day: say it as 19:05."), "{err}");
        assert!(err.contains("rewards.toml"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn today_is_the_configured_time_on_the_games_clock() {
        let settings = Settings {
            time: Some(19 * 60 + 15),
            ..Settings::default()
        };
        let at = settings.today().unwrap();
        let date = eastern::date(Settings::today_now());
        assert_eq!(at, eastern::midnight(&date) + 19 * 3600 + 15 * 60);
        let clock = eastern::date(at);
        assert_eq!(clock, date);
        // no time, no instant
        assert_eq!(Settings::default().today(), None);
    }

    #[test]
    fn next_is_today_or_tomorrow() {
        let settings = Settings {
            time: Some(19 * 60 + 15),
            ..Settings::default()
        };
        let today = settings.today().unwrap();
        let when = next(&settings).unwrap();
        assert!(when == today || when == today + 86_400, "{when} vs {today}");
        assert_eq!(next(&Settings::default()), None);
    }

    #[test]
    fn today_now_is_a_unix_second() {
        let now = Settings::today_now();
        assert!(now > 1_577_836_800 && now < 4_102_444_800, "{now}");
    }

    // --- the `jiff` zone database (Phase 1) -------------------------------

    /// A deterministic `Zoned` in the game's zone, for the DST cases.
    fn eastern_zoned(year: i16, month: i8, day: i8, hour: i8, minute: i8) -> Zoned {
        jiff::civil::date(year, month, day)
            .at(hour, minute, 0, 0)
            .to_zoned(game_zone().expect("the zone database"))
            .expect("an unambiguous wall-clock time")
    }

    fn zone() -> TimeZone {
        game_zone().expect("the zone database")
    }

    /// The zone database is a platform fact, not a code fact: without it the
    /// crate falls back to `hydra_net::eastern`'s fixed rule and these tests
    /// have nothing to say.
    fn have_zone() -> bool {
        game_zone().is_some()
    }

    #[test]
    fn today_at_resolves_the_wall_clock_in_the_zone() {
        if !have_zone() {
            return;
        }
        let settings = Settings {
            time: Some(19 * 60 + 15),
            ..Settings::default()
        };
        let now = eastern_zoned(2024, 7, 15, 9, 0);
        let at = settings.today_at(&now).unwrap();
        // 19:15 EDT (UTC-4) is 23:15Z
        assert_eq!(at.to_string(), "2024-07-15T23:15:00Z");
        assert_eq!(
            at.to_zoned(zone()).datetime().to_string(),
            "2024-07-15T19:15:00"
        );
        // winter is standard time, so the same wall clock is an hour later
        let now = eastern_zoned(2024, 1, 15, 9, 0);
        let at = settings.today_at(&now).unwrap();
        assert_eq!(at.to_string(), "2024-01-16T00:15:00Z");
        // and no time, no instant
        assert_eq!(Settings::default().today_at(&now), None);
    }

    #[test]
    fn a_time_in_the_dst_gap_is_pushed_forward() {
        if !have_zone() {
            return;
        }
        // 2024-03-10: the clocks go 02:00 EST -> 03:00 EDT, so 02:30 never
        // happens.  jiff's `compatible` (the recovered `before` offset, -5)
        // reads it as 03:30 EDT.
        let settings = Settings {
            time: Some(2 * 60 + 30),
            ..Settings::default()
        };
        let now = eastern_zoned(2024, 3, 10, 1, 0);
        let at = settings.today_at(&now).unwrap();
        assert_eq!(at.to_string(), "2024-03-10T07:30:00Z");
        assert_eq!(
            at.to_zoned(zone()).datetime().to_string(),
            "2024-03-10T03:30:00"
        );
        // the hour before the gap is a normal, unambiguous time
        let settings = Settings {
            time: Some(1 * 60 + 30),
            ..Settings::default()
        };
        assert_eq!(
            settings.today_at(&now).unwrap().to_string(),
            "2024-03-10T06:30:00Z"
        );
    }

    #[test]
    fn a_time_in_the_dst_fold_takes_the_earlier_instant() {
        if !have_zone() {
            return;
        }
        // 2024-11-03: the clocks go 02:00 EDT -> 01:00 EST, so 01:30 happens
        // twice.  The recovered handler takes the `Fold`'s `before` offset
        // (-4), i.e. the earlier of the two instants.
        let settings = Settings {
            time: Some(1 * 60 + 30),
            ..Settings::default()
        };
        let now = eastern_zoned(2024, 11, 3, 0, 30);
        let at = settings.today_at(&now).unwrap();
        assert_eq!(at.to_string(), "2024-11-03T05:30:00Z");
        assert_eq!(
            at.to_zoned(zone()).datetime().to_string(),
            "2024-11-03T01:30:00"
        );
        // jiff agrees that the later occurrence is the other offset
        let dt = jiff::civil::date(2024, 11, 3).at(1, 30, 0, 0);
        assert_eq!(
            zone().to_ambiguous_timestamp(dt).later().unwrap().to_string(),
            "2024-11-03T06:30:00Z"
        );
        assert_eq!(
            at.as_second() + 3_600,
            zone().to_ambiguous_timestamp(dt).later().unwrap().as_second()
        );
    }

    #[test]
    fn a_time_that_is_not_a_civil_time_has_no_instant() {
        if !have_zone() {
            return;
        }
        // `parse` cannot produce these (it bounds the hours to 0..=23), but a
        // caller can: the recovered code panicked inside jiff's `Time::constant`
        // here, and this answers `None` instead.
        let now = eastern_zoned(2024, 7, 15, 9, 0);
        for minutes in [MINUTES, MINUTES + 60, u32::MAX] {
            let settings = Settings {
                time: Some(minutes),
                ..Settings::default()
            };
            assert_eq!(settings.today_at(&now), None, "{minutes}");
        }
    }

    #[test]
    fn today_goes_through_the_zone_database() {
        if !have_zone() {
            return;
        }
        let settings = Settings {
            time: Some(19 * 60 + 15),
            ..Settings::default()
        };
        let at = settings.today().unwrap();
        let now = zoned_now();
        assert_eq!(
            at,
            settings
                .today_at(&now)
                .expect("an instant today")
                .as_second()
        );
        // the same wall clock, resolved in the zone, is what the reports quote
        let local = Timestamp::from_second(at)
            .unwrap()
            .to_zoned(zone())
            .datetime();
        assert_eq!((local.hour(), local.minute()), (19, 15));
        assert_eq!(local.date(), now.date());
    }
}
