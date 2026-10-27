//! The folder's own settings: the `window.toml` the General page edits.
//!
//! **ESTABLISHED**:
//!
//!  * The file is `window.toml` (`0x102418xxx`, `own::Own::load`) and its fields
//!    are `record_sessions`, `player_log`, `theme`, `light_theme`,
//!    `follow_computer`, `web_page`, `web_port`, `zoom`, `volume`,
//!    `fresh_seconds`, `check_updates`, `update_when_idle`, `merchant_banners`,
//!    `merchant_notices`, `story_pictures`, `pictures_folder`, `graphics`,
//!    `banner`, `fresh_after` (`0x101471fbc`).
//!  * `graphics` (`0x10147042c`) is one of `auto`, `vulkan`, `dx12`.
//!  * `records_sessions` honours `HYDRA_RECORD` (`0x101469f64`).
//!  * The settings page's id is `hydra_general` (`0x10146b400`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::banner::BannerFile;
use crate::text::{self, Value};

/// The file the folder's settings live in.
pub const FILE: &str = "window.toml";

/// The id the General page is registered under.
///
/// **ESTABLISHED** (`0x10146b400`): `hydra_general`.
pub const PAGE: &str = "hydra_general";

/// The variable that turns session recording on or off whatever the file says.
///
/// **ESTABLISHED** (`0x101469f64`): `HYDRA_RECORD`.
pub const RECORD: &str = "HYDRA_RECORD";

/// The default port the folder's web page is served on.
///
/// **ESTABLISHED** (`0x101469e2c`): the port is read from the file, and this is
/// what it falls back to.
pub const WEB_PORT: u16 = 8080;

/// Which graphics backend the folder asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Graphics {
    /// Whatever the machine does best.
    #[default]
    Auto,
    Vulkan,
    Dx12,
}

impl Graphics {
    /// **ESTABLISHED** (`0x10147042c`): `auto`, `vulkan` and `dx12`.
    pub fn parse(text: &str) -> Option<Graphics> {
        match text.trim() {
            "auto" => Some(Graphics::Auto),
            "vulkan" => Some(Graphics::Vulkan),
            "dx12" => Some(Graphics::Dx12),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Graphics::Auto => "auto",
            Graphics::Vulkan => "vulkan",
            Graphics::Dx12 => "dx12",
        }
    }
}

/// The graphics word for what the machine can do.
///
/// **ESTABLISHED** (`0x10147042c`): the choice is only made where the build can
/// try both; this build has no GPU seam, so `auto` stays `auto`.
pub fn graphics(chosen: Graphics) -> &'static str {
    chosen.as_str()
}

/// Whether sessions are recorded.
///
/// **ESTABLISHED** (`0x101469f64`): `HYDRA_RECORD=1` or `0` wins over the file.
pub fn records_sessions(file_says: bool, env: Option<&str>) -> bool {
    match env.map(str::trim) {
        Some("1") => true,
        Some("0") => false,
        _ => file_says,
    }
}

/// Whether the player log is kept.
///
/// **ESTABLISHED** (`0x101469ec8`): the file's own `player_log` flag.
pub fn keeps_player_log(file_says: bool) -> bool {
    file_says
}

/// How long the update check waits before it looks again.
///
/// **ESTABLISHED** (`0x10146a0c0`): `fresh_seconds`, defaulting to a day.
pub fn starts_fresh_after(fresh_seconds: u64) -> std::time::Duration {
    std::time::Duration::from_secs(if fresh_seconds == 0 {
        60 * 60 * 24
    } else {
        fresh_seconds
    })
}

/// Whether the update check is on.
///
/// **ESTABLISHED** (`0x101470388`): `check_updates` and `update_when_idle` both
/// have to be on for the check to run on its own.
pub fn updates(check_updates: bool, update_when_idle: bool) -> bool {
    check_updates && update_when_idle
}

/// The folder's web page address.
///
/// **ESTABLISHED** (`0x101470568`): `web_page`, which is a host and path.
pub fn web_page(page: &str, port: u16) -> String {
    let page = page.trim();
    let page = page.trim_end_matches('/');
    if page.is_empty() {
        format!("http://localhost:{port}")
    } else if page.contains("://") {
        page.to_string()
    } else {
        format!("http://{page}:{port}")
    }
}

/// The port the web page is served on.
///
/// **ESTABLISHED** (`0x101469e2c`): `web_port`, or [`WEB_PORT`] when it is not
/// set.
pub fn web_page_port(port: Option<u16>) -> u16 {
    port.unwrap_or(WEB_PORT)
}

/// The folder's settings.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub record_sessions: bool,
    pub player_log: bool,
    pub theme: String,
    pub light_theme: String,
    pub follow_computer: bool,
    pub web_page: String,
    pub web_port: u16,
    pub zoom: f32,
    pub volume: f32,
    pub fresh_seconds: u64,
    pub check_updates: bool,
    pub update_when_idle: bool,
    pub merchant_banners: bool,
    pub merchant_notices: bool,
    pub story_pictures: bool,
    pub pictures_folder: String,
    pub graphics: Graphics,
    pub banner: BannerFile,
    pub fresh_after: String,
}

impl Default for File {
    fn default() -> Self {
        File {
            record_sessions: false,
            player_log: true,
            theme: String::new(),
            light_theme: String::new(),
            follow_computer: false,
            web_page: String::new(),
            web_port: WEB_PORT,
            zoom: 1.0,
            volume: 1.0,
            fresh_seconds: 60 * 60 * 24,
            check_updates: true,
            update_when_idle: true,
            merchant_banners: true,
            merchant_notices: true,
            story_pictures: true,
            pictures_folder: String::new(),
            graphics: Graphics::Auto,
            banner: BannerFile::default(),
            fresh_after: String::new(),
        }
    }
}

impl File {
    /// Read the settings from a document.
    ///
    /// **ESTABLISHED** (`0x10146ad64`): the fields above; a file that will not
    /// read is reported as `format!("{}: it does not read: {}", path, error)`.
    pub fn read(doc: &Value) -> File {
        let table = doc.get("window").unwrap_or(doc);
        let mut file = File::default();
        for (key, field) in [
            ("record_sessions", &mut file.record_sessions),
            ("player_log", &mut file.player_log),
            ("follow_computer", &mut file.follow_computer),
            ("check_updates", &mut file.check_updates),
            ("update_when_idle", &mut file.update_when_idle),
            ("merchant_banners", &mut file.merchant_banners),
            ("merchant_notices", &mut file.merchant_notices),
            ("story_pictures", &mut file.story_pictures),
        ] {
            if table.get(key).is_some() {
                *field = table.bool_of(key);
            }
        }
        for (key, field) in [
            ("theme", &mut file.theme),
            ("light_theme", &mut file.light_theme),
            ("web_page", &mut file.web_page),
            ("pictures_folder", &mut file.pictures_folder),
            ("fresh_after", &mut file.fresh_after),
        ] {
            if let Some(value) = table.get(key).and_then(Value::as_str) {
                *field = value.to_string();
            }
        }
        if let Some(port) = table.u64_of("web_port") {
            file.web_port = port as u16;
        }
        if let Some(zoom) = table.f32_of("zoom") {
            file.zoom = zoom;
        }
        if let Some(volume) = table.f32_of("volume") {
            file.volume = volume;
        }
        if let Some(fresh) = table.u64_of("fresh_seconds") {
            file.fresh_seconds = fresh;
        }
        if let Some(graphics) = table.get("graphics").and_then(Value::as_str) {
            file.graphics = Graphics::parse(graphics).unwrap_or(Graphics::Auto);
        }
        if let Some(banner) = doc.get("banner") {
            file.banner = BannerFile::read(&Value::Table(BTreeMap::from([(
                "banner".to_string(),
                banner.clone(),
            )])));
        }
        file
    }

    /// The same, from the file's text.
    pub fn parse(text: &str) -> Result<File, String> {
        Ok(File::read(&text::parse_toml(text)?))
    }

    /// Write the settings back out.
    pub fn write(&self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("record_sessions".to_string(), Value::Bool(self.record_sessions));
        map.insert("player_log".to_string(), Value::Bool(self.player_log));
        map.insert("theme".to_string(), Value::Str(self.theme.clone()));
        map.insert("light_theme".to_string(), Value::Str(self.light_theme.clone()));
        map.insert("follow_computer".to_string(), Value::Bool(self.follow_computer));
        map.insert("web_page".to_string(), Value::Str(self.web_page.clone()));
        map.insert("web_port".to_string(), Value::Int(self.web_port as i64));
        map.insert("zoom".to_string(), Value::Float(self.zoom as f64));
        map.insert("volume".to_string(), Value::Float(self.volume as f64));
        map.insert("fresh_seconds".to_string(), Value::Int(self.fresh_seconds as i64));
        map.insert("check_updates".to_string(), Value::Bool(self.check_updates));
        map.insert("update_when_idle".to_string(), Value::Bool(self.update_when_idle));
        map.insert("merchant_banners".to_string(), Value::Bool(self.merchant_banners));
        map.insert("merchant_notices".to_string(), Value::Bool(self.merchant_notices));
        map.insert("story_pictures".to_string(), Value::Bool(self.story_pictures));
        map.insert(
            "pictures_folder".to_string(),
            Value::Str(self.pictures_folder.clone()),
        );
        map.insert("graphics".to_string(), Value::Str(self.graphics.as_str().to_string()));
        map.insert("fresh_after".to_string(), Value::Str(self.fresh_after.clone()));
        let mut doc = BTreeMap::new();
        doc.insert("window".to_string(), Value::Table(map));
        if let Value::Table(banner) = self.banner.write() {
            doc.insert("banner".to_string(), Value::Table(banner));
        }
        Value::Table(doc)
    }
}

/// The settings of a folder.
///
/// **ESTABLISHED**: `Own` wraps a `File` (`0x10146dbd0` is `banner`, an 8-byte
/// accessor onto it).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Own {
    pub file: File,
}

impl Own {
    /// The banner settings.
    ///
    /// **ESTABLISHED** (`0x10146dbd0`): a thunk onto [`BannerFile::look`].
    pub fn banner(&self) -> [f32; 4] {
        self.file.banner.look()
    }

    /// Read a folder's settings.
    ///
    /// **ESTABLISHED** (`0x10146ad64`): a file that will not read is reported
    /// with the path and the error.
    pub fn load(folder: &Path) -> Result<Own, String> {
        let path = folder.join(FILE);
        if !path.exists() {
            return Ok(Own::default());
        }
        let doc = text::read_file(&path).map_err(|e| format!("{}: it does not read: {e}", path.display()))?;
        Ok(Own {
            file: File::read(&doc),
        })
    }

    /// Where the pictures live.
    ///
    /// **ESTABLISHED** (`0x10146a964`): the folder's `pictures_folder`, else the
    /// data folder itself.
    pub fn pictures_folder(&self, folder: &Path) -> PathBuf {
        if self.file.pictures_folder.is_empty() {
            folder.to_path_buf()
        } else {
            PathBuf::from(&self.file.pictures_folder)
        }
    }

    /// Turn the player log on or off.
    ///
    /// **ESTABLISHED** (`0x10146a904`): the file is cloned, changed and kept.
    pub fn set_player_log(&mut self, on: bool) -> File {
        self.file.player_log = on;
        self.file.clone()
    }

    /// Turn session recording on or off.
    ///
    /// **ESTABLISHED** (`0x10146aa04`): the file is cloned, changed and kept.
    pub fn set_record_sessions(&mut self, on: bool) -> File {
        self.file.record_sessions = on;
        self.file.clone()
    }

    /// The zoom the story is drawn at.
    ///
    /// **ESTABLISHED** (`0x10146b3b8`): clamped into a range the window accepts.
    pub fn zoom(&self) -> f32 {
        self.file.zoom.clamp(ZOOM_LOW, ZOOM_HIGH)
    }

    /// The volume.
    ///
    /// **ESTABLISHED** (`0x10146fd44`): clamped into `0.0 ..= 1.0`.
    pub fn volume(&self) -> f32 {
        self.file.volume.clamp(0.0, 1.0)
    }

    /// Change one setting.
    ///
    /// **ESTABLISHED** (`0x10146dbd8`): `The zoom is a number fr…` and `{} is on
    /// or off, not {}.`
    pub fn change(&mut self, key: &str, value: &str) -> Result<String, String> {
        let value = value.trim();
        match key {
            "zoom" => {
                let zoom: f32 = value.parse().map_err(|_| {
                    format!("The zoom is a number from {ZOOM_LOW:.0} to {ZOOM_HIGH:.0}, not {value}.")
                })?;
                if !(ZOOM_LOW..=ZOOM_HIGH).contains(&zoom) {
                    return Err(format!(
                        "The zoom is a number from {ZOOM_LOW:.0} to {ZOOM_HIGH:.0}, not {value}."
                    ));
                }
                self.file.zoom = zoom;
                Ok(String::new())
            }
            "volume" => {
                let volume: f32 = value.parse().map_err(|_| {
                    format!("The volume is a number from 0 to 100, not {value}.")
                })?;
                self.file.volume = (volume / 100.0).clamp(0.0, 1.0);
                Ok(String::new())
            }
            "player_log" => {
                self.file.player_log = on_off(value)?;
                Ok(String::new())
            }
            "record_sessions" => {
                self.file.record_sessions = on_off(value)?;
                Ok(String::new())
            }
            "merchant_notices" => {
                self.file.merchant_notices = on_off(value)?;
                Ok(String::new())
            }
            "check_updates" => {
                self.file.check_updates = on_off(value)?;
                Ok(String::new())
            }
            "web_page" => {
                self.file.web_page = value.to_string();
                Ok(String::new())
            }
            "web_port" => {
                self.file.web_port = value
                    .parse()
                    .map_err(|_| format!("A port is a number, not {value}."))?;
                Ok(String::new())
            }
            "graphics" => match Graphics::parse(value) {
                Some(graphics) => {
                    self.file.graphics = graphics;
                    Ok(String::new())
                }
                None => Err(format!("{value} is not a way to draw.")),
            },
            other => Err(format!("The General page has no {other}.")),
        }
    }

    /// The General page's rows.
    ///
    /// **ESTABLISHED** (`0x10146b400`): the settings, with the notes the binary
    /// carries for the player log and session recording.
    pub fn pages(&self) -> Vec<crate::banner::Row> {
        use crate::banner::Row;
        let mut rows = Vec::new();
        let mut row = |key: &str, label: &str, value: String, note: &str| {
            rows.push(Row {
                key: key.to_string(),
                label: label.to_string(),
                value,
                note: note.to_string(),
            });
        };
        row(
            "player_log",
            "Player log",
            on_off_word(self.file.player_log),
            "Keep what each character reads, a file a day.",
        );
        row(
            "record_sessions",
            "Record sessions",
            on_off_word(self.file.record_sessions),
            "Write each session's raw game stream to a .bytes file in the logs folder, for hydra import to read back. On in a development build, off in a release; HYDRA_RECORD=1 or 0 overrides.",
        );
        row("web_page", "Web page", self.file.web_page.clone(), "");
        row(
            "web_port",
            "Web port",
            self.file.web_port.to_string(),
            "",
        );
        row("zoom", "Zoom", format!("{:.2}", self.file.zoom), "");
        row(
            "volume",
            "Volume",
            format!("{:.0}", self.file.volume * 100.0),
            "",
        );
        row(
            "graphics",
            "Graphics",
            self.file.graphics.as_str().to_string(),
            "Vulkan where a card has it, DirectX 12 otherwise.",
        );
        rows
    }

    /// The same, from the folder's file.
    pub fn parse(text: &str) -> Result<Own, String> {
        Ok(Own {
            file: File::parse(text)?,
        })
    }
}

/// The zoom the window lets a folder ask for.
pub const ZOOM_LOW: f32 = 0.5;
pub const ZOOM_HIGH: f32 = 3.0;

fn on_off(value: &str) -> Result<bool, String> {
    match value.trim() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        other => Err(format!("{other} is on or off, not {other}.")),
    }
}

fn on_off_word(on: bool) -> String {
    if on { "on".into() } else { "off".into() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_settings_round_trip() {
        let text = "\
[window]
record_sessions = true
player_log = false
theme = \"dark\"
web_page = \"hydra.local\"
web_port = 9000
zoom = 1.25
volume = 0.5
graphics = \"vulkan\"
";
        let file = File::parse(text).unwrap();
        assert!(file.record_sessions);
        assert!(!file.player_log);
        assert_eq!(file.theme, "dark");
        assert_eq!(file.web_page, "hydra.local");
        assert_eq!(file.web_port, 9000);
        assert_eq!(file.zoom, 1.25);
        assert_eq!(file.volume, 0.5);
        assert_eq!(file.graphics, Graphics::Vulkan);
        assert_eq!(File::read(&file.write()), file);
    }

    #[test]
    fn recording_follows_the_environment() {
        assert!(records_sessions(false, Some("1")));
        assert!(!records_sessions(true, Some("0")));
        assert!(records_sessions(true, None));
        assert!(!records_sessions(false, Some("nonsense")));
        assert!(updates(true, true));
        assert!(!updates(true, false));
        assert_eq!(
            starts_fresh_after(0),
            std::time::Duration::from_secs(60 * 60 * 24)
        );
        assert_eq!(starts_fresh_after(60), std::time::Duration::from_secs(60));
    }

    #[test]
    fn graphics_words_are_the_three() {
        assert_eq!(Graphics::parse("auto"), Some(Graphics::Auto));
        assert_eq!(Graphics::parse("vulkan"), Some(Graphics::Vulkan));
        assert_eq!(Graphics::parse("dx12"), Some(Graphics::Dx12));
        assert_eq!(Graphics::parse("metal"), None);
        assert_eq!(graphics(Graphics::Dx12), "dx12");
    }

    #[test]
    fn the_web_page_and_port() {
        assert_eq!(web_page("hydra.local", 8080), "http://hydra.local:8080");
        assert_eq!(web_page("", 8080), "http://localhost:8080");
        assert_eq!(
            web_page("http://hydra.local", 8080),
            "http://hydra.local"
        );
        assert_eq!(web_page_port(None), WEB_PORT);
        assert_eq!(web_page_port(Some(1234)), 1234);
    }

    #[test]
    fn changing_settings_explains_itself() {
        let mut own = Own::default();
        assert_eq!(own.change("zoom", "1.5").unwrap(), "");
        assert_eq!(own.zoom(), 1.5);
        assert!(own.change("zoom", "9").is_err());
        assert_eq!(own.change("volume", "50").unwrap(), "");
        assert_eq!(own.volume(), 0.5);
        assert!(own.change("player_log", "maybe").is_err());
        assert_eq!(own.change("player_log", "off").unwrap(), "");
        assert!(!own.file.player_log);
        assert_eq!(own.set_player_log(true), own.file);
        assert!(own.file.player_log);
        assert_eq!(own.pictures_folder(Path::new("/data")), PathBuf::from("/data"));
        assert_eq!(own.banner()[3], 1.0);
    }

    #[test]
    fn the_page_lists_the_recovered_notes() {
        let rows = Own::default().pages();
        assert!(rows.iter().any(|row| row.key == "player_log"));
        let record = rows.iter().find(|row| row.key == "record_sessions").unwrap();
        assert!(record.note.contains("HYDRA_RECORD=1 or 0 overrides"));
        assert_eq!(PAGE, "hydra_general");
        assert_eq!(RECORD, "HYDRA_RECORD");
    }
}
