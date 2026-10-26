#![allow(dead_code,unused)]
pub mod app;
pub mod badge;
pub mod bar;
pub mod chat;
pub mod keys;
pub mod layout;
pub mod play;
pub mod reports;
pub mod theme_editor;
pub mod widget;

/* unresolved symbols (trait impls, closures, generics):
 <hydra_gui::play::titlebar::TitleBar as core::convert::From<hydra_gui::play::titlebar::Written>>::from @0x00000001003c6b98
 <hydra_gui::bar::_::<impl serde_core::de::Deserialize for hydra_gui::bar::Look>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001006d1978
 <hydra_gui::bar::_::<impl serde_core::de::Deserialize for hydra_gui::bar::Says>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001006d198c
 <hydra_gui::card::_::<impl serde_core::de::Deserialize for hydra_gui::card::Kept>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001004882b8
 <hydra_gui::chat::_::<impl serde_core::de::Deserialize for hydra_gui::chat::Kept>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000100679684
 <hydra_gui::layout::_::<impl serde_core::de::Deserialize for hydra_gui::layout::Holder>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c6d34
 <hydra_gui::layout::_::<impl serde_core::de::Deserialize for hydra_gui::layout::Layout>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c6d48
 <hydra_gui::layout::_::<impl serde_core::de::Deserialize for hydra_gui::layout::Placed>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c6d5c
 <hydra_gui::tracked::_::<impl serde_core::de::Deserialize for hydra_gui::tracked::Tracked>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010054419c
 <hydra_gui::placement::_::<impl serde_core::de::Deserialize for hydra_gui::placement::Place>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001004882cc
 <hydra_gui::logs::view::_::<impl serde_core::de::Deserialize for hydra_gui::logs::view::Show>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001005441b0
 <hydra_gui::logs::view::_::<impl serde_core::de::Deserialize for hydra_gui::logs::view::View>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001005441c4
 <hydra_gui::tracked::_::<impl serde_core::de::Deserialize for hydra_gui::tracked::TrackedEffects>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001005441d8
 <hydra_gui::widget::lines::_::<impl serde_core::de::Deserialize for hydra_gui::widget::lines::Lines>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001006d2344
 <hydra_gui::layout::custom::_::<impl serde_core::de::Deserialize for hydra_gui::layout::custom::Cell>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c76c0
 <hydra_gui::layout::preset::_::<impl serde_core::de::Deserialize for hydra_gui::layout::preset::File>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000100488430
 <hydra_gui::widget::doll::_::<impl serde_core::de::Deserialize for hydra_gui::widget::doll::DollLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001006797c0
 <hydra_gui::widget::quick::_::<impl serde_core::de::Deserialize for hydra_gui::widget::quick::Picked>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a745c
 <hydra_gui::compass_art::_::<impl serde_core::de::Deserialize for hydra_gui::compass_art::CompassLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c76d4
 <hydra_gui::layout::custom::_::<impl serde_core::de::Deserialize for hydra_gui::layout::custom::Custom>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c76e8
 <hydra_gui::layout::preset::_::<impl serde_core::de::Deserialize for hydra_gui::layout::preset::Preset>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000100488444
 <hydra_gui::play::titlebar::_::<impl serde_core::de::Deserialize for hydra_gui::play::titlebar::Written>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001003c76fc
 <hydra_gui::widget::quick::_::<impl serde_core::de::Deserialize for hydra_gui::widget::quick::QuickLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a83ac
 <hydra_gui::layout::drawers::_::<impl serde_core::de::Deserialize for hydra_gui::layout::drawers::Drawer>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a83c0
 <hydra_gui::layout::drawers::_::<impl serde_core::de::Deserialize for hydra_gui::layout::drawers::Drawers>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a83d4
 <hydra_gui::widget::effects::_::<impl serde_core::de::Deserialize for hydra_gui::widget::effects::OwnRows>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a83e8
 <hydra_gui::widget::labels::_::<impl serde_core::de::Deserialize for hydra_gui::widget::labels::LabelLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000100679b60
 <hydra_gui::widget::hotbar::_::<impl serde_core::de::Deserialize for hydra_gui::widget::hotbar::HotbarLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010054434c
 <hydra_gui::widget::roster::_::<impl serde_core::de::Deserialize for hydra_gui::widget::roster::RosterLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a857c
 <hydra_gui::widget::effects::_::<impl serde_core::de::Deserialize for hydra_gui::widget::effects::EffectRows>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a86d0
 <hydra_gui::widget::effects::_::<impl serde_core::de::Deserialize for hydra_gui::widget::effects::TimersLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a86e4
 <hydra_gui::widget::effects::_::<impl serde_core::de::Deserialize for hydra_gui::widget::effects::EveryEffects>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007a86f8
 <hydra_gui::widget::described::_::<impl serde_core::de::Deserialize for hydra_gui::widget::described::RoomParts>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000100544360
 <hydra_gui::widget::status::look::_::<impl serde_core::de::Deserialize for hydra_gui::widget::status::look::IndicatorLook>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007f4c74
 <hydra_gui::widget::status::look::_::<impl serde_core::de::Deserialize for hydra_gui::widget::status::look::EveryIndicator>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007f4c88
 <hydra_gui::widget::custom_indicator::_::<impl serde_core::de::Deserialize for hydra_gui::widget::custom_indicator::CustomIndicator>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001007f4d98
*/
