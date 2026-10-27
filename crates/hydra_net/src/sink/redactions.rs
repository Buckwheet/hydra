//! Scrubbing secrets out of what the session sink writes.
//!
//! Recovered from `hydra_net::sink::redactions` (`0x101c560f0` .. `0x101c56a5c`).
//! Every password, account name and session key that goes over the wire is
//! registered here first, so the log and the `.bytes` recording never carry it.

/// The mask an account name is replaced with.
pub const ACCOUNT_MASK: &str = "<account>";
/// The mask a session key is replaced with.
pub const KEY_MASK: &str = "<session key>";

/// One registered secret and what it is replaced with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redaction {
    pub secret: Vec<u8>,
    pub mask: String,
}

/// The secrets a session has registered so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Redactions {
    entries: Vec<Redaction>,
    account: Option<String>,
}

impl Redactions {
    /// No secrets yet.
    pub fn new() -> Redactions {
        Redactions::default()
    }

    /// Register a secret to be masked.
    ///
    /// Recovered from `0x101c56518`: the secret is kept whole (not truncated) and
    /// an empty secret is not registered — masking nothing would corrupt the
    /// text for no gain.
    pub fn add(&mut self, secret: &[u8], mask: impl Into<String>) {
        if secret.is_empty() {
            return;
        }
        let mask = mask.into();
        if self
            .entries
            .iter()
            .any(|entry| entry.secret == secret && entry.mask == mask)
        {
            return;
        }
        self.entries.push(Redaction {
            secret: secret.to_vec(),
            mask,
        });
    }

    /// Register the account name.
    ///
    /// Recovered from `0x101c569a8`: the account is redacted like any other
    /// secret but is also remembered, so the log can name it back to the user.
    pub fn account(&mut self, account: &str) {
        if account.is_empty() {
            return;
        }
        self.account = Some(account.to_string());
        self.add(account.as_bytes(), ACCOUNT_MASK);
    }

    /// The account name that was registered, if any.
    pub fn account_name(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// The account name if this text carries it.
    ///
    /// Recovered from `0x101c560f0`: the text is searched for the registered
    /// account and it is returned when found (the original also redacts as it
    /// goes, which [`Redactions::apply`] does).
    pub fn account_of<'a>(&self, text: &'a str) -> Option<&'a str> {
        let account = self.account.as_deref()?;
        if text.contains(account) {
            Some(&text[text.find(account)?..text.find(account)? + account.len()])
        } else {
            None
        }
    }

    /// Mask every registered secret in `text`.
    ///
    /// Recovered from `0x101c56600`: the replacements are applied longest secret
    /// first, so a secret that contains another is not half replaced.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for entry in self.longest_first() {
            let secret = &entry.mask;
            let needle = &entry.secret;
            let needle = match std::str::from_utf8(needle) {
                Ok(needle) => needle,
                Err(_) => continue,
            };
            if !needle.is_empty() && out.contains(needle) {
                out = out.replace(needle, secret);
            }
        }
        out
    }

    /// Mask every registered secret in `bytes`, and report where they were.
    ///
    /// Recovered from `0x101c561bc`: the same walk as [`Redactions::apply`], but
    /// over the raw wire bytes, starting at `from` so a secret that straddles a
    /// buffer boundary can be caught on the second pass.
    pub fn apply_bytes_after(&self, bytes: &[u8], from: usize) -> Vec<u8> {
        let mut out = bytes.to_vec();
        for entry in self.longest_first() {
            let needle = &entry.secret;
            if needle.is_empty() || needle.len() > out.len() {
                continue;
            }
            let start = from.min(out.len());
            let mut found = Vec::new();
            let mut at = start;
            while at + needle.len() <= out.len() {
                if &out[at..at + needle.len()] == needle.as_slice() {
                    found.push(at);
                    at += needle.len();
                } else {
                    at += 1;
                }
            }
            for at in found.into_iter().rev() {
                out.splice(at..at + needle.len(), entry.mask.bytes());
            }
        }
        out
    }

    /// True when nothing has been registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many secrets are registered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The secrets, longest first, so masking is stable.
    fn longest_first(&self) -> Vec<&Redaction> {
        let mut sorted: Vec<&Redaction> = self.entries.iter().collect();
        sorted.sort_by_key(|entry| std::cmp::Reverse(entry.secret.len()));
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_account_is_masked_everywhere() {
        let mut redactions = Redactions::new();
        redactions.account("dridarn_acct");
        assert_eq!(redactions.account_name(), Some("dridarn_acct"));
        assert_eq!(
            redactions.apply("login dridarn_acct now"),
            format!("login {ACCOUNT_MASK} now")
        );
        assert_eq!(
            redactions.account_of("K\tdridarn_acct\tKEY"),
            Some("dridarn_acct")
        );
        assert_eq!(redactions.account_of("nothing here"), None);
    }

    #[test]
    fn the_session_key_is_masked_after_it_is_registered() {
        let mut redactions = Redactions::new();
        assert_eq!(redactions.apply("key=abcd"), "key=abcd");
        redactions.add(b"abcd", KEY_MASK);
        assert_eq!(redactions.apply("key=abcd"), format!("key={KEY_MASK}"));
    }

    #[test]
    fn a_longer_secret_is_masked_before_a_shorter_one_inside_it() {
        let mut redactions = Redactions::new();
        redactions.add(b"abcd", "<long>");
        redactions.add(b"bc", "<short>");
        assert_eq!(redactions.apply("abcd"), "<long>");
    }

    #[test]
    fn nothing_is_registered_from_an_empty_secret() {
        let mut redactions = Redactions::new();
        redactions.add(b"", "<nothing>");
        assert!(redactions.is_empty());
        assert_eq!(redactions.apply("unchanged"), "unchanged");
    }

    #[test]
    fn bytes_are_masked_from_an_offset() {
        let mut redactions = Redactions::new();
        redactions.add(b"secret", "<key>");
        let bytes = b"before secret and secret";
        // Starting past the first occurrence only masks the second one.
        let masked = redactions.apply_bytes_after(bytes, 10);
        assert_eq!(masked, b"before secret and <key>");
        // From the start, both are masked and nothing else moves.
        assert_eq!(
            redactions.apply_bytes_after(bytes, 0),
            b"before <key> and <key>"
        );
        // A secret that is not there changes nothing.
        assert_eq!(
            redactions.apply_bytes_after(b"clean", 0),
            b"clean".to_vec()
        );
    }
}
