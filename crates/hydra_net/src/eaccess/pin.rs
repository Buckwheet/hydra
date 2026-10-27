//! Certificate pinning for the eaccess TLS connection.
//!
//! Recovered from `hydra_net::eaccess::pin` (`0x101c53724` .. `0x101c546e4`).
//! The pin file is a PEM certificate; a login is only allowed over a certificate
//! that matches it.  A missing pin file is *first use*: the presented
//! certificate is recorded and later logins must match it.  There is no
//! "trust this time" path — an unreadable or unpinnable pin refuses the login.

use base64::engine::general_purpose::STANDARD;
use base64::{DecodeError, Engine as _};

use std::path::{Path, PathBuf};

/// The literal that opens a PEM certificate block (the module's literals sit in
/// `__const` at `0x103bf9d38`).
pub const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
/// The literal that closes a PEM certificate block.
pub const END: &str = "-----END CERTIFICATE-----";

/// Verify a presented certificate against the pin stored at `pin_path`.
///
/// Recovered from `0x101c53724`.  Every failure carries one of the recovered
/// messages; `Ok(())` means the login may proceed.
pub fn verify(pin_path: &Path, presented_der: &[u8]) -> Result<(), String> {
    let text = match std::fs::read_to_string(pin_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return record_first_use(pin_path, presented_der);
        }
        Err(error) => {
            return Err(format!(
                "the certificate pin {} exists but could not be read: {}. \
                 Refusing to log in unpinned.",
                pin_path.display(),
                error
            ));
        }
    };
    let pinned = pem_decode(&text).map_err(|why| {
        format!(
            "the certificate pin {} is not a PEM certificate ({}). Refusing to log in \
             unpinned. Delete {} to trust the certificate the server presents on the next \
             login.",
            pin_path.display(),
            why,
            pin_path.display()
        )
    })?;
    if pinned == presented_der {
        return Ok(());
    }
    Err(format!(
        "the eaccess server's certificate CHANGED: it does not match the one pinned in {}. \
         Refusing to send the password over this connection -- an unexpected certificate is \
         what a man-in-the-middle looks like. If Simutronics rotated the certificate, delete \
         {} and log in again to pin the new one.",
        pin_path.display(),
        pin_path.display()
    ))
}

/// Record the first certificate seen and treat it as the pin.
fn record_first_use(pin_path: &Path, presented_der: &[u8]) -> Result<(), String> {
    // The original writes `{}.{}.tmp` beside the pin and renames it into place
    // (literal at `0x103e7f8d3`), so an interrupted write cannot leave half a
    // pin behind.
    let tmp = sibling(pin_path, ".tmp");
    write_atomically(pin_path, &tmp, pem_encode(presented_der).as_bytes()).map_err(|error| {
        format!(
            "no certificate was pinned and one could not be recorded at {}: {}. Refusing to \
             log in: without a stored pin every login would be a first use, which is no pin \
             at all.",
            pin_path.display(),
            error
        )
    })
}

/// `path` with `suffix` appended to the file name, as the original's
/// `{}.{}` + `{}` formatting builds it.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

fn write_atomically(path: &Path, tmp: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    match std::fs::rename(tmp, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(tmp);
            Err(error)
        }
    }
}

/// Encode DER bytes as a PEM certificate block.
pub fn pem_encode(der: &[u8]) -> String {
    let body = base64_encode(der);
    let mut out = String::with_capacity(body.len() + 64);
    out.push_str(BEGIN);
    out.push('\n');
    for chunk in body.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push('\n');
    }
    out.push_str(END);
    out.push('\n');
    out
}

/// Decode the DER bytes out of the first PEM certificate block in `text`.
///
/// The error strings are the recovered literals: ``no `...` line``, `the
/// certificate block is empty`, `invalid base64: ...`.
pub fn pem_decode(text: &str) -> Result<Vec<u8>, String> {
    let begin = text.find(BEGIN).ok_or_else(|| format!("no `{BEGIN}` line"))?;
    let rest = &text[begin + BEGIN.len()..];
    let end = rest.find(END).ok_or_else(|| format!("no `{END}` line"))?;
    let body: String = rest[..end].chars().filter(|c| !c.is_whitespace()).collect();
    if body.is_empty() {
        return Err("the certificate block is empty".to_string());
    }
    base64_decode(&body).map_err(|why| format!("invalid base64: {why}"))
}

/// Standard (padded) base64, as `base64-0.22`'s default engine produces.
///
/// The original called `base64`'s default engine — the `+/` alphabet with `=`
/// padding — so this is that engine (`general_purpose::STANDARD`) and not a
/// reimplementation.
pub fn base64_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Standard (padded) base64 decode.
///
/// Whitespace is dropped before decoding: PEM bodies arrive wrapped in newlines,
/// and the recovered `pem_decode` hands the block over with its line breaks
/// intact.  Everything else — a byte outside the alphabet, an impossible
/// length, mis-placed padding — is `base64-0.22`'s own rejection, and its
/// `DecodeError` `Display` is what fills the recovered `invalid base64: {}`.
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    STANDARD
        .decode(bytes)
        .map_err(|error: DecodeError| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("hydra-net-pin-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("eaccess.pem")
    }

    #[test]
    fn base64_round_trip() {
        for probe in [&b""[..], b"a", b"ab", b"abc", b"abcd", b"\x00\xff\x10"] {
            let encoded = base64_encode(probe);
            assert_eq!(base64_decode(&encoded).unwrap(), probe, "{encoded}");
        }
        assert_eq!(
            base64_encode(b"any carnal pleasure."),
            "YW55IGNhcm5hbCBwbGVhc3VyZS4="
        );
        assert_eq!(
            base64_decode("YW55IGNhcm5hbCBwbGVhc3VyZS4=").unwrap(),
            b"any carnal pleasure."
        );
        assert!(base64_decode("!!!!").is_err());
        assert!(base64_decode("abc").is_err());
    }

    #[test]
    fn pem_round_trip() {
        let der = vec![0x30u8, 0x82, 0x01, 0x02, 0xff, 0x00];
        let pem = pem_encode(&der);
        assert!(pem.starts_with(BEGIN));
        assert!(pem.trim_end().ends_with(END));
        assert_eq!(pem_decode(&pem).unwrap(), der);
    }

    #[test]
    fn the_base64_engine_is_the_standard_one() {
        // The standard alphabet (`+/`), not the URL-safe one (`-_`).
        assert_eq!(base64_encode(&[0xfb, 0xff]), "+/8=");
        assert!(base64_decode("-_8=").is_err());

        // Padding is required and canonical: `base64-0.22`'s default engine.
        assert_eq!(base64_decode("YQ==").unwrap(), b"a");
        assert!(base64_decode("YQ").is_err());
        assert!(base64_decode("YQ=").is_err());
        assert!(base64_decode("YQ===").is_err());
        assert!(base64_decode("YQ==YQ==").is_err());

        // Every byte value survives the trip.
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_decode(&base64_encode(&all)).unwrap(), all);

        // Whitespace is dropped before decoding, which is what makes a wrapped
        // PEM body readable at all.
        assert_eq!(
            base64_decode("YW55IGNh\ncm5hbCBwbGVhc3Vy\nZS4=").unwrap(),
            b"any carnal pleasure."
        );
    }

    #[test]
    fn a_pem_block_wraps_at_sixty_four_columns() {
        let der: Vec<u8> = (0..200u8).collect();
        let pem = pem_encode(&der);
        let body: Vec<&str> = pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        assert!(body.len() > 1, "{pem}");
        assert!(body.iter().all(|line| line.len() <= 64), "{pem}");
        assert_eq!(pem_decode(&pem).unwrap(), der);

        // The same DER as one long line decodes just as well.
        let flat = format!("{BEGIN}\n{}\n{END}\n", base64_encode(&der));
        assert_eq!(pem_decode(&flat).unwrap(), der);

        // A certificate file with surrounding text is read from the block.
        let wrapped = format!("subject=CN=eaccess\n{pem}extra\n");
        assert_eq!(pem_decode(&wrapped).unwrap(), der);
    }

    #[test]
    fn pem_decode_explains_what_is_wrong() {
        assert_eq!(
            pem_decode("nothing here").unwrap_err(),
            format!("no `{BEGIN}` line")
        );
        assert_eq!(
            pem_decode(&format!("{BEGIN}\n")).unwrap_err(),
            format!("no `{END}` line")
        );
        assert_eq!(
            pem_decode(&format!("{BEGIN}\n\n{END}\n")).unwrap_err(),
            "the certificate block is empty"
        );
        assert!(pem_decode(&format!("{BEGIN}\n!!!!\n{END}\n"))
            .unwrap_err()
            .starts_with("invalid base64: "));
    }

    #[test]
    fn first_use_records_the_pin_and_later_logins_must_match() {
        let pin = scratch("first-use");
        let der = vec![1u8, 2, 3, 4, 5];

        assert!(verify(&pin, &der).is_ok());
        assert!(pin.exists());
        assert!(verify(&pin, &der).is_ok());

        let err = verify(&pin, &[9u8, 9, 9]).unwrap_err();
        assert!(
            err.starts_with("the eaccess server's certificate CHANGED:"),
            "{err}"
        );

        let _ = std::fs::remove_dir_all(pin.parent().unwrap());
    }

    #[test]
    fn a_pin_that_is_not_pem_refuses_the_login() {
        let pin = scratch("not-pem");
        std::fs::write(&pin, "this is not a certificate\n").unwrap();
        let err = verify(&pin, &[1, 2, 3]).unwrap_err();
        assert!(err.contains("is not a PEM certificate"), "{err}");
        assert!(err.contains(&format!("no `{BEGIN}` line")), "{err}");
        assert!(err.contains("to trust the certificate"));
        let _ = std::fs::remove_dir_all(pin.parent().unwrap());
    }

    #[test]
    fn an_unreadable_pin_refuses_the_login() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("hydra-net-pin-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("eaccess.pem")).unwrap();
        let err = verify(&dir.join("eaccess.pem"), &[1]).unwrap_err();
        assert!(err.starts_with("the certificate pin "), "{err}");
        assert!(err.ends_with("Refusing to log in unpinned."), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
