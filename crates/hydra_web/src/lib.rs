#![allow(dead_code,unused)]
pub mod bonjour;
pub mod client;
pub mod follow;
pub mod highlights;
pub mod http;
pub mod injuries;
pub mod inventory;
pub mod notes;
pub mod pairing;
pub mod phone;
pub mod tasks;
pub mod trainer;
pub mod wire;
pub mod workspace;

/// @0x00000001002cbf3c
pub fn lan_address() { todo!() }

/// @0x00000001002cc054
pub fn one_connections() { todo!() }

/* unresolved symbols (trait impls, closures, generics):
 <hydra_web::wire::_::<impl serde_core::de::Deserialize for hydra_web::wire::In>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010026d06c
 <hydra_web::wire::_::<impl serde_core::de::Deserialize for hydra_web::wire::In>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010026d350
 <hydra_web::highlights::_::<impl serde_core::de::Deserialize for hydra_web::highlights::Form>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001002c45b0
 <hydra_web::highlights::_::<impl serde_core::de::Deserialize for hydra_web::highlights::MarkForm>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001002c45c4
 <hydra_web::highlights::_::<impl serde_core::de::Deserialize for hydra_web::highlights::Form>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001002c45d8
 <hydra_web::highlights::_::<impl serde_core::de::Deserialize for hydra_web::highlights::MarkForm>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001002c45d8
*/
