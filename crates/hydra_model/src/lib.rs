#![allow(dead_code,unused)]
pub mod crit;
pub mod movement;
pub mod pattern;
pub mod spells;
pub mod state;
pub mod status;
pub mod targetid;

/* unresolved symbols (trait impls, closures, generics):
 <hydra_model::targetid::_::<impl serde_core::de::Deserialize for hydra_model::targetid::Shown>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101be80e8
 <hydra_model::state::character::psm::_::<impl serde_core::de::Deserialize for hydra_model::state::character::psm::PsmSet>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b5c310
 <hydra_model::state::character::psm::_::<impl serde_core::de::Deserialize for hydra_model::state::character::psm::PsmRanks>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b5c324
 <hydra_model::state::character::stats::_::<impl serde_core::de::Deserialize for hydra_model::state::character::stats::Stat>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101c11be4
 <hydra_model::state::character::skills::_::<impl serde_core::de::Deserialize for hydra_model::state::character::skills::Skill>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b67eac
 <hydra_model::state::character::stats::_::<impl serde_core::de::Deserialize for hydra_model::state::character::stats::Identity>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101c11bf8
 <hydra_model::state::character::stats::_::<impl serde_core::de::Deserialize for hydra_model::state::character::stats::StatValue>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101c11c0c
 <hydra_model::state::character::skills::_::<impl serde_core::de::Deserialize for hydra_model::state::character::skills::SkillSet>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b67ec0
 <hydra_model::state::character::enhancive::_::<impl serde_core::de::Deserialize for hydra_model::state::character::enhancive::Bonus>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b67ed4
 <hydra_model::state::character::stats::_::<impl serde_core::de::Deserialize for hydra_model::state::character::stats::StatKind>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101c11c20
 <hydra_model::state::character::currency::_::<impl serde_core::de::Deserialize for hydra_model::state::character::currency::Currency>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101bf51dc
 <hydra_model::state::character::standing::_::<impl serde_core::de::Deserialize for hydra_model::state::character::standing::Standing>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b5c338
 <hydra_model::state::character::gemstones::_::<impl serde_core::de::Deserialize for hydra_model::state::character::gemstones::Gemstone>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101ba4344
 <hydra_model::state::character::gemstones::_::<impl serde_core::de::Deserialize for hydra_model::state::character::gemstones::Property>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101ba4358
 <hydra_model::state::character::skills::_::<impl serde_core::de::Deserialize for hydra_model::state::character::skills::SkillKind>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101b67ee8
 <hydra_model::state::character::snapshot::_::<impl serde_core::de::Deserialize for hydra_model::state::character::snapshot::Group>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101c11c20
 <hydra_model::state::character::gemstones::_::<impl serde_core::de::Deserialize for hydra_model::state::character::gemstones::Gemstones>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101ba436c
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::Che>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
 <hydra_model::state::character::enhancive::_::<impl serde_core::de::Deserialize for hydra_model::state::character::enhancive::Section>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101b67ee8
 <hydra_model::state::character::gemstones::_::<impl serde_core::de::Deserialize for hydra_model::state::character::gemstones::Binding>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101ba4380
 <hydra_model::state::character::enhancive::_::<impl serde_core::de::Deserialize for hydra_model::state::character::enhancive::Resource>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101b67ee8
 <hydra_model::state::character::standing::_::<impl serde_core::de::Deserialize for hydra_model::state::character::standing::ResourceAmounts>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b5c34c
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::Warcry>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::Society>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
 <hydra_model::state::character::enhancive::_::<impl serde_core::de::Deserialize for hydra_model::state::character::enhancive::EnhanciveTotals>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101b67efc
 <hydra_model::state::character::snapshot::_::<impl serde_core::de::Deserialize for hydra_model::state::character::snapshot::CharacterSnapshot>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101c11c34
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::AccountType>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::DeathsSting>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::PsmCategory>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
 <hydra_model::state::character::vocabulary::_::<impl serde_core::de::Deserialize for hydra_model::state::character::vocabulary::ResourceType>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101bb1048
*/
