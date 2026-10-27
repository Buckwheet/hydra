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

use crate::json::{self, Json};

/// The checksum list the release publishes beside the archive (14 bytes,
/// compared word-for-word in `newest::{{closure}}` at 0x1010aead0).
pub const SUMS: &str = "SHA256SUMS.txt";

/// Its minisign signature (22 bytes, 0x1010aeb78).  minisign writes
/// `<file>.minisig`.
pub const SUMS_SIG: &str = "SHA256SUMS.txt.minisig";

/// One entry of `GET /releases` — only the four fields Hydra reads; every other
/// key GitHub sends is ignored, as `serde`'s derive does by default.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listed {
    pub tag_name: String,
    pub draft: bool,
    pub html_url: String,
    pub assets: Vec<ListedFile>,
}

/// One downloadable asset of a release.
#[derive(Debug, Clone, Default, PartialEq)]
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

/// The subset of `semver::Version` this crate needs.
///
/// `newest` parses the release tag and compares it with the running version
/// through `semver::Version`'s `PartialOrd` (the four inlined comparisons at
/// 0x1010ae16c–0x1010ae1cc: major, minor, patch, then
/// `Prerelease::partial_cmp`, then `BuildMetadata::partial_cmp`), so the
/// ordering here follows semver 1.x: `pre` empty sorts *above* any pre-release,
/// and `build` is compared last as text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// The `-alpha.1` part, without the dash.
    pub pre: String,
    /// The `+sha.abc` part, without the plus.
    pub build: String,
}

impl Version {
    /// `semver::Version::parse`.  The binary's call site
    /// (`hydra::update::keep` @0x10009e744) parses `"0.1.0-beta.7"`.
    pub fn parse(text: &str) -> Result<Version, String> {
        if text.contains(char::is_whitespace) {
            return Err(bad(text, "whitespace"));
        }
        let (rest, build) = match text.split_once('+') {
            Some((head, build)) => (head, Some(build)),
            None => (text, None),
        };
        let (rest, pre) = match rest.split_once('-') {
            Some((head, pre)) => (head, Some(pre)),
            None => (rest, None),
        };
        let mut numbers = rest.split('.');
        let major = numbers.next().ok_or_else(|| bad(text, "empty version"))?;
        let minor = numbers
            .next()
            .ok_or_else(|| bad(text, "missing minor version"))?;
        let patch = numbers
            .next()
            .ok_or_else(|| bad(text, "missing patch version"))?;
        if numbers.next().is_some() {
            return Err(bad(text, "unexpected fourth number"));
        }
        let numeric = |part: &str| -> Result<u64, String> {
            if part.is_empty() {
                return Err(bad(text, "empty number"));
            }
            if part.len() > 1 && part.starts_with('0') {
                return Err(bad(text, "leading zero in a number"));
            }
            part.parse::<u64>().map_err(|_| bad(text, "invalid number"))
        };
        let major = numeric(major)?;
        let minor = numeric(minor)?;
        let patch = numeric(patch)?;
        if let Some(pre) = pre {
            for identifier in pre.split('.') {
                check_identifier(text, identifier, false)?;
            }
        }
        if let Some(build) = build {
            for identifier in build.split('.') {
                check_identifier(text, identifier, true)?;
            }
        }
        Ok(Version {
            major,
            minor,
            patch,
            pre: pre.unwrap_or_default().to_string(),
            build: build.unwrap_or_default().to_string(),
        })
    }

    /// The text minisign's trusted comment is compared with: `newest`'s caller
    /// builds `format!("Hydra {}", version)`.
    pub fn as_str(&self) -> String {
        self.to_string()
    }
}

fn bad(text: &str, why: &str) -> String {
    format!("unexpected character in version {text:?}: {why}")
}

fn check_identifier(text: &str, identifier: &str, build: bool) -> Result<(), String> {
    if identifier.is_empty() {
        return Err(bad(text, "empty identifier"));
    }
    if !identifier
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(bad(text, "invalid character in an identifier"));
    }
    let numeric = identifier.bytes().all(|b| b.is_ascii_digit());
    if numeric && !build && identifier.len() > 1 && identifier.starts_with('0') {
        return Err(bad(text, "leading zero in a numeric identifier"));
    }
    Ok(())
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            write!(f, "-{}", self.pre)?;
        }
        if !self.build.is_empty() {
            write!(f, "+{}", self.build)?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| compare_prerelease(&self.pre, &other.pre))
            // semver 1.x compares the build metadata last, as text.
            .then_with(|| self.build.cmp(&other.build))
    }
}

/// `Prerelease`'s ordering: no pre-release is greater than any pre-release,
/// dot-separated identifiers compare numeric-to-numeric numerically and
/// otherwise as text, a numeric identifier sorts below an alphanumeric one, and
/// a shorter run that is a prefix of another sorts first.
fn compare_prerelease(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    let mut left = left.split('.');
    let mut right = right.split('.');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => {
                let a_numeric = a.bytes().all(|c| c.is_ascii_digit());
                let b_numeric = b.bytes().all(|c| c.is_ascii_digit());
                let ordering = match (a_numeric, b_numeric) {
                    (true, true) => match (a.parse::<u64>(), b.parse::<u64>()) {
                        (Ok(a), Ok(b)) => a.cmp(&b),
                        _ => a.cmp(b),
                    },
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => a.cmp(b),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

/// Read `Vec<Listed>` out of the releases document.
///
/// `serde`'s derive requires every field of `Listed`/`ListedFile` to be
/// present (neither carries `#[serde(default)]`), so a missing key is an error
/// here too; the message mimics `serde`'s.
pub fn listed(text: &str) -> Result<Vec<Listed>, String> {
    let root = json::parse(text)?;
    let items = root
        .as_array()
        .ok_or_else(|| format!("invalid type: {}, expected a sequence", root.kind()))?;
    items.iter().map(listed_one).collect()
}

fn listed_one(value: &Json) -> Result<Listed, String> {
    let object = match value {
        Json::Object(_) => value,
        other => {
            return Err(format!(
                "invalid type: {}, expected struct Listed",
                other.kind()
            ))
        }
    };
    // Fields are taken in declaration order, as the derive's `visit_map` does,
    // so the first problem reported is the one `serde` would report.
    let tag_name = string_field(object, "tag_name")?;
    let draft = object
        .get("draft")
        .ok_or_else(|| "missing field `draft`".to_string())?
        .as_bool()
        .ok_or_else(|| "invalid type: expected a boolean for `draft`".to_string())?;
    let html_url = string_field(object, "html_url")?;
    let assets = object
        .get("assets")
        .ok_or_else(|| "missing field `assets`".to_string())?;
    let assets = assets
        .as_array()
        .ok_or_else(|| format!("invalid type: {}, expected a sequence", assets.kind()))?;
    Ok(Listed {
        tag_name,
        draft,
        html_url,
        assets: assets.iter().map(listed_file).collect::<Result<_, _>>()?,
    })
}

fn listed_file(value: &Json) -> Result<ListedFile, String> {
    let object = match value {
        Json::Object(_) => value,
        other => {
            return Err(format!(
                "invalid type: {}, expected struct ListedFile",
                other.kind()
            ))
        }
    };
    let size = object
        .get("size")
        .ok_or_else(|| "missing field `size`".to_string())?;
    Ok(ListedFile {
        name: string_field(object, "name")?,
        browser_download_url: string_field(object, "browser_download_url")?,
        size: size
            .as_u64()
            .ok_or_else(|| format!("invalid type: {}, expected u64", size.kind()))?,
    })
}

fn string_field(object: &Json, key: &str) -> Result<String, String> {
    let value = object
        .get(key)
        .ok_or_else(|| format!("missing field `{key}`"))?;
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("invalid type: {}, expected a string for `{key}`", value.kind()))
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
        // Build metadata is compared last, as text (semver 1.x).
        assert!(parse("1.0.0+build.10") < parse("1.0.0+build.9"));
    }

    #[test]
    fn parses_the_running_version() {
        let version = Version::parse("0.1.0-beta.7").unwrap();
        assert_eq!((version.major, version.minor, version.patch), (0, 1, 0));
        assert_eq!(version.pre, "beta.7");
        assert_eq!(version.build, "");
        assert_eq!(version.to_string(), "0.1.0-beta.7");
        assert_eq!(Version::parse("2.10.3+sha.abc").unwrap().build, "sha.abc");
        assert_eq!(Version::parse("1.0.0").unwrap().pre, "");
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
    }
}
