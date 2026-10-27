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
            out.push_str(if want_bold { "<pushBold/>" } else { "<popBold/>" });
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
        assert_eq!(
            escape_attribute("a\tb\nc\rd"),
            "a&#9;b&#10;c&#13;d"
        );
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
        let r = Runs(vec![Run::new("You see "), Run::bold("a gem"), Run::new(".")]);
        let wire = super::runs(&r);
        assert_eq!(wire, "You see <pushBold/>a gem<popBold/>.");
    }

    #[test]
    fn runs_emits_presets() {
        let r = Runs(vec![Run::with_preset("A room", "roomDesc")]);
        assert_eq!(
            super::runs(&r),
            "<preset id=\"roomDesc\">A room</preset>"
        );
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
}
