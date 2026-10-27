//! A minimal ZIP reader: enough of APPNOTE.TXT for a release archive — the
//! central directory, stored and deflated members, and CRC-32 checking.
//!
//! The original used `zip` 9.0.0 (`zip::read::zip_archive::ZipArchive::new`
//! @0x1010ad178, `ZipFileData::find_content` / `make_reader` inlined into
//! `swap::unpack`); the error strings below are the ones `zip` produced and the
//! binary still carries in `__const` at 0x1023fd4e4–0x1023fd534
//! (`"Oversized central directory"`, `"Multi-disk ZIP files are not supported"`,
//! `"Could not find EOCD"`, `"Invalid checksum"` …).

use crate::inflate;

const EOCD: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;
const EOCD64_LOCATOR: u32 = 0x0706_4b50;
const EOCD64: u32 = 0x0606_4b50;

/// One member of the archive, as the central directory describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// 0 = stored, 8 = deflated.  Anything else is unsupported.
    pub method: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub size: u64,
    offset: u64,
}

/// A parsed archive: the member list plus the bytes it was read from.
#[derive(Debug)]
pub struct Archive<'a> {
    bytes: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> Archive<'a> {
    /// `ZipArchive::new(Cursor::new(bytes))`.
    pub fn new(bytes: &'a [u8]) -> Result<Archive<'a>, String> {
        let eocd = find_eocd(bytes)?;
        let mut entries = Vec::new();
        let mut at = eocd.directory_offset as usize;
        for _ in 0..eocd.count {
            let entry = central_entry(bytes, at)?;
            at = entry.1;
            entries.push(entry.0);
        }
        Ok(Archive { bytes, entries })
    }

    /// Every member, in central-directory order (test and diagnostic use: the
    /// binary only ever looks a name up).
    #[allow(dead_code)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// `ZipArchive::by_name`.
    pub fn by_name(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// `ZipFile::read_to_end` for one member.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, String> {
        let data = self.member_bytes(entry)?;
        let bytes = match entry.method {
            0 => data.to_vec(),
            8 => inflate::inflate(data)?,
            other => {
                return Err(format!(
                    "Compression method {other} is not supported"
                ))
            }
        };
        if bytes.len() as u64 != entry.size {
            return Err("Invalid checksum".to_string());
        }
        if crc32(&bytes) != entry.crc32 {
            return Err("Invalid checksum".to_string());
        }
        Ok(bytes)
    }

    fn member_bytes(&self, entry: &Entry) -> Result<&'a [u8], String> {
        let at = entry.offset as usize;
        let header = read_u32(self.bytes, at).ok_or_else(|| "Invalid local file header offset".to_string())?;
        if header != LOCAL {
            return Err("Invalid local file header offset".to_string());
        }
        let name_len = read_u16(self.bytes, at + 26).ok_or_else(truncated)? as usize;
        let extra_len = read_u16(self.bytes, at + 28).ok_or_else(truncated)? as usize;
        let start = at + 30 + name_len + extra_len;
        let end = start
            .checked_add(entry.compressed_size as usize)
            .ok_or_else(truncated)?;
        self.bytes
            .get(start..end)
            .ok_or_else(truncated)
    }
}

fn truncated() -> String {
    "Unexpected end of file".to_string()
}

struct Eocd {
    count: u64,
    directory_offset: u64,
}

/// Locate the end-of-central-directory record: it sits within the last
/// 22 + 65535 bytes, since the archive comment is at most 64 KiB.
fn find_eocd(bytes: &[u8]) -> Result<Eocd, String> {
    if bytes.len() < 22 {
        return Err("Could not find EOCD".to_string());
    }
    let first = bytes.len().saturating_sub(22 + 0xffff);
    let mut at = bytes.len() - 22;
    loop {
        if read_u32(bytes, at) == Some(EOCD) {
            let comment_len = read_u16(bytes, at + 20).ok_or_else(truncated)? as usize;
            if at + 22 + comment_len <= bytes.len() {
                let count = read_u16(bytes, at + 10).ok_or_else(truncated)?;
                let directory_size = read_u32(bytes, at + 12).ok_or_else(truncated)?;
                let directory_offset = read_u32(bytes, at + 16).ok_or_else(truncated)?;
                if count == 0xffff || directory_offset == 0xffff_ffff || directory_size == 0xffff_ffff
                {
                    let eocd64 = find_eocd64(bytes, at)?;
                    return Ok(eocd64);
                }
                if directory_offset as usize + directory_size as usize > bytes.len() {
                    return Err("Oversized central directory".to_string());
                }
                return Ok(Eocd {
                    count: count as u64,
                    directory_offset: directory_offset as u64,
                });
            }
        }
        if at == first {
            return Err("Could not find EOCD".to_string());
        }
        at -= 1;
    }
}

/// ZIP64: the EOCD64 record is pointed at by a locator just before the EOCD.
fn find_eocd64(bytes: &[u8], eocd_at: usize) -> Result<Eocd, String> {
    let locator = eocd_at
        .checked_sub(20)
        .ok_or_else(|| "Could not find EOCD64".to_string())?;
    if read_u32(bytes, locator) != Some(EOCD64_LOCATOR) {
        return Err("Could not find EOCD64".to_string());
    }
    let eocd64_offset = read_u64(bytes, locator + 8).ok_or_else(truncated)? as usize;
    if read_u32(bytes, eocd64_offset) != Some(EOCD64) {
        return Err("Could not find EOCD64".to_string());
    }
    let count = read_u64(bytes, eocd64_offset + 32).ok_or_else(truncated)?;
    let directory_size = read_u64(bytes, eocd64_offset + 40).ok_or_else(truncated)?;
    let directory_offset = read_u64(bytes, eocd64_offset + 48).ok_or_else(truncated)?;
    if directory_offset as usize + directory_size as usize > bytes.len() {
        return Err("Oversized central directory".to_string());
    }
    Ok(Eocd { count, directory_offset })
}

/// Parse one central-directory record; returns it and the offset of the next.
fn central_entry(bytes: &[u8], at: usize) -> Result<(Entry, usize), String> {
    if read_u32(bytes, at) != Some(CENTRAL) {
        return Err("Invalid CDFH offset in EOCD".to_string());
    }
    let method = read_u16(bytes, at + 10).ok_or_else(truncated)?;
    let crc32 = read_u32(bytes, at + 16).ok_or_else(truncated)?;
    let mut compressed_size = read_u32(bytes, at + 20).ok_or_else(truncated)? as u64;
    let mut size = read_u32(bytes, at + 24).ok_or_else(truncated)? as u64;
    let name_len = read_u16(bytes, at + 28).ok_or_else(truncated)? as usize;
    let extra_len = read_u16(bytes, at + 30).ok_or_else(truncated)? as usize;
    let comment_len = read_u16(bytes, at + 32).ok_or_else(truncated)? as usize;
    let mut offset = read_u32(bytes, at + 42).ok_or_else(truncated)? as u64;
    let name_at = at + 46;
    let name = bytes
        .get(name_at..name_at + name_len)
        .ok_or_else(truncated)?;
    let name = String::from_utf8_lossy(name).into_owned();
    // ZIP64 extended information extra field (0x0001) fills in the values the
    // 32-bit fields could not hold, in this order, only for the fields set to
    // 0xffffffff.
    let extra = bytes
        .get(name_at + name_len..name_at + name_len + extra_len)
        .ok_or_else(truncated)?;
    let mut rest = extra;
    while rest.len() >= 4 {
        let id = u16::from_le_bytes([rest[0], rest[1]]);
        let len = u16::from_le_bytes([rest[2], rest[3]]) as usize;
        let body = rest.get(4..4 + len).ok_or_else(truncated)?;
        if id == 0x0001 {
            let mut at = 0usize;
            for field in [&mut size, &mut compressed_size, &mut offset] {
                if *field == 0xffff_ffff && at + 8 <= body.len() {
                    let mut word = [0u8; 8];
                    word.copy_from_slice(&body[at..at + 8]);
                    *field = u64::from_le_bytes(word);
                    at += 8;
                }
            }
        }
        rest = &rest[4 + len..];
    }
    let next = name_at + name_len + extra_len + comment_len;
    Ok((
        Entry { name, method, crc32, compressed_size, size, offset },
        next,
    ))
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let four = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([four[0], four[1], four[2], four[3]]))
}

fn read_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let eight = bytes.get(at..at + 8)?;
    let mut word = [0u8; 8];
    word.copy_from_slice(eight);
    Some(u64::from_le_bytes(word))
}

/// The CRC-32 ZIP uses (reflected, polynomial 0xEDB88320).
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real archive, built once with Python's `zipfile` and pasted here: one
    /// stored member and one deflated member.
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
    fn lists_both_members() {
        let archive = Archive::new(ARCHIVE).expect("opens");
        let names: Vec<&str> = archive.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["stored.txt", "hydra"]);
        assert_eq!(archive.entries()[0].method, 0);
        assert_eq!(archive.entries()[0].size, 15);
        assert_eq!(archive.entries()[1].method, 8);
        assert_eq!(archive.entries()[1].size, 21);
        assert_eq!(archive.entries()[1].compressed_size, 23);
    }

    #[test]
    fn reads_a_stored_member() {
        let archive = Archive::new(ARCHIVE).unwrap();
        let entry = archive.by_name("stored.txt").expect("present");
        assert_eq!(archive.read(entry).unwrap(), b"plain contents\n");
    }

    #[test]
    fn reads_a_deflated_member() {
        let archive = Archive::new(ARCHIVE).unwrap();
        let entry = archive.by_name("hydra").expect("present");
        assert_eq!(archive.read(entry).unwrap(), b"the quick brown fox.\n");
    }

    #[test]
    fn a_missing_member_is_none() {
        let archive = Archive::new(ARCHIVE).unwrap();
        assert!(archive.by_name("SHA256SUMS.txt").is_none());
        // The name must match exactly.
        assert!(archive.by_name("hydro").is_none());
    }

    #[test]
    fn refuses_broken_archives() {
        assert_eq!(Archive::new(b"").unwrap_err(), "Could not find EOCD");
        assert_eq!(
            Archive::new(&ARCHIVE[..ARCHIVE.len() - 1]).unwrap_err(),
            "Could not find EOCD"
        );
        // A corrupted member fails its CRC.
        let mut broken = ARCHIVE.to_vec();
        let at = broken
            .windows(14)
            .position(|w| w == b"plain contents")
            .unwrap();
        broken[at] = b'x';
        let archive = Archive::new(&broken).unwrap();
        let entry = archive.by_name("stored.txt").unwrap();
        assert_eq!(archive.read(entry).unwrap_err(), "Invalid checksum");
    }

    #[test]
    fn crc32_matches_the_reference() {
        // The values `zipfile` wrote into the fixture above.
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"plain contents\n"), 0x46cd_2050);
        assert_eq!(crc32(b"the quick brown fox.\n"), 0x7510_3de4);
    }
}
