//! Attack resolution (`Resolution`): the `as`/`cs`/`uaf`/… codes.
//!
//! Recovered from `Resolution::classify` (1740 B): def family `"resolution"`,
//! group `"as"`, and the code names `cs`, `uaf`, `fs`, `ds`, `td`, `udf`,
//! `fd`, `avd`, `cva`, `mm` (plus `smr`, `ssr`, `activation`, `maneuver_roll`
//! from `ResolutionKind`'s serde variant list). The numeric value comes from
//! `f64::from_str`.
//!
//! `crit_rider_line` (164 B) uses the def family `"crit_rider"` with
//! `Defs::any_match`.

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
        let def = super::defs::defs().first_match(text)?;
        if def.family != "resolution" {
            return None;
        }
        Some(Resolution {
            kind: ResolutionKind::parse(&def.name),
            value: first_float(text).unwrap_or(0.0),
        })
    }
}

/// @0x0000000101be54c0 (164 B) — does the line carry a critical rider?
pub fn crit_rider_line(text: &str) -> bool {
    super::defs::defs()
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
}
