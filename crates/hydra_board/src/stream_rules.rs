//! The stream rules the folder watches: which line goes to which window, and
//! which redirections fold into another.
//!
//! **ESTABLISHED** (`0x101495e68`, `0x1014968d0`, `0x101496ab0`, `0x101496c10`,
//! `0x101496c50`): a rules file is read, `said` decides which stream a line
//! belongs to, `fold_redirects` collapses the redirect chains, and the `Folding`
//! and `Watched` wrappers hold the result and reread it when the file changes.
//!
//! The rules themselves live in `hydra_session::stream_rules`; this is the board
//! side that keeps a folder's copy and hands it out.

use std::path::{Path, PathBuf};

use hydra_session::stream_rules::Rules;

/// The file the rules live in, beside the folder's settings.
pub const FILE: &str = "streams.toml";

/// Where a folder's rules live.
pub fn character_path(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// Read a folder's rules.
///
/// **ESTABLISHED** (`0x1014968d0`): a missing file is an empty rule set, not an
/// error.
pub fn load(folder: &Path) -> Result<Rules, String> {
    let path = character_path(folder);
    if !path.exists() {
        return Ok(Rules::default());
    }
    Rules::load(&path)
}

/// Which stream a line belongs to.
///
/// **ESTABLISHED** (`0x101496ab0`): the compiled rules answer, and a line no rule
/// claims is the main story.
pub fn said(rules: &Rules, line: &str) -> String {
    match rules.compile() {
        Ok(compiled) => compiled
            .stream_of(line)
            .map(str::to_string)
            .unwrap_or_else(|| "main".to_string()),
        Err(_) => "main".to_string(),
    }
}

/// Collapse the redirect chains so no stream leads in a circle.
///
/// **ESTABLISHED** (`0x101495e68`): each redirect is followed to its end, and a
/// chain that loops is cut where it started.
pub fn fold_redirects(redirects: &[(String, String)]) -> Vec<(String, String)> {
    let mut out = Vec::with_capacity(redirects.len());
    for (from, to) in redirects {
        let mut end = to.clone();
        let mut seen = vec![from.clone()];
        loop {
            if seen.contains(&end) {
                // A circle in the file: keep the redirect but stop following it.
                break;
            }
            seen.push(end.clone());
            match redirects.iter().find(|(next, _)| *next == end) {
                Some((_, next)) if !seen.contains(next) => end = next.clone(),
                _ => break,
            }
        }
        out.push((from.clone(), end));
    }
    out
}

/// Rules that fold their redirections, ready to hand out.
///
/// **ESTABLISHED** (`0x101496c10`): `fold` keeps the folded pairs beside the
/// rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Folding {
    pub rules: Rules,
    pub redirects: Vec<(String, String)>,
}

impl Folding {
    /// Fold this set's redirects.
    pub fn fold(&mut self) {
        self.redirects = fold_redirects(&self.redirects);
    }

    /// Where a line said in `stream` goes.
    pub fn of<'a>(&'a self, stream: &'a str) -> &'a str {
        self.redirects
            .iter()
            .find(|(from, _)| from == stream)
            .map(|(_, to)| to.as_str())
            .unwrap_or(stream)
    }
}

/// A folder's rules, reread when the file changes.
///
/// **ESTABLISHED** (`0x101496c50`): the modified time answers whether a reread
/// is worth doing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Watched {
    pub stamp: Option<std::time::SystemTime>,
    pub folding: Folding,
}

impl Watched {
    /// Read the rules again if the file has changed.
    pub fn read_again(&mut self, folder: &Path) -> bool {
        let path = character_path(folder);
        let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if stamp.is_some() && stamp == self.stamp {
            return false;
        }
        self.stamp = stamp;
        self.folding.rules = load(folder).unwrap_or_default();
        self.folding.fold();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn redirect_chains_are_followed_to_their_end() {
        let folded = fold_redirects(&pairs(&[("a", "b"), ("b", "c"), ("x", "y")]));
        assert_eq!(
            folded,
            pairs(&[("a", "c"), ("b", "c"), ("x", "y")])
        );
    }

    #[test]
    fn a_circle_is_cut() {
        let folded = fold_redirects(&pairs(&[("a", "b"), ("b", "a")]));
        assert_eq!(folded[0], ("a".to_string(), "b".to_string()));
        assert_eq!(folded.len(), 2);
    }

    #[test]
    fn folding_answers_where_a_stream_goes() {
        let mut folding = Folding {
            redirects: pairs(&[("a", "b"), ("b", "c")]),
            ..Folding::default()
        };
        assert_eq!(folding.of("a"), "b");
        folding.fold();
        assert_eq!(folding.of("a"), "c");
        assert_eq!(folding.of("z"), "z");
    }

    #[test]
    fn a_line_no_rule_claims_is_the_main_story() {
        let rules = Rules::default();
        assert_eq!(said(&rules, "The innkeeper says hello."), "main");
        assert_eq!(character_path(Path::new("/data")), PathBuf::from("/data/streams.toml"));
    }
}
