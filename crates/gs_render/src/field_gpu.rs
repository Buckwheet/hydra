//! The creature "field": a screen-space lighting pass drawn through an
//! egui/wgpu paint callback.
//!
//! Recovered from `gs_render::field_gpu::*`. The WGSL shaders are embedded
//! literals in the binary (`"field_gpu bounce"`, `"field_gpu opaque"`,
//! `"field_gpu blit\nstruct Blit { ndc: vec4<f32>, uv: vec4<f32>, inner:
//! vec4<f32> };\n@group(0) @binding(0) var<uniform> B: Blit; ..."`);
//!
//! The egui/egui-wgpu half is the real thing:
//!
//! * [`paint_view_ui`] computes the field quad with the recovered lerp and hands
//!   it to `egui::Painter::add` as a
//!   `egui_wgpu::Callback::new_paint_callback(rect, FieldCallback)` —
//!   `bl 0x1009602dc` at `0x10096557c` then `bl egui::Painter::add` at
//!   `0x100965588`;
//! * [`FieldCallback`] implements `egui_wgpu::renderer::CallbackTrait` with the
//!   binary's three methods (`0x100963924`, `0x1009635bc`, `0x1009636f0`);
//! * [`init_ctx`] stores the field state in the `egui::Context` the way `init`
//!   ends (`bl egui::Context::write` at `0x10096a108`).
//!
//! Everything that needs a live `wgpu::Device` (pipelines, render targets,
//! `Gpu::{upload_frame, bounce_pass, prepare_piece, paint_piece}`) still only
//! exists as the descriptor seam [`crate::gpu::Gpu`]; those sites are marked
//! `// UNKNOWN:`.

use crate::gpu::{Gpu, TexFormat};
use egui::epaint::PaintCallbackInfo;
use egui::{Painter, Rect};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, ScreenDescriptor};
use std::sync::{Mutex, MutexGuard};

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
/// `OnceBox`/`OnceLock` static), a byte at `+0x18`, a growable table at
/// `+0x28`/`+0x30` (pointer + length, entries 32 bytes: `add x9, x9, x21, lsl
/// #5`) and a fallback 2-vector at `+0xb8`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldState {
    /// `+0x18` — "the field has data this frame". In the instruction stream the
    /// byte is only read through the mutex-guard bookkeeping
    /// (`ldrb w8, [x24, #0x18]`, `0x1009653f0`), so this reconstruction keeps it
    /// as the model's own liveness flag (see also [`available`]).
    pub live: bool,
    /// `+0x28`/`+0x30` — table of 32-byte pan entries. The lerp reads the four
    /// floats at `+0x08` (offset, `ldp d1, d2, [x9, #0x8]`) and `+0x10` (size,
    /// the second `d`), see [`FieldState::pan_offset`] / [`FieldState::pan_size`].
    pub pans: Vec<[f32; 8]>,
    /// `+0xb8` — the fallback size used when `mode != 2`.
    pub fallback_pan: [f32; 2],
}

impl FieldState {
    /// `+0x30` in the binary, clamped to at least `0x20` before the index check
    /// (`cmp x9, #0x20; csel x9, x9, x10, lo` at `0x100965484`).
    pub fn pan_slots(&self) -> usize {
        self.pans.len().max(0x20)
    }

    /// The 32-byte pan entry at `index` (zeroed when the table is shorter — the
    /// index has already been bounds-checked against [`FieldState::pan_slots`],
    /// which is why the binary never actually reads past the end).
    pub fn pan_entry(&self, index: usize) -> [f32; 8] {
        self.pans.get(index).copied().unwrap_or([0.0; 8])
    }

    /// `+0x08` — the offset the lerp scales (`ldp d1, d2, [x9, #0x8]`).
    pub fn pan_offset(&self, index: usize) -> [f32; 2] {
        let e = self.pan_entry(index);
        [e[2], e[3]]
    }

    /// `+0x10` — the quad size the lerp scales for the far corner (`ldp d1, d2,
    /// [x9, #0x8]`, the second register).
    pub fn pan_size(&self, index: usize) -> [f32; 2] {
        let e = self.pan_entry(index);
        [e[4], e[5]]
    }
}

/// The `at` argument of [`paint_view`] (`[x2]` = two floats, `[x2 + 8]` = the
/// lerp factor).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PanAt {
    pub pos: [f32; 2],
    pub t: f32,
}

/// The geometric core of `paint_view` @0x100965394 — the field quad in points.
///
/// Recovered instruction by instruction from `0x100965478..0x1009654f8`:
///
/// ```text
/// if mode == 2 {
///     if index >= max(len, 0x20) { return None }        // b.hs 0x1009655ac
///     (off, size) = (entry[2..4], entry[4..6])
/// } else {
///     (off, size) = ([0.0, 0.0], state.fallback_pan)     // 0.0 * t
/// }
/// min = at.pos + off  * at.t
/// max = min    + size * at.t
/// ```
///
/// `mode == 2` also selects the table (`cmp x22, #0x2; b.ne`); `None` is the
/// out-of-range arm, which in the binary leaves through the unlock path without
/// painting anything.
pub fn paint_view_quad(state: &FieldState, mode: u64, at: &PanAt, index: usize) -> Option<Rect> {
    let (off, size) = if mode == 2 {
        if index >= state.pan_slots() {
            return None;
        }
        (state.pan_offset(index), state.pan_size(index))
    } else {
        ([0.0, 0.0], state.fallback_pan)
    };
    let min = [
        at.pos[0] + off[0] * at.t,
        at.pos[1] + off[1] * at.t,
    ];
    let max = [min[0] + size[0] * at.t, min[1] + size[1] * at.t];
    Some(Rect::from_min_max(
        egui::pos2(min[0], min[1]),
        egui::pos2(max[0], max[1]),
    ))
}

/// `paint_view` @0x100965394 — resolve the 2-D pan for one field view.
///
/// This is the seam-shaped entry point kept from the first pass: it returns the
/// lerped offset (the quad's `min`) and records the draw through
/// [`crate::gpu::Gpu`]. The real path is [`paint_view_ui`], which adds the
/// recovered `egui_wgpu` paint callback to a painter.
///
/// `None` when `mode == 2` but `index` is past the (clamped) table length: the
/// binary jumps to `slice_index_fail` in that case
/// (`cmp x21, x9; b.hs 0x1009655ac`).
pub fn paint_view(
    state: &FieldState,
    mode: u64,
    at: &PanAt,
    index: usize,
    gpu: &mut dyn Gpu,
) -> Option<[f32; 2]> {
    let quad = paint_view_quad(state, mode, at, index)?;
    let out = [quad.min.x, quad.min.y];
    gpu.draw(&crate::gpu::DrawCall {
        pipeline: "field_gpu opaque",
        texture: None,
        vertex_offset: 0,
        vertex_count: 0,
        depth: (out[0], out[1]),
    });
    Some(out)
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
/// Its egui half would be `field_state(ctx).is_some()`; this overload keeps the
/// model-level answer for callers that already hold the [`FieldState`].
pub fn available(state: &FieldState) -> bool {
    state.live
}

// ---------------------------------------------------------------------------
// egui / egui-wgpu: the real paint path
// ---------------------------------------------------------------------------

/// The `egui` data key `init_ctx` writes [`FieldState`] under.
///
/// `// INFERRED:` the binary's last statement is `bl egui::Context::write`
/// (`0x10096a108`) with a *zero-sized* closure — the `Gpu` itself lives in the
/// `TypeId`-keyed map behind the field frame's `RwLock` (`0x1043b0f18`), so the
/// `Context` write carries no value we can name. Storing the state in egui's own
/// `IdTypeMap` needs a stable `Id`, and this is the only string the whole
/// function names (the tint suffix in the recovered literal pool).
pub const FIELD_STATE_KEY: &str = FIELD_TINT;

/// The `egui::Id` [`init_ctx`] writes and [`field_state`] reads.
pub fn field_state_id() -> egui::Id {
    egui::Id::new(FIELD_STATE_KEY)
}

/// `field_gpu::init` @0x100969f78 — the egui half: publish the field state to
/// the `egui::Context`.
///
/// The binary builds a 1296-byte (`0x510`) `Gpu` value (`bl Gpu::new` at
/// `0x100969fa8`), boxes it, inserts it into the `TypeId` map at the frame's
/// `+0x168`, and finishes with `Context::write`. The render targets are created
/// later by `Gpu::upload_frame`/`bounce_pass`; nothing here needs a device, so
/// the state goes straight into egui's data map under [`field_state_id`].
pub fn init_ctx(ctx: &egui::Context, state: FieldState) {
    ctx.data_mut(|d| d.insert_temp(field_state_id(), state));
}

/// Read back what [`init_ctx`] stored (`Context::read` in the original).
pub fn field_state(ctx: &egui::Context) -> Option<FieldState> {
    ctx.data(|d| d.get_temp::<FieldState>(field_state_id()))
}

/// The `+0x3c` word value that means "there is no piece to draw".
///
/// `paint` reads it as `cmn w24, #0x1; b.eq` (`0x1009638c0`), then splits the
/// surviving word into `(w24 >> 31, w24 & 0x7fff_ffff)` for `paint_piece`.
/// `paint_view` always stores `-1` here (`mov w8, #-0x1` at `0x100965568`), so a
/// callback built by [`paint_view_ui`] never draws in `paint` — the drawing work
/// of that path is `prepare`'s.
pub const NO_PIECE: u32 = u32::MAX;

/// One entry of the field's per-view table.
///
/// The binary's value type is `gs_render::field_gpu::View` (`View::new` at
/// `0x100969544`, 864 = `0x360`-byte hashbrown entries, `drop_in_place<View>` at
/// `0x100938664`). Only the parts `prepare` visibly reads and writes are kept;
/// the rest of the 864 bytes was not decoded (`+0x324` holds a `kind` compared
/// with `3`, which selects "build a new view" over "copy the old one").
/// `// UNKNOWN: the remaining fields of View.`
#[derive(Debug, Clone, PartialEq)]
pub struct FieldView {
    /// The hash key — `FieldCallback::key`, hashed by `hash_one(self + 0x28)`
    /// (`0x100963cac`).
    pub key: u64,
    /// The screen the view was built for (`ScreenDescriptor`).
    pub screen_px: [u32; 2],
    pub pixels_per_point: f32,
    /// The lerped field quad, `FieldCallback`'s `+0x18` rect.
    pub quad: Rect,
    /// `+0x30`/`+0x38` of the callback, handed straight to `prepare_piece`.
    pub at: PanAt,
    pub mode: u64,
    pub index: usize,
    /// The frame id (`+0x4f0`) the view was built in.
    pub frame: u64,
}

/// What `Gpu::paint_piece` (`0x100965908`) is asked to draw.
///
/// Recovered arguments: `(w24 >> 31, w24 & 0x7fff_ffff, self.mode)` plus the
/// view looked up by `self.key`. `// UNKNOWN:` the pipeline, bind group and
/// push-constant work inside `paint_piece`.
#[derive(Debug, Clone, PartialEq)]
pub struct PieceDraw {
    /// `FieldCallback::key` — which view the piece belongs to.
    pub key: u64,
    /// `and w3, w24, #0x7fff_ffff`.
    pub magnitude: u32,
    /// `lsr w4, w24, #31`.
    pub sign: u32,
    /// `ldr x5, [x20]` — the callback's mode.
    pub mode: u64,
    /// The view's quad, when `prepare` already recorded one for this key.
    pub quad: Option<Rect>,
}

/// The CPU-side bookkeeping of the field pass.
///
/// `// UNKNOWN:` the wgpu objects (pipelines, the HDR/albedo targets, bind
/// groups) cannot be created without a `wgpu::Device`, so what is modelled is
/// the state the three callback methods move around: the frame counter
/// (`+0x4f0`), the flush flag `finish_prepare` sets and `prepare` consumes
/// (`+0x509`), the per-view table (`HashMap<u64, View>` at `+0x4c0..+0x4e0`) and
/// what `paint_piece` would draw.
#[derive(Debug, Default)]
pub struct FieldFrame {
    pub views: Vec<FieldView>,
    /// `+0x4f0` — the next frame id.
    pub frame: u64,
    /// `+0x509` — set by `finish_prepare`, taken by the next `prepare`.
    pub flush_pending: bool,
    /// What `Gpu::paint_piece` (`0x100965908`) is asked to draw.
    pub draws: Vec<PieceDraw>,
}

/// The `CallbackResources` value: the binary's `gs_render::field_gpu::Gpu`
/// (1296 = `0x510` bytes, built by `Gpu::new` and boxed by `init`).
///
/// It is a resource rather than a field of the callback because all three
/// `CallbackTrait` methods fetch it by type first and bail out when it is
/// missing (`resources.get::<Gpu>()` — the `TypeId` compares at `0x100963650`,
/// `0x1009636bc` and `0x1009637fc`).
#[derive(Debug)]
pub struct FieldGpu {
    pub textures: Textures,
    frame: Mutex<FieldFrame>,
}

impl Default for FieldGpu {
    fn default() -> Self {
        FieldGpu {
            textures: Textures::default(),
            frame: Mutex::new(FieldFrame::default()),
        }
    }
}

impl FieldGpu {
    pub fn new(textures: Textures) -> Self {
        FieldGpu {
            textures,
            frame: Mutex::new(FieldFrame::default()),
        }
    }

    /// The shared state (`Mutex` only because `paint` receives `&CallbackResources`
    /// while `prepare` receives `&mut`).
    pub fn frame(&self) -> MutexGuard<'_, FieldFrame> {
        self.frame.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The body of `prepare`'s view maintenance (`0x100963ae4..0x100963f14`).
    ///
    /// Recovered control flow (`0x100963adc`): if the `+0x509` flag is set, clear
    /// it, bump the frame counter (`+0x4f0`) and erase every entry of the view
    /// table (the `ldr x11, [x25, #0x4d0]` / `[x25, #0x4d8]` pair inside the loop
    /// is hashbrown's `items`/`growth_left`, not a field of ours); then
    /// insert-or-replace the view under `cb.key`, and when the view's frame
    /// differs from the frame's (`ldr x8, [x20, #0xa0]`, `0x100963e4c`) call
    /// `Gpu::{upload_frame, bounce_pass}` — the GPU half of that is the
    /// `// UNKNOWN:` part.
    pub fn record_view(&self, cb: &FieldCallback, screen: &ScreenDescriptor) -> u64 {
        let mut frame = self.frame();
        if frame.flush_pending {
            frame.flush_pending = false;
            frame.frame += 1;
            frame.views.clear();
        }
        let id = frame.frame;
        let view = FieldView {
            key: cb.key,
            screen_px: screen.size_in_pixels,
            pixels_per_point: screen.pixels_per_point,
            quad: cb.quad,
            at: cb.at,
            mode: cb.mode,
            index: cb.index,
            frame: id,
        };
        match frame.views.iter_mut().find(|v| v.key == cb.key) {
            Some(slot) => *slot = view,
            None => frame.views.push(view),
        }
        id
    }

    /// The body of `finish_prepare` (`0x1009636c8`): `strb w9, [x19, #0x509]`.
    pub fn end_frame(&self) {
        self.frame().flush_pending = true;
    }

    /// The body of `paint`'s draw (`Gpu::paint_piece`, `0x100965908`).
    /// `// UNKNOWN:` the pipeline/bind-group/push-constant work; the recovered
    /// arguments are `(index & 0x7fff_ffff, index >> 31, self.mode)` plus the
    /// view looked up by key.
    pub fn paint_piece(&self, cb: &FieldCallback, magnitude: u32, sign: u32) {
        let mut frame = self.frame();
        let quad = frame
            .views
            .iter()
            .find(|v| v.key == cb.key)
            .map(|v| v.quad);
        frame.draws.push(PieceDraw {
            key: cb.key,
            magnitude,
            sign,
            mode: cb.mode,
            quad,
        });
    }
}

/// `<gs_render::field_gpu::FieldCallback as egui_wgpu::renderer::CallbackTrait>`
/// — the 64-byte (`0x40`) value `paint_view` boxes into
/// `Callback::new_paint_callback`.
///
/// The size is established: `new_paint_callback` allocates `0x40` bytes and
/// copies exactly four 16-byte vectors out of the argument (`mov w0, #0x40` /
/// `ldp q0, q1, [x21]` / `stp q0, q1, [x0]` twice, `0x1009602fc..0x10096031c`),
/// so the binary's `FieldCallback` was `64` bytes. The offsets below are the ones
/// the three trait methods read back out of that copy; this crate's type is
/// *larger* because it owns a [`FieldState`] snapshot (the binary stores an
/// 8-byte `&Arc<Field>` handle at `+0x10`) and uses `Rect`/`Option<u32>` instead
/// of raw float/word pairs:
///
/// | offset | field | evidence |
/// |---|---|---|
/// | `+0x00` | [`FieldCallback::mode`] | `stp x22, …` in `paint_view`; `ldr x5, [x20]` before `paint_piece` |
/// | `+0x08` | [`FieldCallback::index`] | `paint_view`'s 4th argument; `stp x21, x24, [sp, #0x28]` |
/// | `+0x10` | the field handle | `ldr x20, [x22, #0x10]` then `Mutex::lock` in `prepare` |
/// | `+0x18` | [`FieldCallback::quad`] | `ldur q0, [x24, #0x18]` → `prep_piece`'s rect |
/// | `+0x28` | [`FieldCallback::key`] | `hash_one(self + 0x28)` keys the view table |
/// | `+0x30` | [`FieldCallback::at`]`.pos` | stored for `prepare_piece` (`0x100963eb0`) |
/// | `+0x38` | [`FieldCallback::at`]`.t` | `ldr w8, [x24, #0x38]` |
/// | `+0x3c` | [`FieldCallback::piece`] | `ldr w24, [x20, #0x3c]`, `-1` = none |
///
/// `// INFERRED:` the binary stores `&Arc<Field>` at `+0x10` and locks its inner
/// `Mutex` in `prepare`; here the callback owns a snapshot of [`FieldState`]
/// (the same type [`init_ctx`] publishes), which keeps `FieldCallback` `'static +
/// Send + Sync` and lets the headless tests inspect exactly what was handed over.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldCallback {
    /// `+0x00`
    pub mode: u64,
    /// `+0x08`
    pub index: usize,
    /// `+0x18` — the lerped field quad ([`paint_view_quad`]).
    pub quad: Rect,
    /// `+0x28` — the view-table key (`x5` at the call site, the caller's entity id).
    pub key: u64,
    /// `+0x30` — the pan/t pair `prepare_piece` also receives.
    pub at: PanAt,
    /// `+0x3c` — the piece word; [`NO_PIECE`] means "nothing to draw".
    pub piece: Option<u32>,
    /// The field state snapshot (the `+0x10` handle in the binary).
    pub state: FieldState,
}

impl FieldCallback {
    /// Build the callback `paint_view` hands to `new_paint_callback`.
    pub fn new(
        mode: u64,
        index: usize,
        quad: Rect,
        key: u64,
        at: PanAt,
        state: FieldState,
    ) -> Self {
        FieldCallback {
            mode,
            index,
            quad,
            key,
            at,
            piece: None,
            state,
        }
    }

    /// The `+0x3c` word as stored: `None` is `-1`.
    pub fn piece_word(&self) -> u32 {
        self.piece.unwrap_or(NO_PIECE)
    }

    /// The `(magnitude, sign)` split `paint` passes to `paint_piece`:
    /// `lsr w4, w24, #31` / `and w3, w24, #0x7fff_ffff`. `None` is the
    /// `cmn w24, #0x1; b.eq` bail-out.
    pub fn piece_parts(&self) -> Option<(u32, u32)> {
        let w = self.piece?;
        Some((w & 0x7fff_ffff, w >> 31))
    }
}

impl CallbackTrait for FieldCallback {
    /// `prepare` @0x100963924.
    ///
    /// Recovered skeleton: fetch the [`FieldGpu`] resource and return an empty
    /// `Vec` if it is absent (`0x100963a50` → the tail at `0x100963a98`), lock
    /// the field, do the `+0x509` flush, then build/insert the view for
    /// `self.key`. The pipeline/`upload_frame`/`bounce_pass`/`prepare_piece` work
    /// in between needs the `wgpu::Device` and is not reconstructed
    /// (`// UNKNOWN:`).
    fn prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = callback_resources.get_mut::<FieldGpu>() else {
            return Vec::new();
        };
        gpu.record_view(self, screen_descriptor);
        Vec::new()
    }

    /// `finish_prepare` @0x1009635bc — marks the `Gpu` dirty (`+0x509 = 1`) so
    /// the next `prepare` flushes the view table.
    fn finish_prepare(
        &self,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(gpu) = callback_resources.get_mut::<FieldGpu>() else {
            return Vec::new();
        };
        gpu.end_frame();
        Vec::new()
    }

    /// `paint` @0x1009636f0.
    ///
    /// Recovered: fetch the [`FieldGpu`] resource (bail if absent), bail when the
    /// piece word is `-1`, then set the viewport to the whole screen in physical
    /// pixels (`ucvtf` of `info.screen_size_px` into
    /// `RenderPass::set_viewport(0, 0, w, h, 0, 1)`, `0x1009638cc`) and hand the
    /// piece to `Gpu::paint_piece`.
    fn paint(
        &self,
        info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let Some(gpu) = callback_resources.get::<FieldGpu>() else {
            return;
        };
        let Some((magnitude, sign)) = self.piece_parts() else {
            return;
        };
        let [w, h] = info.screen_size_px;
        render_pass.set_viewport(0.0, 0.0, w as f32, h as f32, 0.0, 1.0);
        gpu.paint_piece(self, magnitude, sign);
    }
}

/// `field_gpu::paint_view` @0x100965394 — the real egui path.
///
/// Same recovered maths as [`paint_view`] (`paint_view_quad`), plus the two
/// things the binary does with the result:
///
/// * the quad is tested against the painter's clip rect
///   (`ldr d0, [x19, #0x20]` = `Painter::clip_rect`, `0x100965510..0x10096553c`)
///   and nothing is painted when it misses (`b.hi 0x10096558c`);
/// * `egui_wgpu::Callback::new_paint_callback(rect, FieldCallback)` is added to
///   the painter (`0x10096557c`/`0x100965588`).
///
/// `rect` is the field view's rect — what egui uses to clip the callback
/// (`PaintCallback::rect`). The lerped quad itself is kept inside the callback
/// (its `+0x18` field), which is where the binary passes it on to
/// `Gpu::prepare_piece`.
///
/// Returns the lerped pan (the quad's `min`), or `None` when the index is out of
/// range / the quad is off-screen.
pub fn paint_view_ui(
    painter: &Painter,
    rect: Rect,
    state: &FieldState,
    mode: u64,
    at: &PanAt,
    index: usize,
) -> Option<[f32; 2]> {
    paint_view_ui_keyed(painter, rect, state, mode, at, index, 0)
}

/// [`paint_view_ui`] with the view-table key the binary takes as its 6th
/// argument (`mov x5, x19` at the one recovered call site, `injury_doll::view::
/// paint_in_view` at `0x1008d11ac`). The value is the caller's entity id and
/// keys the per-view `HashMap<u64, View>`.
pub fn paint_view_ui_keyed(
    painter: &Painter,
    rect: Rect,
    state: &FieldState,
    mode: u64,
    at: &PanAt,
    index: usize,
    key: u64,
) -> Option<[f32; 2]> {
    let quad = paint_view_quad(state, mode, at, index)?;
    if !quad.intersects(painter.clip_rect()) {
        return None;
    }
    let cb = FieldCallback::new(mode, index, quad, key, *at, state.clone());
    painter.add(Callback::new_paint_callback(rect, cb));
    Some([quad.min.x, quad.min.y])
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
            // The lerp reads the entry's floats at `+0x08`/`+0x10` (indices 2..6),
            // not the first two (`ldp d1, d2, [x9, #0x8]` at 0x1009654a4).
            pans: vec![[0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0]],
            fallback_pan: [0.5, -0.5],
        };
        let at = PanAt {
            pos: [10.0, 20.0],
            t: 2.0,
        };
        let mut gpu = NullGpu;
        // mode == 2 reads the table.
        assert_eq!(paint_view(&state, 2, &at, 0, &mut gpu), Some([12.0, 24.0]));
        // Any other mode has no offset (the `0.0 * t` addend), so the pan is
        // just `at.pos`; the fallback pan becomes the quad's *size*.
        assert_eq!(paint_view(&state, 0, &at, 0, &mut gpu), Some([10.0, 20.0]));
        // Index past the (clamped) table length fails.
        state.pans.clear();
        assert_eq!(paint_view(&state, 2, &at, 0, &mut gpu), Some([10.0, 20.0]));
        assert_eq!(paint_view(&state, 2, &at, 0x20, &mut gpu), None);
    }

    #[test]
    fn paint_view_quad_is_min_plus_size_times_t() {
        let state = FieldState {
            live: true,
            // unused pair, offset, size
            pans: vec![[9.0, 9.0, 1.0, 2.0, 30.0, 40.0, 0.0, 0.0]],
            fallback_pan: [0.5, -0.5],
        };
        let at = PanAt {
            pos: [10.0, 20.0],
            t: 2.0,
        };
        // mode == 2: min = pos + offset*t, max = min + size*t.
        let q = paint_view_quad(&state, 2, &at, 0).unwrap();
        assert_eq!([q.min.x, q.min.y], [12.0, 24.0]);
        assert_eq!([q.max.x, q.max.y], [12.0 + 60.0, 24.0 + 80.0]);
        // The unused pair really is unused (indices 0 and 1).
        let mut shifted = state.clone();
        shifted.pans[0][0] = 1e6;
        shifted.pans[0][1] = -1e6;
        assert_eq!(paint_view_quad(&shifted, 2, &at, 0), Some(q));
        // mode != 2: min = pos (the `0.0 * t` addend), size = fallback_pan.
        let f = paint_view_quad(&state, 0, &at, 0).unwrap();
        assert_eq!([f.min.x, f.min.y], [10.0, 20.0]);
        assert_eq!([f.max.x, f.max.y], [11.0, 19.0]);
        // `pan_slots().max(0x20)`: index 0x1f is inside, 0x20 is not.
        assert!(paint_view_quad(&state, 2, &at, 0x1f).is_some());
        assert_eq!(paint_view_quad(&state, 2, &at, 0x20), None);
        // A table of 0x21 entries puts index 0x20 in range.
        let wide = FieldState {
            pans: vec![[0.0; 8]; 0x21],
            ..state.clone()
        };
        assert_eq!(wide.pan_slots(), 0x21);
        let w = paint_view_quad(&wide, 2, &at, 0x20).unwrap();
        assert_eq!([w.min.x, w.min.y], [10.0, 20.0]);
        assert_eq!([w.max.x, w.max.y], [10.0, 20.0]);
    }

    fn headless_input() -> egui::RawInput {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        ));
        input
    }

    fn callbacks(out: &egui::FullOutput) -> Vec<egui::epaint::PaintCallback> {
        out.shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                egui::epaint::Shape::Callback(cb) => Some(cb.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn paint_view_ui_adds_one_paint_callback_for_the_rect() {
        let ctx = egui::Context::default();
        let state = FieldState {
            live: true,
            pans: vec![[0.0, 0.0, 1.0, 2.0, 30.0, 40.0, 0.0, 0.0]],
            fallback_pan: [0.5, -0.5],
        };
        let at = PanAt {
            pos: [10.0, 20.0],
            t: 2.0,
        };
        let rect = egui::Rect::from_min_size(egui::pos2(8.0, 8.0), egui::vec2(400.0, 300.0));
        let mut pans = Vec::new();
        let out = ctx.run_ui(headless_input(), |ui| {
            pans.push(paint_view_ui(ui.painter(), rect, &state, 2, &at, 0));
            // mode != 2 takes the fallback pan and paints a second callback.
            pans.push(paint_view_ui(ui.painter(), rect, &state, 0, &at, 0));
            // Out of range: nothing added.
            pans.push(paint_view_ui(ui.painter(), rect, &state, 2, &at, 0x20));
        });
        assert_eq!(pans, vec![Some([12.0, 24.0]), Some([10.0, 20.0]), None]);
        let cbs = callbacks(&out);
        assert_eq!(cbs.len(), 2);
        for cb in &cbs {
            // `new_paint_callback(rect, …)` stores exactly the rect it was given.
            assert_eq!(cb.rect, rect);
        }
    }

    #[test]
    fn paint_view_ui_skips_a_quad_outside_the_clip_rect() {
        let ctx = egui::Context::default();
        let state = FieldState {
            live: true,
            pans: vec![[0.0, 0.0, 1.0, 1.0, 10.0, 10.0, 0.0, 0.0]],
            fallback_pan: [1.0, 1.0],
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let inside = PanAt {
            pos: [100.0, 100.0],
            t: 1.0,
        };
        let outside = PanAt {
            pos: [5000.0, 5000.0],
            t: 1.0,
        };
        let mut res = Vec::new();
        let out = ctx.run_ui(headless_input(), |ui| {
            res.push(paint_view_ui(ui.painter(), rect, &state, 2, &inside, 0));
            // `b.hi 0x10096558c`: off-screen quads are not handed to the painter.
            res.push(paint_view_ui(ui.painter(), rect, &state, 2, &outside, 0));
        });
        assert_eq!(res, vec![Some([101.0, 101.0]), None]);
        assert_eq!(callbacks(&out).len(), 1);
    }

    #[test]
    fn paint_view_ui_keyed_uses_the_view_key() {
        let ctx = egui::Context::default();
        let state = FieldState::default();
        let at = PanAt {
            pos: [1.0, 2.0],
            t: 1.0,
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let mut got = None;
        let out = ctx.run_ui(headless_input(), |ui| {
            got = paint_view_ui_keyed(ui.painter(), rect, &state, 0, &at, 0, 0xfeed);
        });
        assert_eq!(got, Some([1.0, 2.0]));
        let cbs = callbacks(&out);
        assert_eq!(cbs.len(), 1);
        assert_eq!(cbs[0].rect, rect);
        // The callback itself is not inspectable through `PaintCallback`, so pin
        // the constructor the keyed entry point uses.
        let built = FieldCallback::new(
            0,
            0,
            paint_view_quad(&state, 0, &at, 0).unwrap(),
            0xfeed,
            at,
            state.clone(),
        );
        assert_eq!(built.key, 0xfeed);
        assert_eq!(built.piece, None);
        assert_eq!(built.piece_word(), NO_PIECE);
        assert_eq!(built.piece_parts(), None);
    }

    #[test]
    fn field_callback_is_send_sync_and_piece_split() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<FieldCallback>();
        let mut cb = FieldCallback::new(
            2,
            3,
            Rect::from_min_max(egui::pos2(1.0, 2.0), egui::pos2(3.0, 4.0)),
            7,
            PanAt {
                pos: [5.0, 6.0],
                t: 0.5,
            },
            FieldState::default(),
        );
        // `paint` bails on `-1`; anything else is split into (magnitude, sign).
        assert_eq!(cb.piece_parts(), None);
        cb.piece = Some(0x8000_0005);
        assert_eq!(cb.piece_parts(), Some((5, 1)));
        cb.piece = Some(9);
        assert_eq!(cb.piece_parts(), Some((9, 0)));
        assert_eq!(cb.piece_word(), 9);
        assert_eq!(cb.mode, 2);
        assert_eq!(cb.index, 3);
        assert_eq!(cb.key, 7);
        assert_eq!(cb.at.t, 0.5);
        assert_eq!(cb.quad.max.x, 3.0);
    }

    #[test]
    fn init_ctx_round_trips_through_the_context() {
        let ctx = egui::Context::default();
        assert_eq!(field_state(&ctx), None);
        let state = FieldState {
            live: true,
            pans: vec![[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0]],
            fallback_pan: [0.25, 0.5],
        };
        init_ctx(&ctx, state.clone());
        assert_eq!(field_state(&ctx), Some(state.clone()));
        assert_eq!(field_state_id(), egui::Id::new(FIELD_TINT));
        // Overwriting replaces the value.
        let other = FieldState::default();
        init_ctx(&ctx, other.clone());
        assert_eq!(field_state(&ctx), Some(other));
    }

    #[test]
    fn field_gpu_records_views_per_key_and_flushes() {
        let gpu = FieldGpu::new(init(&mut NullGpu, 8, 8));
        let screen = ScreenDescriptor {
            size_in_pixels: [800, 600],
            pixels_per_point: 2.0,
        };
        let at = PanAt {
            pos: [10.0, 20.0],
            t: 2.0,
        };
        let quad = paint_view_quad(
            &FieldState {
                pans: vec![[0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0]],
                ..Default::default()
            },
            2,
            &at,
            0,
        )
        .unwrap();
        let a = FieldCallback::new(2, 0, quad, 1, at, FieldState::default());
        let mut b = a.clone();
        b.key = 2;
        b.at.t = 0.25;
        assert_eq!(gpu.record_view(&a, &screen), 0);
        assert_eq!(gpu.record_view(&b, &screen), 0);
        {
            let f = gpu.frame();
            assert_eq!(f.views.len(), 2);
            assert_eq!(f.views[0].key, 1);
            assert_eq!(f.views[1].key, 2);
            assert_eq!(f.views[0].screen_px, [800, 600]);
            assert_eq!(f.views[0].pixels_per_point, 2.0);
            assert_eq!(f.views[0].quad, quad);
            assert_eq!(f.views[1].at.t, 0.25);
        }
        // Same key again: replaced, not appended.
        assert_eq!(gpu.record_view(&a, &screen), 0);
        assert_eq!(gpu.frame().views.len(), 2);
        // `finish_prepare` sets `+0x509`; the next `prepare` flushes and bumps.
        gpu.end_frame();
        assert!(gpu.frame().flush_pending);
        assert_eq!(gpu.record_view(&a, &screen), 1);
        let f = gpu.frame();
        assert!(!f.flush_pending);
        assert_eq!(f.frame, 1);
        assert_eq!(f.views.len(), 1);
        assert_eq!(f.views[0].frame, 1);
        drop(f);
        // `paint_piece`'s draw record keeps the recovered argument split.
        gpu.paint_piece(&a, 5, 1);
        let f = gpu.frame();
        assert_eq!(f.draws.len(), 1);
        assert_eq!(f.draws[0].key, 1);
        assert_eq!(f.draws[0].magnitude, 5);
        assert_eq!(f.draws[0].sign, 1);
        assert_eq!(f.draws[0].mode, 2);
        assert_eq!(f.draws[0].quad, Some(quad));
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
