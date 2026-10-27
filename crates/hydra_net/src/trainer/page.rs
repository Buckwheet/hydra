//! The training pages on play.net.
//!
//! Recovered from `hydra_net::trainer::page` (`0x101c3c7c8` .. `0x101c43a40`).
//! The trainer page carries the skill list in little `var` declarations and
//! answers a goal submission with a fresh copy of itself.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::trainer::goals::SkillGoals;

/// The skill page for a character.
///
/// Recovered from the literal `https://www.play.net/gs4/play/cm/{}`
/// (0x103e7f2f0).
pub fn page_url(character: &str) -> String {
    format!("https://www.play.net/gs4/play/cm/{}", character)
}

/// One of the page's three recovered `var` patterns, built for `name`.
///
/// The original built the pattern from the skill name it was looking for
/// (`regex::Regex::new(&format!(...))`), which is why `var skill` cannot match
/// `var skills`: the literal name is followed by `\s*=`, and the `s` of
/// `skills` is neither whitespace nor `=`.  The name is escaped so a caller
/// cannot smuggle regex syntax into it.
fn var_pattern(name: &str, tail: &str) -> Option<Regex> {
    Regex::new(&format!(r"var\s+{}\s*=\s*{}", regex::escape(name), tail)).ok()
}

/// The value of a quoted `var NAME = "..."` declaration.
///
/// Recovered from `0x101c3c7c8`: the page is searched with
/// `var\s+NAME\s*=\s*"([^"]*)"\s*;` (0x103e7f26a).
pub fn quoted_var<'a>(page: &'a str, name: &str) -> Option<&'a str> {
    let pattern = var_pattern(name, r#""([^"]*)"\s*;"#)?;
    let captures = pattern.captures(page)?;
    captures.get(1).map(|value| value.as_str())
}

/// The value of a `var NAME = -?123` declaration.
///
/// Recovered from the same function: the page is searched with
/// `var\s+NAME\s*=\s*(-?\d+)\s*;` (0x103e7f2ad).
pub fn number_var(page: &str, name: &str) -> Option<i64> {
    let pattern = var_pattern(name, r"(-?\d+)\s*;")?;
    let captures = pattern.captures(page)?;
    captures.get(1)?.as_str().parse().ok()
}

/// The numbers in a `var NAME = new Array(1, 2, 3)` declaration.
///
/// Recovered from the same function: the page is searched with
/// `var\s+NAME\s*=\s*new\s+Array\(([^)]*)\)` (0x103e7f2c9).  The list is the
/// pattern's own capture, split on `,`; an item that is not a number is
/// dropped, as the original's `filter_map` did.
pub fn array_var(page: &str, name: &str) -> Vec<i64> {
    let Some(pattern) = var_pattern(name, r"new\s+Array\(([^)]*)\)") else {
        return Vec::new();
    };
    let Some(captures) = pattern.captures(page) else {
        return Vec::new();
    };
    let Some(list) = captures.get(1) else {
        return Vec::new();
    };
    list.as_str()
        .split(',')
        .filter_map(|item| item.trim().parse().ok())
        .collect()
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
/// cell and its rank in the `normS1` cell that follows.  The original filled
/// the `{}` with the skill name it was asking about; this scan keeps the same
/// pattern with a capture in place of the name so a page can be read whole.
/// The `</td> … <td>` bridge is part of the recovered pattern, so a table that
/// does not have it yields no rows (and so is refused by [`read_page`]).
fn skill_rows(page: &str) -> Vec<(String, i64)> {
    let Some(pattern) = skill_row_pattern() else {
        return Vec::new();
    };
    pattern
        .captures_iter(page)
        .filter_map(|captures| {
            let name = captures.get(1)?.as_str().trim();
            let rank: i64 = captures.get(2)?.as_str().trim().parse().unwrap_or(0);
            Some((name.to_string(), rank))
        })
        .collect()
}

/// The compiled skill-table pattern (0x103e7f35e), built once.
fn skill_row_pattern() -> Option<&'static Regex> {
    static PATTERN: OnceLock<Option<Regex>> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            Regex::new(
                r#"(?is)<b class="normS2">([^<]+)</b></td>\s*<td[^>]*><b class="normS1">([^<]+)</b>"#,
            )
            .ok()
        })
        .as_ref()
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
    fn the_var_patterns_are_the_recovered_ones() {
        // The trailing `;` is part of all three recovered patterns.
        assert_eq!(quoted_var("var a = \"x\"", "a"), None);
        assert_eq!(number_var("var a = 3", "a"), None);
        assert_eq!(quoted_var("var a = \"x\";", "a"), Some("x"));

        // `var\s+NAME\s*=\s*…`: any whitespace will do, and none will not.
        assert_eq!(number_var("var\ta = 3;", "a"), Some(3));
        assert_eq!(number_var("var\n  a   =   3 ;", "a"), Some(3));
        assert_eq!(number_var("vara = 3;", "a"), None);
        assert_eq!(array_var("var a=new Array(7);", "a"), vec![7]);

        // A name with pattern syntax in it is escaped, not compiled.
        assert_eq!(number_var("var a.b = 3;", "a.b"), Some(3));
        assert_eq!(number_var("var axb = 3;", "a.b"), None);

        // Signs, and an array whose items are not all numbers.
        assert_eq!(number_var("var a = -12;", "a"), Some(-12));
        assert!(array_var("var a = new Array();", "a").is_empty());
        assert_eq!(array_var("var a = new Array( 4 ,x, 5 );", "a"), vec![4, 5]);

        // The quoted value stops at the first closing quote, and the `;` must
        // follow it at once: an escaped quote inside the value therefore ends
        // the match without a `;`, so there is no match at all.
        assert_eq!(quoted_var("var a = \"x \\\" y\";", "a"), None);
        assert_eq!(quoted_var("var a = \"x y\";", "a"), Some("x y"));
    }

    #[test]
    fn the_skill_table_needs_the_recovered_cell_bridge() {
        // Name and rank in adjacent cells, as the recovered pattern reads them.
        let page = "<b class=\"normS2\">Climbing</b></td>\n<td class=\"x\"><b class=\"normS1\">2</b>";
        assert_eq!(read_page(page).unwrap().goal("Climbing"), Some(2));

        // The same two cells without the `</td> … <td>` bridge are not a row.
        let page = "<b class=\"normS2\">Climbing</b><b class=\"normS1\">2</b>";
        assert_eq!(read_page(page).unwrap_err(), "the page has no skills");

        // `(?is)`: the tags are matched case insensitively, and `.`-less classes
        // still span the newline between the cells.
        let page = "<B CLASS=\"NORMS2\">Climbing</B></td><td><B class=\"normS1\">3</B>";
        assert_eq!(read_page(page).unwrap().goal("Climbing"), Some(3));

        // A rank cell that is not a number is read as zero, as the recovered
        // `unwrap_or` did.
        let page = "<b class=\"normS2\">Climbing</b></td><td><b class=\"normS1\">n/a</b>";
        assert_eq!(read_page(page).unwrap().goal("Climbing"), Some(0));
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
