//! Low-level helpers for the XML-ish GemStone wire format.
//!
//! Recovered from the binary:
//!   * `tag_name` strips a leading `<` or `</` and stops at whitespace,
//!     `/` or `>`.
//!   * `attribute` / `attribute_u32` parse a named attribute out of a tag.
//!   * `find_tag_start` finds the next plausible tag start, skipping `$<`
//!     escapes and prose-like angle-bracket spans.
//!   * `is_prose` decides whether an angle-bracket span is ordinary prose
//!     rather than markup.
//!   * `link_from_tag` builds a [`Link`] from `<a exist=... noun=...>`,
//!     `<a href=...>` or `<d cmd=...>` tags.

use crate::frame::payload::{Link, LinkTarget};

pub mod entities;

/// Return the name portion of a tag.
///
/// `tag_name("<a exist=\"1\">") == "a"`,
/// `tag_name("</popBold>") == "popBold"`,
/// `tag_name("<br/>") == "br"`.
///
/// If `tag` does not start with `<` the whole input is treated as tag text,
/// matching the binary's fallback.  The returned slice always borrows from
/// `tag`; an empty input yields an empty slice (never `None`).
///
/// `// UNKNOWN:` the recovered return type is `&str`, not `Option<&str>`:
/// call sites in `parse_line` test the length directly.
pub fn tag_name(tag: &str) -> &str {
    let inner = tag
        .strip_prefix("</")
        .or_else(|| tag.strip_prefix('<'))
        .unwrap_or(tag);

    for (idx, c) in inner.char_indices() {
        if c == '>' || c == '/' || c.is_whitespace() {
            return &inner[..idx];
        }
    }
    inner
}

/// Find the first plausible tag start in `text`, returning its byte offset.
///
/// A candidate is a `<` followed by an ASCII letter, `!`, `/`, `:` or `_`,
/// followed eventually by `>`.  `$<...>` is an escape sequence in GemStone
/// text and is skipped.  Candidates that [`is_prose`] classifies as prose are
/// skipped too.
pub fn find_tag_start(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut search = 0usize;
    while search < bytes.len() {
        let rel = bytes[search..].iter().position(|&b| b == b'<')?;
        let start = search + rel;

        // `$<...>` is an escaped literal '<', not a tag.
        if start > 0 && bytes[start - 1] == b'$' {
            search = start + 1;
            continue;
        }

        // A '<' at the very end cannot be a tag.
        let Some(&next) = bytes.get(start + 1) else {
            return None;
        };
        let plausible = next.is_ascii_alphabetic() || matches!(next, b'!' | b'/' | b':' | b'_');
        if !plausible {
            search = start + 1;
            continue;
        }

        // Find the closing '>'.
        let Some(end_rel) = bytes[start + 1..].iter().position(|&b| b == b'>') else {
            return None;
        };
        let end = start + 1 + end_rel + 1; // exclusive
        let candidate = &text[start..end];
        if is_prose(candidate) {
            search = start + 1;
            continue;
        }
        return Some(start);
    }
    None
}

/// Decide whether a `<...>` span is prose rather than a protocol tag.
///
/// The binary implements this as: after trimming trailing `>`/`/`, a span
/// containing `=` is markup (false); a span containing `#` is prose (true);
/// otherwise the span is prose iff it contains whitespace.
pub fn is_prose(text: &str) -> bool {
    let Some(start) = text.find('<') else {
        return true;
    };
    let Some(end) = text[start..].rfind('>') else {
        return true;
    };
    let inner = &text[start + 1..start + end];

    if inner.contains('=') {
        return false;
    }
    if inner.contains('#') {
        return true;
    }
    inner.chars().any(char::is_whitespace)
}

/// Copy the value of attribute `name` out of `tag`, or `None`.
///
/// Attribute values may be double-quoted, single-quoted, or (as a fallback)
/// unquoted.  The returned string is the raw wire value; call
/// [`entities::decode_attribute_value`] if entity decoding is wanted.
pub fn attribute(tag: &str, name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }
    for (key, value) in attributes(tag) {
        if key == name {
            return Some(value.to_string());
        }
    }
    None
}

/// Parse an attribute as a `u32`.
///
/// Surrounding whitespace is ignored and a leading `+` is accepted.  A
/// leading `-` is rejected (the original returns `None` for negative values)
/// as is any overflow or non-digit.
pub fn attribute_u32(tag: &str, name: &str) -> Option<u32> {
    let raw = attribute(tag, name)?;
    let trimmed = raw.trim();
    let digits = trimmed.strip_prefix('+').unwrap_or(trimmed);
    if digits.is_empty() || digits.starts_with('-') {
        return None;
    }
    let mut value: u32 = 0;
    for c in digits.chars() {
        let d = c.to_digit(10)?;
        value = value.checked_mul(10)?.checked_add(d)?;
    }
    Some(value)
}

/// Iterate over `(key, value)` attribute pairs in a tag.
///
/// This is the equivalent of the binary's private `text::pairs` helper plus
/// the attribute-name/value parsing in `attribute`.  It tolerates a leading
/// `</`, a trailing `>` and a trailing `/`.
pub fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let body = tag_body(tag);
    let mut out = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0usize;

    // Skip the tag name.
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }

    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'/' {
            break;
        }
        let key_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let key = &body[key_start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            // A bare word with no '=': skip it.
            continue;
        }
        i += 1; // '='
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let (value_start, value_end);
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            value_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            value_end = i;
            if i < bytes.len() {
                i += 1;
            }
        } else {
            value_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            value_end = i;
        }
        if !key.is_empty() {
            out.push((key, &body[value_start..value_end]));
        }
    }
    out
}

/// Strip the `<`/`</` prefix, the trailing `>` and a trailing `/`.
fn tag_body(tag: &str) -> &str {
    let start = tag.find('<').map(|i| {
        if tag[i + 1..].starts_with('/') {
            i + 2
        } else {
            i + 1
        }
    });
    let body = match start {
        Some(idx) => &tag[idx..],
        None => tag,
    };
    let mut body = body.trim_end();
    if let Some(stripped) = body.strip_suffix('>') {
        body = stripped.trim_end();
    }
    if let Some(stripped) = body.strip_suffix('/') {
        body = stripped.trim_end();
    }
    body
}

/// Build a [`Link`] from an `<a>`/`<d>` tag, if it has a usable target.
///
/// Precedence matches the binary:
///   1. `href` attribute -> [`LinkTarget::Href`]
///   2. `exist` attribute (plus `noun`) -> [`LinkTarget::Object`]
///   3. `cmd` attribute -> [`LinkTarget::Command`]
///
/// The optional `coord` attribute is preserved on every variant.
pub fn link_from_tag(tag: &str) -> Option<Link> {
    let coord = attribute(tag, "coord");

    if let Some(href) = attribute(tag, "href") {
        return Some(Link {
            coord,
            target: LinkTarget::Href { href },
            ..Link::default()
        });
    }

    if let Some(exist) = attribute(tag, "exist") {
        let noun = attribute(tag, "noun");
        return Some(Link {
            coord,
            target: LinkTarget::Object { exist, noun },
            ..Link::default()
        });
    }

    if let Some(cmd) = attribute(tag, "cmd") {
        return Some(Link {
            coord,
            target: LinkTarget::Command { cmd },
            ..Link::default()
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::payload::LinkTarget;

    #[test]
    fn tag_name_strips_wrappers() {
        assert_eq!(tag_name("<a exist=\"1\">"), "a");
        assert_eq!(tag_name("</popBold>"), "popBold");
        assert_eq!(tag_name("<pushBold/>"), "pushBold");
        assert_eq!(tag_name("<br />"), "br");
        assert_eq!(tag_name("<d cmd=\"look\">"), "d");
        assert_eq!(tag_name(""), "");
    }

    #[test]
    fn attributes_are_raw_and_quoted() {
        let tag = "<a exist=\"123\" noun=\"blue gem\">";
        assert_eq!(attribute(tag, "exist").as_deref(), Some("123"));
        assert_eq!(attribute(tag, "noun").as_deref(), Some("blue gem"));
        assert_eq!(attribute(tag, "missing"), None);
        assert_eq!(
            attribute("<a href='x' cmd=\"y\">", "href").as_deref(),
            Some("x")
        );
    }

    #[test]
    fn attribute_u32_parses_and_rejects() {
        let tag = "<item value=\"  +42 \" other=\"-1\" huge=\"99999999999\">";
        assert_eq!(attribute_u32(tag, "value"), Some(42));
        assert_eq!(attribute_u32(tag, "other"), None);
        assert_eq!(attribute_u32(tag, "huge"), None);
        assert_eq!(attribute_u32(tag, "missing"), None);
    }

    #[test]
    fn find_tag_start_skips_escapes_and_prose() {
        assert_eq!(find_tag_start("hello <pushBold/> world"), Some(6));
        assert_eq!(find_tag_start("$<not a tag>"), None);
        // `a < b > c` looks like prose, so no tag is found.
        assert_eq!(find_tag_start("a < b > c"), None);
        assert_eq!(find_tag_start("no tags here"), None);
        assert_eq!(find_tag_start("<a exist=\"1\">x</a>"), Some(0));
    }

    #[test]
    fn is_prose_classifies_candidates() {
        assert!(is_prose("plain text"));
        assert!(is_prose("< b >"));
        assert!(is_prose("<unterminated"));
        assert!(!is_prose("<pushBold/>"));
        assert!(!is_prose("<a exist=\"1\">"));
        assert!(!is_prose("</popBold>"));
    }

    #[test]
    fn link_from_tag_prefers_href_then_exist_then_cmd() {
        let link = link_from_tag("<a href=\"https://example\" coord=\"1,2\">").unwrap();
        assert_eq!(
            link.target,
            LinkTarget::Href {
                href: "https://example".into()
            }
        );
        assert_eq!(link.coord.as_deref(), Some("1,2"));

        let link = link_from_tag("<a exist=\"123\" noun=\"gem\">").unwrap();
        assert_eq!(
            link.target,
            LinkTarget::Object {
                exist: "123".into(),
                noun: Some("gem".into())
            }
        );

        let link = link_from_tag("<d cmd=\"look\">").unwrap();
        assert_eq!(
            link.target,
            LinkTarget::Command {
                cmd: "look".into()
            }
        );

        assert!(link_from_tag("<a>").is_none());
    }
}
