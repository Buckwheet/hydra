//! Channel lines (`[Merchant]`, `[General]`, …).
//!
//! Recovered table at 0x103995f04: display name then the theme token, for the
//! six chat channels plus the two vitals channels:
//!
//! ```text
//! Merchant channel_merchant  General channel_general  Help channel_help
//! Realm channel_realm        OOC channel_ooc          Society channel_society
//! House channel_house        Health health            Stamina stamina
//! Spirit spirit
//! ```

use crate::theme::Token;

/// `(display name, token)` for every channel prefix.
pub const CHANNELS: [(&str, Token); 9] = [
    ("Merchant", Token::ChannelMerchant),
    ("General", Token::ChannelGeneral),
    ("Help", Token::ChannelHelp),
    ("Realm", Token::ChannelRealm),
    ("OOC", Token::ChannelOoc),
    ("Society", Token::ChannelSociety),
    ("House", Token::ChannelHouse),
    ("Health", Token::Health),
    ("Stamina", Token::Stamina),
];

/// `@0x1019380a8` — the token a `[Name]` prefix paints with.
pub fn token(name: &str) -> Option<Token> {
    CHANNELS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, t)| *t)
}

/// `@0x101937ecc` — split a channel line into `(token, body)`.
///
/// The binary matches the leading `[Name]` and returns the rest of the line.
pub fn mark(line: &str) -> Option<(Token, &str)> {
    let rest = line.trim_start();
    let close = rest.strip_prefix('[')?.find(']')? + 1;
    let name = &rest[1..close];
    let token = token(name)?;
    Some((token, rest[close + 1..].trim_start()))
}

/// `@0x101938298` — the preset a channel body should be styled with.
pub fn preset(name: &str) -> Option<&'static str> {
    token(name).map(|t| t.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recovered_channel_table() {
        assert_eq!(CHANNELS.len(), 9);
        assert_eq!(token("Merchant"), Some(Token::ChannelMerchant));
        assert_eq!(token("general"), Some(Token::ChannelGeneral));
        assert_eq!(token("Help"), Some(Token::ChannelHelp));
        assert_eq!(token("Realm"), Some(Token::ChannelRealm));
        assert_eq!(token("OOC"), Some(Token::ChannelOoc));
        assert_eq!(token("Society"), Some(Token::ChannelSociety));
        assert_eq!(token("House"), Some(Token::ChannelHouse));
        assert_eq!(token("Health"), Some(Token::Health));
        assert_eq!(token("Stamina"), Some(Token::Stamina));
        assert_eq!(token("Spirit"), None);
    }

    #[test]
    fn marking_a_channel_line() {
        let (t, body) = mark("[General] hello there").unwrap();
        assert_eq!(t, Token::ChannelGeneral);
        assert_eq!(body, "hello there");
        assert_eq!(mark("no prefix here"), None);
        assert_eq!(mark("[Nope] x"), None);
        assert_eq!(mark("[General"), None);
    }

    #[test]
    fn presets_are_token_names() {
        assert_eq!(preset("OOC"), Some("channel_ooc"));
        assert_eq!(preset("nope"), None);
    }
}
