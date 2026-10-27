//! The lookout's state file.
//!
//! The file is not JSON and not TOML: `decode` (`0x101abd4f8`) and `encode`
//! (`0x101abd8d4`) hand-roll a little-endian binary layout, 13 bytes per
//! counted key:
//!
//! ```text
//! 0                    8      12                                    +13n
//! +--------------------+------+-------------------------------------+
//! | "HYDRALK1"         | u32  | n x u64 keys                        |  the set
//! +--------------------+------+-------------------------------------+------+-----+
//! | u32 m                                                              | ... |
//! +--------------------------------------------------------------------+-----+
//! followed by m x { u64 key, u8 occasions, u32 day }
//! ```
//!
//! `decode` rejects anything shorter than 8 bytes (`"cut short"`), a magic that
//! does not match (`"not a lookout file of this version"`) and trailing bytes
//! (`"more after the set than the set"`) — all three messages are read straight
//! out of `__cstring` at `0x1039c7947`.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Counting;

/// The state file's prefix (`0x103e7ce70`: `"lookout_"` + name + `".bin"`).
pub const PREFIX: &str = "lookout_";
/// The state file's suffix (see [`PREFIX`]).
pub const SUFFIX: &str = ".bin";
/// The extension an undecodable file is set aside with (`0x1039c77f4`, 7
/// bytes).
pub const SET_ASIDE: &str = "bin.bad";
/// The temporary file's name, appended to the state file's path
/// (`0x103e7ce80`: `".{}-{}.tmp"` with the process id and a counter).
pub const TEMP: &str = ".{}-{}.tmp";

/// How many temporary files this process has written (`0x10458ee50`).
static WRITES: AtomicUsize = AtomicUsize::new(0);

/// Why a state file could not be used.
///
/// `rename` distinguishes "the file did not decode" (set it aside and start
/// over) from "the file could not be read" (leave it alone and switch the
/// lookout off); the two kinds are the `w9` bit `Lookout::open` tests at
/// `0x101ab3614`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Should the file be set aside rather than kept?
    pub rename: bool,
    /// What went wrong, for the log line.
    pub text: String,
}

impl Problem {
    /// A decoding problem: the file is not one of ours.
    pub fn decode(text: impl Into<String>) -> Problem {
        Problem {
            rename: true,
            text: text.into(),
        }
    }

    /// A reading problem: keep the file, stop writing.
    pub fn io(text: impl Into<String>) -> Problem {
        Problem {
            rename: false,
            text: text.into(),
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// A state file that decoded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    /// The keys the lookout has learnt.
    pub known: HashSet<u64>,
    /// The keys it is still counting.
    pub counting: HashMap<u64, Counting>,
    /// How many bytes the file had (`Lookout::written`, `0x101ab3794`).
    pub len: u64,
}

/// The state file for `character` in `dir` (`0x101abce3c`).
///
/// `dir.join(format!("lookout_{}.bin", character.to_lowercase()))`: the name is
/// lowercased with `str::to_lowercase` (`0x101abce78`) before it is joined.
pub fn path(dir: &Path, character: &str) -> PathBuf {
    dir.join(format!("{PREFIX}{}{SUFFIX}", character.to_lowercase()))
}

/// Read a state file (`0x101abcf58`).
///
/// A file that is not there is an empty state, not an error (the branch at
/// `0x101abd02c` returns a zeroed state); every other problem is returned.
pub fn read(path: &Path) -> Result<Loaded, Problem> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(Loaded::default());
            }
            return Err(Problem::io(format!("{}: {}", path.display(), error)));
        }
    };
    let len = bytes.len() as u64;
    match decode(&bytes) {
        Ok(loaded) => Ok(Loaded { len, ..loaded }),
        Err(problem) => Err(problem),
    }
}

/// Write `bytes` to `path` atomically (`0x101abd154`).
///
/// Creates the parent directory first (`fs::create_dir_all`, mode `0o777` at
/// `0x101abd198`), writes `<path>.<pid>-<n>.tmp`, then renames it into place and
/// removes the temporary file if the rename fails.  Every error is rendered as
/// `"<path>: <error>"`.
pub fn write(path: &Path, bytes: &[u8]) -> Result<(), Problem> {
    if let Some(dir) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(dir) {
            return Err(Problem::io(format!("{}: {}", path.display(), error)));
        }
    }
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(temp_name(
        std::process::id(),
        WRITES.fetch_add(1, Ordering::Relaxed),
    ));
    let temporary = PathBuf::from(temporary);
    if let Err(error) = write_all(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err(Problem::io(format!("{}: {}", path.display(), error)));
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(Problem::io(format!("{}: {}", path.display(), error)));
    }
    Ok(())
}

/// The temporary file's suffix (`0x103e7ce80`: `".{}-{}.tmp"`, the process id
/// and a counter that starts at zero for every process).
pub fn temp_name(pid: u32, write: usize) -> String {
    format!(".{pid}-{write}.tmp")
}

fn write_all(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.flush()
}

/// Decode a state file (`0x101abd4f8`).
pub fn decode(bytes: &[u8]) -> Result<Loaded, Problem> {
    if bytes.len() <= 7 {
        return Err(Problem::decode("cut short"));
    }
    if bytes[..8] != crate::STATE_MAGIC {
        return Err(Problem::decode("not a lookout file of this version"));
    }
    let mut at = 8usize;
    let mut known = HashSet::new();
    let set_len = take_u32(bytes, at).ok_or_else(short)?;
    at += 4;
    for _ in 0..set_len {
        let key = take_u64(bytes, at).ok_or_else(short)?;
        known.insert(key);
        at += 8;
    }
    let counting_len = take_u32(bytes, at).ok_or_else(short)?;
    at += 4;
    let mut counting = HashMap::new();
    for _ in 0..counting_len {
        let key = take_u64(bytes, at).ok_or_else(short)?;
        let occasions = *bytes.get(at + 8).ok_or_else(short)?;
        let day = take_u32(bytes, at + 9).ok_or_else(short)?;
        counting.insert(key, Counting { day, occasions });
        at += 13;
    }
    if at != bytes.len() {
        return Err(Problem::decode("more after the set than the set"));
    }
    Ok(Loaded {
        known,
        counting,
        len: bytes.len() as u64,
    })
}

fn short() -> Problem {
    Problem::decode("cut short")
}

fn take_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn take_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

/// Encode a state file (`0x101abd8d4`).
///
/// The original walks the two maps in hash order, so the byte layout is fixed
/// but the order of the keys inside it is not; both counts saturate at
/// `u32::MAX` (`0x101abd9ec`: `csel x8, x23, #-1, lo`).
pub fn encode(known: &HashSet<u64>, counting: &HashMap<u64, Counting>) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + known.len() * 8 + counting.len() * 13);
    out.extend_from_slice(&crate::STATE_MAGIC);
    let set_len = u32::try_from(known.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&set_len.to_le_bytes());
    for key in known {
        out.extend_from_slice(&key.to_le_bytes());
    }
    let counting_len = u32::try_from(counting.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&counting_len.to_le_bytes());
    for (key, value) in counting {
        out.extend_from_slice(&key.to_le_bytes());
        out.push(value.occasions);
        out.extend_from_slice(&value.day.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (HashSet<u64>, HashMap<u64, Counting>) {
        let mut known = HashSet::new();
        known.insert(0x0102_0304_0506_0708);
        let mut counting = HashMap::new();
        counting.insert(
            0xffee_ddcc_bbaa_9988,
            Counting {
                day: 20_123,
                occasions: 4,
            },
        );
        (known, counting)
    }

    #[test]
    fn the_state_file_is_named_after_the_character_in_lower_case() {
        let path = path(Path::new("/tmp/hydra"), "Aelith");
        assert_eq!(path, PathBuf::from("/tmp/hydra/lookout_aelith.bin"));
        assert_eq!(PREFIX, "lookout_");
        assert_eq!(SUFFIX, ".bin");
    }

    #[test]
    fn a_state_file_round_trips() {
        let (known, counting) = state();
        let bytes = encode(&known, &counting);
        assert_eq!(&bytes[..8], b"HYDRALK1");
        // 8 magic + 4 set length + 8 keys + 4 counting length + 13 per key
        assert_eq!(bytes.len(), 8 + 4 + 8 + 4 + 13);
        let loaded = decode(&bytes).expect("decoded");
        assert_eq!(loaded.known, known);
        assert_eq!(loaded.counting, counting);
        assert_eq!(loaded.len, bytes.len() as u64);
    }

    #[test]
    fn an_empty_state_file_is_header_only() {
        let bytes = encode(&HashSet::new(), &HashMap::new());
        assert_eq!(bytes.len(), 16);
        let loaded = decode(&bytes).expect("decoded");
        assert!(loaded.known.is_empty());
        assert!(loaded.counting.is_empty());
    }

    #[test]
    fn a_state_file_that_is_too_short_says_so() {
        assert_eq!(
            decode(b"HYDRAL").expect_err("short"),
            Problem::decode("cut short")
        );
        assert!(decode(&[]).is_err());
        let problem = decode(b"HYDRALK1").expect_err("short");
        assert_eq!(problem.text, "cut short");
        assert!(problem.rename);
    }

    #[test]
    fn a_state_file_with_another_magic_is_rejected() {
        let mut bytes = encode(&HashSet::new(), &HashMap::new());
        bytes[7] = b'2';
        assert_eq!(
            decode(&bytes).expect_err("bad magic").text,
            "not a lookout file of this version"
        );
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = encode(&HashSet::new(), &HashMap::new());
        bytes.push(0);
        assert_eq!(
            decode(&bytes).expect_err("trailing").text,
            "more after the set than the set"
        );
    }

    #[test]
    fn a_missing_file_reads_as_an_empty_state() {
        let path = std::env::temp_dir().join(format!(
            "hydra-lookout-missing-{}/lookout_x.bin",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let loaded = read(&path).expect("absent file");
        assert_eq!(loaded, Loaded::default());
    }

    #[test]
    fn writing_creates_the_directory_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("hydra-lookout-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = path(&dir.join("nested"), "Aelith");
        write(&path, b"hello").expect("written");
        assert_eq!(std::fs::read(&path).unwrap(), b"hello");
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter(|name| name.to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
    }

    #[test]
    fn the_temporary_file_is_named_after_the_process_and_a_counter() {
        assert_eq!(temp_name(4242, 0), ".4242-0.tmp");
        assert_eq!(temp_name(1, 17), ".1-17.tmp");
        assert_eq!(TEMP, ".{}-{}.tmp");
    }

    #[test]
    fn a_missing_counted_key_is_not_invented() {
        // the set length says two keys but only one is there
        let mut bytes = encode(&HashSet::new(), &HashMap::new());
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&7u64.to_le_bytes());
        assert_eq!(decode(&bytes).expect_err("short").text, "cut short");
    }
}
