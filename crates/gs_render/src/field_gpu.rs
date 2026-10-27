//! The creature "field": a screen-space lighting pass drawn through an
//! egui/wgpu callback.
//!
//! Recovered from `gs_render::field_gpu::*`. The WGSL shaders are embedded
//! literals in the binary (`"field_gpu bounce"`, `"field_gpu opaque"`,
//! `"field_gpu blit\nstruct Blit { ndc: vec4<f32>, uv: vec4<f32>, inner:
//! vec4<f32> };\n@group(0) @binding(0) var<uniform> B: Blit; ..."`); only the
//! CPU-side layout and light construction are reconstructed here, everything
//! that used `wgpu` goes through [`crate::gpu::Gpu`].

use crate::gpu::{Gpu, TexFormat};

/// The tint every field texture label is suffixed with
/// (`format!("{}{}", name, "#c9d4ff")` — the `format_inner` call plus the
/// `"#c9d4ff"` literal at `0x1021d7f1a` in `init`/`paint_view`).
pub const FIELD_TINT: &str = "#c9d4ff";

/// The light kind byte stored at `+0x68`. `2` is the moonlight variant
/// (`mov w9, #0x2; strb w9, [x8, #0x68]` at `0x1009652d8`); the other kinds are
/// not visible in `paint_view`'s data.
pub const LIGHT_KIND_MOON: u8 = 2;

/// One local light in the field pass, 110 bytes (`0x6e`) in the binary.
///
/// Every field carries the offset it was written at by
/// [`FieldLight::moon`] @0x1009652b8.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldLight {
    /// `+0x00` — the clamped intensity (`str s1, [x8]`).
    pub intensity: f32,
    /// `+0x04` — an 8-byte value passed in a register and stored verbatim
    /// (`stur x2, [x8, #0x4]`); `// UNKNOWN:` not dereferenced, so its meaning
    /// is not recoverable from this function.
    pub tag: u64,
    /// `+0x0c` — zeroed.
    pub unknown_0c: u32,
    /// `+0x10..0x20` — four zeroed floats.
    pub unknown_10: [f32; 4],
    /// `+0x20` — first direction (12 bytes copied from the first `&[f32; 3]`
    /// argument).
    pub axis: [f32; 3],
    /// `+0x2c` — second direction (12 bytes from the second argument).
    pub dir: [f32; 3],
    /// `+0x38` — the `f32` argument stored verbatim (`str s0, [x8, #0x38]`).
    pub spread: f32,
    /// `+0x3c` — `[0.0, 1.0]` from the literal at `0x10207d4c0`.
    pub window: [f32; 2],
    /// `+0x44` — `0.05 * intensity + 0.02`.
    pub inner: f32,
    /// `+0x48` — `(1 - 0.5 * intensity) * 0.22`.
    pub outer: f32,
    /// `+0x4c` — the four-float falloff ramp `[0.2, 0.1, 0.2, 0.8]` from
    /// `0x1021d7f70`.
    pub falloff: [f32; 4],
    /// `+0x5c` — `[24.0, 80.0]` from `0x1021d7f48`.
    pub reach: [f32; 2],
    /// `+0x64` — `1.0`.
    pub power: f32,
    /// `+0x68` — [`LIGHT_KIND_MOON`].
    pub kind: u8,
    /// `+0x69`, `+0x6a`, `0x6b`, `0x6d` — four bytes written as `1`.
    pub flags: [u8; 4],
    /// `+0x6c` — zeroed, the padding tap between the flag bytes.
    pub unknown_6c: u8,
}

/// `1 - 0.5 * intensity` scaling used for [`FieldLight::outer`] (`fmov s3,
/// #-0.5` at `0x1009652d4`).
pub const MOON_OUTER_SCALE: f32 = 0.22;
/// `fmov s3, #1.0` / `0x3ca3d70a` constants of [`FieldLight::inner`].
pub const MOON_INNER_BIAS: f32 = 0.02;
/// `0x3d4ccccd` = `0.05`, the intensity coefficient of [`FieldLight::inner`].
pub const MOON_INNER_GAIN: f32 = 0.05;
/// The falloff ramp literal at `0x1021d7f70`.
pub const MOON_FALLOFF: [f32; 4] = [0.2, 0.1, 0.2, 0.8];
/// The reach pair literal at `0x1021d7f48`.
pub const MOON_REACH: [f32; 2] = [24.0, 80.0];

impl FieldLight {
    /// `FieldLight::moon` @0x1009652b8 — build a moonlight entry.
    ///
    /// Fully recovered except for the `tag` word (see the field note). The
    /// intensity is clamped into `0..=1` (`fcsel` with `#0.0` and `#1.0`), then
    ///
    /// ```text
    /// inner = 0.05 * i + 0.02
    /// outer = (1 - 0.5 * i) * 0.22
    /// ```
    ///
    /// and the fixed `window`/`falloff`/`reach`/`power` values are stored from
    /// the literals above.
    pub fn moon(
        intensity: f32,
        axis: [f32; 3],
        dir: [f32; 3],
        tag: u64,
        spread: f32,
    ) -> FieldLight {
        let i = if intensity < 0.0 {
            0.0
        } else if intensity > 1.0 {
            1.0
        } else {
            intensity
        };
        FieldLight {
            intensity: i,
            tag,
            unknown_0c: 0,
            unknown_10: [0.0; 4],
            axis,
            dir,
            spread,
            window: [0.0, 1.0],
            inner: MOON_INNER_GAIN * i + MOON_INNER_BIAS,
            outer: (1.0 - 0.5 * i) * MOON_OUTER_SCALE,
            falloff: MOON_FALLOFF,
            reach: MOON_REACH,
            power: 1.0,
            kind: LIGHT_KIND_MOON,
            flags: [1, 1, 1, 1],
            unknown_6c: 0,
        }
    }
}

/// Size of [`FieldLight`] in the binary (`0x6e` bytes) — the last byte written
/// by `moon` is `+0x6d` (`strb w9, [x8, #0x6d]`).
pub const FIELD_LIGHT_SIZE: usize = 0x6e;

/// The field's own view/plumbing state.
///
/// `paint_view` reads `[state]` (a `Mutex`-guarded cell reached through a
/// `OnceBox`/`OnceLock` static), its flag at `+0x18`, a growable table at
/// `+0x28`/`+0x30` (pointer + length, entries 32 bytes: `add x9, x9, x21, lsl
/// #5`) and a fallback 2-vector at `+0xb8`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldState {
    /// `+0x18` — "the field has data this frame".
    pub live: bool,
    /// `+0x28` — table of 32-byte pan entries (first two floats used).
    pub pans: Vec<[f32; 8]>,
    /// `+0xb8` — the fallback pan used when `mode != 2`.
    pub fallback_pan: [f32; 2],
}

impl FieldState {
    /// `+0x30` in the binary, clamped to at least `0x20` before the index check
    /// (`cmp x9, #0x20; csel x9, x9, x10, lo` at `0x100965484`).
    pub fn pan_slots(&self) -> usize {
        self.pans.len().max(0x20)
    }
}

/// `paint_view` @0x100965394 — resolve the 2-D pan for one field view.
///
/// The recovered arithmetic (`0x100965478..0x1009654f0`) is a lerp:
///
/// ```text
/// base = at.pos                       // 2 floats at `[x3]`
/// t    = at.t                         // 1 float at `[x3 + 8]`
/// off  = if mode == 2 { table[index].xy } else { state.fallback_pan }
/// out  = base + off * t
/// ```
///
/// `None` when `mode == 2` but `index` is past the (clamped) table length: the
/// binary jumps to `slice_index_fail` in that case
/// (`cmp x21, x9; b.hs 0x1009655ac`).
///
/// `// UNKNOWN:` the rest of the function (after the lerp it locks the field
/// state and records the draw); the GPU work is handed to the seam.
pub fn paint_view(
    state: &FieldState,
    mode: u64,
    at: &PanAt,
    index: usize,
    gpu: &mut dyn Gpu,
) -> Option<[f32; 2]> {
    let off = if mode == 2 {
        if index >= state.pan_slots() {
            return None;
        }
        let entry = state.pans.get(index).copied().unwrap_or([0.0; 8]);
        [entry[0], entry[1]]
    } else {
        state.fallback_pan
    };
    let out = [at.pos[0] + off[0] * at.t, at.pos[1] + off[1] * at.t];
    gpu.draw(&crate::gpu::DrawCall {
        pipeline: "field_gpu opaque",
        texture: None,
        vertex_offset: 0,
        vertex_count: 0,
        depth: (out[0], out[1]),
    });
    Some(out)
}

/// The `at` argument of [`paint_view`] (`[x3]` = two floats, `[x3 + 8]` = the
/// lerp factor).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PanAt {
    pub pos: [f32; 2],
    pub t: f32,
}

/// GPU resources the field pass owns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Textures {
    /// `"field_gpu frame"` — the HDR accumulation target.
    pub frame: Option<u64>,
    /// `"field_gpu bounce"` — the second bounce target.
    pub bounce: Option<u64>,
    /// Atlas textures, labelled `"<name>#c9d4ff"`.
    pub atlas: Vec<String>,
}

/// `field_gpu::init` @0x100969f78 — create the field's render targets.
///
/// Recovered: a 1296-byte (`0x510`) `Gpu` value is memcpy'd into an `Arc`, its
/// first-use flag is set with `casa` (`+0x10` of the inner state), a lazily
/// cached 32-byte default is installed at `+0x168`, and a `HashMap` entry is
/// inserted under the key formed from [`FIELD_TINT`]. `create_texture` calls
/// follow for the frame/bounce/albedo/litgeo targets.
///
/// `// UNKNOWN:` the exact texture dimensions come from the egui render size at
/// call time, so they are parameters here.
pub fn init<G: Gpu>(gpu: &mut G, width: u32, height: u32) -> Textures {
    gpu.create_target(
        "field_gpu frame",
        width.max(1),
        height.max(1),
        TexFormat::Rgba16Float,
    );
    gpu.create_target(
        "field_gpu bounce",
        width.max(1),
        height.max(1),
        TexFormat::Rgba8Unorm,
    );
    let atlas = ["atlas", "albedo", "litgeo", "geo"]
        .iter()
        .map(|n| format!("{n}{FIELD_TINT}"))
        .collect();
    Textures {
        frame: Some(1),
        bounce: Some(2),
        atlas,
    }
}

/// `field_gpu::available` @0x10096a990 — is the field pass usable?
///
/// The symbol is a 4-byte tail branch (`b egui::Context::read`), i.e. the whole
/// body was `cx.read()`: it asks the UI context whether the field view exists.
/// With no `egui` here that is the `live` flag of [`FieldState`].
pub fn available(state: &FieldState) -> bool {
    state.live
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::NullGpu;

    #[test]
    fn field_light_size_is_110_bytes() {
        assert_eq!(FIELD_LIGHT_SIZE, 0x6e);
        // The recovered layout ends at +0x6d.
        assert_eq!(FIELD_LIGHT_SIZE, 0x6d + 1);
    }

    #[test]
    fn moon_clamps_intensity_and_derives_the_ramp() {
        let l = FieldLight::moon(2.0, [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], 9, 0.25);
        assert_eq!(l.intensity, 1.0);
        // 0.05 * 1.0 + 0.02
        assert_eq!(l.inner, 0.07);
        // (1 - 0.5 * 1.0) * 0.22
        assert_eq!(l.outer, 0.11);
        assert_eq!(l.window, [0.0, 1.0]);
        assert_eq!(l.falloff, [0.2, 0.1, 0.2, 0.8]);
        assert_eq!(l.reach, [24.0, 80.0]);
        assert_eq!(l.power, 1.0);
        assert_eq!(l.kind, LIGHT_KIND_MOON);
        assert_eq!(l.flags, [1, 1, 1, 1]);
        assert_eq!(l.tag, 9);
        assert_eq!(l.spread, 0.25);
        assert_eq!(l.unknown_10, [0.0; 4]);

        let low = FieldLight::moon(-3.0, [0.0; 3], [0.0; 3], 0, 0.0);
        assert_eq!(low.intensity, 0.0);
        assert_eq!(low.inner, MOON_INNER_BIAS);
        assert_eq!(low.outer, MOON_OUTER_SCALE);
    }

    #[test]
    fn paint_view_lerps_the_pan() {
        let mut state = FieldState {
            live: true,
            pans: vec![[1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
            fallback_pan: [0.5, -0.5],
        };
        let at = PanAt {
            pos: [10.0, 20.0],
            t: 2.0,
        };
        let mut gpu = NullGpu;
        // mode == 2 reads the table.
        assert_eq!(paint_view(&state, 2, &at, 0, &mut gpu), Some([12.0, 24.0]));
        // Any other mode reads the fallback pan.
        assert_eq!(paint_view(&state, 0, &at, 0, &mut gpu), Some([11.0, 19.0]));
        // Index past the (clamped) table length fails.
        state.pans.clear();
        assert_eq!(paint_view(&state, 2, &at, 0, &mut gpu), Some([10.0, 20.0]));
        assert_eq!(paint_view(&state, 2, &at, 0x20, &mut gpu), None);
    }

    #[test]
    fn init_labels_match_the_binary_suffix() {
        let mut gpu = NullGpu;
        let t = init(&mut gpu, 0, 7);
        assert_eq!(t.atlas[0], "atlas#c9d4ff");
        assert_eq!(t.atlas[2], "litgeo#c9d4ff");
        assert!(t.frame.is_some() && t.bounce.is_some());
    }

    #[test]
    fn available_tracks_the_live_flag() {
        let mut s = FieldState::default();
        assert!(!available(&s));
        s.live = true;
        assert!(available(&s));
    }
}
