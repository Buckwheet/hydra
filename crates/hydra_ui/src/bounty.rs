//! The bounty brief: what to do next for the current task.
//!
//! `bounty::brief` @0x10185afc4 is 10 KB.  The recovered shape is
//! `brief(&Task) -> Vec<…>`: it matches the task kind and pushes one line per
//! step, each with the theme token it should be painted with.  The literals
//! (0x103992a23) are, in order:
//!
//! ```text
//! "No task currently" "Visit Guard" "Creatures" "Visit Gemshop" "Gem Bounty"
//! "Heirloom Task" "Foraging Bounty" "Rescue Kid" "Visit Furrier"
//! "Skinning Bounty" "Turn in task at AG" "Return Heirloom" "Report Success"
//! "Found Dangerous" "Return Child to Guard" "Bandit Task"
//! "Dangerous Creature Task" "Escort Task" "Foraging Task" "Rescue Child Task"
//! "Skinning Task" "Culling Task" "FAILED BOUNTY!!"
//! ```

use hydra_model::state::bounty::{Task, TaskKind};

use crate::theme::Token;

/// One line of the brief.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub token: Token,
}

impl Line {
    fn new(text: impl Into<String>, token: Token) -> Line {
        Line {
            text: text.into(),
            token,
        }
    }
}

/// The task-kind titles the brief shows.
pub fn title(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Creature => "Dangerous Creature Task",
        TaskKind::Heirloom => "Heirloom Task",
        TaskKind::Skin => "Skinning Task",
        TaskKind::Herb => "Foraging Task",
        TaskKind::Escort => "Escort Task",
        TaskKind::Unknown => "No task currently",
    }
}

/// The town/step a task starts at.
pub fn start(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Creature => "Visit Guard",
        TaskKind::Heirloom => "Visit Gemshop",
        TaskKind::Skin => "Visit Furrier",
        TaskKind::Herb => "Visit Guard",
        TaskKind::Escort => "Visit Guard",
        TaskKind::Unknown => "Visit Guard",
    }
}

/// The turn-in step.
pub fn finish(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Creature => "Return Child to Guard",
        TaskKind::Heirloom => "Return Heirloom",
        TaskKind::Skin => "Turn in task at AG",
        TaskKind::Herb => "Report Success",
        TaskKind::Escort => "Return Child to Guard",
        TaskKind::Unknown => "Visit Guard",
    }
}

/// `@0x10185afc4` — the brief for a task.
pub fn brief(task: &Task) -> Vec<Line> {
    let mut out = Vec::new();
    if task.kind == TaskKind::Unknown {
        out.push(Line::new("No task currently", Token::Muted));
        out.push(Line::new("Visit Guard", BOUNTY));
        return out;
    }
    out.push(Line::new(title(task.kind), BOUNTY));
    if let Some(town) = &task.town {
        out.push(Line::new(town.clone(), Token::Muted));
    }
    out.push(Line::new(start(task.kind), Token::Object));
    if let Some(target) = &task.target {
        if task.count > 1 {
            out.push(Line::new(
                format!("{target} x{}", task.count),
                Token::Creature,
            ));
        } else {
            out.push(Line::new(target.clone(), Token::Creature));
        }
    }
    out.push(Line::new(finish(task.kind), Token::Object));
    out
}

/// The token the brief's title uses.
///
/// `// UNKNOWN:` the recovered brief selects a token per line; `warning` is the
/// warm accent the bounty panel is drawn in.
pub const BOUNTY: Token = Token::Warning;

#[cfg(test)]
mod tests {
    use super::*;

    fn task(kind: TaskKind, target: Option<&str>, count: u32) -> Task {
        Task {
            kind,
            target: target.map(String::from),
            count,
            ..Task::default()
        }
    }

    #[test]
    fn unknown_tasks_get_the_guard_hint() {
        let lines = brief(&task(TaskKind::Unknown, None, 0));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "No task currently");
        assert_eq!(lines[1].text, "Visit Guard");
    }

    #[test]
    fn creature_tasks_list_the_target() {
        let mut t = task(TaskKind::Creature, Some("troll"), 3);
        t.town = Some("Wehnimer's Landing".into());
        let lines = brief(&t);
        assert_eq!(lines[0].text, "Dangerous Creature Task");
        assert_eq!(lines[1].text, "Wehnimer's Landing");
        assert_eq!(lines[2].text, "Visit Guard");
        assert_eq!(lines[3].text, "troll x3");
        assert_eq!(lines[4].text, "Return Child to Guard");
    }

    #[test]
    fn kinds_have_distinct_titles_and_starts() {
        assert_eq!(title(TaskKind::Heirloom), "Heirloom Task");
        assert_eq!(start(TaskKind::Heirloom), "Visit Gemshop");
        assert_eq!(finish(TaskKind::Heirloom), "Return Heirloom");
        assert_eq!(title(TaskKind::Skin), "Skinning Task");
        assert_eq!(start(TaskKind::Skin), "Visit Furrier");
        assert_eq!(title(TaskKind::Herb), "Foraging Task");
        assert_eq!(finish(TaskKind::Herb), "Report Success");
        assert_eq!(title(TaskKind::Escort), "Escort Task");
    }

    #[test]
    fn a_single_target_has_no_count() {
        let lines = brief(&task(TaskKind::Creature, Some("troll"), 1));
        assert!(lines.iter().any(|l| l.text == "troll"));
    }
}
