//! `lookout.toml`: the lookout's choices.
//!
//! The twelve field names are read straight out of the binary: the
//! `Deserialize` field visitor compares against them at `0x101ac9214` and the
//! unknown-field list lives in a `&str` table at `0x1044a02c0`
//! (`occasions`, `window`, `stat_floor`, `forget_days`, `quiet_secs`, `mark`,
//! `paused`, `off`, `ignore`, `always`, `trigger_added`, `seeded`).  The
//! `Serialize` impl at `0x101ac867c` walks them in that order and skips `off`
//! and `always` when they are empty, `ignore` when it is `None` and `seeded`
//! when it is empty.
//!
//! The defaults come from the default constructor that both `Settings::read`
//! (`0x101ac8afc`) and `Lookout::new` (`0x101ab40e8`) inline:
//!
//! | field | default | evidence |
//! | --- | --- | --- |
//! | `occasions` | 5 | `movi.2s v0, #0x5, lsl #24` byte at `+0x7b` |
//! | `window` | 2.0 | `0x4000_0000_0000_0000` stored at `+0x48` |
//! | `stat_floor` | 1500 | `mov w9, #0x5dc` stored at `+0x50` |
//! | `forget_days` | 30 | `0x10213cb50` (`1e 00 00 00`) at `+0x70` |
//! | `quiet_secs` | 60 | `0x10213cb50` (`3c 00 00 00`) at `+0x74` |
//! | `mark`, `paused`, `trigger_added` | 0 | the same four bytes |
//! | `off`, `always`, `seeded` | empty | dangling `Vec`s |
//! | `ignore` | `None` | `isize::MIN` niche at `+0x30` |
//!
//! `ignore` is an `Option<Vec<String>>`: when it is `None`,
//! [`Settings::ignore_list`] returns the 28 built-in patterns instead.

use std::fmt::Write as _;
use std::path::Path;

use crate::file;

/// The settings file's name (`0x1039c7e48`, 12 bytes).
pub const FILE: &str = "lookout.toml";

/// The patterns [`Settings::ignore_list`] falls back on.
///
/// The 28 strings are a `&str` table at `0x1044a0380` (28 pairs of pointer and
/// length, 0x1c0 bytes) that `ignore_list` collects when `ignore` is `None`.
pub const IGNORED_BY_DEFAULT: [&str; 28] = [
    r"^Obvious (?:exits|paths): ",
    r"^\[[^\]]+\](?: \(\d+\))?$",
    r"\[LNet\]|\[Merchant\]|\[Realm\]|\[General\]|\[Help\]|\[OOC\]",
    r"^\[.*?\]-[A-Za-z]+:|^\[server\]: ",
    r"^In the",
    r"just (?:arrived|went)",
    r"^Today is",
    r"^The .* takes",
    r"^Also here",
    r"^You (?:put|remove|retrieve|carefully|slip)",
    r"(?i)^You notice",
    r"^You (?:open|close)",
    r"^You analyze",
    r"^You might be able to",
    r"^You offer to sell",
    r"^.* takes your",
    r"^You ask ",
    r"^Try as you might",
    r"^You can tell that",
    r"^You get no sense",
    r"^You summon",
    r"^As you place",
    r"^You search through",
    r"^You want a locksmith to open",
    r"^This .* has the Liquid Extractor unlock",
    r"An iron portcullis is raised",
    r"^(?:Magic Items|Special|Armor|Weapons|Containers|Wands|Gems|Herbs|Food/Drink|Clothing|Misc) \[\d+\]: ",
    r"^\s*>\s*$",
];

/// The lookout's settings (`lookout.toml`).
///
/// The field order below is the *declaration* order (the order the `Serialize`
/// impl walks); the comment on each field is its offset in the 128-byte
/// struct, which Rust reordered for alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// How many times a key must be seen before it is learnt (`+0x7b`, 5).
    pub occasions: u8,
    /// How long a repeated line stays quiet, in seconds (`+0x48`, 2.0).
    pub window: f64,
    /// The stat value below which a line is not a judgement (`+0x50`, 1500).
    pub stat_floor: u64,
    /// How many days a counted key survives (`+0x70`, 30).
    pub forget_days: u32,
    /// How long a key stays quiet, in seconds (`+0x74`, 60).
    pub quiet_secs: u32,
    /// A level marker the caller sets (`+0x78`, 0).
    pub mark: u8,
    /// A paused lookout reads nothing (`+0x79`, false).
    pub paused: bool,
    /// The lines the lookout never reads (`+0x00`, empty).
    pub off: Vec<String>,
    /// The patterns it ignores; `None` means [`IGNORED_BY_DEFAULT`]
    /// (`+0x30`).
    pub ignore: Option<Vec<String>>,
    /// The lines it always reads (`+0x18`, empty).
    pub always: Vec<String>,
    /// Whether the trigger has been added (`+0x7a`, false).
    pub trigger_added: bool,
    /// The keys the log seed has already been replayed for (`+0x58`, empty).
    ///
    /// **UNKNOWN**: the element type — it serialises through a *different*
    /// `Seq` impl than `off`/`always` (`0x101ac8920` vs `0x101ac88a0`), so the
    /// original is not a `Vec<String>`; `Vec<u64>` matches the `u64` keys the
    /// lookout uses everywhere else.
    pub seeded: Vec<u64>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            occasions: 5,
            window: 2.0,
            stat_floor: 1500,
            forget_days: crate::DEFAULT_FORGET_DAYS,
            quiet_secs: 60,
            mark: 0,
            paused: false,
            off: Vec::new(),
            ignore: None,
            always: Vec::new(),
            trigger_added: false,
            seeded: Vec::new(),
        }
    }
}

impl Settings {
    /// The patterns this lookout ignores (`0x101ac89ac`).
    ///
    /// The clone path (`Vec::clone` at `0x101ac89cc`) is taken when `ignore` is
    /// `Some`; otherwise the 28 built-ins are collected.
    pub fn ignore_list(&self) -> Vec<String> {
        match &self.ignore {
            Some(ignore) => ignore.clone(),
            None => IGNORED_BY_DEFAULT
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
        }
    }

    /// Is `line` one the lookout does not read?  (`0x101ac8e68`)
    ///
    /// The original finds the first `:` in the line (`memchr`), keeps what
    /// follows it as the tag, and then matches every entry of `off` (trimmed,
    /// ASCII-case-insensitively) against **both** the whole line and the tag.
    pub fn is_off(&self, line: &str) -> bool {
        let tag = match line.split_once(':') {
            Some((_, rest)) => rest,
            None => line,
        };
        self.off.iter().any(|pattern| {
            let pattern = pattern.trim();
            pattern.eq_ignore_ascii_case(tag) || pattern.eq_ignore_ascii_case(line)
        })
    }

    /// Read `lookout.toml` out of `dir` (`0x101ac89d0`).
    ///
    /// A missing file is the default settings; a file that cannot be read or
    /// parsed is `"<path>: <error>"`.
    pub fn read(dir: &Path) -> Result<Settings, String> {
        let path = dir.join(FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default())
            }
            Err(error) => return Err(format!("{}: {}", path.display(), error)),
        };
        match parse(&text) {
            Ok(settings) => Ok(settings),
            Err(error) => Err(format!("{}: {}", path.display(), error)),
        }
    }

    /// Write `lookout.toml` into `dir` (`0x101ac8c94`).
    pub fn write(&self, dir: &Path) -> Result<(), String> {
        let text = to_toml(self);
        file::write(&dir.join(FILE), text.as_bytes()).map_err(|problem| problem.text)
    }

    /// Makes the lookout read (or stop reading) the line whose tag is given
    /// (`0x101ac9054`).
    ///
    /// The same first-colon split as [`Settings::is_off`]; the entry is removed
    /// from `off` either way and only pushed back when `reads` is `false`.
    pub fn set_reads(&mut self, line: &str, reads: bool) {
        let tag = match line.split_once(':') {
            Some((_, rest)) => rest,
            None => line,
        };
        let key = tag.to_string();
        self.off
            .retain(|pattern| !pattern.trim().eq_ignore_ascii_case(&key));
        if !reads {
            self.off.push(key);
        }
    }
}

/// Render `settings` the way `toml::to_string_pretty` does.
///
/// Only the twelve recovered keys are written, in declaration order, and the
/// ones `Serialize` skips are skipped here too.
pub fn to_toml(settings: &Settings) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "occasions = {}", settings.occasions);
    let _ = writeln!(out, "window = {}", float(settings.window));
    let _ = writeln!(out, "stat_floor = {}", settings.stat_floor);
    let _ = writeln!(out, "forget_days = {}", settings.forget_days);
    let _ = writeln!(out, "quiet_secs = {}", settings.quiet_secs);
    let _ = writeln!(out, "mark = {}", settings.mark);
    let _ = writeln!(out, "paused = {}", settings.paused);
    if !settings.off.is_empty() {
        let _ = writeln!(out, "off = {}", strings(&settings.off));
    }
    if let Some(ignore) = &settings.ignore {
        let _ = writeln!(out, "ignore = {}", strings(ignore));
    }
    if !settings.always.is_empty() {
        let _ = writeln!(out, "always = {}", strings(&settings.always));
    }
    let _ = writeln!(out, "trigger_added = {}", settings.trigger_added);
    if !settings.seeded.is_empty() {
        let numbers: Vec<String> = settings.seeded.iter().map(|n| n.to_string()).collect();
        let _ = writeln!(out, "seeded = [{}]", numbers.join(", "));
    }
    out
}

/// `2.0` rather than `2` — what a TOML float looks like.
fn float(value: f64) -> String {
    let text = format!("{}", value);
    if text.contains(['.', 'e', 'E', 'n', 'i']) {
        text
    } else {
        format!("{text}.0")
    }
}

fn strings(values: &[String]) -> String {
    let quoted: Vec<String> = values.iter().map(|value| quoted(value)).collect();
    format!("[{}]", quoted.join(", "))
}

fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Parse the settings file: `key = value` lines, `#` comments, nothing else.
///
/// **UNKNOWN**: whether the original's TOML requires every key.  `serde`'s
/// derive would report a missing field, but a hand-edited `lookout.toml` is the
/// obvious use of the file, so a missing key keeps its default here.  An
/// unknown key is an error, exactly as the `unknown_field` call at
/// `0x101ac94a0` makes it for the original.
pub fn parse(text: &str) -> Result<Settings, String> {
    let mut settings = Settings::default();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("line {}: expected `key = value`", number + 1));
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "occasions" => settings.occasions = integer(value)?.clamp(0, u8::MAX as i64) as u8,
            "window" => settings.window = decimal(value)?,
            "stat_floor" => {
                settings.stat_floor = integer(value)?.clamp(0, i64::MAX) as u64
            }
            "forget_days" => {
                settings.forget_days = integer(value)?.clamp(0, u32::MAX as i64) as u32
            }
            "quiet_secs" => settings.quiet_secs = integer(value)?.clamp(0, u32::MAX as i64) as u32,
            "mark" => settings.mark = integer(value)?.clamp(0, u8::MAX as i64) as u8,
            "paused" => settings.paused = boolean(value)?,
            "trigger_added" => settings.trigger_added = boolean(value)?,
            "off" => settings.off = string_array(value)?,
            "always" => settings.always = string_array(value)?,
            "ignore" => settings.ignore = Some(string_array(value)?),
            "seeded" => settings.seeded = number_array(value)?,
            other => {
                return Err(format!(
                    "unknown field `{other}`, expected one of `occasions`, `window`, \
                     `stat_floor`, `forget_days`, `quiet_secs`, `mark`, `paused`, `off`, \
                     `ignore`, `always`, `trigger_added`, `seeded`"
                ))
            }
        }
    }
    Ok(settings)
}

fn integer(value: &str) -> Result<i64, String> {
    value
        .replace('_', "")
        .parse::<i64>()
        .map_err(|_| format!("expected an integer, found `{value}`"))
}

fn decimal(value: &str) -> Result<f64, String> {
    value
        .replace('_', "")
        .parse::<f64>()
        .map_err(|_| format!("expected a float, found `{value}`"))
}

fn boolean(value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected a boolean, found `{other}`")),
    }
}

fn string_array(value: &str) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| format!("expected an array, found `{value}`"))?;
    let mut out = Vec::new();
    let mut chars = inner.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        match chars.peek() {
            None => return Ok(out),
            Some('"') => {
                chars.next();
            }
            Some(other) => return Err(format!("expected a string, found `{other}`")),
        }
        let mut piece = String::new();
        loop {
            match chars.next() {
                None => return Err("unterminated string".to_string()),
                Some('"') => break,
                Some('\\') => match chars.next() {
                    Some('n') => piece.push('\n'),
                    Some('t') => piece.push('\t'),
                    Some('r') => piece.push('\r'),
                    Some('"') => piece.push('"'),
                    Some('\\') => piece.push('\\'),
                    Some(other) => return Err(format!("unknown escape `\\{other}`")),
                    None => return Err("unterminated escape".to_string()),
                },
                Some(other) => piece.push(other),
            }
        }
        out.push(piece);
    }
}

fn number_array(value: &str) -> Result<Vec<u64>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(|| format!("expected an array, found `{value}`"))?;
    let mut out = Vec::new();
    for piece in inner.split(',') {
        let piece = piece.trim().replace('_', "");
        if piece.is_empty() {
            continue;
        }
        out.push(
            piece
                .parse::<u64>()
                .map_err(|_| format!("expected an integer, found `{piece}`"))?,
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_ones_the_binary_builds() {
        let settings = Settings::default();
        assert_eq!(settings.occasions, 5);
        assert_eq!(settings.window, 2.0);
        assert_eq!(settings.stat_floor, 1500);
        assert_eq!(settings.forget_days, 30);
        assert_eq!(settings.quiet_secs, 60);
        assert_eq!(settings.mark, 0);
        assert!(!settings.paused);
        assert!(!settings.trigger_added);
        assert!(settings.off.is_empty());
        assert!(settings.always.is_empty());
        assert!(settings.seeded.is_empty());
        assert_eq!(settings.ignore, None);
        // 128 bytes in the original, eight quadwords of fields plus the Vecs
        assert_eq!(std::mem::size_of::<Settings>(), 128);
    }

    #[test]
    fn the_ignore_list_falls_back_on_the_twenty_eight_patterns() {
        let settings = Settings::default();
        let ignore = settings.ignore_list();
        assert_eq!(ignore.len(), 28);
        assert_eq!(ignore[0], r"^Obvious (?:exits|paths): ");
        assert_eq!(ignore[1], r"^\[[^\]]+\](?: \(\d+\))?$");
        assert_eq!(ignore[26], r"^(?:Magic Items|Special|Armor|Weapons|Containers|Wands|Gems|Herbs|Food/Drink|Clothing|Misc) \[\d+\]: ");
        assert_eq!(ignore[27], r"^\s*>\s*$");
    }

    #[test]
    fn a_set_ignore_list_replaces_the_defaults() {
        let settings = Settings {
            ignore: Some(vec!["^nothing".to_string()]),
            ..Settings::default()
        };
        assert_eq!(settings.ignore_list(), vec!["^nothing".to_string()]);
    }

    #[test]
    fn the_off_list_matches_the_whole_line_or_the_tag_after_a_colon() {
        let settings = Settings {
            off: vec!["  Kobold ".to_string()],
            ..Settings::default()
        };
        assert!(settings.is_off("kobold"));
        // the tag is everything after the first colon, untrimmed
        assert!(settings.is_off("Ydella:kobold"));
        assert!(!settings.is_off("Ydella: kobold"));
        assert!(!settings.is_off("The kobold blinks."));
        // a line with no colon is matched whole
        assert!(!settings.is_off("something else"));
        // the tag is everything after the first colon
        assert!(!settings.is_off("Kobold: says hello"));
    }

    #[test]
    fn setting_reads_adds_and_removes_the_tag() {
        let mut settings = Settings::default();
        settings.set_reads("Ydella:the kobold", false);
        // the first colon splits the line; the tag keeps everything after it
        assert_eq!(settings.off, vec!["the kobold".to_string()]);
        assert!(settings.is_off("Ydella:the kobold"));
        // setting it again does not duplicate
        settings.set_reads("Ydella:the kobold", false);
        assert_eq!(settings.off, vec!["the kobold".to_string()]);
        settings.set_reads("Ydella:the kobold", true);
        assert!(settings.off.is_empty());
        assert!(!settings.is_off("the kobold"));
    }

    #[test]
    fn the_settings_file_round_trips_through_toml() {
        let settings = Settings {
            occasions: 9,
            window: 0.5,
            stat_floor: 20,
            forget_days: 7,
            quiet_secs: 3,
            mark: 2,
            paused: true,
            off: vec!["the kobold".to_string(), "a \"quoted\" one".to_string()],
            ignore: Some(vec!["^Obvious".to_string()]),
            always: vec!["a rusty sword".to_string()],
            trigger_added: true,
            seeded: vec![1, 2, 3],
        };
        let text = to_toml(&settings);
        assert!(text.contains("window = 0.5\n"));
        assert!(text.contains("ignore = [\"^Obvious\"]\n"));
        assert_eq!(parse(&text).expect("parsed"), settings);
    }

    #[test]
    fn the_written_file_is_what_a_toml_reader_expects() {
        let text = to_toml(&Settings::default());
        // the empty lists and the absent ignore list are skipped
        assert!(!text.contains("off ="));
        assert!(!text.contains("always ="));
        assert!(!text.contains("ignore ="));
        assert!(!text.contains("seeded ="));
        assert_eq!(text.lines().count(), 8);
        assert!(text.starts_with("occasions = 5\nwindow = 2.0\n"));
    }

    #[test]
    fn a_missing_settings_file_is_the_defaults() {
        let dir = std::env::temp_dir().join(format!("hydra-lookout-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Settings::read(&dir).expect("absent"), Settings::default());
    }

    #[test]
    fn settings_are_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("hydra-lookout-wr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let settings = Settings {
            paused: true,
            off: vec!["the kobold".to_string()],
            ..Settings::default()
        };
        settings.write(&dir).expect("written");
        assert!(dir.join(FILE).exists());
        assert_eq!(Settings::read(&dir).expect("read back"), settings);
    }

    #[test]
    fn an_unknown_key_is_rejected() {
        let error = parse("paused = true\nnonsense = 1\n").expect_err("unknown");
        assert!(error.starts_with("unknown field `nonsense`"), "{error}");
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_the_missing_keys() {
        let settings = parse("window = 4.5\n").expect("parsed");
        assert_eq!(settings.window, 4.5);
        assert_eq!(settings.stat_floor, 1500);
        assert_eq!(settings.forget_days, 30);
    }
}
