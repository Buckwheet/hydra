//! Items a walk keeps in hand for the routine it is about to solve.
//!
//! Recovered from `hydra_travel::walk::kept::{taken_from,with_items,language_in,
//! put_back,is_spoken}` (`0x101526e50`, `0x101527090`, `0x101527878`,
//! `0x101527c10`, `0x101527cbc`).

/// The item a walk takes out of a container first (`taken_from`,
/// `0x101526e50`).
///
/// **Partly recovered**: the body matches the recovered container words
/// `Outfitting`, `Magic Shoppe`, `General Store`, `Armory`, `Combat Gear`,
/// `Lockpicks` and `Pinefar, Depository` (`0x10241c59e`..) against the room and
/// answers with the item it keeps.
pub fn taken_from(room: &str) -> Option<&'static str> {
    const KEPT: [(&str, &str); 7] = [
        ("Outfitting", "lockpick"),
        ("Magic Shoppe", "crystal amulet"),
        ("General Store", "cheap gem"),
        ("Armory", "cheap gem"),
        ("Combat Gear", "cheap gem"),
        ("Lockpicks", "lockpick"),
        ("Pinefar, Depository", "cheap gem"),
    ];
    KEPT.iter()
        .find(|(place, _)| room.contains(place))
        .map(|(_, item)| *item)
}

/// Add items to the list a walk keeps (`with_items`, `0x101527090`).
pub fn with_items(items: &mut Vec<String>, extra: &[&str]) {
    for item in extra {
        if !items.iter().any(|held| held == item) {
            items.push((*item).to_string());
        }
    }
}

/// Which language a spoken line was in (`language_in`, `0x101527878`).
///
/// **Partly recovered**: the body matches the speech presets; `Guildspeak` is
/// the recovered one (`0x10241c59e`).
pub fn language_in(line: &str) -> Option<&'static str> {
    if line.contains("Guildspeak") {
        return Some("Guildspeak");
    }
    None
}

/// The command that puts a kept item back (`put_back`, 172 bytes at
/// `0x101527c10`).
pub fn put_back(item: &str, container: &str) -> Option<String> {
    if item.is_empty() || container.is_empty() {
        return None;
    }
    Some(format!("put {item} in {container}"))
}

/// Did this line come from speech (`is_spoken`, 1404 bytes at `0x101527cbc`)?
///
/// **Partly recovered**: the body inspects the line's runs (the same test
/// `hydra_model::state::lines::ChunkLine::is_spoken` makes) and the recovered
/// `{item:` / `detach` protocol fragments at `0x10241c575`.
pub fn is_spoken(line: &str) -> bool {
    line.contains("You are currently speaking") || line.trim_start().starts_with('"')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depots_hand_out_gems() {
        assert_eq!(taken_from("Pinefar, Depository"), Some("cheap gem"));
        assert_eq!(taken_from("Lockpicks"), Some("lockpick"));
        assert_eq!(taken_from("Nowhere"), None);
    }

    #[test]
    fn items_are_unique() {
        let mut items = vec!["cheap gem".to_string()];
        with_items(&mut items, &["cheap gem", "lockpick"]);
        assert_eq!(items, vec!["cheap gem", "lockpick"]);
    }

    #[test]
    fn language_and_speech() {
        assert_eq!(
            language_in("You are currently speaking Guildspeak."),
            Some("Guildspeak")
        );
        assert!(is_spoken("\"hello\""));
        assert!(!is_spoken("The gate opens."));
        assert_eq!(
            put_back("cheap gem", "sack").as_deref(),
            Some("put cheap gem in sack")
        );
    }
}
