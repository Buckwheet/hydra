//! Styled text runs.
//!
//! Recovered layout: a `Run` is 248 bytes (`0xf8`) with
//!   * `+0x00`: `String` text (cap/ptr/len),
//!   * `+0x18`: `Option<String>` preset,
//!   * `+0x30`: `u16` style flags (non-zero marks a bold run),
//!   * `+0x32`: `u8` bold-ish flag,
//!   * `+0x38`: `Option<Link>`,
//!   * `+0x98`: `Option<Link>` (an alternate/object link).
//!
//! `Runs` is a plain `Vec<Run>` newtype, as in the binary.

use std::ops::Deref;

use crate::frame::payload::Link;

/// A single styled span of text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    /// Style/preset id (e.g. `"roomDesc"`), if any.
    pub preset: Option<String>,
    /// `// UNKNOWN:` which bits mean what; the binary only tests `!= 0` when
    /// collecting bold fragments.
    pub style: u16,
    /// Additional bold flag copied by `TextFrame::as_run` (offset `0x32`).
    pub bold: bool,
    /// First link slot (binary offset `0x38`).
    pub link: Option<Link>,
    /// Second link slot (binary offset `0x98`).
    pub object: Option<Link>,
}

impl Run {
    /// A plain run with no styling.
    pub fn new(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            ..Run::default()
        }
    }

    /// A bold run.
    ///
    /// `// UNKNOWN:` the binary's style word is opaque; setting it to `1`
    /// matches the observed "non-zero means bold" check.
    pub fn bold(text: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            style: 1,
            bold: true,
            ..Run::default()
        }
    }

    /// A run with a preset/style id.
    pub fn with_preset(text: impl Into<String>, preset: impl Into<String>) -> Self {
        Run {
            text: text.into(),
            preset: Some(preset.into()),
            ..Run::default()
        }
    }

    /// A run carrying a link.
    pub fn with_link(text: impl Into<String>, link: Link) -> Self {
        let object = link.is_object().then(|| link.clone());
        Run {
            text: text.into(),
            link: Some(link),
            object,
            ..Run::default()
        }
    }

    /// True when this run should render as bold.
    pub fn is_bold(&self) -> bool {
        self.style != 0 || self.bold
    }

    /// The object link attached to this run, if any.
    ///
    /// The binary checks its two `Option<Link>` slots at offsets `0x98` then
    /// `0x38` and returns the first variant whose discriminant marks it as an
    /// object link (rather than a command/href link).
    pub fn object(&self) -> Option<&Link> {
        self.object
            .as_ref()
            .filter(|link| link.is_object())
            .or_else(|| self.link.as_ref().filter(|link| link.is_object()))
    }

    /// Convenience: the `exist` id of the object link.
    pub fn object_name(&self) -> Option<&str> {
        self.object().and_then(Link::exist)
    }
}

/// An ordered collection of [`Run`]s.
///
/// The binary represents this as `Vec<Run>`; the newtype is kept public so
/// callers can construct one directly: `Runs(vec![...])`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Runs(pub Vec<Run>);

impl Runs {
    pub fn new() -> Self {
        Runs(Vec::new())
    }

    pub fn from_vec(runs: Vec<Run>) -> Self {
        Runs(runs)
    }

    pub fn push(&mut self, run: Run) {
        self.0.push(run);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Run> {
        self.0.iter()
    }

    pub fn as_slice(&self) -> &[Run] {
        &self.0
    }

    /// Concatenate the text of every run.
    ///
    /// Recovered: the binary copies each run's `String` at offset `0` into a
    /// new `String`; no separators or escaping are added.
    pub fn plain(&self) -> String {
        let mut out = String::new();
        for run in &self.0 {
            out.push_str(&run.text);
        }
        out
    }

    /// Clone the text of every run whose style word is non-zero.
    pub fn bold_fragments(&self) -> Vec<String> {
        self.0
            .iter()
            .filter(|run| run.is_bold())
            .map(|run| run.text.clone())
            .collect()
    }

    /// True when every run has empty text (or there are no runs).
    ///
    /// Recovered: the binary scans for a run with a non-zero text length.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|run| run.text.is_empty())
    }
}

impl Deref for Runs {
    type Target = [Run];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<Run>> for Runs {
    fn from(value: Vec<Run>) -> Self {
        Runs(value)
    }
}

impl FromIterator<Run> for Runs {
    fn from_iter<T: IntoIterator<Item = Run>>(iter: T) -> Self {
        Runs(iter.into_iter().collect())
    }
}

impl IntoIterator for Runs {
    type Item = Run;
    type IntoIter = std::vec::IntoIter<Run>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Runs {
    type Item = &'a Run;
    type IntoIter = std::slice::Iter<'a, Run>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::payload::Link;

    #[test]
    fn plain_concatenates_runs() {
        let runs = Runs(vec![
            Run::new("a blue "),
            Run::new("gem"),
            Run::new("."),
        ]);
        assert_eq!(runs.plain(), "a blue gem.");
    }

    #[test]
    fn bold_fragments_collects_styled_text() {
        let runs = Runs(vec![
            Run::new("You see "),
            Run::bold("a glowing gem"),
            Run::new(" here."),
        ]);
        assert_eq!(runs.bold_fragments(), vec!["a glowing gem".to_string()]);
    }

    #[test]
    fn is_empty_ignores_empty_runs_but_not_text() {
        assert!(Runs::new().is_empty());
        assert!(Runs(vec![Run::new(""), Run::new("")]).is_empty());
        assert!(!Runs(vec![Run::new(""), Run::new("x")]).is_empty());
    }

    #[test]
    fn object_returns_object_links_only() {
        let object_link = Link::object("123", Some("gem"));
        let command_link = Link::command("look");
        let run = Run {
            text: "a blue gem".into(),
            link: Some(command_link),
            object: Some(object_link.clone()),
            ..Run::default()
        };
        assert_eq!(run.object(), Some(&object_link));
        assert_eq!(run.object_name(), Some("123"));

        let command_only = Run {
            text: "look".into(),
            link: Some(Link::command("look")),
            ..Run::default()
        };
        assert!(command_only.object().is_none());
    }

    #[test]
    fn with_link_also_populates_object_slot() {
        let link = Link::object("42", Some("sword"));
        let run = Run::with_link("a sword", link.clone());
        assert_eq!(run.link, Some(link));
        assert_eq!(run.object_name(), Some("42"));
    }
}
