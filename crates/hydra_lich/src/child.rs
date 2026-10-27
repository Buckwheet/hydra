//! Child-process helpers for the Lich relay.
//!
//! The original crate used `tokio::process` for the Lich child.  This
//! std-only reconstruction keeps the recovered logic (password-env-var
//! filtering, Ruby discovery, token generation) but uses `std::process`
//! and `/dev/urandom` instead.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Environment-variable name prefix that carries the Lich password.
///
/// From the binary (`child::is_password` @ `0x101683420`): the check is
/// exactly 15 ASCII bytes, case-insensitive, and it must end on a UTF-8
/// character boundary before the comparison runs.  Nothing after byte 14
/// (the trailing `_`) is inspected, so `HYDRA_PASSWORD_` on its own matches
/// and any suffix (including an empty one) is accepted.
pub const PASSWORD_PREFIX: &str = "hydra_password_";

/// Returns `true` when `name` begins with `HYDRA_PASSWORD_`, ignoring ASCII
/// case.  This is the predicate used to strip password-carrying variables
/// from the Lich child's environment.
///
/// Recovered exactly from the disassembly:
/// ```text
/// name.len() >= 15 && name.is_char_boundary(15)
///     && name[..15].eq_ignore_ascii_case("hydra_password_")
/// ```
/// The `is_char_boundary` test matters: a 14-byte ASCII prefix followed by a
/// multi-byte character puts a UTF-8 continuation byte at index 15 and must
/// not panic (or match).
#[inline]
pub fn is_password(name: &str) -> bool {
    name.len() >= 15
        && name.is_char_boundary(15)
        && name[..15].eq_ignore_ascii_case(PASSWORD_PREFIX)
}

/// The directory searched for an installed Ruby 4Lich5 on Windows.
pub const RUBY4LICH5_ROOT: &str = r"C:\Ruby4Lich5";

/// Build the base `Command` for the Ruby interpreter.
///
/// Recovered (`child::command` @ `0x101683638`): `Command::new(ruby)`, then
/// iterate `std::env::vars_os()` and call `CommandEnv::remove` for every
/// variable whose name satisfies [`is_password`].  This removes both the
/// password itself and any helper variables (e.g. the token) that share the
/// `HYDRA_PASSWORD_` prefix, so the Lich child cannot read them.
pub fn command(ruby: &Path) -> Command {
    let mut cmd = Command::new(ruby);
    filter_password_env(&mut cmd, std::env::vars_os());
    cmd
}

/// Apply the recovered `HYDRA_PASSWORD_*` removal pass to an existing
/// command using an explicit environment iterator.  Split out so tests do
/// not need to mutate the test process's real environment.
pub fn filter_password_env<I>(cmd: &mut Command, vars: I)
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    for (name, _value) in vars {
        // The binary feeds the raw OsStr bytes to is_password; use the
        // lossy view so non-UTF-8 names never panic (they cannot match the
        // ASCII prefix unless their first 15 bytes are ASCII anyway).
        if is_password(&name.to_string_lossy()) {
            cmd.env_remove(&name);
        }
    }
}

/// Find a Ruby interpreter for Lich.
///
/// Recovered behaviour (`find_ruby` @ `0x101689960`):
/// 1. Read `PATH`; for every entry (in order) join `ruby` and return the
///    first path for which `Path::is_file()` is true.
/// 2. Read `C:\Ruby4Lich5` (unconditionally in the binary; on Unix the
///    `read_dir` simply fails), parse every entry name as a dotted version,
///    sort the `(version-parts, path)` tuples, take the greatest, and return
///    `<dir>\bin\ruby`.
/// 3. Otherwise `None`.
///
/// The original used `std::env::var_os("PATH")` + `env::split_paths`.
/// Kept identical here.
pub fn find_ruby() -> Option<PathBuf> {
    find_ruby_in(std::env::var_os("PATH").as_deref())
}

/// Testable core of [`find_ruby`]: the `PATH` value is supplied by the
/// caller so tests never have to mutate the process environment.
pub fn find_ruby_in(path: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            let candidate = dir.join("ruby");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // UNKNOWN: the binary has no `#[cfg(windows)]` guard here; it always
    // attempts `read_dir(r"C:\Ruby4Lich5")` and falls through when that
    // fails.  We reproduce that literally, which is harmless on Unix.
    if let Some(version_dir) = newest_version_dir(Path::new(RUBY4LICH5_ROOT)) {
        return Some(version_dir.join("bin").join("ruby"));
    }

    None
}

/// Pick the newest `X[.Y[.Z...]]` directory below `root`.
///
/// The binary collects `(Vec<u32>, PathBuf)` for each `read_dir` entry,
/// stable-sorts them and uses the last one.  Entries whose names are not
/// purely numeric-and-dots are dropped (the tuple stores successfully parsed
/// version components only; `// UNKNOWN:` the exact filter closure was
/// inlined, but a `Vec<u32>` cannot represent a parse failure, and the sort
/// key is the version vector).
pub fn newest_version_dir(root: &Path) -> Option<PathBuf> {
    let mut versions: Vec<(Vec<u32>, PathBuf)> = Vec::new();
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let mut parts = Vec::new();
        let mut ok = true;
        for part in name.split('.') {
            match part.parse::<u32>() {
                Ok(n) => parts.push(n),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && !parts.is_empty() {
            versions.push((parts, entry.path()));
        }
    }
    // `PathBuf` is the tie-breaker in the binary's 48-byte tuple ordering.
    versions.sort();
    versions.pop().map(|(_, path)| path)
}

/// Token generation error.
///
/// The original returns `Result<String, getrandom::Error>`; std-only code
/// cannot name that type.  This wrapper carries the same display text and
/// keeps a `source` for callers that want the underlying I/O error.
#[derive(Debug)]
pub struct TokenError {
    message: String,
    source: Option<std::io::Error>,
}

impl TokenError {
    fn new(message: impl Into<String>, source: Option<std::io::Error>) -> Self {
        Self {
            message: message.into(),
            source,
        }
    }
}

impl std::fmt::Display for TokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

/// Generate the random token Lich uses to authenticate the local IPC.
///
/// Recovered (`new_token` @ `0x101689d30`): fill **32 random bytes**, then
/// encode each byte as two **lowercase** hex digits, producing a 64-byte
/// `String`.  On an RNG error the binary formats the `getrandom::Error`
/// with `Display` into `Err`.
///
/// std-only equivalent: read `/dev/urandom`.  If that is unavailable the
/// documented fallback mixes the clock, process id and a stack address
/// through a SplitMix64 stream, which is *not* cryptographic but keeps the
/// relay functional on exotic targets (and is marked UNKNOWN because the
/// binary has no such fallback).
pub fn new_token() -> Result<String, TokenError> {
    let mut bytes = [0u8; 32];
    fill_random(&mut bytes)?;
    Ok(hex_lower(&bytes))
}

/// Hex-encode bytes in lowercase; each input byte yields two characters.
pub fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn fill_random(buf: &mut [u8]) -> Result<(), TokenError> {
    #[cfg(unix)]
    {
        match fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(buf)) {
            Ok(()) => return Ok(()),
            Err(err) => {
                // Fall through to the non-cryptographic fallback, but keep
                // the I/O error in case everything below also fails.
                if fill_fallback(buf).is_ok() {
                    // UNKNOWN: the binary never falls back; it returns Err.
                    return Ok(());
                }
                return Err(TokenError::new(
                    format!("/dev/urandom unavailable: {err}"),
                    Some(err),
                ));
            }
        }
    }

    #[cfg(not(unix))]
    {
        // UNKNOWN: non-Unix std-only fallback (the binary used getrandom).
        fill_fallback(buf).map_err(|err| {
            TokenError::new(format!("no secure randomness source: {err}"), Some(err))
        })
    }
}

fn fill_fallback(buf: &mut [u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let pid = std::process::id() as u64;
    let stack = &seed as *const u64 as u64;
    let mut state = seed
        ^ pid.rotate_left(17)
        ^ stack.rotate_left(33)
        ^ COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);

    for chunk in buf.chunks_mut(8) {
        // SplitMix64
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        let bytes = z.to_le_bytes();
        for (dst, src) in chunk.iter_mut().zip(bytes.iter()) {
            *dst = *src;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn command_removes_password_prefixed_environment_variables() {
        let mut cmd = Command::new("ruby");
        cmd.env("HYDRA_PASSWORD_A", "secret-a");
        cmd.env("hydra_password_B", "secret-b");
        cmd.env("HYDRA_PASSWORD", "fourteen-bytes");
        cmd.env("HYDRA_PASSWORDX", "wrong-prefix");
        cmd.env("KEEP_ME", "kept");

        let vars: Vec<(OsString, OsString)> = vec![
            ("HYDRA_PASSWORD_A".into(), "secret-a".into()),
            ("hydra_password_B".into(), "secret-b".into()),
            ("HYDRA_PASSWORD".into(), "fourteen-bytes".into()),
            ("HYDRA_PASSWORDX".into(), "wrong-prefix".into()),
            ("KEEP_ME".into(), "kept".into()),
        ];
        filter_password_env(&mut cmd, vars);

        let envs: Vec<(String, Option<String>)> = cmd
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();

        // Password-prefixed variables were explicitly removed ...
        for (key, value) in &envs {
            if key.eq_ignore_ascii_case("HYDRA_PASSWORD_A")
                || key.eq_ignore_ascii_case("HYDRA_PASSWORD_B")
            {
                assert_eq!(value, &None, "{key} should have been removed");
            }
        }
        // ... while a 14-byte name and an unrelated name survive.
        assert!(envs.contains(&("HYDRA_PASSWORD".into(), Some("fourteen-bytes".into()))));
        assert!(envs.contains(&("HYDRA_PASSWORDX".into(), Some("wrong-prefix".into()))));
        assert!(envs.contains(&("KEEP_ME".into(), Some("kept".into()))));
    }

    #[test]
    fn command_uses_the_given_ruby_program() {
        let cmd = command(Path::new("/opt/ruby/bin/ruby"));
        assert_eq!(cmd.get_program(), "/opt/ruby/bin/ruby");
    }

    #[test]
    fn is_password_accepts_prefix_and_suffixes() {
        assert!(is_password("HYDRA_PASSWORD_"));
        assert!(is_password("hydra_password_"));
        assert!(is_password("HyDrA_PaSsWoRd_lich"));
        assert!(is_password("HYDRA_PASSWORD_GEMSTONE"));
        assert!(is_password("HYDRA_PASSWORD_1234567890"));
    }

    #[test]
    fn is_password_rejects_short_or_different_names() {
        assert!(!is_password(""));
        assert!(!is_password("hydra"));
        assert!(!is_password("HYDRA_PASSWORD")); // 14 bytes, no trailing `_`
        assert!(!is_password("HYDRA_PASSWORd")); // wrong last byte
        assert!(!is_password("HYDRA_PASSWORDX_suffix"));
        assert!(!is_password("xhydra_password_"));
    }

    #[test]
    fn is_password_handles_utf8_boundary() {
        // Byte 15 is inside `é` (0xc3 0xa9): must not panic and must not match.
        let split = "hydra_password\u{e9}";
        assert!(!split.is_char_boundary(15));
        assert!(!is_password(split));

        // Non-ASCII suffix after a complete ASCII prefix still matches.
        assert!(is_password("HYDRA_PASSWORD_\u{e9}"));
        assert!(is_password("hydra_password_秘密"));
    }

    #[test]
    fn is_password_ignores_everything_past_the_prefix() {
        // The binary only compares bytes 0..15; suffixes are irrelevant,
        // including control characters and invalid-looking content.
        assert!(is_password("hydra_password_\n"));
        assert!(is_password("HYDRA_PASSWORD_ "));
    }

    #[test]
    fn find_ruby_without_path_returns_none_on_unix() {
        // No PATH and (normally) no C:\Ruby4Lich5 => None.
        assert_eq!(find_ruby_in(None), None);
        assert_eq!(find_ruby_in(Some(OsStr::new(""))), None);
    }

    #[test]
    fn find_ruby_finds_first_ruby_on_path() {
        let root = std::env::temp_dir().join(format!(
            "hydra-lich-find-ruby-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let good = root.join("good");
        let also = root.join("also");
        fs::create_dir_all(&good).unwrap();
        fs::create_dir_all(&also).unwrap();
        let ruby = good.join("ruby");
        let mut f = fs::File::create(&ruby).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        drop(f);

        // `also` comes first and has no ruby; `good` is found second.
        let path = std::env::join_paths([&also, &good]).unwrap();
        assert_eq!(find_ruby_in(Some(path.as_os_str())), Some(ruby));

        // A path with only `also` yields no hit (and no Windows install).
        let path = std::env::join_paths([&also]).unwrap();
        assert_eq!(find_ruby_in(Some(path.as_os_str())), None);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_ruby_prefers_path_over_version_dir() {
        let root = std::env::temp_dir().join(format!(
            "hydra-lich-path-first-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let ruby = root.join("ruby");
        fs::File::create(&ruby).unwrap();
        let path = std::env::join_paths([&root]).unwrap();
        // On Unix `C:\Ruby4Lich5` is a single odd file name and does not
        // exist, so PATH always wins.
        assert_eq!(find_ruby_in(Some(path.as_os_str())), Some(ruby));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn newest_version_dir_sorts_numerically_not_lexically() {
        let root = std::env::temp_dir().join(format!(
            "hydra-lich-versions-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for name in ["3.2.2", "3.10.1", "3.9.9", "not-a-version", "4"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        let picked = newest_version_dir(&root).unwrap();
        assert_eq!(picked.file_name().unwrap(), "4");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn newest_version_dir_compares_dotted_components() {
        let root = std::env::temp_dir().join(format!(
            "hydra-lich-versions2-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for name in ["3.2.2", "3.2.10", "3.2.9"] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        let picked = newest_version_dir(&root).unwrap();
        assert_eq!(picked.file_name().unwrap(), "3.2.10");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn newest_version_dir_missing_root_is_none() {
        assert_eq!(newest_version_dir(Path::new("/definitely/not/here")), None);
    }

    #[test]
    fn new_token_is_64_lowercase_hex_chars() {
        let token = new_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(
            token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "unexpected token: {token}"
        );
        assert_eq!(token, token.to_ascii_lowercase());
    }

    #[test]
    fn new_token_differs_between_calls() {
        // 32 random bytes; the probability of a collision is 2^-256.  This
        // is a real randomness assertion, not a tautology.
        let a = new_token().unwrap();
        let b = new_token().unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn hex_lower_matches_known_encoding() {
        assert_eq!(hex_lower(&[]), "");
        assert_eq!(hex_lower(&[0x00, 0x01, 0x0f, 0x10, 0xab, 0xff]), "00010f10abff");
        // 32 bytes -> exactly 64 chars.
        assert_eq!(hex_lower(&[0xde; 32]).len(), 64);
        assert_eq!(hex_lower(&[0xde; 32]), "dede".repeat(16));
    }
}
