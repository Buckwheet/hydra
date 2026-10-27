//! Highlights: the words the board listens for and what it does when it hears
//! them.
//!
//! **ESTABLISHED**:
//!
//!  * The file is `highlights.json` (the serde types `Called`, `Highlights` and
//!    `Watched`; `0x1014a69a8` for `load`).
//!  * `Highlights::call` (`0x1014a5758`) and `call_once` (`0x1014a6708`) are the
//!    two ways a line is announced; `hushed` (`0x1014a65ec`) is the volume being
//!    zero; `generation` (`0x1014a50e4`) changes whenever the set changes;
//!    `set_volume` (`0x1014a50f0`), `set_merchants` (`0x1014a55c0`) and `set`
//!    (`0x1014a55d0`) are the writers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::text::{self, Value};

/// The file highlights live in.
pub const FILE: &str = "highlights.json";

/// One thing worth being told about.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::highlights::Called`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Called {
    /// The words to listen for.
    pub words: String,
    /// The colour to say them in.
    pub color: String,
    /// The sound to play, when there is one.
    pub sound: Option<String>,
    /// Whether it is on.
    pub on: bool,
}

/// The whole highlights file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Highlights {
    /// The words, in file order.
    pub called: Vec<Called>,
    /// The volume, 0.0 to 1.0.
    pub volume: f32,
    /// Whether merchant announcements are highlighted.
    pub merchants: bool,
    /// Bumped whenever the set changes, so the story rereads it.
    pub generation: u32,
    /// What has already been announced, by key.
    pub said: BTreeMap<String, String>,
}

impl Highlights {
    /// The highlights a data folder gets before it has a file.
    pub fn defaults() -> Highlights {
        Highlights {
            called: Vec::new(),
            volume: 1.0,
            merchants: true,
            generation: 0,
            said: BTreeMap::new(),
        }
    }

    /// **ESTABLISHED** (`0x1014a69a8`): the file, read again.
    pub fn load(folder: &Path) -> Result<Highlights, String> {
        let path = file(folder);
        if !path.exists() {
            return Ok(Highlights::defaults());
        }
        Highlights::read(&text::read_file(&path)?)
    }

    /// Read the document.
    pub fn read(doc: &Value) -> Result<Highlights, String> {
        let mut highlights = Highlights::defaults();
        highlights.volume = doc.f32_of("volume").unwrap_or(1.0);
        highlights.merchants = doc.get("merchants").map(|_| doc.bool_of("merchants")).unwrap_or(true);
        highlights.generation = doc.u64_of("generation").unwrap_or(0) as u32;
        if let Some(called) = doc.get("called").and_then(Value::as_array) {
            highlights.called = called
                .iter()
                .map(|item| Called {
                    words: item.str_of("words"),
                    color: item.str_of("color"),
                    sound: item.get("sound").and_then(Value::as_str).map(str::to_string),
                    on: item.get("on").map(|_| item.bool_of("on")).unwrap_or(true),
                })
                .collect();
        }
        Ok(highlights)
    }

    /// Write the document.
    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("volume".to_string(), Value::Float(self.volume as f64));
        map.insert("merchants".to_string(), Value::Bool(self.merchants));
        map.insert("generation".to_string(), Value::Int(self.generation as i64));
        map.insert(
            "called".to_string(),
            Value::Array(
                self.called
                    .iter()
                    .map(|called| {
                        let mut item = BTreeMap::new();
                        item.insert("words".to_string(), Value::Str(called.words.clone()));
                        item.insert("color".to_string(), Value::Str(called.color.clone()));
                        if let Some(sound) = &called.sound {
                            item.insert("sound".to_string(), Value::Str(sound.clone()));
                        }
                        item.insert("on".to_string(), Value::Bool(called.on));
                        Value::Table(item)
                    })
                    .collect(),
            ),
        );
        Value::Table(map)
    }

    /// The generation, which changes when the set does.
    ///
    /// **ESTABLISHED** (`0x1014a50e4`).
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Is the volume off?
    ///
    /// **ESTABLISHED** (`0x1014a65ec`): a volume of zero hushes everything.
    pub fn hushed(&self) -> bool {
        self.volume <= 0.0
    }

    /// Set the volume.
    ///
    /// **ESTABLISHED** (`0x1014a50f0`): clamped into `0.0 ..= 1.0`, and the
    /// generation moves when it changes.
    pub fn set_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        if (volume - self.volume).abs() > f32::EPSILON {
            self.volume = volume;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    /// Whether merchant announcements are highlighted.
    ///
    /// **ESTABLISHED** (`0x1014a55c0`).
    pub fn set_merchants(&mut self, on: bool) {
        if self.merchants != on {
            self.merchants = on;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    /// Replace the words.
    ///
    /// **ESTABLISHED** (`0x1014a55d0`).
    pub fn set(&mut self, called: Vec<Called>) {
        self.called = called;
        self.generation = self.generation.wrapping_add(1);
    }

    /// The highlights for one character: its own words, else the shared ones.
    ///
    /// **PARTLY** (`0x1014a510c`): the binary picks the character's own file
    /// when it has one.  UNKNOWN how the two are named apart, so this takes the
    /// already-parsed pair.
    pub fn for_character(shared: &Highlights, own: Option<&Highlights>) -> Highlights {
        own.cloned().unwrap_or_else(|| shared.clone())
    }

    /// The file the highlights are kept in.
    ///
    /// **ESTABLISHED** (`0x1014a58a8`): the folder's `highlights.json`.
    pub fn file(folder: &Path) -> PathBuf {
        file(folder)
    }

    /// Announce a line.
    ///
    /// **PARTLY** (`0x1014a5758`): the line is queued with its colour and sound,
    /// and a hushed board says nothing.  UNKNOWN: the binary also takes the words
    /// that matched as their own argument; here they are the thing to look for.
    pub fn call(&mut self, words: &str, line: &str) -> String {
        let line = if words.is_empty() || line.contains(words) {
            line
        } else {
            return String::new();
        };
        if self.hushed() {
            return String::new();
        }
        let called = self
            .called
            .iter()
            .find(|called| called.on && line.contains(&called.words) && !called.words.is_empty());
        let color = called.map(|c| c.color.clone()).unwrap_or_default();
        let sound = called.and_then(|c| c.sound.clone());
        match sound {
            Some(sound) => format!("{line}\u{7}{sound}"),
            None if color.is_empty() => line.to_string(),
            None => format!("{line}\u{7}{color}"),
        }
    }

    /// Announce a line only the first time it is heard.
    ///
    /// **ESTABLISHED** (`0x1014a6708`): the key stands in for the line, so the
    /// same key is only ever shown once.
    pub fn call_once(&mut self, key: &str, line: &str, shown_ms: u32) -> String {
        if self.hushed() || self.said.contains_key(key) {
            return String::new();
        }
        self.said.insert(key.to_string(), line.to_string());
        let _ = shown_ms;
        line.to_string()
    }
}

/// The highlights file for a folder.
///
/// **ESTABLISHED** (`0x1014a58a8`).
pub fn file(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// Read a folder's highlights.
///
/// **ESTABLISHED** (`0x1014a69a8`).
pub fn load(folder: &Path) -> Result<Highlights, String> {
    Highlights::load(folder)
}

/// A watched file, reread when it changes.
///
/// **ESTABLISHED** (the drop glue for `hydra_board::highlights::Watched`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Watched {
    /// The file's modified time when it was last read.
    pub stamp: Option<std::time::SystemTime>,
    /// What was read.
    pub highlights: Highlights,
}

impl Watched {
    /// Read the file again if it has changed.
    ///
    /// **ESTABLISHED** (`0x1014a6a70`): the modified time answers whether the
    /// read is worth doing.
    pub fn read_again(&mut self, folder: &Path) -> bool {
        let path = file(folder);
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if stamp.is_some() && stamp == self.stamp {
            return false;
        }
        self.stamp = stamp;
        self.highlights = Highlights::load(folder).unwrap_or_else(|_| Highlights::defaults());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_zero_volume_hushes_everything() {
        let mut highlights = Highlights::defaults();
        assert!(!highlights.hushed());
        highlights.set_volume(0.0);
        assert!(highlights.hushed());
        assert_eq!(highlights.call("wolf", "a grey wolf"), "");
        assert_eq!(highlights.generation(), 1);
        // Setting the same volume again does not move the generation.
        highlights.set_volume(0.0);
        assert_eq!(highlights.generation(), 1);
    }

    #[test]
    fn a_called_word_gives_the_line_its_colour() {
        let mut highlights = Highlights {
            called: vec![Called {
                words: "wolf".into(),
                color: "#b03030".into(),
                sound: None,
                on: true,
            }],
            ..Highlights::defaults()
        };
        assert_eq!(
            highlights.call("wolf", "a grey wolf"),
            "a grey wolf\u{7}#b03030"
        );
        assert_eq!(highlights.call("bear", "a bear"), "a bear");
    }

    #[test]
    fn a_line_is_only_announced_once_per_key() {
        let mut highlights = Highlights::defaults();
        assert_eq!(highlights.call_once("k", "hello", 600), "hello");
        assert_eq!(highlights.call_once("k", "hello", 600), "");
        assert_eq!(highlights.call_once("k2", "hello", 600), "hello");
    }

    #[test]
    fn the_file_round_trips() {
        let mut highlights = Highlights::defaults();
        highlights.set(
            vec![Called {
                words: "wolf".into(),
                color: "red".into(),
                sound: Some("wolf.wav".into()),
                on: false,
            }],
        );
        highlights.set_merchants(false);
        let again = Highlights::read(&highlights.write()).unwrap();
        assert_eq!(again.called, highlights.called);
        assert!(!again.merchants);
        assert_eq!(again.generation, 2);
    }
}
