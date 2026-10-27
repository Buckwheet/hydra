//! Stow/ready containers.
//!
//! Recovered literals from `Containers::ready` (3768 B): `"Set \""`, `"\" to be
//! your "`, `" STOW container."`, `" container."`, `" to be your default "`;
//! `Containers::apply` (2620 B) folds `<container>`/`<dialogData>` blocks in;
//! `StowSlot::parse` (180 B), `StoreMode::parse` (3396 B) and
//! `ReadySlot::parse` (2856 B) parse the enums; `store_mode` (48 B) and `stow`
//! (76 B) are accessors.

use std::collections::BTreeMap;

/// Which hand/ready slot a container occupies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReadySlot {
    #[default]
    None,
    Left,
    Right,
    Both,
}

impl ReadySlot {
    /// @0x0000000101bdc?? (2856 B)
    pub fn parse(text: &str) -> ReadySlot {
        match text.trim().to_ascii_lowercase().as_str() {
            "left" => ReadySlot::Left,
            "right" => ReadySlot::Right,
            "both" => ReadySlot::Both,
            _ => ReadySlot::None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ReadySlot::None => "",
            ReadySlot::Left => "left",
            ReadySlot::Right => "right",
            ReadySlot::Both => "both",
        }
    }
}

/// The stow mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StoreMode {
    #[default]
    None,
    Put,
    Stow,
    Wear,
    Sheath,
}

impl StoreMode {
    /// @0x0000000101bde440 (3396 B)
    pub fn parse(text: &str) -> StoreMode {
        let text = text.trim().to_ascii_lowercase();
        if text.contains("you don't have access") {
            return StoreMode::None;
        }
        match text.as_str() {
            "put" => StoreMode::Put,
            "stow" => StoreMode::Stow,
            "wear" => StoreMode::Wear,
            "sheath" => StoreMode::Sheath,
            _ => StoreMode::None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            StoreMode::None => "",
            StoreMode::Put => "put",
            StoreMode::Stow => "stow",
            StoreMode::Wear => "wear",
            StoreMode::Sheath => "sheath",
        }
    }
}

/// Which container is the STOW / ready target.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StowSlot {
    #[default]
    None,
    Stow,
    Ready,
}

impl StowSlot {
    /// @0x0000000101bdd864 (180 B)
    pub fn parse(text: &str) -> StowSlot {
        let text = text.trim().to_ascii_lowercase();
        if text.contains("stow") {
            StowSlot::Stow
        } else if text.contains("ready") {
            StowSlot::Ready
        } else {
            StowSlot::None
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            StowSlot::None => "",
            StowSlot::Stow => "stow",
            StowSlot::Ready => "ready",
        }
    }
}

/// The container configuration.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct Containers {
    /// slot -> container description
    pub slots: BTreeMap<StowSlot, String>,
    pub mode: StoreMode,
    pub ready: ReadySlot,
}

impl Containers {
    /// @0x0000000101bdbf70 (2620 B)
    pub fn apply(&mut self, line: &str) -> bool {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Set \"") {
            if let Some((name, tail)) = rest.split_once("\" to be your ") {
                let slot = if tail.contains("STOW") {
                    StowSlot::Stow
                } else {
                    StowSlot::Ready
                };
                if self.slots.get(&slot).map(String::as_str) == Some(name) {
                    return false;
                }
                self.slots.insert(slot, name.to_string());
                return true;
            }
        }
        false
    }

    /// @0x0000000101bdc9ac (3768 B) — the `ready` message.
    pub fn ready(&mut self, line: &str) -> bool {
        self.apply(line)
    }

    /// @0x0000000101b? (48 B)
    pub fn store_mode(&self) -> StoreMode {
        self.mode
    }

    /// @0x0000000101b? (76 B)
    pub fn stow(&self) -> Option<&str> {
        self.slots.get(&StowSlot::Stow).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_messages_set_the_slot() {
        let mut containers = Containers::default();
        assert!(containers.apply("Set \"a leather backpack\" to be your STOW container."));
        assert_eq!(containers.stow(), Some("a leather backpack"));
        assert!(!containers.apply("Set \"a leather backpack\" to be your STOW container."));
    }

    #[test]
    fn ready_slot_parse() {
        assert_eq!(ReadySlot::parse("left"), ReadySlot::Left);
        assert_eq!(ReadySlot::parse("RIGHT"), ReadySlot::Right);
        assert_eq!(ReadySlot::parse("both"), ReadySlot::Both);
        assert_eq!(ReadySlot::parse(""), ReadySlot::None);
    }

    #[test]
    fn store_mode_parse() {
        assert_eq!(StoreMode::parse("put"), StoreMode::Put);
        assert_eq!(StoreMode::parse("sheath"), StoreMode::Sheath);
        assert_eq!(StoreMode::parse("you don't have access"), StoreMode::None);
    }

    #[test]
    fn stow_slot_parse() {
        assert_eq!(StowSlot::parse("STOW container"), StowSlot::Stow);
        assert_eq!(StowSlot::parse("ready"), StowSlot::Ready);
        assert_eq!(StowSlot::parse("x"), StowSlot::None);
    }
}
