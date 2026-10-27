//! The claimant — who this session says it is, and which command symbol it uses.
//!
//! Recovered from `claimant::{usable, Desk::*, Settings::*}`:
//! `0x101a807ec`, `0x101a803bc`, `0x101a8049c`, `0x101a804a8`, `0x101a8066c`,
//! `0x101a80818`, `0x101a80890`.
//!
//! **ESTABLISHED** `Desk` layout (from `Desk::new`, `set_lich_symbol`,
//! `with_words`, and the clone in `command::SessionHandle::symbol`):
//!
//! ```text
//! +0x00  Arc<ArcInner{ strong, weak, char }>   -- the command symbol
//! +0x08  Arc<ArcInner{ strong, weak, char }>   -- Lich's command symbol
//! +0x10  ptr  \  the character name (a borrowed str: `new` stores the
//! +0x18  len  /  incoming (ptr,len) pair without cloning)
//! +0x20  Arc<ArcInner{ Vec<String> }>          -- the words
//! ```
//!
//! * `Desk::new(symbol, character)`: the symbol is an `Option<char>` (the
//!   4-byte niche value `0x110000` marks `None`, exactly `Option<char>`), and
//!   `None` becomes `'.'` (`csel w8, #0x2e, …`).  The **Lich** symbol starts as
//!   `';'` (`mov w8, #0x3b`) — Lich's own default command symbol.
//! * `Desk::claim(&self, line)` (`0x101a8066c`) trims the desk's symbol off the
//!   front of `line` (`str::trim_start_matches`) and returns the rest, or
//!   `None` when the line does not start with the symbol (the
//!   `subs x23, len, sym_len` / `b.lo` guard).
//! * `Desk::set_lich_symbol(&self, c)` writes straight through the `Arc`
//!   (`str w1,[[x0+8]+0x10]`), so it takes `&self` and still mutates.
//! * `Desk::with_words(self, words)` replaces the `Arc<Vec<String>>` at
//!   `+0x20`, dropping the old one, and hands back the desk.
//! * `usable(c)` (`0x101a807ec`) is the four-range SIMD check at
//!   `0x102268000`: `c - 33 < 15`, `c - 58 < 7`, `c - 91 < 6`, `c - 123 < 4`
//!   — that is exactly `char::is_ascii_punctuation`.
//!
//! Our reconstruction keeps the same shape but replaces the bare `Arc<Cell<…>>`
//! with `Arc<Mutex<…>>`, so the door can be shared across threads without
//! `unsafe`.

use std::sync::{Arc, Mutex};

/// The symbol a session uses when nothing else is configured.
pub const DEFAULT_SYMBOL: char = '.';
/// Lich's own command symbol, the default for [`Desk::lich_symbol`]
/// (**ESTABLISHED**: `mov w8, #0x3b` in `Desk::new`).
pub const LICH_SYMBOL: char = ';';

/// Is `c` a reasonable command symbol?  **ESTABLISHED**: `usable` is
/// `char::is_ascii_punctuation`.
pub fn usable(c: char) -> bool {
    c.is_ascii_punctuation()
}

/// The mutable half of a desk, behind one lock so a clone shares it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DeskShared {
    symbol: char,
    lich_symbol: char,
}

/// What the session claims to be: a character name, a command symbol, Lich's
/// symbol, and a word list for command parsing.
#[derive(Debug, Clone)]
pub struct Desk {
    shared: Arc<Mutex<DeskShared>>,
    character: String,
    words: Arc<Vec<String>>,
}

impl Desk {
    /// A desk for `character` whose command symbol is `symbol` (default `'.'`)
    /// and whose Lich symbol is `';'`.
    pub fn new(symbol: Option<char>, character: &str) -> Self {
        Desk {
            shared: Arc::new(Mutex::new(DeskShared {
                symbol: symbol.unwrap_or(DEFAULT_SYMBOL),
                lich_symbol: LICH_SYMBOL,
            })),
            character: character.to_string(),
            words: Arc::new(Vec::new()),
        }
    }

    /// The same desk with a new word list.
    pub fn with_words(self, words: Vec<String>) -> Self {
        Desk {
            words: Arc::new(words),
            ..self
        }
    }

    /// The character name.
    pub fn character(&self) -> &str {
        &self.character
    }

    /// The words this desk was built with.
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// The command symbol.
    pub fn symbol(&self) -> char {
        self.lock().symbol
    }

    /// Set the command symbol.  (The binary only exposes `set_lich_symbol`; a
    /// session's own symbol is set once at construction, but keeping a setter
    /// makes the type self-consistent.)
    pub fn set_symbol(&self, c: char) {
        self.lock().symbol = c;
    }

    /// Lich's command symbol.
    pub fn lich_symbol(&self) -> char {
        self.lock().lich_symbol
    }

    /// Set Lich's command symbol.  **ESTABLISHED**: takes `&self` and writes
    /// through the shared `Arc`.
    pub fn set_lich_symbol(&self, c: char) {
        self.lock().lich_symbol = c;
    }

    /// Strip one leading command symbol off `line`.
    ///
    /// **ESTABLISHED** (`0x101a8066c`): `str::trim_start_matches(self.symbol)`
    /// and `None` when the result is not shorter than the input.
    pub fn claim<'a>(&self, line: &'a str) -> Option<&'a str> {
        let sym = self.symbol();
        let mut chars = line.char_indices();
        let mut seen = false;
        let mut rest = line;
        while let Some((i, c)) = chars.next() {
            if c == sym {
                seen = true;
                let next = i + c.len_utf8();
                rest = &line[next..];
            } else {
                break;
            }
        }
        if seen { Some(rest) } else { None }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DeskShared> {
        self.shared.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for Desk {
    fn default() -> Self {
        Desk::new(None, "")
    }
}

impl PartialEq for Desk {
    fn eq(&self, other: &Self) -> bool {
        self.symbol() == other.symbol()
            && self.lich_symbol() == other.lich_symbol()
            && self.character == other.character
            && *self.words == *other.words
    }
}

/// The desk as the settings record it: `symbol` then `lich_symbol`.
///
/// **ESTABLISHED** shape: `Settings::symbol` (`0x101a80890`) and
/// `Settings::lich_symbol` (`0x101a80818`) both clone the `Settings` value
/// (five `Arc` refcount bumps at `+0x20`..`+0x48` of the object behind
/// `SessionHandle+0x18`), check whether the desk is present (`cmp x21, #1`)
/// and then read the char at `+0x10` of one of the two `Arc`s.  A missing desk
/// comes back as the `Option<char>` niche (`0x110000`), which
/// `LichDoor::symbol` maps to `'.'`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    /// the desk this session claims, if any
    pub desk: Option<Desk>,
}

impl Settings {
    /// Settings with no desk yet.
    pub fn new() -> Self {
        Settings { desk: None }
    }

    /// Settings for a desk.
    pub fn with(desk: Desk) -> Self {
        Settings { desk: Some(desk) }
    }

    /// The command symbol, if a desk is installed.
    pub fn symbol(&self) -> Option<char> {
        self.desk.as_ref().map(Desk::symbol)
    }

    /// Lich's command symbol, if a desk is installed.
    pub fn lich_symbol(&self) -> Option<char> {
        self.desk.as_ref().map(Desk::lich_symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_is_ascii_punctuation() {
        for c in "!\"#%&'()*+,-./:;<=>?@[\\]^_`{|}~".chars() {
            assert!(usable(c), "{c:?} should be usable");
        }
        for c in ['a', 'Z', '0', ' ', '\t', 'é', '×'] {
            assert!(!usable(c), "{c:?} should not be usable");
        }
        // The four recovered ranges, end to end.
        assert!(usable('!'));
        assert!(usable('/'));
        assert!(!usable('0'));
        assert!(usable(':'));
        assert!(usable('@'));
        assert!(!usable('A'));
        assert!(usable('['));
        assert!(usable('`'));
        assert!(!usable('a'));
        assert!(usable('{'));
        assert!(usable('~'));
        assert!(!usable('\u{7f}'));
    }

    #[test]
    fn a_new_desk_defaults_to_dot_and_semicolon() {
        let d = Desk::new(None, "Aelith");
        assert_eq!(d.symbol(), '.');
        assert_eq!(d.lich_symbol(), ';');
        assert_eq!(d.character(), "Aelith");
        let e = Desk::new(Some(']'), "Aelith");
        assert_eq!(e.symbol(), ']');
    }

    #[test]
    fn clones_share_the_symbols() {
        let d = Desk::new(None, "Aelith");
        let c = d.clone();
        d.set_lich_symbol(',');
        assert_eq!(c.lich_symbol(), ',');
        c.set_symbol('!');
        assert_eq!(d.symbol(), '!');
    }

    #[test]
    fn claim_strips_the_symbol() {
        let d = Desk::new(None, "Aelith");
        assert_eq!(d.claim(".go north"), Some("go north"));
        assert_eq!(d.claim("go north"), None);
        assert_eq!(d.claim("..double"), Some("double"));
        assert_eq!(d.claim("."), Some(""));
        let e = Desk::new(Some(']'), "Aelith");
        assert_eq!(e.claim("]n"), Some("n"));
        assert_eq!(e.claim(".n"), None);
    }

    #[test]
    fn with_words_replaces_the_list_and_is_returned() {
        let d = Desk::new(None, "Aelith").with_words(vec!["go".into(), "kill".into()]);
        assert_eq!(d.words(), ["go", "kill"]);
    }

    #[test]
    fn settings_reads_through_to_the_desk() {
        assert_eq!(Settings::new().symbol(), None);
        assert_eq!(Settings::new().lich_symbol(), None);
        let s = Settings::with(Desk::new(Some('*'), "Aelith"));
        assert_eq!(s.symbol(), Some('*'));
        assert_eq!(s.lich_symbol(), Some(';'));
    }
}
