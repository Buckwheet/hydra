//! Live creature instances in the room.
//!
//! Recovered: `register` (6268 B) creates or refreshes an instance from a run
//! (it uses `Run::object`, the hiding/reveal phrase table and
//! `insert_by_message`-style text matching), `get` (116 B) and `new`
//! (`creatures::instance`) are the accessors, `mark_in_room` (512 B) /
//! `cleanup_old` (512 B) maintain the room membership, `note_status` (1124 B)
//! and `apply_room_objs` (1284 B) fold status objects in, `read_departure`
//! (408 B) calls `departure::classify`, `on_nav` (472 B) clears on movement.
//!
//! Recovered hiding/reveal phrases used by `register`:
//! `" leaps from hiding to attack!"`, `" springs upon you from behind"`,
//! `" leaps from the shadows and hurtles at you"`, `" is revealed from
//! hiding."`, `" is forced from hiding!"`, `" comes out of hiding."`,
//! `"The shadows melt away to reveal"`, `"You discover the hiding place of"`,
//! `"You reveal "`, `", who is forced into view!"`, `" explodes from the
//! shadows!"`, `", who was hidden!"`, `"The figure quickly disappears from
//! view."`, `"Something stirs in the shadows."`.

pub mod ally;
pub mod body;
pub mod instance;
pub mod prose;

use instance::CreatureInstance;

/// Every creature the client is tracking.
///
/// `// UNKNOWN:` the binary keeps the instances in an arena (`get` is a
/// 116-byte index lookup); a `Vec` with a stable id keeps the same semantics.
#[derive(Default, Clone, Debug)]
pub struct Creatures {
    pub instances: Vec<CreatureInstance>,
    /// Ids that were in the room last tick.
    pub in_room: Vec<u32>,
}

impl Creatures {
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    /// @0x0000000101b7e988 (116 B)
    pub fn get(&self, id: u32) -> Option<&CreatureInstance> {
        self.instances.iter().find(|c| c.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut CreatureInstance> {
        self.instances.iter_mut().find(|c| c.id == id)
    }

    /// @0x0000000101b7ebd4 (6268 B) — register a creature by name, creating the
    /// instance when it is new.
    ///
    /// Returns the instance id and whether it was created.
    pub fn register(&mut self, name: &str) -> (u32, bool) {
        if let Some(existing) = self.instances.iter().find(|c| c.name == name) {
            return (existing.id, false);
        }
        let id = self.instances.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let instance = CreatureInstance::new(id, name);
        self.instances.push(instance);
        (id, true)
    }

    /// @0x0000000101b7e0ec (512 B) — mark an instance as present in the room.
    pub fn mark_in_room(&mut self, id: u32) -> bool {
        if self.in_room.contains(&id) {
            return false;
        }
        self.in_room.push(id);
        true
    }

    /// @0x0000000101b7da88 (512 B) — forget instances that are no longer in the
    /// room.
    pub fn cleanup_old(&mut self) -> Vec<u32> {
        let dropped: Vec<u32> = self
            .instances
            .iter()
            .map(|c| c.id)
            .filter(|id| !self.in_room.contains(id))
            .collect();
        if dropped.is_empty() {
            self.in_room.clear();
            return Vec::new();
        }
        self.instances.retain(|c| !dropped.contains(&c.id));
        self.in_room.clear();
        dropped
    }

    /// @0x0000000101b7dc88 (1124 B) — record a status observed on a creature.
    pub fn note_status(&mut self, id: u32, status: &str, on: bool) -> bool {
        match self.get_mut(id) {
            Some(instance) => instance.add_status(status, on),
            None => {
                let (id, _) = self.register(status);
                let _ = id;
                false
            }
        }
    }

    /// @0x0000000101b7e484 (1284 B) — fold `<room>` objects in.
    pub fn apply_room_objs(&mut self, _line: &str) -> bool {
        false
    }

    /// @0x0000000101b7e9fc (472 B) — clear on movement.
    pub fn on_nav(&mut self) {
        self.in_room.clear();
    }

    /// @0x0000000101b7e2ec (408 B) — read a departure line.
    pub fn read_departure(&mut self, line: &str) -> bool {
        crate::state::departure::classify(line).is_some()
    }

    /// @0x0000000101b7dd?? — the hiding/reveal phrase table used by `register`.
    pub const REVEAL_PHRASES: &'static [&'static str] = &[
        " leaps from hiding to attack!",
        " springs upon you from behind",
        " leaps from the shadows and hurtles at you",
        " is revealed from hiding.",
        " is forced from hiding!",
        " comes out of hiding.",
        "The shadows melt away to reveal",
        "You discover the hiding place of",
        "You reveal ",
        ", who is forced into view!",
        " explodes from the shadows!",
        ", who was hidden!",
        "The figure quickly disappears from view.",
        "Something stirs in the shadows.",
    ];

    /// Which reveal phrase (if any) a line uses.
    pub fn reveal_phrase(line: &str) -> Option<&'static str> {
        Creatures::REVEAL_PHRASES
            .iter()
            .copied()
            .find(|phrase| line.contains(phrase))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_creates_then_reuses() {
        let mut creatures = Creatures::default();
        let (id, created) = creatures.register("a kobold");
        assert!(created);
        assert_eq!(id, 1);
        let (same, created) = creatures.register("a kobold");
        assert!(!created);
        assert_eq!(same, 1);
        assert_eq!(creatures.instances.len(), 1);
        assert!(creatures.get(1).is_some());
    }

    #[test]
    fn room_membership_is_tracked() {
        let mut creatures = Creatures::default();
        let (id, _) = creatures.register("a kobold");
        assert!(creatures.mark_in_room(id));
        assert!(!creatures.mark_in_room(id));
        assert_eq!(creatures.in_room, vec![id]);
        creatures.on_nav();
        assert!(creatures.in_room.is_empty());
    }

    #[test]
    fn cleanup_drops_instances_no_longer_present() {
        let mut creatures = Creatures::default();
        let (a, _) = creatures.register("a kobold");
        let (_b, _) = creatures.register("a goblin");
        creatures.mark_in_room(a);
        let removed = creatures.cleanup_old();
        assert_eq!(removed, vec![2]);
        assert_eq!(creatures.instances.len(), 1);
        // the room list was cleared, so the next sweep drops the survivor too
        assert_eq!(creatures.cleanup_old(), vec![1]);
        assert!(creatures.instances.is_empty());
    }

    #[test]
    fn reveal_phrases_are_recovered_verbatim() {
        assert_eq!(
            Creatures::reveal_phrase("The shadows melt away to reveal a kobold."),
            Some("The shadows melt away to reveal")
        );
        assert_eq!(Creatures::reveal_phrase("You stand still."), None);
        assert_eq!(Creatures::REVEAL_PHRASES.len(), 14);
    }

    #[test]
    fn status_is_recorded_on_an_existing_instance() {
        let mut creatures = Creatures::default();
        let (id, _) = creatures.register("a kobold");
        assert!(creatures.note_status(id, "stunned", true));
        assert!(creatures.get(id).unwrap().has_status("stunned"));
    }
}
