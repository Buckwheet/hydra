#![allow(dead_code,unused)]
pub mod actor;
pub mod alias;
pub mod bestiary;
pub mod character_store;
pub mod claimant;
pub mod command;
pub mod lich;
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
   here, either by hand or by the std-only replacements documented in NOTES.md:

   hydra_session::notes::<impl From<notes::Refused> for std::io::Error>::from  @0x101a527c8
       -> notes::Refused + `impl From<Refused> for io::Error`               (recovered)
   <impl Deserialize for notes::File / notes::Note / notes::Clipped>::…__Visitor
       -> textfmt::parse_json + notes::Notes::from_json                       (replaced)
   <impl Deserialize for alias::AliasFile>::…__Visitor
       -> textfmt::parse_json + alias::AliasFile::absorb                      (replaced)
   <impl Deserialize for claimant::Settings>::…__Visitor
       -> claimant::Settings (a plain Option<Desk>)                           (replaced)
   <impl Deserialize for settings_store::SettingsFile>::…__Visitor
       -> textfmt::parse_json + settings_store::SettingsFile::from_json       (replaced)
   <impl Deserialize for player_log::links::{LogLink, Does}>::…__Visitor
       -> textfmt::parse_json + player_log::links::{read, Does::of}            (replaced)
   <impl Deserialize for player_log::feed::LogSettings>::…__Visitor
       -> player_log::feed::LogSettings (plain fields)                        (replaced)
*/
