//! What the controller rumbles for, and how hard.
//!
//! **ESTABLISHED** shapes:
//!
//!  * `Pattern` is 40 bytes — a `String` at `+0x0`, then `strength` (`f32`,
//!    `+0x18`), `ms` (`+0x1c`), `count` (`+0x20`) and `gap` (`+0x24`); see
//!    [`Rumble::pattern`] at `0x10141eefc` and the serde message
//!    `struct Pattern with 5 elements`.
//!  * The three built-in patterns are `short`, `double` and `long`.
//!  * `Rumble` holds `enabled`, `roundtime_end`, `death`, `stunned` and a
//!    `Vec<Pattern>` at `+0x48`.

use crate::text::Value;
use hydra_model::state::GameState;

/// The key the rumble settings live under.
///
/// **ESTABLISHED** (`0x102417ef3`): `controller_rumble`.
pub const FILE: &str = "controller_rumble";

/// Rumble under this many seconds is not worth the motor.
///
/// **ESTABLISHED** (`0x10141f010`): `0x3d4ccdcd`.
pub const MIN_STRENGTH: f32 = 0.05;

/// A single rumble: how hard, for how long, how often.
///
/// Field order and offsets as in `0x10141eefc`: `name` at `+0x0`,
/// `strength` at `+0x18`, `ms` at `+0x1c`, `count` at `+0x20`, `gap` at `+0x24`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pattern {
    pub name: String,
    /// 0.0 to 1.0.
    pub strength: f32,
    /// How long one pulse lasts, in milliseconds.
    pub ms: u32,
    /// How many pulses.
    pub count: u32,
    /// How long the motor rests between pulses, in milliseconds.
    pub gap: u32,
}

impl Pattern {
    fn new(name: &str, strength: f32, ms: u32, count: u32, gap: u32) -> Pattern {
        Pattern {
            name: name.to_string(),
            strength,
            ms,
            count,
            gap,
        }
    }

    /// The built-in patterns.
    ///
    /// **ESTABLISHED** (`0x10141ef64`, `0x10141ef98`, `0x10141f078`): the
    /// `short`, `double` and `long` branches.
    pub fn built_in(name: &str) -> Option<Pattern> {
        Some(match name {
            "short" => Pattern::new("short", 0.5, 160, 1, 120),
            "double" => Pattern::new("double", 0.8, 180, 2, 120),
            "long" => Pattern::new("long", 0.9, 450, 1, 120),
            _ => return None,
        })
    }

    /// Read a pattern out of a parsed document.
    pub fn read(value: &Value) -> Pattern {
        Pattern {
            name: value.str_of("name"),
            strength: value.f32_of("strength").unwrap_or(0.0),
            ms: value.u64_of("ms").unwrap_or(0) as u32,
            count: value.u64_of("count").unwrap_or(0) as u32,
            gap: value.u64_of("gap").unwrap_or(0) as u32,
        }
    }

    /// The same, tidied into the range the motors take.
    ///
    /// **ESTABLISHED** (`0x10141f00c`–`0x10141f068`): the strength is clamped
    /// into `[0.05, 1.0]`, `ms` into `[20, 2000]`, the count to at least 8
    /// (one pulse when it is zero) and the gap to at most 2000 ms.
    pub fn sane(&self) -> Pattern {
        let count = if self.count == 0 {
            1
        } else {
            self.count.max(8)
        };
        Pattern {
            name: self.name.clone(),
            strength: self.strength.clamp(MIN_STRENGTH, 1.0),
            ms: self.ms.clamp(20, 2000),
            count,
            gap: self.gap.min(2000),
        }
    }

    /// How long the whole pattern runs, in milliseconds.
    pub fn total_ms(&self) -> u32 {
        let pulses = self.count.max(1);
        self.ms * pulses + self.gap * pulses.saturating_sub(1)
    }
}

/// The moments worth rumbling for.
///
/// **ESTABLISHED** (`0x102417ef3`, `0x102418587`): `stunned` and `dead` are
/// looked up in the game state's status flags, and the roundtime is watched.
pub const MOMENTS: [&str; 3] = ["roundtime_end", "death", "stunned"];

/// The rumble settings, read from `controller_rumble`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rumble {
    pub enabled: bool,
    /// The pattern to play when the roundtime ends, by name.
    pub roundtime_end: Option<String>,
    /// The pattern to play when the character dies.
    pub death: Option<String>,
    /// The pattern to play when the character is stunned.
    pub stunned: Option<String>,
    /// The patterns the names above can refer to.
    pub patterns: Vec<Pattern>,
}

impl Rumble {
    /// **ESTABLISHED** (`0x10141d6e0`): the `controller_rumble` table, with a
    /// bad entry reported as `format!("[controller_rumble]: {}", error)`
    /// (`0x102417f60`).
    pub fn read(doc: &Value) -> Result<Rumble, String> {
        let table = doc.get(FILE).unwrap_or(&Value::Null);
        if !matches!(table, Value::Table(_) | Value::Null) {
            return Err(format!("[{FILE}]: expected a table"));
        }
        let mut rumble = Rumble {
            enabled: table.bool_of("enabled"),
            roundtime_end: named(table, "roundtime_end"),
            death: named(table, "death"),
            stunned: named(table, "stunned"),
            patterns: Vec::new(),
        };
        if let Some(patterns) = table.get("patterns").and_then(Value::as_array) {
            rumble.patterns = patterns.iter().map(Pattern::read).collect();
        }
        Ok(rumble)
    }

    /// The pattern called `name`: one of the built-ins, or one from the file.
    ///
    /// **ESTABLISHED** (`0x10141eefc`): the built-ins are tried first, then the
    /// file's patterns are searched by name, and a configured pattern is tidied
    /// by [`Pattern::sane`].  Nothing found is `None`.
    pub fn pattern(&self, name: &str) -> Option<Pattern> {
        let name = name.trim();
        if let Some(built) = Pattern::built_in(name) {
            return Some(built);
        }
        self.patterns
            .iter()
            .find(|p| p.name == name)
            .map(Pattern::sane)
    }

    /// The pattern for one of [`MOMENTS`].
    pub fn for_moment(&self, moment: &str) -> Option<Pattern> {
        if !self.enabled {
            return None;
        }
        let name = match moment {
            "roundtime_end" => self.roundtime_end.as_deref(),
            "death" => self.death.as_deref(),
            "stunned" => self.stunned.as_deref(),
            _ => None,
        }?;
        self.pattern(name)
    }
}

fn named(table: &Value, key: &str) -> Option<String> {
    match table.get(key) {
        Some(Value::Str(s)) => Some(s.clone()),
        Some(Value::Bool(true)) => Some(key.to_string()),
        _ => None,
    }
}

/// Watches the game state for the moments worth rumbling for.
///
/// **ESTABLISHED** (`0x10141d530`): eleven bytes of memory — the last sequence
/// number, the last roundtime/casttime read and three flags — so a moment is
/// only reported when it *changes*.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Watch {
    /// The roundtime was running last time.
    pub in_roundtime: bool,
    /// The character was stunned last time.
    pub stunned: bool,
    /// The character was dead last time.
    pub dead: bool,
    /// The sequence number the flags were read at.
    pub seq: u32,
}

impl Watch {
    /// Which moments have just begun.
    ///
    /// **ESTABLISHED** (`0x10141d530`): the roundtime comes from
    /// `GameState::in_roundtime` (casttime counts as roundtime), and `stunned`
    /// and `dead` from the status flags of the same name.
    pub fn moments(&mut self, seq: u32, state: &GameState) -> Vec<&'static str> {
        let roundtime = state.in_roundtime().unwrap_or(false) || state.in_casttime().unwrap_or(false);
        let stunned = state.status.get("stunned");
        let dead = state.status.get("dead");

        let mut out = Vec::new();
        if self.in_roundtime && !roundtime {
            out.push("roundtime_end");
        }
        if stunned && !self.stunned {
            out.push("stunned");
        }
        if dead && !self.dead {
            out.push("death");
        }

        self.seq = seq;
        self.in_roundtime = roundtime;
        self.stunned = stunned;
        self.dead = dead;
        out
    }
}

/// The last moment that was highlighted, so it is only rumbled for once.
///
/// **PARTLY** (`0x10141d330`): the list it searches is a `Vec` of 24-byte
/// entries whose first word is a niche sentinel (`0x8000000000000000`) rather
/// than a flag.  UNKNOWN: what the entries hold besides their name, so they are
/// modelled as `(name, pattern)` pairs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Highlighted {
    pub seen: Vec<(String, Pattern)>,
}

impl Highlighted {
    /// The pattern to play for `name`, if that name has not just been played.
    ///
    /// **PARTLY** (`0x10141d330`): a matching name is found by comparing the
    /// stored text, and the answer is `None` when there is no entry.
    pub fn take(seen: &[(String, Pattern)], name: &str) -> Option<Pattern> {
        seen.iter()
            .find(|(key, _)| key == name)
            .map(|(_, pattern)| pattern.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_built_in_patterns() {
        let short = Pattern::built_in("short").unwrap();
        assert_eq!((short.strength, short.ms, short.count, short.gap), (0.5, 160, 1, 120));
        let double = Pattern::built_in("double").unwrap();
        assert_eq!((double.strength, double.ms, double.count), (0.8, 180, 2));
        let long = Pattern::built_in("long").unwrap();
        assert_eq!((long.strength, long.ms, long.count), (0.9, 450, 1));
        assert!(Pattern::built_in("nope").is_none());
    }

    #[test]
    fn a_configured_pattern_is_tidied() {
        let raw = Pattern {
            name: "tap".into(),
            strength: 0.001,
            ms: 5,
            count: 0,
            gap: 9000,
        };
        let sane = raw.sane();
        assert_eq!(sane.strength, MIN_STRENGTH);
        assert_eq!(sane.ms, 20);
        assert_eq!(sane.count, 1);
        assert_eq!(sane.gap, 2000);
        // A big count is raised to 8, not lowered.
        let many = Pattern {
            count: 3,
            ..raw.clone()
        }
        .sane();
        assert_eq!(many.count, 8);
        assert_eq!(many.total_ms(), 20 * 8 + 2000 * 7);
    }

    #[test]
    fn patterns_are_read_from_the_file_and_named_built_ins_win() {
        let text = "\
[controller_rumble]
enabled = true
stunned = \"tap\"
patterns = [
  { name = \"tap\", strength = 0.5, ms = 100, count = 8, gap = 50 },
  { name = \"short\", strength = 0.9, ms = 10, count = 1, gap = 0 },
]
";
        let doc = crate::text::parse_toml(text).unwrap();
        let rumble = Rumble::read(&doc).unwrap();
        assert!(rumble.enabled);
        assert_eq!(rumble.stunned.as_deref(), Some("tap"));
        assert_eq!(rumble.patterns.len(), 2);
        // The file's "tap" is found by name.
        let tap = rumble.pattern("tap").unwrap();
        assert_eq!(tap.ms, 100);
        // A built-in name wins even when the file redefines it.
        assert_eq!(rumble.pattern("short").unwrap().strength, 0.5);
        assert_eq!(rumble.for_moment("stunned").unwrap().name, "tap");
        assert!(rumble.for_moment("death").is_none());
    }

    #[test]
    fn rumble_disabled_plays_nothing() {
        let text = "[controller_rumble]\nenabled = false\nstunned = \"double\"\n";
        let rumble = Rumble::read(&crate::text::parse_toml(text).unwrap()).unwrap();
        assert!(rumble.for_moment("stunned").is_none());
    }

    #[test]
    fn the_watch_only_reports_a_change() {
        let mut state = GameState::default();
        let mut watch = Watch::default();
        // Nothing has begun yet, so nothing is reported.
        assert!(watch.moments(1, &state).is_empty());
        state.status.set("stunned", true);
        assert_eq!(watch.moments(2, &state), vec!["stunned"]);
        // Still stunned: not reported twice.
        assert!(watch.moments(3, &state).is_empty());
        assert_eq!(watch.seq, 3);
    }

    #[test]
    fn take_finds_the_named_pattern_once() {
        let seen = vec![(
            "stunned".to_string(),
            Pattern::new("stunned", 0.5, 100, 1, 0),
        )];
        assert_eq!(Highlighted::take(&seen, "stunned").unwrap().ms, 100);
        assert!(Highlighted::take(&seen, "death").is_none());
    }
}
