//! `serde_json`, under the name this crate used.
//!
//! **Phase 1 reconnection.**  This module used to be a hand-written JSON
//! reader, because the reconstruction had no external crates.  The original did
//! not have one: the monomorphization of `serde_json::from_str::<Vec<Listed>>`
//! is visible as `serde_json::de::from_trait` @0x1010a0f70, called from
//! `hydra_update::release::newest` @0x1010adfc0, and the two
//! `{{Visitor}}::expecting` globals at 0x1010aed0c / 0x1010aed20 are the
//! derives on `Listed` and `ListedFile`.
//!
//! So the module stays as the name the crate already spoke, but everything in
//! it is `serde_json`'s: [`Json`] **is** [`serde_json::Value`] and [`parse`] is
//! `serde_json::from_str`.  There is no second parser.  The hand-written
//! `Json::kind()` helper (a stand-in for `serde`'s `Unexpected` wording) went
//! with it: `release` reports `serde`'s own messages now.

pub use serde_json::Value as Json;

/// Parse one JSON document.
///
/// Was the hand-written parser; is now `serde_json::from_str`, the recovered
/// `serde_json::de::from_trait`.  The error keeps the `String` shape this crate
/// reports (`serde_json::Error`'s `Display`, e.g. `"expected value at line 1
/// column 1"`).
pub fn parse(text: &str) -> Result<Json, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_release_shape() {
        let text = r#"[
            {"tag_name": "v0.1.0-beta.8", "draft": false,
             "html_url": "https://example.invalid/hydra/v0.1.0-beta.8",
             "assets": [
                {"name": "hydra-arm64", "browser_download_url": "https://example.invalid/a", "size": 12345}
             ]},
            {"tag_name": "v0.1.0-beta.7", "draft": true, "html_url": "", "assets": []}
        ]"#;
        let root = parse(text).expect("parses");
        let items = root.as_array().expect("an array");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("tag_name").and_then(Json::as_str), Some("v0.1.0-beta.8"));
        assert_eq!(items[0].get("draft").and_then(Json::as_bool), Some(false));
        assert_eq!(items[1].get("draft").and_then(Json::as_bool), Some(true));
        let assets = items[0].get("assets").and_then(Json::as_array).unwrap();
        assert_eq!(assets[0].get("size").and_then(Json::as_u64), Some(12345));
    }

    #[test]
    fn reads_escapes_and_unknown_keys() {
        let root = parse(r#"{"a":"x\u00e9\n\"y\"","b":[null,true,-2.5e3],"c":{"d":1}}"#).unwrap();
        assert_eq!(root.get("a").and_then(Json::as_str), Some("xé\n\"y\""));
        assert_eq!(
            root.get("b").and_then(Json::as_array).map(|items| items.len()),
            Some(3)
        );
        assert_eq!(
            root.get("c").and_then(|c| c.get("d")).and_then(Json::as_u64),
            Some(1)
        );
    }

    #[test]
    fn surrogate_pairs() {
        let root = parse(r#""\ud83d\ude00""#).unwrap();
        assert_eq!(root.as_str(), Some("😀"));
    }

    #[test]
    fn rejects_broken_documents() {
        for bad in ["", "[", "{\"a\":}", "{\"a\" 1}", "nul", "[1,]", "\"unterminated", "{} x"] {
            assert!(parse(bad).is_err(), "{bad:?} should not parse");
        }
        let err = parse("[1,]").unwrap_err();
        assert!(err.contains("line 1 column"), "{err}");
    }

    /// The value type is `serde_json`'s, with its writers: nothing here
    /// re-escapes by hand.
    #[test]
    fn is_serde_json() {
        let value: Json = Json::from("a\"b\\c\nd\te\u{1}");
        assert_eq!(value.to_string(), r#""a\"b\\c\nd\te\u0001""#);
        assert_eq!(parse(&value.to_string()).unwrap(), value);
    }
}
