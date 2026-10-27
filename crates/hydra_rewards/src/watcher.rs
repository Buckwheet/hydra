//! The watcher: the state machine that turns what a session hears into
//! login-reward rows.
//!
//! Recovered from `hydra_rewards::watcher` — `given_before` (`0x1014efde4`),
//! `Heard::at` (`0x1014efe2c`), `Watcher::hear` (`0x1014eff64`) and
//! `Watcher::hear_line` (`0x1014f006c`), plus the private
//! `<Heard as Clone>::clone` (`0x1000a5940`).

use crate::parse;
use crate::row::{Stamp, Who};
use hydra_net::eastern;

/// One login reward, exactly as `hydra_ledger`'s `login_rewards` row holds it.
///
/// **ESTABLISHED**: 0xc0 bytes.  The field offsets are those `Heard::at`
/// (`0x1014efe50` … `0x1014efef4`) fills and the reports read:
///
/// | offset | field | evidence |
/// |---|---|---|
/// | `+0x00`/`+0x08` | [`Heard::day`] | `report::streak` (`0x1014d012c`) reads `[+0x00]` as a tag (`== 1` = `Some`) and `[+0x08]` as the number, then prints `", day {}"` |
/// | `+0x10`/`+0x18` | [`Heard::bridges`] | `report::streak` (`0x1014d0164`, `0x1014d018c`) prints `", {} bridge{s}"` |
/// | `+0x20` | [`Heard::instance`] | cloned from `who+0x00` (`0x1014efe50`) |
/// | `+0x38` | [`Heard::character`] | cloned from `who+0x18` (`0x1014efe60`) |
/// | `+0x50` | [`Heard::day_text`] | `hydra_net::eastern::format(eastern::date(stamp+0x8))` (`0x1014efe88`, `0x1014efe9c`) |
/// | `+0x68` | [`Heard::reward`] | the `{}` of `own`'s "…Eastern: {}{}." (`0x1014ceaf0`) |
/// | `+0x80` | [`Heard::line`] | copied from the state's line, one of the DB's columns |
/// | `+0x98` | [`Heard::said`] | **UNKNOWN** — copied from the state, never read by a report |
/// | `+0xb0` | [`Heard::stamp`] | the `Stamp`, verbatim (`0x1014efed8`) |
///
/// The column list of the table is recovered from the `CREATE TABLE
/// login_rewards` blob (`0x1039857f3`) and the `INSERT` (`0x103985e59`):
/// `at, game_time, instance, character, day, streak, bridges, reward,
/// thanks, line`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heard {
    /// The streak length the game announced that day, if it did.
    pub day: Option<u32>,
    /// The login bridges crossed, if the game said.
    pub bridges: Option<u32>,
    /// The game's instance code.
    pub instance: String,
    /// The character's name.
    pub character: String,
    /// `YYYY-MM-DD` on the game's clock.
    pub day_text: String,
    /// What the reward was; empty when Hydra only saw the "is in" confirmation.
    pub reward: String,
    /// The whole line, as the game said it.
    pub line: String,
    /// The streak line that preceded the reward, if any.
    pub said: String,
    /// When it was heard.
    pub stamp: Stamp,
}

impl Heard {
    /// The same hearing, told for `who` and stamped `stamp`.
    ///
    /// `Heard::at(&self, who: &Who, stamp: Stamp) -> Heard` —
    /// `@0x1014efe2c` (**full**).
    ///
    /// The two `Option`s and the three text columns of `self` are carried
    /// over verbatim, `who`'s two `String`s are cloned into
    /// [`Heard::instance`]/[`Heard::character`], the day text is rebuilt from
    /// `stamp.game_time` on the game's clock, and `stamp` is stored as-is:
    ///
    /// * `out[0x00..0x20] = self[0x00..0x20]` (`0x1014efef0`);
    /// * `out[0x20] = who.instance.clone()`, `out[0x38] = who.character.clone()`
    ///   (`0x1014efe50`, `0x1014efe60`);
    /// * `out[0x50] = eastern::format(eastern::date(stamp.game_time))`
    ///   (`0x1014efe88`: `ldr x0, [x21, #0x8]` — the `Stamp`'s second word);
    /// * `out[0x68/0x80/0x98] = self[0x20..0x68]` (`0x1014efea0` …);
    /// * `out[0xb0..0xc0] = stamp` (`0x1014efed8`).
    pub fn at(&self, who: &Who, stamp: Stamp) -> Heard {
        Heard {
            day: self.day,
            bridges: self.bridges,
            instance: who.instance.clone(),
            character: who.character.clone(),
            day_text: eastern::format(&eastern::date(stamp.game_time)),
            reward: self.reward.clone(),
            line: self.line.clone(),
            said: self.said.clone(),
            stamp,
        }
    }

    /// A hearing that Hydra only saw *after* the fact.
    ///
    /// `given_before(who: &Who, stamp: Stamp) -> Heard` — `@0x1014efde4`
    /// (**full**).
    ///
    /// ```text
    /// given_before(&Who::new("GS4", "Aelith"), Stamp::new(a, g))
    ///     == Heard { instance: "GS4", character: "Aelith",
    ///                day_text: <Eastern date of g>, .. }
    /// ```
    ///
    /// The emitted body builds the 0x68-byte *state* on the stack — the two
    /// `Option`s zeroed and three empty `String`s at `+0x20`, `+0x38`, `+0x50`
    /// (`0x1014efdf8` … `0x1014efe14`, i.e. `(cap, ptr, len) = (0, 1, 0)`),
    /// then tail-calls [`Heard::at`] with it.  Everything is therefore empty:
    /// no reward, no line, no streak — the row only records *that* the reward
    /// was in before Hydra was watching, which is where the reports' "given
    /// before Hydra saw it" (`0x104437120`) comes from.
    pub fn given_before(who: &Who, stamp: Stamp) -> Heard {
        State::default().into_row(who, stamp)
    }
}

/// The free-function spelling of [`Heard::given_before`] the symbol table has
/// (`hydra_rewards::watcher::given_before`, `@0x1014efde4`).
pub fn given_before(who: &Who, stamp: Stamp) -> Heard {
    Heard::given_before(who, stamp)
}

/// The watcher's memory of the streak line it last saw.
///
/// **ESTABLISHED**: 0x38 bytes, the first field of [`Watcher`]:
///
/// | offset | field | evidence |
/// |---|---|---|
/// | `+0x00`/`+0x08` | `day` | written by `hear_line`'s streak path (`0x1014f02d8`), read at `0x1014f02a4` where the tag is compared with `2` |
/// | `+0x10`/`+0x18` | `bridges` | `0x1014f02dc` |
/// | `+0x20` | `line` | `stp x23, x24, [x20, #0x20]` / `str x23, [x20, #0x30]` (`0x1014f02e0`) |
///
/// `Nothing` is encoded as the tag `2` (`0x1014f0164`: `mov w9, #2; str x9,
/// [x20]`), which is what resets the state after a reward line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct State {
    day: Option<u32>,
    bridges: Option<u32>,
    line: String,
}

impl State {
    fn into_row(self, who: &Who, stamp: Stamp) -> Heard {
        Heard {
            day: self.day,
            bridges: self.bridges,
            instance: who.instance.clone(),
            character: who.character.clone(),
            day_text: eastern::format(&eastern::date(stamp.game_time)),
            reward: String::new(),
            line: String::new(),
            said: self.line,
            stamp,
        }
    }
}

/// What one line told the watcher.
///
/// **ESTABLISHED**: a 0x68-byte value returned through `x8` by both
/// [`Watcher::hear`] and [`Watcher::hear_line`], with the tag word `2`
/// meaning "nothing" (`0x1014f0148`: `mov w8, #2; str x8, [x19]`) and the
/// other variants carrying, at `+0x00..0x20`, the streak numbers, at `+0x20`
/// the reward, at `+0x38` the streak line and at `+0x50` the whole line
/// (`0x1014f0234` … `0x1014f0250`).
///
/// **UNKNOWN**: the original's variant order and names.  This reconstruction
/// keeps the same field sets and puts "nothing" last, which is the variant the
/// binary tags `2`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Said {
    /// A streak line — `Thank you for logging into … consecutive day …`.  It
    /// only updates the watcher's memory and reports nothing (`hear_line`
    /// answers "nothing" for it, `0x1014f02e8`).
    Streak {
        day: Option<u32>,
        bridges: Option<u32>,
        line: String,
    },
    /// A reward line.  The numbers and the streak line are those of the
    /// streak that came *before* it (the state is cleared as the reward is
    /// reported, `0x1014f0160`).
    Reward {
        day: Option<u32>,
        bridges: Option<u32>,
        said: String,
        reward: String,
        line: String,
    },
    /// Nothing the crate cares about.
    #[default]
    Nothing,
}

/// A chunk of protocol text: the bytes, and whether the line ends here.
///
/// **ESTABLISHED** (`Watcher::hear`, `0x1014eff84` … `0x1014effbc`): the
/// argument is a pointer to a struct whose `+0x08` is a byte slice
/// (`ptr` at `+0x08`, `len` at `+0x10`; the `memcpy` at `0x1014effb0`
/// appends `len` bytes from `ptr`) and whose byte at `+0x110` decides
/// whether a line is complete (`0x1014effbc`: `ldrb w8, [x21, #0x110];
/// tbz w8, #0x0, …`).
///
/// The binary's struct is `hydra_protocol::frame::vocabulary::Frame` (a
/// 0x118-byte tagged frame; its 0x110-byte payload is what `hear` walks).
/// This reconstruction does not reconstruct the frame, so the two facts the
/// watcher needs are passed directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    /// The bytes this frame adds to the line.
    pub bytes: &'a [u8],
    /// Whether the line ends with this frame.
    pub ends_line: bool,
}

impl<'a> Chunk<'a> {
    pub const fn new(bytes: &'a [u8], ends_line: bool) -> Chunk<'a> {
        Chunk { bytes, ends_line }
    }

    /// One whole line.
    pub fn line(text: &'a str) -> Chunk<'a> {
        Chunk {
            bytes: text.as_bytes(),
            ends_line: true,
        }
    }

    /// More text for the line being built.
    pub fn more(text: &'a str) -> Chunk<'a> {
        Chunk {
            bytes: text.as_bytes(),
            ends_line: false,
        }
    }
}

/// Watches a session's text for login rewards.
///
/// **ESTABLISHED**: the state at `+0x00` (0x38 bytes, [`State`]) and a
/// `Vec<u8>` line buffer at `+0x38` (`cap` at `+0x38`, `ptr` at `+0x40`,
/// `len` at `+0x48` — `0x1014eff8c` … `0x1014efff4` append into it with
/// `RawVec::reserve`'s `do_reserve_and_handle`).  The bytes past `+0x50` are
/// not touched by any recovered function.
#[derive(Clone, Debug, Default)]
pub struct Watcher {
    state: State,
    line: Vec<u8>,
}

impl Watcher {
    pub fn new() -> Watcher {
        Watcher::default()
    }

    /// Feed one chunk of protocol text.
    ///
    /// `Watcher::hear(&mut self, chunk: &Chunk) -> Said` — `@0x1014eff64`
    /// (**full**).
    ///
    /// The chunk's bytes are appended to the line buffer
    /// (`0x1014effb0`); when the chunk does not end the line, the answer is
    /// "nothing" (`0x1014f0008`: `mov w8, #2; str x8, [x19]`).  When it does,
    /// the buffer is taken (the `Vec` is reset to `(0, 1, 0)` at
    /// `0x1014effc8`), handed to [`Watcher::hear_line`], and freed.
    ///
    /// A buffer that is not valid UTF-8 answers "nothing": the original
    /// passes the raw bytes to `str::from_utf8`'s unwrap-free path — the
    /// recovered `hear_line` only ever calls `core::str::trim_matches` on it,
    /// so a lossy conversion is the closest std equivalent.
    pub fn hear(&mut self, chunk: &Chunk<'_>) -> Said {
        self.line.extend_from_slice(chunk.bytes);
        if !chunk.ends_line {
            return Said::Nothing;
        }
        let bytes = std::mem::take(&mut self.line);
        let line = String::from_utf8_lossy(&bytes).into_owned();
        self.hear_line(&line)
    }

    /// Feed one whole line.
    ///
    /// `Watcher::hear_line(&mut self, line: &str) -> Said` — `@0x1014f006c`
    /// (**full**).  The order of the two grammars is the emitted one:
    ///
    /// 1. trim the line (`0x1014f00a4`);
    /// 2. if it is at least 27 bytes and starts with
    ///    `parse::THANKS` (`0x1014f00b0` … `0x1014f00e0`), read the two
    ///    numbers in front of `parse::CONSECUTIVE_DAY` and
    ///    `parse::LOGIN_BRIDGE` (`0x1014f00fc`, `0x1014f011c`); when a day
    ///    count was found this is a *streak* line: the watcher's memory is
    ///    replaced with the two numbers and the trimmed line (`0x1014f02d8`)
    ///    and the answer is "nothing" (`0x1014f02e8`);
    /// 3. otherwise [`crate::parse::reward`] decides (`0x1014f0134`).  `None`
    ///    answers "nothing" (`0x1014f0148`); `Some(reward)` clears the memory
    ///    (`0x1014f0164`) and answers a reward carrying the memory it just
    ///    cleared (`0x1014f01d0`), the reward and the trimmed line
    ///    (`0x1014f0234`).
    ///
    /// **Conflict** — the emitted code tests the day count with
    /// `cmp x21, #0x2; b.ne` (`0x1014f0120`), where `x21` is the first word
    /// `parse::number_before` answers (`Option<u32>`, whose tag is `0` or
    /// `1`, so the test can never be equal).  The only reading that keeps the
    /// two grammars apart is "a day count was found", which is what this
    /// reconstruction does; the equality test is reported in `NOTES.md`.
    pub fn hear_line(&mut self, line: &str) -> Said {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(parse::THANKS) {
            let day = parse::number_before(rest, parse::CONSECUTIVE_DAY);
            if day.is_some() {
                let bridges = parse::number_before(rest, parse::LOGIN_BRIDGE);
                self.state = State {
                    day,
                    bridges,
                    line: trimmed.to_string(),
                };
                return Said::Nothing;
            }
        }
        let Some(reward) = parse::reward(trimmed) else {
            return Said::Nothing;
        };
        let prior = std::mem::take(&mut self.state);
        Said::Reward {
            day: prior.day,
            bridges: prior.bridges,
            said: prior.line,
            reward,
            line: trimmed.to_string(),
        }
    }
}

/// The hearing a reward line produces, told for `who` and stamped `stamp`.
///
/// This is the glue the recorder uses: the streak line's numbers and line
/// travel with the reward (they are what `Said::Reward` carries), and the row
/// is completed from them.
pub fn heard(said: &Said, who: &Who, stamp: Stamp) -> Option<Heard> {
    match said {
        Said::Reward {
            day,
            bridges,
            said,
            reward,
            line,
        } => {
            let state = State {
                day: *day,
                bridges: *bridges,
                line: said.clone(),
            };
            let mut row = state.into_row(who, stamp);
            row.reward = reward.clone();
            row.line = line.clone();
            Some(row)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who() -> Who {
        Who::new("GS4", "Aelith")
    }

    const STREAK: &str = "Thank you for logging into GemStone IV!  You have logged in \
                          4 consecutive days and crossed 9 login bridges.";

    #[test]
    fn the_day_text_is_the_eastern_date_of_the_game_clock() {
        // 2024-01-15 19:50 UTC = 14:50 Eastern
        let stamp = Stamp::new(1_705_348_200, 1_705_348_200);
        let row = Heard::given_before(&who(), stamp);
        assert_eq!(row.day_text, "2024-01-15");
        assert_eq!(row.instance, "GS4");
        assert_eq!(row.character, "Aelith");
        assert_eq!(row.stamp, stamp);
        assert_eq!(row.reward, "");
        assert_eq!(row.said, "");
    }

    #[test]
    fn at_rebuilds_the_day_and_keeps_the_rest() {
        let early = Heard::given_before(&who(), Stamp::new(1_705_348_200, 1_705_348_200));
        let later = early.at(&who(), Stamp::new(1_706_000_000, 1_706_000_000));
        assert_eq!(later.day_text, "2024-01-23");
        assert_eq!(later.character, "Aelith");
        assert_eq!(later.stamp, Stamp::new(1_706_000_000, 1_706_000_000));
    }

    #[test]
    fn a_streak_line_only_remembers() {
        let mut w = Watcher::new();
        assert_eq!(w.hear_line(STREAK), Said::Nothing);
        // and the memory is what the next reward carries
        let said = w.hear_line("You have earned the following reward: a brass ring");
        assert_eq!(
            said,
            Said::Reward {
                day: Some(4),
                bridges: Some(9),
                said: STREAK.to_string(),
                reward: "a brass ring".to_string(),
                line: "You have earned the following reward: a brass ring".to_string(),
            }
        );
    }

    #[test]
    fn the_memory_is_cleared_by_a_reward() {
        let mut w = Watcher::new();
        w.hear_line(STREAK);
        w.hear_line("You have earned the following reward: a brass ring");
        let second = w.hear_line("You have earned the following reward: a tin cup");
        assert_eq!(
            second,
            Said::Reward {
                day: None,
                bridges: None,
                said: String::new(),
                reward: "a tin cup".to_string(),
                line: "You have earned the following reward: a tin cup".to_string(),
            }
        );
    }

    #[test]
    fn other_lines_are_nothing() {
        let mut w = Watcher::new();
        assert_eq!(w.hear_line("You wave."), Said::Nothing);
        assert_eq!(w.hear_line(""), Said::Nothing);
        assert_eq!(w.hear_line("  "), Said::Nothing);
        assert_eq!(w.hear_line("Thank you for logging into GemStone IV!"), Said::Nothing);
    }

    #[test]
    fn hear_buffers_until_the_line_ends() {
        let mut w = Watcher::new();
        assert_eq!(w.hear(&Chunk::more("You have earned ")), Said::Nothing);
        assert_eq!(w.hear(&Chunk::more("the following reward: ")), Said::Nothing);
        let said = w.hear(&Chunk::new(b"a blue orb", true));
        assert_eq!(
            said,
            Said::Reward {
                day: None,
                bridges: None,
                said: String::new(),
                reward: "a blue orb".to_string(),
                line: "You have earned the following reward: a blue orb".to_string(),
            }
        );
        // the buffer is empty again
        assert_eq!(w.hear(&Chunk::new(b"You wave.", true)), Said::Nothing);
    }

    #[test]
    fn a_hearing_becomes_a_row() {
        let mut w = Watcher::new();
        w.hear_line(STREAK);
        let said = w.hear_line("You have earned the following reward: a brass ring");
        let row = heard(&said, &who(), Stamp::new(1_705_348_200, 1_705_348_200)).unwrap();
        assert_eq!(row.day, Some(4));
        assert_eq!(row.bridges, Some(9));
        assert_eq!(row.reward, "a brass ring");
        assert_eq!(row.said, STREAK);
        assert_eq!(row.day_text, "2024-01-15");
        assert_eq!(heard(&Said::Nothing, &who(), Stamp::default()), None);
    }
}
