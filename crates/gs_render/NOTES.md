# `gs_render` — recovery notes

Reconstructed from the arm64 Mach-O binary `/Users/cpmbpro/Code/hydra-inspect/hydra`
(`hydra/0.1.0-beta.7`, `dev.hydrafe.desktop`, no DWARF). All addresses are the
unslid virtual addresses in that file.

Tooling used: `.work/recon.py` (syms/sizes/disas/bytes/findstr) plus
`nm -an hydra | rustfilt` to get the **local** symbols, which give the true
extent of every function (the demangled globals list only gives the next *global*
symbol, which is often far away — e.g. `PuppetKey::new` looks like 4408 bytes in
the global list but is 576 bytes in the local one).

## Legend

| tag | meaning |
|---|---|
| **established** | read directly out of the instruction stream / literal pool. |
| **INFERRED** | behaviour constrained by evidence (offsets, constants, call shapes) but a source-level detail was chosen. |
| **UNKNOWN** | the binary does not pin it down; a safe non-invented default was used and is marked in the code with `// UNKNOWN:`. |

## Dependency reality (Phase 4)

~~The original used `wgpu`, `egui`/`egui_wgpu`, `image`, `tracing`, `parking_lot`,
`toml_edit` and `gs_puppet` (mesh/pose/splat/uv model). None are available here
(no network, no vendored cache), so all GPU calls go through the no-op seam
`gs_render::gpu::Gpu`.~~

**Struck in Phase 4.** `egui`, `epaint`, `egui-wgpu` and `wgpu` are now real
dependencies at the binary's own git rev (`docs/EGUI.md` §4.2) and both draw
seams are egui code — see § "Phase 4: real egui" below. What remains departed:

* the *pipelines* (vertex/index buffers, bind groups, render targets, shader
  modules) still only exist as the descriptor seam `gs_render::gpu::Gpu` with
  the reconstructed descriptors `TextureUpload` / `DrawCall` (`NullGpu` is the
  default implementation): creating any of them needs a live `wgpu::Device`,
  which neither this crate nor its headless tests create;
* `std::collections::HashMap`/`std::sync::Mutex` replace `hashbrown`/`parking_lot`;
* `thread_local` `OnceLock`+`OnceBox` statics became plain `static … = Mutex::new(Vec::new())`
  (the binary's `skin_gpu::jobs::JOBS` / `puppet_cache::marked_jobs::JOBS` are
  literally `Mutex<Vec<…>>` behind a lazy cell; `Vec::new()` is const);
* the model types that came from `gs_puppet` are redefined locally with the field
  offsets seen in the disassembly and marked `// INTEGRATION:`.

`gs_base` **is** a validated path dependency: the binary calls
`gs_base::body::part_label` (`bl` at `0x1008f4610`, `0x1008fe3f8`, `0x100902494`)
and `gs_base::status::Statuses::{has_name, resolved}` (`0x1008f41b4`,
`0x10090231c`), and that crate is already recovered on main.

## Function table

Local-symbol extents (start..end, size in bytes) and status.

| symbol | address | size | status |
|---|---|---|---|
| `<SplatKey as From<gs_puppet::splat::SplatParams>>::from` | `0x1008f1cb4` | 180 | **established** |
| `<Source as Default>::default` | `0x1008f3dc8` | 16 | **established** |
| `<PuppetKey as PartialEq>::eq` | `0x1008f3dd8` | 832 | **established** |
| `<NoSource as PuppetSource>::*` | `0x1008f4118`…`0x1008f4158` | 8–12 each | **established** |
| `PoseChoice::of_statuses` | `0x1008f4170` | 368 | **established** |
| `PoseChoice::of_statuses::{{closure}}` | `0x1008f42e0` | 148 | **established** |
| `PoseChoice::resolve` | `0x1008f4374` | 640 | **established** (names/order) |
| `nerve_rank` | `0x1008f45f4` | 96 | **established** |
| `PosedPuppet::heap_bytes` | `0x1008f4654` | 304 | partly (see below) |
| `PuppetCache::begin_frame` | `0x1008f4db4` | 496 | **established** (control flow) |
| `dead_choice` | `0x1008f4fa4` | 784 | mostly established |
| `breath_paced` | `0x1008f52b4` | 620 | partly |
| `death_flinch` | `0x1008f5520` | 384 | **established** (shape) |
| `posed_cached` | `0x1008f56a0` | 2652 | partly |
| `posed_surface` | `0x1008f60fc` | 12 | **established** (`w3=1, w4=1`) |
| `posed_falling_as` | `0x1008f6108` | 1368 | partly |
| `posed_changing_as` | `0x1008f6660` | 1460 | partly |
| `posed_flinching_as` | `0x1008f6c14` | 1452 | partly |
| `flinch` | `0x1008f71c0` | 784 | **established** (shape) |
| `motion` | `0x1008f74d0` | 740 | partly |
| `puppet` | `0x1008f77b4` | 4148 | partly (delegates to `gs_puppet`) |
| `texture` | `0x1008f87e8` | 3864 | partly (delegates to `gs_puppet::uv`) |
| `has_body` | `0x1008f9b10` | 1060 | mostly established |
| `last_hit` | `0x1008f9f34` | 292 | **established** (lookup) |
| `pose_set` | `0x1008fa058` | 2880 | mostly established |
| `creature_of` | `0x1008fab98` | 456 | **established** |
| `nerve_twitch` | `0x1008fe3c4` | 532 | **established** |
| `flinch_region` | `0x1008fe5d8` | 988 | **established** |
| `body_size_step` | `0x1008fe9b4` | 268 | **established** |
| `PuppetPlacement::paint_figure` | `0x1008feac0` | 220 | **established** (quad math + both egui paths) |
| `PuppetPlacement::paint_figure_in` | `0x1008feac0` | (same) | **established** (`Painter::image` / `Painter::extend`) |
| `PuppetKey::with_fresh` | `0x100901f2c` | 132 | established + 1 UNKNOWN |
| `PuppetKey::with_scars` | `0x100901fb0` | 208 | **established** |
| `PuppetKey::breath_pace` | `0x100902080` | 216 | **established** |
| `PuppetKey::with_breath` | `0x100902158` | 64 | **established** |
| `PuppetKey::with_layers` | `0x100902198` | 284 | **established** |
| `PuppetKey::with_modelled` | `0x1009022b4` | 80 | **established** |
| `PuppetKey::with_statuses` | `0x100902304` | 284 | established (set) + UNKNOWN (lane order) |
| `PuppetKey::new` | `0x100902420` | 576 | **established** |
| `size_step` | `0x100903558` | 212 | **established** |
| `figure_image::ground_geo` | `0x1009644cc` | 732 | **established** |
| `figure_image::skin_image` | `0x1009647a8` | 2276 | partly (basis UNKNOWN) |
| `figure_image::with_glow` | `0x10096508c` | 556 | **established** |
| `field_gpu::FieldLight::moon` | `0x1009652b8` | 220 | **established** |
| `field_gpu::paint_view` | `0x100965394` | 568 | **established** (quad lerp, clip test, paint callback) |
| `field_gpu::paint_view_quad` / `paint_view_ui` | (same) | (same) | **established** (see Phase 4 §) |
| `field_gpu::init` | `0x100969f78` | 620 | partly (`Context::write` established, `Gpu::new` needs a device) |
| `field_gpu::init_ctx` / `field_state` | (same) | (same) | INFERRED (`IdTypeMap` key) |
| `<FieldCallback as CallbackTrait>::prepare` | `0x100963924` | 1684 | partly (resource lookup + view maintenance established) |
| `<FieldCallback as CallbackTrait>::finish_prepare` | `0x1009635bc` | 308 | **established** (`+0x509 = 1`) |
| `<FieldCallback as CallbackTrait>::paint` | `0x1009636f0` | 560 | partly (bail rules + `set_viewport` established) |
| `field_gpu::available` | `0x10096a990` | 4 | established (a single `b`) |
| `skin_gpu::job` | `0x10091d8fc` | 292 | **established** |
| `skin_gpu::submit` | `0x10091da20` | 520 | **established** |
| `SkinJob::depth_span` | `0x10092acec` | 496 | **established** |
| `skin_gpu::{upload_rgba, upload_mipped}` | `0x10091d228`, `0x10091d3a8` | 384, 1364 | partly (wgpu) |

Helpers recovered alongside (local symbols, `t`):

| symbol | address | role |
|---|---|---|
| `puppet_cache::PuppetCache::over` | `0x1008e8f84` | source swap |
| `puppet_cache::PuppetSource::{arm, dress, body_dir}` | `0x1008e9288`, `0x1008e94f8`, `0x1008fb748` | trait default bodies |
| `PuppetCache::bodies_for` | `0x1008f4784` | body table lookup |
| `fitted_body` | `0x1008fad60` | fit a body to a size step |
| `layer_cloud` | `0x1008fb168` | `LayerCloud::fit_bound/build` |
| `build_marked` | `0x1008fb754` | background job builder |
| `pose_puppet_inner` | `0x1008feb9c` | the actual posing |
| `held` / `breathe` / `body_now` / `flinched` | `0x1009012a0`, `0x100901584`, `0x1009017f0`, `0x100901d70` | pose modifiers |
| `load_body` | `0x100902660` | `Mesh::load_obj` + `body.obj`/`body_thin.obj` |
| `skin_gpu::SkinGpu::{new, draw, finish, draw_body}` | `0x10091dda4`, `0x10091fdd4`, `0x100925adc`, `0x1009261c0` | wgpu pipelines |
| `field_gpu::Gpu::{new, upload_frame, prepare_piece, paint_piece, bounce_pass}` | `0x100968540`… | wgpu pipelines |
| `field_gpu::{hex_lin, Textures::{new, upload}}` | `0x10096a1e4`, `0x10096a520`, `0x10096a7f8` | colour conversion, texture cache |
| `relight_gpu::*` | `0x10096d2cc`… | not part of the stub set |

## Recovered constants (the acceptance evidence)

### `puppet_cache`

| constant | evidence | value |
|---|---|---|
| `SplatKey` scale/clamps | `0x1008f1cb4`…`0x1008f1d60` | size `round(a*1000).clamp(5,1000) as u16`, width `round(b*100).clamp(20,200) as u16`, height `round(c*100).clamp(5,100) as u8`; packed `a \| b<<16 \| c<<32` |
| `PuppetKey` size | `PuppetKey::new` writes up to `+0x77`; every builder copies `0x78` | `120` |
| per-part arrays | `new` (max 3) and `with_scars` | `[u8; 14]` at `+0x28`, `+0x36`, `+0x4c`, `+0x5a` |
| `effects` | `with_statuses` stores 16 bytes at `+0x0c` + 4 bytes at `+0x1c` | 20 flag bytes |
| status bits expanded | shift table `0x1021b4c70..0x1021b4d10` | `7,8,9,10,11,12,13,14,15,16,17,18,19,20,26,28,29,31` (+2 padded lanes) |
| default layer paces | `mov #0x1e` / `movk #0x3c,lsl16` / `mov #0x5a` | `[30, 60, 90]` |
| `breath` sentinel | `tst w1, #0xf0; csel … #0x10` | `0x10` |
| `breath_pace` blocked mask | `mov w10, #0x6320; tst x9, w10` | `0x6320` = bits 5,8,9,13,14 |
| `breath_pace` rates | `0x4004cccccccccccd` 2.6 / `0x3ff33333` 1.9; `0x400199999999999a` 2.2 / `0x3fc00000` 1.5; `0x3fdccccccccccccd` 0.45 / 1.0; `0x3ffccccccccccccd` 1.8 / `0x3f99999a` 1.2; else `1.0/1.0` | see table |
| `with_fresh` window | `fccmp d0, d1(4.0), #0, ge` + `0x3fa999999999999a` = 0.05 | `0 ≤ age < 4`, step `min(age/0.05, 255)` |
| `with_modelled` step | `tst w2, #0xfe; csel … #0x2` | `0` if modelled, else `step` when `step&0xfe == 0` else `2` |
| `with_layers` packing | `0x42c80000` 100.0, `0x447a0000` 1000.0, `0x44fa0000` 2000.0 | period `round(a*100).clamp(0,100)`, share `round(b*1000).clamp(0,2000)`, side `1|2` |
| `size_step` steps | `0x100903580`…`0x100903628` | `[24, 36, 52, 76, 110, 160, 240, 360]` |
| `body_size_step` extra | `mov w0, #0x208` / `#0x2f8` | `520`, `760` |
| non-finite height | `and w8,#0x7fffffff; cmp w8,#0x7f800000; fcsel … 0x42800000` | replaced with `64.0` |
| `creature_of` delimiters | `memchr_aligned` with `0x23`, `0x40` | `'#'` … `'@'` |
| `nerve_twitch` hash | `0x811c9dc5`, `mul … #0x1000193`, then `h^=h>>13; h*=0x5bd1e995; (h>>8)^(h>>23)` | FNV-1a-32 + mix |
| `nerve_twitch` phase | `ucvtf s0, w8, #0x18`, `0x400d99999999999a` = 3.7, `0x3fe1999999999001` ≈ 0.55 | `f = fract(phase/3.7 + h/2^24)`, fire when `f*3.7 < 0.55` |
| `nerve_twitch` hands | `part_label("leftHand"…"rightHand")`, parity of `floor(phase/3.7+u)` | even → left, odd → right |
| `flinch_region` table | literals `0x1021b4b45`…`0x1021b4c05` | head/eye→head, arm→arm, leg→leg, neck, chest, back, hand, abdomen |
| `PoseChoice` candidates | tables `0x1043ae650..0x1043ae860` | see `PoseChoice::candidates` |
| `PoseChoice` jump table | `0x1021b46e8` = `3b 00 16 1a 04 24 29 1f 2e 08` | 10 offsets → the 10 arms |
| `dead_choice` hash clamp | `mov w8, #0xff; csel x19, x0, x8, lo` | `dead_hash.min(0xff)` |
| `hits` map entry stride | `mov w10, #0x60; mneg` | `0x60` |
| `marked` job entry stride | `add x8, x8, x8, lsl #2; lsl x24, x8, #4` | `80` bytes, `+0x40` id, `+0x49` flag |

### `figure_image`

| constant | evidence | value |
|---|---|---|
| ground vertex | `mov.s v0[1]/[2]/[3]` | `[0.0, u, v, depth]` |
| depth epsilon | `0x38D1B717` | `1e-4` |
| depth clamp | `0x42C80000` | `100.0` |
| uv fallback | `fcsel` with `#-1.0` / `#0.0` | `(-cos, -sin)` or `(0.0, -1.0)` |
| sin/cos | `bl __sincosf_stret` at `0x100964514` | `f32::sin_cos()` |
| skin texel stride | `x26 = i*4` | 4 bytes RGBA |
| alpha source | `ldrb w24, [x28, x26]` with `x28 = ptr+3` | `pixels[i*4+3]` |
| vertex buffer stride | `add x20, x20, #0x10` | 16 bytes |
| `with_glow` | `out[i*4+3] = glow[i]` | fully recovered |

### `skin_gpu`

| constant | evidence | value |
|---|---|---|
| `Vertex80` stride | `mov w8, #0x50; madd x8, x22, x8, x21` | `0x50` = 80, pos at `+0x18` |
| `Vertex84` stride | `add x10, x9, #0x54` | `0x54` = 84, pos at `+0x34` |
| point payload | `add x9, x21, #0x98` / `add x8, x21, #0xf8` | `[Vec3; 8]` (12-byte elems) |
| job table cap | `cmp x23, #0x5f` + `Vec::remove(0)` | 96 entries, stride 16 |
| job entry | `lsl x10, x10, #4` | `(u64, Arc<SkinJob>)` |
| depth scale | `ldr s12, [x20, #0xa0]` | `f32` at `+0xa0` |
| depth seeds | `0x7f7fffff` / `0xff7fffff` | `+inf` / `-inf` |
| `upload_rgba` pitch | `mul w8, w25, w24` | `width * 4` |

### `field_gpu`

| constant | evidence | value |
|---|---|---|
| `FieldLight` size | last byte written is `+0x6d` | `0x6e` = 110 |
| intensity clamp | `fcsel` with `#0.0` / `#1.0` | `0..=1` |
| `inner` | `0x3d4ccccd` (0.05), `0x3ca3d70a` (0.02) | `0.05*i + 0.02` |
| `outer` | `#-0.5`, `0x3e6147ae` (0.22) | `(1 - 0.5*i) * 0.22` |
| `window` | `0x10207d4c0` = `00 00 00 00 00 00 80 3f` | `[0.0, 1.0]` |
| `falloff` | `0x1021d7f70` = `0.2, 0.1, 0.2, 0.8` | `[0.2, 0.1, 0.2, 0.8]` |
| `reach` | `0x1021d7f48` = `0x41c00000`, `0x42a00000` | `[24.0, 80.0]` |
| `power` | `mov w9, #0x3f800000` | `1.0` |
| `kind` | `mov w9, #0x2` | `2` (moon) |
| `flags` | `mov w9, #0x101` / `sturh` | `[1,1,1,1]` |
| texture label tint | literal `0x1021d7f1a` | `"#c9d4ff"` |
| pan table | `add x9, x9, x21, lsl #5`, `cmp x9, #0x20` | 32-byte entries, ≥ 32 slots |

## Unknowns left in the code

1. `PuppetKey::with_fresh` — the `else` path stores an uninitialised `w10` into
   `+0x71`; `0` is used.
2. `PuppetKey::with_statuses` — the *lane order* of the Neon gather was not
   decoded instruction by instruction; the status set is established, the order
   is ascending-bit.
3. `figure_image::skin_image` — the 3×3 light basis at `0x100964898..0x100964990`
   was not decoded lane by lane (the arithmetic is listed below); a documented
   `N·L` clamp stands in for it.
4. `field_gpu::paint_view` — the quad lerp, the clip test and the
   `new_paint_callback` call are recovered; **what the callback draws**
   (`Gpu::{upload_frame, bounce_pass, prepare_piece, paint_piece}`) is not: those
   build and bind wgpu pipelines. `paint` always bails for a `paint_view`
   callback anyway (`+0x3c == -1`, see the Phase 4 §), so on this path all the
   drawing happens in `prepare`.
5. `field_gpu::init`'s `Gpu::new` (1296 bytes of pipelines/targets),
   `skin_gpu::upload_mipped`, `SkinGpu::*`, `Gpu::*`, `RelightGpu::*` — `wgpu`
   pipelines; only descriptors are reconstructed.
6. The `+0x08` "`== 1`" equality rule for `PuppetKey` (and the same rule for
   `+0x6f`): implemented exactly, but what the field *means* is unknown.
7. `LastHit` field names (`at`, `part`, `strength`) are INFERRED from use;
   `flinch`/`death_flinch` only read the byte at `+0x08`.
8. `PuppetCache::posed_cached`'s `w3` argument is passed straight through
   `posed_with` (`mov w4, #0x0; b posed_cached`), so `posed_with`'s own flag is
   not recoverable; it is modelled as `0`.

### The `skin_image` basis (for whoever wants to finish it)

```text
l = surface.light;  negz = -0.0
t3 = l.z;  t1 = -0.0;  t0 = -l.x
len = sqrt(t3*t3 + t1*t1 + t0*t0).max(1e-6)
a = t3/len; b = t1/len; c = t0/len
row_a = (l.y*a - l.z*b, l.z*b - l.y*c, -l.x/len)
row_b = (l.y*a, 0.0, -len)
```

## Phase 4: real egui

The two no-op draw seams are gone; `egui`, `epaint`, `egui-wgpu` and `wgpu` are
real dependencies (`Cargo.toml`, the binary's git rev per `docs/EGUI.md` §4.2).

### What replaced what

| removed seam | replaced by | evidence |
|---|---|---|
| `field_gpu::paint_view` returning data and calling `gpu.draw(..)` | [`paint_view_ui`] / [`paint_view_ui_keyed`]: recovered quad maths → clip test → `Painter::add(egui_wgpu::Callback::new_paint_callback(rect, FieldCallback))` | `0x100965394`, calls at `0x10096557c` (`new_paint_callback`) and `0x100965588` (`Painter::add`) |
| nothing (no `CallbackTrait` impl existed) | `FieldCallback` + `impl egui_wgpu::renderer::CallbackTrait` (`prepare`, `finish_prepare`, `paint`) | `0x100963924`, `0x1009635bc`, `0x1009636f0` |
| `field_gpu::init(gpu, w, h)` only creating descriptors | `init_ctx(&egui::Context, FieldState)` storing the state in the context + `field_state(&ctx)` reader | `init`'s tail: `bl egui::Context::write` (`0x10096a108`) |
| `PuppetPlacement::paint_figure` returning a `FigureQuad` | `paint_figure_in(&Painter, &FigureCtx, TextureId, &[Shape])`: `Painter::image(texture_id, rect, uv, Color32::WHITE)` or, on the three `cbz` guards, `Painter::extend(cached)` | `0x1008feac0`; `bl egui::painter::Painter::image` at `0x1008feb44`, `bl …::extend` at `0x1008feb68`, uv literal `0x102073df0`, tint `mov w5, #-1` |

The seam types themselves (`gpu::{Gpu, NullGpu, TextureUpload, DrawCall,
TexFormat}`) are kept — they are recovered evidence for the *pipeline* half, and
the existing tests assert them. `paint_view` and `paint_figure` keep their old
signatures and now delegate to the recovered maths.

### The recovered `field_gpu::paint_view` body (newly decoded)

`0x100965394` is a free function, not a method:
`paint_view(painter: &Painter /*x0*/, field: &Arc<Field> /*x1*/, at /*x2*/,
mode: u64 /*x3*/, index: usize /*x4*/, view_key: u64 /*x5*/)`, called from
`injury_doll::view::paint_in_view` (`bl 0x100965394` at `0x1008d11ac`, with
`mode = 0` and `x5` = the caller's entity id). The instruction stream:

```text
if mode == 2 {                                   // cmp x22, #0x2; b.ne
    if index >= max(len, 0x20) { unlock; return }// cmp x21, x9; b.hs
    (off, size) = (entry[2..4], entry[4..6])     // ldp d1, d2, [x9, #0x8]
} else {
    min = pos; max = pos + fallback_pan * t      // fmul s1, s0, s1 with a zeroed s1
}
min  = pos + off  * t                            // fmul.2s / fadd.2s
max  = min + size * t
if !(max.x >= clip.min.x && max.y >= clip.min.y
     && min.x <= clip.max.x && min.y <= clip.max.y) { return }   // Rect::intersects
painter.add(Callback::new_paint_callback(Rect { min, max }, FieldCallback { .. }))
```

Two corrections to the first-pass reconstruction, both now pinned by tests:

* the 32-byte pan entry is read at **`+0x08` and `+0x10`** (floats 2,3 = offset
  and 4,5 = size), not at `+0x00`. `ldp d1, d2, [x9, #0x8]` loads the offset
  into `d1` and the size into `d2`;
* the lerp produces a **rect**, not a point: `max = min + size * t`. The value
  `paint_view` used to return was `min`; the fallback pan is a *size*, so for
  `mode != 2` the pan is exactly `at.pos`.

### Exact new public API

```rust
// field_gpu
pub fn paint_view_quad(state: &FieldState, mode: u64, at: &PanAt, index: usize) -> Option<egui::Rect>;
pub fn paint_view_ui(painter: &Painter, rect: Rect, state: &FieldState, mode: u64,
                     at: &PanAt, index: usize) -> Option<[f32; 2]>;
pub fn paint_view_ui_keyed(painter: &Painter, rect: Rect, state: &FieldState, mode: u64,
                           at: &PanAt, index: usize, key: u64) -> Option<[f32; 2]>;
pub fn init_ctx(ctx: &egui::Context, state: FieldState);
pub fn field_state(ctx: &egui::Context) -> Option<FieldState>;
pub fn field_state_id() -> egui::Id;
pub const FIELD_STATE_KEY: &str;      // = FIELD_TINT ("#c9d4ff")
pub const NO_PIECE: u32;              // = u32::MAX, the `+0x3c` "no piece" word
pub struct FieldCallback { pub mode: u64, pub index: usize, pub quad: Rect, pub key: u64,
                           pub at: PanAt, pub piece: Option<u32>, pub state: FieldState }
impl FieldCallback { pub fn new(..) -> Self; pub fn piece_word(&self) -> u32;
                     pub fn piece_parts(&self) -> Option<(u32, u32)>; }
impl egui_wgpu::renderer::CallbackTrait for FieldCallback { prepare, finish_prepare, paint }
pub struct FieldGpu { pub textures: Textures }         // the CallbackResources value
impl FieldGpu { pub fn new(Textures) -> Self; pub fn frame(&self) -> MutexGuard<FieldFrame>;
                pub fn record_view(&self, &FieldCallback, &ScreenDescriptor) -> u64;
                pub fn end_frame(&self); pub fn paint_piece(&self, &FieldCallback, u32, u32); }
pub struct FieldFrame { pub views: Vec<FieldView>, pub frame: u64,
                        pub flush_pending: bool, pub draws: Vec<PieceDraw> }
pub struct FieldView { key, screen_px, pixels_per_point, quad, at, mode, index, frame }
pub struct PieceDraw { key, magnitude, sign, mode, quad: Option<Rect> }
impl FieldState { pub fn pan_entry(&self, usize) -> [f32; 8];
                  pub fn pan_offset(&self, usize) -> [f32; 2];
                  pub fn pan_size(&self, usize) -> [f32; 2]; }

// puppet_cache
impl FigureQuad { pub fn rect(&self) -> egui::Rect; pub fn uv_rect(&self) -> egui::Rect;
                  pub fn color(&self) -> egui::Color32; }
impl PuppetPlacement {
    pub fn paint_figure_in(&self, painter: &egui::Painter, ctx: &FigureCtx,
                           texture: egui::TextureId, cached: &[epaint::Shape]) -> Option<FigureQuad>;
}
```

Everything that existed before still compiles unchanged (`paint_view`,
`paint_figure`, `init`, `available`, the `gpu::*` descriptors).

### `FieldCallback` — the recovered 64-byte layout

`new_paint_callback` allocates `0x40` bytes and copies four 16-byte vectors out of
its argument (`0x1009602fc..0x10096031c`), so the binary's `FieldCallback` is 64
bytes; the three trait methods read that copy back at
`+0x00` mode, `+0x08` index, `+0x10` the field handle (`&Arc<Field>`, locked with
`Mutex::lock` in `prepare`), `+0x18` the lerped quad, `+0x28` the view key
(`hash_one(self + 0x28)` keys `HashMap<u64, View>`), `+0x30`/`+0x38` `at.pos` /
`at.t`, `+0x3c` the piece word.

`paint` bails on `+0x3c == 0xffff_ffff` (`cmn w24, #0x1; b.eq`), and `paint_view`
always stores `-1` there — so a `paint_view` callback never draws in `paint`; all
of that path's GPU work is in `prepare`. Anything else at `+0x3c` is split into
`(w24 & 0x7fff_ffff, w24 >> 31)` and handed to `Gpu::paint_piece` together with
`self.mode`; what those two numbers *mean* is not recoverable
(`// UNKNOWN:` — modelled as `Option<u32>` = `[piece word | sign bit]`).

The three methods all start with `callback_resources.get::<Gpu>()` and return an
empty `Vec` when the resource is absent (`0x100963a50`/`0x100963a98`,
`0x100963650`, `0x1009637fc`) — `FieldGpu` is that resource here.

### Remaining UNKNOWNs (this phase)

1. **No pipeline is created.** `Gpu::{new, upload_frame, bounce_pass,
   prepare_piece, paint_piece}` need a `wgpu::Device`/`Queue`; the crate still has
   no device and the tests never open a window. `FieldGpu` records exactly what
   those calls are *given* (`FieldView`, `PieceDraw`) and `paint` still issues the
   recovered `RenderPass::set_viewport(0, 0, screen_px.x, screen_px.y, 0, 1)`.
2. **`View`'s 864 bytes.** Only the fields `prepare` visibly writes are modelled
   (`FieldView`); the `kind` at `+0x324` (compared with `3`) and the rest are not.
3. **The `+0x3c` piece word's meaning** (index? reverse flag?) — see above.
4. **The `egui::Context` data key.** `init`'s `Context::write` closure captures
   nothing; the `Id` used here is `Id::new(FIELD_TINT)`, `// INFERRED:`.
5. **`FieldCallback` holds a `FieldState` snapshot**, not the binary's
   `&Arc<Field>` handle: the struct is therefore bigger than 64 bytes and
   `prepare` does not lock a mutex. `// INFERRED:`, chosen so the type is
   `'static + Send + Sync` and the headless tests can inspect what was handed
   over.
6. **`paint_view_ui`'s `rect` parameter.** The binary passes the *lerped quad*
   to `new_paint_callback`. Here the caller supplies the field view's rect (as
   the Phase-4 task asks) and the lerped quad travels inside the callback, where
   the binary also keeps it (`+0x18`) before handing it to `prepare_piece`. With
   `rect = paint_view_quad(..)` the two are identical.

### Tests added (all headless, `egui::Context` frames only)

67 tests total (57 before). New:

| test | values pinned |
|---|---|
| `paint_view_quad_is_min_plus_size_times_t` | `min = pos + off*t`, `max = min + size*t`; entry floats 2..6 (mutating 0/1 changes nothing); `mode != 2` → `min = pos`, `size = fallback_pan`; `pan_slots().max(0x20)` boundary at 0x1f/0x20 and a 0x21-entry table |
| `paint_view_ui_adds_one_paint_callback_for_the_rect` | one frame, two `Shape::Callback`s, each `PaintCallback::rect == the rect passed`; returns `Some([12,24])` (table) / `Some([10,20])` (fallback); out-of-range index adds nothing and returns `None` |
| `paint_view_ui_skips_a_quad_outside_the_clip_rect` | an off-screen quad returns `None` and produces no callback (one callback for the on-screen one) |
| `paint_view_ui_keyed_uses_the_view_key` | keyed entry adds one callback with the given rect, returns the lerp; `FieldCallback::new` keeps `key = 0xfeed`, `piece_word() == NO_PIECE`, `piece_parts() == None` |
| `field_callback_is_send_sync_and_piece_split` | `FieldCallback: Send + Sync`; `-1` → `None`; `0x8000_0005` → `(5, 1)`; `9` → `(9, 0)` |
| `init_ctx_round_trips_through_the_context` | `field_state` is `None` before, equals what `init_ctx` stored after (pans/fallback included), `field_state_id() == Id::new("#c9d4ff")`, overwrite replaces |
| `field_gpu_records_views_per_key_and_flushes` | frame id 0 for two keys, view carries `screen_px`/`pixels_per_point`/quad/`at.t`; same key replaces instead of appending; `end_frame` sets `flush_pending`, the next `record_view` clears it, bumps to frame 1 and drops the old views; `paint_piece` records `(magnitude, sign, mode, quad)` |
| `figure_quad_converts_to_egui_types` | `FigureQuad::rect() == (12,18)-(72,98)`, `uv_rect() == (0,0)-(1,1)`, `color() == Color32::WHITE`, `FIGURE_UV` / `FIGURE_TINT` literals |
| `paint_figure_in_paints_one_textured_quad` | **one** `Shape::Mesh`, `Mesh::texture_id == the loaded handle's id`, 4 vertices, indices `[0,1,2,2,1,3]`, `calc_bounds() == (12,18)-(72,98)`, vertex order `left_top/right_top/left_bottom/right_bottom` with the matching uv corners `(0,0)/(1,0)/(0,1)/(1,1)`, all vertex colours white, and the texture upload present in `textures_delta.set` |
| `paint_figure_in_falls_back_to_the_cached_shapes` | `width == 0` → `None`, **no** mesh, the two cached circles replayed at their centres/radii |

`paint_view_lerps_the_pan` was adapted to the corrected entry offsets (same
expected values).

## Integration gap — `gs_puppet::splat::SplatParams`

`<gs_render::puppet_cache::SplatKey as From<gs_puppet::splat::SplatParams>>::from`
is the only symbol in this crate that *names* a `gs_puppet` type in its signature
(`0x1008f1cb4`, 180 bytes; the demangled-size heuristics report 8468 because the
next **global** symbol is far away, but the next local symbol is
`drop_in_place<MutexGuard<MarkedJobs>>` at `0x1008f1d98`).

`gs_puppet` was reconstructed in a sibling worktree in parallel. It is merged
now, and this crate has a path dependency on it, so the real impl is present:

```rust
impl From<gs_puppet::splat::SplatParams> for gs_render::puppet_cache::SplatKey {
    fn from(p: gs_puppet::splat::SplatParams) -> Self {
        SplatKey::from_params_parts(p.radius, p.opacity, p.detail)
    }
}
```

* `SplatKey` itself **is** recovered (`pub struct SplatKey(pub u64)`, the exact
  quantisation and packing, plus `size()/width()/height()` accessors);
* `SplatKey::from_params_parts(a: f32, b: f32, c: f32)` is the exact body of the
  `From` impl and `SplatKey::from_params([f32; 3])` is its array form;
* the test `splat_key_quantises_like_the_from_impl` exercises both the raw parts
  and the real `From<SplatParams>`.

The three `f32`s the conversion reads are `gs_puppet::splat::SplatParams`'s
`radius`, `opacity` and `detail` at offsets `+0x00`, `+0x04`, `+0x08` — scaled by
`1000.0`, `100.0`, `100.0` and clamped to `5..500`, `20..200`, `5..100`
respectively (`0x43fa0000` is **500.0**, not 1000.0; the earlier note in this
file had that typo and the constant here is the corrected one).

Other `gs_puppet` types referenced by this crate's functions (redefined locally,
each marked `// INTEGRATION:`): `skin::SkinSurface`, `skin::Effects`,
`pose::{Pose, PoseSet}`, `camera::Camera`, `mesh::{Mesh, BodyMeta, SplatBody,
Hollows}`, `layers::{LayerCloud, Hollows}`, `uv::{UvLayout, Texture}`,
`shadow::ShadowCaster`, `fall::Fall`, `hang::Puppet::arm_hangs_at`,
`model::Puppet`.  These local models intentionally keep only the fields the
cache code reads (they are cache-key/placement views, not a second animation
engine); `splat::SplatParams` was the one type the binary names in a symbol, and
it is now the real sibling type.

Other `gs_puppet` types referenced by this crate's functions (redefined locally,
each marked `// INTEGRATION:`): `skin::SkinSurface`, `skin::Effects`,
`pose::{Pose, PoseSet}`, `camera::Camera`, `mesh::{Mesh, BodyMeta, SplatBody,
Hollows}`, `layers::{LayerCloud, Hollows}`, `uv::{UvLayout, Texture}`,
`shadow::ShadowCaster`, `fall::Fall`, `hang::Puppet::arm_hangs_at`,
`model::Puppet`.

## Test coverage

`cargo test -p gs_render` → **67** unit tests (plus one `ignore`d doc example that
shows the future `From` impl). They assert the recovered literals, not
tautologies: the `size_step`/`body_size_step` tables and their NaN/inf handling,
the `PuppetKey` byte layout via `mem::offset_of!`, the recovered equality rules
(`pose_side` only for poses 8/9, the `== 1` rules, the byte-wide third layer
pace), `breath_pace`'s five rows and six blocking statuses, `with_breath`'s
`0x10` sentinel, `with_modelled`, `with_fresh`, `with_layers`, the `Effects`
expansion, `PoseChoice::of_statuses` order and the ten candidate tables,
`resolve`'s search order, `SplatKey` quantisation/clamps/packing, `flinch_region`
for every table row, `creature_of`, the FNV-1a + mix vectors, the `nerve_twitch`
window, `ground_geo`'s depth math and uv fallback, `with_glow` (including the
empty and over-long cases), the skin sheet slicing, the vertex strides and the
96-entry job table, `depth_span` for all three payloads, `FieldLight::moon`'s
derived ramp, `paint_view`'s lerp and quad, `paint_figure`'s quad and the cache's
generation/retain behaviour.

The ten Phase-4 tests (§ "Tests added") drive real headless `egui::Context`
frames: they assert on the `Shape::Callback` / `Shape::Mesh` values that come out
of `Context::run_ui` (counts, rect coordinates, uv corners, texture ids, vertex
colours) and on the state `FieldGpu` keeps, never merely that a frame ran.

