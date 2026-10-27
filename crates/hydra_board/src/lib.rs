#![allow(dead_code, unused)]
pub mod banner;
pub mod binding;
pub mod chips;
pub mod clip;
pub mod disk;
pub mod highlights;
pub mod hotbars;
pub mod kept;
pub mod lookout;
pub mod macros;
pub mod merchants;
pub mod own;
pub mod pad;
pub mod paint;
pub mod push;
pub mod seat;
pub mod sessions;
pub mod store;
pub mod stream_rules;
#[cfg(test)]
pub(crate) mod testkit;
pub mod text;
pub mod targets;
pub mod trainer;

/* unresolved symbols (trait impls, closures, generics):
 <hydra_board::push::_::<impl serde_core::de::Deserialize for hydra_board::push::Kept>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001014adcd8
 <hydra_board::push::_::<impl serde_core::de::Deserialize for hydra_board::push::Phone>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001014adcec
 <hydra_board::chips::_::<impl serde_core::de::Deserialize for hydra_board::chips::Chip>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010147806c
 <hydra_board::push::_::<impl serde_core::de::Deserialize for hydra_board::push::Forgotten>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001014add00
 <hydra_board::chips::_::<impl serde_core::de::Deserialize for hydra_board::chips::Says>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001014782a4
 <hydra_board::chips::_::<impl serde_core::de::Deserialize for hydra_board::chips::Empty>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001014782a4
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::Part>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101483454
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::Threshold>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101483468
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::StatusLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010148347c
 <hydra_board::trainer::_::<impl serde_core::de::Deserialize for hydra_board::trainer::GoalProfiles>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001014add14
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::NameShown>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101483454
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::Threshold>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101483578
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::StatusLook>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101483578
 <hydra_board::targets::_::<impl serde_core::de::Deserialize for hydra_board::targets::TargetsLook>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101483578
*/
