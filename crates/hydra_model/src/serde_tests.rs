//! Wire-format tests for the recovered `serde` implementations.
//!
//! Thirty `Deserialize` impls survive in the binary (one `expecting` symbol
//! each, e.g. `<impl Deserialize for psm::PsmSet>::__Visitor as
//! Visitor>::expecting @0x101b5c310`); *no* `Serialize` impl does, so the
//! tests below are one-directional: document → model. `serde_json` and `toml`
//! are dev-dependencies only.
//!
//! The documents spell the field and variant names the derived impls use —
//! i.e. the recovered field names, and the enum variant identifiers, which the
//! binary does not carry (`// UNKNOWN:`, see NOTES.md).

use crate::state::character::currency::Currency;
use crate::state::character::enhancive::{Bonus, EnhanciveTotals, Resource, Section};
use crate::state::character::gemstones::{Binding, Gemstone, Gemstones, Property};
use crate::state::character::psm::{PsmRanks, PsmSet};
use crate::state::character::skills::{Skill, SkillKind, SkillSet};
use crate::state::character::snapshot::{CharacterSnapshot, Group};
use crate::state::character::stance::Stance;
use crate::state::character::standing::{ResourceAmounts, Standing};
use crate::state::character::stats::{Identity, Stat, StatKind, StatSet, StatValue};
use crate::state::character::vocabulary::{
    AccountType, Che, DeathsSting, PsmCategory, ResourceType, Society, Warcry,
};

fn json<T: serde::de::DeserializeOwned>(text: &str) -> T {
    serde_json::from_str(text).expect("the document deserialises")
}

#[test]
fn currency_round_trips_through_json() {
    let currency: Currency = json(
        r#"{
            "silver": 1234,
            "stored": {"cloak": 1234, "locker": 900},
            "notes": 40000,
            "holder": "cloak"
        }"#,
    );
    assert_eq!(currency.silver, 1234);
    assert_eq!(currency.stored.get("locker"), Some(&900));
    assert_eq!(currency.notes, 40000);
    assert_eq!(currency.holder.as_deref(), Some("cloak"));
}

#[test]
fn stats_and_identity_round_trip_through_json() {
    let stats: StatSet = json(r#"{"stats": {"Agility": {"value": 90, "bonus": 10}}}"#);
    assert_eq!(stats.value(StatKind::Agility), 90);
    assert_eq!(
        stats.get(StatKind::Agility),
        Some(StatValue {
            value: 90,
            bonus: 10
        })
    );

    let stat: Stat = json(r#"{"kind": "Logic", "value": {"value": 70, "bonus": 5}}"#);
    assert_eq!(stat.kind, Some(StatKind::Logic));
    assert_eq!(stat.value.bonus, 5);

    let identity: Identity = json(
        r#"{"name": "Kithar", "race": "Human", "profession": "Wizard", "level": 42}"#,
    );
    assert_eq!(identity.name, "Kithar");
    assert_eq!(identity.profession, "Wizard");
    assert_eq!(identity.level, 42);
}

#[test]
fn skill_set_round_trips_through_json() {
    // a `BTreeMap<SkillKind, Skill>`: the enum keys are the variant names
    let skills: SkillSet = json(
        r#"{
            "skills": {
                "Dodging": {"rank": 8, "bonus": 4},
                "TwoWeaponCombat": {"rank": 10, "bonus": 5}
            },
            "level": 42
        }"#,
    );
    assert_eq!(skills.ranks(SkillKind::Dodging), 8);
    assert_eq!(skills.ranks(SkillKind::TwoWeaponCombat), 10);
    assert_eq!(skills.circle(), 42);
    let skill: Skill = json(r#"{"rank": 3, "bonus": 1}"#);
    assert_eq!(skill, Skill { rank: 3, bonus: 1 });
}

#[test]
fn gemstones_round_trip_through_toml() {
    let document = r#"
        [[items]]
        name = "a fiery red ruby"
        rank = 4
        binding = "Greater"
        equipped = true
        charges = 12
        properties = [{ name = "Strength", amount = 2 }]

        [[items]]
        name = "a plain stone"
        rank = 2
        binding = "None"
        equipped = false
        properties = []
    "#;
    let gemstones: Gemstones = toml::from_str(document).expect("the manifest deserialises");
    assert_eq!(gemstones.items.len(), 2);
    assert_eq!(gemstones.items[0].name, "a fiery red ruby");
    assert_eq!(gemstones.items[0].rank, 4);
    assert_eq!(gemstones.items[0].binding, Binding::Greater);
    assert!(gemstones.items[0].equipped);
    assert_eq!(gemstones.items[0].charges, Some(12));
    assert_eq!(gemstones.items[0].properties.len(), 1);
    assert_eq!(gemstones.equipped().count(), 1);
    assert_eq!(gemstones.items[1].binding, Binding::None);
    assert_eq!(gemstones.items[1].charges, None);

    let gemstone: Gemstone =
        json(r#"{"name": "an opal", "rank": 1, "binding": "Lesser", "equipped": false,
                "properties": [{"name": "Logic", "amount": 5}], "charges": null}"#);
    assert_eq!(gemstone.binding, Binding::Lesser);
    assert_eq!(
        gemstone.properties[0],
        Property {
            name: "Logic".into(),
            amount: 5
        }
    );
}

#[test]
fn standing_and_its_vocabulary_round_trip_through_json() {
    let standing: Standing = json(
        r#"{
            "society": "OrderOfVoln",
            "citizenship": "Wehnimer's Landing",
            "resources": {"weekly": 1500, "total": 4321},
            "urchins": true,
            "warcry": "For the Glory",
            "suffused": {"BountyPoints": 7}
        }"#,
    );
    assert_eq!(standing.society, Some(Society::OrderOfVoln));
    assert_eq!(standing.citizenship.as_deref(), Some("Wehnimer's Landing"));
    assert_eq!(
        standing.resources,
        ResourceAmounts {
            weekly: 1500,
            total: 4321
        }
    );
    assert_eq!(standing.urchins(), Some(true));
    assert_eq!(standing.warcry.as_deref(), Some("For the Glory"));
    assert_eq!(standing.suffused.get(&ResourceType::BountyPoints), Some(&7));

    let amounts: ResourceAmounts = json(r#"{"weekly": 1, "total": 2}"#);
    assert_eq!(amounts.weekly, 1);
    // the *display* name is not the serialised form: the derived impl keys on
    // the variant identifier (an `// UNKNOWN:` in the binary)
    assert!(serde_json::from_str::<Society>(r#""Guardians of Sunfist""#).is_err());
    let society: Society = json(r#""GuardiansOfSunfist""#);
    assert_eq!(society, Society::GuardiansOfSunfist);
    let resource: ResourceType = json(r#""CovertArtsCharges""#);
    assert_eq!(resource, ResourceType::CovertArtsCharges);
    let category: PsmCategory = json(r#""CombatManeuvers""#);
    assert_eq!(category, PsmCategory::CombatManeuvers);
    let warcry: Warcry = json(r#""For the Glory""#);
    assert_eq!(warcry, Warcry("For the Glory".into()));
    let account: AccountType = json(r#""Premium""#);
    assert_eq!(account, AccountType::Premium);
    let sting: DeathsSting = json(r#""Active""#);
    assert_eq!(sting, DeathsSting::Active);
    let che: Che = json(r#""five""#);
    assert_eq!(che, Che("five".into()));
}

#[test]
fn psm_tables_round_trip_through_json() {
    let psm: PsmSet = json(
        r#"{"categories": {"Feats": {"ranks": {"acuity": 3}}}}"#,
    );
    assert!(psm.has_table(PsmCategory::Feats));
    assert_eq!(psm.rank(PsmCategory::Feats, "acuity"), 3);
    let ranks: PsmRanks = json(r#"{"ranks": {"cheapshots": 5}}"#);
    assert_eq!(ranks.rank("cheapshots"), 5);
}

#[test]
fn enhancives_round_trip_through_json() {
    let section: Section = json(r#""Ranks""#);
    assert_eq!(section, Section::Ranks);
    let resource: Resource = json(r#""Mana""#);
    assert_eq!(resource, Resource::Mana);
    // a map with no keys needs no key deserialisation, so the `Stat(..)` variant
    // (which cannot be a JSON object key) does not get in the way
    let totals: EnhanciveTotals = json(r#"{"bonuses": {}, "ranks": {}}"#);
    assert_eq!(totals.bonus(Resource::Mana), 0);
    let bonus: Bonus = json(r#"{"amount": 3, "ranks": 1}"#);
    assert_eq!(bonus, Bonus { amount: 3, ranks: 1 });
}

#[test]
fn a_whole_character_snapshot_round_trips_through_json() {
    let document = r#"{
        "character": {
            "name": "Kithar",
            "race": "Human",
            "profession": "Wizard",
            "level": 42,
            "age": 31,
            "info": {"encum": "none", "blurb": "a weathered wizard"},
            "stats": {"stats": {"Logic": {"value": 70, "bonus": 5}}},
            "skills": {"skills": {"Dodging": {"rank": 8, "bonus": 4}}, "level": 42},
            "stance": "Neutral",
            "standing": {
                "society": null,
                "citizenship": null,
                "resources": {"weekly": 0, "total": 0},
                "urchins": null,
                "warcry": null,
                "suffused": {}
            },
            "experience": {"total": 0, "brooch": 0, "absorbed": 0, "field": 12},
            "currency": {"silver": 5, "stored": {}, "notes": 0, "holder": null},
            "gemstones": {"items": []},
            "psm": {"categories": {}},
            "enhancive": {"bonuses": {}, "ranks": {}},
            "taught": 30,
            "exist_id": 9
        },
        "group": {"leader": "Kithar", "members": ["Ael"]},
        "known": true
    }"#;
    let snapshot: CharacterSnapshot = json(document);
    assert_eq!(snapshot.character.name, "Kithar");
    assert_eq!(snapshot.character.stance, Stance::Neutral);
    assert_eq!(snapshot.character.age, Some(31));
    assert_eq!(snapshot.character.experience.field, 12);
    assert_eq!(snapshot.character.stats.value(StatKind::Logic), 70);
    assert_eq!(snapshot.character.taught, Some(30));
    assert_eq!(snapshot.character.exist_id, 9);
    assert_eq!(snapshot.group.leader.as_deref(), Some("Kithar"));
    assert_eq!(snapshot.group.members, vec!["Ael".to_string()]);
    assert!(snapshot.is_known());
    assert!(snapshot.describes("Kithar"));

    let group: Group = json(r#"{"leader": null, "members": []}"#);
    assert_eq!(group, Group::default());
}

#[test]
fn the_vitals_record_and_targetid_shown_round_trip_through_json() {
    let vital: crate::state::vitals::Vital =
        json(r#"{"kind": 1, "payload": 16909060, "extra": 9, "percent": 42}"#);
    assert_eq!(vital.kind, 1);
    assert_eq!(vital.percent, 42);
    // a newtype struct deserialises transparently, like the record itself
    let shown: crate::targetid::Shown =
        json(r#"{"kind": 1114112, "payload": 0, "extra": 0, "percent": 0}"#);
    assert_eq!(shown.0.kind, crate::state::vitals::Vital::NO_READING);
    assert!(!shown.0.is_reading());
}
