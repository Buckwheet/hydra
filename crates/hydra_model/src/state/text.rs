//! Prose helpers: pull a creature or an item out of a line.
//!
//! Both are 300-byte functions. `creature` matches the article-prefixed noun
//! phrases (`a `/`an `/`the `) and `item` the same shape for objects.

/// @0x0000000101b8d8?? (300 B) — the creature mentioned in a line.
pub fn creature(line: &str) -> Option<String> {
    extract(line, &["a ", "an ", "the ", "some "])
}

/// @0x0000000101b8d968 (300 B) — the item mentioned in a line.
pub fn item(line: &str) -> Option<String> {
    extract(line, &["a ", "an ", "the ", "some ", "your "])
}

fn extract(line: &str, articles: &[&str]) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    for article in articles {
        // look for the article at a word boundary
        let mut from = 0usize;
        while let Some(found) = lower[from..].find(article) {
            let at = from + found;
            let boundary = at == 0
                || !lower[..at]
                    .chars()
                    .next_back()
                    .map(|c| c.is_alphanumeric())
                    .unwrap_or(false);
            if boundary {
                let rest = &line[at + article.len()..];
                let end = rest
                    .find(|c: char| !(c.is_alphanumeric() || c == ' ' || c == '\'' || c == '-'))
                    .unwrap_or(rest.len());
                let text = rest[..end].trim();
                if !text.is_empty() {
                    let lead = &line[at..at + article.len()];
                    return Some(format!("{lead}{text}"));
                }
            }
            from = at + article.len();
            if from >= lower.len() {
                break;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creature_phrase_is_extracted_with_its_article() {
        assert_eq!(
            creature("The kobold swings at you.").as_deref(),
            Some("The kobold swings at you")
        );
        assert_eq!(creature("A scruffy kobold arrives.").as_deref(), Some("A scruffy kobold arrives"));
    }

    #[test]
    fn item_phrase_is_extracted() {
        assert_eq!(
            item("You pick up a rusty sword.").as_deref(),
            Some("a rusty sword")
        );
    }

    #[test]
    fn no_article_means_none() {
        assert_eq!(creature("Kobold."), None);
        assert_eq!(item("nothing"), None);
    }
}
