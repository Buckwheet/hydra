//! Standing: society affiliation, citizenship, warcries, resources, urchins.
//!
//! Recovered literals (per function):
//!
//! * `society_line` (3888 B): `"The Grandmaster says, \"You are now a member of
//!   the Guardians of Sunfist"`, `"The Grand Poohbah smiles broadly.  \"Welcome
//!   to the Lodge,\" he cries"`, `"The Grandmaster says, \"I'm sorry to hear
//!   that.  You are no longer in our service."`, `"I wish you well with any of
//!   your future endeavors."`, `" at "`, and the three society names
//!   `Order of Voln` / `Council of Light` / `Guardians of Sunfist`.
//! * `citizenship_line` (472 B): `" citizenship in "`.
//! * `warcry_line` (432 B): `"You must be an active member of the Warrior Guild
//!   to use this skill."`.
//! * `resource_line` (308 B): `": "`, `"/50,000 (Weekly)"`, `"/200,000 (Total)"`.
//! * `suffused_line` (220 B): `": "` + `ResourceType::parse`.
//! * `urchin_until` (1892 B): `"You have permanent access to the urchin
//!   guides"`, `"You currently have no access to the urchin guides"`.
//! * `Standing::urchins` (60 B) is a plain accessor.
//!
//! `// UNKNOWN:` how the recovered `" at "` split is used beyond the society
//! name; only the name extraction is implemented.

use super::vocabulary::{ResourceType, Society};

/// Everything the `STANDING`/society output reports.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// `Order of Voln`, `Council of Light`, `Guardians of Sunfist` or `None`.
    pub society: Option<Society>,
    pub citizenship: Option<String>,
    /// Weekly / total resource amounts.
    pub resources: ResourceAmounts,
    /// `true` = permanent access, `false` = none.
    pub urchins: Option<bool>,
    pub warcry: Option<String>,
    /// Suffused/charged amounts by resource type.
    pub suffused: std::collections::BTreeMap<ResourceType, u32>,
}

/// The two resource counters.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceAmounts {
    pub weekly: u32,
    pub total: u32,
}

impl Standing {
    /// @0x0000000101b5b9d4 (60 B)
    pub fn urchins(&self) -> Option<bool> {
        self.urchins
    }
}

/// @0x0000000101b59b2c (3888 B) — society membership changes.
pub fn society_line(standing: &mut Standing, line: &str) -> bool {
    if line.contains("You are now a member of the Guardians of Sunfist")
        || line.contains("Welcome to the Lodge")
    {
        let society = if line.contains("Guardians of Sunfist") {
            Society::GuardiansOfSunfist
        } else {
            Society::OrderOfVoln
        };
        if standing.society == Some(society) {
            return false;
        }
        standing.society = Some(society);
        return true;
    }
    if line.contains("You are no longer in our service")
        || line.contains("I wish you well with any of your future endeavors")
    {
        if standing.society.is_none() {
            return false;
        }
        standing.society = None;
        return true;
    }
    // "Council of Light" / "Order of Voln" mentions
    for society in [Society::OrderOfVoln, Society::CouncilOfLight, Society::GuardiansOfSunfist] {
        if line.contains(society.name()) && standing.society != Some(society) {
            standing.society = Some(society);
            return true;
        }
    }
    false
}

/// @0x0000000101b5b5a8 (1068 B) — the `PROFILE` affiliation line.
pub fn profile_affiliation(standing: &mut Standing, line: &str) -> bool {
    society_line(standing, line)
}

/// @0x0000000101b5b3d0 (472 B) — `"<town> citizenship in ..."`.
pub fn citizenship_line(standing: &mut Standing, line: &str) -> bool {
    let Some((town, _)) = line.split_once(" citizenship in ") else {
        return false;
    };
    let town = town.trim();
    if town.is_empty() || standing.citizenship.as_deref() == Some(town) {
        return false;
    }
    standing.citizenship = Some(town.to_string());
    true
}

/// @0x0000000101b5b1c0 (308 B) — `"<Resource> <n>/50,000 (Weekly)"`,
/// `"... <n>/200,000 (Total)"`.
pub fn resource_line(standing: &mut Standing, line: &str) -> bool {
    let Some((label, rest)) = line.split_once(": ") else {
        return false;
    };
    let Some(kind) = ResourceType::parse(label) else {
        return false;
    };
    let number: u32 = rest
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(0);
    let mut changed = false;
    if rest.contains("(Weekly)") {
        changed |= standing.resources.weekly != number;
        standing.resources.weekly = number;
    } else if rest.contains("(Total)") {
        changed |= standing.resources.total != number;
        standing.resources.total = number;
    } else {
        changed |= standing.suffused.get(&kind) != Some(&number);
        standing.suffused.insert(kind, number);
    }
    changed
}

/// @0x0000000101b5b2f4 (220 B) — `"<Resource>: <n>"`.
pub fn suffused_line(standing: &mut Standing, line: &str) -> bool {
    let Some((label, rest)) = line.split_once(": ") else {
        return false;
    };
    let Some(kind) = ResourceType::parse(label) else {
        return false;
    };
    let number: u32 = rest.trim().parse().unwrap_or(0);
    if standing.suffused.get(&kind) == Some(&number) {
        return false;
    }
    standing.suffused.insert(kind, number);
    true
}

/// @0x0000000101b5b2f4 (432 B) — warcry refusal / availability.
pub fn warcry_line(standing: &mut Standing, line: &str) -> bool {
    if line.contains("You must be an active member of the Warrior Guild to use this skill.") {
        if standing.warcry.as_deref() == Some("") {
            return false;
        }
        standing.warcry = Some(String::new());
        return true;
    }
    if let Some(rest) = line.strip_prefix("You bellow") {
        let cry = rest.trim().trim_end_matches('!').trim().to_string();
        if standing.warcry.as_deref() == Some(cry.as_str()) {
            return false;
        }
        standing.warcry = Some(cry);
        return true;
    }
    false
}

/// @0x0000000101b5aa5c (1892 B) — urchin-guide access.
pub fn urchin_until(standing: &mut Standing, line: &str) -> bool {
    let verdict = if line.contains("You have permanent access to the urchin guides") {
        Some(true)
    } else if line.contains("You currently have no access to the urchin guides") {
        Some(false)
    } else {
        None
    };
    match verdict {
        Some(value) if standing.urchins != Some(value) => {
            standing.urchins = Some(value);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn society_membership_is_recorded_and_cleared() {
        let mut standing = Standing::default();
        assert!(society_line(
            &mut standing,
            "The Grandmaster says, \"You are now a member of the Guardians of Sunfist"
        ));
        assert_eq!(standing.society, Some(Society::GuardiansOfSunfist));
        assert!(!society_line(
            &mut standing,
            "The Grandmaster says, \"You are now a member of the Guardians of Sunfist"
        ));
        assert!(society_line(
            &mut standing,
            "The Grandmaster says, \"I'm sorry to hear that.  You are no longer in our service."
        ));
        assert_eq!(standing.society, None);
    }

    #[test]
    fn citizenship_line_extracts_the_town() {
        let mut standing = Standing::default();
        assert!(citizenship_line(&mut standing, "Wehnimer's Landing citizenship in ..."));
        assert_eq!(standing.citizenship.as_deref(), Some("Wehnimer's Landing"));
    }

    #[test]
    fn resource_lines_parse_both_counters() {
        let mut standing = Standing::default();
        assert!(resource_line(&mut standing, "Bounty Points: 1500/50,000 (Weekly)"));
        assert_eq!(standing.resources.weekly, 1500);
        assert!(resource_line(&mut standing, "Bounty Points: 4321/200,000 (Total)"));
        assert_eq!(standing.resources.total, 4321);
        assert!(!resource_line(&mut standing, "Bounty Points: 4321/200,000 (Total)"));
    }

    #[test]
    fn suffused_line_records_by_resource() {
        let mut standing = Standing::default();
        assert!(suffused_line(&mut standing, "Covert Arts Charges: 7"));
        assert_eq!(
            standing.suffused.get(&ResourceType::CovertArtsCharges),
            Some(&7)
        );
    }

    #[test]
    fn urchin_access_is_three_state() {
        let mut standing = Standing::default();
        assert_eq!(standing.urchins(), None);
        assert!(urchin_until(
            &mut standing,
            "You have permanent access to the urchin guides"
        ));
        assert_eq!(standing.urchins(), Some(true));
        assert!(!urchin_until(
            &mut standing,
            "You have permanent access to the urchin guides"
        ));
    }

    #[test]
    fn warcry_refusal_is_recorded() {
        let mut standing = Standing::default();
        assert!(warcry_line(
            &mut standing,
            "You must be an active member of the Warrior Guild to use this skill."
        ));
        assert_eq!(standing.warcry.as_deref(), Some(""));
        assert!(!warcry_line(
            &mut standing,
            "You must be an active member of the Warrior Guild to use this skill."
        ));
    }
}
