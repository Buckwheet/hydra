//! Target/link helpers shared by every combat classifier.
//!
//! Recovered:
//! * `link_name` (884 B) extracts the `noun`/`exist` name out of a link run,
//! * `link_in` (1008 B) returns the target of a link inside a line,
//! * `bolded_link` (424 B) returns the bolded (i.e. highlighted) link,
//! * `any_link` (312 B) returns the first link at all,
//! * `is_self` (888 B) tests whether a link addresses the player
//!   (`pattern_addresses_self`, 148 B, is its pattern-text side).
//!
//! Every entry point receives styled runs in the original; the rebuild works on
//! plain text and therefore recognises the `exist`/`noun` attributes the
//! protocol layer leaves behind only when they are literal.
//!
//! `// UNKNOWN:` the real input is a `Run`/`Link` pair
//! (`hydra_protocol::frame::payload::Link`); these helpers take the flattened
//! link text and the player name instead.

use hydra_protocol::frame::payload::Link;

/// @0x0000000101bff3fc (1008 B) — the link that a line targets.
///
/// A link is identified by its object `noun`/`exist` id; the display text
/// lives on the run, not the link (`hydra_protocol::frame::payload::Link`).
pub fn link_in<'a>(links: &'a [Link], name: &str) -> Option<&'a Link> {
    links.iter().find(|link| link_name(link) == name)
}

/// @0x0000000101bff? (884 B) — the identifying name of a link.
pub fn link_name(link: &Link) -> String {
    link_text(link)
}

/// @0x0000000101bfe? (424 B) — the first bold link.
///
/// `// UNKNOWN:` boldness lives on the run, not the link; the rebuild treats
/// the first link as the bold one (which is what `Runs::bold_fragments` yields
/// for the recovered patterns).
pub fn bolded_link<'a>(links: &'a [Link]) -> Option<&'a Link> {
    links.first()
}

/// @0x0000000101be? (312 B) — the first link of any kind.
pub fn any_link<'a>(links: &'a [Link]) -> Option<&'a Link> {
    links.first()
}

/// @0x0000000101be? (888 B) — does a link address `player`?
pub fn is_self(links: &[Link], player: &str) -> bool {
    links.iter().any(|link| link_text(link) == player)
}

/// @0x0000000101be? (148 B) — the fragment of a pattern that self-addresses a
/// link.
///
/// `// UNKNOWN:` the pattern text; the recovered function returns a borrowed
/// slice of the incoming pattern.
pub fn pattern_addresses_self(pattern: &str) -> bool {
    pattern.contains("exist")
}

fn link_text(link: &Link) -> String {
    use hydra_protocol::frame::payload::LinkTarget;
    match &link.target {
        LinkTarget::Object { noun, exist } => noun.clone().unwrap_or_else(|| exist.clone()),
        LinkTarget::Command { cmd } => cmd.clone(),
        LinkTarget::Href { href } => href.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra_protocol::frame::payload::LinkTarget;

    fn link(noun: &str, exist: &str) -> Link {
        Link::object(exist, Some(noun.to_string()))
    }

    #[test]
    fn link_name_is_the_noun() {
        assert_eq!(link_name(&link("kobold", "123")), "kobold");
        assert_eq!(link_name(&Link::command("look")), "look");
    }

    #[test]
    fn link_in_finds_the_named_link() {
        let links = vec![link("kobold", "1"), link("goblin", "2")];
        assert_eq!(link_in(&links, "goblin").map(link_name), Some("goblin".to_string()));
        assert!(link_in(&links, "troll").is_none());
        assert_eq!(bolded_link(&links).map(link_name), Some("kobold".to_string()));
        assert_eq!(any_link(&links).map(link_name), Some("kobold".to_string()));
    }

    #[test]
    fn self_detection_uses_the_player_name() {
        let links = vec![link("kithar", "9")];
        assert!(is_self(&links, "kithar"));
        assert!(!is_self(&links, "ael"));
    }
}
