//! The settings' wire format: `serde`/`serde_json` over the recovered member
//! names.
//!
//! The names and their order are established from the field visitors
//! (`0x1019a95ac` for `CasterSettings`, `0x1019a948c` for `Options`) and the
//! `Serialize` impls (`0x101276278`); `Chosen` is the strings `cast`,
//! `channel`, `evoke` (enum deserialiser at `0x1019b49a8`).

use hydra_cast::settings::{CasterSettings, Chosen, Options};

#[test]
fn the_empty_settings_json_is_the_recorded_shape() {
    assert_eq!(
        CasterSettings::default().to_json().unwrap(),
        concat!(
            r#"{"on":false,"#,
            r#""options":{"channel":false,"conserve":false,"safety":false,"#,
            r#""stance":false,"hide":false},"#,
            r#""aliases":{},"verbs":{},"stances":{},"hide":[]}"#
        )
    );
}

#[test]
fn a_full_round_trip_keeps_every_member() {
    let mut settings = CasterSettings::default();
    settings.switch(true);
    settings.set_option("channel", "on").unwrap();
    settings.set_option("safety", "on").unwrap();
    settings.set_option("hide", "on").unwrap();
    settings.aliases.insert("zap".to_string(), 901);
    settings.verbs.insert("zap".to_string(), Chosen::Evoke);
    settings
        .stances
        .insert("901".to_string(), "guarded".to_string());
    settings.hide.insert(902);
    settings.hide.insert(903);

    let json = settings.to_json().unwrap();
    assert_eq!(
        json,
        concat!(
            r#"{"on":true,"#,
            r#""options":{"channel":true,"conserve":false,"safety":true,"#,
            r#""stance":false,"hide":true},"#,
            r#""aliases":{"zap":901},"#,
            r#""verbs":{"zap":"evoke"},"#,
            r#""stances":{"901":"guarded"},"#,
            r#""hide":[902,903]}"#
        )
    );
    assert_eq!(CasterSettings::from_json(&json).unwrap(), settings);
}

#[test]
fn chosen_is_the_word_not_the_index() {
    assert_eq!(serde_json::to_string(&Chosen::Cast).unwrap(), r#""cast""#);
    assert_eq!(
        serde_json::to_string(&Chosen::Channel).unwrap(),
        r#""channel""#
    );
    assert_eq!(serde_json::to_string(&Chosen::Evoke).unwrap(), r#""evoke""#);
    assert_eq!(
        serde_json::from_str::<Chosen>(r#""channel""#).unwrap(),
        Chosen::Channel
    );
    // 0x1019b49a8 goes through `serde::de::Error::unknown_variant`.
    let refused = serde_json::from_str::<Chosen>(r#""shout""#).unwrap_err();
    assert!(refused.to_string().contains("unknown variant"), "{refused}");
}

#[test]
fn a_missing_member_keeps_its_default() {
    // A settings file that predates an option must still load.
    assert_eq!(
        CasterSettings::from_json("{}").unwrap(),
        CasterSettings::default()
    );
    let options: Options = serde_json::from_str(r#"{"channel":true}"#).unwrap();
    assert!(options.channel);
    assert_eq!(options.safety, Options::default().safety);

    let partial = CasterSettings::from_json(r#"{"on":true,"hide":[901]}"#).unwrap();
    assert!(partial.on);
    assert_eq!(partial.hide.iter().copied().collect::<Vec<_>>(), vec![901]);
    assert!(partial.aliases.is_empty());
}
