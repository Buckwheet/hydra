//! Putting a downloaded release in place, safely enough that a crash halfway
//! through leaves one of the two binaries runnable.
//!
//! * [`unpack`] @0x1010a649c pulls one member out of the release archive with
//!   `zip::ZipArchive::new` (0x1010ad178) + `by_name` + `read_to_end`
//!   (inlined; `ZipFileData::find_content` / `make_reader` at 0x1010ac7e4 /
//!   0x1010ac3b8);
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
//!
//! **Phase 1 reconnection.**  [`unpack`] calls the real `zip` 9.0.0 crate
//! again, so the recovered wrapper messages now carry `zip`'s own error texts
//! (`"invalid Zip archive: Could not find EOCD"`,
//! `"specified file not found in archive"`, `"Invalid checksum"`) — the same
//! strings the binary carried in `__const`.  `src/zip.rs` and `src/inflate.rs`
//! are gone; what their unit tests asserted is now asserted through this
//! function and through `zip` itself (see the tests below and `NOTES.md` §4.5).

use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

/// The most bytes [`unpack`] will pre-allocate for a member.
///
/// `// UNKNOWN:` the binary sized the buffer from the archive's declared
/// uncompressed size (0x1010a674c) and did not guard it.  A corrupt or hostile
/// central directory can declare any `u64`, and `Vec::with_capacity` would
/// abort on it, so the *hint* is capped here: `read_to_end` still returns the
/// whole member, just without one huge up-front allocation.  No check is
/// weakened — `zip`'s reader still verifies the member's CRC-32 over every byte
/// it hands back.
const CAPACITY_HINT: u64 = 64 * 1024 * 1024;

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
/// `"The release file has no {}: {}"` (`zip`'s `ZipError` as the payload), then
/// `read_to_end` into a buffer sized from the member's uncompressed size →
/// `"{} cannot be read out of the release file: {}"`.
///
/// The real crate supplies the recovered error texts: an archive that is not a
/// ZIP is `"invalid Zip archive: Could not find EOCD"` (`"Could not find EOCD"`,
/// `"Oversized central directory"` and `"Multi-disk ZIP files are not
/// supported"` are all in `__const` 0x1023fd4e4–0x1023fd534), a missing name is
/// `"specified file not found in archive"`, and a member whose data does not
/// match its CRC-32 is `"Invalid checksum"`.  ZIP64, every compression method
/// `zip` was built with, and the CRC/size checks come with the crate.
pub fn unpack(bytes: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("The release file cannot be opened: {e}"))?;
    let mut entry = archive
        .by_name(name)
        .map_err(|e| format!("The release file has no {name}: {e}"))?;
    let mut out = Vec::with_capacity(entry.size().min(CAPACITY_HINT) as usize);
    entry
        .read_to_end(&mut out)
        .map_err(|e| format!("{name} cannot be read out of the release file: {e}"))?;
    Ok(out)
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
    use crate::testdata::{archive_with, archive_zip64_section, find, ARCHIVE};
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

    /// The `zip` fixture in `testdata`: one stored member, one deflated one.
    #[test]
    fn unpacks_the_member_it_is_named() {
        assert_eq!(unpack(ARCHIVE, "stored.txt").unwrap(), b"plain contents\n");
        assert_eq!(unpack(ARCHIVE, "hydra").unwrap(), b"the quick brown fox.\n");
    }

    /// Both compression methods are real: the stored member's bytes sit in the
    /// archive verbatim, and the deflated member's bytes are an RFC 1951 stream
    /// that an independent decoder (`flate2`, the crate `zip` itself uses)
    /// reproduces.
    #[test]
    fn reads_real_stored_and_deflated_members() {
        let mut archive = zip::ZipArchive::new(Cursor::new(ARCHIVE)).expect("opens");
        assert_eq!(archive.len(), 2);

        {
            let entry = archive.by_name("stored.txt").expect("present");
            assert_eq!(entry.compression(), zip::CompressionMethod::Stored);
            assert_eq!(entry.size(), 15);
            assert_eq!(entry.compressed_size(), 15);
            let start = entry.data_start().expect("a data offset") as usize;
            let stored = &ARCHIVE[start..start + entry.compressed_size() as usize];
            assert_eq!(stored, b"plain contents\n");
        }

        {
            let entry = archive.by_name("hydra").expect("present");
            assert_eq!(entry.compression(), zip::CompressionMethod::Deflated);
            assert_eq!(entry.size(), 21);
            assert_eq!(entry.compressed_size(), 23);
            let start = entry.data_start().expect("a data offset") as usize;
            let deflated = ARCHIVE[start..start + entry.compressed_size() as usize].to_vec();
            // Raw DEFLATE, no zlib wrapper: `flate2`'s `DeflateDecoder` takes it.
            let mut plain = Vec::new();
            flate2::read::DeflateDecoder::new(&deflated[..])
                .read_to_end(&mut plain)
                .expect("a real deflate stream");
            assert_eq!(plain, b"the quick brown fox.\n");
        }
    }

    /// What `zip`'s *writer* produces, `hydra_update` can read: stored,
    /// deflated, and a ZIP64 archive (`large_file`), which the hand-written
    /// reader had to parse out of the 0x0001 extra field itself.
    #[test]
    fn unpacks_what_the_real_writer_wrote() {
        for method in [zip::CompressionMethod::Stored, zip::CompressionMethod::Deflated] {
            let archive = archive_with(&[("hydra", b"a member written by zip itself\n")], method, false);
            assert_eq!(
                unpack(&archive, "hydra").unwrap(),
                b"a member written by zip itself\n",
                "{method:?}"
            );
        }

        // ZIP64, per entry: the central directory's 32-bit size fields carry
        // the sentinel and the real values live in the 0x0001 extra field.
        let zip64 = archive_with(
            &[("hydra", b"the same bytes, ZIP64\n")],
            zip::CompressionMethod::Deflated,
            true,
        );
        let central = find(&zip64, [0x50, 0x4b, 0x01, 0x02]).expect("a central header");
        assert_eq!(&zip64[central + 20..central + 28], &[0xff; 8], "the size sentinels");
        assert!(
            zip64[central..]
                .windows(2)
                .any(|window| window == [0x01, 0x00]),
            "no ZIP64 extra field"
        );
        assert_eq!(unpack(&zip64, "hydra").unwrap(), b"the same bytes, ZIP64\n");

        // ZIP64, whole archive: the `PK\x06\x06` record and its locator.
        let zip64 = archive_zip64_section(&[("hydra", b"a ZIP64 archive\n")], zip::CompressionMethod::Stored);
        assert!(find(&zip64, [0x50, 0x4b, 0x06, 0x06]).is_some(), "no ZIP64 EOCD");
        assert!(find(&zip64, [0x50, 0x4b, 0x06, 0x07]).is_some(), "no ZIP64 locator");
        assert_eq!(unpack(&zip64, "hydra").unwrap(), b"a ZIP64 archive\n");
    }

    #[test]
    fn unpack_reports_what_went_wrong() {
        assert_eq!(
            unpack(ARCHIVE, "SHA256SUMS.txt").unwrap_err(),
            "The release file has no SHA256SUMS.txt: specified file not found in archive"
        );
        let error = unpack(b"not a zip", "hydra").unwrap_err();
        assert!(
            error.starts_with("The release file cannot be opened: "),
            "{error}"
        );
        assert!(error.contains("Could not find EOCD"), "{error}");
        // An empty file has no EOCD either.
        let error = unpack(b"", "hydra").unwrap_err();
        assert!(error.contains("Could not find EOCD"), "{error}");
    }

    /// CRC-32 and the member's size are `zip`'s checks now: a single changed
    /// byte in a stored member is caught before the bytes are handed back.
    #[test]
    fn refuses_a_member_whose_checksum_does_not_match() {
        let mut broken = ARCHIVE.to_vec();
        let at = broken
            .windows(14)
            .position(|window| window == b"plain contents")
            .expect("the stored member");
        broken[at] = b'x';
        assert_eq!(
            unpack(&broken, "stored.txt").unwrap_err(),
            "stored.txt cannot be read out of the release file: Invalid checksum"
        );
        // The other member is untouched and still readable.
        assert_eq!(unpack(&broken, "hydra").unwrap(), b"the quick brown fox.\n");
    }

    /// A deflated member whose stream is damaged fails inside the read, with
    /// `zip`'s CRC check ("Invalid checksum") or its decoder's own complaint.
    #[test]
    fn refuses_a_member_whose_deflate_stream_is_damaged() {
        let central = find(ARCHIVE, [0x50, 0x4b, 0x01, 0x02]).expect("a central header");

        // A byte in the middle of `hydra`'s compressed data: still inflates,
        // so the CRC-32 is what catches it.
        let mut corrupt = ARCHIVE.to_vec();
        corrupt[central - 12] ^= 0xff;
        assert_eq!(
            unpack(&corrupt, "hydra").unwrap_err(),
            "hydra cannot be read out of the release file: Invalid checksum"
        );

        // The stream's last byte: the decoder runs out of input first.
        let mut cut = ARCHIVE.to_vec();
        cut[central - 1] ^= 0x01;
        assert_eq!(
            unpack(&cut, "hydra").unwrap_err(),
            "hydra cannot be read out of the release file: incomplete deflate stream"
        );

        // Neither of those touches the stored member.
        assert_eq!(unpack(&corrupt, "stored.txt").unwrap(), b"plain contents\n");
    }

    /// `zip` uses the declared uncompressed size as the decoder's buffer hint,
    /// not as an equality check: an archive that lies about the size but whose
    /// bytes and CRC-32 agree still reads.  (The hand-written reader compared
    /// the two and called a mismatch `"Invalid checksum"`; the real crate is
    /// what the binary had, and the CRC is what integrity rests on.)
    #[test]
    fn a_wrong_declared_size_is_not_an_error_of_its_own() {
        let member = b"twenty-one-ish bytes\n";
        let mut archive = archive_with(
            &[("hydra", member)],
            zip::CompressionMethod::Stored,
            false,
        );
        let central = find(&archive, [0x50, 0x4b, 0x01, 0x02]).expect("a central header");
        // Uncompressed size: +22 in the local header, +24 in the central one.
        for at in [22, central + 24] {
            archive[at..at + 4].copy_from_slice(&9_999u32.to_le_bytes());
        }
        assert_eq!(unpack(&archive, "hydra").unwrap(), member);
    }

    /// A truncated archive never opens at all: the end-of-central-directory
    /// record is incomplete, which is `zip`'s own wording (it names the block
    /// type, so only the stable prefix is asserted).
    #[test]
    fn refuses_a_truncated_archive() {
        let mut cut = ARCHIVE.to_vec();
        cut.truncate(ARCHIVE.len() - 1);
        let error = unpack(&cut, "hydra").unwrap_err();
        assert!(
            error.starts_with("The release file cannot be opened: invalid Zip archive: "),
            "{error}"
        );
        // Cutting the archive's tail off entirely is the recovered literal.
        assert_eq!(
            unpack(&ARCHIVE[..32], "hydra").unwrap_err(),
            "The release file cannot be opened: invalid Zip archive: Could not find EOCD"
        );
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

    /// `write_new` truncates what it finds: a longer `*.new` left over from a
    /// crashed run must not leave its tail behind the new bytes.
    #[test]
    fn overwrites_a_longer_leftover_new_file() {
        let scratch = Scratch::new();
        let place = scratch.write("hydra", b"current\n");
        scratch.write("hydra.new", &[b'z'; 4096]);
        put_in_place(place.to_str().unwrap(), b"short\n").expect("swaps");
        assert_eq!(fs::read(&place).unwrap(), b"short\n");
        assert!(!scratch.file("hydra.new").exists());
    }
}

