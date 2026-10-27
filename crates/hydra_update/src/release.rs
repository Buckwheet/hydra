//! Release discovery: the JSON GitHub serves for a repository's releases, the
//! assets in it, and picking the newest one that is newer than the running
//! Hydra.
//!
//! Recovered from `hydra_update::release::newest` @0x1010adf74 (3480 B) and its
//! closure @0x1010ae95c (the per-release filter).  The struct field names and
//! their order come from the derive's string table in `__const` at
//! 0x1023fd534:
//!
//! ```text
//! Listed     tag_name §8, draft §5, html_url §8, assets §6   ("struct Listed with 4 elements")
//! ListedFile name §4, browser_download_url §20, size §4      ("struct ListedFile with 3 elements")
//! ```
//!
//! and the layout from `drop_in_place<Listed>` @0x1010b05a4 (Strings at 0x00
//! and 0x18, `Vec<ListedFile>` at 0x30 with a 0x38-byte stride, the `bool` last
//! at 0x48) and `drop_in_place<ListedFile>` @0x1010b0660 (Strings at 0x00 and
//! 0x18, `size` at 0x30).
//!
//! **Phase 1 reconnection.**  The two structs are again `serde` derives — which
//! is where the string table above comes from — `listed` is
//! `serde_json::from_str::<Vec<Listed>>` (the recovered `from_trait` @0x1010a0f70)
//! and [`Version`] is a newtype over `semver::Version` 1.0.28, so the recovered
//! comparison chain (`major`, `minor`, `patch`, `Prerelease`, then
//! `BuildMetadata`) is the crate's own `Ord` again instead of a reproduction.

use serde::{Deserialize, Serialize};

/// The checksum list the release publishes beside the archive (14 bytes,
/// compared word-for-word in `newest::{{closure}}` at 0x1010aead0).
pub const SUMS: &str = "SHA256SUMS.txt";

/// Its minisign signature (22 bytes, 0x1010aeb78).  minisign writes
/// `<file>.minisig`.
pub const SUMS_SIG: &str = "SHA256SUMS.txt.minisig";

/// One entry of `GET /releases` — only the four fields Hydra reads; every other
/// key GitHub sends is ignored, as `serde`'s derive does by default.
///
/// Fields are declared in the derive's order (the order the binary's
/// `expecting`/string table lists them), which is also the order they are
/// serialized in.  Nothing carries `#[serde(default)]`, so each one is
/// **required** — a release without `draft` is an error, not a `false`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    pub tag_name: String,
    pub draft: bool,
    pub html_url: String,
    pub assets: Vec<ListedFile>,
}

/// One downloadable asset of a release.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ListedFile {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}

/// A download offered by a release: where it is and how big it claims to be.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Asset {
    pub url: String,
    pub size: u64,
}

/// What `newest` hands back: the release to install, with the three downloads
/// it needs and the version to expect in the signature's trusted comment.
///
/// Field order follows the binary's layout (`drop_in_place<Offer>`
/// @0x1010aee5c: five `String`s at 0x00/0x18/0x30/0x50/0x70, the semver
/// `Version`'s two `Identifier`s at 0x90/0x98; `Offer` is 0xb8 bytes).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Offer {
    pub tag_name: String,
    pub html_url: String,
    /// The release archive for this platform (the asset named by the caller).
    pub archive: Asset,
    /// `SHA256SUMS.txt`.
    pub sums: Asset,
    /// `SHA256SUMS.txt.minisig`.
    pub signature: Asset,
    pub version: Version,
}

/// `semver::Version` (1.0.28), which is what the binary parsed release tags
/// with.
///
/// `newest` compares the release tag with the running version through
/// `semver::Version`'s ordering (the four inlined comparisons at
/// 0x1010ae16c–0x1010ae1cc: major, minor, patch, then `Prerelease`, then
/// `BuildMetadata`), which is exactly what `semver::Version`'s derived `Ord`
/// does — the fields are compared in declaration order with the two
/// `Identifier`s' own comparisons.  The newtype keeps the recovered path
/// (`release::Version`) and the recovered `as_str` while leaving the parsing
/// and the ordering to the real crate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(semver::Version);

impl Version {
    /// `semver::Version::parse`.  The binary's call site
    /// (`hydra::update::keep` @0x10009e744) parses `"0.1.0-beta.7"`.
    ///
    /// The error text is `semver`'s own (`"unexpected character 'v' while
    /// parsing major version number"`); the hand-rolled parser had its own
    /// wording, which said the same thing less precisely.
    pub fn parse(text: &str) -> Result<Version, String> {
        semver::Version::parse(text)
            .map(Version)
            .map_err(|e| e.to_string())
    }

    /// A version with no pre-release and no build metadata.
    pub fn new(major: u64, minor: u64, patch: u64) -> Version {
        Version(semver::Version::new(major, minor, patch))
    }

    /// The text minisign's trusted comment is compared with: `newest`'s caller
    /// builds `format!("Hydra {}", version)`.
    pub fn as_str(&self) -> String {
        self.to_string()
    }

    /// The `semver` value underneath.
    pub fn semver(&self) -> &semver::Version {
        &self.0
    }
}

impl Default for Version {
    fn default() -> Self {
        Version::new(0, 0, 0)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Field access onto `semver::Version` (`version.major`, `version.pre`,
/// `version.build`), which is what the hand-written struct's public fields gave
/// the rest of the crate.
impl std::ops::Deref for Version {
    type Target = semver::Version;

    fn deref(&self) -> &semver::Version {
        &self.0
    }
}

impl From<semver::Version> for Version {
    fn from(version: semver::Version) -> Version {
        Version(version)
    }
}

impl From<Version> for semver::Version {
    fn from(version: Version) -> semver::Version {
        version.0
    }
}

/// Read `Vec<Listed>` out of the releases document.
///
/// `serde_json::from_str::<Vec<Listed>>` — the recovered
/// `serde_json::de::from_trait` — so every field of `Listed`/`ListedFile` is
/// required and a missing or mistyped one is `serde`'s own error
/// (`"missing field `draft` at line 1 column 20"`).
pub fn listed(text: &str) -> Result<Vec<Listed>, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Pick the newest release to install, or `None` when every release is older
/// than (or the same as) the running Hydra.
///
/// Recovered from `newest` @0x1010adf74: parse the document
/// (`serde_json::from_str::<Vec<Listed>>`, error →
/// `"The releases could not be read: {}"`), then for each entry
///
/// * skip `draft` releases (the `bool` at record+0x48, 0x1010ae0b0),
/// * strip one leading `'v'` (0x1010ae994) and parse the rest as a version
///   (0x1010a9b0), skipping the entry if either fails,
/// * find the three assets by exact name — the archive the caller asked for,
///   `SHA256SUMS.txt` and `SHA256SUMS.txt.minisig` — skipping the entry if any
///   is missing (0x1010a9d0, 0x1010aeac0, 0x1010aeb68),
/// * skip it unless its version compares *greater* than the running version
///   (0x1010ae1d0 keeps only `Ordering::Greater`),
///
/// and keep the greatest of the rest.
pub fn newest(releases: &str, running: &Version, asset: &str) -> Result<Option<Offer>, String> {
    let listed = listed(releases).map_err(|e| format!("The releases could not be read: {e}"))?;
    let mut best: Option<Offer> = None;
    for release in listed {
        if release.draft {
            continue;
        }
        let version = match release
            .tag_name
            .strip_prefix('v')
            .and_then(|rest| Version::parse(rest).ok())
        {
            Some(version) => version,
            None => continue,
        };
        let Some(archive) = find(&release.assets, asset) else {
            continue;
        };
        let Some(sums) = find(&release.assets, SUMS) else {
            continue;
        };
        let Some(signature) = find(&release.assets, SUMS_SIG) else {
            continue;
        };
        if version <= *running {
            continue;
        }
        let offer = Offer {
            tag_name: release.tag_name,
            html_url: release.html_url,
            archive,
            sums,
            signature,
            version,
        };
        if best.as_ref().is_none_or(|held| offer.version > held.version) {
            best = Some(offer);
        }
    }
    Ok(best)
}

fn find(assets: &[ListedFile], name: &str) -> Option<Asset> {
    assets
        .iter()
        .find(|file| file.name == name)
        .map(|file| Asset { url: file.browser_download_url.clone(), size: file.size })
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNNING: &str = "0.1.0-beta.7";

    /// A release document shaped like GitHub's (trimmed to the keys read).
    const SAMPLE: &str = r#"[
      {
        "url": "https://api.example.invalid/repos/nisugi/hydra/releases/1",
        "tag_name": "v0.1.0-beta.9",
        "draft": false,
        "prerelease": true,
        "html_url": "https://example.invalid/nisugi/hydra/releases/tag/v0.1.0-beta.9",
        "assets": [
          {"name": "SHA256SUMS.txt.minisig",
           "browser_download_url": "https://example.invalid/dl/beta.9/SHA256SUMS.txt.minisig",
           "size": 344},
          {"name": "not-the-archive",
           "browser_download_url": "https://example.invalid/dl/beta.9/other",
           "size": 1},
          {"name": "SHA256SUMS.txt",
           "browser_download_url": "https://example.invalid/dl/beta.9/SHA256SUMS.txt",
           "size": 130},
          {"name": "hydra-arm64",
           "browser_download_url": "https://example.invalid/dl/beta.9/hydra-arm64",
           "size": 8123456}
        ]
      },
      {
        "tag_name": "v0.1.0-beta.8",
        "draft": false,
        "html_url": "https://example.invalid/nisugi/hydra/releases/tag/v0.1.0-beta.8",
        "assets": [
          {"name": "hydra-arm64", "browser_download_url": "https://example.invalid/dl/beta.8/hydra-arm64", "size": 8000000},
          {"name": "SHA256SUMS.txt", "browser_download_url": "https://example.invalid/dl/beta.8/SHA256SUMS.txt", "size": 130},
          {"name": "SHA256SUMS.txt.minisig", "browser_download_url": "https://example.invalid/dl/beta.8/SHA256SUMS.txt.minisig", "size": 344}
        ]
      },
      {
        "tag_name": "v0.2.0",
        "draft": true,
        "html_url": "https://example.invalid/nisugi/hydra/releases/tag/v0.2.0",
        "assets": [
          {"name": "hydra-arm64", "browser_download_url": "https://example.invalid/dl/draft", "size": 1},
          {"name": "SHA256SUMS.txt", "browser_download_url": "https://example.invalid/dl/draft-sums", "size": 1},
          {"name": "SHA256SUMS.txt.minisig", "browser_download_url": "https://example.invalid/dl/draft-sig", "size": 1}
        ]
      },
      {
        "tag_name": "not-a-version",
        "draft": false,
        "html_url": "https://example.invalid/nisugi/hydra/releases/tag/nope",
        "assets": []
      },
      {
        "tag_name": "v0.1.0-beta.6",
        "draft": false,
        "html_url": "https://example.invalid/nisugi/hydra/releases/tag/v0.1.0-beta.6",
        "assets": [
          {"name": "hydra-arm64", "browser_download_url": "https://example.invalid/dl/beta.6/hydra-arm64", "size": 7000000},
          {"name": "SHA256SUMS.txt", "browser_download_url": "https://example.invalid/dl/beta.6/SHA256SUMS.txt", "size": 130},
          {"name": "SHA256SUMS.txt.minisig", "browser_download_url": "https://example.invalid/dl/beta.6/SHA256SUMS.txt.minisig", "size": 344}
        ]
      }
    ]"#;

    fn running() -> Version {
        Version::parse(RUNNING).unwrap()
    }

    #[test]
    fn picks_the_newest_release_newer_than_hydra() {
        let offer = newest(SAMPLE, &running(), "hydra-arm64").unwrap().unwrap();
        assert_eq!(offer.tag_name, "v0.1.0-beta.9");
        assert_eq!(offer.version.to_string(), "0.1.0-beta.9");
        assert_eq!(
            offer.archive.url,
            "https://example.invalid/dl/beta.9/hydra-arm64"
        );
        assert_eq!(offer.archive.size, 8_123_456);
        assert_eq!(
            offer.sums.url,
            "https://example.invalid/dl/beta.9/SHA256SUMS.txt"
        );
        assert_eq!(offer.sums.size, 130);
        assert_eq!(
            offer.signature.url,
            "https://example.invalid/dl/beta.9/SHA256SUMS.txt.minisig"
        );
        assert_eq!(
            offer.html_url,
            "https://example.invalid/nisugi/hydra/releases/tag/v0.1.0-beta.9"
        );
    }

    #[test]
    fn skips_releases_that_are_not_newer() {
        // Running beta.8: only beta.9 is newer (0.2.0 is a draft, beta.6 is
        // older, and "not-a-version" is not a version at all).
        let running = Version::parse("0.1.0-beta.8").unwrap();
        let offer = newest(SAMPLE, &running, "hydra-arm64").unwrap().unwrap();
        assert_eq!(offer.tag_name, "v0.1.0-beta.9");

        // Running beta.9: nothing listed is newer — 0.2.0 is newer but a draft.
        let running = Version::parse("0.1.0-beta.9").unwrap();
        assert!(newest(SAMPLE, &running, "hydra-arm64").unwrap().is_none());

        // Newer than every listed release: nothing to offer.
        let running = Version::parse("0.1.0-beta.10").unwrap();
        assert!(newest(SAMPLE, &running, "hydra-arm64").unwrap().is_none());
    }

    #[test]
    fn needs_the_asset_to_be_named_exactly() {
        let running = Version::parse("0.0.0").unwrap();
        // No release has an asset by this name.
        assert!(newest(SAMPLE, &running, "hydra-macos").unwrap().is_none());
    }

    #[test]
    fn a_release_without_the_checksums_is_skipped() {
        let text = r#"[
          {"tag_name": "v9.0.0", "draft": false, "html_url": "u",
           "assets": [{"name": "hydra-arm64", "browser_download_url": "a", "size": 1}]},
          {"tag_name": "v1.0.0", "draft": false, "html_url": "u",
           "assets": [
             {"name": "hydra-arm64", "browser_download_url": "a", "size": 1},
             {"name": "SHA256SUMS.txt", "browser_download_url": "b", "size": 1},
             {"name": "SHA256SUMS.txt.minisig", "browser_download_url": "c", "size": 1}]}
        ]"#;
        let offer = newest(text, &running(), "hydra-arm64").unwrap().unwrap();
        assert_eq!(offer.tag_name, "v1.0.0");
    }

    #[test]
    fn unreadable_documents_are_reported() {
        let error = newest("not json", &running(), "hydra-arm64").unwrap_err();
        assert!(
            error.starts_with("The releases could not be read: "),
            "{error}"
        );
        let error = newest(r#"[{"tag_name": "v1.0.0"}]"#, &running(), "x").unwrap_err();
        assert!(error.contains("missing field `draft`"), "{error}");
        // `serde` says where, which the hand-written reader also tried to.
        assert!(error.contains("line 1 column"), "{error}");
    }

    /// The two documents `serde` reads are the recovered schema: the four (and
    /// three) field names, in the recovered order, and all of them required.
    #[test]
    fn the_schema_is_the_recovered_one() {
        let one = Listed {
            tag_name: "v0.1.0-beta.8".to_string(),
            draft: false,
            html_url: "https://example.invalid/r".to_string(),
            assets: vec![ListedFile {
                name: "hydra-arm64".to_string(),
                browser_download_url: "https://example.invalid/dl/hydra-arm64".to_string(),
                size: 42,
            }],
        };
        let text = serde_json::to_string(&one).expect("writes");
        assert_eq!(
            text,
            "{\"tag_name\":\"v0.1.0-beta.8\",\"draft\":false,\
             \"html_url\":\"https://example.invalid/r\",\"assets\":[{\"name\":\"hydra-arm64\",\
             \"browser_download_url\":\"https://example.invalid/dl/hydra-arm64\",\"size\":42}]}"
        );
        // Round trip: what is written is what is read.
        let back: Vec<Listed> = serde_json::from_str(&format!("[{text}]")).expect("reads");
        assert_eq!(back, vec![one]);

        // Every field is required — no `#[serde(default)]` anywhere.
        for missing in ["tag_name", "draft", "html_url", "assets"] {
            let document = match missing {
                "tag_name" => r#"[{"draft":false,"html_url":"u","assets":[]}]"#,
                "draft" => r#"[{"tag_name":"v1.0.0","html_url":"u","assets":[]}]"#,
                "html_url" => r#"[{"tag_name":"v1.0.0","draft":false,"assets":[]}]"#,
                _ => r#"[{"tag_name":"v1.0.0","draft":false,"html_url":"u"}]"#,
            };
            let error = listed(document).unwrap_err();
            assert_eq!(
                error.split(" at line").next().unwrap(),
                format!("missing field `{missing}`"),
                "{document}"
            );
        }
        for missing in ["name", "browser_download_url", "size"] {
            let document = match missing {
                "name" => r#"[{"tag_name":"v1.0.0","draft":false,"html_url":"u",
                    "assets":[{"browser_download_url":"u","size":1}]}]"#,
                "browser_download_url" => r#"[{"tag_name":"v1.0.0","draft":false,"html_url":"u",
                    "assets":[{"name":"n","size":1}]}]"#,
                _ => r#"[{"tag_name":"v1.0.0","draft":false,"html_url":"u",
                    "assets":[{"name":"n","browser_download_url":"u"}]}]"#,
            };
            let error = listed(document).unwrap_err();
            assert!(
                error.contains(&format!("missing field `{missing}`")),
                "{document}: {error}"
            );
        }
        // A mistyped field is refused too, rather than coerced.
        let error = listed(r#"[{"tag_name":"v1.0.0","draft":"no","html_url":"u","assets":[]}]"#)
            .unwrap_err();
        assert!(error.contains("invalid type"), "{error}");
    }

    #[test]
    fn version_ordering_is_semver() {
        let parse = |s: &str| Version::parse(s).unwrap();
        assert!(parse("0.1.0") > parse("0.1.0-beta.7"));
        assert!(parse("0.1.0-beta.8") > parse("0.1.0-beta.7"));
        assert!(parse("0.1.0-beta.10") > parse("0.1.0-beta.9"));
        assert!(parse("0.1.0-beta.2") < parse("0.1.0-beta.10"));
        assert!(parse("0.2.0") > parse("0.1.99"));
        assert!(parse("1.0.0") > parse("0.99.99"));
        // A numeric identifier sorts below an alphanumeric one.
        assert!(parse("1.0.0-alpha.1") < parse("1.0.0-alpha.beta"));
        // A shorter run that is a prefix sorts first.
        assert!(parse("1.0.0-alpha") < parse("1.0.0-alpha.1"));
        assert_eq!(parse("1.0.0+build.2"), parse("1.0.0+build.2"));
        // Build metadata is compared last, by `semver`'s `BuildMetadata`
        // ordering: dot-separated identifiers, numeric ones numerically.
        // (The hand-written stand-in compared the whole string as text, which
        // is why this line used to assert the opposite.)
        assert!(parse("1.0.0+build.9") < parse("1.0.0+build.10"));
        assert!(parse("1.0.0+build") < parse("1.0.0+build.1"));
    }

    /// The recovered chain — major, minor, patch, pre-release, build — is
    /// `semver::Version`'s own, so the newtype must agree with it on every
    /// pair.
    #[test]
    fn version_ordering_agrees_with_semver() {
        let order = [
            "0.1.0-beta.6",
            "0.1.0-beta.7",
            "0.1.0-beta.10",
            "0.1.0-beta.10+build.1",
            "0.1.0",
            "0.1.0+build.1",
            "0.1.1",
            "0.2.0-alpha",
            "0.2.0",
            "1.0.0",
        ];
        let ours: Vec<Version> = order.iter().map(|s| Version::parse(s).unwrap()).collect();
        for (i, left) in ours.iter().enumerate() {
            for (j, right) in ours.iter().enumerate() {
                let expected = order[i]
                    .parse::<semver::Version>()
                    .unwrap()
                    .cmp(&order[j].parse::<semver::Version>().unwrap());
                assert_eq!(left.cmp(right), expected, "{} vs {}", order[i], order[j]);
                assert_eq!(left.partial_cmp(right), Some(expected));
            }
        }
        // And the running version comes out where `newest` expects it.
        let running = Version::parse(RUNNING).unwrap();
        assert!(Version::parse("0.1.0-beta.6").unwrap() < running);
        assert!(Version::parse("0.1.0-beta.8").unwrap() > running);
    }

    #[test]
    fn parses_the_running_version() {
        let version = Version::parse("0.1.0-beta.7").unwrap();
        assert_eq!((version.major, version.minor, version.patch), (0, 1, 0));
        assert_eq!(version.pre.as_str(), "beta.7");
        assert_eq!(version.build.as_str(), "");
        assert_eq!(version.to_string(), "0.1.0-beta.7");
        assert_eq!(version.as_str(), "0.1.0-beta.7");
        assert_eq!(Version::parse("2.10.3+sha.abc").unwrap().build.as_str(), "sha.abc");
        assert_eq!(Version::parse("1.0.0").unwrap().pre.as_str(), "");
        // The newtype and the crate agree, in both directions.
        assert_eq!(version.semver(), &semver::Version::parse("0.1.0-beta.7").unwrap());
        assert_eq!(
            semver::Version::from(version.clone()),
            semver::Version::parse("0.1.0-beta.7").unwrap()
        );
        assert_eq!(Version::from(semver::Version::new(1, 2, 3)).to_string(), "1.2.3");
        assert_eq!(Version::default().to_string(), "0.0.0");
        assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
    }

    #[test]
    fn rejects_versions_semver_rejects() {
        for bad in [
            "",
            "1",
            "1.0",
            "1.0.0.0",
            "01.0.0",
            "1.0.0-",
            "1.0.0-01",
            "1.0.0-a..b",
            "v1.0.0",
            "1.0.0-a b",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?} should not parse");
        }
        // The error is `semver`'s, not a stand-in's: it names the position.
        let error = Version::parse("1.0").unwrap_err();
        assert!(error.contains("unexpected end of input"), "{error}");
    }
}
