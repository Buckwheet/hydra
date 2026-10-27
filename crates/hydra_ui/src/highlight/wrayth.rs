//! Importing a Wrayth / StormFront highlight file.
//!
//! `highlight::wrayth::import_wrayth` @0x10195ffdc is 23 KB.  The recovered
//! support types are `Imported` and `Imported::colour`.  A Wrayth highlight is
//! `<highlight><pattern>text</pattern><style>…</style></highlight>` inside the
//! skin's XML.

use crate::highlight::file::{HighlightFile, Rule};
use crate::theme::{parse_hex, Rgb};

/// One highlight read out of a Wrayth skin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    pub pattern: String,
    pub color: Option<Rgb>,
    pub sound: Option<String>,
}

impl Imported {
    /// The colour this highlight asks for.
    pub fn colour(&self) -> Option<Rgb> {
        self.color
    }
}

/// `@0x10195ffdc` — read the highlights out of a Wrayth skin.
pub fn import_wrayth(text: &str) -> Result<Vec<Imported>, String> {
    if !text.contains("<highlight") && !text.contains("<pattern") {
        return Err("it is not a Wrayth skin".to_string());
    }
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("<highlight") {
        rest = &rest[open..];
        let end = match rest.find("</highlight>") {
            Some(e) => e,
            None => rest.len(),
        };
        let block = &rest[..end];
        rest = &rest[end..];
        let pattern = text_between(block, "<pattern", "</pattern>").unwrap_or_default();
        if pattern.trim().is_empty() {
            continue;
        }
        let style = attrs_of(block, "<style").unwrap_or_default();
        let color = style
            .split(';')
            .find(|f| f.trim_start().starts_with("color"))
            .and_then(|f| f.split('=').nth(1))
            .and_then(quoted)
            .and_then(|v| parse_hex(&v));
        let sound = text_between(block, "<sound", "</sound>")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        out.push(Imported {
            pattern: pattern.trim().to_string(),
            color,
            sound,
        });
    }
    Ok(out)
}

/// The value inside the first pair of quotes, if there is one.
fn quoted(text: &str) -> Option<String> {
    let start = text.find(|c| c == '"' || c == '\'')?;
    let quote = text.as_bytes()[start] as char;
    let rest = &text[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// The attribute text of a self-closing tag, e.g. the `color="#ff0000"` of
/// `<style color="#ff0000"/>`.
fn attrs_of(text: &str, open: &str) -> Option<String> {
    let at = text.find(open)?;
    let after = &text[at + open.len()..];
    let end = after.find('>')?;
    Some(after[..end].to_string())
}

/// The text between an opening and a closing tag, with the opening tag's
/// attributes stripped.
fn text_between(text: &str, open: &str, close: &str) -> Option<String> {
    let at = text.find(open)?;
    let after = &text[at + open.len()..];
    let body = match after.find('>') {
        Some(gt) => &after[gt + 1..],
        None => after,
    };
    let end = body.find(close)?;
    Some(body[..end].to_string())
}

/// `@0x10195ffdc` — import a Wrayth skin's highlights as Hydra rules.
pub fn import_wrayth_rules(text: &str) -> Result<HighlightFile, String> {
    let entries = import_wrayth(text)?;
    Ok(HighlightFile::from_rules(
        entries
            .into_iter()
            .map(|e| Rule {
                pattern: e.pattern,
                color: e.color,
                sound: e.sound,
                ..Rule::default()
            })
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKIN: &str = r##"
<skin name="x">
  <highlight><pattern>an orc</pattern><style color="#ff0000"/></highlight>
  <highlight><pattern>a kobold</pattern><sound>kobold.wav</sound></highlight>
</skin>
"##;

    #[test]
    fn reads_highlights() {
        let got = import_wrayth(SKIN).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].pattern, "an orc");
        assert_eq!(got[0].colour(), Some(crate::theme::rgb(0xff, 0, 0)));
        assert_eq!(got[1].sound.as_deref(), Some("kobold.wav"));
    }

    #[test]
    fn rejects_non_wrayth_text() {
        assert_eq!(
            import_wrayth("<html></html>").unwrap_err(),
            "it is not a Wrayth skin"
        );
    }

    #[test]
    fn rules_are_built() {
        let f = import_wrayth_rules(SKIN).unwrap();
        assert_eq!(f.highlight.len(), 2);
        assert_eq!(f.highlight["a kobold"].sound.as_deref(), Some("kobold.wav"));
    }
}
