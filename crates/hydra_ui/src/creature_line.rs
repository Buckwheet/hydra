//! Creature lines: the adjectives and nouns the game prepends to a creature.
//!
//! The binary carries a long table of `adjective + noun` / `noun` pairs
//! (0x1039929c4 .. 0x103992f3c), e.g.
//!
//! ```text
//! athletic dark-eyed incubus        dark-eyed incubus
//! behemothic gorefrost golem        gorefrost golem
//! blackened decaying tumbleweed     decaying tumbleweed
//! brawny gigas shield-maiden        gigas shield-maiden
//! colossal boreal undansormr        boreal undansormr
//! darkly inked fetish master        inked fetish master
//! ethereal triton psionicist        triton psionicist
//! heavily armored battle mastodon   battle mastodon
//! immense gold-bristled hinterboar  immense hinterboar
//! patchwork flesh monstrosity       flesh monstrosity
//! phantasmal bestial swordsman      bestial swordsman
//! roiling crimson angargeist        crimson angargeist
//! rotting Citadel arbalester        Citadel arbalester
//! savage fork-tongued wendigo       savage wendigo
//! seething pestilent vision         pestilent vision
//! spectral triton protector         triton protector
//! squamous reptilian mutant         reptilian mutant
//! stunted halfling bloodspeaker     stunted bloodspeaker
//! withered shadow-cloaked draugr    withered draugr
//! writhing frost-glazed vine        frost-glazed vine
//! ```

/// The `(with adjective, bare)` pairs read out of the binary.
pub const CREATURES: [(&str, &str); 21] = [
    ("athletic dark-eyed incubus", "dark-eyed incubus"),
    ("behemothic gorefrost golem", "gorefrost golem"),
    ("blackened decaying tumbleweed", "decaying tumbleweed"),
    ("brawny gigas shield-maiden", "gigas shield-maiden"),
    ("cinereous chthonian sybil", "chthonian sybil"),
    ("colossal boreal undansormr", "boreal undansormr"),
    ("darkly inked fetish master", "inked fetish master"),
    ("decaying Citadel guardsman", "Citadel guardsman"),
    ("ethereal triton psionicist", "triton psionicist"),
    ("heavily armored battle mastodon", "battle mastodon"),
    ("immense gold-bristled hinterboar", "immense hinterboar"),
    ("patchwork flesh monstrosity", "flesh monstrosity"),
    ("phantasmal bestial swordsman", "bestial swordsman"),
    ("roiling crimson angargeist", "crimson angargeist"),
    ("rotting Citadel arbalester", "Citadel arbalester"),
    ("savage fork-tongued wendigo", "savage wendigo"),
    ("seething pestilent vision", "pestilent vision"),
    ("spectral triton protector", "triton protector"),
    ("squamous reptilian mutant", "reptilian mutant"),
    ("withered shadow-cloaked draugr", "withered draugr"),
    ("writhing frost-glazed vine", "frost-glazed vine"),
];

/// `@0x10195c784` — the bare creature name a line names, if it is one of the
/// table's.
pub fn mark(line: &str) -> Option<&'static str> {
    CREATURES
        .iter()
        .find(|(full, bare)| line.contains(full) || line.contains(bare))
        .map(|(_, bare)| *bare)
}

/// `@0x10195c8c8` — the preset a creature line is styled with.
pub fn preset(noun: &str) -> &'static str {
    let _ = noun;
    "creature"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_find_the_bare_name() {
        assert_eq!(
            mark("An athletic dark-eyed incubus glares at you."),
            Some("dark-eyed incubus")
        );
        assert_eq!(mark("The savage wendigo attacks!"), Some("savage wendigo"));
        assert_eq!(mark("A dark-eyed incubus waits."), Some("dark-eyed incubus"));
        assert_eq!(mark("a small rabbit"), None);
    }

    #[test]
    fn the_table_is_the_recovered_one() {
        assert_eq!(CREATURES.len(), 21);
        assert_eq!(CREATURES[0].1, "dark-eyed incubus");
        assert_eq!(CREATURES[20].1, "frost-glazed vine");
    }

    #[test]
    fn presets_are_the_creature_token() {
        assert_eq!(preset("golem"), "creature");
    }
}
