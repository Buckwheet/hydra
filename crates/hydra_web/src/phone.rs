//! The phone's own little storage area: `phone/<kind>.json`.
//!
//! Recovered from `phone::file` (`0x10029f7cc`), `phone::keep` (`0x10029f94c`)
//! and `phone::kept` (`0x10029fd2c`).

use std::path::{Path, PathBuf};

/// The directory under the storage root (`root.join("phone")`, 5 bytes at
/// `0x102071b88`).
pub const DIR: &str = "phone";

/// The three kinds of phone state, in the order the binary compared them.
pub const KINDS: [&str; 3] = ["ui", "wheel", "buttons"];

/// The file for one kind of phone state (`0x10029f7cc`).
///
/// Recovered: the kind is matched against `"ui"` (2 bytes), `"wheel"` (5) and
/// `"buttons"` (7); anything else is `None` (the `PathBuf` niche).  The path is
/// `dir.join("phone").join(format!("{kind}.json"))`.
pub fn file(root: &Path, kind: &str) -> Option<PathBuf> {
    if !KINDS.contains(&kind) {
        return None;
    }
    Some(root.join(DIR).join(format!("{kind}.json")))
}

/// Store one kind of phone state.
///
/// Recovered (`0x10029f94c`): the text is validated as JSON
/// (`serde_json::from_str::<Value>`), the parent directory is created
/// (`DirBuilder::create`) and the bytes are written with
/// `hydra_session::store::save_bytes`.  A malformed document is refused before
/// anything is written.
pub fn keep(root: &Path, kind: &str, text: &str) -> Result<(), String> {
    let path = file(root, kind).ok_or_else(|| format!("no phone state called `{kind}`"))?;
    crate::json::parse(text).map_err(|e| format!("{kind}.json is not JSON: {e}"))?;
    let dir = path.parent().unwrap_or(root);
    hydra_session::store::save_bytes(dir, &path, text.as_bytes())
        .map_err(|e| format!("{} cannot be written: {e}", path.display()))
}

/// Read back one kind of phone state (`0x10029fd2c`).
///
/// Recovered: `phone::file`, `read_to_string`, `metadata`, `modified` and
/// `duration_since(UNIX_EPOCH)`; the pair returned is the document text and the
/// whole seconds since the epoch.  Any missing piece is `None`.
pub fn kept(root: &Path, kind: &str) -> Option<(String, u64)> {
    let path = file(root, kind)?;
    let text = std::fs::read_to_string(&path).ok()?;
    let meta = std::fs::metadata(&path).ok()?;
    let when = meta.modified().ok()?;
    let secs = when.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some((text, secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra-web-phone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_the_three_recovered_kinds_have_files() {
        let root = Path::new("/store");
        assert_eq!(
            file(root, "ui"),
            Some(PathBuf::from("/store/phone/ui.json"))
        );
        assert_eq!(
            file(root, "wheel"),
            Some(PathBuf::from("/store/phone/wheel.json"))
        );
        assert_eq!(
            file(root, "buttons"),
            Some(PathBuf::from("/store/phone/buttons.json"))
        );
        assert_eq!(file(root, "notes"), None);
        assert_eq!(file(root, ""), None);
    }

    #[test]
    fn keep_validates_then_stores_and_kept_reads_back() {
        let root = scratch();
        assert!(kept(&root, "ui").is_none());

        assert!(keep(&root, "ui", "not json").is_err());
        assert!(kept(&root, "ui").is_none(), "nothing was written");

        keep(&root, "ui", r#"{"tab":"inventory"}"#).unwrap();
        let (text, secs) = kept(&root, "ui").unwrap();
        assert_eq!(text, r#"{"tab":"inventory"}"#);
        assert!(secs > 1_600_000_000);
        assert!(keep(&root, "nope", "{}").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
