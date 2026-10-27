//! Checksums and their signature.
//!
//! Two steps guard a release download:
//!
//! * `Sums::checked` reads `SHA256SUMS.txt` and `SHA256SUMS.txt.minisig`, checks
//!   the minisign signature over the checksum text with the release key, checks
//!   that the signature's *trusted comment* names this version, and then parses
//!   the text into `(file name, sha256 hex)` pairs;
//! * `Sums::check` finds the archive's name in that list and compares the
//!   SHA-256 of the downloaded bytes with the listed digest.
//!
//! Everything except the curve arithmetic is recovered in full:
//!
//! | what | evidence |
//! |---|---|
//! | `Sums { list: Vec<(String, String)> }` (48-byte entries, name then digest) | `Sums::check` @0x1010a442c walks `self+0x8`/`self+0x10` by 0x30 comparing `entry.name.len` |
//! | SHA-256 of the candidate bytes, lowercase hex | `sha2::sha256::compress256` calls, `String::from_iter::<char>` @0x1010a4644 |
//! | `"The release's checksums do not list {}"` / `"{} is not the file the release signed: its hash differs"` | `__cstring` 0x103e5ec16 / 0x103e5ebde |
//! | key parse → sig parse → verify → trusted comment → parse | `Sums::checked` @0x1010a46bc (0x1010a4708, 0x1010a47c8, 0x1010a48d0, 0x1010a4918, 0x1010a4a5c) |
//! | `"Hydra {}"` is the expected trusted comment | `format!` at 0x1010a48ec with arg `(x4, x5)` |
//! | `split_once(' ')`, then one `' '` or `'*'` marker, then `lines()` with `\r\n` handling, digest lowercased | the filter closure @0x1010a4b48 and `FilterMap::next` @0x1010a80f4 |
//!
//! The signature itself is Ed25519 over BLAKE2b-512 (minisign 0.3.0).  Neither
//! BLAKE2b nor the curve is reproduced here, and `Sums::checked` **never**
//! reports success without a `Verifier`: the built-in one is
//! [`Unavailable`], which always fails, so an unchecked release cannot be
//! installed by accident.

use crate::sha256;

/// A minisign public key: `<algorithm:2><key id:8><ed25519 public key:32>`,
/// base64 encoded — 56 characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey {
    pub algorithm: [u8; 2],
    pub key_id: [u8; 8],
    pub key: [u8; 32],
}

/// A decoded minisign signature file (`.minisig`): the signature over the data,
/// plus the trusted comment and the signature over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub algorithm: [u8; 2],
    pub key_id: [u8; 8],
    pub signature: [u8; 64],
    /// The text after `trusted comment: ` — for a Hydra release,
    /// `Hydra 0.1.0-beta.7`.
    pub trusted_comment: String,
    pub global_signature: [u8; 64],
}

/// minisign's two algorithm identifiers.  `Ed` is the legacy (data, not
/// prehashed) form; `ED` is the default.
pub const ALGORITHM: [u8; 2] = *b"ED";
pub const LEGACY_ALGORITHM: [u8; 2] = *b"Ed";

/// The seam where the signature arithmetic would go.
///
/// `Sums::checked` calls [`Verifier::verify`] only after the key and the
/// signature parse and the algorithm and key id agree, so an implementation
/// only has to do the crypto:
///
/// * prehashed signatures (`ED`) are Ed25519 over BLAKE2b-512 of the data,
/// * legacy signatures (`Ed`) are Ed25519 over the data itself,
/// * and the trusted comment is covered by [`Signature::global_signature`]
///   over `signature ‖ trusted_comment`.
pub trait Verifier {
    /// `Ok(())` only when the signature is genuine.
    fn verify(
        &self,
        key: &PublicKey,
        signature: &Signature,
        data: &[u8],
    ) -> Result<(), String>;
}

/// The only verifier that ships: it refuses everything.
///
/// `// UNKNOWN:` the curve25519 arithmetic was not reconstructed, so there is
/// deliberately no "best effort" success path here — a release whose signature
/// cannot be checked is a release that is not installed.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unavailable;

impl Verifier for Unavailable {
    fn verify(&self, _: &PublicKey, _: &Signature, _: &[u8]) -> Result<(), String> {
        Err("the signature cannot be checked: this build of Hydra has no \
             Ed25519 verifier, so no release can be proven genuine"
            .to_string())
    }
}

/// A release's checksum list: `(file name, lowercase SHA-256 hex)` pairs, in the
/// order the file listed them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sums {
    entries: Vec<(String, String)>,
}

impl Sums {
    /// Verify one download against the list.
    ///
    /// `check` @0x1010a442c: walk the entries, match the name exactly (length
    /// then bytes), hash `body` with SHA-256, and compare the lowercase hex with
    /// the listed digest.
    pub fn check(&self, name: &str, body: &[u8]) -> Result<(), String> {
        let entry = self
            .entries
            .iter()
            .find(|(listed, _)| listed == name)
            .ok_or_else(|| format!("The release's checksums do not list {name}"))?;
        let digest = sha256::hex(&sha256::digest(body));
        if digest == entry.1 {
            Ok(())
        } else {
            Err(format!(
                "{name} is not the file the release signed: its hash differs"
            ))
        }
    }

    /// Read a checksum list, checking its signature first.
    ///
    /// `checked` @0x1010a46bc, in the binary's order:
    ///
    /// 1. `PublicKey::from_base64` (0x1010a4708) →
    ///    `"The release key cannot be read: {}"`,
    /// 2. `Signature::decode` (0x1010a47c8) →
    ///    `"The checksums' signature cannot be read: {}"`,
    /// 3. `PublicKey::verify(data, sig, allow_legacy = false)` (0x1010a48d0) →
    ///    `"The checksums' signature does not hold: {}"`,
    /// 4. `Signature::trusted_comment()` against `format!("Hydra {}", version)`
    ///    (0x1010a4900) → `"The checksums are signed for \"{}\", not {}"`,
    /// 5. parse the text into the list (0x1010a4a5c).
    pub fn checked(
        sums: &[u8],
        signature: &str,
        version: &str,
        key: &str,
    ) -> Result<Sums, String> {
        Sums::checked_with(&Unavailable, sums, signature, version, key)
    }

    /// [`Sums::checked`] with an explicit [`Verifier`] — the entry point a build
    /// with the curve arithmetic (or a test) uses.
    pub fn checked_with<V: Verifier + ?Sized>(
        verifier: &V,
        sums: &[u8],
        signature: &str,
        version: &str,
        key: &str,
    ) -> Result<Sums, String> {
        let key = decode_public_key(key)
            .map_err(|e| format!("The release key cannot be read: {e}"))?;
        let signature = decode_signature(signature)
            .map_err(|e| format!("The checksums' signature cannot be read: {e}"))?;
        // What minisign_verify::PublicKey::verify checks before the math.  The
        // call site passes `allow_legacy = false` (`mov w4, #0x0` at
        // 0x1010a48cc), so a legacy `Ed` signature is refused outright.
        if signature.algorithm != ALGORITHM {
            return Err("The checksums' signature does not hold: \
                        This signature algorithm is not supported by this implementation"
                .to_string());
        }
        if signature.key_id != key.key_id {
            return Err("The checksums' signature does not hold: \
                        The signature was created with a different key than the one provided"
                .to_string());
        }
        verifier
            .verify(&key, &signature, sums)
            .map_err(|e| format!("The checksums' signature does not hold: {e}"))?;
        let expected = format!("Hydra {version}");
        if signature.trusted_comment != expected {
            return Err(format!(
                "The checksums are signed for \"{}\", not {}",
                signature.trusted_comment, expected
            ));
        }
        Ok(Sums::parse(sums))
    }

    /// Parse a `sha256sum`-style list.
    ///
    /// The filter closure @0x1010a4b48 splits each line at the first space,
    /// skips one `' '` or `'*'` binary marker, keeps the rest as the file name
    /// and the part before the space as the digest, and lowercases the digest
    /// with a vectorised ASCII fold (0x1010a4dc0–0x1010a4ea0).  Lines without a
    /// space, or where the marker is missing, are dropped.
    pub fn parse(text: &[u8]) -> Sums {
        let text = String::from_utf8_lossy(text);
        let mut entries = Vec::new();
        for line in text.lines() {
            let Some((digest, rest)) = line.split_once(' ') else {
                continue;
            };
            // One `' '` or `'*'` binary marker follows the first space.
            let mut chars = rest.chars();
            let name = match chars.next() {
                Some(' ') | Some('*') => chars.as_str(),
                _ => continue,
            };
            entries.push((name.to_string(), digest.to_ascii_lowercase()));
        }
        Sums { entries }
    }

    /// The parsed pairs, `(file name, digest)`.
    pub fn list(&self) -> &[(String, String)] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `minisign_verify::PublicKey::from_base64`.
pub fn decode_public_key(text: &str) -> Result<PublicKey, String> {
    let raw = base64(text).ok_or_else(|| "Invalid encoding in minisign data".to_string())?;
    if raw.len() != 42 {
        return Err("Invalid encoding in minisign data".to_string());
    }
    let mut algorithm = [0u8; 2];
    algorithm.copy_from_slice(&raw[..2]);
    let mut key_id = [0u8; 8];
    key_id.copy_from_slice(&raw[2..10]);
    let mut key = [0u8; 32];
    key.copy_from_slice(&raw[10..42]);
    if algorithm != ALGORITHM && algorithm != LEGACY_ALGORITHM {
        return Err("This signature algorithm is not supported by this implementation".to_string());
    }
    Ok(PublicKey { algorithm, key_id, key })
}

/// `minisign_verify::Signature::decode` — the four-line `.minisig` file:
/// untrusted comment, base64 signature (74 bytes), trusted comment, base64
/// global signature (64 bytes).
///
/// Unlike `minisign-verify` (which treats the last two lines as optional) this
/// requires all four: Hydra's `checked` compares the trusted comment, so a file
/// without one could never satisfy it, and refusing it here says so plainly.
pub fn decode_signature(text: &str) -> Result<Signature, String> {
    let mut lines = text.lines();
    let next = |line: Option<&str>| -> Result<String, String> {
        line.map(str::to_string)
            .ok_or_else(|| "Invalid encoding in minisign data".to_string())
    };
    let _untrusted = next(lines.next())?;
    let encoded = next(lines.next())?;
    let trusted = next(lines.next())?;
    let global = next(lines.next())?;

    let raw = base64(&encoded).ok_or_else(|| "Invalid encoding in minisign data".to_string())?;
    if raw.len() != 74 {
        return Err("Invalid encoding in minisign data".to_string());
    }
    let mut algorithm = [0u8; 2];
    algorithm.copy_from_slice(&raw[..2]);
    let mut key_id = [0u8; 8];
    key_id.copy_from_slice(&raw[2..10]);
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&raw[10..74]);

    let global_raw =
        base64(&global).ok_or_else(|| "Invalid encoding in minisign data".to_string())?;
    if global_raw.len() != 64 {
        return Err("Invalid encoding in minisign data".to_string());
    }
    let mut global_signature = [0u8; 64];
    global_signature.copy_from_slice(&global_raw);

    let trusted_comment = trusted
        .strip_prefix("trusted comment: ")
        .ok_or_else(|| "Invalid encoding in minisign data".to_string())?
        .to_string();

    Ok(Signature { algorithm, key_id, signature, trusted_comment, global_signature })
}

/// Standard base64 with `=` padding, as minisign writes it.
pub fn base64(text: &str) -> Option<Vec<u8>> {
    const INVALID: u8 = 0xff;
    let mut table = [INVALID; 256];
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for (i, byte) in alphabet.iter().enumerate() {
        table[*byte as usize] = i as u8;
    }
    let text = text.trim();
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut values = [0u8; 4];
        let mut padding = 0usize;
        for (i, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                // Only the last two positions may be padding.
                if i < 2 {
                    return None;
                }
                padding += 1;
                values[i] = 0;
            } else {
                if padding > 0 {
                    return None; // data after padding
                }
                let value = table[*byte as usize];
                if value == INVALID {
                    return None;
                }
                values[i] = value;
            }
        }
        let triple = ((values[0] as u32) << 18)
            | ((values[1] as u32) << 12)
            | ((values[2] as u32) << 6)
            | values[3] as u32;
        out.push((triple >> 16) as u8);
        if padding < 2 {
            out.push((triple >> 8) as u8);
        }
        if padding < 1 {
            out.push(triple as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The release key, from `__const` 0x1020701b8 (56 characters, loaded at
    /// 0x10025e410 with length 0x38: `Ed` + key id `e73f8c729a034284` + 32 bytes).
    const KEY: &str = "RWTnP4xymgNChF6vPB59oTv8tc6coKg6Svl9A5P0MtDOB/Enpl70UtLc";

    /// A checksum list exactly as `sha256sum` writes it.
    const SUMS: &str = "\
9e6a15d5b1c0f3a7d27c1c1b0ff4f9a5f36b0f8c3a90f6e3e6d5f5c8dd0e0f11  hydra-arm64
47ae559b12c5756ca438e050dca825201238c2283f17eaf037f7f2648a73c5d8  notes.txt
";

    #[test]
    fn parses_a_checksum_list() {
        let sums = Sums::parse(SUMS.as_bytes());
        assert_eq!(sums.list().len(), 2);
        assert_eq!(sums.list()[0].0, "hydra-arm64");
        assert_eq!(
            sums.list()[0].1,
            "9e6a15d5b1c0f3a7d27c1c1b0ff4f9a5f36b0f8c3a90f6e3e6d5f5c8dd0e0f11"
        );
        assert_eq!(sums.list()[1].0, "notes.txt");
    }

    #[test]
    fn parses_the_binary_marker_and_crlf() {
        let text = "47ae559b12c5756ca438e050dca825201238c2283f17eaf037f7f2648a73c5d8 *hydra\r\n";
        let sums = Sums::parse(text.as_bytes());
        assert_eq!(sums.list(), [(
            "hydra".to_string(),
            "47ae559b12c5756ca438e050dca825201238c2283f17eaf037f7f2648a73c5d8".to_string()
        )]);
    }

    #[test]
    fn digests_are_lowercased_when_read() {
        let text = "47AE559B12C5756CA438E050DCA825201238C2283F17EAF037F7F2648A73C5D8  hydra\n";
        let sums = Sums::parse(text.as_bytes());
        assert_eq!(
            sums.list()[0].1,
            "47ae559b12c5756ca438e050dca825201238c2283f17eaf037f7f2648a73c5d8"
        );
    }

    #[test]
    fn lines_without_a_space_or_marker_are_dropped() {
        let sums = Sums::parse(b"no-space-here\nabc  fine\nhash-only \nx\tname\n");
        // Only `abc  fine` survives: "hash-only " has no name left, "x\tname"
        // has no space at all.
        assert_eq!(sums.list(), [("fine".to_string(), "abc".to_string())]);
    }

    #[test]
    fn a_matching_checksum_passes() {
        let sums = Sums::parse(SUMS.as_bytes());
        // The listed digest of `notes.txt` is the SHA-256 of these bytes.
        sums.check("notes.txt", b"Hydra 0.1.0").expect("matches");
    }

    #[test]
    fn a_mismatched_checksum_is_refused() {
        let sums = Sums::parse(SUMS.as_bytes());
        let error = sums.check("notes.txt", b"tampered").unwrap_err();
        assert_eq!(
            error,
            "notes.txt is not the file the release signed: its hash differs"
        );
    }

    #[test]
    fn a_file_the_list_does_not_name_is_refused() {
        let sums = Sums::parse(SUMS.as_bytes());
        let error = sums.check("hydra-arm64.zip", b"").unwrap_err();
        assert_eq!(
            error,
            "The release's checksums do not list hydra-arm64.zip"
        );
    }

    #[test]
    fn reads_the_release_key() {
        let key = decode_public_key(KEY).expect("decodes");
        assert_eq!(key.algorithm, *b"Ed");
        assert_eq!(key.key.len(), 32);
        // 42 raw bytes out of 56 base64 characters.
        assert_eq!(base64(KEY).map(|raw| raw.len()), Some(42));
        assert_eq!(&base64(KEY).unwrap()[..2], b"Ed");
    }

    #[test]
    fn refuses_malformed_keys() {
        for bad in ["", "not base64!", "AAAA", &"A".repeat(55)] {
            let error = decode_public_key(bad).unwrap_err();
            assert!(!error.is_empty(), "{bad:?}");
        }
    }

    #[test]
    fn reads_a_signature_file() {
        // A structurally valid .minisig: two comments, a 74-byte signature and a
        // 64-byte global signature.
        let text = format!(
            "untrusted comment: signature from minisign secret key\n{}\ntrusted comment: Hydra 0.1.0-beta.9\n{}\n",
            encode(&[b'E', b'D', 1, 2, 3, 4, 5, 6, 7, 8].iter().copied().chain(std::iter::repeat_n(0x11, 64)).collect::<Vec<_>>()),
            encode(&[0x22u8; 64]),
        );
        let signature = decode_signature(&text).expect("decodes");
        assert_eq!(signature.algorithm, *b"ED");
        assert_eq!(signature.key_id, [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(signature.signature, [0x11; 64]);
        assert_eq!(signature.trusted_comment, "Hydra 0.1.0-beta.9");
        assert_eq!(signature.global_signature, [0x22; 64]);
    }

    #[test]
    fn refuses_malformed_signatures() {
        assert_eq!(
            decode_signature("too\nshort\n"),
            Err("Invalid encoding in minisign data".to_string())
        );
        let text = "untrusted comment: x\nAAAA\ntrusted comment: y\nAAAA\n";
        assert!(decode_signature(text).is_err());
        let text = format!(
            "untrusted comment: x\n{}\nno trusted comment\n{}\n",
            encode(&[b'E'; 74]),
            encode(&[b'G'; 64])
        );
        assert!(decode_signature(&text).is_err());
    }

    /// `checked` without a verifier must never accept a release.
    #[test]
    fn checking_needs_a_verifier() {
        let text = format!(
            "untrusted comment: signature from minisign secret key\n{}\ntrusted comment: Hydra 0.1.0-beta.7\n{}\n",
            encode(&signature_bytes()),
            encode(&[0x22u8; 64]),
        );
        let error = Sums::checked(SUMS.as_bytes(), &text, "0.1.0-beta.7", KEY).unwrap_err();
        assert!(
            error.starts_with("The checksums' signature does not hold: "),
            "{error}"
        );
        assert!(error.contains("Ed25519"), "{error}");
    }

    /// With a verifier that agrees, the rest of `checked` runs: the trusted
    /// comment must name the version, then the list is parsed.
    #[test]
    fn a_stub_verifier_runs_the_rest() {
        struct Trusting;
        impl Verifier for Trusting {
            fn verify(&self, key: &PublicKey, sig: &Signature, data: &[u8]) -> Result<(), String> {
                assert_eq!(key.key_id, sig.key_id);
                assert_eq!(data, SUMS.as_bytes());
                Ok(())
            }
        }
        let key = decode_public_key(KEY).unwrap();
        let text = format!(
            "untrusted comment: signature from minisign secret key\n{}\ntrusted comment: Hydra 0.1.0-beta.7\n{}\n",
            encode(&signature_bytes_with_key(key.key_id)),
            encode(&[0x22u8; 64]),
        );
        let sums = Sums::checked_with(&Trusting, SUMS.as_bytes(), &text, "0.1.0-beta.7", KEY)
            .expect("accepts");
        assert_eq!(sums.list().len(), 2);
        sums.check("notes.txt", b"Hydra 0.1.0").expect("matches");

        // The trusted comment names another version: refused.
        let error =
            Sums::checked_with(&Trusting, SUMS.as_bytes(), &text, "0.1.0-beta.8", KEY).unwrap_err();
        assert_eq!(
            error,
            "The checksums are signed for \"Hydra 0.1.0-beta.7\", not Hydra 0.1.0-beta.8"
        );
    }

    #[test]
    fn a_signature_from_another_key_is_refused() {
        struct Trusting;
        impl Verifier for Trusting {
            fn verify(&self, _: &PublicKey, _: &Signature, _: &[u8]) -> Result<(), String> {
                panic!("must not be reached: the key ids differ");
            }
        }
        let text = format!(
            "untrusted comment: x\n{}\ntrusted comment: Hydra 0.1.0-beta.7\n{}\n",
            encode(&signature_bytes_with_key([9; 8])),
            encode(&[0x22u8; 64]),
        );
        let error = Sums::checked_with(&Trusting, SUMS.as_bytes(), &text, "0.1.0-beta.7", KEY)
            .unwrap_err();
        assert!(error.contains("different key"), "{error}");
    }

    #[test]
    fn a_legacy_signature_is_refused() {
        struct Trusting;
        impl Verifier for Trusting {
            fn verify(&self, _: &PublicKey, _: &Signature, _: &[u8]) -> Result<(), String> {
                Ok(())
            }
        }
        let mut raw = signature_bytes();
        raw[0] = b'E';
        raw[1] = b'd';
        let text = format!(
            "untrusted comment: x\n{}\ntrusted comment: Hydra 0.1.0-beta.7\n{}\n",
            encode(&raw),
            encode(&[0u8; 64]),
        );
        let error = Sums::checked_with(&Trusting, SUMS.as_bytes(), &text, "0.1.0-beta.7", KEY)
            .unwrap_err();
        assert!(error.contains("not supported"), "{error}");
    }

    #[test]
    fn a_bad_key_is_reported_as_such() {
        let error = Sums::checked(SUMS.as_bytes(), "", "", "nonsense").unwrap_err();
        assert_eq!(error, "The release key cannot be read: Invalid encoding in minisign data");
    }

    #[test]
    fn base64_round_trips() {
        for bytes in [vec![], vec![1], vec![1, 2], vec![1, 2, 3], vec![0xff; 42]] {
            let encoded = encode(&bytes);
            assert_eq!(base64(&encoded), Some(bytes));
        }
        assert_eq!(base64("AAA"), None);
        assert_eq!(base64("A==="), None);
        assert_eq!(base64("AA=A"), None);
        assert_eq!(base64("*AAA"), None);
    }

    fn signature_bytes() -> Vec<u8> {
        signature_bytes_with_key(decode_public_key(KEY).unwrap().key_id)
    }

    fn signature_bytes_with_key(key_id: [u8; 8]) -> Vec<u8> {
        let mut raw = vec![b'E', b'D'];
        raw.extend_from_slice(&key_id);
        raw.extend(std::iter::repeat_n(0x11, 64));
        raw
    }

    fn encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let triple = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
            out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
            if chunk.len() > 1 {
                out.push(ALPHABET[(triple >> 6) as usize & 0x3f] as char);
            } else {
                out.push('=');
            }
            if chunk.len() > 2 {
                out.push(ALPHABET[triple as usize & 0x3f] as char);
            } else {
                out.push('=');
            }
        }
        out
    }
}
