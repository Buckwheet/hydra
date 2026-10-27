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
/// **Partly recovered** (`0x10154e9b0`, 96 bytes): the body takes `&GameState`
/// in `x0`, builds the `Vec<String>` through `sret` (untouched `x8`), reads the
/// state at `+0x10d0`/`+0x1088`, and joins them with the 5-byte `right`
/// (`0x10241ef96`) and 4-byte `left` (`0x10207aed4`) words through
/// `SpecFromIterNested::from_iter`.
///
/// // UNKNOWN: the exact iterator (which of `store`/`get`/`remove` from the
/// merged pool applies to each hand) and the container predicate are not
/// recoverable. `hydra_model`'s `Containers::ready` also has a different
/// signature than the binary's `ready(slot)`, so this build keeps the simplified
/// `(right, left)` seam and emits one recovered `store <hand>` per non-empty
/// hand.
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
/// **Partly recovered** (`0x10154ea10`): the real call shape is
/// `fn(&GameState, &str) -> Vec<String>` — it reads `x1`'s `(ptr, len)` item,
/// calls `Containers::ready(state + 0xc00, slot)` for slots `0..=3`
/// (`0x10154ea40`, `0x10154ea84`, `0x10154eac8`, `0x10154eb10`), compares the
/// container text (`memcmp` at `0x10154ea60`) and then tests
/// `Containers::store_mode(slot)`.
///
/// // UNKNOWN: the reconstructed `hydra_model::state::containers::Containers`
/// has a `ready(line: &str) -> bool` reader rather than the binary's
/// `ready(slot) -> Option<&str>`, so the recovered four-slot walk cannot be
/// expressed against it. This seam takes the four slot names and returns the
/// recovered `get <item> from <hand>` shape for the matching slot.
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
