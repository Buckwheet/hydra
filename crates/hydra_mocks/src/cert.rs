//! A self-signed Ed25519 certificate, generated at test time.
//!
//! No files and no external process: `ring` makes the key pair and the DER in
//! this module is assembled by hand, so nothing resembling a certificate or a
//! private key ever lands in the repository.  Every call generates a *new* key,
//! which is what makes "the pinned certificate changed" testable — start a
//! second [`crate::MockEaccess`] and it presents a different certificate.
//!
//! The certificate is deliberately minimal (a v3 certificate with no
//! extensions): the only thing the mock needs is something a TLS client accepts
//! as an end-entity certificate with an Ed25519 signature it can check.

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

/// The subject (and issuer) common name.
///
/// `.invalid` is reserved by RFC 2606, so this name can never resolve to a real
/// host even if it leaked out of a test.
pub const COMMON_NAME: &str = "eaccess.invalid";

/// `notBefore`, as an X.509 `UTCTime`.
const NOT_BEFORE: &[u8] = b"240101000000Z";
/// `notAfter`, as an X.509 `UTCTime`.
const NOT_AFTER: &[u8] = b"340101000000Z";

const TAG_INTEGER: u8 = 0x02;
const TAG_BIT_STRING: u8 = 0x03;
const TAG_OID: u8 = 0x06;
const TAG_UTF8_STRING: u8 = 0x0c;
const TAG_UTC_TIME: u8 = 0x17;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_SET: u8 = 0x31;
/// `[0]` constructed: the `version` field of a v3 `TBSCertificate`.
const TAG_VERSION: u8 = 0xa0;

/// `1.3.101.112`, the Ed25519 signature algorithm OID.
const OID_ED25519: [u8; 3] = [0x2b, 0x65, 0x70];
/// `2.5.4.3`, the `commonName` attribute OID.
const OID_COMMON_NAME: [u8; 3] = [0x55, 0x04, 0x03];

/// A certificate and the private key it was signed with, both DER.
///
/// `cert_der` is the whole self-signed certificate (what a TLS server presents);
/// `key_der` is the PKCS#8 private key (what it signs with).  Neither is ever
/// written to disk by this crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedCert {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

/// Generate a fresh self-signed Ed25519 certificate.
pub fn generate() -> GeneratedCert {
    let rng = SystemRandom::new();
    let key_der = Ed25519KeyPair::generate_pkcs8(&rng)
        .expect("the system random source must be able to make an Ed25519 key")
        .as_ref()
        .to_vec();
    let key = Ed25519KeyPair::from_pkcs8(&key_der)
        .expect("a key ring just generated must parse back");
    let public_key = key.public_key().as_ref().to_vec();

    let algorithm = ed25519_algorithm();
    let name = name(COMMON_NAME);
    let subject_public_key_info = sequence(&[&algorithm, &bit_string(&public_key)]);
    // version, serialNumber, signature, issuer, validity, subject, SPKI.  The
    // serial is a constant: every call has a fresh key, so every call already
    // produces a different certificate.
    let tbs_certificate = sequence(&[
        &tlv(TAG_VERSION, &tlv(TAG_INTEGER, &[2])),
        &tlv(TAG_INTEGER, &[1]),
        &algorithm,
        &name,
        &validity(),
        &name,
        &subject_public_key_info,
    ]);
    let signature = key.sign(&tbs_certificate);
    let cert_der = sequence(&[
        &tbs_certificate,
        &algorithm,
        &bit_string(signature.as_ref()),
    ]);

    GeneratedCert { cert_der, key_der }
}

/// The DER length prefix: one byte below `0x80`, else `0x80 | n` and `n` big
/// endian length bytes (definite form only).
fn length(len: usize) -> Vec<u8> {
    if len < 0x80 {
        return vec![len as u8];
    }
    let bytes: Vec<u8> = len
        .to_be_bytes()
        .into_iter()
        .skip_while(|byte| *byte == 0)
        .collect();
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.push(0x80 | bytes.len() as u8);
    out.extend_from_slice(&bytes);
    out
}

/// One tag-length-value element.
fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 4);
    out.push(tag);
    out.extend(length(body.len()));
    out.extend_from_slice(body);
    out
}

/// A `SEQUENCE` whose body is the concatenation of `parts`.
fn sequence(parts: &[&[u8]]) -> Vec<u8> {
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(part);
    }
    tlv(TAG_SEQUENCE, &body)
}

/// A `BIT STRING` with no unused trailing bits.
fn bit_string(bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(bytes.len() + 1);
    body.push(0);
    body.extend_from_slice(bytes);
    tlv(TAG_BIT_STRING, &body)
}

/// `AlgorithmIdentifier` for Ed25519 (`SEQUENCE { OID 1.3.101.112 }`).
fn ed25519_algorithm() -> Vec<u8> {
    sequence(&[&tlv(TAG_OID, &OID_ED25519)])
}

/// The validity `SEQUENCE` of two `UTCTime`s.
fn validity() -> Vec<u8> {
    sequence(&[
        &tlv(TAG_UTC_TIME, NOT_BEFORE),
        &tlv(TAG_UTC_TIME, NOT_AFTER),
    ])
}

/// A distinguished name with one `commonName` attribute.
fn name(common_name: &str) -> Vec<u8> {
    let attribute = sequence(&[
        &tlv(TAG_OID, &OID_COMMON_NAME),
        &tlv(TAG_UTF8_STRING, common_name.as_bytes()),
    ]);
    sequence(&[&tlv(TAG_SET, &attribute)])
}

#[cfg(test)]
mod tests {
    use super::*;

    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

    #[test]
    fn rustls_accepts_the_certificate_and_its_key() {
        crate::install_provider();
        let generated = generate();
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(generated.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.key_der.clone())),
            );
        assert!(
            config.is_ok(),
            "the generated certificate must be usable by a rustls server: {:?}",
            config.err()
        );
    }

    #[test]
    fn every_call_makes_a_new_certificate() {
        let first = generate();
        let second = generate();
        assert_ne!(first.cert_der, second.cert_der);
        assert_ne!(first.key_der, second.key_der);
        // The same call's bytes are stable, so the pin comparison is meaningful.
        assert_eq!(first.cert_der, first.clone().cert_der);
    }

    /// The bytes of the DER header (tag plus length) of the element at `der`.
    fn header_len(der: &[u8]) -> usize {
        if der[1] & 0x80 == 0 {
            2
        } else {
            2 + (der[1] & 0x7f) as usize
        }
    }

    /// The length the header at `der` declares for its body.
    fn body_len(der: &[u8]) -> usize {
        if der[1] & 0x80 == 0 {
            der[1] as usize
        } else {
            der[2..header_len(der)]
                .iter()
                .fold(0, |acc, byte| (acc << 8) | *byte as usize)
        }
    }

    #[test]
    fn the_certificate_is_a_signed_ed25519_der_structure() {
        let generated = generate();
        let cert = &generated.cert_der;
        assert_eq!(cert[0], TAG_SEQUENCE, "a certificate is a SEQUENCE");
        assert!(cert[1] & 0x80 != 0, "a certificate is longer than 127 bytes");
        assert_eq!(
            body_len(cert) + header_len(cert),
            cert.len(),
            "the outer length must cover exactly the certificate"
        );

        // The public key of the private key is inside the certificate's SPKI.
        let key = Ed25519KeyPair::from_pkcs8(&generated.key_der).unwrap();
        let public_key = key.public_key().as_ref().to_vec();
        assert_eq!(public_key.len(), 32, "an Ed25519 public key is 32 bytes");
        assert!(
            cert.windows(public_key.len())
                .any(|window| window == public_key.as_slice()),
            "the certificate must carry the public half of its key"
        );

        // The subject common name is in there too, as a UTF8String.
        assert!(cert
            .windows(COMMON_NAME.len())
            .any(|window| window == COMMON_NAME.as_bytes()));

        // The first element of the certificate is the tbsCertificate, and the
        // signature over it is the last 64 bytes: check the signature really is
        // this key's signature over that element.
        let body = &cert[header_len(cert)..];
        let tbs = &body[..header_len(body) + body_len(body)];
        let signature = &cert[cert.len() - 64..];
        let verifier = ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &public_key);
        assert!(
            verifier.verify(tbs, signature).is_ok(),
            "the certificate must be signed by the key it carries"
        );
    }

    #[test]
    fn the_length_prefix_uses_the_definite_form() {
        assert_eq!(length(0), vec![0x00]);
        assert_eq!(length(0x7f), vec![0x7f]);
        assert_eq!(length(0x80), vec![0x81, 0x80]);
        assert_eq!(length(200), vec![0x81, 0xc8]);
        assert_eq!(length(256), vec![0x82, 0x01, 0x00]);
        assert_eq!(length(0x1234), vec![0x82, 0x12, 0x34]);

        // A long body gets a long-form length: the tag is still one byte.
        let big = tlv(TAG_SEQUENCE, &vec![0u8; 200]);
        assert_eq!(&big[..3], &[TAG_SEQUENCE, 0x81, 0xc8]);
        assert_eq!(big.len(), 203);
    }

    #[test]
    fn a_name_is_a_set_of_one_sequence() {
        let name = name("a");
        assert_eq!(name[0], TAG_SEQUENCE);
        assert_eq!(name[2], TAG_SET);
        assert_eq!(name[4], TAG_SEQUENCE);
        assert_eq!(name[6], TAG_OID);
    }
}
