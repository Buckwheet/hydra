//! Maneuver (cman) readiness.
//!
//! `ready_line` (812 B) parses the "you are ready to use X again" line and
//! `cooldown_refusal` (384 B) the `": "`-separated refusal.

/// A maneuver becoming ready.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    pub name: String,
}

/// @0x0000000101b5ba10 (812 B)
pub fn ready_line(line: &str) -> Option<Ready> {
    let line = line.trim();
    let rest = line
        .strip_prefix("You are now ready to use ")
        .or_else(|| line.strip_prefix("You are ready to use "))
        .or_else(|| line.strip_prefix("Roundtime for "))?;
    let name = rest
        .split(" again")
        .next()
        .unwrap_or(rest)
        .trim()
        .trim_end_matches('.')
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some(Ready { name })
    }
}

/// @0x0000000101b5bd3c (384 B) — `"<maneuver>: <reason>"`.
pub fn cooldown_refusal(line: &str) -> Option<(String, String)> {
    let (name, reason) = line.trim().split_once(": ")?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some((name.to_string(), reason.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_lines_parse_the_name() {
        assert_eq!(
            ready_line("You are now ready to use cheapshots again."),
            Some(Ready {
                name: "cheapshots".into()
            })
        );
        assert_eq!(ready_line("nothing"), None);
    }

    #[test]
    fn cooldown_refusal_splits_on_the_separator() {
        assert_eq!(
            cooldown_refusal("cheapshots: you must wait 3 seconds"),
            Some(("cheapshots".into(), "you must wait 3 seconds".into()))
        );
        assert_eq!(cooldown_refusal("no separator"), None);
    }
}
