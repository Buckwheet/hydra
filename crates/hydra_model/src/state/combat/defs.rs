//! Combat line definitions (`Defs`).
//!
//! Recovered: `defs()` (56 B) is an accessor for the loaded table;
//! `Def::extra` (568 B) parses the `extra` column as `k=v;…`
//! (`damaging=1;aoe=0;spawns=0;key=ThornsImpact_GS;`); `Defs::first_match`
//! (788 B), `first_match_with_role` (780 B) and `any_match` (680 B) run the
//! compiled pattern set and read named capture groups
//! (`regex_automata::util::captures::Captures::get_group_by_name`), while
//! `swing_weapon` / `nock_weapon` / `first_flight` / `barrier` /
//! `gemstone_property` / `flare_names` are thin wrappers that look up the
//! `flare` / `flare_mirror` families.
//!
//! Column order, read out of the literal at `0x103…`:
//! `family  name  role  order  flags  extra  pattern`.
//!
//! `// UNKNOWN:` the table itself lives in a generated data file and is *not*
//! embedded as a whole (unlike `crit.tsv`); [`defs`] therefore returns an empty
//! table, and every `first_match`-style lookup answers `None`. That is the
//! behaviour the recovered code shows for an empty table.

use std::sync::OnceLock;

use crate::pattern::{compile, Pattern};

/// The recovered column order of the defs TSV.
pub const COLUMNS: [&str; 7] = ["family", "name", "role", "order", "flags", "extra", "pattern"];

/// One definition row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Def {
    pub family: String,
    pub name: String,
    pub role: String,
    pub order: u32,
    pub flags: String,
    pub extra: String,
    pub pattern: String,
    /// The compiled `pattern` column.
    pub matcher: Pattern,
}

impl Def {
    /// Build a row from the TSV columns.
    pub fn new(
        family: &str,
        name: &str,
        role: &str,
        order: u32,
        flags: &str,
        extra: &str,
        pattern: &str,
    ) -> Def {
        Def {
            family: family.to_string(),
            name: name.to_string(),
            role: role.to_string(),
            order,
            flags: flags.to_string(),
            extra: extra.to_string(),
            pattern: pattern.to_string(),
            matcher: compile(&format!("(?i){pattern}")),
        }
    }

    /// @0x0000000101b9f? (568 B) — parse the `extra` column (`k=v;k=v`).
    pub fn extra_values(&self) -> Vec<(&str, &str)> {
        self.extra
            .split(';')
            .filter_map(|pair| pair.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect()
    }

    /// One `extra` value.
    pub fn extra_value(&self, key: &str) -> Option<&str> {
        self.extra_values()
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }
}

/// The whole definition table.
#[derive(Clone, Debug, Default)]
pub struct Defs {
    pub defs: Vec<Def>,
}

impl Defs {
    /// @0x0000000101b9f? (788 B) — the first match that yields every required
    /// group.
    ///
    /// `// UNKNOWN:` the recovered body reads named groups
    /// (`get_group_by_name`); this engine has no capture support, so only the
    /// presence of a match is reported through [`Defs::first_def`].
    pub fn first_match(&self, line: &str) -> Option<&Def> {
        self.defs.iter().find(|def| def.matcher.is_match(line))
    }

    /// @0x0000000101b9f? (780 B) — like [`Defs::first_match`] but restricted to
    /// one `role` column value.
    pub fn first_match_with_role(&self, line: &str, role: &str) -> Option<&Def> {
        self.defs
            .iter()
            .find(|def| def.role == role && def.matcher.is_match(line))
    }

    /// @0x0000000101b9f? (680 B) — does *any* definition match?
    pub fn any_match(&self, line: &str) -> bool {
        self.defs.iter().any(|def| def.matcher.is_match(line))
    }

    /// The first definition of a family.
    pub fn family(&self, family: &str) -> Option<&Def> {
        self.defs.iter().find(|def| def.family == family)
    }

    /// @0x0000000101b9f708 (196 B) — the `flare` / `flare_mirror` first-flight
    /// marker.
    pub fn first_flight(&self, line: &str) -> Option<&Def> {
        self.defs
            .iter()
            .find(|def| matches!(def.family.as_str(), "flare" | "flare_mirror") && def.matcher.is_match(line))
    }

    /// @0x0000000101b9ff84 (184 B) — the `barrier` def for a line.
    pub fn barrier(&self, line: &str) -> Option<&Def> {
        self.defs
            .iter()
            .find(|def| def.family == "barrier" && def.matcher.is_match(line))
    }

    /// @0x0000000101b9ef4c (96 B) — the flare family names.
    pub fn flare_names(&self) -> Vec<&str> {
        self.defs
            .iter()
            .filter(|def| matches!(def.family.as_str(), "flare" | "flare_mirror"))
            .map(|def| def.name.as_str())
            .collect()
    }

    /// @0x0000000101b9f4f8 (528 B) — the `weapon` capture of a nock line.
    ///
    /// `// UNKNOWN:` needs named captures; returns the def instead.
    pub fn nock_weapon(&self, line: &str) -> Option<&Def> {
        self.defs
            .iter()
            .find(|def| def.family == "nock" && def.matcher.is_match(line))
    }

    /// @0x0000000101b9f7cc (528 B) — the `weapon` capture of a swing line.
    pub fn swing_weapon(&self, line: &str) -> Option<&Def> {
        self.defs
            .iter()
            .find(|def| def.family == "swing" && def.matcher.is_match(line))
    }

    /// @0x0000000101b9f9dc (440 B) — the gemstone `property` of a flare line.
    pub fn gemstone_property(&self, line: &str) -> Option<&Def> {
        self.defs
            .iter()
            .find(|def| def.family == "gemstone" && def.matcher.is_match(line))
    }
}

static DEFS: OnceLock<Defs> = OnceLock::new();

/// @0x0000000101b9? (56 B) — the process-wide table.
///
/// `// UNKNOWN:` the table is *not* embedded in this rebuild, so the accessor
/// hands back an empty table (see the module note).
pub fn defs() -> &'static Defs {
    DEFS.get_or_init(Defs::default)
}

/// Install a table for tests / callers that load their own data.
pub fn set_defs(table: Defs) -> bool {
    DEFS.set(table).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_column_parses_key_values() {
        let def = Def::new(
            "Thorns",
            "ThornsImpact_GS",
            "damaging",
            274,
            "",
            "damaging=1;aoe=0;spawns=0;key=ThornsImpact_GS;",
            "^The thorny barrier",
        );
        assert_eq!(def.extra_value("damaging"), Some("1"));
        assert_eq!(def.extra_value("key"), Some("ThornsImpact_GS"));
        assert_eq!(def.extra_value("missing"), None);
        assert_eq!(def.extra_values().len(), 4);
    }

    #[test]
    fn empty_table_answers_none() {
        let defs = Defs::default();
        assert!(defs.first_match("anything").is_none());
        assert!(!defs.any_match("anything"));
        assert!(defs.family("flare").is_none());
        assert!(defs.flare_names().is_empty());
        assert!(defs.barrier("x").is_none());
        assert!(defs.nock_weapon("x").is_none());
    }

    #[test]
    fn lookup_uses_the_compiled_pattern_and_role() {
        let defs = Defs {
            defs: vec![
                Def::new("swing", "swing", "damaging", 1, "", "", "^You swing at"),
                Def::new("nock", "nock", "setup", 2, "", "", "^You nock an arrow"),
            ],
        };
        assert_eq!(defs.first_match("You swing at the orc.").unwrap().name, "swing");
        assert!(defs.first_match("You nock an arrow.").is_some());
        assert_eq!(
            defs.first_match_with_role("You nock an arrow.", "setup")
                .unwrap()
                .order,
            2
        );
        assert!(defs.any_match("You swing at the orc."));
        assert!(!defs.any_match("You stand still."));
    }

    #[test]
    fn flare_families_are_selected() {
        let defs = Defs {
            defs: vec![
                Def::new("flare", "flare", "damaging", 1, "", "", "^A burst of flame"),
                Def::new("flare_mirror", "flare_mirror", "damaging", 2, "", "", "^A mirror"),
            ],
        };
        assert_eq!(defs.flare_names(), vec!["flare", "flare_mirror"]);
        assert!(defs.first_flight("A burst of flame erupts.").is_some());
    }
}
