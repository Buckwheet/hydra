#![allow(dead_code,unused)]
pub mod actor;
pub mod alias;
pub mod bestiary;
pub mod character_store;
pub mod claimant;
pub mod command;
pub mod lich;
pub mod lich_command;
pub mod lifecycle;
pub mod lookout_seed;
pub mod menu_store;
pub mod notes;
pub mod notice;
mod pattern;
pub mod password;
pub mod player_log;
pub mod queue;
pub mod saves;
pub mod settings_store;
pub mod steps;
pub mod store;
pub mod stream_rules;
pub mod supervisor;
mod textfmt;

/* Unresolved symbols in the original binary (trait impls, closures, generics).
   They are listed so the recovery is auditable; all of them are now covered
   here, either by hand or by the real crates documented in NOTES.md §4:

   hydra_session::notes::<impl From<notes::Refused> for std::io::Error>::from  @0x101a527c8
       -> notes::Refused + `impl From<Refused> for io::Error`               (recovered)
   <impl Deserialize for notes::File / notes::Note / notes::Clipped>::…__Visitor
       -> serde_json + textfmt::Value + notes::Notes::from_json               (Phase 1)
   <impl Deserialize for alias::AliasFile>::…__Visitor
       -> serde_json + textfmt::Value + alias::AliasFile::absorb              (Phase 1)
   <impl Deserialize for claimant::Settings>::…__Visitor
       -> claimant::Settings (a plain Option<Desk>)                           (replaced)
   <impl Deserialize for settings_store::SettingsFile>::…__Visitor
       -> a derived helper + a hand-written serde::Serialize (key order)      (Phase 1)
   <impl Deserialize for player_log::links::{LogLink, Does}>::…__Visitor
       -> `#[derive(Serialize, Deserialize)]` on both types                    (Phase 1)
   <impl Deserialize for player_log::feed::LogSettings>::…__Visitor
       -> player_log::feed::LogSettings (plain fields)                        (replaced)
   <impl Deserialize for stream_rules::Rules / Rule>::…__Visitor
       -> derived RulesDoc / RuleDoc read by `toml::from_str`                 (Phase 1)
*/
