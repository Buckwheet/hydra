//! Archiving old day files.
//!
//! Recovered from `player_log::archive::*`:
//! `0x101a64140` (`path`), `0x101a6487c` (`names`), `0x101a649c4` (`sweep`),
//! `0x101a66938` (`usage`), `0x101a66ea8` (`Archive::period`),
//! `0x101a66fa0` (`Archive::from_word`), `0x101a671f0` (`archives`),
//! `0x101a674b0` (`manifest`).
//!
//! **ESTABLISHED** formats:
//!
//! ```text
//! "{}.gz"           one archive file                    0x103e7c43a
//! "{}\t{}\n"        a manifest line: name, size         0x103e7c3f3
//! "week-{}"         the week period name                0x103e7c432
//! "{}: {}"          the shared error shape              0x103e7c3ed
//! "not a manifest line: {}"                             0x103e7c948
//! "{} did not read back as written; the day-files are kept"  0x103e7c3fa
//! ```
//!
//! **ESTABLISHED** behaviour (`sweep`):
//! * the period comes from `eastern::date` + `eastern::stretch_start` +
//!   `Archive::period`, and the day files come from `writer::days`;
//! * the archive is opened with `OpenOptions`, written through a `BufWriter`,
//!   `sync_data`-ed, and its stream position is recorded (`stream_position`);
//! * **each member is read back** and compared, and if anything is off the
//!   day files are kept and the message above is reported;
//! * only then are the day files removed (`remove_file`) and the manifest
//!   renamed into place (`rename`).
//!
//! **std-only departure** (removed in Phase 1): this used to write plain
//! tar-less concatenations because `flate2` was not available.  The archive is
//! now **real gzip**: every member is written as its own gzip member
//! (`flate2::write::GzEncoder`) which is appended to `<period>.gz`, and the
//! sidecar `<period>.manifest` still records `name<TAB>size` with the
//! *uncompressed* size of each member in the order the members were appended.
//! Reading goes through `flate2::read::MultiGzDecoder`, so a multi-member
//! archive decompresses to exactly the concatenation the old writer produced;
//! an archive that is not gzip (one written before Phase 1) is still read as a
//! plain concatenation, which keeps old logs readable.

use super::writer;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The extension every archive file carries.
pub const EXT: &str = "gz";
/// The manifest's file name (inside the log directory).
pub const MANIFEST: &str = "MANIFEST";
/// The prefix of a week archive.
pub const WEEK: &str = "week-";
/// The prefix of a month archive.
pub const MONTH: &str = "month-";

/// How much a sweep folds together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Archive {
    /// one day per file, nothing to do
    #[default]
    Day,
    /// a week, named `week-<first day>`
    Week,
    /// a month, named `month-<first day>`
    Month,
}

impl Archive {
    /// The word a [`Archive`] is asked for by.  **ESTABLISHED** shape
    /// (`0x101a66fa0`): matches the word, refusing anything else.
    pub fn from_word(word: &str) -> Option<Archive> {
        match word.to_ascii_lowercase().as_str() {
            "day" | "days" | "none" => Some(Archive::Day),
            "week" | "weeks" => Some(Archive::Week),
            "month" | "months" => Some(Archive::Month),
            _ => None,
        }
    }

    /// The word this archive is written as.
    pub fn word(self) -> &'static str {
        match self {
            Archive::Day => "day",
            Archive::Week => "week",
            Archive::Month => "month",
        }
    }

    /// The period name for the day `day`: the first day of its week or month.
    ///
    /// **ESTABLISHED** (`0x101a66ea8`): `eastern::week_start` + `eastern::format`,
    /// with the `week-` prefix literal.
    pub fn period(self, day: &str) -> Option<String> {
        let (y, m, d) = parse_day(day)?;
        match self {
            Archive::Day => Some(day.to_string()),
            Archive::Week => Some(format!("{WEEK}{}", week_start(y, m, d))),
            Archive::Month => Some(format!("{MONTH}{y:04}-{m:02}-01")),
        }
    }
}

/// Split a `YYYY-MM-DD` day key.
pub fn parse_day(day: &str) -> Option<(i64, u32, u32)> {
    let mut parts = day.split('-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y, m, d))
}

/// The Monday of the week `(y, m, d)` is in, as a `YYYY-MM-DD` key.
///
/// `hydra_net::eastern::week_start` is not available here, so this is the
/// documented equivalent: step back to the most recent Monday.
pub fn week_start(y: i64, m: u32, d: u32) -> String {
    let days = crate::notes::days_from_civil(y, m, d);
    // 1970-01-01 was a Thursday, so day 0 is weekday 4 with Monday == 1.
    let weekday = ((days + 3).rem_euclid(7) + 1) as i64; // Mon=1..Sun=7
    let monday = days - (weekday - 1);
    let (y, m, d) = crate::notes::civil_from_days(monday);
    format!("{y:04}-{m:02}-{d:02}")
}

/// `<dir>/<period>.<EXT>`.
///
/// **ESTABLISHED** (`0x101a64140`): `writer::dir` + `writer::file_name` +
/// `Path::join`, then `"{}.gz"`.
pub fn path(dir: &Path, period: &str) -> PathBuf {
    dir.join(format!("{period}.{EXT}"))
}

/// `<dir>/MANIFEST`.
pub fn manifest_path(dir: &Path) -> PathBuf {
    dir.join(MANIFEST)
}

/// The member names recorded in an archive's manifest.
///
/// **ESTABLISHED** (`0x101a6487c`): `OpenOptions` + `manifest` + a sort.
pub fn names(archive: &Path) -> std::io::Result<Vec<String>> {
    Ok(manifest(archive)?.into_keys().collect())
}

/// The manifest of an archive: member name to byte size.
///
/// **ESTABLISHED** (`0x101a674b0`): the manifest is a sidecar file of
/// `name<TAB>size` lines (`from_utf8`, `"not a manifest line: {}"`).
pub fn manifest(archive: &Path) -> std::io::Result<BTreeMap<String, u64>> {
    let path = archive.with_extension(MANIFEST.to_lowercase());
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e),
    };
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let Some((name, size)) = line.split_once('\t') else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("not a manifest line: {line}"),
            ));
        };
        let size: u64 = size.parse().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("not a manifest line: {line}"),
            )
        })?;
        out.insert(name.to_string(), size);
    }
    Ok(out)
}

/// The two bytes every gzip stream starts with.
pub const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Read one member out of an archive.
///
/// **ESTABLISHED** shape: the member is looked up in the manifest and its
/// bytes are read back.  **Phase 1**: the body is gzip, so it is decompressed
/// with `MultiGzDecoder` (one gzip member per archived day file) and sliced by
/// the manifest's uncompressed sizes; an archive whose body is not gzip — one
/// written before Phase 1 — is sliced from the raw bytes, which is the plain
/// concatenation the old writer produced.
pub fn read_archive(archive: &Path, name: &str) -> std::io::Result<Option<String>> {
    let manifest = manifest(archive)?;
    let Some(size) = manifest.get(name).copied() else {
        return Ok(None);
    };
    let body = body_of(archive)?;
    // The members are stored back to back in manifest order.
    let mut offset = 0usize;
    for (member, len) in &manifest {
        if member == name {
            let start = offset.min(body.len());
            let end = (start + *len as usize).min(body.len());
            return Ok(Some(String::from_utf8_lossy(&body[start..end]).into_owned()));
        }
        offset += *len as usize;
    }
    let _ = size;
    Ok(None)
}

/// Is this archive's body gzip?
pub fn is_gzip(archive: &Path) -> std::io::Result<bool> {
    let mut head = [0u8; 2];
    let mut file = fs::File::open(archive)?;
    match file.read_exact(&mut head) {
        Ok(()) => Ok(head == GZIP_MAGIC),
        // An archive shorter than the magic is empty, not corrupt.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(e) => Err(e),
    }
}

/// The decompressed body of an archive, or its raw bytes when it is not gzip.
fn body_of(archive: &Path) -> std::io::Result<Vec<u8>> {
    if !is_gzip(archive)? {
        return fs::read(archive);
    }
    let file = fs::File::open(archive)?;
    let mut body = Vec::new();
    MultiGzDecoder::new(file).read_to_end(&mut body)?;
    Ok(body)
}

/// Append one member to the archive as its own gzip member.
///
/// **ESTABLISHED**: the original wrote through a `BufWriter`, `sync_data`-ed
/// the file and recorded the stream position of every member; this writes
/// through `flate2`'s `GzEncoder` instead, and the manifest records the
/// uncompressed size so a member can be found again.
fn append_member(file: &mut fs::File, text: &[u8]) -> std::io::Result<()> {
    let mut encoder = GzEncoder::new(file, Compression::default());
    encoder.write_all(text)?;
    // `finish` writes the gzip trailer; the inner `&mut File` is released
    // afterwards so the caller can `sync_data` it.
    encoder.finish()?;
    Ok(())
}

/// Fold every day file in `dir` into archives of `how`.
///
/// Returns the archives it wrote or extended.  Day files are only removed once
/// the archive has been read back and compared.
pub fn sweep(dir: &Path, how: Archive) -> Result<Vec<PathBuf>, String> {
    let mut groups: BTreeMap<String, Vec<(String, PathBuf)>> = BTreeMap::new();
    for (day, path) in writer::days(dir) {
        let Some(period) = how.period(&day) else {
            continue;
        };
        groups.entry(period).or_default().push((day, path));
    }
    let mut written = Vec::new();
    for (period, members) in groups {
        if members.len() < 2 && how != Archive::Day {
            continue;
        }
        let archive = path(dir, &period);
        let mut manifest = manifest(&archive).unwrap_or_default();
        let mut used = Vec::new();
        {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&archive)
                .map_err(|e| format!("{}: {e}", archive.display()))?;
            for (_day, member) in &members {
                let name = member
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let text =
                    fs::read_to_string(member).map_err(|e| format!("{}: {e}", member.display()))?;
                // One gzip member per day file, appended to the archive; the
                // manifest keeps the uncompressed size.
                append_member(&mut file, text.as_bytes())
                    .map_err(|e| format!("{}: {e}", archive.display()))?;
                manifest.insert(name, text.len() as u64);
                used.push(member.clone());
            }
            file.sync_data().map_err(|e| format!("{}: {e}", archive.display()))?;
        }
        // Record the manifest, then read every member back before deleting
        // anything.
        let sidecar = archive.with_extension(MANIFEST.to_lowercase());
        let rendered: String = manifest
            .iter()
            .map(|(name, size)| format!("{name}\t{size}\n"))
            .collect();
        let partial = sidecar.with_extension("partial");
        fs::write(&partial, rendered).map_err(|e| format!("{}: {e}", partial.display()))?;
        fs::rename(&partial, &sidecar).map_err(|e| format!("{}: {e}", sidecar.display()))?;
        let mut verified = true;
        for (_day, member) in &members {
            let name = member
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let want = fs::read_to_string(member).ok();
            let got = read_archive(&archive, &name).ok().flatten();
            if want != got {
                verified = false;
            }
        }
        if !verified {
            return Err(format!(
                "{} did not read back as written; the day-files are kept",
                archive.display()
            ));
        }
        for member in used {
            let _ = fs::remove_file(member);
        }
        written.push(archive);
    }
    Ok(written)
}

/// The archives in `dir`, oldest first.
///
/// **ESTABLISHED** shape (`0x101a671f0`): `writer::dir` + `read_dir`, keep the
/// `.gz` files, sort.
pub fn archives(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(period) = name.strip_suffix(&format!(".{EXT}")) {
            out.push((period.to_string(), entry.path()));
        }
    }
    out.sort();
    out
}

/// How many bytes the day files and the archives take up.
///
/// **ESTABLISHED** shape (`0x101a66938`): `writer::days` + `metadata`, then the
/// archives, returning `(days, archived)` byte counts.
pub fn usage(dir: &Path) -> (u64, u64) {
    let live: u64 = writer::days(dir)
        .iter()
        .filter_map(|(_, p)| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    let archived: u64 = archives(dir)
        .iter()
        .filter_map(|(_, p)| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    (live, archived)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_log::writer::PlayerWriter;

    #[test]
    fn periods_name_the_first_day_of_the_week_and_month() {
        // 2024-05-01 is a Wednesday.
        assert_eq!(
            Archive::Week.period("2024-05-01").unwrap(),
            "week-2024-04-29"
        );
        assert_eq!(
            Archive::Month.period("2024-05-17").unwrap(),
            "month-2024-05-01"
        );
        assert_eq!(Archive::Day.period("2024-05-17").unwrap(), "2024-05-17");
        assert_eq!(Archive::Week.period("nope"), None);
        // 2024-04-29 really is a Monday.
        assert_eq!(week_start(2024, 4, 29), "2024-04-29");
        assert_eq!(week_start(2024, 5, 5), "2024-04-29");
    }

    #[test]
    fn words_map_to_archives() {
        assert_eq!(Archive::from_word("WEEK"), Some(Archive::Week));
        assert_eq!(Archive::from_word("months"), Some(Archive::Month));
        assert_eq!(Archive::from_word("year"), None);
        assert_eq!(Archive::Week.word(), "week");
    }

    #[test]
    fn paths_and_manifest_lines() {
        let dir = Path::new("/l");
        assert_eq!(path(dir, "week-2024-04-29"), PathBuf::from("/l/week-2024-04-29.gz"));
        assert_eq!(manifest_path(dir), PathBuf::from("/l/MANIFEST"));
        assert_eq!(EXT, "gz");
        assert_eq!(WEEK, "week-");
    }

    #[test]
    fn a_bad_manifest_line_is_refused_with_the_recovered_message() {
        let dir = std::env::temp_dir().join(format!("hydra-arch-bad-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let archive = path(&dir, "week-2024-04-29");
        fs::write(archive.with_extension("manifest"), "no tab here\n").unwrap();
        let e = manifest(&archive).unwrap_err();
        assert_eq!(e.to_string(), "not a manifest line: no tab here");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_folds_days_into_a_week_and_reads_them_back() {
        let root = std::env::temp_dir().join(format!("hydra-arch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        w.write_at("2024-04-29", "10:00", "speech", "monday").unwrap();
        w.write_at("2024-04-30", "10:00", "speech", "tuesday").unwrap();
        w.flush().unwrap();
        let dir = w.directory().to_path_buf();
        let written = sweep(&dir, Archive::Week).unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0], path(&dir, "week-2024-04-29"));
        let names = names(&written[0]).unwrap();
        assert_eq!(names.len(), 2);
        assert!(names[0].starts_with("2024-04-29"));
        let text = read_archive(&written[0], &names[0]).unwrap().unwrap();
        assert!(text.contains("monday"));
        // the day files are gone, the archive is there
        assert_eq!(writer::days(&dir).len(), 0);
        assert_eq!(archives(&dir).len(), 1);
        let (live, archived) = usage(&dir);
        assert_eq!(live, 0);
        assert!(archived > 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_archive_is_real_gzip_with_one_member_per_day() {
        let root = std::env::temp_dir().join(format!("hydra-gz-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        w.write_at("2024-04-29", "10:00", "speech", "monday").unwrap();
        w.write_at("2024-04-30", "10:00", "speech", "tuesday").unwrap();
        w.flush().unwrap();
        let dir = w.directory().to_path_buf();
        let source: usize = writer::days(&dir)
            .iter()
            .filter_map(|(_, p)| fs::metadata(p).ok())
            .map(|m| m.len() as usize)
            .sum();
        let written = sweep(&dir, Archive::Week).unwrap();
        let archive = &written[0];
        // The body starts with the gzip magic, and decompresses back to the
        // source bytes (a two-line day file is too small for the per-member
        // gzip header to pay for itself, so the file itself may grow).
        assert!(is_gzip(archive).unwrap(), "the archive is gzip");
        let raw = fs::read(archive).unwrap();
        assert_eq!(&raw[..2], &GZIP_MAGIC);
        assert!(source > 0);
        // `MultiGzDecoder` over the whole file gives the plain concatenation,
        // in manifest (name) order.
        let manifest = manifest(archive).unwrap();
        let file = fs::File::open(archive).unwrap();
        let mut body = Vec::new();
        MultiGzDecoder::new(file).read_to_end(&mut body).unwrap();
        let mut expected = Vec::new();
        let mut offset = 0;
        for (name, size) in &manifest {
            let member = &body[offset..offset + *size as usize];
            expected.extend_from_slice(member);
            offset += *size as usize;
            let text = read_archive(archive, name).unwrap().unwrap();
            assert_eq!(text.as_bytes(), member, "{name}");
        }
        assert_eq!(offset, body.len(), "the manifest covers the whole body");
        assert!(!expected.is_empty());
        // A name that is not in the manifest is `None`, not an error.
        assert!(read_archive(archive, "nope.log").unwrap().is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_plain_concatenation_written_before_phase_1_still_reads() {
        // Backwards compatibility: an archive body that is not gzip is read as
        // the plain concatenation the old writer produced.
        let dir = std::env::temp_dir().join(format!("hydra-plain-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let archive = path(&dir, "week-2024-04-29");
        let first = "one\n";
        let second = "two\n";
        fs::write(&archive, format!("{first}{second}")).unwrap();
        fs::write(
            archive.with_extension("manifest"),
            format!("a.log\t{}\nb.log\t{}\n", first.len(), second.len()),
        )
        .unwrap();
        assert!(!is_gzip(&archive).unwrap());
        assert_eq!(read_archive(&archive, "a.log").unwrap().unwrap(), first);
        assert_eq!(read_archive(&archive, "b.log").unwrap().unwrap(), second);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_archive_or_missing_member_is_not_an_error() {
        let dir = std::env::temp_dir().join(format!("hydra-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let archive = path(&dir, "month-2024-05-01");
        fs::write(&archive, b"").unwrap();
        assert!(!is_gzip(&archive).unwrap(), "a short file is not gzip");
        assert!(read_archive(&archive, "x").unwrap().is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_sweep_appends_to_the_same_gzip_archive() {
        let root = std::env::temp_dir().join(format!("hydra-gz2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut w = PlayerWriter::new(&root, "Aelith");
        w.write_at("2024-04-29", "10:00", "speech", "monday").unwrap();
        w.write_at("2024-04-30", "10:00", "speech", "tuesday").unwrap();
        w.flush().unwrap();
        let dir = w.directory().to_path_buf();
        let first = sweep(&dir, Archive::Week).unwrap();
        assert_eq!(first.len(), 1);
        // Two more days in the same week, swept later.
        w.write_at("2024-05-01", "10:00", "speech", "wednesday")
            .unwrap();
        w.write_at("2024-05-02", "10:00", "speech", "thursday")
            .unwrap();
        w.flush().unwrap();
        let second = sweep(&dir, Archive::Week).unwrap();
        assert_eq!(second, first, "the same period is extended, not replaced");
        assert!(is_gzip(&second[0]).unwrap());
        let names = names(&second[0]).unwrap();
        assert_eq!(names.len(), 4);
        let mut seen = Vec::new();
        for name in &names {
            seen.push(read_archive(&second[0], name).unwrap().unwrap());
        }
        for word in ["monday", "tuesday", "wednesday", "thursday"] {
            assert!(
                seen.iter().any(|text| text.contains(word)),
                "{word} is missing from {seen:?}"
            );
        }
        let _ = fs::remove_dir_all(&root);
    }
}
