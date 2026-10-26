#![allow(dead_code,unused)]
pub mod command;
pub mod coverage;
pub mod facts;
pub mod file;
pub mod route;
pub mod sail;
pub mod settings;
pub mod swim;
pub mod walk;

/* unresolved symbols (trait impls, closures, generics):
 hydra_travel::file::<impl core::convert::From<hydra_travel::file::Refused> for std::io::error::Error>::from @0x0000000101505338
 <hydra_travel::file::_::<impl serde_core::de::Deserialize for hydra_travel::file::Spot>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010150990c
 <hydra_travel::file::_::<impl serde_core::de::Deserialize for hydra_travel::file::Shared>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101509920
 <hydra_travel::walk::instability::_::<impl serde_core::de::Deserialize for hydra_travel::walk::instability::Seen>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101558cc8
*/
