//! The auto-caster.
//!
//! Recovered from the arm64 binary at `/Users/cpmbpro/Code/hydra-inspect/hydra`;
//! every item here carries the unslid virtual address of the code or literal it
//! came from.  See `NOTES.md` for the function table, the recovered constants
//! and the list of unknowns.
//!
//! The three things the original got from other crates are real again
//! (Phase 1, see `NOTES.md` §7):
//!
//! * `hydra_model` (spells, known spells, mana, spell costs, proc lines) is
//!   behind [`game::World`] and answered by [`game::ModelWorld`];
//! * the session's command queue is behind [`driver::Sink`] and implemented by
//!   [`session::SessionSink`] over `hydra_session`;
//! * `tokio` is the driver loop ([`driver::Relay`]) and `serde`/`serde_json`
//!   are the settings' wire format ([`settings::CasterSettings`]).

#![allow(dead_code, unused)]

pub mod answers;
pub mod driver;
pub mod game;
pub mod plan;
pub mod request;
pub mod session;
pub mod settings;

/* unresolved symbols (trait impls, closures, generics):
 <hydra_cast::settings::_::<impl serde_core::de::Deserialize for hydra_cast::settings::Options>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001019b51ec
 <hydra_cast::settings::_::<impl serde_core::de::Deserialize for hydra_cast::settings::CasterSettings>::deserialize::__Visitor as serde_core::de::Visitor>::expecting @0x00000001019b5200
*/

/* local text symbols of the crate that the stub table did not list:
 hydra_cast::answers::stance                                 @0x00000001019a1de8
 hydra_cast::driver::Settings::get                           @0x00000001019a82dc
 hydra_cast::driver::Game::{idle,next,send,stance,release,
                           cast,cast_line,cast_through,
                           prepare,answer,out_of_roundtime}  @0x000000010197a58c..0x10197c128
 hydra_cast::settings::CasterSettings::which                 @0x00000001019b3b24
 hydra_cast::settings::CasterSettings::listed::{{closure}}   @0x00000001019b4270
 hydra_cast::settings::spell                                 @0x00000001019b4b14
 hydra_cast::settings::<impl Deserialize for Chosen>         @0x00000001019b49a8
 hydra_cast::settings::<impl Serialize for CasterSettings>   @0x0000000101276278
*/
