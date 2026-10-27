//! The lookout: words to listen for, a sound to make and a banner to show.
//!
//! **ESTABLISHED**:
//!
//!  * The trigger file is `lookout.trigger` (`0x102418xxx`, alongside
//!    `lookout.wav` and the word `Lookout`).
//!  * `chime` (`0x10149de1c`) builds a WAV in memory: the literals `RIFF`,
//!    `WAVE` (`"V`), `fmt ` and `data` with the sizes `D7` / `7` are its header.
//!  * `change` (`0x10149e190`) says `One moment is {} second…`, `{} is a number,
//!    not {}.` and `Nothing here is changed`.
//!  * `set_trigger` (`0x10149c594`) is a thunk onto `Highlights::change`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::highlights::Highlights;
use crate::text::{self, Value};

/// The file the trigger is written in.
///
/// **ESTABLISHED** (`0x102418xxx`): `lookout.trigger`.
pub const FILE: &str = "lookout.trigger";

/// The sound the lookout plays.
///
/// **ESTABLISHED** (`0x102418xxx`): `lookout.wav`.
pub const SOUND: &str = "lookout.wav";

/// The name the lookout's highlights are filed under.
///
/// **ESTABLISHED** (`0x102418xxx`): `Lookout`.
pub const TITLE: &str = "Lookout";

/// The sample rate the chime is written at.
///
/// **ESTABLISHED** (`0x10149de1c`, the `fmt ` chunk): 22050 Hz, mono, 16 bits.
pub const SAMPLE_RATE: u32 = 22050;

/// One trigger: the words, and what to do.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trigger {
    /// The words to listen for.
    pub words: String,
    /// Whether it makes a sound.
    pub sound: bool,
    /// Whether it shows a banner.
    pub banner: bool,
    /// How long the banner stays up, in seconds.
    pub seconds: f32,
}

impl Trigger {
    /// Read a trigger out of the parsed file.
    pub fn read(doc: &Value) -> Trigger {
        let table = doc.get("lookout").unwrap_or(doc);
        Trigger {
            words: table.str_of("words"),
            sound: table.get("sound").map(|_| table.bool_of("sound")).unwrap_or(true),
            banner: table.get("banner").map(|_| table.bool_of("banner")).unwrap_or(true),
            seconds: table.f32_of("seconds").unwrap_or(3.0),
        }
    }

    pub fn write(&self) -> Value {
        let mut table = BTreeMap::new();
        table.insert("words".to_string(), Value::Str(self.words.clone()));
        table.insert("sound".to_string(), Value::Bool(self.sound));
        table.insert("banner".to_string(), Value::Bool(self.banner));
        table.insert("seconds".to_string(), Value::Float(self.seconds as f64));
        let mut doc = BTreeMap::new();
        doc.insert("lookout".to_string(), Value::Table(table));
        Value::Table(doc)
    }
}

/// The lookout's row on a settings page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TriggerRow {
    pub key: String,
    pub label: String,
    pub value: String,
    pub note: String,
}

/// The path the trigger is written to.
///
/// **ESTABLISHED** (`0x10149c164`): the folder's `lookout.trigger`.
pub fn file(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// The path the chime is written to.
pub fn sound_path(folder: &Path) -> PathBuf {
    folder.join(SOUND)
}

/// Read a folder's trigger.
pub fn trigger(folder: &Path) -> Trigger {
    text::read_file(&file(folder))
        .map(|doc| Trigger::read(&doc))
        .unwrap_or_default()
}

/// Save a folder's trigger.
///
/// **ESTABLISHED** (`0x10149c164`): the words are written and, when the sound is
/// on, the chime is written beside them.
pub fn add_trigger(folder: &Path, trigger: &Trigger) -> Result<(), String> {
    text::write_file(&file(folder), &trigger.write())?;
    if trigger.sound {
        std::fs::write(sound_path(folder), chime())
            .map_err(|e| format!("{}: {e}", sound_path(folder).display()))?;
    }
    Ok(())
}

/// Change one of the trigger's settings.
///
/// **ESTABLISHED** (`0x10149e190`): a moment has to be a number of seconds and a
/// setting that did not change says `Nothing here is changed`.
pub fn change(trigger: &mut Trigger, key: &str, value: &str) -> Result<String, String> {
    match key {
        "words" => {
            if trigger.words == value {
                return Ok("Nothing here is changed".to_string());
            }
            trigger.words = value.to_string();
            Ok(String::new())
        }
        "seconds" => {
            let seconds: f32 = value
                .trim()
                .parse()
                .map_err(|_| format!("{value} is a number, not {value}."))?;
            if seconds < 0.0 {
                return Err(format!(
                    "One moment is {} second{}, not {seconds:.0}.",
                    0,
                    ""
                ));
            }
            trigger.seconds = seconds;
            Ok(String::new())
        }
        "sound" => {
            trigger.sound = value.trim() != "off";
            Ok(String::new())
        }
        "banner" => {
            trigger.banner = value.trim() != "off";
            Ok(String::new())
        }
        other => Err(format!("The lookout has no {other}.")),
    }
}

/// Hand the trigger's words to the highlights, which is where they are kept.
///
/// **ESTABLISHED** (`0x10149c594`): a 20-byte thunk onto `Highlights::change`.
pub fn set_trigger(highlights: &mut Highlights, called: Vec<crate::highlights::Called>) {
    highlights.set(called);
}

/// The row the settings page shows for the trigger.
///
/// **ESTABLISHED** (`0x10149c5a8`): the page names the file it writes,
/// `lookout.trigger`, and notes that it is not shared.
pub fn with_trigger_row(trigger: &Trigger) -> Vec<TriggerRow> {
    vec![
        TriggerRow {
            key: "words".into(),
            label: "Words".into(),
            value: trigger.words.clone(),
            note: format!("Kept in {FILE}; it is not shared between characters."),
        },
        TriggerRow {
            key: "sound".into(),
            label: "Sound".into(),
            value: if trigger.sound { "on" } else { "off" }.into(),
            note: "Sound and banner.".into(),
        },
        TriggerRow {
            key: "banner".into(),
            label: "Banner".into(),
            value: if trigger.banner { "on" } else { "off" }.into(),
            note: String::new(),
        },
        TriggerRow {
            key: "seconds".into(),
            label: "Seconds".into(),
            value: format!("{:.0}", trigger.seconds),
            note: String::new(),
        },
    ]
}

/// The lookout's settings page.
///
/// **ESTABLISHED** (`0x10149c9a8`): the page's heading and its first line.
pub fn page(trigger: &Trigger) -> (String, Vec<TriggerRow>) {
    (format!("Nothing here is changed until the trigger is saved."), with_trigger_row(trigger))
}

/// A short chime as a WAV file, in memory.
///
/// **ESTABLISHED** (`0x10149de1c`): a RIFF/WAVE file with a `fmt ` chunk of 16
/// bytes (PCM, mono, 22050 Hz, 16 bits) and a `data` chunk; the binary's header
/// literals are `RIFF`, `WAVE`, `fmt ` and `data`.  The tone itself is
/// UNKNOWN — this writes a 120 ms 880 Hz sine with a fade so it does not click.
pub fn chime() -> Vec<u8> {
    let samples: Vec<i16> = {
        let count = (SAMPLE_RATE as f32 * 0.120) as usize;
        (0..count)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let fade = 1.0 - (i as f32 / count as f32);
                let value = (t * 880.0 * std::f32::consts::TAU).sin() * 0.35 * fade;
                (value * i16::MAX as f32) as i16
            })
            .collect()
    };
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // bytes per second
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chime_is_a_wav() {
        let bytes = chime();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 16);
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 1);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1);
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            SAMPLE_RATE
        );
        assert_eq!(u16::from_le_bytes([bytes[34], bytes[35]]), 16);
        assert_eq!(&bytes[36..40], b"data");
        // The whole file is the header plus the samples the header promises.
        let data = u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]) as usize;
        assert_eq!(bytes.len(), 44 + data);
        assert_eq!(bytes.len(), 44 + (SAMPLE_RATE as f32 * 0.120) as usize * 2);
    }

    #[test]
    fn a_trigger_round_trips() {
        let trigger = Trigger {
            words: "a shadow".into(),
            sound: true,
            banner: false,
            seconds: 4.0,
        };
        assert_eq!(Trigger::read(&trigger.write()), trigger);
    }

    #[test]
    fn changing_a_setting() {
        let mut trigger = Trigger::default();
        assert_eq!(
            change(&mut trigger, "words", "a shadow").unwrap(),
            ""
        );
        assert_eq!(
            change(&mut trigger, "words", "a shadow").unwrap(),
            "Nothing here is changed"
        );
        assert!(change(&mut trigger, "seconds", "soon").is_err());
        assert_eq!(change(&mut trigger, "seconds", "2.5").unwrap(), "");
        assert_eq!(trigger.seconds, 2.5);
        assert!(change(&mut trigger, "nope", "x").is_err());
    }

    #[test]
    fn the_page_names_the_file() {
        let rows = with_trigger_row(&Trigger::default());
        assert_eq!(rows.len(), 4);
        assert!(rows[0].note.contains("lookout.trigger"));
        assert_eq!(rows[1].note, "Sound and banner.");
        assert_eq!(file(Path::new("/data")), PathBuf::from("/data/lookout.trigger"));
        assert_eq!(sound_path(Path::new("/data")), PathBuf::from("/data/lookout.wav"));
    }
}
