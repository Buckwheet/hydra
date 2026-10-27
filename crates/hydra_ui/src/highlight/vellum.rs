//! Importing a Vellum highlight file.
//!
//! `highlight::vellum::import_vellum` @0x1018737d0 is the second-largest stub in
//! the crate (346 KB): it is the whole Vellum reader, inlined.  The recovered
//! support types are `VellumFile`, `FromVellum`, `Entry` and the helpers
//! `same_but_whose`, `FromVellum::colour`, and the error literal
//! `"a pattern: not Vellum's highlights"` / `"it holds no highlight with a
//! pattern"`.

use crate::highlight::file::{HighlightFile, Rule};

/// One highlight in a Vellum file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    /// The pattern text.
    pub pattern: String,
    /// The style letter Vellum uses (`b` bold, `h` highlight, …).
    pub style: String,
    /// The foreground colour, if it set one.
    pub color: Option<crate::theme::Rgb>,
}

/// A parsed Vellum highlight file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VellumFile {
    pub entries: Vec<Entry>,
}

impl VellumFile {
    /// `@0x1018737d0` — read a Vellum highlight file.
    ///
    /// The format is one rule per line: a pattern, then `/` separated style and
    /// colour fields, `#` starting a comment.
    pub fn parse(text: &str) -> Result<VellumFile, String> {
        let mut entries = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let mut parts = line.split('/');
            let pattern = parts.next().unwrap_or("").trim().to_string();
            if pattern.is_empty() {
                continue;
            }
            let mut style = String::new();
            let mut color = None;
            for part in parts {
                let part = part.trim();
                if let Some(hex) = part.strip_prefix('c') {
                    color = crate::theme::parse_hex(hex);
                } else if !part.is_empty() {
                    style.push_str(part);
                }
            }
            entries.push(Entry {
                pattern,
                style,
                color,
            });
        }
        if entries.is_empty() {
            return Err("it holds no highlight with a pattern".to_string());
        }
        Ok(VellumFile { entries })
    }

    /// The colour a Vellum entry asks for.
    pub fn colour(&self, pattern: &str) -> Option<crate::theme::Rgb> {
        self.entries
            .iter()
            .find(|e| e.pattern == pattern)
            .and_then(|e| e.color)
    }
}

/// `@0x1018737d0` — import a Vellum highlight file as Hydra rules.
pub fn import_vellum(text: &str) -> Result<HighlightFile, String> {
    let file = VellumFile::parse(text)?;
    let rules = file
        .entries
        .iter()
        .map(|e| Rule {
            pattern: e.pattern.clone(),
            whole_word: e.style.contains('w'),
            case_sensitive: e.style.contains('C'),
            color: e.color,
            ..Rule::default()
        })
        .collect();
    Ok(HighlightFile::from_rules(rules))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# Vellum highlights\norc/c#ff0000\nkobold\n";

    #[test]
    fn parses_patterns_and_colours() {
        let f = VellumFile::parse(FILE).unwrap();
        assert_eq!(f.entries.len(), 2);
        assert_eq!(f.entries[0].pattern, "orc");
        assert_eq!(f.entries[0].color, Some(crate::theme::rgb(0xff, 0, 0)));
        assert_eq!(f.entries[1].pattern, "kobold");
        assert_eq!(f.colour("orc"), Some(crate::theme::rgb(0xff, 0, 0)));
        assert_eq!(f.colour("nope"), None);
    }

    #[test]
    fn an_empty_file_is_an_error() {
        assert_eq!(
            VellumFile::parse("# nothing\n\n").unwrap_err(),
            "it holds no highlight with a pattern"
        );
    }

    #[test]
    fn importing_builds_rules() {
        let file = import_vellum(FILE).unwrap();
        assert_eq!(file.highlight.len(), 2);
        assert_eq!(
            file.highlight["orc"].color,
            Some(crate::theme::rgb(0xff, 0, 0))
        );
    }

    #[test]
    fn style_letters_map_to_flags() {
        let f = import_vellum("orc/wC\n").unwrap();
        assert!(f.highlight["orc"].whole_word);
        assert!(f.highlight["orc"].case_sensitive);
    }
}
