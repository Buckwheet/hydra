//! The real model: `game::ModelWorld` over `hydra_model`.
//!
//! The spell table itself has no embedded data — `hydra_model::spells` reads it
//! from a generated file through `set_spells` — so these tests fill the table
//! with two rows and check that the adapter the crate installs by default
//! answers from it.

use hydra_cast::game::{self, ModelWorld, World};
use hydra_cast::plan::{self, Cast};
use hydra_cast::settings::{spell, CasterSettings};

use hydra_model::spells::{self, Role, Spell};
use hydra_model::state::known_spells::KnownSpells;

const SHOCK: u16 = 101;
const FIREBALL: u16 = 202;
const UNKNOWN: u16 = 303;

fn table() -> Vec<Spell> {
    let shock = Spell::new("Minor Shock", 2.0, SHOCK, "^A tiny bolt of lightning");
    let fireball = Spell {
        role: Some(Role::Attack),
        ..Spell::new("Fireball", 3.0, FIREBALL, "^A ball of fire")
    };
    vec![shock, fireball]
}

fn known() -> &'static KnownSpells {
    let mut known = KnownSpells::default();
    // The recovered model keeps the set lower-cased, by name.
    known.names.insert("minor shock".to_string());
    known.names.insert("fireball".to_string());
    Box::leak(Box::new(known))
}

/// Fill the spell table and install the adapter.  Both are process-wide and
/// one-shot (the seams are `OnceLock`s), so every test calls this first and no
/// test depends on another one having run.
fn setup() {
    let _ = spells::set_spells(table());
    let _ = game::install_model_with(known());
}

#[test]
fn the_real_model_answers_real_spells() {
    setup();

    // The crate's default model is the hydra_model adapter now.
    let model = game::world_model();
    assert_eq!(model.spell_name(FIREBALL), Some("Fireball"));
    assert_eq!(model.spell_name(UNKNOWN), None);
    assert!(model.knows(SHOCK));
    // Not in the known-spell set: refused.
    let forgotten: &'static KnownSpells = Box::leak(Box::new(KnownSpells::default()));
    assert!(!ModelWorld::new(Some(forgotten)).knows(SHOCK));
    // `Role::Attack` is the recovered kind 0 (`game::KIND_ATTACK`).
    assert_eq!(game::KIND_ATTACK, Role::Attack.index());
    assert!(model.is_attack(FIREBALL));
    assert!(!model.is_attack(SHOCK));
    // The cast duration is the row's own `f64`.
    assert_eq!(model.seconds(FIREBALL), Some(3.0));
    assert_eq!(model.spell_cost(FIREBALL, "mana"), None);

    // `settings::spell` (0x1019b4b14) now goes through the real table.
    assert_eq!(spell(" 101 "), Ok(SHOCK));
    assert_eq!(
        spell("303"),
        Err("`303` is not a spell number.".to_string())
    );

    // And a whole plan builds on it.
    let made = plan::plan(
        &Cast::at(FIREBALL, "kobold"),
        &CasterSettings::default(),
        model,
    )
    .expect("the model knows fireball");
    assert_eq!(
        made.lines,
        vec!["prepare Fireball", "target kobold", "cast kobold"]
    );
    assert_eq!(made.seconds, 3.0);

    // Whatever this test did, the seams are one-shot: a second install is
    // refused, exactly as the `OnceLock` seam always refused one.
    assert!(!game::install_model());
    assert!(!game::install_model_with(known()));
}

#[test]
fn a_model_without_known_spells_refuses_every_cast() {
    setup();
    // No global state: a bare adapter over the installed table.
    let model = ModelWorld::new(None);
    assert!(model.spell_name(SHOCK).is_some(), "the table is global");
    assert!(!model.knows(SHOCK), "no known-spell set: do not act");
    // 0x103e7b521 "You do not know {}."
    assert_eq!(
        plan::plan(&Cast::new(SHOCK), &CasterSettings::default(), &model),
        Err("You do not know Minor Shock.".to_string())
    );
    // 0x103e7b536 "{} is not a spell."
    assert_eq!(
        plan::plan(&Cast::new(UNKNOWN), &CasterSettings::default(), &model),
        Err("303 is not a spell.".to_string())
    );
}
