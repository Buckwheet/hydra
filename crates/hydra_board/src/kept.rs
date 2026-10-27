//! Where a window's kept lines live on disk.
//!
//! **ESTABLISHED**:
//!
//!  * [`file`] (`0x101499080`) joins a folder with `format!("{}.json", name)`,
//!    or `format!("{}_{}.json", kind, name)` when a kind is given — the format
//!    metadata at `0x103e6a0e84`/`0x103e6a0e8c` holds the two pieces and
//!    `.json`.
//!  * [`unread`] (`0x101499290`) appends `.unread`, built byte by byte at
//!    `0x101499324` (`".unr"` then `"ead"`).

use std::path::{Path, PathBuf};

/// The suffix a "there were new lines" marker carries.
///
/// **ESTABLISHED** (`0x101499324`): `.unread`.
pub const UNREAD: &str = ".unread";

/// The extension kept files are written with.
///
/// **ESTABLISHED** (`0x103e6a0e88`): `.json`.
pub const EXTENSION: &str = ".json";

/// The file a window's kept lines live in.
///
/// **ESTABLISHED** (`0x101499080`): `folder/{name}.json`, or
/// `folder/{kind}_{name}.json` when `kind` is not empty.
pub fn file(folder: &Path, kind: &str, name: &str) -> PathBuf {
    if kind.is_empty() {
        folder.join(format!("{name}{EXTENSION}"))
    } else {
        folder.join(format!("{kind}_{name}{EXTENSION}"))
    }
}

/// The name of the marker that says a kept file has new lines.
///
/// **ESTABLISHED** (`0x101499290`): `name` with `.unread` on the end.
pub fn unread(name: &str) -> String {
    format!("{name}{UNREAD}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_file_is_named_after_the_window() {
        assert_eq!(
            file(Path::new("/data"), "", "story"),
            PathBuf::from("/data/story.json")
        );
        assert_eq!(
            file(Path::new("/data"), "logs", "story"),
            PathBuf::from("/data/logs_story.json")
        );
    }

    #[test]
    fn the_unread_marker_is_the_name_plus_suffix() {
        assert_eq!(unread("story.json"), "story.json.unread");
        assert_eq!(UNREAD, ".unread");
        assert_eq!(EXTENSION, ".json");
    }
}
