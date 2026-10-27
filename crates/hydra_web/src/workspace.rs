//! Per-character web workspace files: `web/<ACCOUNT>_<character>.workspace.json`.
//!
//! Recovered from `workspace::path` (`0x1002cadd4`), `workspace::answered`
//! (`0x1002cafd4`) and `workspace::revision` (`0x1002cb280`).

use std::path::{Path, PathBuf};

/// Tail of a workspace file (the recovered format piece
/// `{}_{}.workspace.json`, `0x103e263b8`).
pub const SUFFIX: &str = ".workspace.json";
/// The sub-directory under the storage root (`root.join("web")`, `0x1020725fe`).
pub const DIR: &str = "web";
/// Side file recording the revision last accepted.
pub const VERSION_FILE: &str = ".storage_version";
/// Largest workspace body the endpoint accepts (64 KiB, `0x1002cb110`).
pub const MAX_BODY: usize = 0x1_0000;

/// Where one character's web workspace lives.
///
/// **ESTABLISHED** (`0x1002cadd4`): `root.join("web")` then
/// `format!("{}_{}.workspace.json", account.to_uppercase(),
/// character.to_lowercase())`.
pub fn path(root: &Path, account: &str, character: &str) -> PathBuf {
    root.join(DIR).join(format!(
        "{}_{}{SUFFIX}",
        account.to_uppercase(),
        character.to_lowercase()
    ))
}

/// The only part of a workspace document this crate reads.
///
/// **ESTABLISHED**: `workspace::revision` deserializes the text with
/// `serde_json` and reads the object's `revision` key (the pool holds `revision`
/// right next to the workspace literals).  Every other field is the page's and
/// is ignored.
#[derive(serde::Deserialize)]
struct Revision {
    revision: u32,
}

/// The `revision` field of a workspace document, or `None` when the text is not
/// a document or carries no usable revision.
pub fn revision(text: &str) -> Option<u32> {
    serde_json::from_str::<Revision>(text)
        .ok()
        .map(|document| document.revision)
}

/// The answer to a workspace HTTP request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    /// HTTP status (`200`, `400`, `404`, `412`, `413`, `500`).
    pub status: u16,
    /// The stored document, for `GET`.
    pub text: Option<String>,
}

impl Answer {
    fn status_only(status: u16) -> Answer {
        Answer { status, text: None }
    }
}

/// The workspace endpoint: `GET` returns the stored document, `PUT` replaces it.
///
/// Recovered control flow (`0x1002cafd4`):
///
/// | case | status |
/// | --- | --- |
/// | read failed with something other than `NotFound` | `500` |
/// | method is not exactly 3 bytes, or is not `GET`/`PUT` | `400` |
/// | `GET`, nothing stored | `404` |
/// | `GET`, stored | `200` + the document |
/// | `PUT`, empty body or a body without a `revision` | `400` |
/// | `PUT`, body larger than 64 KiB | `413` |
/// | `PUT`, stored revision strictly newer than the body's | `412` |
/// | `PUT` accepted | `200` |
///
/// `// UNKNOWN:` the side file written next to the document
/// (`.storage_version`) — the binary writes it through
/// `hydra_session::store::save_bytes` after a successful `PUT`, but the content
/// is not readable from the code, so the accepted revision is stored as decimal
/// text.
pub fn answered(file: &Path, method: &str, body: &str) -> Answer {
    let stored = match std::fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Answer::status_only(500),
    };

    match method {
        "GET" => match stored {
            Some(text) => Answer {
                status: 200,
                text: Some(text),
            },
            None => Answer::status_only(404),
        },
        "PUT" => {
            if body.is_empty() {
                return Answer::status_only(400);
            }
            if body.len() > MAX_BODY {
                return Answer::status_only(413);
            }
            let Some(incoming) = revision(body) else {
                return Answer::status_only(400);
            };
            if let Some(old) = stored.as_deref().and_then(revision) {
                if old > incoming {
                    // The page is behind: it must re-fetch before writing.
                    return Answer::status_only(412);
                }
            }
            let dir = file.parent().unwrap_or_else(|| Path::new("."));
            if hydra_session::store::save_bytes(dir, file, body.as_bytes()).is_err() {
                return Answer::status_only(500);
            }
            let stamp = dir.join(VERSION_FILE);
            let _ = hydra_session::store::save_bytes(dir, &stamp, incoming.to_string().as_bytes());
            Answer::status_only(200)
        }
        _ => Answer::status_only(400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hydra-web-ws-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn path_uses_upper_account_and_lower_character() {
        let p = path(Path::new("/store"), "ImaGine", "TestChar");
        assert_eq!(p, PathBuf::from("/store/web/IMAGINE_testchar.workspace.json"));
        assert_eq!(SUFFIX, ".workspace.json");
    }

    #[test]
    fn revision_reads_the_json_field() {
        assert_eq!(revision(r#"{"revision":7,"layout":{}}"#), Some(7));
        assert_eq!(revision(r#"{"layout":{}}"#), None);
        assert_eq!(revision("nonsense"), None);
        assert_eq!(revision(r#"{"revision":-1}"#), None);
    }

    #[test]
    fn endpoint_answers() {
        let dir = scratch("answer");
        let file = path(&dir, "acct", "char");

        // Nothing stored yet.
        assert_eq!(answered(&file, "GET", "").status, 404);
        // Wrong method.
        assert_eq!(answered(&file, "POST", "").status, 400);

        let body = r#"{"revision":2,"layout":{"panels":[]}}"#;
        assert_eq!(answered(&file, "PUT", body).status, 200);
        let got = answered(&file, "GET", "");
        assert_eq!(got.status, 200);
        assert_eq!(got.text.as_deref(), Some(body));

        // Empty body / no revision.
        assert_eq!(answered(&file, "PUT", "").status, 400);
        assert_eq!(answered(&file, "PUT", "{}").status, 400);

        // Older revisions are refused, equal and newer ones are accepted.
        assert_eq!(answered(&file, "PUT", r#"{"revision":1}"#).status, 412);
        assert_eq!(answered(&file, "PUT", r#"{"revision":2}"#).status, 200);
        assert_eq!(answered(&file, "PUT", r#"{"revision":3}"#).status, 200);

        // Too big.
        let big = format!(r#"{{"revision":9,"pad":"{}"}}"#, "x".repeat(MAX_BODY));
        assert_eq!(answered(&file, "PUT", &big).status, 413);

        // The revision stamp is written next to the document.
        let stamp = std::fs::read_to_string(dir.join(DIR).join(VERSION_FILE)).unwrap();
        assert_eq!(stamp, "3");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
