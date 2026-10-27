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

## Standard-library departure

The original used `wgpu`, `egui`/`egui_wgpu`, `image`, `tracing`, `parking_lot`,
`toml_edit` and `gs_puppet` (mesh/pose/splat/uv model). None are available here
(no network, no vendored cache), so:

* all GPU calls go through the no-op seam `gs_render::gpu::Gpu` with the
  reconstructed descriptors `TextureUpload` / `DrawCall` (`NullGpu` is the
  default implementation);
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
| `PuppetPlacement::paint_figure` | `0x1008feac0` | 220 | **established** (quad math) |
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
| `field_gpu::paint_view` | `0x100965394` | 568 | partly (lerp established) |
| `field_gpu::init` | `0x100969f78` | 620 | partly (wgpu) |
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
4. `field_gpu::paint_view` — only the pan lerp is recovered; the rest of the
   function locks the field state and records draws (seam).
5. `field_gpu::init`, `skin_gpu::upload_mipped`, `SkinGpu::*`, `Gpu::*`,
   `RelightGpu::*` — `wgpu` pipelines; only descriptors are reconstructed.
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

## Integration gap — `gs_puppet::splat::SplatParams`

`<gs_render::puppet_cache::SplatKey as From<gs_puppet::splat::SplatParams>>::from`
is the only symbol in this crate that *names* a `gs_puppet` type in its signature
(`0x1008f1cb4`, 180 bytes; the demangled-size heuristics report 8468 because the
next **global** symbol is far away, but the next local symbol is
`drop_in_place<MutexGuard<MarkedJobs>>` at `0x1008f1d98`).

`gs_puppet` is being reconstructed in a sibling worktree and its
`SplatParams` does not exist on main, so `cargo check` would fail if this crate
depended on it. Therefore:

* `SplatKey` itself **is** recovered (`pub struct SplatKey(pub u64)`, the exact
  quantisation and packing, plus `size()/width()/height()` accessors);
* the conversion is exposed as two seams:
  `SplatKey::from_params_parts(a: f32, b: f32, c: f32)` (the exact body of the
  `From` impl) and `SplatKey::from_params([f32; 3])`;
* the parent wires the real impl after merging:

```rust
// crates/gs_render/src/puppet_cache.rs, after gs_puppet lands:
impl From<gs_puppet::splat::SplatParams> for gs_render::puppet_cache::SplatKey {
    fn from(p: gs_puppet::splat::SplatParams) -> Self {
        // The `From` body reads +0x00, +0x04, +0x08 of SplatParams as f32.
        SplatKey::from_params_parts(p.??? /* +0x00 */, p.??? /* +0x04 */, p.??? /* +0x08 */)
    }
}
```

The only constraint on `SplatParams` is that the three `f32`s the conversion reads
are at offsets `+0x00`, `+0x04`, `+0x08` — scaled by `1000.0`, `100.0`, `100.0`
and clamped to `5..1000`, `20..200`, `5..100` respectively. `gs_puppet`'s own
recovery notes call those fields `size`, `width`, `height` (the `SplatKey`
accessors use the same names).

Other `gs_puppet` types referenced by this crate's functions (redefined locally,
each marked `// INTEGRATION:`): `skin::SkinSurface`, `skin::Effects`,
`pose::{Pose, PoseSet}`, `camera::Camera`, `mesh::{Mesh, BodyMeta, SplatBody,
Hollows}`, `layers::{LayerCloud, Hollows}`, `uv::{UvLayout, Texture}`,
`shadow::ShadowCaster`, `fall::Fall`, `hang::Puppet::arm_hangs_at`,
`model::Puppet`.

## Test coverage

`cargo test -p gs_render` → **57** unit tests (plus one `ignore`d doc example that
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
derived ramp, `paint_view`'s lerp, `paint_figure`'s quad and the cache's
generation/retain behaviour.
