//! Combat status lines (`StatusLine` / `StatusName`).
//!
//! `StatusLine::classify` (884 B): def family `"status"`, group `"target"`, then
//! `StatusName::parse` (184 B).
//!
//! `// UNKNOWN:` the `StatusName` variant list — the 184-byte parse only
//! dispatches, and the names it accepts are the same status words the
//! creature/status tables use.

/// A creature status mentioned by combat output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusName(pub String);

impl StatusName {
    /// @0x0000000101b9216c (184 B)
    pub fn parse(text: &str) -> Option<StatusName> {
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(StatusName(text.to_string()))
        }
    }
}

/// One status line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusLine {
    pub target: Option<String>,
    pub status: Option<StatusName>,
}

impl StatusLine {
    /// @0x0000000101b91df8 (884 B)
    pub fn classify(text: &str) -> Option<StatusLine> {
        let def = super::defs::defs().first_match(text)?;
        if def.family != "status" {
            return None;
        }
        Some(StatusLine {
            target: None,
            status: StatusName::parse(&def.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_needs_definitions() {
        assert_eq!(StatusLine::classify("The kobold is stunned."), None);
        assert_eq!(StatusName::parse(""), None);
        assert_eq!(
            StatusName::parse("stunned"),
            Some(StatusName("stunned".into()))
        );
    }
}
