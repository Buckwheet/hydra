#![allow(dead_code, unused)]
//! `gs_puppet` — the 3-D puppet rig behind the Hydra front end.
//!
//! Reconstructed from the stripped arm64 Mach-O binary of Hydra
//! (`hydra/0.1.0-beta.7`).  There is no DWARF, so every signature, field offset
//! and constant below was read out of the instruction stream.  The original's
//! `glam` / `wgpu` / `toml` / `serde` usage is replaced by hand-written
//! equivalents, except where the data it drove is expressed as `epaint` values
//! instead (see below).  `NOTES.md` next to this file records which functions are
//! established, which are inferred, and what could not be recovered at all.
//!
//! # The `epaint` seams
//!
//! The binary has **no** `gs_puppet` -> `egui` call edge (docs/EGUI.md §3.6): this
//! crate never drew widgets.  What it did do was hand draw *data* to `gs_render`'s
//! GPU passes, and those seams now emit `epaint` painting values as well, from the
//! geometry the embedded WGSL pins down:
//!
//! | seam | `epaint` producer |
//! |---|---|
//! | `splat::render` (projected splats) | [`splat::Footprint`], [`splat::mesh`], [`splat::solid_mesh`], [`splat::prepared_mesh`] |
//! | `draw::Posed::draw_list` (limb quads) | [`draw::DrawList::shapes`], [`draw::Posed::shapes`] |
//! | `draw::Posed::ground_shadow` | [`draw::Posed::shadow_shapes`] |
//! | `shadow::ShadowCaster` | [`shadow::ShadowCaster::shapes`] |
//! | `skin::SkinSurface` (the lit skin) | [`skin::SkinSurface::color_image`] |
//! | `uv::Texture` (the atlas) | [`uv::Texture::color_image`] |
//!
//! Module map (the recovered `gs_puppet` namespace):
//!
//! | module | role |
//! |---|---|
//! | [`math`] | `Vec3` / `Mat3` / `Quat` / `Xf`, slerp, Euler convention |
//! | [`model`] | [`model::Puppet`] and its [`model::Segment`] tree |
//! | [`pose`] | poses, limits, idle breathing, flinch envelope, FK |
//! | [`joints`] | joint sharing / ring transforms |
//! | [`splat`] | Gaussian-splat clouds (`.spz` / `.ply`), [`splat::SplatParams`] |
//! | [`mesh`] | OBJ/frame meshes, skin weights, splat binding |
//! | [`uv`] | UV atlas layout and texture sampling |
//! | [`camera`] | the tiny yaw/pitch projector |
//! | [`draw`] | 2-D draw primitives produced from a posed rig |
//! | [`skin`] | skin surface / mesh building and scene lighting |
//! | [`shadow`] | ground-shadow casting |
//! | [`layers`] | status-driven particle layers |
//! | [`fall`] | ragdoll fall poses |
//! | [`hang`] | which bones hang from a held object |
//! | [`baked`] | baked (pre-computed) puppets from TOML |
//!
//! [`splat::SplatParams`] and the [`pose`] / [`model`] / [`math`] types are the
//! public seam `gs_render` consumes (`gs_render::puppet_cache::SplatKey` has a
//! `From<gs_puppet::splat::SplatParams>` impl).

pub mod baked;
pub mod camera;
pub mod draw;
pub mod fall;
pub mod hang;
pub mod joints;
pub mod layers;
pub mod math;
pub mod mesh;
pub mod model;
pub mod pose;
pub mod shadow;
pub mod skin;
pub mod splat;
/// The TOML subset reader the shipped pose files need (**std departure**: the
/// original used `toml` + `serde`).
pub mod toml;
pub mod uv;
