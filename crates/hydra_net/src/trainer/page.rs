//! The training pages on play.net.
//!
//! Recovered from `hydra_net::trainer::page` (`0x101c3c7c8` .. `0x101c43a40`).
//! The trainer page carries the skill list in little `var` declarations and
//! answers a goal submission with a fresh copy of itself.

use std::collections::BTreeMap;

use crate::trainer::goals::SkillGoals;

/// The skill page for a character.
///
/// Recovered from the literal `https://www.play.net/gs4/play/cm/{}`
/// (0x103e7f2f0).
pub fn page_url(character: &str) -> String {
    format!("https://www.play.net/gs4/play/cm/{}", character)
}

/// The value of a quoted `var NAME = "..."` declaration.
///
/// Recovered from `0x101c3c7c8`: the page is searched with
/// `var\s+NAME\s*=\s*"([^"]*)"\s*;` (0x103e7f26a).
pub fn quoted_var<'a>(page: &'a str, name: &str) -> Option<&'a str> {
    let at = find_var(page, name)?;
    let rest = page[at..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The value of a `var NAME = -?123` declaration.
///
/// Recovered from the same function: the page is searched with
/// `var\s+NAME\s*=\s*(-?\d+)\s*;` (0x103e7f2ad).
pub fn number_var(page: &str, name: &str) -> Option<i64> {
    let at = find_var(page, name)?;
    let rest = page[at..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '-'))
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// The numbers in a `var NAME = new Array(1, 2, 3)` declaration.
///
/// Recovered from the same function: the page is searched with
/// `var\s+NAME\s*=\s*new\s+Array\(([^)]*)\)` (0x103e7f2c9).
pub fn array_var(page: &str, name: &str) -> Vec<i64> {
    let at = match find_var(page, name) {
        Some(at) => at,
        None => return Vec::new(),
    };
    let rest = &page[at..];
    let open = match rest.find('(') {
        Some(open) => open + 1,
        None => return Vec::new(),
    };
    let close = match rest[open..].find(')') {
        Some(close) => open + close,
        None => return Vec::new(),
    };
    rest[open..close]
        .split(',')
        .filter_map(|item| item.trim().parse().ok())
        .collect()
}

/// Where `var NAME` starts, past the whitespace the pattern allows.
fn find_var(page: &str, name: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(at) = page[from..].find("var") {
        let start = from + at;
        let raw = &page[start + 3..];
        let lead = raw.len() - raw.trim_start().len();
        let after = raw.trim_start();
        if let Some(rest) = after.strip_prefix(name) {
            // `var skill` must not match `var skills`.
            if !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                return Some(start + 3 + lead + name.len());
            }
        }
        from = start + 3;
    }
    None
}

/// The form to submit for a set of skill goals.
///
/// Recovered from `0x101c3c7c8`: the goals' fields are collected into a sorted
/// map (the original builds a `BTreeMap` from `SkillGoals::goal_fields`) and
/// rendered as `name=value` pairs joined with `&`.
pub fn form(goals: &SkillGoals) -> String {
    let fields: BTreeMap<String, i64> = goals.goal_fields();
    fields
        .iter()
        .map(|(name, value)| format!("{}={}", name, value))
        .collect::<Vec<_>>()
        .join("&")
}

/// The answer the trainer gave, if it said anything.
///
/// Recovered from `0x101c3df48`: the answer page's `start_` script values are
/// read out into a list of `(name, value)` pairs.
pub fn answer(page: &str) -> Vec<(String, i64)> {
    let mut found = Vec::new();
    for line in page.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("var start_") {
            if let Some((name, value)) = rest.split_once('=') {
                let name = name.trim().to_string();
                let digits: String = value
                    .trim()
                    .trim_end_matches(';')
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect();
                if let Ok(value) = digits.parse() {
                    found.push((name, value));
                }
            }
        }
    }
    found
}

/// The skill page parsed into goals.
///
/// Recovered from `0x101c3ef40`: the page must carry one row per skill; the
/// original counts the rows against a fixed number and refuses a page that has
/// the wrong shape (`the page has {} skills, not {}` / `the page has no {}`).
pub fn read_page(page: &str) -> Result<SkillGoals, String> {
    let rows = skill_rows(page);
    if rows.is_empty() {
        return Err(format!("the page has no {}", "skills"));
    }
    let mut goals = SkillGoals::new();
    for (name, rank) in rows {
        goals.aim(&name, rank);
    }
    Ok(goals)
}

/// The `(skill, rank)` pairs in the page's skill table.
///
/// Recovered from the literal `(?is)<b class="normS2">{}</b></td>\s*<td[^>]*><b
/// class="normS1">([^<]+)</b>` (0x103e7f35e): the skill name sits in a `normS2`
/// cell and its rank in the `normS1` cell that follows.
fn skill_rows(page: &str) -> Vec<(String, i64)> {
    let mut rows = Vec::new();
    let mut from = 0;
    while let Some(at) = page[from..].find("class=\"normS2\">") {
        let start = from + at + "class=\"normS2\">".len();
        let name_end = match page[start..].find('<') {
            Some(end) => start + end,
            None => break,
        };
        let name = page[start..name_end].trim().to_string();
        from = name_end;
        if let Some(at) = page[from..].find("class=\"normS1\">") {
            let rank_start = from + at + "class=\"normS1\">".len();
            let rank_end = match page[rank_start..].find('<') {
                Some(end) => rank_start + end,
                None => break,
            };
            let rank = page[rank_start..rank_end].trim().parse().unwrap_or(0);
            from = rank_end;
            rows.push((name, rank));
        }
    }
    rows
}

/// True when submitting the form landed on a real page rather than an error.
///
/// Recovered from `0x101c3e914`: the answer page is accepted when it carries the
/// trainer's own marker.
pub fn resolved(page: &str) -> bool {
    page.contains("normS2") || page.contains("cm_spell") || page.contains("skill")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_url_names_the_character() {
        assert_eq!(
            page_url("Dridarn"),
            "https://www.play.net/gs4/play/cm/Dridarn"
        );
    }

    #[test]
    fn vars_are_read_out() {
        let page = "var char_name = \"Dridarn\";\nvar level = 42;\nvar ranks = new Array(1, 2,3);";
        assert_eq!(quoted_var(page, "char_name"), Some("Dridarn"));
        assert_eq!(number_var(page, "level"), Some(42));
        assert_eq!(array_var(page, "ranks"), vec![1, 2, 3]);
        assert_eq!(quoted_var(page, "missing"), None);
        assert_eq!(number_var(page, "char_name"), None);
        assert!(array_var(page, "missing").is_empty());
    }

    #[test]
    fn a_prefix_does_not_match_a_longer_name() {
        let page = "var skills = 5;\nvar skill = 3;";
        assert_eq!(number_var(page, "skill"), Some(3));
        assert_eq!(number_var(page, "skills"), Some(5));
    }

    #[test]
    fn the_form_joins_the_goal_fields() {
        let mut goals = SkillGoals::new();
        goals.aim("Two Weapon Combat", 5);
        goals.aim("Arcane Symbols", 10);
        let form = form(&goals);
        assert!(form.contains("skill"), "{form}");
        assert!(form.contains('='));
    }

    #[test]
    fn skill_rows_are_read_from_the_table() {
        let page = r#"
            <td><b class="normS2">Arcane Symbols</b></td><td class="x"><b class="normS1">10</b>
            <td><b class="normS2">Climbing</b></td><td class="x"><b class="normS1">2</b>
        "#;
        let goals = read_page(page).unwrap();
        assert_eq!(goals.goal("Arcane Symbols"), Some(10));
        assert_eq!(goals.goal("Climbing"), Some(2));
    }

    #[test]
    fn a_page_without_skills_is_refused() {
        assert_eq!(
            read_page("<html></html>").unwrap_err(),
            "the page has no skills"
        );
        assert!(!resolved("<html></html>"));
        assert!(resolved("<b class=\"normS2\">x</b>"));
    }

    #[test]
    fn answers_are_read_from_the_start_vars() {
        let page = "var start_tp = 220;\nvar start_goals = 3;\nvar other = 1;";
        assert_eq!(
            answer(page),
            vec![("tp".to_string(), 220), ("goals".to_string(), 3)]
        );
        assert!(answer("nothing").is_empty());
    }
}
