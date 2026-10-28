//! The eaccess (Simutronics `LAUNCH`) login wire protocol.
//!
//! Recovered from `hydra_net::eaccess::wire` (`0x101c560f0` .. `0x101c57bb8`).
//! The original exchanged newline-terminated lines of tab-separated fields with
//! the eaccess server; the stages are named by the log lines the crate emitted
//! (`[stage: k_response]`, `[stage: a_response]`, ... which live in
//! `eaccess/handshake.rs` in the original tree).

use std::collections::BTreeMap;

/// Why a stage refused to continue.
///
/// The layout is recovered from the sret structs: a `String` detail followed by
/// the `&'static str` naming the stage, in a `Result` whose `Ok` is the niche
/// (`i64::MIN` in the string capacity slot).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub detail: String,
    pub stage: &'static str,
}

impl Refusal {
    pub fn new(stage: &'static str, detail: impl Into<String>) -> Refusal {
        Refusal {
            detail: detail.into(),
            stage,
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[stage: {}] {}", self.stage, self.detail)
    }
}

impl std::error::Error for Refusal {}

/// Build a [`Refusal`]; the original has this as the private `wire::err`
/// helper at `0x101c57918`.
pub fn err(stage: &'static str, detail: impl Into<String>) -> Refusal {
    Refusal::new(stage, detail)
}

/// Mask key material in a wire line before it is logged.
///
/// Recovered from `0x101c57a14`: the line is split on tabs and rejoined with
/// tabs; when the line is the `A` response and its third field is `KEY`, the
/// fields that carry key material are replaced.
///
/// INFERRED: the replacement text is not recoverable (the mapping closure is
/// inlined); every field from the third onwards is masked with `<key>`, which is
/// what "not printed -- key material" implies.
pub fn redact(text: &str) -> String {
    let parts: Vec<&str> = text.split('\t').collect();
    let carries_key =
        parts.len() >= 3 && parts[0].len() == 1 && parts[0].as_bytes()[0] == b'A' && parts[2] == "KEY";
    let masked: Vec<String> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            // UNKNOWN: the original's mask text; only "key material" survives.
            if carries_key && i >= 3 {
                "<key>".to_string()
            } else {
                (*part).to_string()
            }
        })
        .collect();
    masked.join("\t")
}

/// Redact a character code before it is logged.
///
/// Recovered from `0x101c575e8`: the code is split on `_`; when it has at least
/// three fields and the second is exactly `W`, the middle is replaced, giving
/// `W_<ACCOUNT>_<last>` (the format string is at `0x103e7fd02`).  Anything else
/// is returned unchanged.
pub fn redact_char_code(code: &str) -> String {
    let parts: Vec<&str> = code.split('_').collect();
    if parts.len() >= 3 && parts[1] == "W" {
        format!("W_<ACCOUNT>_{}", parts[parts.len() - 1])
    } else {
        code.to_string()
    }
}

/// The tab separated fields of a line, trimmed.
///
/// Recovered from `0x101c57584` (`character_names`): `text.trim().split('\t')`
/// collected into a vector.
pub fn character_names(text: &str) -> Vec<&str> {
    text.trim().split('\t').collect()
}

/// The game codes the server offered in its `M` response.
///
/// Recovered from `0x101c578b8`: the same tab split as [`character_names`], on
/// the `M` response line.
pub fn offered_game_codes(text: &str) -> Vec<&str> {
    text.trim().split('\t').collect()
}

/// True when a launch line is the server's "already OK" answer.
///
/// Recovered from `0x101c56c34`: `text.len() >= 4 && text[..4] == "L\tOK"` (the
/// four bytes are compared as one 32-bit word, `0x4b4f094c`).
pub fn is_launch_ok(text: &str) -> bool {
    text.len() >= 4 && text.as_bytes()[..4] == *b"L\tOK"
}

/// Check that the next line is the expected command echo.
///
/// Recovered from `0x101c56a5c`: the line must start with `<expected>\t`
/// (the format string at `0x103e7fbdb`); otherwise the read and write streams
/// have drifted apart and the error says so.
pub fn expect_echo(line: &str, expected: u8, stage: &'static str) -> Result<(), Refusal> {
    let trimmed = line.trim();
    let wanted = format!("{}\t", expected as char);
    if trimmed.starts_with(&wanted) {
        return Ok(());
    }
    Err(err(
        stage,
        format!(
            "expected a {} response, got {} -- the read and write streams are out of step, \
             and nothing parsed after this point is meaningful",
            expected as char,
            redact(trimmed)
        ),
    ))
}

/// Key the password with the server's hash key.
///
/// Recovered from `0x101c572b8`: for each password byte,
/// `out[i] = ((password[i] - 0x20) ^ key[i]) + 0x20`, which keeps the result
/// printable.  The key must be at least as long as the password — the original
/// refuses to truncate rather than send a wrong password.
pub fn hash_password(password: &[u8], key: &[u8]) -> Result<Vec<u8>, Refusal> {
    if key.len() < password.len() {
        return Err(err(
            "hash",
            format!(
                "key ({} bytes) shorter than password ({} bytes) -- refusing to truncate, \
                 which would send a wrong password rather than fail",
                key.len(),
                password.len()
            ),
        ));
    }
    let mut out = Vec::with_capacity(password.len());
    for (i, &byte) in password.iter().enumerate() {
        out.push((byte.wrapping_sub(0x20) ^ key[i]).wrapping_add(0x20));
    }
    Ok(out)
}

/// The parts of the launch payload Hydra needs.
///
/// The struct is three `&str` fields plus a `u16`, which is what the sret struct
/// in `0x101c56c5c` is: three pointer/length pairs and then a two-byte port.
/// Which three strings the original kept is INFERRED from the field names it
/// looked for — `KEY`, `GAMEHOST` and the selected game code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Launch<'a> {
    pub key: &'a str,
    pub host: &'a str,
    pub code: &'a str,
    pub port: u16,
}

/// Parse the launch payload into a [`Launch`].
///
/// Recovered from `0x101c56c5c`: the payload is split on tabs, each field is
/// split again on `=` into a name and a value, and the fields `KEY`,
/// `GAMEHOST` and `GAMEPORT` are picked out.  `GAMEPORT` is parsed as a number
/// (the original accepts a leading `+`/`-`, then rejects anything but digits).
/// The missing-field messages are the recovered literals at `0x103bfa5cd`,
/// `0x103bfa5ea` and `0x103bfa607`.
pub fn parse_launch(payload: &str) -> Result<Launch<'_>, Refusal> {
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    for part in payload.trim().split('\t') {
        if let Some((name, value)) = part.split_once('=') {
            fields.insert(name.trim(), value.trim());
        }
    }
    let key = fields
        .get("KEY")
        .copied()
        .ok_or_else(|| err("launch", "no KEY in launch payload"))?;
    let host = fields
        .get("GAMEHOST")
        .copied()
        .ok_or_else(|| err("launch", "no GAMEHOST in launch payload"))?;
    let port = fields
        .get("GAMEPORT")
        .copied()
        .ok_or_else(|| err("launch", "no GAMEPORT in launch payload"))?;
    let port: u16 = port
        .parse()
        .map_err(|_| err("launch", format!("GAMEPORT is not a port: {port}")))?;
    // UNKNOWN: which third string the original kept; GAMECODE is the only
    // remaining field the literals mention.
    let code = fields.get("GAMECODE").copied().unwrap_or_default();
    Ok(Launch {
        key,
        host,
        code,
        port,
    })
}

/// Resolve a character name to the character code that precedes it.
///
/// Recovered from `0x101c5775c`: split the line on tabs, then walk the fields in
/// threes — `parts[6]` is compared with `wanted` (ASCII case insensitively) and
/// on a match `parts[5]` is returned, then `parts[9]`/`parts[8]`, and so on.
/// Fewer than seven fields cannot contain a pair, so the answer is `None`.
pub fn resolve_char_code<'a>(line: &'a str, wanted: &str) -> Option<&'a str> {
    let parts: Vec<&str> = line.trim().split('\t').collect();
    if parts.len() < 7 {
        return None;
    }
    let mut name = 6;
    while name < parts.len() {
        if parts[name].len() == wanted.len() && parts[name].eq_ignore_ascii_case(wanted) {
            return parts.get(name - 1).copied();
        }
        name += 3;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_password_known_vector() {
        let out = hash_password(b"abc", b"\x01\x02\x03").unwrap();
        assert_eq!(
            out,
            vec![
                (b'a' - 0x20 ^ 1) + 0x20,
                (b'b' - 0x20 ^ 2) + 0x20,
                (b'c' - 0x20 ^ 3) + 0x20,
            ]
        );
        // A zero key shifts by 0x20 and shifts back: identity.
        assert_eq!(hash_password(b"hydra", &[0; 5]).unwrap(), b"hydra");
        // "password" keyed with 0x55 bytes.
        assert_eq!(
            hash_password(b"pw", &[0x55, 0x55]).unwrap(),
            vec![(b'p' - 0x20) ^ 0x55 | 0x20, (b'w' - 0x20 ^ 0x55) + 0x20]
        );
    }

    #[test]
    fn hash_password_refuses_short_key() {
        let err = hash_password(b"password", b"key").unwrap_err();
        assert_eq!(err.stage, "hash");
        assert!(
            err.detail
                .starts_with("key (3 bytes) shorter than password (8 bytes)"),
            "{}",
            err.detail
        );
    }

    #[test]
    fn hash_password_uses_the_key_whole() {
        assert_eq!(
            hash_password(b"ab", b"\x01\x02\x03\x04").unwrap(),
            hash_password(b"ab", b"\x01\x02").unwrap()
        );
    }

    #[test]
    fn launch_ok_needs_the_exact_prefix() {
        assert!(is_launch_ok("L\tOK"));
        assert!(is_launch_ok("L\tOK\tGAMEHOST=x"));
        assert!(!is_launch_ok("L\tNO"));
        assert!(!is_launch_ok("L\tO"));
        assert!(!is_launch_ok(""));
    }

    #[test]
    fn parse_launch_reads_the_three_fields() {
        let launch =
            parse_launch("KEY=ab12\tGAMEHOST=gs-play.simutronics.net\tGAMEPORT=10121\tGAMECODE=GS4")
                .unwrap();
        assert_eq!(launch.key, "ab12");
        assert_eq!(launch.host, "gs-play.simutronics.net");
        assert_eq!(launch.port, 10121);
        assert_eq!(launch.code, "GS4");
    }

    #[test]
    fn parse_launch_names_the_missing_field() {
        assert_eq!(
            parse_launch("KEY=x").unwrap_err().detail,
            "no GAMEHOST in launch payload"
        );
        assert_eq!(
            parse_launch("KEY=x\tGAMEHOST=h").unwrap_err().detail,
            "no GAMEPORT in launch payload"
        );
        assert_eq!(
            parse_launch("GAMEHOST=h\tGAMEPORT=1").unwrap_err().detail,
            "no KEY in launch payload"
        );
    }

    #[test]
    fn parse_launch_rejects_a_bad_port() {
        let err = parse_launch("KEY=x\tGAMEHOST=h\tGAMEPORT=nope").unwrap_err();
        assert!(err.detail.contains("not a port"), "{}", err.detail);
    }

    #[test]
    fn expect_echo_matches_the_command_letter() {
        assert!(expect_echo("A\tOK\n", b'A', "a_response").is_ok());
        // A lettered reply; the `K` reply is the exception — it is the bare key
        // with no leading letter at all (`handshake::sge_key`), so a command
        // letter check never applies to it.
        assert!(expect_echo("F\tGS3", b'F', "f_response").is_ok());
        let err = expect_echo("M\t1", b'A', "a_response").unwrap_err();
        assert_eq!(err.stage, "a_response");
        assert!(
            err.detail.starts_with("expected a A response, got M\t1 --"),
            "{}",
            err.detail
        );
        assert!(err
            .detail
            .ends_with("nothing parsed after this point is meaningful"));
    }

    #[test]
    fn expect_echo_will_not_accept_a_prefix() {
        assert!(expect_echo("AB\t", b'A', "a_response").is_err());
        assert!(expect_echo("A", b'A', "a_response").is_err());
    }

    #[test]
    fn redact_masks_key_material() {
        assert_eq!(redact("K\tabcdef"), "K\tabcdef");
        assert_eq!(redact("A\tacct\tKEY\tsecret"), "A\tacct\tKEY\t<key>");
        assert_eq!(redact("A\tacct\tOTHER\tvalue"), "A\tacct\tOTHER\tvalue");
    }

    #[test]
    fn redact_char_code_keeps_only_the_last_field() {
        assert_eq!(redact_char_code("1_W_DRIDARN"), "W_<ACCOUNT>_DRIDARN");
        assert_eq!(redact_char_code("acct_W_1234"), "W_<ACCOUNT>_1234");
        assert_eq!(redact_char_code("W_ACCOUNT_1234"), "W_ACCOUNT_1234");
        assert_eq!(redact_char_code("GS4"), "GS4");
        assert_eq!(redact_char_code("a_b_c"), "a_b_c");
        assert_eq!(redact_char_code(""), "");
    }

    #[test]
    fn resolve_char_code_finds_the_code_before_the_name() {
        let line = "M\t1\t2\t3\t4\tGS4_1\tDridarn\t7\tGS4_2\tAelith";
        assert_eq!(resolve_char_code(line, "Dridarn"), Some("GS4_1"));
        assert_eq!(resolve_char_code(line, "dridarn"), Some("GS4_1"));
        assert_eq!(resolve_char_code(line, "Aelith"), Some("GS4_2"));
        assert_eq!(resolve_char_code(line, "Nobody"), None);
        assert_eq!(resolve_char_code("M\t1", "Dridarn"), None);
    }

    #[test]
    fn names_and_codes_split_on_tabs() {
        assert_eq!(character_names("  a\tb\tc\n"), vec!["a", "b", "c"]);
        assert_eq!(offered_game_codes("GS4\tGS3"), vec!["GS4", "GS3"]);
    }
}
