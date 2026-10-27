//! Creature prose lines (aura/rebuke/death messages).
//!
//! `prose::read` (2072 B) literals: `"is suddenly surrounded by a blood red
//! haze"`, `"the blood red haze dissipates from around"`, `"visibly struggling
//! against your radiant aura!"`, `"in awe of your radiant aura!"`,
//! `"recovers from being rebuked"`, plus the creature names
//! `"battle-worn empyrean captain"` and `"cold wyrm"`.

use crate::state::creatures::Creatures;

/// The prose event a line describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProseEvent {
    BloodHazeGained,
    BloodHazeLost,
    StrugglingAgainstAura,
    InAweOfAura,
    RecoveredFromRebuke,
}

/// @0x0000000101be74cc (2072 B)
pub fn read(creatures: &mut Creatures, line: &str) -> Option<ProseEvent> {
    let event = if line.contains("is suddenly surrounded by a blood red haze") {
        ProseEvent::BloodHazeGained
    } else if line.contains("the blood red haze dissipates from around") {
        ProseEvent::BloodHazeLost
    } else if line.contains("visibly struggling against your radiant aura!") {
        ProseEvent::StrugglingAgainstAura
    } else if line.contains("in awe of your radiant aura!") {
        ProseEvent::InAweOfAura
    } else if line.contains("recovers from being rebuked") {
        ProseEvent::RecoveredFromRebuke
    } else {
        return None;
    };
    if let Some(name) = crate::state::text::creature(line) {
        let (id, _) = creatures.register(&name);
        let on = matches!(event, ProseEvent::BloodHazeGained | ProseEvent::RecoveredFromRebuke);
        if let Some(instance) = creatures.get_mut(id) {
            instance.add_status("blood_red_haze", on);
        }
    }
    Some(event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haze_events_are_recognised() {
        let mut creatures = Creatures::default();
        assert_eq!(
            read(&mut creatures, "A kobold is suddenly surrounded by a blood red haze."),
            Some(ProseEvent::BloodHazeGained)
        );
        assert_eq!(
            read(&mut creatures, "the blood red haze dissipates from around a kobold."),
            Some(ProseEvent::BloodHazeLost)
        );
        assert_eq!(read(&mut creatures, "nothing happens"), None);
    }

    #[test]
    fn aura_events_are_recognised() {
        let mut creatures = Creatures::default();
        assert_eq!(
            read(&mut creatures, "A kobold is visibly struggling against your radiant aura!"),
            Some(ProseEvent::StrugglingAgainstAura)
        );
        assert_eq!(
            read(&mut creatures, "A kobold stands in awe of your radiant aura!"),
            Some(ProseEvent::InAweOfAura)
        );
    }
}
