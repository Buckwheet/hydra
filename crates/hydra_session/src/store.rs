//! Path building and durable writes for the per-character files Hydra keeps.
//!
//! Recovered from `store::{safe_component, character_path, save_bytes}` at
//! `0x101aa1d9c`, `0x101aa1bbc`, `0x101aa16dc`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Reduce `s` to something that is safe as one path component.
///
/// **ESTABLISHED** (`0x101aa1d9c`): the body is one
/// `chars().filter(...).map(...).collect::<String>()` — the shared
/// `Map::fold` monomorphisation at `0x101a9f48c` was read instruction by
/// instruction:
///
/// * every character that is **not** an ASCII digit and **not** an ASCII
///   letter is dropped (the `sub #0x30` / `sub #0x41` range checks; both the
///   one-byte and the multi-byte UTF-8 paths end in the same check, so every
///   non-ASCII code point is dropped too);
/// * ASCII `A`-`Z` is folded to lower case (`orr #0x20` selected with
///   `cmp w9, #0x1a` / `csel`).
///
/// So `"Aelith's Cart"` becomes `"aelithscart"`, and a component with no
/// alphanumerics at all becomes the empty string — which is what makes
/// [`character_path`] return `None`.
pub fn safe_component(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// `root/<safe(first)>_<safe(second)><ext>`.
///
/// **ESTABLISHED** (`0x101aa1bbc`): two `safe_component` folds, then the
/// format string at `0x103e7c9f5` — one argument, the piece `_`, a second
/// argument, then a third argument that formats a plain `&str` (it is stored
/// as a 16-byte `&str` and displayed through `<&str as Display>`) — i.e.
/// `format!("{}_{}{}", safe_a, safe_b, ext)`, and finally `Path::join` on the
/// root.  The result is stored through the sret pointer as an
/// `Option<PathBuf>`: when *either* sanitised component is empty the function
/// jumps to `0x101aa1cdc`, which writes the "empty" niche word
/// (`0x8000000000000000`) and nothing else — `None`.
pub fn character_path(root: &Path, first: &str, second: &str, ext: &str) -> Option<PathBuf> {
    let a = safe_component(first);
    let b = safe_component(second);
    if a.is_empty() || b.is_empty() {
        return None;
    }
    Some(root.join(format!("{a}_{b}{ext}")))
}

/// The temporary extension a [`save_bytes`] write uses:
/// `format!("{}.{}-{}.tmp", ext, pid, hash)`.
///
/// **ESTABLISHED** (`0x101aa16dc`): `Path::extension`,
/// `String::from_utf8_lossy`, `process::id()`, then a hand-inlined
/// `DefaultHasher` seeded from `thread::current().id()`
/// (`0x101aa1808`..`0x101aa1928` is the SipHash round function the standard
/// library emits for `Hash for ThreadId`), and the format string at
/// `0x103e7c9e8` (`{}` `.` `{}` `-` `{}` `.tmp`).  The final name is produced
/// with `Path::with_extension`, so `notes.json` gets
/// `notes.json.<pid>-<hash>.tmp`.
pub fn temp_extension(path: &Path, pid: u32, hash: u64) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{ext}.{pid}-{hash:x}.tmp")
}

/// A stable stand-in for the standard library's thread-id hash.
///
/// **INFERRED:** the original hashes `thread::current().id()` with the default
/// hasher so two threads writing the same file never collide.  We hash the
/// same thing with `DefaultHasher`, which is the same construction.
pub fn thread_hash() -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::thread::current().id().hash(&mut h);
    h.finish()
}

/// Write `bytes` to `path` atomically: create `dir`, write a sibling `.tmp`
/// file, `sync_data`, then rename it over `path`.
///
/// **ESTABLISHED** (`0x101aa16dc`):
///
/// 1. `DirBuilder::recursive(true)` (`strh #0x1ff` = mode `0o777`, byte `+0x2`
///    = recursive) on `dir`;
/// 2. build the temporary name (see [`temp_extension`]) and
///    `path.with_extension(...)`;
/// 3. `OpenOptions` with mode `0x1b6` = `0o666`, create+truncate;
/// 4. `write_all`, `sync_data`;
/// 5. `std::sys::fs::rename(tmp, path)`; if the rename fails the temporary
///    file is removed (`remove_file`) and the error returned;
/// 6. after a successful rename the *directory* is opened again and
///    `sync_data`-ed, so the rename itself is durable.
///
/// Any `io::Error` short-circuits the whole function.
pub fn save_bytes(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = path.with_extension(temp_extension(path, std::process::id(), thread_hash()));
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_data()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // Re-open the directory and sync it so the rename survives a crash.
    fs::File::open(dir)?.sync_data()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_component_drops_punctuation_and_folds_case() {
        assert_eq!(safe_component("Aelith"), "aelith");
        assert_eq!(safe_component("Aelith's Cart"), "aelithscart");
        assert_eq!(safe_component("../etc/passwd"), "etcpasswd");
        assert_eq!(safe_component(".."), "");
        assert_eq!(safe_component(""), "");
        assert_eq!(safe_component("Ünïcode"), "ncode");
        assert_eq!(safe_component("a b\tc\nd"), "abcd");
        assert_eq!(safe_component("Z9"), "z9");
    }

    #[test]
    fn character_path_joins_the_two_components_and_the_extension() {
        let root = Path::new("/data/characters");
        let p = character_path(root, "GemStone IV", "Aelith", ".json").unwrap();
        assert_eq!(p, PathBuf::from("/data/characters/gemstoneiv_aelith.json"));
    }

    #[test]
    fn character_path_refuses_an_empty_component() {
        let root = Path::new("/data");
        assert_eq!(character_path(root, "..", "Aelith", ".json"), None);
        assert_eq!(character_path(root, "Aelith", "/", ".json"), None);
    }

    #[test]
    fn temp_extension_keeps_the_original_extension_and_adds_pid_and_hash() {
        let p = Path::new("/data/notes.json");
        assert_eq!(temp_extension(p, 42, 0xdead), "json.42-dead.tmp");
        let q = Path::new("/data/MANIFEST");
        assert_eq!(temp_extension(q, 7, 0xff), ".7-ff.tmp");
    }

    #[test]
    fn save_bytes_writes_then_renames() {
        let dir = std::env::temp_dir().join(format!("hydra-store-{}", std::process::id()));
        let path = dir.join("text.txt");
        save_bytes(&dir, &path, b"first").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "first");
        save_bytes(&dir, &path, b"second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        // No temporary leftovers.
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(left.is_empty(), "leftover temporaries: {left:?}");
        let _ = fs::remove_dir_all(&dir);
    }
}
