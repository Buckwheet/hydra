#![allow(dead_code,unused)]
pub mod bounty;
pub mod channel_line;
pub mod combat_line;
pub mod condition;
pub mod creature_line;
pub mod emoji;
pub mod frame;
pub mod highlight;
pub mod history;
pub mod hotbar;
pub mod injuries;
pub mod input;
pub mod inventory;
pub mod lookout_line;
pub mod macros;
pub mod merge;
pub mod object_menu;
pub mod perf;
pub mod picture;
pub mod run;
pub mod settings;
pub mod sorter;
pub mod story;
pub mod theme;
pub mod view;

/* unresolved symbols (trait impls, closures, generics):
 <hydra_ui::hotbar::icon::Icon as core::convert::TryFrom<hydra_ui::hotbar::icon::IconDe>>::try_from @0x00000001018cace8
 <hydra_ui::run::_::<impl serde_core::de::Deserialize for hydra_ui::run::RunLink>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101955370
 <hydra_ui::history::_::<impl serde_core::de::Deserialize for hydra_ui::history::Kept>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101929974
 <hydra_ui::run::_::<impl serde_core::de::Deserialize for hydra_ui::run::RunLink>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001019553ac
 <hydra_ui::hub::_::<impl serde_core::de::Deserialize for hydra_ui::hub::TeamMode>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101929988
 <hydra_ui::sorter::_::<impl serde_core::de::Deserialize for hydra_ui::sorter::Sorting>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010184c51c
 <hydra_ui::hotbar::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::Bar>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::hotbar::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::Style>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::frame::_::<impl serde_core::de::Deserialize for hydra_ui::frame::Measures>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::hotbar::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::Button>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::Cmp>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::hotbar::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::Countdown>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cbbd8
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::Condition>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001018351f0
 <hydra_ui::hotbar::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::HotbarFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::hotbar::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::ButtonState>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::theme::file::_::<impl serde_core::de::Deserialize for hydra_ui::theme::file::Chosen>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x000000010194392c
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::IconDe>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001018cbec0
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::DayPhase>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::HandSlot>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::theme::file::_::<impl serde_core::de::Deserialize for hydra_ui::theme::file::File>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::Condition>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101836cd0
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::NameMatch>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::VitalKind>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::VitalUnit>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::theme::file::_::<impl serde_core::de::Deserialize for hydra_ui::theme::file::Chosen>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::RefDe>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cbbd8
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::Sheet>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::story::stamp::_::<impl serde_core::de::Deserialize for hydra_ui::story::stamp::Hours>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ead4
 <hydra_ui::theme::typeface::_::<impl serde_core::de::Deserialize for hydra_ui::theme::typeface::Face>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101943b00
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::ResourceKind>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::IconDe>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::Sheets>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cb1e8
 <hydra_ui::theme::shape::_::<impl serde_core::de::Deserialize for hydra_ui::theme::shape::Buttons>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ead4
 <hydra_ui::theme::shape::_::<impl serde_core::de::Deserialize for hydra_ui::theme::shape::Density>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ead4
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::EffectCategory>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::condition::_::<impl serde_core::de::Deserialize for hydra_ui::condition::TargetKindName>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101834ce4
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::IconMode>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cbbd8
 <hydra_ui::theme::file::_::<impl serde_core::de::Deserialize for hydra_ui::theme::file::PromptFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::theme::file::_::<impl serde_core::de::Deserialize for hydra_ui::theme::file::RecipeFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::theme::shape::_::<impl serde_core::de::Deserialize for hydra_ui::theme::shape::TabStyle>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ead4
 <hydra_ui::theme::shape::_::<impl serde_core::de::Deserialize for hydra_ui::theme::shape::ShapeFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ed3c
 <hydra_ui::highlight::file::_::<impl serde_core::de::Deserialize for hydra_ui::highlight::file::Mark>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ed3c
 <hydra_ui::highlight::file::_::<impl serde_core::de::Deserialize for hydra_ui::highlight::file::Rule>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ed3c
 <hydra_ui::theme::shape::_::<impl serde_core::de::Deserialize for hydra_ui::theme::shape::TitleAlign>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ead4
 <hydra_ui::theme::typeface::_::<impl serde_core::de::Deserialize for hydra_ui::theme::typeface::Face>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::hotbar::icon::_::<impl serde_core::de::Deserialize for hydra_ui::hotbar::icon::GradientDir>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x00000001018cbbd8
 <hydra_ui::theme::shape::_::<impl serde_core::de::Deserialize for hydra_ui::theme::shape::CornersFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ed3c
 <hydra_ui::theme::typeface::_::<impl serde_core::de::Deserialize for hydra_ui::theme::typeface::TypeFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
 <hydra_ui::story::scrollback::_::<impl serde_core::de::Deserialize for hydra_ui::story::scrollback::Scrollback>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x0000000101943f0c
 <hydra_ui::highlight::file::_::<impl serde_core::de::Deserialize for hydra_ui::highlight::file::HighlightFile>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x000000010183ed3c
 <hydra_ui::story::scrollback::_::<impl serde_core::de::Deserialize for hydra_ui::story::scrollback::Scrollback>::deserialize::__FieldVisitor as serde_core::de::Visitor>::expecting @0x0000000101943918
*/
