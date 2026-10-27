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
//! **std-only departure**: `flate2` is not available, so the archives this
//! crate writes are plain tar-less concatenations with the same `{name}\t{size}`
//! manifest.  They round-trip through [`read_archive`] and are verified the
//! same way; they are simply not compressed.  The file extension stays `.gz` so
//! the on-disk layout matches the recovered one, and `NOTES.md` records the
//! departure.

use super::writer;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
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

/// Read one member out of an archive.
pub fn read_archive(archive: &Path, name: &str) -> std::io::Result<Option<String>> {
    let manifest = manifest(archive)?;
    let Some(size) = manifest.get(name).copied() else {
        return Ok(None);
    };
    let mut file = fs::File::open(archive)?;
    // The members are stored back to back in manifest order.
    let mut offset = 0u64;
    for (member, len) in &manifest {
        if member == name {
            file.seek(SeekFrom::Start(offset))?;
            let mut buf = vec![0u8; *len as usize];
            file.read_exact(&mut buf)?;
            return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
        }
        offset += *len;
    }
    let _ = size;
    Ok(None)
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
        let mut body = Vec::new();
        let mut manifest = manifest(&archive).unwrap_or_default();
        let mut used = Vec::new();
        for (_day, member) in &members {
            let name = member
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let text = fs::read_to_string(member).map_err(|e| format!("{}: {e}", member.display()))?;
            manifest.insert(name.clone(), text.len() as u64);
            body.extend_from_slice(text.as_bytes());
            used.push(member.clone());
        }
        {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&archive)
                .map_err(|e| format!("{}: {e}", archive.display()))?;
            file.write_all(&body).map_err(|e| format!("{}: {e}", archive.display()))?;
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
}
