//! `<dir value="…"/>` extraction.
//!
//! Recovered from `crates/hydra-protocol/src/parser/compass.rs`:
//! `directions` (568 B, `0x101c39e38`).  The panic sites in that function
//! name the file, `simplify`/`missing` were not part of this crate's symbol
//! set, and the four `phy_left`/`mnt_left`/`phy_conv`/`mnt_conv` literals that
//! share its pool are only ever *loaded* by `thin_frame`, so `directions` is
//! the only function that belongs here.

use crate::text;

/// The literal the scan looks for.  Five bytes at `0x103bf91af`.
const NEEDLE: &str = "<dir value";

/// The attribute name handed to [`text::attribute`].  Five bytes at
/// `0x103bf91b4`.
const ATTRIBUTE: &str = "value";

/// Collect the `value` attribute of every `<dir value="…">` in `text`.
///
/// Established control flow (`0x101c39e38`):
///
/// ```text
/// let mut out = Vec::new();
/// let mut rest = text;
/// while let Some(i) = rest.find("<dir value") {
///     rest = &rest[i..];
///     let Some(j) = rest.find('>') else { break };
///     let tag = &rest[..j + 1];
///     if let Some(v) = text::attribute(tag, "value") { out.push(v) }
///     rest = &rest[j + 1..];
/// }
/// out
/// ```
///
/// Three details are worth pinning because they are easy to get wrong:
///
/// * the needle includes the space after `dir`, so `<dirvalue="n"/>` is not
///   a direction and neither is `<dir/>`;
/// * the `'>'` search is a `CharSearcher` (`core::str::pattern::
///   CharSearcher::next_match`) over the *remaining* text, and an unterminated
///   `<dir value` ends the scan instead of being skipped;
/// * an unterminated tag leaves the already-collected values in place — the
///   loop breaks, it does not return `None`.
pub fn directions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find(NEEDLE) {
        rest = &rest[i..];
        let Some(gt) = rest.find('>') else { break };
        let end = gt + '>'.len_utf8();
        if let Some(value) = text::attribute(&rest[..end], ATTRIBUTE) {
            out.push(value);
        }
        rest = &rest[end..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_every_direction_in_order() {
        let text = "<compass>\
                    <dir value=\"north\"/>\
                    <dir value=\"southeast\"/>\
                    <dir value=\"up\"/>\
                    </compass>";
        assert_eq!(directions(text), ["north", "southeast", "up"]);
    }

    #[test]
    fn ignores_tags_without_a_value_attribute() {
        let text = "<dir/><dir value/><dir/>";
        assert!(directions(text).is_empty());
    }

    #[test]
    fn the_needle_requires_exactly_one_space_after_dir() {
        // The needle is the five bytes `"<dir value"`, so neither a missing
        // space nor a second one matches and the tag is not a direction.
        assert!(directions("<dirvalue=\"north\"/>").is_empty());
        assert!(directions("<dir  value=\"north\"/>").is_empty());
        assert!(directions("<dir\tvalue=\"north\"/>").is_empty());
        assert_eq!(directions("<dir value=\"north\"/>"), ["north"]);
    }

    #[test]
    fn an_unterminated_tag_keeps_what_was_already_collected() {
        let text = "<dir value=\"north\"/><dir value=\"south\"";
        assert_eq!(directions(text), ["north"]);
    }

    #[test]
    fn text_without_a_direction_is_empty() {
        assert!(directions("").is_empty());
        assert!(directions("You are standing in a room.").is_empty());
    }

    #[test]
    fn values_are_raw_but_can_be_decoded_by_the_caller() {
        // `text::attribute` does not decode entities; a caller that renders
        // the value has to run it through `text::entities`.
        assert_eq!(
            directions("<dir value=\"a&amp;b\"/>"),
            ["a&amp;b"],
        );
    }
}
