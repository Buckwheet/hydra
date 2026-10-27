//! Creature messages.
//!
//! `classify` (54548 B — the largest function in the crate) dispatches to every
//! combat/creature classifier: `match_template` (2680 B), `combat::ucs`,
//! `combat::status`, `combat::target::is_self`, `combat::flare`,
//! `combat::spell_loss`, `combat::resolution`, `combat::outcome`,
//! `combat::resolution::crit_rider_line`, `combat::attack::swing_weapon`,
//! `combat::defs::Defs::swing_weapon`, `combat::attack::attackerless_line`,
//! `combat::damage`, `combat::attack::coup_kill_location`,
//! `CritTables::parse`, `combat::bracket::assault_start/end`.
//! Recovered literals include `"Maximum Health Points:"`, the body parts
//! (`head neck chest abdomen back nerves right_leg left_leg …`) and the
//! postures (`kneeling prone sitting`).

/// What a creature message turned out to be.
#[derive(Clone, Debug, PartialEq)]
pub enum CreatureMessage<'a> {
    Attack(crate::state::combat::attack::AttackLine),
    Damage(crate::state::combat::damage::DamageLine),
    Flare(crate::state::combat::flare::FlareLine),
    Status(crate::state::combat::status::StatusLine),
    Outcome(crate::state::combat::outcome::Outcome),
    Resolution(crate::state::combat::resolution::Resolution),
    SpellLoss(crate::state::combat::spell_loss::SpellLoss),
    Ucs(crate::state::combat::ucs::UcsLine),
    Crit(CritMatch<'a>),
    Assault,
}

/// A critical-hit match: either the `crit_rider` marker or a table row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CritMatch<'a> {
    Rider,
    Table(&'a str),
}

/// @0x0000000101b6d690 (54548 B)
///
/// Runs the recovered dispatch chain and returns the first classifier that
/// claims the line.
pub fn classify<'a>(
    state: &'a crate::state::GameState,
    line: &str,
    links: &[hydra_protocol::frame::payload::Link],
) -> Option<CreatureMessage<'a>> {
    use crate::state::combat;
    if let Some(ucs) = combat::ucs::UcsLine::classify(line) {
        return Some(CreatureMessage::Ucs(ucs));
    }
    if let Some(status) = combat::status::StatusLine::classify(line) {
        return Some(CreatureMessage::Status(status));
    }
    if let Some(flare) = combat::flare::FlareLine::classify(line) {
        return Some(CreatureMessage::Flare(flare));
    }
    if let Some(loss) = combat::spell_loss::SpellLoss::classify(line) {
        return Some(CreatureMessage::SpellLoss(loss));
    }
    if let Some(resolution) = combat::resolution::Resolution::classify(line) {
        return Some(CreatureMessage::Resolution(resolution));
    }
    if combat::resolution::crit_rider_line(line) {
        return Some(CreatureMessage::Crit(CritMatch::Rider));
    }
    if let Some(outcome) = combat::outcome::Outcome::classify(line) {
        return Some(CreatureMessage::Outcome(outcome));
    }
    if let Some(attack) = combat::attack::AttackLine::classify(line) {
        return Some(CreatureMessage::Attack(attack));
    }
    if let Some(damage) = combat::damage::DamageLine::classify(line) {
        return Some(CreatureMessage::Damage(damage));
    }
    if let Some(tables) = state.combat.crit_tables.as_ref() {
        if let Some(entry) = tables.best(line) {
            return Some(CreatureMessage::Crit(CritMatch::Table(&entry.damage_type)));
        }
    }
    let _ = links;
    match_template(line)
}

/// @0x0000000101b6cc18 (2680 B) — the plain-text template table.
///
/// `// UNKNOWN:` the template list; the recovered body sorts the templates by
/// length and tries each. Only the health phrase is recovered
/// (`"Maximum Health Points:"`).
pub fn match_template(line: &str) -> Option<CreatureMessage<'static>> {
    if line.contains("Maximum Health Points:") {
        return Some(CreatureMessage::Crit(CritMatch::Table("health")));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_def_table_leaves_only_text_matches() {
        let state = crate::state::GameState::default();
        assert_eq!(classify(&state, "You stand still.", &[]), None);
        assert!(classify(&state, "Maximum Health Points: 100", &[]).is_some());
    }

    #[test]
    fn crit_table_matches_are_used_when_installed() {
        let mut state = crate::state::GameState::default();
        state
            .combat
            .set_crit_tables(crate::crit::CritTables::load().unwrap());
        let classified = classify(&state, "Too bad the kobold ducked.", &[]).unwrap();
        assert_eq!(
            classified,
            CreatureMessage::Crit(CritMatch::Table("acid"))
        );
    }
}
