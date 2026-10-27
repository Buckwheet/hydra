//! Recovered from `hydra_lookout` (arm64 Mach-O, no DWARF).  See `NOTES.md`
//! for the address of every function and the evidence behind it.
//!
//! The lookout watches the game's lines: it learns which creatures and objects
//! are worth remembering, keeps a small set of counters on disk, and lets a
//! player mask ("off") or pin ("always") individual lines.  Everything is
//! std-only here: the original linked `regex`/`regex_automata` for the ignore
//! set and the stat pattern, `serde`/`toml` for `lookout.toml`, and `serde` for
//! the state file; the state file turned out to be a hand-rolled binary format
//! (`HYDRALK1`), so only the settings file needs a (small) TOML reader here.

#![allow(dead_code)]

pub mod file;
pub mod key;
pub mod settings;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hydra_model::pattern::{self, Pattern};
use settings::Settings;

/// How many days a counted key survives without being seen again
/// (`Settings::forget_days`, default 30, applied by `Lookout::snapshot`).
pub const DEFAULT_FORGET_DAYS: u32 = 30;

/// `Lookout::save_now_and_then` only writes when the last write is older than
/// this (`0x101ab3ea4`: `cmp x0, #0x12c` on the elapsed seconds).
pub const SAVE_EVERY: Duration = Duration::from_secs(300);

/// FNV-1a offset basis, inlined at every key site (`0x101ab49bc`).
pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a prime (see [`FNV_OFFSET`]).
pub const FNV_PRIME: u64 = 0x100_0000_01b3;

/// The prefix byte every sentence key is hashed with
/// (`Lookout::set`/`Lookout::forget`, both `mov w9, #0x53`).
pub const SENTENCE_TAG: u8 = b'S';
/// The prefix byte every object key is hashed with
/// (`Lookout::forget`, `mov w9, #0x4f`).
pub const OBJECT_TAG: u8 = b'O';

/// The stat pattern the lookout compiles at construction
/// (`0x1039c77b4`, 50 bytes: `\b(?:AS|DS|CS|TD|UAF|UDF|AvD|CvA)\s*:\s*([+-]?\d+)`).
pub const STAT_PATTERN: &str = r"\b(?:AS|DS|CS|TD|UAF|UDF|AvD|CvA)\s*:\s*([+-]?\d+)";

/// The most recent keys `Lookout::count` keeps before it prunes
/// (`0x101ab4f84`: `cmp x8, #0x1000`).
pub const MAX_RECENT: usize = 0x1000;

/// `Lookout::count` saturates the per-key occasion counter at 255
/// (`0x101ab5120`: `cmp w10, #0xff`).
pub const MAX_OCCASIONS: u8 = 0xff;

/// The state file's magic, `HYDRALK1` (`0x101abd528`, `0x101abd9d8`).
pub const STATE_MAGIC: [u8; 8] = *b"HYDRALK1";

/// Seconds in a day, as the float the day number is computed with
/// (`0x101ab6cf8`: `0x40f5_1800_0000_0000` = 86400.0).
pub const SECONDS_PER_DAY: f64 = 86_400.0;

/// A shared lookout: what `open` hands out and what `lock` unlocks.
pub type Shared = Arc<Mutex<Lookout>>;

/// What `OPEN` holds: the state-file path and the lookout behind it.
pub type Opened = (PathBuf, Shared);

/// Every lookout that has been opened, keyed by state-file path.
///
/// `0x10458a880`, `Mutex<Vec<(PathBuf, Arc<Mutex<Lookout>>)>>`.
static OPEN: LazyLock<Mutex<Vec<Opened>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// The generation last written per state-file path, so an older snapshot never
/// overwrites a newer one (`0x10458a8a8`, `Mutex<Vec<(PathBuf, u64)>>`).
static WRITTEN: LazyLock<Mutex<Vec<(PathBuf, u64)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// How many things this lookout knows, is counting, and has written.
///
/// `counts_all` builds one of these per lookout; the element of
/// `Vec<(String, Counts)>` is 48 bytes, so `Counts` is 24 (three machine
/// words).  **ESTABLISHED**: the three values copied out of the lookout are
/// `known.items()` (`Lookout+0x140`), `counting.items()` (`Lookout+0x170`) and
/// `written` (`Lookout+0x218`) — see `0x101aca588`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Keys the lookout has already learnt (`known.len()`).
    pub known: usize,
    /// Keys still being counted (`counting.len()`).
    pub counting: usize,
    /// The generation of the state file last written (`written`).
    pub written: u64,
}

/// One key the lookout is still counting towards its `occasions` threshold.
///
/// Value of the persisted map at `Lookout+0x158`: the day number at `+8` and
/// the occasion count at `+0xc` (13 bytes on disk, see [`file::encode`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counting {
    /// Days since the Unix epoch when the key was last counted.
    pub day: u32,
    /// How many times the key has been counted (`count` saturates at 255).
    pub occasions: u8,
}

/// The recent-keys map (`Lookout+0x188`), used for the quiet window.
///
/// Entries are 24 bytes in the original: an `f64` timestamp at `+0`, a `u32`
/// tag at `+8` and the occasion count at `+0xc` (`Lookout::count`,
/// `0x101ab50f0`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Recent {
    /// `now_seconds()` when the key was counted.
    pub at: f64,
    /// The tag the key was counted with; a different tag re-arms the window.
    pub tag: u32,
    /// The occasion count carried over from [`Counting`].
    pub occasions: u8,
}

/// A line the lookout judged worth remembering.
///
/// 96 bytes: three `String`s at `+0`, `+0x18`, `+0x30` and a `Vec<String>` at
/// `+0x48` (`Lookout::always`, `Lookout::forget`, `0x101ab6628`).
/// **INFERRED**: the field names — no symbol or literal names them.  The keys
/// `forget` derives from `objects[0]` (sentence tag) and `objects[1..]`
/// (object tag) are `ESTABLISHED`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shown {
    /// The speaker, e.g. `"Ydella"`; empty when the game itself spoke.
    pub said: String,
    /// The line as it arrived.
    pub line: String,
    /// Where it happened, when the line carried a room.
    pub room: String,
    /// The sentences and objects the line mentions; `[0]` is the sentence.
    pub objects: Vec<String>,
}

/// An object a lookout knows about.
///
/// `Vec<Object>` drops 40-byte elements with a single `String` at `+0`
/// (`0x101ab1d38`), so `Object` is a `String` plus 16 bytes of plain data.
/// **UNKNOWN**: which numbers those are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Object {
    /// The object's name as the game shows it.
    pub name: String,
    /// The object's key (an FNV hash of its normalised name).
    pub key: u64,
    /// The day the object was last seen.
    pub day: u32,
    /// How many occasions it has been counted.
    pub occasions: u8,
}

/// A line offered to [`Lookout::judge`].
///
/// The value `judge` is handed has a `String` at `+0x08` (its text) and an
/// 8-byte prefix (`0x101ab5234` reads `[x3+0x10]`/`[x3+0x18]` as that text's
/// pointer and length).  **UNKNOWN**: what the prefix is; `mark` keeps a tag
/// the caller can pass, which `count` forwards as its `tag` argument.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    /// A caller-supplied tag for the line.
    pub mark: u32,
    /// The line's text.
    pub text: String,
}

/// `pattern` with the leading `\b` dropped.
///
/// `hydra_model::pattern` has no word-boundary assertion (only anchors, `.`,
/// classes, groups and repetitions), so the recovered stat pattern is compiled
/// without its leading `\b`.  Every other construct in it is supported.
pub fn without_boundary(pattern: &str) -> &str {
    pattern.strip_prefix(r"\b").unwrap_or(pattern)
}

/// The FNV-1a hash of `tag` followed by `text`.
///
/// `Lookout::set` and `Lookout::forget` inline exactly this: the offset basis,
/// the tag byte, then `(hash ^ byte) * prime` per byte (`0x101ab49e4`).
pub fn key_hash(tag: u8, text: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    hash = (hash ^ tag as u64).wrapping_mul(FNV_PRIME);
    for byte in text.bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Lock a lookout, ignoring poisoning (`0x101ab32f4`).
///
/// The original builds the `MutexGuard` itself and forwards the poison flag;
/// `unwrap_or_else(into_inner)` is the equivalent.
pub fn lock(arc: &Arc<Mutex<Lookout>>) -> MutexGuard<'_, Lookout> {
    arc.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// Seconds since the Unix epoch, as a float (`0x101ab2fdc`).
///
/// `SystemTime::now().duration_since(UNIX_EPOCH)` split into
/// `secs as f64 + nanos as f64 / 1e9`; `0.0` when the clock is before the
/// epoch (the `movi.2d v0, #0` at `0x101ab300c`).
pub fn now_seconds() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_secs() as f64 + since.subsec_nanos() as f64 / 1e9,
        Err(_) => 0.0,
    }
}

/// Every lookout that is open (`0x101ab3ad4`).
fn opened() -> Vec<Arc<Mutex<Lookout>>> {
    let open = OPEN.lock().unwrap_or_else(|poison| poison.into_inner());
    open.iter().map(|(_, arc)| Arc::clone(arc)).collect()
}

/// Counts for every open lookout, by name (`0x101ab2ea8`).
pub fn counts_all() -> Vec<(String, Counts)> {
    opened()
        .iter()
        .map(|arc| {
            let lookout = lock(arc);
            (lookout.character.clone(), lookout.counts())
        })
        .collect()
}

/// The lookout for `character` in `dir`, opening it on first use
/// (`0x101ab3368`).
///
/// Returns the same `Arc` for a directory/name pair that is already open.  A
/// state file that cannot be decoded is renamed to `<name>.bad` and learning
/// starts over; a state file that cannot be read at all switches the lookout
/// off (`Lookout::shut`), so it never writes over what it could not read.
pub fn open(dir: &Path, character: &str) -> Arc<Mutex<Lookout>> {
    let wanted = file::path(dir, character);
    {
        let open = OPEN.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some((_, arc)) = open.iter().find(|(path, _)| *path == wanted) {
            return Arc::clone(arc);
        }
    }
    let settings = match Settings::read(dir) {
        Ok(settings) => settings,
        Err(problem) => {
            eprintln!(
                "[hydra] lookout: {}; its choices are the defaults\n",
                problem
            );
            Settings::default()
        }
    };
    let mut lookout = Lookout::new(dir, character, &settings);
    let path = file::path(dir, character);
    match file::read(&path) {
        Ok(loaded) => lookout.adopt(loaded),
        Err(problem) if problem.rename => {
            let bad = path.with_extension("bin.bad");
            if let Err(error) = std::fs::rename(&path, &bad) {
                eprintln!("[hydra] lookout: {}: {}\n", path.display(), error);
            }
            eprintln!(
                "[hydra] lookout: {} does not read ({}); set aside as {}, and learning starts again\n",
                path.display(),
                problem.text,
                bad.display()
            );
        }
        Err(problem) => {
            eprintln!(
                "[hydra] lookout: {}; the lookout is off until Hydra starts again\n",
                problem.text
            );
            lookout.shut = true;
        }
    }
    let arc = Arc::new(Mutex::new(lookout));
    {
        let mut open = OPEN.lock().unwrap_or_else(|poison| poison.into_inner());
        open.push((path, Arc::clone(&arc)));
    }
    arc
}

/// Re-read `lookout.toml` and push it into every open lookout
/// (`0x101ab3c40`).
///
/// A settings file that cannot be read falls back to the defaults silently
/// (unlike [`open`], which says so).
pub fn reload(dir: &Path) {
    let settings = Settings::read(dir).unwrap_or_default();
    for arc in opened() {
        let mut lookout = lock(&arc);
        lookout.set(&settings);
    }
}

/// One character's lookout: what it has learnt and what it is counting.
///
/// 560 bytes in the original (`0x230`), laid out as recovered from
/// `Lookout::new` (`0x101ab4064`), `Lookout::set` (`0x101ab4754`) and
/// `Lookout::snapshot` (`0x101ab6c78`):
///
/// | offset | field |
/// | --- | --- |
/// | `+0x000` | `dir: PathBuf` (24) |
/// | `+0x018` | `character: String` (24) |
/// | `+0x030` | `settings: Settings` (128) |
/// | `+0x0b0` | `own: Vec<String>` (24) |
/// | `+0x0c8` | `shown: VecDeque<Shown>` (32) |
/// | `+0x0e8` | `instant: Instant` (16) |
/// | `+0x0f8` | `always: HashSet<u64>` (48) |
/// | `+0x128` | `known: HashSet<u64>` (48) |
/// | `+0x158` | `counting: HashMap<u64, Counting>` (48) |
/// | `+0x188` | `recent: HashMap<u64, Recent>` (48) |
/// | `+0x1b8` | `ignore: Vec<Pattern>` (the original's `Option<RegexSet>`) |
/// | `+0x1d8` | `stats: Pattern` (the original's `Option<Regex>`) |
/// | `+0x1f8` | `owns: Option<Pattern>` (`Option<Regex>`) |
/// | `+0x218` | `written: u64` |
/// | `+0x220` | `snaps: u64` |
/// | `+0x228` | `dirty: bool` |
/// | `+0x229` | `shut: bool` |
pub struct Lookout {
    /// Where the state and settings files live.
    pub dir: PathBuf,
    /// The character this lookout watches for.
    pub character: String,
    /// Its choices, read from `lookout.toml`.
    pub settings: Settings,
    /// The names that are ours (`(?i)\b(?:<joined>)\b`).
    pub own: Vec<String>,
    /// The records it has shown, oldest first.
    pub shown: VecDeque<Shown>,
    /// When it last wrote.
    pub instant: Instant,
    /// The hashes of `settings.always`, rebuilt by [`Lookout::set`].
    pub always: HashSet<u64>,
    /// The keys it has learnt; the first section of the state file.
    pub known: HashSet<u64>,
    /// The keys it is still counting; the second section of the state file.
    pub counting: HashMap<u64, Counting>,
    /// The quiet-window bookkeeping, in memory only.
    pub recent: HashMap<u64, Recent>,
    /// The compiled ignore set.
    pub ignore: Vec<Pattern>,
    /// The compiled stat pattern (`STAT_PATTERN`).
    pub stats: Pattern,
    /// The compiled name pattern, when [`Lookout::own`] has been given a name.
    pub owns: Option<Pattern>,
    /// The generation of the last state file written.
    pub written: u64,
    /// How many snapshots this lookout has taken.
    pub snaps: u64,
    /// Something changed since the last write.
    pub dirty: bool,
    /// The state file could not be read, so it is never written over.
    pub shut: bool,
}

/// Flush every dirty lookout (`0x101ab6e7c`).
pub fn save_all() {
    for arc in opened() {
        let mut lookout = lock(&arc);
        if lookout.dirty {
            if let Err(problem) = lookout.write() {
                eprintln!("[hydra] lookout: {}\n", problem);
            }
        }
    }
}

impl Lookout {
    /// A fresh lookout for `dir`/`character` (`0x101ab4064`).
    pub fn new(dir: &Path, character: &str, settings: &Settings) -> Lookout {
        let mut lookout = Lookout {
            dir: dir.to_path_buf(),
            character: character.to_string(),
            settings: Settings::default(),
            own: Vec::new(),
            shown: VecDeque::new(),
            instant: Instant::now(),
            always: HashSet::new(),
            known: HashSet::new(),
            counting: HashMap::new(),
            recent: HashMap::new(),
            ignore: Vec::new(),
            stats: pattern::compile(without_boundary(STAT_PATTERN)),
            owns: None,
            written: 0,
            snaps: 0,
            dirty: false,
            shut: false,
        };
        lookout.set(settings);
        lookout
    }

    /// Take on a state file that was read successfully (`0x101ab36f8`).
    fn adopt(&mut self, loaded: file::Loaded) {
        self.known = loaded.known;
        self.counting = loaded.counting;
        self.written = loaded.len;
    }

    /// Replace the settings and rebuild what derives from them
    /// (`0x101ab4754`).
    pub fn set(&mut self, settings: &Settings) {
        let ignore = settings.ignore_list();
        let patterns: Vec<Pattern> = ignore.iter().map(|p| pattern::compile(p)).collect();
        if let Some((_, source)) = patterns
            .iter()
            .zip(ignore.iter())
            .find(|(compiled, _)| !compiled.is_ok())
        {
            eprintln!(
                "[hydra] lookout: the ignore pattern `{}` does not read\n",
                source
            );
        }
        self.ignore = patterns;
        self.always = settings
            .always
            .iter()
            .map(|line| key_hash(SENTENCE_TAG, line))
            .collect();
        self.settings = settings.clone();
    }

    /// Does this lookout read `text`?  (`0x101ab6424`)
    pub fn reads(&self, text: &str) -> bool {
        if self.settings.paused {
            false
        } else {
            !self.settings.is_off(text)
        }
    }

    /// The counts [`counts_all`] reports (`0x101aca588`).
    pub fn counts(&self) -> Counts {
        Counts {
            known: self.known.len(),
            counting: self.counting.len(),
            written: self.written,
        }
    }

    /// The most recent record that mentions `wanted` (`0x101ab4acc`).
    ///
    /// The original lowercases `wanted`, walks the deque from the newest record
    /// backwards and then wraps around to the oldest, and compares against the
    /// record's `said`/`line` fields.
    pub fn find(&self, said: &str, wanted: &str) -> Option<&Shown> {
        let lower = wanted.trim().to_lowercase();
        self.shown.iter().rev().find(|shown| {
            shown.line.eq_ignore_ascii_case(&lower)
                || shown.line.eq_ignore_ascii_case(wanted)
                || shown.said.eq_ignore_ascii_case(said.trim())
        })
    }

    /// Remember `name` as one of ours (`0x101ab4434`).
    ///
    /// Names are kept once (`eq_ignore_ascii_case`) and recompiled into the
    /// pattern `(?i)\b(?:{})\b` (`0x103e7ccb5`), joined with `|`.
    pub fn own(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if self.own.iter().any(|have| have.eq_ignore_ascii_case(name)) {
            return;
        }
        self.own.push(name.to_string());
        let alternation = self.own.join("|");
        let source = format!("(?i)\\b(?:{})\\b", alternation);
        let compiled = pattern::compile(&source);
        self.owns = if compiled.is_ok() { Some(compiled) } else { None };
    }

    /// Count `key` once (`0x101ab4d34`).
    ///
    /// Returns `false` when nothing changed: the key is already known, or it was
    /// counted inside `settings.window` seconds with a different tag.  Once a
    /// key reaches `settings.occasions` it moves from `counting` to `known`.
    pub fn count(&mut self, key: u64, tag: u32, day: u32, now: f64) -> bool {
        if self.known.contains(&key) {
            return false;
        }
        let window = self.settings.window;
        if let Some(recent) = self.recent.get(&key).copied() {
            let since = now - recent.at;
            if recent.tag != tag && since >= 0.0 && since < window {
                return false;
            }
        }
        let occasions = self.recent.get(&key).map(|r| r.occasions).unwrap_or(0);
        self.recent.insert(
            key,
            Recent {
                at: now,
                tag,
                occasions,
            },
        );
        if self.recent.len() > MAX_RECENT {
            self.recent.retain(|_, recent| now - recent.at < window);
        }
        let occasions = {
            let entry = self.counting.entry(key).or_default();
            entry.occasions = entry.occasions.saturating_add(1);
            entry.day = day;
            entry.occasions
        };
        if occasions >= self.settings.occasions {
            self.counting.remove(&key);
            self.known.insert(key);
            self.recent.remove(&key);
        }
        self.dirty = true;
        true
    }

    /// Judge one line (`0x101ab51d8`).
    ///
    /// **Partly recovered.**  Established: the early outs (a shut lookout or
    /// `settings.paused`, an ignored line, an empty line), that the stat regex
    /// is applied to the line and that a stat value below
    /// `settings.stat_floor` is not a judgement (`0x101ab5894`), and that the
    /// surviving lines are counted through [`Lookout::count`].  The exact set
    /// of keys one line produces is inlined across 4 KB of `regex_automata`
    /// capture handling and is *not* recovered; we count the sentence key and
    /// every object key the sentence mentions.
    pub fn judge(&mut self, said: &str, line: &Line, now: f64) -> bool {
        if self.shut || self.settings.paused {
            return false;
        }
        if self.settings.is_off(said) {
            return false;
        }
        let text = line.text.trim_end().trim();
        if text.is_empty() {
            return false;
        }
        let day = (now / SECONDS_PER_DAY).max(0.0) as u32;
        let mut counted = false;
        if self.stats.is_ok()
            && self.stats.is_match(text)
            && stat_reach(text).is_some_and(|reach| reach < self.settings.stat_floor)
        {
            return false;
        }
        let sentence = key::sentence(text);
        if !sentence.is_empty() {
            let key = key_hash(SENTENCE_TAG, &sentence);
            counted |= self.count(key, line.mark, day, now);
        }
        let objects = key::objects(text);
        for object in &objects {
            let key = key_hash(OBJECT_TAG, object);
            counted |= self.count(key, line.mark, day, now);
        }
        if counted {
            let objects = std::iter::once(sentence)
                .chain(objects)
                .filter(|part| !part.is_empty())
                .collect();
            self.shown.push_back(Shown {
                said: said.to_string(),
                line: text.to_string(),
                room: String::new(),
                objects,
            });
            self.dirty = true;
        }
        counted
    }

    /// Pin a record: add its sentence to `settings.always`, save the settings
    /// and drop the key from `known` so it is counted again (`0x101ab65f8`).
    pub fn always(&mut self, said: &str, wanted: &str) -> Result<(), String> {
        let Some(shown) = self.find(said, wanted) else {
            return Ok(());
        };
        let shown = shown.clone();
        let sentence = shown.objects.first().cloned().unwrap_or_default();
        let mut settings = self.settings.clone();
        if !settings.always.contains(&sentence) {
            settings.always.push(sentence.clone());
        }
        settings.write(&self.dir)?;
        let key = key_hash(SENTENCE_TAG, &sentence);
        self.known.remove(&key);
        self.set(&settings);
        Ok(())
    }

    /// Forget a record: drop its keys from `counting`, mark them `known` and
    /// remove it from the tail (`0x101ab69a4`).
    pub fn forget(&mut self, said: &str, wanted: &str) -> Option<Shown> {
        let shown = self.find(said, wanted)?.clone();
        for (index, part) in shown.objects.iter().enumerate() {
            let tag = if index == 0 { SENTENCE_TAG } else { OBJECT_TAG };
            let key = key_hash(tag, part);
            self.counting.remove(&key);
            self.known.insert(key);
        }
        self.shown.retain(|have| *have != shown);
        self.dirty = true;
        Some(shown)
    }

    /// Drop everything learnt, keeping the settings (`0x101ab4c34`).
    pub fn clear(&mut self) {
        self.known.clear();
        self.counting.clear();
        self.recent.clear();
        self.shown.clear();
        self.dirty = true;
    }

    /// The state file path, its bytes and the new generation
    /// (`0x101ab6c78`).
    ///
    /// Prunes `counting` keys older than `settings.forget_days` first.
    pub fn snapshot(&mut self) -> (PathBuf, Vec<u8>, u64) {
        let now = now_seconds();
        if self.settings.forget_days != 0 && !self.counting.is_empty() {
            let today = (now / SECONDS_PER_DAY).max(0.0) as u32;
            let forget = self.settings.forget_days;
            self.counting
                .retain(|_, counting| today.saturating_sub(counting.day) <= forget);
        }
        self.snaps += 1;
        let path = file::path(&self.dir, &self.character);
        let bytes = file::encode(&self.known, &self.counting);
        (path, bytes, self.snaps)
    }

    /// Write the state file (`0x101ab6450`).
    ///
    /// Refuses to write when the lookout could not read its file
    /// (`0x101ab6474`).
    pub fn write(&mut self) -> Result<(), String> {
        if self.shut {
            let path = file::path(&self.dir, &self.character);
            return Err(format!(
                "{} could not be read, so it is not written over",
                path.display()
            ));
        }
        let (path, bytes, generation) = self.snapshot();
        write_newest(&path, &bytes, generation)?;
        self.dirty = false;
        self.instant = Instant::now();
        self.written = generation;
        Ok(())
    }

    /// Write the state file at most every [`SAVE_EVERY`], on a background
    /// thread named `hydra-lookout` (`0x101ab3e6c`).
    pub fn save_now_and_then(&mut self) {
        if self.shut || !self.dirty {
            return;
        }
        if self.instant.elapsed() < SAVE_EVERY {
            return;
        }
        let (path, bytes, generation) = self.snapshot();
        self.dirty = false;
        self.instant = Instant::now();
        self.written = generation;
        let spawned = std::thread::Builder::new()
            .name("hydra-lookout".to_string())
            .spawn(move || {
                if let Err(problem) = write_newest(&path, &bytes, generation) {
                    eprintln!("[hydra] lookout: {}\n", problem);
                }
            });
        // UNKNOWN: the original drops the spawn result the same way (the handle
        // is never joined and a failed spawn is not reported).
        let _ = spawned;
    }
}

/// Write `bytes` to `path` unless a newer generation is already there
/// (`0x101ab3044`).
///
/// `WRITTEN` keeps the last generation per path so a slow writer cannot
/// replace a fresher file.
fn write_newest(path: &Path, bytes: &[u8], generation: u64) -> Result<(), String> {
    let mut written = WRITTEN.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((_, last)) = written.iter().find(|(known, _)| known == path) {
        if *last >= generation {
            return Ok(());
        }
    }
    file::write(path, bytes).map_err(|problem| problem.text)?;
    match written.iter_mut().find(|(known, _)| known == path) {
        Some((_, last)) => *last = generation,
        None => written.push((path.to_path_buf(), generation)),
    }
    Ok(())
}

/// The largest absolute stat value in `text`, if any — the number the lookout
/// compares with `settings.stat_floor` (`0x101ab584c`).
fn stat_reach(text: &str) -> Option<u64> {
    let mut reach: Option<u64> = None;
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let start = index;
        let mut sign = 1i64;
        if bytes[index] == b'+' || bytes[index] == b'-' {
            let negative = bytes[index] == b'-';
            index += 1;
            if index >= bytes.len() || !bytes[index].is_ascii_digit() {
                index = start + 1;
                continue;
            }
            if negative {
                sign = -1;
            }
        }
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let mut value = 0i64;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            value = value
                .saturating_mul(10)
                .saturating_add((bytes[index] - b'0') as i64);
            index += 1;
        }
        let value = (value * sign).unsigned_abs();
        reach = Some(reach.map_or(value, |have| have.max(value)));
    }
    reach
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookout(tag: &str) -> Lookout {
        let dir = std::env::temp_dir().join(format!("hydra-lookout-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Lookout::new(&dir, "Aelith", &Settings::default())
    }

    #[test]
    fn a_fresh_lookout_knows_nothing_and_is_not_dirty() {
        let lookout = lookout("fresh");
        assert_eq!(lookout.counts(), Counts::default());
        assert!(!lookout.dirty);
        assert!(!lookout.shut);
        assert!(lookout.reads("The kobold stares at you."));
        assert_eq!(lookout.character, "Aelith");
    }

    #[test]
    fn a_paused_lookout_reads_nothing() {
        let mut lookout = lookout("paused");
        let settings = Settings {
            paused: true,
            ..Settings::default()
        };
        lookout.set(&settings);
        assert!(!lookout.reads("The kobold stares at you."));
    }

    #[test]
    fn a_line_on_the_off_list_is_not_read() {
        let mut lookout = lookout("off");
        let settings = Settings {
            off: vec!["stares".to_string()],
            ..Settings::default()
        };
        lookout.set(&settings);
        assert!(!lookout.reads("stares"));
        // the tag after a colon is what the off list matches
        assert!(!lookout.reads("You:stares"));
        assert!(lookout.reads("The kobold blinks."));
    }

    #[test]
    fn keys_are_fnv1a_with_a_tag_byte() {
        // FNV-1a of a single byte is (offset ^ byte) * prime.
        assert_eq!(
            key_hash(b'S', ""),
            (FNV_OFFSET ^ 0x53).wrapping_mul(FNV_PRIME)
        );
        assert_ne!(key_hash(b'S', "kobold"), key_hash(b'O', "kobold"));
        assert_eq!(key_hash(b'S', "kobold"), key_hash(b'S', "kobold"));
    }

    #[test]
    fn a_key_becomes_known_after_enough_occasions() {
        let mut lookout = lookout("count");
        let key = key_hash(b'S', "kobold");
        // the default threshold is five occasions
        for occasion in 0..4 {
            assert!(lookout.count(key, 1, 20_000, 1_000.0 + occasion as f64 * 100.0));
            assert!(!lookout.known.contains(&key));
            assert_eq!(lookout.counting[&key].occasions, occasion + 1);
        }
        assert!(lookout.count(key, 1, 20_000, 1_400.0));
        assert!(lookout.known.contains(&key));
        assert!(!lookout.counting.contains_key(&key));
        assert!(lookout.dirty);
        // a known key never counts again
        assert!(!lookout.count(key, 1, 20_000, 2_000.0));
    }

    #[test]
    fn a_quiet_window_swallows_the_second_count() {
        let mut lookout = lookout("quiet");
        let key = key_hash(b'S', "kobold");
        // the same tag always counts: the window only silences a changed tag
        assert!(lookout.count(key, 1, 20_000, 1_000.0));
        assert!(lookout.count(key, 1, 20_000, 1_001.0));
        // a different tag inside `window` (2 s in the defaults) is swallowed
        assert!(!lookout.count(key, 2, 20_000, 1_002.0));
        assert!(lookout.count(key, 2, 20_000, 1_004.0));
    }

    #[test]
    fn an_empty_name_is_not_owned_and_names_are_not_duplicated() {
        let mut lookout = lookout("own");
        lookout.own("   ");
        assert!(lookout.own.is_empty());
        lookout.own("Aelith");
        lookout.own("  aelith ");
        assert_eq!(lookout.own, vec!["Aelith".to_string()]);
        assert!(lookout.owns.is_some());
    }

    #[test]
    fn a_judged_line_is_shown_and_forgetting_it_marks_its_keys_known() {
        let mut lookout = lookout("judge");
        let line = Line {
            mark: 7,
            text: "A kobold scratches you.".to_string(),
        };
        assert!(lookout.judge("Ydella", &line, 1_700_000_000.0));
        let shown = lookout.shown.back().cloned().expect("a record");
        assert_eq!(shown.said, "Ydella");
        assert_eq!(shown.line, "A kobold scratches you.");
        // the first key is the sentence, the rest are the objects
        assert_eq!(shown.objects[0], "a kobold scratches you");
        assert_eq!(shown.objects[1], "kobold scratches you.");

        let wanted = shown.line.clone();
        let forgotten = lookout.forget("Ydella", &wanted).expect("forgotten");
        assert_eq!(forgotten, shown);
        assert!(lookout.shown.is_empty());
        assert!(!lookout.known.is_empty());
    }

    #[test]
    fn clearing_keeps_the_settings() {
        let mut lookout = lookout("clear");
        lookout.count(key_hash(b'S', "kobold"), 1, 20_000, 1_000.0);
        lookout.clear();
        assert!(lookout.known.is_empty());
        assert!(lookout.counting.is_empty());
        assert!(lookout.recent.is_empty());
        assert!(lookout.shown.is_empty());
        assert!(lookout.dirty);
    }

    #[test]
    fn knowing_a_stat_line_below_the_floor_is_not_a_judgement() {
        let mut lookout = lookout("floor");
        let small = Line {
            mark: 1,
            text: "AS: 12, DS: 40".to_string(),
        };
        assert!(!lookout.judge("Ydella", &small, 1_700_000_000.0));
        let big = Line {
            mark: 1,
            text: "AS: 1500, DS: 40".to_string(),
        };
        assert!(lookout.judge("Ydella", &big, 1_700_000_000.0));
    }

    #[test]
    fn a_stat_reach_is_the_largest_absolute_value() {
        assert_eq!(stat_reach("AS: -42, DS: 7"), Some(42));
        assert_eq!(stat_reach("nothing here"), None);
    }

    #[test]
    fn writing_then_reading_a_state_file_round_trips() {
        let mut lookout = lookout("roundtrip");
        lookout.known.insert(key_hash(b'S', "kobold"));
        lookout.counting.insert(
            key_hash(b'O', "sword"),
            Counting {
                day: 20_000,
                occasions: 3,
            },
        );
        lookout.dirty = true;
        lookout.write().expect("written");
        let path = file::path(&lookout.dir, &lookout.character);
        assert!(path.exists());
        let loaded = file::read(&path).expect("read back");
        assert_eq!(loaded.known, lookout.known);
        assert_eq!(loaded.counting, lookout.counting);
    }

    #[test]
    fn a_second_open_returns_the_same_lookout() {
        let dir = std::env::temp_dir().join(format!("hydra-lookout-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = open(&dir, "Aelith");
        let second = open(&dir, "Aelith");
        assert!(Arc::ptr_eq(&first, &second));
        // the name is lowercased into the path, so the same lookout comes back
        let lower = open(&dir, "aelith");
        assert!(Arc::ptr_eq(&first, &lower));
        assert_eq!(lock(&first).character, "Aelith");
    }

    #[test]
    fn a_shut_lookout_will_not_write_over_its_file() {
        let mut lookout = lookout("shut");
        lookout.shut = true;
        let problem = lookout.write().expect_err("refused");
        assert!(problem.ends_with("could not be read, so it is not written over"));
        assert!(!file::path(&lookout.dir, &lookout.character).exists());
    }
}
