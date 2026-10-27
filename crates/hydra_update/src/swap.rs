//! Putting a downloaded release in place, safely enough that a crash halfway
//! through leaves one of the two binaries runnable.
//!
//! * [`unpack`] @0x1010a649c pulls one member out of the release archive with
//!   `zip::ZipArchive::by_name` + `read_to_end` (inlined;
//!   `zip_archive::ZipArchive::new` is called at 0x1010ad178);
//! * [`put_in_place`] @0x1010a5db8 writes the bytes to a sibling `*.new` file,
//!   `sync_all`s it, copies the mode of the file it replaces, and renames it
//!   over the top — and if that rename fails, moves the old file aside as
//!   `*.old` first, retries, and puts the old one back if the retry fails too;
//! * [`clear_old`] @0x1010a68d8 deletes a leftover `*.old` (called from
//!   `hydra::gui::run` @0x100111958 as Hydra starts);
//! * [`beside`] @0x1010a62f0 builds those sibling names.
//!
//! Every message below is the binary's own format string, recovered from
//! `__cstring` 0x103e5ebde–0x103e5eea7 and 0x103e23bbe.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::zip;

/// The sibling path used for the half-written file and the moved-aside one:
/// `beside("/Applications/Hydra.app", "new")` is `Hydra.new.app`, and
/// `beside("/usr/local/bin/hydra", "old")` is `hydra.old`.
///
/// `beside` @0x1010a62f0: `Path::file_stem` (0x1010a6320), then
/// `format!("{}.{}.{}", stem, tag, ext)` when `Path::extension` has something
/// (0x1010a6378) and `format!("{}.{}", stem, tag)` when it has not
/// (0x1010a63d0), then `Path::with_file_name` (0x1010a63f8).
pub(crate) fn beside(place: &str, tag: &str) -> PathBuf {
    let path = Path::new(place);
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(extension) => format!("{stem}.{tag}.{}", extension.to_string_lossy()),
        None => format!("{stem}.{tag}"),
    };
    path.with_file_name(name)
}

/// Take one member out of the release archive.
///
/// `unpack` @0x1010a649c: `ZipArchive::new(Cursor::new(bytes))` →
/// `"The release file cannot be opened: {}"`, then `by_name(name)` →
/// `"The release file has no {}: {}"` (with `zip`'s own `file not found` as the
/// payload), then `read_to_end` into a buffer sized from the member's
/// uncompressed size → `"{} cannot be read out of the release file: {}"`.
pub fn unpack(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let archive = zip::Archive::new(bytes)
        .map_err(|e| format!("The release file cannot be opened: {e}"))?;
    let entry = archive
        .by_name(name)
        // `// UNKNOWN:` zip's `ZipError::FileNotFound` Display text; the binary
        // formats whatever `zip` produced here.
        .ok_or_else(|| format!("The release file has no {name}: file not found"))?;
    archive
        .read(entry)
        .map_err(|e| format!("{name} cannot be read out of the release file: {e}"))
}

/// Write `bytes` over `place`, keeping a copy of whatever was there.
///
/// `put_in_place` @0x1010a5db8, step by step:
///
/// 1. `new = beside(place, "new")`; create it with `OpenOptions::new()`
///    `write(true).create(true).truncate(true)` and mode `0o666` (the
///    `OpenOptions` at 0x1010a5e00: `mov w8, #0x1b6`, then the three one-byte
///    flags) — failure is `"{} cannot be written: {}"`;
/// 2. `write_all` then `sync_all` — same message;
/// 3. `fs::metadata(place)`; if it exists, copy its permissions onto `new` with
///    `fs::set_permissions` — failure is `"{} cannot be made runnable: {}"`.  A
///    missing `place` is *not* an error (0x1010a5f30 drops the error and goes
///    straight to the rename);
/// 4. `fs::rename(new, place)`; success is the end;
/// 5. otherwise `old = beside(place, "old")`, `fs::remove_file(old)` (only a
///    `NotFound` failure is tolerated — `"{} cannot be taken away: {}"`
///    otherwise), `fs::rename(place, old)` — `"{} cannot be moved aside: {}"`,
///    then `fs::rename(new, place)` again — `"{} cannot be put in place: {}"`;
/// 6. if that second rename also fails, put the original back with
///    `fs::rename(old, place)`; if *that* fails, say so and stop:
///    `"{} cannot be put in place ({}), and {} could not be put back: rename it
///    to {} by hand before Hydra's next start"`.
pub fn put_in_place(place: &str, bytes: &[u8]) -> Result<(), String> {
    let new = beside(place, "new");
    write_new(&new, bytes)?;

    // Copy the mode of the file being replaced, so the new one stays runnable.
    if let Ok(metadata) = fs::metadata(place) {
        if let Err(error) = fs::set_permissions(&new, metadata.permissions()) {
            return Err(format!(
                "{} cannot be made runnable: {}",
                new.display(),
                error
            ));
        }
    }

    match fs::rename(&new, place) {
        Ok(()) => Ok(()),
        // The first failure is only interesting when the retry below fails too,
        // and that path reports its own error.
        Err(_) => {
            let old = beside(place, "old");
            match fs::remove_file(&old) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "{} cannot be taken away: {}",
                        old.display(),
                        error
                    ));
                }
            }
            if let Err(error) = fs::rename(place, &old) {
                return Err(format!(
                    "{} cannot be moved aside: {}",
                    place,
                    error
                ));
            }
            if let Err(error) = fs::rename(&new, place) {
                // Try to put the original back, then — the one place the binary
                // tidies up after itself — remove the half-written `*.new`
                // (0x1010a61f4) before reporting.
                let put_back = fs::rename(&old, place);
                let _ = fs::remove_file(&new);
                return match put_back {
                    Ok(()) => Err(format!("{} cannot be put in place: {}", place, error)),
                    Err(_) => Err(format!(
                        "{} cannot be put in place ({}), and {} could not be put back: \
                         rename it to {} by hand before Hydra's next start",
                        place,
                        error,
                        old.display(),
                        place
                    )),
                };
            }
            Ok(())
        }
    }
}

fn write_new(new: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o666);
    }
    let mut file = match options.open(new) {
        Ok(file) => file,
        Err(error) => {
            return Err(format!("{} cannot be written: {}", new.display(), error))
        }
    };
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    // The file handle is closed here, as `File::drop` does in the binary.
    drop(file);
    match written {
        Ok(()) => Ok(()),
        Err(error) => Err(format!("{} cannot be written: {}", new.display(), error)),
    }
}

/// Delete a `*.old` left behind by an interrupted swap.  Errors are ignored:
/// the binary drops the `io::Result` (`clear_old` @0x1010a68d8 tail-calls
/// `drop_in_place<io::Error>`).
pub fn clear_old(place: &str) {
    let _ = fs::remove_file(beside(place, "old"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNT: AtomicUsize = AtomicUsize::new(0);

    /// A private directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Scratch {
            let n = COUNT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "hydra-update-swap-{}-{}",
                std::process::id(),
                n
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("scratch dir");
            Scratch(path)
        }

        fn file(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.file(name);
            fs::write(&path, bytes).expect("write");
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The archive used by `unpack`'s tests: `stored.txt` and a deflated
    /// `hydra`, built with Python's `zipfile` (see `zip::tests`).
    const ARCHIVE: &[u8] = &[
        0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe2, 0x38, 0x5b, 0x5d, 0x50, 0x20,
        0xcd, 0x46, 0x0f, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x73, 0x74,
        0x6f, 0x72, 0x65, 0x64, 0x2e, 0x74, 0x78, 0x74, 0x70, 0x6c, 0x61, 0x69, 0x6e, 0x20, 0x63, 0x6f,
        0x6e, 0x74, 0x65, 0x6e, 0x74, 0x73, 0x0a, 0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x08,
        0x00, 0xe2, 0x38, 0x5b, 0x5d, 0xe4, 0x3d, 0x10, 0x75, 0x17, 0x00, 0x00, 0x00, 0x15, 0x00, 0x00,
        0x00, 0x05, 0x00, 0x00, 0x00, 0x68, 0x79, 0x64, 0x72, 0x61, 0x2b, 0xc9, 0x48, 0x55, 0x28, 0x2c,
        0xcd, 0x4c, 0xce, 0x56, 0x48, 0x2a, 0xca, 0x2f, 0xcf, 0x53, 0x48, 0xcb, 0xaf, 0xd0, 0xe3, 0x02,
        0x00, 0x50, 0x4b, 0x01, 0x02, 0x14, 0x03, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe2, 0x38, 0x5b,
        0x5d, 0x50, 0x20, 0xcd, 0x46, 0x0f, 0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x73,
        0x74, 0x6f, 0x72, 0x65, 0x64, 0x2e, 0x74, 0x78, 0x74, 0x50, 0x4b, 0x01, 0x02, 0x14, 0x03, 0x14,
        0x00, 0x00, 0x00, 0x08, 0x00, 0xe2, 0x38, 0x5b, 0x5d, 0xe4, 0x3d, 0x10, 0x75, 0x17, 0x00, 0x00,
        0x00, 0x15, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x80, 0x01, 0x37, 0x00, 0x00, 0x00, 0x68, 0x79, 0x64, 0x72, 0x61, 0x50, 0x4b, 0x05, 0x06,
        0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0x00, 0x6b, 0x00, 0x00, 0x00, 0x71, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    #[test]
    fn names_the_siblings_beside_the_file() {
        assert_eq!(beside("/usr/local/bin/hydra", "new"), Path::new("/usr/local/bin/hydra.new"));
        assert_eq!(beside("/usr/local/bin/hydra", "old"), Path::new("/usr/local/bin/hydra.old"));
        // An extension is kept last: `Hydra.app` → `Hydra.new.app`.
        assert_eq!(
            beside("/Applications/Hydra.app", "new"),
            Path::new("/Applications/Hydra.new.app")
        );
        assert_eq!(
            beside("/Applications/Hydra.app", "old"),
            Path::new("/Applications/Hydra.old.app")
        );
        // A dotted *directory* is not an extension: the name is what counts.
        assert_eq!(
            beside("/Users/x/Hydra.app/Contents/MacOS/hydra", "new"),
            Path::new("/Users/x/Hydra.app/Contents/MacOS/hydra.new")
        );
        // Relative paths stay relative.
        assert_eq!(beside("hydra", "new"), Path::new("hydra.new"));
        assert_eq!(beside("./hydra", "new"), Path::new("./hydra.new"));
    }

    #[test]
    fn unpacks_the_member_it_is_named() {
        assert_eq!(unpack(ARCHIVE, "stored.txt").unwrap(), b"plain contents\n");
        assert_eq!(unpack(ARCHIVE, "hydra").unwrap(), b"the quick brown fox.\n");
    }

    #[test]
    fn unpack_reports_what_went_wrong() {
        assert_eq!(
            unpack(ARCHIVE, "SHA256SUMS.txt").unwrap_err(),
            "The release file has no SHA256SUMS.txt: file not found"
        );
        let error = unpack(b"not a zip", "hydra").unwrap_err();
        assert_eq!(error, "The release file cannot be opened: Could not find EOCD");
    }

    #[test]
    fn writes_a_new_file_and_keeps_the_mode() {
        let scratch = Scratch::new();
        let place = scratch.write("hydra", b"old binary\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&place, fs::Permissions::from_mode(0o755)).unwrap();
        }
        put_in_place(place.to_str().unwrap(), b"new binary\n").expect("swaps");
        assert_eq!(fs::read(&place).unwrap(), b"new binary\n");
        // The `*.new` file is gone, and nothing was moved aside.
        assert!(!scratch.file("hydra.new").exists());
        assert!(!scratch.file("hydra.old").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&place).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755, "the executable bit is kept");
        }
    }

    #[test]
    fn writes_a_file_that_was_not_there() {
        let scratch = Scratch::new();
        let place = scratch.file("hydra");
        put_in_place(place.to_str().unwrap(), b"first binary\n").expect("writes");
        assert_eq!(fs::read(&place).unwrap(), b"first binary\n");
    }

    #[test]
    fn moves_the_old_file_aside_when_the_rename_fails() {
        // Renaming a file over a non-empty *directory* fails, so the swap takes
        // the long way round: the directory is moved to `*.old` and the new
        // file goes where it was.
        let scratch = Scratch::new();
        let place = scratch.file("Hydra.app");
        fs::create_dir(&place).unwrap();
        fs::write(place.join("inside"), b"the old bundle").unwrap();

        put_in_place(place.to_str().unwrap(), b"bundle bytes\n").expect("swaps");
        assert_eq!(fs::read(&place).unwrap(), b"bundle bytes\n");
        let old = scratch.file("Hydra.old.app");
        assert_eq!(fs::read(old.join("inside")).unwrap(), b"the old bundle");
        assert!(!scratch.file("Hydra.new.app").exists());
    }

    #[test]
    fn refuses_a_directory_that_cannot_be_written() {
        let scratch = Scratch::new();
        // A `place` inside a directory that does not exist: `*.new` cannot be
        // created, so the first message is the one reported.
        let place = scratch.file("missing").join("hydra");
        let error = put_in_place(place.to_str().unwrap(), b"bytes").unwrap_err();
        assert!(
            error.contains("cannot be written: "),
            "{error}"
        );
        assert!(error.contains("hydra.new"), "{error}");
    }

    #[test]
    fn clears_a_leftover_old_file() {
        let scratch = Scratch::new();
        let place = scratch.write("hydra", b"current\n");
        scratch.write("hydra.old", b"stale\n");
        clear_old(place.to_str().unwrap());
        assert!(!scratch.file("hydra.old").exists());
        assert!(place.exists(), "the live file is untouched");
        // Clearing again is fine: the error is dropped.
        clear_old(place.to_str().unwrap());
    }
}
