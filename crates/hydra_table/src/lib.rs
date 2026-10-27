#![allow(dead_code,unused)]
pub mod account;
pub mod commands;
pub mod inventory_command;
pub mod json;
pub mod ledger;
pub mod lich_command;
pub mod pages;
pub mod relay;
pub mod reward_claim;
pub mod roster;
pub mod seat;
pub mod session;
pub mod store_claim;
pub mod table;
pub mod teams;
pub mod travel_command;

/* unresolved symbols (trait impls, closures, generics):
 <hydra_table::teams::_::<impl serde_core::de::Deserialize for hydra_table::teams::File>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001012148dc
 <hydra_table::teams::_::<impl serde_core::de::Deserialize for hydra_table::teams::Team>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001012148f0
 <hydra_table::roster::_::<impl serde_core::de::Deserialize for hydra_table::roster::File>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001012a7218
 <hydra_table::roster::_::<impl serde_core::de::Deserialize for hydra_table::roster::Entry>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001012a722c
*/
