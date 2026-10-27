//! Attack resolution (`Resolution`): the `as`/`cs`/`uaf`/… codes.
//!
//! Recovered from `Resolution::classify` (1740 B): def family `"resolution"`,
//! group `"as"`, and the code names `cs`, `uaf`, `fs`, `ds`, `td`, `udf`,
//! `fd`, `avd`, `cva`, `mm` (plus `smr`, `ssr`, `activation`, `maneuver_roll`
//! from `ResolutionKind`'s serde variant list). The numeric value comes from
//! `f64::from_str` on the `as` group ([`crate::pattern::Groups`]) when the row
//! captures one, and from the first number on the line otherwise.
//!
//! `crit_rider_line` (164 B) uses the def family `"crit_rider"` with
//! `Defs::any_match`.

use super::defs::{defs, Defs};

/// The resolution kind codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResolutionKind {
    AttackStrength,
    CastStrength,
    Unattackable,
    FumbleStrength,
    DefenseStrength,
    TargetDefense,
    UnarmedDefense,
    FumbleDefense,
    Avoidance,
    CombatValue,
    ManeuverMargin,
    SpellManeuverRoll,
    SpellSavingRoll,
    Activation,
    ManeuverRoll,
}

impl ResolutionKind {
    /// The recovered code strings.
    pub fn code(self) -> &'static str {
        match self {
            ResolutionKind::AttackStrength => "as",
            ResolutionKind::CastStrength => "cs",
            ResolutionKind::Unattackable => "uaf",
            ResolutionKind::FumbleStrength => "fs",
            ResolutionKind::DefenseStrength => "ds",
            ResolutionKind::TargetDefense => "td",
            ResolutionKind::UnarmedDefense => "udf",
            ResolutionKind::FumbleDefense => "fd",
            ResolutionKind::Avoidance => "avd",
            ResolutionKind::CombatValue => "cva",
            ResolutionKind::ManeuverMargin => "mm",
            ResolutionKind::SpellManeuverRoll => "smr",
            ResolutionKind::SpellSavingRoll => "ssr",
            ResolutionKind::Activation => "activation",
            ResolutionKind::ManeuverRoll => "maneuver_roll",
        }
    }

    pub fn parse(code: &str) -> Option<ResolutionKind> {
        let code = code.trim();
        [
            ResolutionKind::AttackStrength,
            ResolutionKind::CastStrength,
            ResolutionKind::Unattackable,
            ResolutionKind::FumbleStrength,
            ResolutionKind::DefenseStrength,
            ResolutionKind::TargetDefense,
            ResolutionKind::UnarmedDefense,
            ResolutionKind::FumbleDefense,
            ResolutionKind::Avoidance,
            ResolutionKind::CombatValue,
            ResolutionKind::ManeuverMargin,
            ResolutionKind::SpellManeuverRoll,
            ResolutionKind::SpellSavingRoll,
            ResolutionKind::Activation,
            ResolutionKind::ManeuverRoll,
        ]
        .into_iter()
        .find(|kind| kind.code() == code)
    }
}

/// One resolution reading.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolution {
    pub kind: Option<ResolutionKind>,
    pub value: f64,
}

impl Resolution {
    /// @0x0000000101be4bb0 (1740 B)
    pub fn classify(text: &str) -> Option<Resolution> {
        Resolution::classify_with_defs(defs(), text)
    }

    /// [`Resolution::classify`] against a caller-supplied table.
    pub fn classify_with_defs(table: &Defs, text: &str) -> Option<Resolution> {
        let (def, groups) = table.first_match_groups(text)?;
        if def.family != "resolution" {
            return None;
        }
        Some(Resolution {
            kind: ResolutionKind::parse(&def.name),
            value: groups
                .get("as")
                .and_then(|group| group.trim().parse::<f64>().ok())
                .or_else(|| first_float(text))
                .unwrap_or(0.0),
        })
    }
}

/// @0x0000000101be54c0 (164 B) — does the line carry a critical rider?
pub fn crit_rider_line(text: &str) -> bool {
    crit_rider_line_with_defs(defs(), text)
}

/// [`crit_rider_line`] against a caller-supplied table.
pub fn crit_rider_line_with_defs(table: &Defs, text: &str) -> bool {
    table
        .family("crit_rider")
        .map(|def| def.matcher.is_match(text))
        .unwrap_or(false)
}

pub(crate) fn first_float(text: &str) -> Option<f64> {
    let start = text.find(|c: char| c.is_ascii_digit() || c == '-' || c == '.')?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::super::defs::Def;
    use super::*;

    #[test]
    fn codes_round_trip() {
        for kind in [
            ResolutionKind::AttackStrength,
            ResolutionKind::CastStrength,
            ResolutionKind::Unattackable,
            ResolutionKind::FumbleStrength,
            ResolutionKind::DefenseStrength,
            ResolutionKind::TargetDefense,
            ResolutionKind::UnarmedDefense,
            ResolutionKind::FumbleDefense,
            ResolutionKind::Avoidance,
            ResolutionKind::CombatValue,
            ResolutionKind::ManeuverMargin,
            ResolutionKind::SpellManeuverRoll,
            ResolutionKind::SpellSavingRoll,
            ResolutionKind::Activation,
            ResolutionKind::ManeuverRoll,
        ] {
            assert_eq!(ResolutionKind::parse(kind.code()), Some(kind));
        }
        assert_eq!(ResolutionKind::parse("zz"), None);
    }

    #[test]
    fn float_extraction() {
        assert_eq!(first_float("CS: 431"), Some(431.0));
        assert_eq!(first_float("no numbers"), None);
        assert_eq!(first_float("-12.5 margin"), Some(-12.5));
    }

    #[test]
    fn classify_without_definitions_is_none() {
        assert_eq!(Resolution::classify("CS: 431"), None);
        assert!(!crit_rider_line("anything"));
    }

    #[test]
    fn the_as_group_is_parsed_as_the_value() {
        let table = Defs {
            defs: vec![
                Def::new(
                    "resolution",
                    "as",
                    "damaging",
                    1,
                    "",
                    "",
                    "^AS: (?P<as>[-+]?[0-9.]+)",
                ),
                Def::new("crit_rider", "rider", "damaging", 2, "", "", "^Critical rider"),
            ],
        };
        let resolution =
            Resolution::classify_with_defs(&table, "AS: -12.5 (attack strength)").unwrap();
        assert_eq!(resolution.kind, Some(ResolutionKind::AttackStrength));
        assert_eq!(resolution.value, -12.5);
        assert!(crit_rider_line_with_defs(&table, "Critical rider"));
    }
}
