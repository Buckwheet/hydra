//! Shadow-essence / animate messages.
//!
//! `shadow::line` (1616 B) matches the recovered literals
//! `"forming shadow essence."`, `"You begin to sacrifice your victim but
//! immediately sense tha…"`, `"Focusing on the bond to your animate, you force
//! shadow essence into "`, `"There is a flood of power as you feel "`,
//! `" mana surge into you!"`, `"the unnatural revitalization of its animate
//! matter."`, `"Mastering the struggle against the frantic rush of stolen
//! po…"`, `"You close your eyes momentarily and visualize the strands of…"`,
//! `"Summoning the shadow essence within yourself, you will it to…"`.

/// The kind of shadow-essence event a line describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShadowEvent {
    EssenceForming,
    SacrificeBegan,
    EssenceForced,
    ManaSurge,
    StruggleMastered,
    Visualising,
    Summoning,
}

/// @0x0000000101ba2168 (1616 B)
pub fn line(text: &str) -> Option<ShadowEvent> {
    const TABLE: &[(&str, ShadowEvent)] = &[
        ("forming shadow essence.", ShadowEvent::EssenceForming),
        (
            "You begin to sacrifice your victim",
            ShadowEvent::SacrificeBegan,
        ),
        ("you force shadow essence into ", ShadowEvent::EssenceForced),
        ("There is a flood of power", ShadowEvent::ManaSurge),
        (" mana surge into you!", ShadowEvent::ManaSurge),
        (
            "unnatural revitalization of its animate matter",
            ShadowEvent::StruggleMastered,
        ),
        (
            "Mastering the struggle against the frantic rush of stolen",
            ShadowEvent::StruggleMastered,
        ),
        (
            "You close your eyes momentarily and visualize the strands",
            ShadowEvent::Visualising,
        ),
        ("Summoning the shadow essence within yourself", ShadowEvent::Summoning),
    ];
    TABLE
        .iter()
        .find(|(needle, _)| text.contains(needle))
        .map(|(_, event)| *event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovered_phrases_map_to_events() {
        assert_eq!(
            line("You sense the forming shadow essence."),
            Some(ShadowEvent::EssenceForming)
        );
        assert_eq!(
            line("There is a flood of power as you feel the bond strengthen."),
            Some(ShadowEvent::ManaSurge)
        );
        assert_eq!(
            line("Summoning the shadow essence within yourself, you will it to obey."),
            Some(ShadowEvent::Summoning)
        );
    }

    #[test]
    fn unrelated_lines_are_none() {
        assert_eq!(line("You feel a gentle breeze."), None);
    }
}
