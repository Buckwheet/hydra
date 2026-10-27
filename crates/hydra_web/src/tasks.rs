//! Task/bounty panel state for the page.
//!
//! Recovered from `tasks::tasks` (`0x1002cc48c`) and the serde glue of
//! `TasksOut` (`0x1002cc0f0`, `0x1002cc228`) and `ObjectiveOut`
//! (`0x1002cc354`).

use crate::json::Value;

/// One objective (`ObjectiveOut`): `name, kind, location, state, description`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ObjectiveOut {
    pub name: String,
    pub kind: String,
    pub location: String,
    pub state: String,
    pub description: String,
}

impl ObjectiveOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("name", Value::str(self.name.clone())),
            ("kind", Value::str(self.kind.clone())),
            ("location", Value::str(self.location.clone())),
            ("state", Value::str(self.state.clone())),
            ("description", Value::str(self.description.clone())),
        ]
    }
}

/// The panel payload (`TasksOut`): `brief, bounty, objectives`.
#[derive(Clone, Debug, Default, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TasksOut {
    pub brief: String,
    pub bounty: String,
    pub objectives: Vec<ObjectiveOut>,
}

impl TasksOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("brief", Value::str(self.brief.clone())),
            ("bounty", Value::str(self.bounty.clone())),
            (
                "objectives",
                Value::Arr(
                    self.objectives
                        .iter()
                        .map(|o| crate::wire::object(o.fields()))
                        .collect(),
                ),
            ),
        ]
    }
}

/// The objectives panel (`0x1002cc48c`).
///
/// Recovered: the story stream with id `"tasks"` (a 6-byte needle) is fetched
/// with `hydra_ui::story::streams::Streams::get`, then `brief` / `bounty` /
/// `objectives` are filled from it.  The stream text is the `-brief` /
/// `-bounty` markup the game sends; this splits it on those markers.
///
/// `// UNKNOWN:` the exact in-stream markup.  The safest behaviour keeps the
/// whole stream text as `brief` when neither marker is present.
pub fn tasks(stream_text: &str) -> TasksOut {
    let mut out = TasksOut::default();
    let mut objectives_start = None;
    for (i, line) in stream_text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("bounty:") {
            out.bounty = rest.trim().to_string();
        } else if let Some(rest) = trimmed.strip_prefix("brief:") {
            out.brief = rest.trim().to_string();
        } else if trimmed.eq_ignore_ascii_case("objectives") {
            objectives_start = Some(i + 1);
        }
    }
    if out.brief.is_empty() {
        out.brief = stream_text.trim().to_string();
    }
    if let Some(start) = objectives_start {
        for line in stream_text.lines().skip(start) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, ':');
            let name = parts.next().unwrap_or("").trim();
            let description = parts.next().unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            out.objectives.push(ObjectiveOut {
                name: name.to_string(),
                kind: String::new(),
                location: String::new(),
                state: String::new(),
                description: description.to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_order_matches_the_binary() {
        let t = TasksOut {
            brief: "Kill ten kobolds".into(),
            bounty: "kobold".into(),
            objectives: vec![ObjectiveOut {
                name: "kobolds".into(),
                kind: "kill".into(),
                location: "Wehnimer's Landing".into(),
                state: "2/10".into(),
                description: "kobolds slain".into(),
            }],
        };
        let json = crate::wire::object(t.fields()).to_json();
        assert!(json.starts_with(r#"{"brief":"Kill ten kobolds","bounty":"kobold","objectives":[{"name":"kobolds","#));
        assert!(json.contains(r#""state":"2/10","description":"kobolds slain"}"#));
    }

    #[test]
    fn parses_a_stream_with_markers() {
        let text = "brief: Kill ten kobolds\nbounty: kobold\nobjectives\nkobolds: kobolds slain\n";
        let t = tasks(text);
        assert_eq!(t.brief, "Kill ten kobolds");
        assert_eq!(t.bounty, "kobold");
        assert_eq!(t.objectives.len(), 1);
        assert_eq!(t.objectives[0].name, "kobolds");
        assert_eq!(t.objectives[0].description, "kobolds slain");
    }

    #[test]
    fn unmarked_stream_becomes_the_brief() {
        let t = tasks("You have no tasks.\n");
        assert_eq!(t.brief, "You have no tasks.");
        assert!(t.bounty.is_empty());
        assert!(t.objectives.is_empty());
    }
}
