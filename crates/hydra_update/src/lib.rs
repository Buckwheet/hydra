//! `hydra_update` — finding a newer Hydra release, proving it is the one its
//! signature names, and swapping it into place.
//!
//! The original crate talked to GitHub with `reqwest` on a `tokio` runtime and
//! verified with `minisign_verify`; this reconstruction is std only.  What that
//! changes is recorded here and in `NOTES.md`:
//!
//! * **the network is a seam.**  [`Transport`] is one method wide; the only
//!   implementation that ships is [`Offline`], which refuses every request, so
//!   nothing here can reach the network by accident.  A real client is supplied
//!   by the caller (see `Releases::with`).
//! * **the signature math is a seam.**  [`signed::Verifier`] is called only
//!   after the key, the signature and the trusted comment have been parsed and
//!   checked; the built-in verifier refuses everything, so a release is never
//!   accepted on trust.
//! * **the releases URL and the asset name are parameters.**  The binary does
//!   not carry either as a string (there is no releases endpoint among the 31
//!   `https://` literals, and no `*.zip`/`macos`/`arm64` asset name anywhere in
//!   the file), and `hydra::update::keep` builds `Releases::look(&self,
//!   &Version)` with only those two arguments, so they reached `look` as
//!   compile-time constants this build cannot see.  They are explicit
//!   parameters here.
//!
//! What *is* recovered exactly: the user agent, the two timeouts, the minisign
//! public key, the asset names inside a release, the JSON schema, the checksum
//! and swap algorithms, and every error message.

pub mod json;
pub mod release;
pub mod sha256;
pub mod signed;
pub mod swap;
mod inflate;
mod zip;

use release::{Offer, Version};
use std::time::Duration;

/// The version this build reports as its own, parsed by
/// `hydra::update::keep` @0x10009e744 (`semver::Version::parse("0.1.0-beta.7")`
/// with length 12).
pub const VERSION: &str = "0.1.0-beta.7";

/// `Releases::new` @0x1010af1e0: `ClientBuilder::user_agent("hydra/0.1.0-beta.7")`,
/// 18 bytes.
pub const USER_AGENT: &str = "hydra/0.1.0-beta.7";

/// The release signing key, in minisign's base64 form (56 bytes at `__const`
/// 0x1020701b8, loaded at 0x10025e410 with length 0x38).
pub const KEY: &str = "RWTnP4xymgNChF6vPB59oTv8tc6coKg6Svl9A5P0MtDOB/Enpl70UtLc";

/// `Releases::new` sets 30 seconds on the client: two `Some(30s)` written into
/// the builder at 0x1010af1f4 and 0x1010af200 (the total and the connect
/// timeout).
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(30);

/// `hydra::update::keep` wraps the release lookup in
/// `tokio::time::timeout(60s, look(...))` (0x10009ed68) and reports
/// `"no answer in {} seconds"`.
pub const LOOK_TIMEOUT: Duration = Duration::from_secs(60);

/// The download is wrapped in `timeout(15 minutes, fetch(...))` (0x10009ea28,
/// `"not fetched in {} minutes"`).
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// The most bytes [`body`] will accept for one download.
///
/// `// UNKNOWN:` the binary's guard is visible only as the format string
/// `"{} is larger than any release of Hydra's"` in
/// `crates/hydra-update/src/lib.rs`'s group (`__cstring` 0x103e2580b); the
/// number it compares against is not a literal we could find — it may come from
/// `Content-Length`, which a [`Transport`] need not report.  Half a gigabyte is
/// far above anything Hydra has published and still refuses a runaway response.
pub const MOST: u64 = 512 * 1024 * 1024;

/// Where a release download comes from.
///
/// `reqwest::Client` in the original: `Releases::new` keeps one, `look` and
/// `fetch` use it, and it sends [`USER_AGENT`] with a [`CLIENT_TIMEOUT`].
/// Implementations must be honest about failure — every `Err` here becomes
/// `"The update stopped: {}"`.
pub trait Transport {
    /// `GET url`, whole body.
    fn get(&self, url: &str) -> Result<Vec<u8>, String>;
}

/// The transport that ships: it refuses everything, so an unfurnished
/// `Releases` cannot reach the network.
#[derive(Debug, Clone, Copy, Default)]
pub struct Offline;

impl Transport for Offline {
    fn get(&self, url: &str) -> Result<Vec<u8>, String> {
        Err(format!("no transport is installed, so {url} was not fetched"))
    }
}

/// The updater: the release list, the checksums and the swap.
///
/// The binary's `Releases` is one word — the `reqwest::Client` (`Arc`) — stored
/// at `hydra::update::keep`'s state+0x108 and handed to `look` as `&self`
/// (0x10009ed48).
pub struct Releases {
    client: Box<dyn Transport>,
}

impl Default for Releases {
    fn default() -> Self {
        Releases { client: Box::new(Offline) }
    }
}

impl Releases {
    /// `Releases::new` @0x1010af1bc: build the client.
    ///
    /// The original can fail here (`ClientBuilder::build` returning an error →
    /// `"Hydra cannot reach the releases: {}"`); with no client to build this
    /// only ever succeeds, and the failure moves to the first request.
    pub fn new() -> Result<Releases, String> {
        Ok(Releases::default())
    }

    /// The same updater over an explicit transport — how a networked build (or
    /// a test) supplies one.
    pub fn with(client: Box<dyn Transport>) -> Releases {
        Releases { client }
    }

    pub fn transport(&self) -> &dyn Transport {
        self.client.as_ref()
    }

    /// Read the release list and pick the newest release that is newer than
    /// `running` and carries the three assets (the archive named `asset`,
    /// `SHA256SUMS.txt` and `SHA256SUMS.txt.minisig`).
    ///
    /// `// UNKNOWN:` `url` is a parameter here — see the module note.  The
    /// original's `look` took `(&self, &Version)` only and was wrapped in
    /// [`LOOK_TIMEOUT`] by its caller; it returned
    /// `Result<Option<Offer>, String>`.
    pub fn look(&self, url: &str, running: &Version, asset: &str) -> Result<Option<Offer>, String> {
        let releases = text(self.client.as_ref(), url)?;
        release::newest(&releases, running, asset)
    }

    /// Fetch the release, prove it, and put it in place.
    ///
    /// `Releases::fetch`'s generator (inlined into
    /// `Timeout::poll` @0x10025e16c) in order: `text` the checksums
    /// (0x10025e2d8), `text` the signature (0x10025e38c),
    /// `Sums::checked(checksums, signature, &version.to_string(), KEY)`
    /// (0x10025e424), then `body` the archive and inside it
    /// `Sums::check(asset, bytes)` → `swap::unpack(bytes, entry)` →
    /// `swap::put_in_place(place, &bytes)` (0x100217edc).
    ///
    /// `asset` is the archive's name as `SHA256SUMS.txt` spells it, `entry` is
    /// the member inside the archive, and `place` is the file to replace.
    pub fn fetch(
        &self,
        offer: &Offer,
        asset: &str,
        entry: &str,
        place: &str,
    ) -> Result<(), String> {
        self.fetch_with(&signed::Unavailable, offer, asset, entry, place)
    }

    /// [`Releases::fetch`] with an explicit [`signed::Verifier`] — the path a
    /// build with the curve arithmetic takes, and the only way to reach the
    /// checksum and swap steps.
    pub fn fetch_with<V: signed::Verifier + ?Sized>(
        &self,
        verifier: &V,
        offer: &Offer,
        asset: &str,
        entry: &str,
        place: &str,
    ) -> Result<(), String> {
        let sums_text = text(self.client.as_ref(), &offer.sums.url)?;
        let signature = text(self.client.as_ref(), &offer.signature.url)?;
        let version = offer.version.to_string();
        let sums = signed::Sums::checked_with(
            verifier,
            sums_text.as_bytes(),
            &signature,
            &version,
            KEY,
        )?;
        body(self.client.as_ref(), &offer.archive.url, |bytes| {
            sums.check(asset, bytes)?;
            let unpacked = swap::unpack(bytes, entry)?;
            swap::put_in_place(place, &unpacked)
        })
    }
}

/// `hydra_update::text`: the body of `url` as text.
///
/// The binary's `text` is an `async fn` whose generator is
/// `hydra_update::text::{{closure}}` @0x100240204 — `IntoUrl` (0x101cc4ac4),
/// `RequestBuilder::new` (0x101cb06cc), `send` (0x101cb08b0),
/// `error_for_status` (0x101ce60dc), then `hydra_update::body` with a closure
/// that insists on UTF-8.
pub fn text(client: &dyn Transport, url: &str) -> Result<String, String> {
    body(client, url, |bytes| {
        String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url} is not text"))
    })
}

/// `hydra_update::body`: the body of `url`, handed to `then`.
///
/// Both callers in the binary are instantiations of one generic — the
/// `drop_in_place<hydra_update::body<…>>` for `text`'s closure and for
/// `hydra::update::fetch`'s closure — which is why the reading, the size guard
/// and `"The update stopped: {}"` live here rather than in each caller.
pub fn body<T>(
    client: &dyn Transport,
    url: &str,
    then: impl FnOnce(&[u8]) -> Result<T, String>,
) -> Result<T, String> {
    let response = client
        .get(url)
        .map_err(|e| format!("The update stopped: {e}"))?;
    if response.len() as u64 > MOST {
        // `// UNKNOWN:` see [`MOST`]; the message is the binary's.
        return Err(format!(
            "{} is larger than any release of Hydra's",
            response.len()
        ));
    }
    then(&response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A transport that hands out canned bodies, in order.
    struct Canned(RefCell<VecDeque<(String, Vec<u8>)>>);

    impl Canned {
        fn new(pages: &[(&str, &[u8])]) -> Box<Canned> {
            Box::new(Canned(RefCell::new(
                pages
                    .iter()
                    .map(|(url, body)| (url.to_string(), body.to_vec()))
                    .collect(),
            )))
        }
    }

    impl Transport for Canned {
        fn get(&self, url: &str) -> Result<Vec<u8>, String> {
            let mut pages = self.0.borrow_mut();
            let page = pages
                .pop_front()
                .ok_or_else(|| format!("nothing left for {url}"))?;
            if page.0 != url {
                return Err(format!("asked for {url}, had {}", page.0));
            }
            Ok(page.1)
        }
    }

    /// A transport that always fails, like a network that is down.
    struct Broken;

    impl Transport for Broken {
        fn get(&self, _: &str) -> Result<Vec<u8>, String> {
            Err("dns error: no such host".to_string())
        }
    }

    /// The release document, trimmed to the keys Hydra reads.
    const RELEASES: &str = r#"[
      {"tag_name": "v0.1.0-beta.8", "draft": false, "html_url": "https://example.invalid/r/beta.8",
       "assets": [
         {"name": "hydra-arm64", "browser_download_url": "https://example.invalid/dl/hydra-arm64", "size": 42},
         {"name": "SHA256SUMS.txt", "browser_download_url": "https://example.invalid/dl/sums", "size": 99},
         {"name": "SHA256SUMS.txt.minisig", "browser_download_url": "https://example.invalid/dl/sums.minisig", "size": 344}
       ]}
    ]"#;

    #[test]
    fn looks_up_the_newest_release_over_the_transport() {
        let releases = Releases::with(Canned::new(&[(
            "https://example.invalid/releases",
            RELEASES.as_bytes(),
        )]));
        let running = Version::parse(VERSION).unwrap();
        let offer = releases
            .look("https://example.invalid/releases", &running, "hydra-arm64")
            .expect("reads")
            .expect("offers");
        assert_eq!(offer.tag_name, "v0.1.0-beta.8");
        assert_eq!(offer.archive.url, "https://example.invalid/dl/hydra-arm64");
        assert_eq!(offer.archive.size, 42);
    }

    #[test]
    fn nothing_newer_is_not_an_error() {
        let releases = Releases::with(Canned::new(&[(
            "https://example.invalid/releases",
            RELEASES.as_bytes(),
        )]));
        let running = Version::parse("0.1.0-beta.8").unwrap();
        assert!(releases
            .look("https://example.invalid/releases", &running, "hydra-arm64")
            .unwrap()
            .is_none());
    }

    #[test]
    fn the_default_updater_is_offline() {
        let releases = Releases::new().expect("builds");
        let running = Version::parse(VERSION).unwrap();
        let error = releases
            .look("https://example.invalid/releases", &running, "hydra-arm64")
            .unwrap_err();
        assert_eq!(
            error,
            "The update stopped: no transport is installed, so \
             https://example.invalid/releases was not fetched"
        );
    }

    #[test]
    fn a_dead_network_says_so() {
        let releases = Releases::with(Box::new(Broken));
        let error = text(releases.transport(), "https://example.invalid/x").unwrap_err();
        assert_eq!(error, "The update stopped: dns error: no such host");
    }

    #[test]
    fn text_insists_on_utf8() {
        let releases = Releases::with(Canned::new(&[(
            "https://example.invalid/bin",
            &[0x50, 0x4b, 0x03, 0x04, 0xff, 0xfe],
        )]));
        let error = text(releases.transport(), "https://example.invalid/bin").unwrap_err();
        assert_eq!(error, "https://example.invalid/bin is not text");
    }

    #[test]
    fn body_hands_the_bytes_over() {
        let releases = Releases::with(Canned::new(&[("https://example.invalid/x", b"hello")]));
        let handed = body(releases.transport(), "https://example.invalid/x", |bytes| {
            Ok(bytes.to_vec())
        })
        .unwrap();
        assert_eq!(handed, b"hello");
    }

    /// The end-to-end path, which must stop at the signature: no verifier
    /// ships, so a release is never installed on trust.
    #[test]
    fn fetch_refuses_a_release_it_cannot_prove() {
        let offer = Offer {
            tag_name: "v0.1.0-beta.8".to_string(),
            html_url: String::new(),
            archive: release::Asset {
                url: "https://example.invalid/dl/hydra-arm64".to_string(),
                size: 42,
            },
            sums: release::Asset {
                url: "https://example.invalid/dl/sums".to_string(),
                size: 99,
            },
            signature: release::Asset {
                url: "https://example.invalid/dl/sums.minisig".to_string(),
                size: 344,
            },
            version: Version::parse("0.1.0-beta.8").unwrap(),
        };
        let releases = Releases::with(Canned::new(&[
            ("https://example.invalid/dl/sums", b"whatever\n"),
            ("https://example.invalid/dl/sums.minisig", b"untrusted\n"),
        ]));
        let error = releases
            .fetch(&offer, "hydra-arm64", "hydra", "/tmp/hydra")
            .unwrap_err();
        // The truncated signature file cannot be decoded at all.
        assert!(
            error.starts_with("The checksums' signature cannot be read: "),
            "{error}"
        );
    }

    /// The archive `swap::unpack` can open (see `zip::tests`): a stored
    /// `stored.txt` and a deflated `hydra` holding "the quick brown fox.\n".
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

    /// The SHA-256 of `ARCHIVE`, i.e. what a `SHA256SUMS.txt` line for the
    /// archive named `hydra-arm64` would carry.
    const ARCHIVE_SUM: &str =
        "9d53b7c6c1cd88f6829c41c5b52137dce0c2e749c4a3616b72444035de04060a";

    /// A verifier a test can install, standing in for the Ed25519 check.
    struct Agreeing;
    impl signed::Verifier for Agreeing {
        fn verify(
            &self,
            _: &signed::PublicKey,
            _: &signed::Signature,
            _: &[u8],
        ) -> Result<(), String> {
            Ok(())
        }
    }

    /// A `.minisig` file whose key id is the release key's, so everything but
    /// the curve arithmetic lines up.
    fn minisig(trusted: &str) -> String {
        let key = signed::decode_public_key(KEY).expect("the release key parses");
        let mut raw = vec![b'E', b'D'];
        raw.extend_from_slice(&key.key_id);
        raw.extend(std::iter::repeat_n(0x11, 64));
        format!(
            "untrusted comment: signature from minisign secret key\n{}\ntrusted comment: {}\n{}\n",
            base64_text(&raw),
            trusted,
            base64_text(&[0x22u8; 64]),
        )
    }

    fn base64_text(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let triple = ((chunk[0] as u32) << 16)
                | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
                | *chunk.get(2).unwrap_or(&0) as u32;
            out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
            out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(triple >> 6) as usize & 0x3f] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[triple as usize & 0x3f] as char
            } else {
                '='
            });
        }
        out
    }

    fn offer(version: &str) -> Offer {
        Offer {
            tag_name: format!("v{version}"),
            html_url: "https://example.invalid/r".to_string(),
            archive: release::Asset {
                url: "https://example.invalid/dl/hydra-arm64".to_string(),
                size: ARCHIVE.len() as u64,
            },
            sums: release::Asset {
                url: "https://example.invalid/dl/sums".to_string(),
                size: 130,
            },
            signature: release::Asset {
                url: "https://example.invalid/dl/sums.minisig".to_string(),
                size: 344,
            },
            version: Version::parse(version).unwrap(),
        }
    }

    /// A private directory under the system temp dir, removed on drop.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let path = std::env::temp_dir().join(format!(
                "hydra-update-fetch-{}-{tag}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("scratch dir");
            Scratch(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The whole chain with a verifier installed: fetch the checksums and their
    /// signature, parse them, hash the archive against them, unpack the member
    /// and swap it into place.
    #[test]
    fn fetch_installs_the_member_it_verified() {
        let scratch = Scratch::new("ok");
        let place = scratch.0.join("hydra");
        std::fs::write(&place, b"the old binary\n").unwrap();

        let sums = format!("{ARCHIVE_SUM}  hydra-arm64\n");
        let releases = Releases::with(Canned::new(&[
            ("https://example.invalid/dl/sums", sums.as_bytes()),
            (
                "https://example.invalid/dl/sums.minisig",
                minisig("Hydra 0.1.0-beta.8").as_bytes(),
            ),
            ("https://example.invalid/dl/hydra-arm64", ARCHIVE),
        ]));
        releases
            .fetch_with(
                &Agreeing,
                &offer("0.1.0-beta.8"),
                "hydra-arm64",
                "hydra",
                place.to_str().unwrap(),
            )
            .expect("installs");
        assert_eq!(std::fs::read(&place).unwrap(), b"the quick brown fox.\n");
        assert!(!scratch.0.join("hydra.new").exists());
        assert!(!scratch.0.join("hydra.old").exists());
    }

    /// A checksum that does not match the archive stops the install before
    /// anything is written.
    #[test]
    fn fetch_refuses_an_archive_that_is_not_the_one_signed() {
        let scratch = Scratch::new("badsum");
        let place = scratch.0.join("hydra");
        std::fs::write(&place, b"the old binary\n").unwrap();

        let sums = "0000000000000000000000000000000000000000000000000000000000000000  hydra-arm64\n";
        let releases = Releases::with(Canned::new(&[
            ("https://example.invalid/dl/sums", sums.as_bytes()),
            (
                "https://example.invalid/dl/sums.minisig",
                minisig("Hydra 0.1.0-beta.8").as_bytes(),
            ),
            ("https://example.invalid/dl/hydra-arm64", ARCHIVE),
        ]));
        let error = releases
            .fetch_with(
                &Agreeing,
                &offer("0.1.0-beta.8"),
                "hydra-arm64",
                "hydra",
                place.to_str().unwrap(),
            )
            .unwrap_err();
        assert_eq!(
            error,
            "hydra-arm64 is not the file the release signed: its hash differs"
        );
        assert_eq!(std::fs::read(&place).unwrap(), b"the old binary\n");
        assert!(!scratch.0.join("hydra.new").exists());
    }

    /// The signature must name this version.
    #[test]
    fn fetch_refuses_checksums_signed_for_another_version() {
        let scratch = Scratch::new("version");
        let place = scratch.0.join("hydra");
        let sums = format!("{ARCHIVE_SUM}  hydra-arm64\n");
        let releases = Releases::with(Canned::new(&[
            ("https://example.invalid/dl/sums", sums.as_bytes()),
            (
                "https://example.invalid/dl/sums.minisig",
                minisig("Hydra 0.1.0-beta.9").as_bytes(),
            ),
        ]));
        let error = releases
            .fetch_with(
                &Agreeing,
                &offer("0.1.0-beta.8"),
                "hydra-arm64",
                "hydra",
                place.to_str().unwrap(),
            )
            .unwrap_err();
        assert_eq!(
            error,
            "The checksums are signed for \"Hydra 0.1.0-beta.9\", not Hydra 0.1.0-beta.8"
        );
    }

    /// The user agent and the timeouts the original client carried.
    #[test]
    fn carries_the_recorded_client_settings() {
        assert_eq!(USER_AGENT, "hydra/0.1.0-beta.7");
        assert_eq!(USER_AGENT.len(), 18);
        assert_eq!(VERSION, "0.1.0-beta.7");
        assert_eq!(KEY.len(), 56);
        assert_eq!(CLIENT_TIMEOUT, Duration::from_secs(30));
        assert_eq!(LOOK_TIMEOUT, Duration::from_secs(60));
        assert_eq!(FETCH_TIMEOUT, Duration::from_secs(900));
    }
}
