//! The eframe entry point — recovered as a run plan, not an event loop.
//!
//! `hydra_gui::app::run` @`0x100512f78` is **partly** recovered. The ABI is
//! seven arguments plus an `sret` (`x8`) and a 72-byte return value, and the
//! body does, in order:
//!
//! | what | evidence |
//! |---|---|
//! | load the per-character placements from a `&Path` | `Placements::load(x3, x4)` @`0x100512fd0` |
//! | build the viewport for the placement named **`hub`** | `Placements::builder(.., "hub", 3, ..)` @`0x100512ff0`, literal at `0x10207ecc3` |
//! | title the window **`Hydra`** | a 5-byte `String` written byte by byte: `mov w8, #0x7948; movk w8, #0x7264, lsl #16; strb w8=0x61` @`0x100513014` |
//! | take the graphics folder | `hydra_board::own::graphics(&Path)` @`0x1005130a4` |
//! | `egui_wgpu::WgpuConfiguration::default()` | @`0x1005130ac` |
//! | `eframe::epi::NativeOptions::default()` | @`0x1005130b8` |
//! | box a 112-byte app value and call `eframe::run_native("Hydra", 5, &options, app, vtable, sret)` | @`0x1005132a4` |
//! | wait for the seat to close, then copy the `Result` out | `hydra_board::seat::Wake::closed` @`0x1005132ac` |
//!
//! `eframe` cannot exist here, so [`run`] stops after building the same plan and
//! returns it. Nothing is guessed about the event loop.

use crate::seam::Vec2;

/// The window title: `"Hydra"`, from the 5-byte `String` built at
/// `0x100513014` (`0x79 0x48` = `"Hy"`, `0x7264` = `"dr"`, `0x61` = `"a"`) and
/// passed to `eframe::run_native` as `(&str, len = 5)` @`0x1005132a0`.
pub const TITLE: &str = "Hydra";

/// The placement profile the window is built from: `"hub"` (3 bytes at
/// `0x10207ecc3`, length in `w2` @`0x100512fe4`).
pub const PLACEMENT: &str = "hub";

/// The renderer the original asked eframe for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Renderer {
    /// `egui_wgpu::WgpuConfiguration::default()` is built at `0x1005130ac`, and
    /// `eframe`'s default renderer in this build is the wgpu one.
    #[default]
    Wgpu,
    Glow,
}

/// What `app::run` hands to the window system.
#[derive(Clone, Debug, PartialEq)]
pub struct RunPlan {
    /// [`TITLE`].
    pub title: String,
    /// [`PLACEMENT`].
    pub placement: String,
    /// The size the placement builder was given.
    ///
    /// `mov x3, #0x44250000; movk x3, #0x4402, lsl #48` @`0x100512fe8` packs two
    /// `f32`s into one register: `0x44250000` = `660.0` and `0x44020000` =
    /// `520.0`.
    // UNKNOWN: which of the two is the width; `660 × 520` is assumed.
    pub size: Vec2,
    /// The graphics folder the app would display (`hydra_board::own::graphics`).
    pub graphics: std::path::PathBuf,
    /// The per-character data folder the placements were read from.
    pub placements_path: std::path::PathBuf,
    /// The fifth argument, `cmp w5, #0x0; cset w26, ne` @`0x100512fbc` — carried
    /// into the app value as a bool (`strb w26, [sp, #0x338]` @`0x10051324c`).
    pub logged_on: bool,
    pub renderer: Renderer,
    /// The app value is boxed with `_rust_alloc(0x70, 8)`; 112 bytes.
    pub app_size: usize,
}

impl RunPlan {
    /// The recovered title and placement, with everything else defaulted.
    pub fn new() -> RunPlan {
        RunPlan::default()
    }
}

impl Default for RunPlan {
    fn default() -> Self {
        RunPlan {
            title: TITLE.to_string(),
            placement: PLACEMENT.to_string(),
            size: Vec2::new(660.0, 520.0),
            graphics: std::path::PathBuf::new(),
            placements_path: std::path::PathBuf::new(),
            logged_on: false,
            renderer: Renderer::Wgpu,
            app_size: 0x70,
        }
    }
}

/// The inputs the original took by reference: a slice/array of shared handles
/// (`x0` points at eight pointers, `ldp q0, q1, [x19]` / `[x19, #0x20]` copy 64
/// bytes), a pair of strings (`x1`, `x2`), the data folder (`x3`, `x4`) and two
/// flags (`w5`, `x6`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Start {
    /// `x1`/`x2`: the character name pair stored at the front of the app value
    /// (`str x22, [sp, #0x2d0]; str x21, [sp, #0x2d8]` @`0x100513244`).
    pub character: String,
    /// `x3`/`x4`: the per-character folder (`Placements::load` and
    /// `Path::to_path_buf` are both called with it).
    pub data_dir: std::path::PathBuf,
    /// `w5`.
    pub logged_on: bool,
}

impl Start {
    pub fn new(character: impl Into<String>, data_dir: impl Into<std::path::PathBuf>) -> Start {
        Start {
            character: character.into(),
            data_dir: data_dir.into(),
            logged_on: false,
        }
    }
}

/// Build the run plan `eframe::run_native` would have been called with.
///
/// This is the documented seam: the original function never returns in a
/// meaningful way (it blocks in the platform event loop), so the recovered part
/// — everything up to `run_native` — is returned as a value instead.
pub fn run(start: &Start) -> RunPlan {
    RunPlan {
        title: TITLE.to_string(),
        placement: PLACEMENT.to_string(),
        graphics: graphics_dir(&start.data_dir),
        placements_path: start.data_dir.clone(),
        logged_on: start.logged_on,
        ..RunPlan::default()
    }
}

/// `hydra_board::own::graphics` @`0x10147042c` — the graphics folder that lives
/// beside the character data.
///
/// The callee is in `hydra_board`, which is being reconstructed in parallel, so
/// the recovered *call shape* (`graphics(&data_dir)`) is kept and the join is
/// the documented stand-in.
pub fn graphics_dir(data_dir: &std::path::Path) -> std::path::PathBuf {
    data_dir.join("graphics")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_constants_are_the_recovered_ones() {
        assert_eq!(TITLE, "Hydra");
        assert_eq!(TITLE.len(), 5);
        assert_eq!(PLACEMENT, "hub");
        assert_eq!(PLACEMENT.len(), 3);
    }

    #[test]
    fn default_size_is_the_packed_pair() {
        let plan = RunPlan::default();
        // 0x44250000 = 660.0, 0x44020000 = 520.0.
        assert_eq!(f32::from_bits(0x4425_0000), 660.0);
        assert_eq!(f32::from_bits(0x4402_0000), 520.0);
        assert_eq!(plan.size, Vec2::new(660.0, 520.0));
        assert_eq!(plan.renderer, Renderer::Wgpu);
        assert_eq!(plan.app_size, 0x70);
    }

    #[test]
    fn run_carries_the_start_into_the_plan() {
        let start = Start {
            character: "Aelish".into(),
            data_dir: "/tmp/hydra".into(),
            logged_on: true,
        };
        let plan = run(&start);
        assert_eq!(plan.title, "Hydra");
        assert_eq!(plan.placement, "hub");
        assert_eq!(plan.placements_path, std::path::PathBuf::from("/tmp/hydra"));
        assert_eq!(plan.graphics, std::path::PathBuf::from("/tmp/hydra/graphics"));
        assert!(plan.logged_on);
    }
}
