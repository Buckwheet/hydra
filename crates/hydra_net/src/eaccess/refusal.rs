//! Why a launch was refused, and whether retrying could ever help.
//!
//! Recovered from `hydra_net::eaccess::refusal` (`0x101c546e4` .. `0x101c549d0`).
//! The server refuses a launch with a `L\tPROBLEM<tab>N` line, where `N` is a
//! one-digit sub-code.  Codes 1..=4 are known — the four messages below are the
//! exact literals from the binary (`__const` at `0x103bfa1e0`, `0x103bfa145`,
//! `0x103bf9f61`, `0x103bf9f16`).

use crate::eaccess::wire::redact;

/// The message for each known `PROBLEM` sub-code, indexed from code 1.
pub const PROBLEM_CODES: [&str; 4] = [
    "the account's access level does not permit playing this instance -- a lapsed or \
     missing subscription -- or the account service timed out. DO NOT RETRY: this is \
     account state, and the fix is a subscription, not another login.",
    "the server has no STORM launch entry for this game's configuration. SERVER-SIDE, not \
     an account problem. DO NOT RETRY: it will not change between attempts.",
    "the server has no configuration for the selected game. SERVER-SIDE. DO NOT RETRY. \
     Check first that the game code was listed by M, and that G selected the instance you \
     meant -- a rejected G leaves the session pointed at an instance the server cannot \
     launch, which produces exactly this, and NOTHING IN THIS CLIENT VERIFIES IT: the \
     handshake checks each response's command letter, not the code it carries. \
     Byte-diffing the L request is a dead end: its bytes are correct in this failure.",
    "the account service failed while assigning the character. RETRY: transient.",
];

/// What the original says when the sub-code is not one it knows.
pub const UNKNOWN_CODE: &str =
    "unrecognised PROBLEM sub-code. Codes 1-4 are known; a fifth means the server grew one.";

/// The `PROBLEM` prefix every refusal line starts with.
const PROBLEM: &str = "L\tPROBLEM";

/// The sub-code carried by a refusal line, if the line is one and carries a
/// one-digit code the server knows.
///
/// Recovered from the shared prologue of `describe_launch_refusal`
/// (`0x101c546e4`) and `launch_refusal_is_fatal` (`0x101c548c8`): trim the line,
/// require the `L\tPROBLEM` prefix, then take the field after the next tab and
/// trim it — it must be exactly one byte long and must lie in `1..=4`.
fn problem_code(text: &str) -> Option<usize> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix(PROBLEM)?;
    let field = rest
        .trim_start_matches('\t')
        .split('\t')
        .next()
        .unwrap_or("")
        .trim();
    if field.len() != 1 {
        return None;
    }
    let code = field.as_bytes()[0].wrapping_sub(b'1') as usize;
    if code < PROBLEM_CODES.len() {
        Some(code)
    } else {
        None
    }
}

/// A human description of a launch refusal line.
///
/// Recovered from `0x101c546e4`: the line is redacted and either
/// `launch refused (<message>): <line>` for a known sub-code or
/// `launch refused: <line>` otherwise.
pub fn describe_launch_refusal(text: &str) -> String {
    let shown = redact(text.trim());
    match problem_code(text) {
        Some(code) => format!("launch refused ({}): {}", PROBLEM_CODES[code], shown),
        None => format!("launch refused: {}", shown),
    }
}

/// True when a launch refusal is worth reporting as fatal rather than transient.
///
/// Recovered from `0x101c548c8`: `true` exactly when the line is a
/// `L\tPROBLEM` line with a known sub-code.
pub fn launch_refusal_is_fatal(text: &str) -> bool {
    problem_code(text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes_are_described() {
        let text = "L\tPROBLEM\t1";
        let described = describe_launch_refusal(text);
        assert!(
            described.starts_with("launch refused (the account's access level"),
            "{described}"
        );
        assert!(described.ends_with(&format!("): {}", text)), "{described}");
        assert!(launch_refusal_is_fatal(text));
    }

    #[test]
    fn every_known_code_maps_to_its_message() {
        for (i, message) in PROBLEM_CODES.iter().enumerate() {
            let text = format!("L\tPROBLEM {} \n", i + 1);
            assert_eq!(
                describe_launch_refusal(&text),
                format!("launch refused ({}): {}", message, redact(text.trim()))
            );
            assert!(launch_refusal_is_fatal(&text));
        }
    }

    #[test]
    fn unknown_codes_are_not_fatal() {
        let text = "L\tPROBLEM\t9";
        assert_eq!(
            describe_launch_refusal(text),
            format!("launch refused: {}", text)
        );
        assert!(!launch_refusal_is_fatal(text));
    }

    #[test]
    fn other_lines_are_not_refusals() {
        assert!(!launch_refusal_is_fatal("L\tOK"));
        assert!(!launch_refusal_is_fatal("something else"));
        assert!(!launch_refusal_is_fatal(""));
        assert_eq!(
            describe_launch_refusal("L\tOK"),
            "launch refused: L\tOK"
        );
    }

    #[test]
    fn the_unknown_code_note_is_kept() {
        assert!(UNKNOWN_CODE.starts_with("unrecognised PROBLEM sub-code."));
        assert!(PROBLEM_CODES[3].ends_with("RETRY: transient."));
    }
}
