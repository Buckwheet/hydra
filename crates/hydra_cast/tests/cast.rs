//! Behavioural tests for `hydra_cast`.
//!
//! Every expected string is a literal recovered from the binary (the address is
//! quoted next to it).  The spell table below is a fixture: the real one lives
//! in `hydra_model`, which is being recovered separately, so the numbers here
//! are made up and only the *messages* are evidence.

use hydra_cast::answers::{self, Answer};
use hydra_cast::game::{self, World};
use hydra_cast::plan::{self, Cast};
use hydra_cast::request::{self, Request};
use hydra_cast::settings::{spell, CasterSettings, Chosen, STANCES};

/// A tiny stand-in for `hydra_model`: three spells, one of them an attack.
struct Fixture;

const SHOCK: u16 = 901;
const BALL: u16 = 902;
const UNKNOWN_NUMBER: u16 = 999;
const HIDDEN: u16 = 903;

impl World for Fixture {
    fn spell_name(&self, number: u16) -> Option<&str> {
        match number {
            SHOCK | HIDDEN => Some("shock"),
            BALL => Some("ball"),
            _ => None,
        }
    }

    fn knows(&self, number: u16) -> bool {
        number != HIDDEN
    }

    fn is_attack(&self, number: u16) -> bool {
        number == BALL
    }

    fn mana(&self) -> Option<i64> {
        Some(10)
    }

    fn spell_cost(&self, number: u16, resource: &str) -> Option<(f64, i64)> {
        match (number, resource) {
            (SHOCK, "mana") => Some((1.0, 3)),
            (SHOCK, "second") => Some((1.0, 2)),
            (BALL, "mana") => Some((1.0, 40)),
            _ => None,
        }
    }

    fn has_live_target(&self, target: &str) -> bool {
        target == "kobold"
    }
}

static FIXTURE: Fixture = Fixture;

fn settings() -> CasterSettings {
    // Install once; a second install is a no-op, which is fine for tests.
    game::install(&FIXTURE);
    CasterSettings::default()
}

#[test]
fn spell_number_parsing() {
    // 0x1019b4b14: trim, `u16`, then the game's spell table.
    assert_eq!(spell(" 901 "), Ok(SHOCK));
    assert_eq!(spell("+901"), Ok(SHOCK));
    assert_eq!(
        spell("999"),
        Err("`999` is not a spell number.".to_string())
    );
    // The message keeps the argument untrimmed.
    assert_eq!(
        spell(" zzz "),
        Err("` zzz ` is not a spell number.".to_string())
    );
    assert!(spell("-901").is_err());
    assert!(spell("").is_err());
    assert!(spell("65536").is_err());
}

#[test]
fn set_option_names_and_states() {
    let mut s = settings();
    // 0x103e62c3e "{} is {}." with 0x1039c49b8/0x1039c49ba.
    assert_eq!(
        s.set_option("channel", "on"),
        Ok("channel is on.".to_string())
    );
    assert!(s.options.channel);
    assert_eq!(
        s.set_option("HIDE", "OFF"),
        Ok("hide is off.".to_string())
    );
    assert!(!s.options.hide);
    assert_eq!(
        s.set_option("conserve", "yes"),
        Err("`yes` is not on or off.".to_string())
    );
    // 0x103e7b5c9, with the lower-cased option name.
    assert_eq!(
        s.set_option("Speed", "on"),
        Err("`speed` is not an option: channel, conserve, safety, stance or hide.".to_string())
    );
}

#[test]
fn aliases_are_one_word_of_letters_and_digits() {
    let mut s = settings();
    assert_eq!(
        s.alias("901", " Zap "),
        Ok("\nThe alias zap casts spell 901.".to_string())
    );
    // 0x103e7b708, replacing an alias that pointed at another spell.
    assert_eq!(
        s.alias("902", "zap"),
        Ok("\nThe alias zap was spell 901; it is now 902.".to_string())
    );
    assert_eq!(s.aliases.get("zap"), Some(&BALL));
    // 0x1039c4a21 and 0x1039c4a4c are fixed messages.
    assert_eq!(
        s.alias("901", "two words"),
        Err("An alias is one word of letters and digits.".to_string())
    );
    assert_eq!(
        s.alias("901", "901"),
        Err("An alias is not a number: that is a spell's own.".to_string())
    );
    // An alias name may contain an underscore.
    assert!(s.alias("901", "my_zap").is_ok());
}

#[test]
fn clearing_an_alias() {
    let mut s = settings();
    assert_eq!(
        s.clear_alias("ghost"),
        Err("There is no alias ghost.".to_string())
    );
    s.alias("901", "zap").unwrap();
    assert_eq!(
        s.clear_alias(" ZAP "),
        Ok("The alias zap is taken away.".to_string())
    );
    assert!(s.aliases.is_empty());
}

#[test]
fn an_alias_resolves_for_verbs_and_stances() {
    let mut s = settings();
    s.alias("901", "zap").unwrap();
    // `which` prefers an alias, so the entry is keyed by the alias name.
    assert_eq!(
        s.set_verb("zap", "evoke"),
        Ok("zap is cast with evoke.".to_string())
    );
    assert_eq!(s.verb(SHOCK, Some("zap")), Some(Chosen::Evoke));
    // The number is the fallback key.
    assert_eq!(
        s.set_stance("901", "guarded"),
        Ok("901 is cast from guarded.".to_string())
    );
    assert_eq!(s.stance(SHOCK, None), Some("guarded"));
}

#[test]
fn verbs_cast_channel_evoke_and_clear() {
    let mut s = settings();
    // 0x103e7b824 / 0x103e7b801 use the words from the Chosen table.
    assert_eq!(
        s.set_verb("901", "CHANNEL"),
        Ok("901 is cast with channel.".to_string())
    );
    assert_eq!(
        s.set_verb("901", "cast"),
        Ok("901 was cast with channel; it is now cast.".to_string())
    );
    // 0x103e7b7e6 and 0x103e7b7cb.
    assert_eq!(
        s.set_verb("901", "clear"),
        Ok("901 is cast as it would be.".to_string())
    );
    assert_eq!(
        s.set_verb("901", "clear"),
        Ok("901 had no verb of its own.".to_string())
    );
    // 0x103e7b838 with the argument as it was typed.
    assert_eq!(
        s.set_verb("901", "Shout"),
        Err("`Shout` is not a verb: cast, channel or evoke.".to_string())
    );
}

#[test]
fn stances_are_the_six_the_game_has() {
    let mut s = settings();
    assert_eq!(
        s.set_stance("901", "Defensive"),
        Ok("901 is cast from defensive.".to_string())
    );
    // 0x103e7b661, then 0x103e7b629 for the clear.
    assert_eq!(
        s.set_stance("901", "advance"),
        Ok("901 was cast from defensive; it is now cast from advance.".to_string())
    );
    assert_eq!(
        s.set_stance("901", "clear"),
        Ok("901 has no stance of its own.".to_string())
    );
    assert_eq!(
        s.set_stance("901", "clear"),
        Ok("901 had no stance of its own.".to_string())
    );
    // 0x103e7b646 with the six stances joined by ", " in table order.
    assert_eq!(
        s.set_stance("901", "berserk"),
        Err(format!(
            "`berserk` is not a stance: {}.",
            STANCES.join(", ")
        ))
    );
    assert_eq!(
        STANCES.join(", "),
        "offensive, advance, forward, neutral, guarded, defensive"
    );
}

#[test]
fn hide_needs_the_hide_option_and_toggles() {
    let mut s = settings();
    // 0x1039c49f8: the hint is a 41-byte literal with a leading space.
    assert_eq!(
        s.toggle_hide("901"),
        Ok(
            "901 is on the hide list. The hide option is off: .ec set hide on."
                .to_string()
        )
    );
    assert!(s.hide.contains(&SHOCK));
    s.set_option("hide", "on").unwrap();
    assert_eq!(
        s.toggle_hide("901"),
        Ok("901 is off the hide list.".to_string())
    );
    assert_eq!(s.toggle_hide("901"), Ok("901 is on the hide list.".to_string()));
}

#[test]
fn switch_names_the_typed_spell_mode() {
    let mut s = settings();
    // 0x1039c4aa4 and 0x1039c4a80.
    assert_eq!(s.switch(true), "Typed spells are cast.".to_string());
    assert!(s.on);
    assert_eq!(
        s.switch(false),
        "Typed spells are sent as typed.".to_string()
    );
    assert!(!s.on);
}

#[test]
fn chosen_names_and_words() {
    assert_eq!(Chosen::named("CAST"), Some(Chosen::Cast));
    assert_eq!(Chosen::named("Channel"), Some(Chosen::Channel));
    assert_eq!(Chosen::named("evoke"), Some(Chosen::Evoke));
    assert_eq!(Chosen::named(""), None);
    assert_eq!(Chosen::named("shout"), None);
    assert_eq!(Chosen::Channel.word(), "channel");
    assert_eq!(Chosen::Evoke.as_index(), 2);
}

#[test]
fn listed_prints_the_six_lines() {
    let mut s = settings();
    let empty = s.listed();
    assert_eq!(
        empty,
        vec![
            "Typed spells cast: off".to_string(),
            "Options: channel off, conserve off, safety off, stance off, hide off".to_string(),
            "Aliases: none".to_string(),
            "Verbs: none".to_string(),
            "Stances: none".to_string(),
            "Hide after: none".to_string(),
        ]
    );
    s.switch(true);
    s.set_option("safety", "on").unwrap();
    s.alias("901", "zap").unwrap();
    s.set_verb("901", "evoke").unwrap();
    s.set_stance("901", "guarded").unwrap();
    s.hide.insert(902);
    s.hide.insert(903);
    assert_eq!(
        s.listed(),
        vec![
            "Typed spells cast: on".to_string(),
            "Options: channel off, conserve off, safety on, stance off, hide off".to_string(),
            "Aliases: zap 901".to_string(),
            "Verbs: 901 evoke".to_string(),
            "Stances: 901 guarded".to_string(),
            "Hide after: 902, 903".to_string(),
        ]
    );
}

#[test]
fn plan_refuses_and_plans() {
    let s = settings();

    // 0x103e7b536 "{} is not a spell."
    assert_eq!(
        plan::plan(&Cast::new(UNKNOWN_NUMBER), &s, &FIXTURE),
        Err("999 is not a spell.".to_string())
    );
    // 0x103e7b521 "You do not know {}."
    assert_eq!(
        plan::plan(&Cast::new(HIDDEN), &s, &FIXTURE),
        Err("You do not know shock.".to_string())
    );

    // Plain cast: prepare, then cast.
    let made = plan::plan(&Cast::new(SHOCK), &s, &FIXTURE).unwrap();
    assert_eq!(made.lines, vec!["prepare shock", "cast"]);
    assert_eq!(made.seconds, 2.0);
    assert_eq!(made.stance, None);

    // At a target: prepare, target, cast.
    let at = plan::plan(&Cast::at(SHOCK, "kobold"), &s, &FIXTURE).unwrap();
    assert_eq!(
        at.lines,
        vec!["prepare shock", "target kobold", "cast kobold"]
    );

    // A channelled spell uses the incant form from the suffix table.
    let mut channelled = settings_with_verb(SHOCK, "channel");
    let made = plan::plan(&Cast::new(SHOCK), &channelled, &FIXTURE).unwrap();
    assert_eq!(made.lines, vec!["incant shockchannel", "cast"]);
    channelled.verbs.clear();
}

fn settings_with_verb(number: u16, verb: &str) -> CasterSettings {
    let mut s = settings();
    s.set_verb(&number.to_string(), verb).unwrap();
    s
}

#[test]
fn plan_honours_conserve_safety_and_stance() {
    let mut s = settings();
    s.set_option("conserve", "on").unwrap();
    // 0x103e7b4c4: 40 mana against the 10 the fixture has.
    assert_eq!(
        plan::plan(&Cast::new(BALL), &s, &FIXTURE),
        Err("Not enough mana to cast ball.".to_string())
    );

    s.set_option("conserve", "off").unwrap();
    s.set_option("safety", "on").unwrap();
    // 0x103e7b4e1 and 0x103e7b4fd.
    assert_eq!(
        plan::plan(&Cast::new(BALL), &s, &FIXTURE),
        Err("No live target here is ball.".to_string())
    );
    assert_eq!(
        plan::plan(&Cast::at(BALL, "rat"), &s, &FIXTURE),
        Err("No live target here to cast ball at.".to_string())
    );

    s.set_option("stance", "on").unwrap();
    let made = plan::plan(&Cast::at(BALL, "kobold"), &s, &FIXTURE).unwrap();
    assert_eq!(made.stance, Some("offensive"));
}

#[test]
fn answers_and_requests() {
    assert_eq!(
        answers::answer("You can only evoke certain spells."),
        Some(Answer::OnlyEvoke)
    );
    assert!(answers::stance("You are now in a guarded stance."));
    assert_eq!(request::request("cast 901 kobold").unwrap().spell, 901);
    assert_eq!(
        request::request("incant 901 evoke").unwrap().verb,
        Some(Chosen::Evoke)
    );
    assert_eq!(request::request("look"), None);
    assert!(request::blocked("Your guarded stance keeps the spell from working."));
    let _: Request = Request::new(SHOCK);
}
