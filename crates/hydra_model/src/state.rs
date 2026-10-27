//! The mutable client-side game model.
//!
//! `GameState::apply` (12920 B at `0x101bd5534`) is a dispatcher: its `bl`
//! targets are `Dialogs::read`, `CustomTimers::read`,
//! `InventorySnapshot::apply_page`, `Windows::declare`, `ship::aboard`,
//! `Creatures::apply_room_objs`, `StatusInfo::set`, `Group::emptied`,
//! `Creatures::note_status`, `Hand::read`, `Objectives::apply` and
//! `LearnedCommands::apply`. The compiler inlined most of those arms, so the
//! dispatch table itself is not recoverable line by line; [`GameState::apply`]
//! reproduces the recovered call order.
//!
//! `// UNKNOWN:` the argument type of `apply`. The parsers it calls go through
//! `Runs::plain` (`hydra_protocol::runs`), so the original received styled
//! runs; this std-only rebuild threads `&str` through every entry point and
//! documents the departure in `NOTES.md`.
//!
//! The struct is a faithful *field set* — the recovered member types and their
//! modules — but not a byte-faithful layout: only the clock block
//! (`+0x1118`..`+0x1150`) and the vitals map (`+0x1178`/`+0x1180`) offsets are
//! established.

pub mod afflictions;
pub mod bank;
pub mod bounty;
pub mod bounty_status;
pub mod character;
pub mod clock;
pub mod combat;
pub mod containers;
pub mod cooldowns;
pub mod creature;
pub mod creature_message;
pub mod creatures;
pub mod custom_timers;
pub mod departure;
pub mod dialogs;
pub mod effects;
pub mod gameobj;
pub mod group;
pub mod hands;
pub mod incident;
pub mod inventory_snapshot;
pub mod known_spells;
pub mod lines;
pub mod login;
pub mod maneuvers;
pub mod menu;
pub mod message;
pub mod objectives;
pub mod overwatch;
pub mod reconnect;
pub mod room;
pub mod ship;
pub mod spell_time;
pub mod stream_windows;
pub mod stun;
pub mod targeting;
pub mod text;
pub mod vitals;

/// Everything the client tracks about the current session.
#[derive(Default, Clone, Debug)]
pub struct GameState {
    /// `+0x1118`..`+0x1150` in the binary.
    pub clock: clock::Clock,
    /// `+0x1178` (map root) / `+0x1180` (length).
    pub vitals: vitals::Vitals,
    pub status: crate::status::StatusInfo,
    pub character: character::Character,
    pub creatures: creatures::Creatures,
    pub combat: combat::tracker::CombatTracker,
    pub effects: effects::Effects,
    pub bounty: bounty_status::BountyStatus,
    pub timers: custom_timers::CustomTimers,
    pub menu: menu::MenuCommands,
    pub learned: menu::LearnedCommands,
    pub containers: containers::Containers,
    pub hands: hands::Hand,
    pub dialogs: dialogs::Dialogs,
    pub group: group::Group,
    pub ship: ship::Ship,
    pub bank: bank::Account,
    pub objectives: objectives::Objectives,
    pub inventory: inventory_snapshot::InventorySnapshot,
    pub windows: stream_windows::Windows,
    pub spells: known_spells::KnownSpells,
    pub room: room::Room,
    pub targeting: targeting::Targeting,
    pub overwatch: overwatch::Overwatch,
    pub cooldowns: cooldowns::Cooldowns,
    pub incidents: incident::Incidents,
    pub lines: lines::Chunk,
    pub timing: spell_time::SpellTiming,
    /// `// UNKNOWN:` login account type (`FREETRUE` in the recovered login block).
    pub account: Option<character::vocabulary::AccountType>,
}

/// One side effect of applying a line, as reported back to the caller.
///
/// `// UNKNOWN:` the original returned `Vec<Fact>`-shaped data from `apply`
/// (the string pool has `Fact`-adjacent field runs); the variant set below is
/// the minimum needed to observe the recovered dispatch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fact {
    /// A status flag changed.
    Status { name: String, value: bool },
    /// A vitals record changed.
    Vital(String),
    /// A timer was recorded.
    Timer(String),
    /// The character block was updated.
    Character,
    /// A creature was added or updated.
    Creature(String),
    /// A combat fact was recorded.
    Combat(String),
    /// A room/stream window was (re)declared.
    Window(String),
    /// The line was not recognised by any reader.
    Ignored,
}

impl GameState {
    pub fn new() -> Self {
        GameState::default()
    }

    /// Apply one line of game output, returning what changed.
    ///
    /// The recovered `apply` runs the readers in this order (recovered from the
    /// call order in the 12920-byte function); any reader may consume the line.
    pub fn apply(&mut self, line: &str) -> Vec<Fact> {
        let mut facts = Vec::new();
        if line.trim().is_empty() {
            return facts;
        }

        if let Some(flag) = self.status.set_from_line(line) {
            facts.push(flag);
        }
        if self.dialogs.read(line) {
            facts.push(Fact::Window("dialog".to_string()));
        }
        if self.timers.read(line) {
            facts.push(Fact::Timer(line.to_string()));
        }
        if self.inventory.apply_page(line) {
            facts.push(Fact::Window("inventory".to_string()));
        }
        if self.windows.declare(line) {
            facts.push(Fact::Window(line.to_string()));
        }
        if self.ship.aboard(line) {
            facts.push(Fact::Window("ship".to_string()));
        }
        if self.creatures.apply_room_objs(line) {
            facts.push(Fact::Creature(line.to_string()));
        }
        if self.group.emptied(line) {
            facts.push(Fact::Window("group".to_string()));
        }
        if self.hands.read(line) {
            facts.push(Fact::Combat("hands".to_string()));
        }
        if self.objectives.apply(line) {
            facts.push(Fact::Combat("objectives".to_string()));
        }
        if self.learned.read(line) > 0 {
            facts.push(Fact::Window("menu".to_string()));
        }
        if self.character.apply_line(line) {
            facts.push(Fact::Character);
        }
        if facts.is_empty() {
            facts.push(Fact::Ignored);
        }
        facts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_state_is_empty() {
        let state = GameState::new();
        assert!(state.status.flags.is_empty());
        assert!(state.vitals.map.is_empty());
        assert!(state.creatures.is_empty());
        assert_eq!(state.game_time_now(), None);
    }

    #[test]
    fn apply_ignores_blank_lines() {
        let mut state = GameState::new();
        assert!(state.apply("").is_empty());
        assert!(state.apply("   ").is_empty());
    }

    #[test]
    fn apply_reports_unrecognised_lines() {
        let mut state = GameState::new();
        assert_eq!(
            state.apply("The wind blows gently from the north."),
            vec![Fact::Ignored]
        );
    }

    #[test]
    fn apply_records_status_indicator_lines() {
        let mut state = GameState::new();
        let facts = state.apply("<indicator id=\"IconDEAD\" visible=\"y\"/>");
        assert!(state.status.dead());
        assert!(facts.iter().any(|f| matches!(f, Fact::Status { .. })));
    }
}
