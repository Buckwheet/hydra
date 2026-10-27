//! Combat status lines (`StatusLine` / `StatusName`).
//!
//! `StatusLine::classify` (884 B): def family `"status"`, group `"target"`, then
//! `StatusName::parse` (184 B). The target comes from the hit's capture set
//! ([`crate::pattern::Groups`]).
//!
//! `// UNKNOWN:` the `StatusName` variant list — the 184-byte parse only
//! dispatches, and the names it accepts are the same status words the
//! creature/status tables use.

use super::defs::{defs, Defs};

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
        StatusLine::classify_with_defs(defs(), text)
    }

    /// [`StatusLine::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<StatusLine> {
        let (def, groups) = table.first_match_groups(text)?;
        if def.family != "status" {
            return None;
        }
        Some(StatusLine {
            target: groups.text("target"),
            status: StatusName::parse(&def.name),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::defs::Def;
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

    fn table() -> Defs {
        Defs {
            defs: vec![
                Def::new(
                    "status",
                    "stunned",
                    "damaging",
                    1,
                    "",
                    "",
                    "^(?P<target>The kobold) is stunned",
                ),
                Def::new("status", "prone", "damaging", 2, "", "", "^It falls prone"),
            ],
        }
    }

    #[test]
    fn the_target_group_is_captured() {
        let table = table();
        let line = StatusLine::classify_with_defs(&table, "The kobold is stunned").unwrap();
        assert_eq!(line.target.as_deref(), Some("The kobold"));
        assert_eq!(line.status, Some(StatusName("stunned".into())));
        // a row without the group keeps the status and leaves the target empty
        let line = StatusLine::classify_with_defs(&table, "It falls prone").unwrap();
        assert_eq!(line.target, None);
        assert_eq!(line.status, Some(StatusName("prone".into())));
    }
}
