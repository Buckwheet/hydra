//! The trainer ("skill manager") panel.
//!
//! Recovered from `trainer::trainer` (`0x1002ccc38`) and the serde glue of
//! `TrainerOut` (`0x1002cc9d4`, `0x1002ccaf0`), `SheetOut` (`0x1002cc748`) and
//! `SkillOut` (`0x1002cc880`).

use crate::json::Value;

/// One trainable skill (`SkillOut`): `section, name, cost, has, goal, id`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillOut {
    pub section: String,
    pub name: String,
    pub cost: String,
    pub has: String,
    pub goal: String,
    pub id: String,
}

impl SkillOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("section", Value::str(self.section.clone())),
            ("name", Value::str(self.name.clone())),
            ("cost", Value::str(self.cost.clone())),
            ("has", Value::str(self.has.clone())),
            ("goal", Value::str(self.goal.clone())),
            ("id", Value::str(self.id.clone())),
        ]
    }
}

/// The skill sheet header (`SheetOut`): `title, physical, mental, skills,
/// changed`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetOut {
    pub title: String,
    pub physical: String,
    pub mental: String,
    pub skills: Vec<SkillOut>,
    pub changed: String,
}

impl SheetOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            ("title", Value::str(self.title.clone())),
            ("physical", Value::str(self.physical.clone())),
            ("mental", Value::str(self.mental.clone())),
            ("skills", Value::Arr(self.skills.iter().map(|s| crate::wire::object(s.fields())).collect())),
            ("changed", Value::str(self.changed.clone())),
        ]
    }
}

/// The whole trainer panel (`TrainerOut`): `profiles, said, sheet, busy`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrainerOut {
    pub profiles: Vec<String>,
    pub said: String,
    pub sheet: Option<SheetOut>,
    pub busy: bool,
}

impl TrainerOut {
    pub fn fields(&self) -> Vec<(&'static str, Value)> {
        vec![
            (
                "profiles",
                Value::Arr(self.profiles.iter().map(|s| Value::str(s.clone())).collect()),
            ),
            ("said", Value::str(self.said.clone())),
            (
                "sheet",
                match &self.sheet {
                    Some(s) => crate::wire::object(s.fields()),
                    None => Value::Null,
                },
            ),
            ("busy", Value::Bool(self.busy)),
        ]
    }
}

/// Recovered advice strings the panel shows while the sheet is missing.
pub const LOADING: &str = "Load reads your skills from play.net.";
pub const READING: &str = "Reading the skill manager…";
pub const APPLYING: &str = "Applying…";
/// The empty-state title.
pub const TITLE: &str = "The skill manager";

/// The trainer panel (`0x1002ccc38`).
///
/// Recovered: `TrainerOut` is built from `Seat::trainer` (the `Trainer` state at
/// `+0x1e0`), the stored goal profiles and the parsed sheet.  The three advice
/// strings above are the ones the binary holds next to the trainer data.
///
/// `// UNKNOWN:` the exact sheet markup.  This parses the recovered shape: a
/// title line, the two training-point words and one `section:name:cost:has:goal`
/// row per skill.
pub fn trainer(profiles: Vec<String>, said: &str, sheet_text: Option<&str>, busy: bool) -> TrainerOut {
    TrainerOut {
        profiles,
        said: said.to_string(),
        sheet: sheet_text.map(|text| sheet(text, said)),
        busy,
    }
}

/// Parse the skill-manager sheet text.
pub fn sheet(text: &str, changed: &str) -> SheetOut {
    let mut out = SheetOut {
        title: TITLE.to_string(),
        changed: changed.to_string(),
        ..SheetOut::default()
    };
    let mut rows = text.lines().filter(|l| !l.trim().is_empty());
    if let Some(title) = rows.next() {
        if !title.trim().starts_with("You have") {
            out.title = title.trim().to_string();
        }
    }
    for line in rows {
        let fields: Vec<&str> = line.split(':').map(str::trim).collect();
        if fields.len() < 2 {
            continue;
        }
        out.skills.push(SkillOut {
            section: fields[0].to_string(),
            name: fields[1].to_string(),
            cost: fields.get(2).copied().unwrap_or("").to_string(),
            has: fields.get(3).copied().unwrap_or("").to_string(),
            goal: fields.get(4).copied().unwrap_or("").to_string(),
            id: fields[1].to_lowercase().replace(' ', "-"),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_order_matches_the_binary() {
        let out = TrainerOut {
            profiles: vec!["hunter".into()],
            said: "Applying…".into(),
            sheet: None,
            busy: true,
        };
        assert_eq!(
            crate::wire::object(out.fields()).to_json(),
            r#"{"profiles":["hunter"],"said":"Applying…","sheet":null,"busy":true}"#
        );
    }

    #[test]
    fn sheet_rows_are_split_into_skills() {
        let s = sheet(
            "The skill manager\nWeapon:Edged Weapons:3:5:10\nLore:Arcane Symbols:1:0:4\n",
            "now",
        );
        assert_eq!(s.title, "The skill manager");
        assert_eq!(s.skills.len(), 2);
        assert_eq!(s.skills[0].section, "Weapon");
        assert_eq!(s.skills[0].name, "Edged Weapons");
        assert_eq!(s.skills[0].id, "edged-weapons");
        assert_eq!(s.skills[0].goal, "10");
        assert_eq!(s.changed, "now");
    }

    #[test]
    fn empty_sheet_keeps_the_default_title() {
        let s = sheet("", "x");
        assert!(s.skills.is_empty());
        assert_eq!(s.title, TITLE);
    }
}
