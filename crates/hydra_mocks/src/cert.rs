//! A self-signed RSA certificate, generated at test time.
//!
//! No files and no external process: the DER in this module is assembled by
//! hand and signed in-process, so nothing resembling a certificate ever lands
//! in the repository.  The signing key is a **throwaway synthetic** RSA-2048
//! key embedded below (see [`SYNTHETIC_KEY_PKCS8`]): it protects nothing, it is
//! only here so a test server can complete a TLS 1.2 handshake.  Every call
//! mints a *new* certificate (a fresh serial number is signed into the TBS),
//! which is what makes "the pinned certificate changed" testable — start a
//! second [`crate::MockEaccess`] and it presents a different certificate.
//!
//! The key is RSA rather than Ed25519 because of the client: the real login
//! uses the platform TLS stack (`native-tls`), and on macOS that is Secure
//! Transport, which in TLS 1.2 does not offer `ed25519` in its
//! `signature_algorithms` — a rustls server holding an Ed25519 key therefore
//! ends the handshake with "incompatible signing key".  RSA also happens to
//! match what the real eaccess server presents (static RSA), so the mock is
//! closer to the real thing this way.  The signature is `sha256WithRSAEncryption`
//! and the key exchange is ECDHE_RSA, which rustls offers and Secure Transport
//! accepts.  (The real server does static RSA key exchange, which rustls never
//! offers at all; the certificate is what the tests are about, not the key
//! exchange, and the client disables chain validation anyway — the certificate
//! pin is the trust decision.)
//!
//! The certificate is deliberately minimal: a v3 certificate with no
//! extensions.  The only thing the mock needs is something a TLS client accepts
//! as an end-entity certificate whose signature it can check.

use std::sync::atomic::{AtomicU64, Ordering};

use ring::rand::SystemRandom;
use ring::signature::{KeyPair, RsaKeyPair, RSA_PKCS1_SHA256};

/// The subject (and issuer) common name.
///
/// `.invalid` is reserved by RFC 2606, so this name can never resolve to a real
/// host even if it leaked out of a test.
pub const COMMON_NAME: &str = "eaccess.invalid";

/// `notBefore`, as an X.509 `UTCTime`.
const NOT_BEFORE: &[u8] = b"240101000000Z";
/// `notAfter`, as an X.509 `UTCTime`.
const NOT_AFTER: &[u8] = b"340101000000Z";

/// A throwaway RSA-2048 signing key, PKCS#8 DER, **synthetic**.
///
/// Generated once with `openssl` for the mock server only; it is not a secret,
/// it signs nothing but loopback test certificates, and every certificate this
/// module mints is publicly marked `CN=eaccess.invalid`.  Kept as bytes (not as
/// a file) so no key material is ever read from disk at test time.
const SYNTHETIC_KEY_PKCS8: &[u8] = &[
    0x30, 0x82, 0x04, 0xbf, 0x02, 0x01, 0x00, 0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7,
    0x0d, 0x01, 0x01, 0x01, 0x05, 0x00, 0x04, 0x82, 0x04, 0xa9, 0x30, 0x82, 0x04, 0xa5, 0x02, 0x01,
    0x00, 0x02, 0x82, 0x01, 0x01, 0x00, 0xcd, 0xa9, 0x8b, 0x02, 0x89, 0x72, 0xfa, 0x5c, 0x63, 0x84,
    0x27, 0x01, 0x23, 0x30, 0x5e, 0x09, 0xfa, 0xb0, 0xfb, 0xb4, 0xdc, 0x4a, 0xc4, 0x9c, 0x0a, 0x76,
    0x6f, 0x0b, 0xb7, 0xb2, 0x6b, 0x2b, 0xda, 0x93, 0x4e, 0xa3, 0xc2, 0x95, 0x29, 0x76, 0xe4, 0xbf,
    0x69, 0x1c, 0xdd, 0x14, 0x9a, 0x37, 0x93, 0xc7, 0x8c, 0xf0, 0x41, 0x38, 0x78, 0xd4, 0x3f, 0x7e,
    0xff, 0x8f, 0x22, 0xb7, 0xa7, 0xa0, 0xcb, 0x78, 0x9f, 0x21, 0x17, 0x15, 0xa5, 0x02, 0xf7, 0xd3,
    0xd9, 0x9d, 0xba, 0x7c, 0xa0, 0x56, 0x9b, 0xca, 0x7b, 0x22, 0xf3, 0x0d, 0x40, 0x62, 0x50, 0xc8,
    0x6d, 0x9b, 0x3d, 0xa3, 0xe8, 0xe4, 0xfb, 0xaf, 0xd1, 0xa5, 0xb3, 0x9d, 0x8f, 0xcf, 0x82, 0xe0,
    0x98, 0xd2, 0x6f, 0x0c, 0x14, 0x54, 0xdb, 0xbe, 0x8e, 0xe0, 0xf6, 0xe5, 0x87, 0x1d, 0x1e, 0x95,
    0x0a, 0xc4, 0x7d, 0x82, 0x76, 0xd2, 0x47, 0xef, 0xd2, 0x17, 0xc2, 0x6c, 0x81, 0xff, 0xb2, 0x70,
    0x8c, 0x25, 0x5b, 0x23, 0xe4, 0xbe, 0xa4, 0xb3, 0x04, 0x15, 0xd4, 0x5f, 0xb1, 0x9c, 0xe6, 0xb0,
    0x89, 0x07, 0xe2, 0xc1, 0x9a, 0xe2, 0x48, 0x7f, 0xdf, 0xd4, 0x16, 0x70, 0x93, 0xc1, 0xcf, 0x67,
    0xb4, 0xaf, 0xc9, 0x19, 0x98, 0x78, 0x55, 0x45, 0x4d, 0xf9, 0x2f, 0x57, 0x38, 0x8b, 0xc0, 0x66,
    0xb0, 0xed, 0xfd, 0x36, 0x0f, 0x76, 0x75, 0x4f, 0x5f, 0x18, 0xbb, 0x81, 0x5a, 0x5a, 0x28, 0x0f,
    0xb4, 0x84, 0xdf, 0x1d, 0xc3, 0x89, 0x8c, 0x0a, 0x03, 0xce, 0x5d, 0x15, 0xbd, 0x66, 0xe9, 0xc4,
    0x8d, 0x2b, 0x86, 0xf2, 0x32, 0x15, 0x5f, 0x93, 0x3c, 0x66, 0x69, 0xb2, 0xf4, 0xad, 0x99, 0x4e,
    0x10, 0x6b, 0x00, 0xd4, 0x4b, 0x1a, 0xa9, 0x75, 0xde, 0xbb, 0x9e, 0x79, 0x29, 0xde, 0x63, 0xe2,
    0xce, 0x24, 0x4b, 0xe1, 0x21, 0x63, 0x02, 0x03, 0x01, 0x00, 0x01, 0x02, 0x82, 0x01, 0x00, 0x4e,
    0x2b, 0x99, 0x28, 0xb7, 0xb1, 0xba, 0x33, 0x8f, 0xa7, 0xd5, 0x63, 0x7b, 0x4a, 0x91, 0x13, 0xdd,
    0xf3, 0x93, 0x58, 0xa5, 0x3a, 0xce, 0x0c, 0x39, 0x75, 0xd6, 0xb3, 0x6f, 0x79, 0x23, 0x7f, 0x21,
    0xaf, 0x47, 0xfe, 0xba, 0xa1, 0x13, 0x87, 0x9b, 0x5e, 0x9d, 0xc7, 0x58, 0xc8, 0xd1, 0xed, 0x3a,
    0x00, 0x69, 0x33, 0x90, 0xb6, 0xb4, 0x46, 0x88, 0x73, 0xef, 0x13, 0x2a, 0xb0, 0xff, 0xc8, 0x00,
    0x4e, 0xb2, 0xbd, 0x53, 0x16, 0xe9, 0x07, 0x7d, 0xc7, 0x4a, 0x39, 0xc7, 0x7e, 0x43, 0x3b, 0x68,
    0x2f, 0xc8, 0x29, 0xfa, 0x94, 0x18, 0xda, 0xc5, 0xb5, 0xc8, 0xe3, 0x6e, 0xc4, 0x74, 0x6b, 0x44,
    0xb7, 0x09, 0x4a, 0xa0, 0x40, 0xcd, 0x3e, 0xa4, 0x92, 0x55, 0xc3, 0xf6, 0x6d, 0x72, 0xf4, 0xe6,
    0x29, 0x42, 0x09, 0x84, 0x0e, 0xae, 0x12, 0x85, 0x63, 0x45, 0x1a, 0x76, 0x52, 0x36, 0x72, 0xfc,
    0x47, 0x4e, 0x17, 0xfe, 0xca, 0xcb, 0xcc, 0x0e, 0x5e, 0x53, 0xec, 0x32, 0x9c, 0xc2, 0x18, 0xe0,
    0x39, 0x32, 0xff, 0xd1, 0x50, 0x49, 0x7c, 0x2f, 0x50, 0xf1, 0xee, 0x65, 0x7d, 0x40, 0x56, 0x33,
    0xa0, 0x10, 0x66, 0x0b, 0xcf, 0xa1, 0x77, 0xda, 0xdf, 0x85, 0xda, 0xf7, 0xc0, 0x2f, 0x4f, 0x05,
    0x46, 0x6b, 0x40, 0xf3, 0x5a, 0xc0, 0x70, 0x6e, 0x36, 0x75, 0x8d, 0x5f, 0xb1, 0x45, 0xfa, 0xa0,
    0x28, 0x05, 0x0e, 0x91, 0x4d, 0x54, 0x02, 0x90, 0x79, 0xc0, 0x40, 0x23, 0x6d, 0xcb, 0x50, 0x63,
    0x35, 0x9d, 0x15, 0x2e, 0xd6, 0x60, 0xf1, 0x3d, 0x17, 0x50, 0x6e, 0x5f, 0x8d, 0xe7, 0x7c, 0xc4,
    0x76, 0x49, 0x41, 0x10, 0x0d, 0xb6, 0xa9, 0xb7, 0x88, 0xbf, 0xd1, 0xf5, 0x21, 0xbe, 0xfe, 0x1a,
    0x65, 0xed, 0xb0, 0xeb, 0x99, 0x3c, 0x23, 0x12, 0x57, 0x47, 0x28, 0xf6, 0x26, 0xf7, 0x61, 0x02,
    0x81, 0x81, 0x00, 0xef, 0xcc, 0xc8, 0xad, 0x84, 0x3c, 0x7f, 0x18, 0x28, 0x16, 0x0b, 0x37, 0xf9,
    0x00, 0x84, 0xa1, 0x21, 0xb8, 0xeb, 0x30, 0xce, 0x91, 0x53, 0x56, 0x81, 0xaf, 0x11, 0xe9, 0x61,
    0xcf, 0x9f, 0x1c, 0x5c, 0x0b, 0x00, 0x08, 0x5f, 0x22, 0x68, 0x94, 0x00, 0xf9, 0xdc, 0xd0, 0x95,
    0x2c, 0xbd, 0x05, 0x3a, 0x5c, 0x6f, 0xd9, 0xb4, 0xb9, 0xc3, 0xbc, 0x6c, 0x1a, 0xe6, 0x8c, 0xf2,
    0xac, 0x67, 0x4e, 0x0a, 0x08, 0x3b, 0x81, 0x4e, 0xeb, 0xe7, 0x05, 0x68, 0xe3, 0x88, 0x96, 0x5a,
    0x33, 0xc7, 0x20, 0x53, 0xa0, 0x17, 0x5b, 0x37, 0x8b, 0xe0, 0x01, 0xb4, 0x21, 0x3f, 0x10, 0x59,
    0x8b, 0xf0, 0x7d, 0x66, 0x07, 0xc0, 0x68, 0xca, 0x41, 0x15, 0x7f, 0x0c, 0xbf, 0x6d, 0xe9, 0x26,
    0x75, 0x4c, 0x75, 0x49, 0x4c, 0x2a, 0xe2, 0xf3, 0x0c, 0x24, 0x33, 0xb3, 0xdc, 0x79, 0xf1, 0x8e,
    0x2a, 0xd8, 0x4b, 0x02, 0x81, 0x81, 0x00, 0xdb, 0x8e, 0x5d, 0xac, 0x4b, 0x5a, 0xc1, 0xf1, 0xf0,
    0x66, 0x71, 0x47, 0xc0, 0xf6, 0xe8, 0xed, 0x2a, 0x53, 0xa8, 0xb3, 0x51, 0xe5, 0x5c, 0x27, 0x2f,
    0x73, 0x71, 0xe7, 0xbb, 0xb6, 0x75, 0xfe, 0x28, 0xe7, 0x31, 0xfd, 0x74, 0x5d, 0x43, 0x40, 0xc0,
    0xee, 0x76, 0x70, 0xdf, 0x7f, 0x85, 0x07, 0x3b, 0x54, 0x3c, 0x7e, 0x2a, 0x3b, 0x40, 0x5d, 0x4f,
    0x40, 0x4d, 0xa3, 0x7a, 0xbc, 0x3e, 0x5e, 0x15, 0x8a, 0x61, 0x15, 0x40, 0x84, 0x60, 0x90, 0xfc,
    0x4c, 0x00, 0x95, 0xa0, 0x5f, 0xf5, 0x51, 0x69, 0xd8, 0x9a, 0x98, 0x79, 0x37, 0xe5, 0xd3, 0x90,
    0x46, 0x7c, 0xf8, 0xf4, 0x80, 0xcd, 0x20, 0xb8, 0x95, 0x5a, 0x69, 0x84, 0x92, 0xa0, 0xa4, 0x52,
    0x48, 0x24, 0x27, 0x91, 0xf1, 0x1c, 0x9d, 0x82, 0x7c, 0xdc, 0xf2, 0x14, 0xe8, 0x49, 0x83, 0x28,
    0x48, 0x55, 0x39, 0x33, 0xbb, 0xdc, 0x49, 0x02, 0x81, 0x81, 0x00, 0xb9, 0x38, 0xe7, 0x64, 0xc7,
    0x78, 0x66, 0xff, 0x04, 0xaf, 0x8a, 0x38, 0xd0, 0xb7, 0x6a, 0xba, 0x7f, 0x2e, 0xb3, 0x53, 0x7e,
    0xaa, 0xd7, 0xc1, 0x40, 0xe8, 0x36, 0x20, 0x9b, 0x71, 0x60, 0x47, 0xea, 0x7c, 0x80, 0xa8, 0xf6,
    0xba, 0xc0, 0x26, 0x59, 0xde, 0xa5, 0x85, 0xe3, 0x43, 0xac, 0x38, 0xe6, 0x8c, 0x51, 0xc4, 0xca,
    0x08, 0x96, 0xbc, 0xea, 0x8f, 0x98, 0xb7, 0x14, 0x16, 0xc0, 0xb0, 0x91, 0xa5, 0x2c, 0x39, 0x43,
    0xf9, 0x88, 0xf7, 0x2f, 0xbe, 0x58, 0x59, 0x32, 0x11, 0x3e, 0x64, 0xa4, 0xd3, 0x4a, 0x57, 0x25,
    0xbf, 0x7f, 0xe6, 0x30, 0xf9, 0x61, 0x19, 0xc2, 0xe1, 0x09, 0x80, 0x02, 0x98, 0x48, 0x31, 0x68,
    0xcf, 0xfe, 0xc7, 0xa4, 0xd4, 0x14, 0x14, 0x7b, 0xa5, 0x2c, 0xca, 0x87, 0x34, 0x33, 0x08, 0x1b,
    0x8d, 0xac, 0x44, 0xd7, 0x4f, 0x9c, 0x4d, 0x4d, 0x6f, 0xa0, 0xa3, 0x02, 0x81, 0x81, 0x00, 0xc5,
    0xa1, 0x30, 0xd0, 0x96, 0x43, 0xeb, 0xb0, 0x4c, 0x24, 0xea, 0x73, 0x1c, 0x4f, 0x24, 0xfb, 0xd6,
    0xcd, 0xd7, 0x5d, 0x12, 0xaa, 0xd2, 0x78, 0xb3, 0x4d, 0xc7, 0x8a, 0x40, 0x7d, 0xcd, 0x58, 0x2a,
    0x99, 0x71, 0x76, 0x23, 0xf8, 0x8f, 0x41, 0x0a, 0x69, 0x9c, 0x53, 0x9d, 0xda, 0xbb, 0x73, 0xf7,
    0x03, 0xac, 0x30, 0x24, 0xf3, 0x90, 0xcc, 0x6d, 0x3a, 0x04, 0xa8, 0xd3, 0x21, 0xff, 0x41, 0xd1,
    0x41, 0xee, 0x81, 0xa2, 0xc1, 0x85, 0xea, 0xfc, 0x2f, 0xad, 0x97, 0x01, 0xf5, 0x5e, 0x6d, 0x1a,
    0x49, 0xcf, 0x5e, 0xd0, 0x3e, 0x70, 0xb2, 0x2f, 0x7c, 0x43, 0xa2, 0x14, 0xf4, 0x9b, 0xb0, 0x88,
    0x76, 0x68, 0x1b, 0xf3, 0x29, 0xc9, 0x50, 0x20, 0x80, 0xaf, 0x8a, 0xae, 0xe3, 0xca, 0x4e, 0x3d,
    0xd7, 0x07, 0x4b, 0x21, 0x66, 0xb6, 0x45, 0x49, 0x4d, 0x52, 0xe9, 0x5a, 0x98, 0x0f, 0x81, 0x02,
    0x81, 0x81, 0x00, 0xcc, 0x5e, 0x57, 0xf6, 0x29, 0xf6, 0x68, 0xae, 0xaf, 0x79, 0x29, 0x4b, 0xd4,
    0x30, 0x72, 0x43, 0x8f, 0x09, 0x88, 0x78, 0xe5, 0x73, 0x45, 0xc9, 0x77, 0x74, 0x39, 0xc2, 0x2b,
    0xd7, 0x25, 0x4c, 0xdb, 0x47, 0x18, 0x6a, 0xc2, 0x2b, 0x3c, 0x8b, 0x13, 0x16, 0x95, 0x99, 0x61,
    0xed, 0xa3, 0xe8, 0xed, 0x42, 0x79, 0xec, 0x83, 0xc8, 0xb3, 0x7e, 0xb4, 0xad, 0xdc, 0x59, 0x80,
    0x54, 0x3d, 0x52, 0x01, 0x49, 0xaa, 0xff, 0x5b, 0xe4, 0x0a, 0xb7, 0x86, 0xf0, 0x2c, 0xfd, 0x78,
    0xb8, 0xeb, 0x26, 0x64, 0x13, 0xc1, 0x09, 0xfd, 0xe7, 0x97, 0x3d, 0xe7, 0xd9, 0xde, 0x58, 0x9a,
    0xdb, 0x93, 0x14, 0xa9, 0x28, 0x99, 0x56, 0xe9, 0x3a, 0x33, 0x58, 0x2e, 0xb8, 0x9b, 0xf1, 0x71,
    0x5f, 0x64, 0xfc, 0xaf, 0xc8, 0xc2, 0xc0, 0x94, 0x5f, 0x9a, 0x68, 0x57, 0x7c, 0x50, 0x8a, 0x13,
    0x7c, 0xb9, 0xa7,
];

/// Source of the per-certificate serial numbers.
///
/// A new serial every call means a new TBS, a new signature and therefore new
/// certificate bytes, even though the signing key is the same.
static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

const TAG_INTEGER: u8 = 0x02;
const TAG_BIT_STRING: u8 = 0x03;
const TAG_NULL: u8 = 0x05;
const TAG_OID: u8 = 0x06;
const TAG_UTF8_STRING: u8 = 0x0c;
const TAG_UTC_TIME: u8 = 0x17;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_SET: u8 = 0x31;
/// `[0]` constructed: the `version` field of a v3 `TBSCertificate`.
const TAG_VERSION: u8 = 0xa0;

/// `1.2.840.113549.1.1.1`, the `rsaEncryption` OID (the SPKI algorithm).
const OID_RSA_ENCRYPTION: [u8; 9] = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
/// `1.2.840.113549.1.1.11`, the `sha256WithRSAEncryption` OID.
const OID_SHA256_WITH_RSA_ENCRYPTION: [u8; 9] =
    [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b];
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

/// Generate a fresh self-signed RSA certificate.
///
/// The signing key is the embedded synthetic one, but the serial number is
/// unique, so the bytes returned differ on every call.
pub fn generate() -> GeneratedCert {
    let key_der = SYNTHETIC_KEY_PKCS8.to_vec();
    let key = RsaKeyPair::from_pkcs8(&key_der)
        .expect("the embedded synthetic RSA key must parse as a PKCS#8 key pair");

    let algorithm = sha256_with_rsa_algorithm();
    let name = name(COMMON_NAME);
    let subject_public_key_info = {
        // ring hands back the modulus and exponent as big-endian bytes with no
        // leading zeros, which is exactly what an INTEGER body wants.
        let components = ring::rsa::PublicKeyComponents::<Vec<u8>>::from(key.public_key());
        sequence(&[
            &rsa_encryption_algorithm(),
            &bit_string(&rsa_public_key(&components.n, &components.e)),
        ])
    };
    // version, serialNumber, signature, issuer, validity, subject, SPKI.  The
    // only part that changes between calls is the serial.
    let tbs_certificate = sequence(&[
        &tlv(TAG_VERSION, &tlv(TAG_INTEGER, &[2])),
        &tlv(TAG_INTEGER, &positive_integer(&next_serial().to_be_bytes())),
        &algorithm,
        &name,
        &validity(),
        &name,
        &subject_public_key_info,
    ]);

    let mut signature = vec![0u8; key.public().modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        &tbs_certificate,
        &mut signature,
    )
    .expect("signing with a key ring just parsed must succeed");
    let cert_der = sequence(&[&tbs_certificate, &algorithm, &bit_string(&signature)]);

    GeneratedCert { cert_der, key_der }
}

/// The next serial number for a certificate: a per-process counter, never `0`.
fn next_serial() -> u64 {
    NEXT_SERIAL.fetch_add(1, Ordering::Relaxed)
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

/// The body of an `INTEGER` for a non-negative big-endian number of `bytes`.
///
/// `n` and `e` arrive without leading zeros, so all this adds is the `0x00`
/// that X.509's two's complement encoding needs when the top bit is set.
fn positive_integer(bytes: &[u8]) -> Vec<u8> {
    let stripped: Vec<u8> = bytes
        .iter()
        .copied()
        .skip_while(|byte| *byte == 0)
        .collect();
    let mut out = Vec::with_capacity(stripped.len() + 1);
    if stripped.first().map_or(true, |first| first & 0x80 != 0) {
        out.push(0);
    }
    out.extend_from_slice(&stripped);
    out
}

/// `AlgorithmIdentifier` for `sha256WithRSAEncryption`:
/// `SEQUENCE { OID 1.2.840.113549.1.1.11, NULL }`.
fn sha256_with_rsa_algorithm() -> Vec<u8> {
    sequence(&[
        &tlv(TAG_OID, &OID_SHA256_WITH_RSA_ENCRYPTION),
        &tlv(TAG_NULL, &[]),
    ])
}

/// `AlgorithmIdentifier` for `rsaEncryption`:
/// `SEQUENCE { OID 1.2.840.113549.1.1.1, NULL }`.
fn rsa_encryption_algorithm() -> Vec<u8> {
    sequence(&[&tlv(TAG_OID, &OID_RSA_ENCRYPTION), &tlv(TAG_NULL, &[])])
}

/// The PKCS#1 `RSAPublicKey`: `SEQUENCE { INTEGER n, INTEGER e }`.
fn rsa_public_key(modulus: &[u8], exponent: &[u8]) -> Vec<u8> {
    sequence(&[
        &tlv(TAG_INTEGER, &positive_integer(modulus)),
        &tlv(TAG_INTEGER, &positive_integer(exponent)),
    ])
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

    use ring::signature::{UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
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
        // One synthetic key, but a new serial each call, so the certificate
        // bytes differ.
        assert_eq!(first.key_der, second.key_der);
        assert_ne!(first.cert_der, second.cert_der);
        // The same call's bytes are stable, so the pin comparison is meaningful.
        assert_eq!(first.cert_der, first.clone().cert_der);
    }

    #[test]
    fn the_embedded_key_is_a_2048_bit_rsa_key() {
        let key = RsaKeyPair::from_pkcs8(SYNTHETIC_KEY_PKCS8)
            .expect("the embedded synthetic key must parse as PKCS#8");
        assert_eq!(
            key.public().modulus_len(),
            256,
            "a 2048-bit RSA modulus is 256 bytes"
        );
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

    /// The whole first element of `der`: its tag, its length and its body.
    fn element(der: &[u8]) -> &[u8] {
        let end = header_len(der) + body_len(der);
        assert!(end <= der.len(), "an element must fit in the bytes given");
        &der[..end]
    }

    /// Split `der` (one tag-length-value) into its tag and its body.
    fn split(der: &[u8]) -> (u8, &[u8]) {
        let header = header_len(der);
        (der[0], &der[header..header_len(der) + body_len(der)])
    }

    /// The `(tag, body)` children of one element's body.
    fn children(body: &[u8]) -> Vec<(u8, &[u8])> {
        let mut out = Vec::new();
        let mut rest = body;
        while !rest.is_empty() {
            let one = element(rest);
            out.push(split(one));
            rest = &rest[one.len()..];
        }
        out
    }

    #[test]
    fn the_certificate_is_a_signed_rsa_der_structure() {
        let generated = generate();
        let cert = &generated.cert_der;
        assert_eq!(cert[0], TAG_SEQUENCE, "a certificate is a SEQUENCE");
        assert!(
            cert[1] & 0x80 != 0,
            "a certificate is longer than 127 bytes"
        );
        assert_eq!(
            body_len(cert) + header_len(cert),
            cert.len(),
            "the outer length must cover exactly the certificate"
        );

        // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm,
        //                            signatureValue }
        let (body_tag, body) = split(cert);
        assert_eq!(body_tag, TAG_SEQUENCE);
        let certificate = children(body);
        assert_eq!(certificate.len(), 3, "a certificate has three fields");
        let tbs = element(&body[..]);
        let (tbs_tag, tbs_body) = certificate[0];
        assert_eq!(tbs_tag, TAG_SEQUENCE);

        // The signature algorithm is sha256WithRSAEncryption, with the NULL
        // parameter X.509 spells out.
        let (_, signature_algorithm) = certificate[1];
        let signature_algorithm = children(signature_algorithm);
        assert_eq!(signature_algorithm.len(), 2);
        assert_eq!(
            signature_algorithm[0],
            (TAG_OID, OID_SHA256_WITH_RSA_ENCRYPTION.as_slice())
        );
        assert_eq!(signature_algorithm[1], (TAG_NULL, [].as_slice()));

        // The TBS carries the version, the serial, the names, the validity and
        // the SPKI.
        let tbs_fields = children(tbs_body);
        assert_eq!(tbs_fields.len(), 7, "a v3 TBSCertificate has seven fields");
        assert_eq!(
            tbs_fields[0].0, TAG_VERSION,
            "v3: a [0] wrapper around INTEGER 2"
        );
        assert_eq!(tbs_fields[1].0, TAG_INTEGER, "then the serial number");
        assert!(
            tbs_fields[1].1.iter().any(|byte| *byte != 0),
            "the serial number must be a positive integer"
        );

        // The SPKI holds the RSA public key of the embedded private key.
        let (spki_tag, spki_body) = tbs_fields[6];
        assert_eq!(spki_tag, TAG_SEQUENCE);
        let spki = children(spki_body);
        assert_eq!(spki.len(), 2, "an SPKI is an algorithm and a key");
        let (_, algorithm_body) = spki[0];
        let algorithm = children(algorithm_body);
        assert_eq!(
            algorithm[0],
            (TAG_OID, OID_RSA_ENCRYPTION.as_slice()),
            "the SPKI algorithm must be rsaEncryption"
        );
        let (bit_tag, bit_body) = spki[1];
        assert_eq!(bit_tag, TAG_BIT_STRING);
        assert_eq!(bit_body[0], 0, "no unused bits in the key BIT STRING");
        let expected_public_key = {
            let key = RsaKeyPair::from_pkcs8(&generated.key_der).unwrap();
            let components = ring::rsa::PublicKeyComponents::<Vec<u8>>::from(key.public_key());
            assert_eq!(components.n.len(), 256, "a 2048-bit modulus is 256 bytes");
            rsa_public_key(&components.n, &components.e)
        };
        assert_eq!(
            &bit_body[1..],
            expected_public_key,
            "the certificate must carry the public half of its key"
        );

        // The subject common name is in there too, as a UTF8String.
        assert!(cert
            .windows(COMMON_NAME.len())
            .any(|window| window == COMMON_NAME.as_bytes()));

        // The signature is 256 bytes and verifies against the key the
        // certificate carries, over exactly the TBS bytes.
        let (signature_tag, signature_body) = certificate[2];
        assert_eq!(signature_tag, TAG_BIT_STRING);
        assert_eq!(signature_body[0], 0);
        let signature = &signature_body[1..];
        assert_eq!(
            signature.len(),
            256,
            "a 2048-bit RSA signature is 256 bytes"
        );
        assert!(
            UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &bit_body[1..])
                .verify(tbs, signature)
                .is_ok(),
            "the certificate must be signed by the key it carries"
        );
    }

    /// The serial number of the certificate `der`, as the INTEGER body bytes.
    fn serial(der: &[u8]) -> &[u8] {
        let (_, certificate_body) = split(der);
        let (_, tbs_body) = children(certificate_body)[0];
        children(tbs_body)[1].1
    }

    #[test]
    fn every_certificate_has_its_own_serial_number() {
        let first = generate();
        let second = generate();
        assert_ne!(
            serial(&first.cert_der),
            serial(&second.cert_der),
            "a fresh serial is what makes each certificate's DER different"
        );
        assert!(
            serial(&first.cert_der) == serial(&first.cert_der),
            "the bytes of one certificate are stable"
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
    fn a_positive_integer_gets_the_padding_x509_needs() {
        assert_eq!(positive_integer(&[0x01]), vec![0x01]);
        assert_eq!(positive_integer(&[0x00, 0x7f]), vec![0x7f]);
        // A set top bit would read as negative without a leading zero.
        assert_eq!(positive_integer(&[0x80, 0x01]), vec![0x00, 0x80, 0x01]);
        assert_eq!(positive_integer(&[0x7f, 0xff]), vec![0x7f, 0xff]);
        assert_eq!(positive_integer(&[]), vec![0x00]);
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
