//! Hydra's `egui` front end, reconstructed from the arm64 binary.
//!
//! This build has **no external crates**: `eframe`/`egui`, `image` and
//! `serde`/`serde_json` are replaced by the [`seam`] painter/ui traits, a
//! hand-written [`json`] reader/writer and a pre-decoded badge asset. Every
//! departure is recorded in `NOTES.md`, as is the evidence (unslid addresses)
//! for each recovered function.
//!
//! Original crate layout recovered from the symbol table: `app`, `badge`,
//! `bar`, `card`, `chat`, `compass_art`, `doll_art`, `hub`, `keys`, `layout`,
//! `logs`, `main_menu`, `menu`, `pad`, `placement`, `play`, `reports`,
//! `theme_editor`, `titlebar_page`, `tracked` and `widget`. Only the modules
//! that carried a stub (plus their evidenced types) are reconstructed here.

pub mod app;
pub mod badge;
pub mod bar;
pub mod chat;
pub mod json;
pub mod keys;
pub mod layout;
pub mod play;
pub mod reports;
pub mod seam;
pub mod theme_editor;
pub mod widget;
