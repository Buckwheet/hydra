//! Incident log (combat incidents awaiting resolution).
//!
//! `read` (2860 B) builds one from a run (`Runs::plain`, `Run::object`,
//! `text::creature`), `Incidents::push` (580 B) queues it.

/// One recorded incident.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Incident {
    pub creature: Option<String>,
    pub text: String,
}

/// The incident queue.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Incidents {
    pub items: Vec<Incident>,
}

impl Incidents {
    /// @0x0000000101b? (580 B)
    pub fn push(&mut self, incident: Incident) -> bool {
        if self.items.iter().any(|i| i.text == incident.text) {
            return false;
        }
        self.items.push(incident);
        true
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// @0x0000000101b93cb4 (2860 B)
pub fn read(line: &str) -> Option<Incident> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    Some(Incident {
        creature: crate::state::text::creature(line),
        text: line.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incidents_are_deduplicated() {
        let mut incidents = Incidents::default();
        let incident = read("A kobold strikes you.").unwrap();
        assert!(incidents.push(incident.clone()));
        assert!(!incidents.push(incident));
        assert_eq!(incidents.len(), 1);
    }

    #[test]
    fn read_extracts_the_creature() {
        let incident = read("A scruffy kobold swings at you.").unwrap();
        assert!(incident.creature.is_some());
        assert_eq!(read(""), None);
    }
}
