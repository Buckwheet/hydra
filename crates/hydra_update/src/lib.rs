//! `hydra_update` — finding a newer Hydra release, proving it is the one its
//! signature names, and swapping it into place.
//!
//! The original crate talked to GitHub with `reqwest` on a `tokio` runtime,
//! parsed the release list with `serde_json`, parsed release tags with
//! `semver`, unpacked the archive with `zip` and verified with
//! `minisign_verify`.  **Phase 1** reconnected all of that at the workspace
//! pins except the curve arithmetic; `NOTES.md` §5 has the version table.
//!
//! What that changes, and what it does not:
//!
//! * **the network is real.**  [`Releases::new`] builds the recovered
//!   [`reqwest::Client`] ([`USER_AGENT`], [`CLIENT_TIMEOUT`] total *and*
//!   connect) and installs it as the default [`Transport`].  The seam is still
//!   there — [`Transport`] is one boxed-future method wide, so tests (and a
//!   caller that must not reach the network) can supply [`Offline`] — but the
//!   shipped transport is [`Http`], not an always-failing stub.
//! * **the signature math is still a seam.**  [`signed::Verifier`] is called
//!   only after the key, the signature and the trusted comment have been parsed
//!   and checked; `minisign-verify` is not a workspace pin, so the built-in
//!   verifier [`signed::Unavailable`] refuses everything.  Nothing is ever
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

/// Fixtures the tests share (a real ZIP built with Python's `zipfile`, and the
/// `zip` writer that builds fresh ones).
#[cfg(test)]
mod testdata;

use release::{Offer, Version};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
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

/// The future a [`Transport`] hands back.
///
/// The seam has to stay **object safe** (the binary's `Releases` holds a
/// `dyn` client, and the tests pass their own transport in), so the trait
/// cannot use `async fn` itself in edition 2021; a boxed `+ Send` future is the
/// explicit form of what `async fn` in the trait would desugar to.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where a release download comes from.
///
/// In the original this was a `reqwest::Client` held by `Releases`: `look` and
/// `fetch` used it, and it sent [`USER_AGENT`] with a [`CLIENT_TIMEOUT`].
/// Implementations must be honest about failure — every `Err` here becomes
/// `"The update stopped: {}"`.
pub trait Transport: Send + Sync {
    /// `GET url`, whole body.  Implementations report transport *and* HTTP
    /// failures: the recovered `text` generator called `error_for_status`
    /// (0x101ce60dc) right after `send`.
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>>;

    /// The `reqwest` client behind this transport, when there is one.
    ///
    /// `reqwest::Client` exposes no getters, so this is how a test can see that
    /// the shipped [`Releases::new`] really installed the recovered client
    /// settings instead of a stub.  `None` for every other transport.
    fn reqwest(&self) -> Option<&reqwest::Client> {
        None
    }
}

/// A transport that refuses everything, so an updater furnished with it cannot
/// reach the network.
///
/// **Not** what ships — [`Releases::new`] installs [`Http`].  This exists so the
/// refusal path stays testable, and so a caller that must stay offline can say
/// `Releases::with(Arc::new(Offline))` out loud.
#[derive(Debug, Clone, Copy, Default)]
pub struct Offline;

impl Transport for Offline {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        let refusal = format!("no transport is installed, so {url} was not fetched");
        Box::pin(async move { Err(refusal) })
    }
}

/// The [`reqwest::ClientBuilder`] `Releases::new` builds (@0x1010af1bc, 208 B).
///
/// * `user_agent("hydra/0.1.0-beta.7")` — [`USER_AGENT`], set at 0x1010af1e0;
/// * `timeout(30s)` and `connect_timeout(30s)` — the two `Some(30s)` at
///   0x1010af1f4 / 0x1010af200.
///
/// reqwest 0.12 has no `blocking` feature here on purpose: it needs
/// `futures-io`, which is not in the offline registry.
pub fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(CLIENT_TIMEOUT)
        .connect_timeout(CLIENT_TIMEOUT)
}

/// The transport that ships: the real `reqwest` client.
#[derive(Debug, Clone)]
pub struct Http {
    client: reqwest::Client,
}

impl Http {
    /// Build the recovered client — `Releases::new`'s only step.
    ///
    /// The original could fail here (`ClientBuilder::build` → an error), which
    /// is why `Releases::new` returns a `Result`.
    pub fn new() -> Result<Http, String> {
        let client = client_builder()
            .build()
            .map_err(|e| format!("Hydra cannot reach the releases: {e}"))?;
        Ok(Http { client })
    }

    /// Wrap a client the caller built (tests, or a caller with its own proxy
    /// settings).
    pub fn with_client(client: reqwest::Client) -> Http {
        Http { client }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// One GET with the recovered settings applied to the request itself.
    ///
    /// `reqwest` copies the builder's default headers and timeout onto the
    /// request only at `execute` time (`Client::execute_request` inserts
    /// `self.inner.headers` and the total timeout), so nothing about a built
    /// `Request` shows them.  Setting them here as well is the same two values
    /// — one `user-agent` header and one 30 s total timeout — and it makes them
    /// observable without a network round trip.
    pub fn request(&self, url: &str) -> reqwest::RequestBuilder {
        self.client
            .get(url)
            .header(reqwest::header::USER_AGENT, USER_AGENT)
            .timeout(CLIENT_TIMEOUT)
    }
}

impl Transport for Http {
    fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>> {
        Box::pin(async move {
            // `text`'s recovered generator: `send` (0x101cb08b0) then
            // `error_for_status` (0x101ce60dc).
            let response = self
                .request(url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let body = response.bytes().await.map_err(|e| e.to_string())?;
            Ok(body.to_vec())
        })
    }

    fn reqwest(&self) -> Option<&reqwest::Client> {
        Some(&self.client)
    }
}

/// The updater: the release list, the checksums and the swap.
///
/// The binary's `Releases` is one word — the `reqwest::Client` (`Arc`) — stored
/// at `hydra::update::keep`'s state+0x108 and handed to `look` as `&self`
/// (0x10009ed48).  The same one word is kept here: an `Arc<dyn Transport>`.
pub struct Releases {
    client: Arc<dyn Transport>,
}

impl Releases {
    /// `Releases::new` @0x1010af1bc: build the client.
    ///
    /// Can fail, as the original could (`ClientBuilder::build` erroring), with
    /// `"Hydra cannot reach the releases: {}"`.
    pub fn new() -> Result<Releases, String> {
        Ok(Releases { client: Arc::new(Http::new()?) })
    }

    /// The same updater over an explicit transport — how a test, or a caller
    /// that must stay off the network, supplies one.
    pub fn with(client: Arc<dyn Transport>) -> Releases {
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
    /// original's `look` was an `async fn` taking `(&self, &Version)` only and
    /// was wrapped in [`LOOK_TIMEOUT`] by its caller; it returned
    /// `Result<Option<Offer>, String>`.
    pub async fn look(
        &self,
        url: &str,
        running: &Version,
        asset: &str,
    ) -> Result<Option<Offer>, String> {
        let releases = text(self.client.as_ref(), url).await?;
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
    pub async fn fetch(
        &self,
        offer: &Offer,
        asset: &str,
        entry: &str,
        place: &str,
    ) -> Result<(), String> {
        self.fetch_with(&signed::Unavailable, offer, asset, entry, place)
            .await
    }

    /// [`Releases::fetch`] with an explicit [`signed::Verifier`] — the path a
    /// build that can do the curve arithmetic takes, and the only way to reach
    /// the checksum and swap steps.
    pub async fn fetch_with<V>(
        &self,
        verifier: &V,
        offer: &Offer,
        asset: &str,
        entry: &str,
        place: &str,
    ) -> Result<(), String>
    where
        V: signed::Verifier + ?Sized + Sync,
    {
        let sums_text = text(self.client.as_ref(), &offer.sums.url).await?;
        let signature = text(self.client.as_ref(), &offer.signature.url).await?;
        let version = offer.version.to_string();
        let sums =
            signed::Sums::checked_with(verifier, sums_text.as_bytes(), &signature, &version, KEY)?;
        body(self.client.as_ref(), &offer.archive.url, |bytes| {
            sums.check(asset, bytes)?;
            let unpacked = swap::unpack(bytes, entry)?;
            swap::put_in_place(place, &unpacked)
        })
        .await
    }
}

/// `hydra_update::text`: the body of `url` as text.
///
/// The binary's `text` is an `async fn` whose generator is
/// `hydra_update::text::{{closure}}` @0x100240204 — `IntoUrl` (0x101cc4ac4),
/// `RequestBuilder::new` (0x101cb06cc), `send`, `error_for_status` (inside the
/// transport here), then `hydra_update::body` with a closure that insists on
/// UTF-8.
pub async fn text(client: &dyn Transport, url: &str) -> Result<String, String> {
    body(client, url, |bytes| {
        String::from_utf8(bytes.to_vec()).map_err(|_| format!("{url} is not text"))
    })
    .await
}

/// `hydra_update::body`: the body of `url`, handed to `then`.
///
/// Both callers in the binary are instantiations of one generic — the
/// `drop_in_place<hydra_update::body<…>>` for `text`'s closure and for
/// `hydra::update::fetch`'s closure — which is why the reading, the size guard
/// and `"The update stopped: {}"` live here rather than in each caller.
///
/// `then` is `Send` so the returned future is too: `hydra::update::keep` awaits
/// `fetch` inside `tokio::time::timeout`, and keeping these futures spawnable
/// costs nothing.
pub async fn body<T>(
    client: &dyn Transport,
    url: &str,
    then: impl FnOnce(&[u8]) -> Result<T, String> + Send,
) -> Result<T, String> {
    let response = client
        .get(url)
        .await
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
    use crate::testdata::{ARCHIVE, ARCHIVE_SUM};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    /// A transport that hands out canned bodies, in order.
    struct Canned(Mutex<VecDeque<(String, Vec<u8>)>>);

    impl Canned {
        fn new(pages: &[(&str, &[u8])]) -> Arc<Canned> {
            Arc::new(Canned(Mutex::new(
                pages
                    .iter()
                    .map(|(url, body)| (url.to_string(), body.to_vec()))
                    .collect(),
            )))
        }
    }

    impl Transport for Canned {
        fn get<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>> {
            let page = self
                .0
                .lock()
                .expect("the canned list")
                .pop_front()
                .ok_or_else(|| format!("nothing left for {url}"));
            Box::pin(async move {
                let page = page?;
                if page.0 != url {
                    return Err(format!("asked for {url}, had {}", page.0));
                }
                Ok(page.1)
            })
        }
    }

    /// A transport that always fails, like a network that is down.
    struct Broken;

    impl Transport for Broken {
        fn get<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Result<Vec<u8>, String>> {
            Box::pin(async { Err("dns error: no such host".to_string()) })
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

    #[tokio::test]
    async fn looks_up_the_newest_release_over_the_transport() {
        let releases = Releases::with(Canned::new(&[(
            "https://example.invalid/releases",
            RELEASES.as_bytes(),
        )]));
        let running = Version::parse(VERSION).unwrap();
        let offer = releases
            .look("https://example.invalid/releases", &running, "hydra-arm64")
            .await
            .expect("reads")
            .expect("offers");
        assert_eq!(offer.tag_name, "v0.1.0-beta.8");
        assert_eq!(offer.archive.url, "https://example.invalid/dl/hydra-arm64");
        assert_eq!(offer.archive.size, 42);
    }

    #[tokio::test]
    async fn nothing_newer_is_not_an_error() {
        let releases = Releases::with(Canned::new(&[(
            "https://example.invalid/releases",
            RELEASES.as_bytes(),
        )]));
        let running = Version::parse("0.1.0-beta.8").unwrap();
        assert!(releases
            .look("https://example.invalid/releases", &running, "hydra-arm64")
            .await
            .unwrap()
            .is_none());
    }

    /// The default updater is the real client, built with the recovered
    /// settings.  No request leaves the process: only the request *builder* is
    /// inspected (header and timeout), never `send`.
    #[tokio::test]
    async fn the_default_updater_is_the_recorded_reqwest_client() {
        let releases = Releases::new().expect("builds the client");

        // The shipped transport is `Http`, not a stub.
        let client = releases
            .transport()
            .reqwest()
            .expect("the default transport is a reqwest client")
            .clone();

        // The requests it makes carry the recovered user agent and the 30-second
        // total timeout (`Http::get` builds them with `Http::request`).
        let request = Http::with_client(client)
            .request("https://example.invalid/releases")
            .build()
            .expect("builds a request");
        assert_eq!(
            request.headers()[reqwest::header::USER_AGENT],
            "hydra/0.1.0-beta.7"
        );
        assert_eq!(request.timeout(), Some(&CLIENT_TIMEOUT));
        assert_eq!(CLIENT_TIMEOUT, Duration::from_secs(30));

        // A built client from the recovered builder is usable too (and, again,
        // is never sent anything).
        client_builder().build().expect("the recovered builder builds");
    }

    /// An updater someone deliberately kept offline still refuses everything.
    #[tokio::test]
    async fn an_offline_updater_refuses_everything() {
        let releases = Releases::with(Arc::new(Offline));
        assert!(releases.transport().reqwest().is_none());
        let running = Version::parse(VERSION).unwrap();
        let error = releases
            .look("https://example.invalid/releases", &running, "hydra-arm64")
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "The update stopped: no transport is installed, so \
             https://example.invalid/releases was not fetched"
        );
    }

    #[tokio::test]
    async fn a_dead_network_says_so() {
        let releases = Releases::with(Arc::new(Broken));
        let error = text(releases.transport(), "https://example.invalid/x")
            .await
            .unwrap_err();
        assert_eq!(error, "The update stopped: dns error: no such host");
    }

    #[tokio::test]
    async fn text_insists_on_utf8() {
        let releases = Releases::with(Canned::new(&[(
            "https://example.invalid/bin",
            &[0x50, 0x4b, 0x03, 0x04, 0xff, 0xfe],
        )]));
        let error = text(releases.transport(), "https://example.invalid/bin")
            .await
            .unwrap_err();
        assert_eq!(error, "https://example.invalid/bin is not text");
    }

    #[tokio::test]
    async fn body_hands_the_bytes_over() {
        let releases = Releases::with(Canned::new(&[("https://example.invalid/x", b"hello")]));
        let handed = body(releases.transport(), "https://example.invalid/x", |bytes| {
            Ok(bytes.to_vec())
        })
        .await
        .unwrap();
        assert_eq!(handed, b"hello");
    }

    /// The futures are `Send`, as the original's were: `keep` drives `fetch`
    /// through `tokio::time::timeout` inside a spawned task.
    #[tokio::test]
    async fn the_async_paths_are_spawnable() {
        fn is_send<T: Send>(value: T) -> T {
            value
        }
        let releases = Releases::with(Arc::new(Offline));
        let running = Version::parse(VERSION).unwrap();
        let look = is_send(releases.look("https://example.invalid/x", &running, "hydra-arm64"));
        assert!(look.await.is_err());
        let offer = offer("0.1.0-beta.8");
        let fetch = is_send(releases.fetch(&offer, "hydra-arm64", "hydra", "/tmp/hydra"));
        assert!(fetch.await.is_err());
    }

    /// The end-to-end path, which must stop at the signature: no verifier
    /// ships, so a release is never installed on trust.
    #[tokio::test]
    async fn fetch_refuses_a_release_it_cannot_prove() {
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
            .await
            .unwrap_err();
        // The truncated signature file cannot be decoded at all.
        assert!(
            error.starts_with("The checksums' signature cannot be read: "),
            "{error}"
        );
    }

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
    #[tokio::test]
    async fn fetch_installs_the_member_it_verified() {
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
            .await
            .expect("installs");
        assert_eq!(std::fs::read(&place).unwrap(), b"the quick brown fox.\n");
        assert!(!scratch.0.join("hydra.new").exists());
        assert!(!scratch.0.join("hydra.old").exists());
    }

    /// A checksum that does not match the archive stops the install before
    /// anything is written.
    #[tokio::test]
    async fn fetch_refuses_an_archive_that_is_not_the_one_signed() {
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
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "hydra-arm64 is not the file the release signed: its hash differs"
        );
        assert_eq!(std::fs::read(&place).unwrap(), b"the old binary\n");
        assert!(!scratch.0.join("hydra.new").exists());
    }

    /// The signature must name this version.
    #[tokio::test]
    async fn fetch_refuses_checksums_signed_for_another_version() {
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
            .await
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
