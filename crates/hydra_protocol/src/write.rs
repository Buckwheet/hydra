//! Serialisation helpers for the XML-ish GemStone wire format.
//!
//! Recovered from the binary:
//!   * `escape` escapes `& < > " '` (all five),
//!   * `escape_attribute` first escapes then replaces tab/newline/CR with
//!     numeric references (`&#9;`, `&#10;`, `&#13;`),
//!   * `empty(name, attrs)` -> `<name k="v".../>`,
//!   * `element(name, attrs, body)` -> `<name k="v"...>body</name>`,
//!   * `runs` re-emits styled runs using `<pushBold/>`/`<popBold/>`,
//!     `<preset id="...">` and `<a ...>` links.

use crate::frame::payload::{Link, LinkTarget};
use crate::runs::Runs;

/// Escape XML special characters in text content.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape an attribute value.
///
/// On top of [`escape`], literal tab/newline/CR characters become numeric
/// character references so an attribute always stays on one line.
pub fn escape_attribute(text: &str) -> String {
    let escaped = escape(text);
    let mut out = String::with_capacity(escaped.len());
    for c in escaped.chars() {
        match c {
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render ` key="value"` pairs (leading space included).
fn attributes(attrs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (key, value) in attrs {
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        out.push_str(&escape_attribute(value));
        out.push('"');
    }
    out
}

/// Build a self-closing element: `<name/>` or `<name k="v"/>`.
pub fn empty(name: &str, attrs: &[(&str, &str)]) -> String {
    format!("<{}{}/>", name, attributes(attrs))
}

/// Build an element with a raw (already escaped) body:
/// `<name k="v">body</name>`.
pub fn element(name: &str, attrs: &[(&str, &str)], body: &str) -> String {
    format!("<{}{}>{}</{}>", name, attributes(attrs), body, name)
}

/// Render a [`Link`] as its wire tag.
fn open_link(link: &Link, out: &mut String) {
    match &link.target {
        LinkTarget::Object { exist, noun } => {
            out.push_str("<a exist=\"");
            out.push_str(&escape_attribute(exist));
            out.push('"');
            if let Some(noun) = noun {
                out.push_str(" noun=\"");
                out.push_str(&escape_attribute(noun));
                out.push('"');
            }
            if let Some(coord) = &link.coord {
                out.push_str(" coord=\"");
                out.push_str(&escape_attribute(coord));
                out.push('"');
            }
            out.push('>');
        }
        LinkTarget::Command { cmd } => {
            out.push_str("<d cmd=\"");
            out.push_str(&escape_attribute(cmd));
            out.push('"');
            if let Some(coord) = &link.coord {
                out.push_str(" coord=\"");
                out.push_str(&escape_attribute(coord));
                out.push('"');
            }
            out.push('>');
        }
        LinkTarget::Href { href } => {
            out.push_str("<a href=\"");
            out.push_str(&escape_attribute(href));
            out.push('"');
            if let Some(coord) = &link.coord {
                out.push_str(" coord=\"");
                out.push_str(&escape_attribute(coord));
                out.push('"');
            }
            out.push('>');
        }
    }
}

/// True when two link slots hold the same link.
///
/// Recovered from `write::same` (`0x101c38cb0`, 328 B).  The signature is
/// `fn same(a: Option<&Link>, b: Option<&Link>) -> bool`: the first four
/// instructions are
///
/// ```text
/// orr  x8, x0, x1        ; both null?
/// cmp  x8, #0x0
/// cset w8, eq            ; both null -> true
/// cbz  x0, return        ; a is None -> (b is None)
/// ```
///
/// and the body then compares, in this order:
///
///   * the `LinkKind` discriminant at `Link + 0x30` (decoded out of the
///     niche word with the same `and …, asr #63` idiom the `Debug` impl
///     uses), then
///   * for the `Exist`-family discriminant (`0`) the **two** strings at
///     `+0x30`/`+0x48`, for `Direct`/`Url` the one string at `+0x38`
///     (`bcmp` of the length then the bytes each time), then
///   * `Option<String> coord` at `+0x18` (`cap` niche `isize::MIN`).
///
/// **`attrs` (`Link + 0x00`) is never compared** — that is the whole point of
/// the function: two runs whose links differ only in their extra attributes
/// count as the same link and coalesce in [`runs`].
///
/// Two things this reconstruction cannot reproduce exactly, because the
/// public [`LinkTarget`] merges the original's variants: the original's
/// `Exist.noun` is a plain `String` (so `""` and absent are the same value)
/// while [`LinkTarget::Object`] carries an `Option<String>`, and the
/// original's `DirectText`/`NotActionable` variants have no counterpart here.
/// Everything reachable through the public API compares identically.
pub fn same(a: Option<&Link>, b: Option<&Link>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.coord == b.coord && a.target == b.target,
        _ => false,
    }
}

/// The closing tag the original's `Writing::close_links` writes for a link.
///
/// Recovered from `write::Writing::close_links` (`0x101c3927c`, 356 B), which
/// closes two slots in turn:
///
///   * the *anchor* slot (`Writing + 0x90`) always with `</a>` — the four
///     bytes `'<' '/' 'a' '>'` are stored from the immediate `0x3e612f3c`;
///   * the *command* slot (`Writing + 0x30`) with `</d>` **unless** the
///     target is `Url`/`NotActionable`: the immediate is bumped by
///     `w9 + #0x30, lsl #12` (giving `0x3e642f3c`, `</d>`) and selected by
///     `cmp x20, #0x8000_0000_0000_0002; csel w9, w10, w9, lt`, where
///     `x20` is the raw `LinkKind` niche word ( `0x8000_0000_0000_0000`
///     `Direct`, `…0001` `DirectText`, `…0002` `Url`, `…0003`
///     `NotActionable`; anything non-negative is `Exist`).
///
/// Both slots are `mem::take`n and the taken [`Link`] is dropped before the
/// closing bytes are appended, so a second call writes nothing.
///
/// This crate's [`runs`] is a single-function rewrite of the original's
/// `Writing` state machine and closes **every** link with `</a>`; see
/// `NOTES.md`.  This helper exposes the recovered rule on its own.
pub fn close_link(target: &LinkTarget) -> &'static str {
    match target {
        LinkTarget::Href { .. } => "</a>",
        // `Exist`, `Direct` and `DirectText` all sit below `Url` in the
        // niche ordering, so the binary writes `</d>` for them.
        LinkTarget::Object { .. } | LinkTarget::Command { .. } => "</d>",
    }
}

/// Render a sequence of styled runs back to the wire format.
///
/// The output is not necessarily byte-identical to the original line: the
/// binary normalises style/link transitions.  The important invariants are
/// preserved: text is escaped, bold spans are bracketed by
/// `<pushBold/>`/`<popBold/>`, presets by `<preset id="...">...</preset>`,
/// and links by their opening tag plus `</a>`.
pub fn runs(runs: &Runs) -> String {
    let mut out = String::new();
    let mut current_preset: Option<&str> = None;
    let mut bold = false;
    let mut link_open = false;

    for run in &runs.0 {
        let preset = run.preset.as_deref();
        if preset != current_preset {
            if link_open {
                out.push_str("</a>");
                link_open = false;
            }
            if let Some(previous) = current_preset {
                let _ = previous;
                out.push_str("</preset>");
            }
            if let Some(preset) = preset {
                out.push_str("<preset id=\"");
                out.push_str(&escape_attribute(preset));
                out.push_str("\">");
            }
            current_preset = preset;
        }

        let want_bold = run.is_bold();
        if want_bold != bold {
            if link_open {
                out.push_str("</a>");
                link_open = false;
            }
            out.push_str(if want_bold {
                "<pushBold/>"
            } else {
                "<popBold/>"
            });
            bold = want_bold;
        }

        let active_link = run.object.as_ref().or(run.link.as_ref());
        if let Some(link) = active_link {
            if link_open {
                out.push_str("</a>");
            }
            open_link(link, &mut out);
            link_open = true;
        }

        out.push_str(&escape(&run.text));
    }

    if link_open {
        out.push_str("</a>");
    }
    // The binary pops every still-open bold level at the end.
    if bold {
        out.push_str("<popBold/>");
    }
    if current_preset.is_some() {
        out.push_str("</preset>");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::payload::Link;
    use crate::runs::{Run, Runs};

    #[test]
    fn escape_covers_all_five_entities() {
        assert_eq!(
            escape("<a> & \"b\" 'c'"),
            "&lt;a&gt; &amp; &quot;b&quot; &apos;c&apos;"
        );
    }

    #[test]
    fn escape_attribute_quotes_controls() {
        assert_eq!(escape_attribute("a\tb\nc\rd"), "a&#9;b&#10;c&#13;d");
        assert_eq!(escape_attribute("a&b"), "a&amp;b");
    }

    #[test]
    fn empty_and_element_build_wire_tags() {
        assert_eq!(empty("br", &[]), "<br/>");
        assert_eq!(
            empty("style", &[("id", "roomDesc")]),
            "<style id=\"roomDesc\"/>"
        );
        assert_eq!(element("a", &[], "hi"), "<a>hi</a>");
        assert_eq!(
            element("a", &[("exist", "123")], "a blue gem"),
            "<a exist=\"123\">a blue gem</a>"
        );
    }

    #[test]
    fn runs_escapes_text() {
        let r = Runs(vec![Run::new("a < b & c")]);
        assert_eq!(super::runs(&r), "a &lt; b &amp; c");
    }

    #[test]
    fn runs_emits_bold_markers() {
        let r = Runs(vec![
            Run::new("You see "),
            Run::bold("a gem"),
            Run::new("."),
        ]);
        let wire = super::runs(&r);
        assert_eq!(wire, "You see <pushBold/>a gem<popBold/>.");
    }

    #[test]
    fn runs_closes_an_unterminated_bold_span() {
        let r = Runs(vec![Run::bold("hi")]);
        assert_eq!(super::runs(&r), "<pushBold/>hi<popBold/>");
    }

    #[test]
    fn runs_emits_presets() {
        let r = Runs(vec![Run::with_preset("A room", "roomDesc")]);
        assert_eq!(super::runs(&r), "<preset id=\"roomDesc\">A room</preset>");
    }

    #[test]
    fn runs_emits_object_links() {
        let r = Runs(vec![Run::with_link(
            "a blue gem",
            Link::object("123", Some("gem")),
        )]);
        assert_eq!(
            super::runs(&r),
            "<a exist=\"123\" noun=\"gem\">a blue gem</a>"
        );
    }

    #[test]
    fn two_absent_links_are_the_same() {
        assert!(same(None, None));
        let lone = Link::object("1", None::<String>);
        assert!(!same(Some(&lone), None));
        assert!(!same(None, Some(&lone)));
    }

    #[test]
    fn links_compare_by_target_and_coord_only() {
        let mut a = Link::object("123", Some("gem"));
        let mut b = Link::object("123", Some("gem"));
        assert!(same(Some(&a), Some(&b)));

        // `attrs` is never compared (`0x101c38cb0` compares `+0x30` and
        // `+0x18`, never `+0x00`).
        a.attrs.push(("colour".into(), "blue".into()));
        assert!(same(Some(&a), Some(&b)));

        // `coord` is compared.
        b.coord = Some("room:1".into());
        assert!(!same(Some(&a), Some(&b)));
        a.coord = Some("room:1".into());
        assert!(same(Some(&a), Some(&b)));

        // …and so is the target.
        let c = Link::object("124", Some("gem"));
        assert!(!same(Some(&a), Some(&c)));
    }

    #[test]
    fn different_link_kinds_are_never_the_same() {
        let object = Link::object("look", None::<String>);
        let command = Link::command("look");
        let href = Link::hyperlink("look");
        for (a, b) in [(&object, &command), (&object, &href), (&command, &href)] {
            assert!(!same(Some(a), Some(b)), "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn close_link_follows_the_recovered_niche_ordering() {
        // `Exist` and `Direct` are both below `Url` in the niche order, so
        // the binary's `csel … lt` picks `</d>` for them.
        assert_eq!(close_link(&LinkTarget::Object { exist: "1".into(), noun: None }), "</d>");
        assert_eq!(close_link(&LinkTarget::Command { cmd: "look".into() }), "</d>");
        // `Url` (and `NotActionable`) are at or above the threshold.
        assert_eq!(close_link(&LinkTarget::Href { href: "x".into() }), "</a>");
    }
}
