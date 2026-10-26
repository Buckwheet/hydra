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
pub mod player_log;
pub mod queue;
pub mod saves;
pub mod settings_store;
pub mod steps;
pub mod store;
pub mod stream_rules;
pub mod supervisor;

/* unresolved symbols (trait impls, closures, generics):
 hydra_session::notes::<impl core::convert::From<hydra_session::notes::Refused> for std::io::error::Error>::from @0x0000000101a527c8
 <hydra_session::notes::_::<impl serde_core::de::Deserialize for hydra_session::notes::File>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101a54fb4
 <hydra_session::notes::_::<impl serde_core::de::Deserialize for hydra_session::notes::Note>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101a54fc8
 <hydra_session::alias::_::<impl serde_core::de::Deserialize for hydra_session::alias::AliasFile>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101ab13e0
 <hydra_session::notes::_::<impl serde_core::de::Deserialize for hydra_session::notes::File>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101a54fdc
 <hydra_session::notes::_::<impl serde_core::de::Deserialize for hydra_session::notes::Note>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101a54fdc
 <hydra_session::notes::_::<impl serde_core::de::Deserialize for hydra_session::notes::Clipped>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101a54fdc
 <hydra_session::claimant::_::<impl serde_core::de::Deserialize for hydra_session::claimant::Settings>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101a809fc
 <hydra_session::settings_store::_::<impl serde_core::de::Deserialize for hydra_session::settings_store::SettingsFile>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101a06d8c
 <hydra_session::player_log::links::_::<impl serde_core::de::Deserialize for hydra_session::player_log::links::LogLink>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101a550c0
 <hydra_session::player_log::feed::_::<impl serde_core::de::Deserialize for hydra_session::player_log::feed::LogSettings>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101a89050
 <hydra_session::player_log::links::_::<impl serde_core::de::Deserialize for hydra_session::player_log::links::Does>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101a55254
 <hydra_session::player_log::links::_::<impl serde_core::de::Deserialize for hydra_session::player_log::links::LogLink>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101a54fdc
*/
