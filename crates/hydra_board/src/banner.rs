//! The banner across the top of the story.
//!
//! **ESTABLISHED**:
//!
//!  * The fields are `across`, `width`, `opacity`, `ground` and `seconds`
//!    (`0x102417f60`), plus the file's `forget_days` and `quiet_secs`.
//!  * The settings page's id is `hydra_banners` (`0x1014ab118`).
//!  * `look` (`0x1014aaed4`) turns the colours through
//!    `hydra_ui::theme::parse_hex`, which is a UI call and stands as a seam here.
//!  * `change` (`0x1014abe8c`) says `The Banners page has no …`, `A stat of {}
//!    or …`, `A percent from 10 to 10…` and `Banners are shown so from now.`

use std::collections::BTreeMap;

use crate::text::{self, Value};

/// The file the banner settings live in.
pub const FILE: &str = "banner.toml";

/// The id the settings page is registered under.
///
/// **ESTABLISHED** (`0x1014ab118`): `hydra_banners`.
pub const PAGE: &str = "hydra_banners";

/// A percentage has to be at least this.
///
/// **ESTABLISHED** (`0x1014abe8c`, "A percent from 10 to 100").
pub const PERCENT_LOW: f32 = 10.0;
/// And at most this.
pub const PERCENT_HIGH: f32 = 100.0;

/// How the banner is laid across the top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Across {
    /// Stretch to the window.
    #[default]
    Wide,
    /// Only as wide as the words.
    Words,
}

impl Across {
    /// **ESTABLISHED** (`0x102417f60`): the two spellings.
    pub fn parse(text: &str) -> Option<Across> {
        match text.trim() {
            "wide" => Some(Across::Wide),
            "words" => Some(Across::Words),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Across::Wide => "wide",
            Across::Words => "words",
        }
    }
}

/// The banner settings.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerFile {
    pub across: Across,
    /// The height of the band, in points.
    pub width: f32,
    /// 0.0 to 1.0.
    pub opacity: f32,
    /// The colour behind the words, as the file writes it.
    pub ground: String,
    /// How long a banner stays up, in seconds.
    pub seconds: f32,
    /// Forget a line that has been learning this many days, 0 for never.
    pub forget_days: u32,
    /// Stay quiet for this many seconds after a line.
    pub quiet_secs: u32,
}

impl Default for BannerFile {
    fn default() -> Self {
        BannerFile {
            across: Across::Wide,
            width: 24.0,
            opacity: 0.85,
            ground: "#000000".to_string(),
            seconds: 5.0,
            forget_days: 0,
            quiet_secs: 0,
        }
    }
}

/// One row of the Banners settings page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    /// The setting's key.
    pub key: String,
    /// The words the page shows for it.
    pub label: String,
    /// The value, as text.
    pub value: String,
    /// The note the page shows under it.
    pub note: String,
}

impl BannerFile {
    /// **ESTABLISHED** (`0x1014aaed4` is `look`; the read is beside it).
    pub fn read(doc: &Value) -> BannerFile {
        let table = doc.get("banner").unwrap_or(&Value::Null);
        let mut file = BannerFile::default();
        if let Some(across) = table.get("across").and_then(Value::as_str) {
            file.across = Across::parse(across).unwrap_or(Across::Wide);
        }
        for (key, field) in [
            ("width", &mut file.width),
            ("opacity", &mut file.opacity),
            ("seconds", &mut file.seconds),
        ] {
            if let Some(value) = table.f32_of(key) {
                *field = value;
            }
        }
        if let Some(ground) = table.get("ground").and_then(Value::as_str) {
            file.ground = ground.to_string();
        }
        for (key, field) in [
            ("forget_days", &mut file.forget_days),
            ("quiet_secs", &mut file.quiet_secs),
        ] {
            if let Some(value) = table.u64_of(key) {
                *field = value as u32;
            }
        }
        file
    }

    /// The same, from the file's text.
    pub fn parse(text: &str) -> Result<BannerFile, String> {
        Ok(BannerFile::read(&text::parse_toml(text)?))
    }

    /// Write the settings back out.
    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("across".to_string(), Value::Str(self.across.as_str().to_string()));
        map.insert("width".to_string(), Value::Float(self.width as f64));
        map.insert("opacity".to_string(), Value::Float(self.opacity as f64));
        map.insert("ground".to_string(), Value::Str(self.ground.clone()));
        map.insert("seconds".to_string(), Value::Float(self.seconds as f64));
        map.insert("forget_days".to_string(), Value::Int(self.forget_days as i64));
        map.insert("quiet_secs".to_string(), Value::Int(self.quiet_secs as i64));
        let mut doc = BTreeMap::new();
        doc.insert("banner".to_string(), Value::Table(map));
        Value::Table(doc)
    }

    /// Are these the settings a folder starts with?
    ///
    /// **ESTABLISHED** (`0x1014ac804`): a plain comparison against the defaults.
    pub fn is_empty(&self) -> bool {
        *self == BannerFile::default()
    }

    /// The colours the banner is drawn in.
    ///
    /// **PARTLY** (`0x1014aaed4`): the binary parses `ground` with
    /// `hydra_ui::theme::parse_hex` and hands the result to the banner; that is a
    /// UI call, so this returns the numbers the hex spells (`r, g, b, a` in
    /// 0..=1) and the seam is left for the GUI.
    pub fn look(&self) -> [f32; 4] {
        parse_hex(&self.ground).unwrap_or([0.0, 0.0, 0.0, self.opacity])
    }

    /// The Banners settings page.
    ///
    /// **ESTABLISHED** (`0x1014ab118`): the six settings with the notes the
    /// binary carries.
    pub fn page(&self) -> Vec<Row> {
        vec![
            Row {
                key: "across".into(),
                label: "Across".into(),
                value: self.across.as_str().into(),
                note: "Stretch the banner across the window, or only as wide as the words.".into(),
            },
            Row {
                key: "width".into(),
                label: "Height".into(),
                value: format!("{:.0}", self.width),
                note: String::new(),
            },
            Row {
                key: "opacity".into(),
                label: "Opacity".into(),
                value: format!("{:.0}%", self.opacity * 100.0),
                note: String::new(),
            },
            Row {
                key: "ground".into(),
                label: "Ground".into(),
                value: self.ground.clone(),
                note: String::new(),
            },
            Row {
                key: "seconds".into(),
                label: "Seconds".into(),
                value: format!("{:.0}", self.seconds),
                note: String::new(),
            },
            Row {
                key: "forget_days".into(),
                label: "Forget after, days".into(),
                value: self.forget_days.to_string(),
                note: "Days: a line not yet learned that has not come in this long is dropped, which keeps the file small. 0, never.".into(),
            },
        ]
    }

    /// Change one setting.
    ///
    /// **ESTABLISHED** (`0x1014abe8c`): `across` takes `wide` or `words`, a
    /// percentage has to be from 10 to 100, and a plain number has to be a
    /// number.
    pub fn change(&mut self, key: &str, value: &str) -> Result<String, String> {
        let value = value.trim();
        match key {
            "across" => match Across::parse(value) {
                Some(across) => {
                    self.across = across;
                    Ok(format!("Banners are shown {}.", across.as_str()))
                }
                None => Err(format!("{value} is not a way to lay the banner out.")),
            },
            "opacity" => {
                let percent = number(value)?;
                if !(PERCENT_LOW..=PERCENT_HIGH).contains(&percent) {
                    return Err(format!(
                        "A percent from {PERCENT_LOW:.0} to {PERCENT_HIGH:.0} was wanted, not {percent:.0}."
                    ));
                }
                self.opacity = percent / 100.0;
                Ok(format!("Banners are shown so from now."))
            }
            "width" => {
                self.width = number(value)?;
                Ok(String::new())
            }
            "seconds" => {
                self.seconds = number(value)?;
                Ok(String::new())
            }
            "ground" => {
                if parse_hex(value).is_none() {
                    return Err(format!("{value} is not a colour."));
                }
                self.ground = value.to_string();
                Ok(String::new())
            }
            "forget_days" => {
                self.forget_days = number(value)?.max(0.0) as u32;
                Ok(String::new())
            }
            other => Err(format!("The Banners page has no {other}.")),
        }
    }
}

/// Read a number, or say what was wrong with it.
///
/// **ESTABLISHED** (`0x1014abe8c`): `{} is a number, not {}.`
pub fn number(text: &str) -> Result<f32, String> {
    text.trim()
        .parse::<f32>()
        .map_err(|_| format!("{text} is a number, not {text}."))
}

/// A `#rrggbb` (or `#rrggbbaa`) colour as `r, g, b, a` in `0.0 ..= 1.0`.
///
/// **ESTABLISHED** (the shape `hydra_ui::theme::parse_hex` takes, called from
/// `0x1014aaed4`).
pub fn parse_hex(text: &str) -> Option<[f32; 4]> {
    let hex = text.trim().trim_start_matches('#');
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    match hex.len() {
        6 => Some([
            byte(0)? as f32 / 255.0,
            byte(2)? as f32 / 255.0,
            byte(4)? as f32 / 255.0,
            1.0,
        ]),
        8 => Some([
            byte(0)? as f32 / 255.0,
            byte(2)? as f32 / 255.0,
            byte(4)? as f32 / 255.0,
            byte(6)? as f32 / 255.0,
        ]),
        3 => {
            let nibble = |at: usize| u8::from_str_radix(hex.get(at..at + 1)?, 16).ok().map(|v| v * 17);
            Some([
                nibble(0)? as f32 / 255.0,
                nibble(1)? as f32 / 255.0,
                nibble(2)? as f32 / 255.0,
                1.0,
            ])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_empty_and_round_trip() {
        let banner = BannerFile::default();
        assert!(banner.is_empty());
        assert_eq!(BannerFile::read(&banner.write()), banner);
        assert!(!BannerFile::parse("[banner]\nwidth = 30.0\n").unwrap().is_empty());
    }

    #[test]
    fn colours_are_parsed_from_hex() {
        assert_eq!(parse_hex("#ffffff"), Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parse_hex("000000"), Some([0.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex("#f00"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_hex("#00000000"), Some([0.0, 0.0, 0.0, 0.0]));
        assert_eq!(parse_hex("nope"), None);
        // The banner's own look is its ground colour with its opacity.
        let banner = BannerFile {
            ground: "#102030".into(),
            opacity: 0.5,
            ..BannerFile::default()
        };
        assert_eq!(banner.look()[3], 1.0);
        let no_colour = BannerFile {
            ground: "nonsense".into(),
            opacity: 0.5,
            ..BannerFile::default()
        };
        assert_eq!(no_colour.look(), [0.0, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn changing_a_setting_explains_itself() {
        let mut banner = BannerFile::default();
        assert_eq!(
            banner.change("across", "words").unwrap(),
            "Banners are shown words."
        );
        assert_eq!(banner.across, Across::Words);
        assert_eq!(
            banner.change("opacity", "50").unwrap(),
            "Banners are shown so from now."
        );
        assert_eq!(banner.opacity, 0.5);
        assert!(banner.change("opacity", "5").is_err());
        assert!(banner.change("nope", "1").is_err());
        assert!(banner.change("ground", "not a colour").is_err());
        assert_eq!(banner.change("ground", "#ffffff").unwrap(), "");
        assert_eq!(banner.ground, "#ffffff");
    }

    #[test]
    fn the_page_lists_the_settings() {
        let rows = BannerFile::default().page();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].key, "across");
        assert_eq!(rows[0].value, "wide");
        assert!(rows[5].note.contains("keeps the file small"));
        assert_eq!(PAGE, "hydra_banners");
    }
}
