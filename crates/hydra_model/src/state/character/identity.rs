//! Character identity helpers.
//!
//! `Character::exist_id` (320 B at `0x101bd87ac`) lives here; the disassembly
//! takes the character by reference and returns a 32-bit id (the string pool
//! near it holds the `<a exist=...>` marker names used by creature links).

impl crate::state::character::Character {
    /// @0x0000000101bd87ac (320 B)
    ///
    /// `// UNKNOWN:` the id is derived from stored creature/link state; the
    /// recovered function only reads the character block. Returning the stored
    /// field is the safe reconstruction.
    pub fn exist_id(&self) -> u32 {
        self.exist_id
    }

    /// Record the identity fields recovered from the `INFO` report.
    pub fn set_identity(&mut self, name: &str, race: &str, profession: &str) {
        self.name = name.to_string();
        self.race = race.to_string();
        self.profession = profession.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_fields_round_trip() {
        let mut c = crate::state::character::Character::default();
        c.set_identity("Kithar", "Human", "Wizard");
        assert_eq!(c.name, "Kithar");
        assert_eq!(c.race, "Human");
        assert_eq!(c.profession, "Wizard");
        assert_eq!(c.exist_id(), 0);
    }
}
