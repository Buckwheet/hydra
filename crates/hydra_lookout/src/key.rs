//! Identity keys: what the lookout calls an object or a sentence.
//!
//! Both entry points are recovered from the disassembly:
//!
//! * [`object`] (`0x101ace5b8`) trims, lowercases, splits on whitespace,
//!   drops a leading `a`/`an`/`the`/`some` when at least one word follows and
//!   joins the words back with a single space.
//! * [`sentence`] (`0x101ace768`) joins the spans a pattern matched out of a
//!   line with `'‹'` (`0x101ace8e0`: bytes `e2 80 b9`).  The spans come from
//!   the caller, so this module offers both the span-joining form and the
//!   word-level normalisation the keys end up using.

/// The separator `sentence` puts between the spans it joins
/// (`0x101ace8e0`, `0x101ace8e8`).
pub const SPAN_SEPARATOR: char = '\u{2039}';

/// The articles [`object`] drops when something follows them
/// (`0x101ace630` … `0x101ace724`).
pub const ARTICLES: [&str; 4] = ["a", "an", "the", "some"];

/// The identity key of an object name (`0x101ace5b8`).
///
/// ```text
/// object("  A Rusty   Sword ") == "rusty sword"
/// object("some")              == "some"     (nothing follows the article)
/// ```
///
/// Words made only of the punctuation `!"#$%&'()*+,-.` are dropped: the filter
/// in the original keeps a word when it holds a character above `'.'`
/// (`0x101ace60c`…`0x101ace714` compare `c - 0x3a` against `-11`, i.e. `c`
/// above `'.'`).
pub fn object(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let mut words: Vec<&str> = lower
        .split_whitespace()
        .filter(|word| word.chars().any(|c| c > '.'))
        .collect();
    if words.len() >= 2 && ARTICLES.contains(&words[0]) {
        words.remove(0);
    }
    words.join(" ")
}

/// The words of a line, lowercased, with everything that is not a letter, a
/// digit or an apostrophe treated as a separator.
///
/// This is the word-level half of [`sentence`]: the original asks the caller's
/// pattern for the spans and then only joins them, so what a "word" is here
/// decides what a sentence key looks like.
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .map(|word| word.to_lowercase())
        .collect()
}

/// The identity key of a sentence: its words joined by one space.
///
/// `sentence("A Kobold scratches you.") == "a kobold scratches you"`.
pub fn sentence(text: &str) -> String {
    words(text).join(" ")
}

/// The sentence key of `text` with the spans a pattern matched kept apart
/// (`0x101ace768`).
///
/// The original copies `text[from..to]` for every match into one `String`,
/// separating them with [`SPAN_SEPARATOR`], after checking that the ends sit on
/// character boundaries (`0x101ace85c`).  Spans that are out of bounds or
/// inside a character are skipped.
pub fn spans(text: &str, spans: &[(usize, usize)]) -> String {
    let mut out = String::with_capacity(text.len());
    for &(from, to) in spans {
        if from > to || to > text.len() {
            continue;
        }
        if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
            continue;
        }
        if !out.is_empty() {
            out.push(SPAN_SEPARATOR);
        }
        out.push_str(&text[from..to]);
    }
    out.to_lowercase()
}

/// Every object key in a line: the phrases split on commas, semicolons and
/// `and`.
///
/// **INFERRED**: the original splits a judged line into the objects it names
/// inside 4 KB of inlined `regex_automata` capture code (`Lookout::judge`), so
/// the split itself is not recovered; this is the documented equivalent.
pub fn objects(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for phrase in text.split([',', ';']) {
        for part in phrase.split(" and ") {
            let key = object(part);
            if !key.is_empty() && !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_object_key_drops_the_article_lowercases_and_collapses_spaces() {
        assert_eq!(object("  A Rusty   Sword "), "rusty sword");
        assert_eq!(object("The Kobold"), "kobold");
        assert_eq!(object("AN Ancient Tome"), "ancient tome");
        assert_eq!(object("some Goblins"), "goblins");
    }

    #[test]
    fn an_object_key_keeps_a_lone_article() {
        // the article is only dropped when another word follows it
        assert_eq!(object("a"), "a");
        assert_eq!(object("some"), "some");
        assert_eq!(object("the"), "the");
    }

    #[test]
    fn an_object_key_keeps_punctuation_but_drops_punctuation_only_words() {
        assert_eq!(object("A kobold scratches you."), "kobold scratches you.");
        assert_eq!(object("(*)"), "");
        assert_eq!(object("(*) kobold"), "kobold");
        assert_eq!(object("--- , kobold"), "kobold");
    }

    #[test]
    fn a_sentence_key_is_its_words_joined() {
        assert_eq!(sentence("A Kobold scratches you."), "a kobold scratches you");
        assert_eq!(sentence("  The   kobold,  again!  "), "the kobold again");
        assert_eq!(sentence(""), "");
    }

    #[test]
    fn spans_are_joined_with_the_recovered_separator() {
        let text = "AS: 25, DS: 10";
        assert_eq!(spans(text, &[(0, 6), (8, 14)]), "as: 25\u{2039}ds: 10");
        assert_eq!(spans(text, &[]), "");
        // a span that is out of bounds is skipped
        assert_eq!(spans(text, &[(0, 6), (0, 99)]), "as: 25");
    }

    #[test]
    fn objects_are_split_on_commas_and_and() {
        assert_eq!(
            objects("a rusty sword, a kobold and some goblins"),
            vec![
                "rusty sword".to_string(),
                "kobold".to_string(),
                "goblins".to_string()
            ]
        );
        // duplicates are dropped
        assert_eq!(objects("a kobold and a kobold"), vec!["kobold".to_string()]);
    }
}
