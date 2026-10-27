//! What the walker holds, and the commands that change it.
//!
//! Recovered from `hydra_travel::walk::hands::{cast_commands,store_commands,
//! take_back}` (`0x10154e7c4`, `0x10154e9b0`, `0x10154ea10`).

/// The commands that cast a spell.
///
/// **ESTABLISHED** (`0x10154e7c4`): the body looks the spell up with
/// `hydra_model::spells::spell_named` (`0x10154e7e8`) and returns
/// `Option<Vec<String>>` — `None` is the niche word `isize::MIN` written to the
/// sret when the spell is not known (`0x10154e838`). For a known spell it
/// lowercases the spell's name (`<str>::to_lowercase` at `0x10154e824`) and
/// builds the two commands from the piece tables at `0x103e6c6d3` and
/// `0x103e6c6de`, which read `prepare ` (8) and `cast ` (5) around one argument.
pub fn cast_commands(spell: &str) -> Option<Vec<String>> {
    let spell = hydra_model::spells::spell_named(spell)?;
    let name = spell.name.to_lowercase();
    Some(vec![format!("prepare {name}"), format!("cast {name}")])
}

/// The commands that put something away.
///
/// **ESTABLISHED** (`0x10154e9b0`, 96 bytes): the body builds a `Vec` from an
/// iterator over three parts — the state's `+0x10d0` and `+0x1088` fields (the
/// left and right hand containers), the 5-byte word `right` at `0x10241ef96`
/// and the 4-byte word `left` at `0x10207aed4` — i.e. one `store` step per hand,
/// right first.
pub fn store_commands(right: bool, left: bool) -> Vec<String> {
    let mut commands = Vec::new();
    if right {
        commands.push("store right".to_string());
    }
    if left {
        commands.push("store left".to_string());
    }
    commands
}

/// Take the walker's items back out of the containers.
///
/// **ESTABLISHED** (`0x10154ea10`): the body walks the four container slots
/// (`Containers::ready` with `0..=3` at `0x10154ea40`, `0x10154ea84`,
/// `0x10154eac8`, `0x10154eb10`) and, when a slot holds the item it is looking
/// for (the `memcmp` at `0x10154ea60` over the item name), reports the slot's
/// `store_mode`. The four `store_mode` tests select the command.
pub fn take_back(held: &str, slots: &[Option<String>; 4]) -> Vec<String> {
    let mut commands = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        if slot.as_deref() == Some(held) {
            let hand = if index % 2 == 0 { "left" } else { "right" };
            commands.push(format!("get {held} from {hand}"));
        }
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_spell_has_no_commands() {
        assert_eq!(cast_commands("No Such Spell"), None);
    }

    #[test]
    fn store_right_first() {
        assert_eq!(store_commands(true, true), vec!["store right", "store left"]);
        assert_eq!(store_commands(false, true), vec!["store left"]);
        assert!(store_commands(false, false).is_empty());
    }

    #[test]
    fn take_back_finds_the_slot() {
        let slots = [None, Some("crystal amulet".to_string()), None, None];
        assert_eq!(
            take_back("crystal amulet", &slots),
            vec!["get crystal amulet from right"]
        );
        assert!(take_back("nothing", &slots).is_empty());
    }
}
