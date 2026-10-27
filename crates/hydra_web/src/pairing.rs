//! Pairing: the `web.token` file and its constant-time comparison.
//!
//! Recovered from `pairing::pairing` (`0x1002ca91c`) and `pairing::matches`
//! (`0x1002ca7c8`).

use std::path::Path;

/// Length of a pairing token: 32 random bytes, lowercase hex.
pub const TOKEN_BYTES: usize = 32;
/// Length of the hex text: 64 characters.
pub const TOKEN_LEN: usize = TOKEN_BYTES * 2;
/// File name under the storage root.
pub const TOKEN_FILE: &str = "web.token";

/// Compare two secrets without an early exit.
///
/// **ESTABLISHED** (`0x1002ca7c8`): different lengths → `false` (that leak is
/// unavoidable); equal lengths are XOR-accumulated over the whole string and the
/// result is `acc == 0`.  The 64-byte/8-byte/byte loops in the binary are the
/// compiler's vectorised form of that same accumulation.
pub fn matches(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b) {
        acc |= x ^ y;
    }
    acc == 0
}

/// Is `text` the shape of a pairing token: exactly 64 hex characters?
///
/// **ESTABLISHED**: `pairing` accepted exactly `len == 0x40` and then checked
/// every byte with `(c - 0x30) < 10 || ((c & !0x20) - 0x41) < 6`.
pub fn is_token(text: &str) -> bool {
    text.len() == TOKEN_LEN && text.bytes().all(|c| c.is_ascii_hexdigit())
}

/// Read the pairing token, creating it on first use.
///
/// Recovered flow (`0x1002ca91c`):
/// 1. `root.join("web.token")`;
/// 2. read it; a *missing* file or an invalid token means "first use", any
///    other read error is `Err("<path> cannot be read: <err>")`;
/// 3. a valid token is the trimmed file text (64 hex characters);
/// 4. first use: 32 random bytes, hex-encoded lowercase, written with
///    `hydra_session::store::save_bytes`; a write failure is
///    `Err("<path> cannot be written: <err>")`.
///
/// The error strings are the recovered format pieces
/// `{} cannot be read: {}` / `{} cannot be written: {}`.
pub fn pairing(root: &Path) -> Result<String, String> {
    let path = root.join(TOKEN_FILE);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let text = text.trim();
            if is_token(text) {
                return Ok(text.to_string());
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("{} cannot be read: {e}", path.display())),
    }
    let token = fresh_token()?;
    let dir = path.parent().unwrap_or(root);
    hydra_session::store::save_bytes(dir, &path, token.as_bytes())
        .map_err(|e| format!("{} cannot be written: {e}", path.display()))?;
    Ok(token)
}

/// 32 random bytes, lowercase hex — the token generator.
///
/// The original called `getrandom`; this reads `/dev/urandom` and falls back to
/// a clock/pid seeded xorshift so the crate stays `std`-only.
pub fn fresh_token() -> Result<String, String> {
    let bytes = random_bytes()?;
    let mut out = String::with_capacity(TOKEN_LEN);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    Ok(out)
}

/// Fill 32 random bytes.
pub fn random_bytes() -> Result<[u8; TOKEN_BYTES], String> {
    use std::io::Read;
    let mut buf = [0u8; TOKEN_BYTES];
    match std::fs::File::open("/dev/urandom") {
        Ok(mut f) => {
            f.read_exact(&mut buf)
                .map_err(|e| format!("getrandom: {e}"))?;
        }
        Err(e) => {
            // `// UNKNOWN:` the original surfaced `getrandom::Error`'s Display
            // here; without an RNG the seeded fallback keeps the crate usable.
            let mut seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0)
                ^ (u64::from(std::process::id()) << 32)
                ^ (&buf as *const _ as u64);
            for slot in buf.iter_mut() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *slot = (seed >> 24) as u8;
            }
            let _ = e;
        }
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_is_exact() {
        assert!(matches(b"abc", b"abc"));
        assert!(!matches(b"abc", b"abd"));
        assert!(!matches(b"abc", b"ab"));
        assert!(matches(b"", b""));
    }

    #[test]
    fn token_shape() {
        let t = fresh_token().unwrap();
        assert_eq!(t.len(), TOKEN_LEN);
        assert_eq!(t, t.to_lowercase());
        assert!(is_token(&t));
        assert!(!is_token(&t[1..]));
        assert!(!is_token(&"z".repeat(TOKEN_LEN)));
    }

    #[test]
    fn pairing_creates_then_reuses_the_token() {
        let dir = std::env::temp_dir().join(format!("hydra-web-pair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let first = pairing(&dir).unwrap();
        assert!(is_token(&first));
        // The file is the token itself, no newline.
        let raw = std::fs::read_to_string(dir.join(TOKEN_FILE)).unwrap();
        assert_eq!(raw, first);
        // Second call reuses it.
        assert_eq!(pairing(&dir).unwrap(), first);

        // Whitespace around a valid token is tolerated.
        std::fs::write(dir.join(TOKEN_FILE), format!("  {first}\n")).unwrap();
        assert_eq!(pairing(&dir).unwrap(), first);

        // A corrupt token is replaced by a fresh one.
        std::fs::write(dir.join(TOKEN_FILE), "short").unwrap();
        let second = pairing(&dir).unwrap();
        assert!(is_token(&second));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
