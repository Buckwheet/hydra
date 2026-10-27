//! The trainer: the goals the character is training towards, and the profile the
//! folder keeps them under.
//!
//! **ESTABLISHED**:
//!
//!  * `GoalProfiles` is the file of named goal sets (`struct GoalProfiles with 2
//!    elements`, `load` `0x1014acd50`, `names` `0x1014acebc`, `change`
//!    `0x1014acf04`).
//!  * `typed` (`0x1014ad130`) reads a typed line as a trainer command.
//!  * `Trainer` is the running state: `read` (`0x1014ad4d8`), `change`
//!    (`0x1014ad684`), `applied` (`0x1014ad858`), `launched` (`0x1014adaf4`).
//!
//! **std departure**: the original held a `hydra_net::trainer::goals::SkillGoals`
//! behind an `Arc<dyn …>`; this crate may not depend on `hydra_net`, so the goals
//! are a plain [`SkillGoals`] here.  The seam is [`Trainer::Launched`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::text::{self, Value};

/// The file the profiles live in.
pub const FILE: &str = "trainer.toml";

/// One goal: a skill and how much of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Goal {
    /// The skill's name, as the game writes it.
    pub skill: String,
    /// How much of it is wanted, 0 meaning "as much as there is".
    pub to: u32,
}

/// A named set of goals.
///
/// **std departure**: stands in for `hydra_net::trainer::goals::SkillGoals`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillGoals {
    pub name: String,
    pub goals: Vec<Goal>,
}

impl SkillGoals {
    /// The goals in the order the trainer should take them.
    pub fn in_order(&self) -> Vec<&Goal> {
        let mut goals: Vec<&Goal> = self.goals.iter().collect();
        goals.sort_by(|a, b| b.to.cmp(&a.to).then(a.skill.cmp(&b.skill)));
        goals
    }
}

/// A change the settings page asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// Use this profile.
    Use(String),
    /// Add or replace a profile.
    Keep(SkillGoals),
    /// Forget a profile.
    Forget(String),
    /// Set one goal's target.
    Goal(String, u32),
}

/// A question the trainer asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// Which profile should be used?
    Which,
    /// What should this goal be taken to?
    HowMuch(String),
}

/// What the trainer is doing.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Status {
    /// Nothing is being trained.
    #[default]
    Idle,
    /// The goals are read and the character is training.
    Training,
    /// The trainer is waiting for the game to say something.
    Waiting(String),
    /// It has stopped, and why.
    Stopped(String),
}

/// The goal profiles a folder keeps.
///
/// **ESTABLISHED** (`0x1014acd50`): the `trainer` table's profiles, one set per
/// name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoalProfiles {
    pub profiles: BTreeMap<String, SkillGoals>,
    /// The profile in use.
    pub using: String,
}

impl GoalProfiles {
    /// **ESTABLISHED** (`0x1014acd50`): read the file, defaults when it is not
    /// there.
    pub fn load(folder: &Path) -> Result<GoalProfiles, String> {
        let path = path(folder);
        if !path.exists() {
            return Ok(GoalProfiles::default());
        }
        GoalProfiles::read(&text::read_file(&path)?)
    }

    /// Read the document.
    pub fn read(doc: &Value) -> Result<GoalProfiles, String> {
        let mut profiles = GoalProfiles::default();
        if let Some(table) = doc.get("trainer").and_then(Value::as_table) {
            profiles.using = table.get("using").and_then(Value::as_str).unwrap_or("").to_string();
            if let Some(sets) = table.get("profiles").and_then(Value::as_table) {
                for (name, value) in sets {
                    let goals = value
                        .get("goals")
                        .and_then(Value::as_array)
                        .map(|items| {
                            items
                                .iter()
                                .map(|item| Goal {
                                    skill: item.str_of("skill"),
                                    to: item.u64_of("to").unwrap_or(0) as u32,
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    profiles.profiles.insert(
                        name.clone(),
                        SkillGoals {
                            name: name.clone(),
                            goals,
                        },
                    );
                }
            }
        }
        Ok(profiles)
    }

    /// Write the document.
    pub fn write(&self) -> Value {
        let mut sets = BTreeMap::new();
        for (name, goals) in &self.profiles {
            let items = goals
                .goals
                .iter()
                .map(|goal| {
                    let mut map = BTreeMap::new();
                    map.insert("skill".to_string(), Value::Str(goal.skill.clone()));
                    map.insert("to".to_string(), Value::Int(goal.to as i64));
                    Value::Table(map)
                })
                .collect();
            let mut set = BTreeMap::new();
            set.insert("goals".to_string(), Value::Array(items));
            sets.insert(name.clone(), Value::Table(set));
        }
        let mut table = BTreeMap::new();
        table.insert("using".to_string(), Value::Str(self.using.clone()));
        table.insert("profiles".to_string(), Value::Table(sets));
        let mut doc = BTreeMap::new();
        doc.insert("trainer".to_string(), Value::Table(table));
        Value::Table(doc)
    }

    /// The profile names, in order.
    ///
    /// **ESTABLISHED** (`0x1014acebc`).
    pub fn names(&self) -> Vec<&str> {
        self.profiles.keys().map(String::as_str).collect()
    }

    /// Apply a change.
    ///
    /// **ESTABLISHED** (`0x1014acf04`): a profile that is not there cannot be
    /// used.
    pub fn change(&mut self, change: Change) -> Result<(), String> {
        match change {
            Change::Use(name) => {
                if !self.profiles.contains_key(&name) {
                    return Err(format!("There is no profile {name}."));
                }
                self.using = name;
                Ok(())
            }
            Change::Keep(goals) => {
                self.profiles.insert(goals.name.clone(), goals);
                Ok(())
            }
            Change::Forget(name) => {
                self.profiles.remove(&name);
                if self.using == name {
                    self.using.clear();
                }
                Ok(())
            }
            Change::Goal(skill, to) => {
                let goals = self
                    .profiles
                    .get_mut(&self.using)
                    .ok_or_else(|| "No profile is in use.".to_string())?;
                match goals.goals.iter_mut().find(|goal| goal.skill == skill) {
                    Some(goal) => goal.to = to,
                    None => goals.goals.push(Goal { skill, to }),
                }
                Ok(())
            }
        }
    }

    /// The profile in use.
    pub fn used(&self) -> Option<&SkillGoals> {
        self.profiles.get(&self.using)
    }
}

/// Where a folder's profiles live.
pub fn path(folder: &Path) -> PathBuf {
    folder.join(FILE)
}

/// The trainer's running state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trainer {
    pub status: Status,
    pub goals: SkillGoals,
    /// The line the trainer last had the character type.
    pub typed: String,
}

/// A row of the trainer's window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrainerView {
    pub skill: String,
    pub to: u32,
    pub doing: bool,
}

impl Trainer {
    /// **ESTABLISHED** (`0x1014ad4d8`): the goals as the window lists them.
    pub fn read(&self) -> Vec<TrainerView> {
        self.goals
            .goals
            .iter()
            .map(|goal| TrainerView {
                skill: goal.skill.clone(),
                to: goal.to,
                doing: self.status == Status::Training,
            })
            .collect()
    }

    /// Apply a change.
    ///
    /// **ESTABLISHED** (`0x1014ad684`): the goals are changed and the trainer
    /// goes back to waiting.
    pub fn change(&mut self, change: Change) -> Result<(), String> {
        match change {
            Change::Keep(goals) => {
                self.goals = goals;
                Ok(())
            }
            Change::Goal(skill, to) => {
                match self.goals.goals.iter_mut().find(|goal| goal.skill == skill) {
                    Some(goal) => goal.to = to,
                    None => self.goals.goals.push(Goal { skill, to }),
                }
                Ok(())
            }
            Change::Use(_) | Change::Forget(_) => Ok(()),
        }
    }

    /// The command the trainer wants typed next.
    ///
    /// **PARTLY** (`0x1014ad858`): UNKNOWN how the trainer words its own
    /// commands; this asks the game for the first goal in order.
    pub fn applied(&mut self) -> Option<String> {
        let goal = self.goals.in_order().first().map(|goal| goal.skill.clone())?;
        self.typed = format!("train {goal}");
        Some(self.typed.clone())
    }

    /// Start training.
    ///
    /// **ESTABLISHED** (`0x1014adaf4`): the state moves to `Training` and the
    /// first command is typed.  The tokio task the binary spawned is the seam:
    /// this is synchronous, and the caller drives it.
    pub fn launched(&mut self) -> Option<String> {
        if self.goals.goals.is_empty() {
            self.status = Status::Stopped("There is nothing to train.".to_string());
            return None;
        }
        self.status = Status::Training;
        self.applied()
    }
}

/// Read a typed line as a trainer command.
///
/// **ESTABLISHED** (`0x1014ad130`): `train` and `trainer` with a skill name.
pub fn typed(line: &str) -> Option<Change> {
    let mut words = line.split_whitespace();
    match words.next()? {
        "train" | "trainer" => {
            let skill = words.next()?;
            let to = words.next().and_then(|word| word.parse().ok()).unwrap_or(0);
            Some(Change::Goal(skill.to_string(), to))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_round_trip() {
        let text = "\
[trainer]
using = \"combat\"
[trainer.profiles.combat]
goals = [ { skill = \"Two Weapon Combat\", to = 40 }, { skill = \"Dodging\", to = 20 } ]
";
        let profiles = GoalProfiles::read(&text::parse_toml(text).unwrap()).unwrap();
        assert_eq!(profiles.names(), vec!["combat"]);
        assert_eq!(profiles.using, "combat");
        assert_eq!(profiles.used().unwrap().goals.len(), 2);
        let again = GoalProfiles::read(&profiles.write()).unwrap();
        assert_eq!(again, profiles);
    }

    #[test]
    fn changes_use_keep_and_forget_a_profile() {
        let mut profiles = GoalProfiles::default();
        profiles
            .change(Change::Keep(SkillGoals {
                name: "combat".into(),
                goals: vec![Goal {
                    skill: "Dodging".into(),
                    to: 20,
                }],
            }))
            .unwrap();
        assert!(profiles.change(Change::Use("combat".into())).is_ok());
        assert!(profiles.change(Change::Use("nope".into())).is_err());
        profiles.change(Change::Goal("Dodging".into(), 30)).unwrap();
        assert_eq!(profiles.used().unwrap().goals[0].to, 30);
        profiles.change(Change::Goal("Shield Use".into(), 5)).unwrap();
        assert_eq!(profiles.used().unwrap().goals.len(), 2);
        profiles.change(Change::Forget("combat".into())).unwrap();
        assert!(profiles.using.is_empty());
    }

    #[test]
    fn the_trainer_takes_its_goals_in_order() {
        let mut trainer = Trainer {
            goals: SkillGoals {
                name: "combat".into(),
                goals: vec![
                    Goal { skill: "Dodging".into(), to: 20 },
                    Goal { skill: "Two Weapon Combat".into(), to: 40 },
                ],
            },
            ..Trainer::default()
        };
        assert_eq!(trainer.read().len(), 2);
        assert_eq!(
            trainer.launched(),
            Some("train Two Weapon Combat".to_string())
        );
        assert_eq!(trainer.status, Status::Training);
        assert!(trainer.read()[0].doing);
        // Nothing to train stops the trainer.
        let mut empty = Trainer::default();
        assert_eq!(empty.launched(), None);
        assert!(matches!(empty.status, Status::Stopped(_)));
    }

    #[test]
    fn typed_lines_become_goal_changes() {
        assert_eq!(
            typed("train Dodging 30"),
            Some(Change::Goal("Dodging".into(), 30))
        );
        assert_eq!(
            typed("trainer Dodging"),
            Some(Change::Goal("Dodging".into(), 0))
        );
        assert_eq!(typed("look"), None);
        assert_eq!(typed("train"), None);
    }
}
