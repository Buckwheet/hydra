# `gs_puppet` — recovery notes

Reconstructed from the stripped arm64 Mach-O binary of Hydra
(`hydra/0.1.0-beta.7`, `dev.hydrafe.desktop`), which has **no DWARF**.  Everything
below was read out of the instruction stream, the literal pools, the shared
string pool, the demangled globals and the local symbols (`nm -an | rustfilt`
gives many more local text symbols than `symbols_demangled.txt`, which is what
made the struct layouts recoverable via the drop glue).

The crate is `std`-only apart from `egui` + `epaint`, which arrived in Phase 4
for the draw seams only (see "Phase 4: egui" below): the binary has **no**
`gs_puppet` -> `egui` call edge, so nothing here is widget code.  The original
used `glam`, `wgpu`, `toml` + `serde`, `ruzstd`, `hashbrown` and `anyhow`; each is
replaced by a hand-written `std`-only equivalent and every departure is marked in
the code with `**std departure**`.

| tag | meaning |
|---|---|
| **established** | read directly out of the instruction stream / literal pool |
| **INFERRED** | constrained by evidence (strides, call shapes, message text) but the source-level detail had to be chosen |
| **UNKNOWN** | the binary does not pin it down; the safest non-invented behaviour was chosen and marked in the code |

---

## Legend of the biggest wins

1. **The shipped pose data was recovered whole.**  `PoseSet::shipped_toml`
   (`0x100a4a254`) walks a 13 × 32-byte `(&str, &str)` table at `0x1043b60d8`
   (0x1a0 bytes) and returns the TOML text for a body template.  All 13 files
   (biped, quadruped, avian, serpentine, multileg, winged_biped,
   winged_quadruped, mound, upright_insect, centaur, naga, fish, floating_orb)
   were dumped **verbatim** into `crates/gs_puppet/poses/*.toml` — ~68 KB of
   authored joint limits, poses, idle amplitudes, flinch peaks, guards and
   postures, with the file's own comments (which document the angle
   convention).  This is real data, not inference.
2. **`SplatParams` is pinned by the cross-crate `From` impl.**
   `gs_render::puppet_cache::SplatKey as From<gs_puppet::splat::SplatParams>`
   (`0x1008f1cb4`, 148 bytes) reads three `f32`s at offsets 0/4/8 and quantises
   them; see [`splat::SplatParams`] below.
3. **`Camera::project` is fully recovered** — 296 bytes, no ambiguity.

---

## Function table

### `math`

| symbol | address | status | recovered signature |
|---|---|---|---|
| `math::quat_from_m3` | `0x1009fe4c8` | established | `fn(&Mat3) -> Quat` |
| `math::quat_norm` (local) | `0x1009feab8` | established | `fn(Quat) -> Quat` |
| `math::quat_to_m3` (local) | `0x1009fe41c` | mostly established | `fn(&Quat) -> Mat3` |
| `math::slerp` | `0x1009fe8b0` | established | `fn(&Quat, &Quat, f32) -> Quat` |
| `math::Xf::dir` | `0x1009fe7d8` | established | `fn(&self, Vec3) -> Vec3` |
| `math::Xf::point` | `0x1009fe83c` | established | `fn(&self, Vec3) -> Vec3` |
| `math::Xf::then_inner` | `0x1009fe688` | composition established, SIMD unread | `fn(&self, &Xf) -> Xf` |

Recovered layout facts:

* `Vec3` = 3 × `f32`, 12 bytes (`ldr s` at `+0/+4/+8`).
* `Mat3` = 9 × `f32`, **row-major**: `quat_from_m3` reads the diagonal at
  `+0/+16/+32` and the off-diagonals as consecutive triples; `Xf::dir` computes
  `dst[i] = dot(m[3i..3i+3], v)`, which only works row-major.
* `Quat` = 4 × `f32` with **`w` first**.  Identity is `[1,0,0,0]` — the 16-byte
  constant at `0x1021b4d40` (`quat_norm`'s degenerate return *and*
  `quat_from_m3`'s fallback) and the pose-override default at `0x1021dc4a8`.
* `Xf` = 48 bytes: rotation at `+0` (row-major), translation at `+0x24`
  (`Xf::point` adds `+0x24/+0x28/+0x2c`; `then_inner` loads/stores three 16-byte
  vectors).
* `quat_from_m3`'s trace branch negates the vector part while the three
  diagonal-dominant branches do not; both are the same rotation up to sign, i.e.
  the conjugate of the column-major reading.  That is exactly the row-vector
  convention, and it is what makes `quat_to_m3(&quat_from_m3(m)) == m` hold (a
  test asserts the round trip).
* Constants: `1e-9` (`0x3089705f`) degenerate-length cutoff, `0.9995`
  (`0x3f7fdf3b`) nlerp threshold, `π/180` (`0x3c8efa35`) degrees→radians,
  `6.75`/`0.55` flinch, `2π` (`0x40c90fdb`) breathing.

### `model`

| symbol | address | status | recovered signature |
|---|---|---|---|
| `model::Puppet::subtree` | `0x1009d6734` | established | `fn(&self, usize) -> Vec<usize>` |
| `model::PuppetError as Display::fmt` | `0x1009c9c7c` | established (exists) | `impl Display` |

* `Puppet` holds a `Vec<Segment>` at `+0x30` (cap) / `+0x38` (ptr) / `+0x40` (len).
* `Segment` stride is **224 = 0xe0**; word 0 is a `bool` (`tbz w9, #0`) and
  `+0x08` is the **parent index** (`usize`).
* `subtree` seeds the result with the root and then walks `root+1..len`, keeping
  each segment whose parent is already in the set — the transitive descendant set,
  relying on parents-first ordering.  The NEON `cmeq` block is the `contains`
  test.

### `pose`

| symbol | address | status | recovered signature |
|---|---|---|---|
| `pose::flinch_envelope` | `0x100a45894` | **established** | `fn(f32) -> f32` |
| `pose::Limit::clamp` | `0x100a486a4` | established (shape + 52-byte return) | `fn(&self, Vec3, &[f32]) -> BonePose` |
| `pose::BonePose::lerp` | `0x100a4df44` | established (linear + `slerp`) | `fn(&self, &BonePose, f32) -> BonePose` |
| `pose::Idle::with_override` | `0x100a458d0` | established | `fn(&self, &Idle) -> Idle` |
| `pose::Pose::bone` | `0x100a46ccc` | established (pair fallback) | `fn(&self, &str) -> BonePose` |
| `pose::Pose::blend` | `0x100a470dc` | established | `fn(&self, &Pose, f32) -> Pose` |
| `pose::Pose::breathe` | `0x100a47a94` | established | `fn(&self, &Idle, f32, f32) -> Pose` |
| `pose::Pose::flinched` | `0x100a47cb0` | established | `fn(&self, &Pose, f32) -> Pose` |
| `pose::Pose::set_euler` | `0x100a482dc` | established (degrees, identity default) | `fn(&mut self, &str, Vec3)` |
| `pose::Pose::with_override` | `0x100a4689c` | established | `fn(&self, &PoseOverride) -> Pose` |
| `pose::PoseSet::shipped_toml` | `0x100a4a254` | **established (data recovered)** | `fn(&str) -> Option<&'static str>` |
| `pose::PoseSet::get` | `0x100a4a310` | established | `fn(&self, &str) -> Option<&Pose>` |
| `pose::PoseSet::clamp` | `0x100a4a3ec` | established | `fn(&self, &Pose) -> Pose` |
| `pose::PoseSet::limit` | `0x100a4a490` | partly | `fn(&self, &str) -> Limit` |
| `pose::PoseSet::guarded` | `0x100a4a8b4` | partly | `fn(&self, &Pose, &[&str]) -> Pose` |
| `pose::PoseSet::postured` | `0x100a4aa3c` | partly | `fn(&self, &Pose, &str) -> Pose` |
| `pose::PoseSet::dead_hash` | `0x100a4aba4` | partly | `fn(&self, &str) -> u64` |
| `pose::PoseSet::dead_name` | `0x100a4af50` | partly | `fn(&self, u64) -> &'static str` |
| `pose::PoseSet::for_family` | `0x100a4920c` | established outline | `fn(&str, &str) -> Result<PoseSet, _>` |
| `pose::PoseSet::from_toml` | `0x100a4aff4` | established schema | `fn(&str) -> Result<PoseSet, _>` |
| `pose::Posed::bounds` | `0x100a48d90` | established | `fn(&self) -> (Vec3, Vec3)` |
| `Puppet::pose` | `0x1009d2ef0` | established (12-byte tail call) | `fn(&self) -> Posed` |
| `Puppet::pose_as_drawn` | `0x1009d2ee4` | established (12-byte tail call) | `fn(&self) -> Posed` |
| `Puppet::pose_with` | `0x1009d4a4c` | outline established | `fn(&self, &PoseSet, &str, f32) -> Posed` |
| `Puppet::pin_arms` | `0x1009d2efc` | partly | `fn(&self, &mut Pose, &PoseSet) -> bool` |

`flinch_envelope` is recovered instruction-for-instruction:

```text
u = t / 0.55                       ; 0x3f0ccccd
(u < 1.0 && u >= 0.0) ? 6.75·u·(1-u)² : 0.0
```

The in-range test is the `fcmp` / `fccmp s0, s2, #0x8, mi` / `fcsel … ge`
triple — `u < 1.0` exclusive, NaN fails both arms.  The hump peaks at exactly
`1.0` when `u = 1/3`.  (The immediate `6.75` and the peak identity are asserted
in the tests.)

`BonePose` is **52 bytes**, from `Limit::clamp`'s stores:
`bend: Vec<f32>` at `+0`, `quat: Quat` at `+0x18`, `rot: Vec3` at `+0x28`.

`Pose::breathe` computes `gain · sin(2π(t + phase))` per idle bone (the `2π`
immediate `0x40c90fdb`, the phase `fadd`, then `sinf`), and returns the pose
untouched when `t` is not finite or `gain == 0.0`.

### `splat` (the `gs_render` seam)

`SplatParams` — **recovered in full** from
`gs_render::puppet_cache::SplatKey as From<SplatParams>`:

```rust
pub struct SplatParams {
    pub radius: f32,   // +0, world-space size, 0.005..=0.5
    pub opacity: f32,  // +4, multiplier,       0.2  ..=2.0
    pub detail: f32,   // +8, kept fraction,    0.05 ..=1.0
}
```

and the `SplatKey` packing that impl performs (reproduced exactly by
`SplatParams::quantised` / `key_bits`):

```text
a = clamp(round(radius  * 1000.0), 5.0, 500.0) as u32, capped at 0xffff
b = clamp(round(opacity *  100.0), 20.0, 200.0) as u32, capped at 0xffff
c = clamp(round(detail  *  100.0), 5.0, 100.0) as u32, capped at 0xff
key = a | b << 16 | c << 32
```

with the immediates `0x447a0000` (1000.0), `0x43fa0000` (500.0), `0x42c80000`
(100.0), `0x43480000` (200.0), `0xffff`, `0xff` and `frinta` (round-half-away).

| symbol | address | status |
|---|---|---|
| `splat::cloud_file` | `0x1009fef00` | **established** (`{stem}.spz` if it exists, else `{stem}.ply`) |
| `splat::decode_spz` | `0x1009ff05c` | header/TOC **established**, stream payload partly |
| `splat::load_cloud` | `0x100a00290` | established (dispatch on `.ply`) |
| `splat::load_ply` | `0x100a02a90` | header/errors established, payload INFERRED |
| `splat::cloud` | `0x100a00948` | partly (samples the UV atlas per splat) |
| `splat::prepare` | `0x100a01c14` | partly (project + far-to-near) |
| `splat::render` | `0x100a01444` | **GPU seam** |
| `splat::BoundCloud::posed` | `0x1009fed98` | outlined established |

SPZ **version 4** header (32 bytes), validated in this order:
`len >= 0x20` (`"too short for a header"`), magic `NGSP` (else gzip magic
`1f 8b` → `"a gzip spz (version 1..3) is not read"`, else `"not an spz file (no
NGSP magic)"`), `version == 4` (else `"version N is not read, only 4"`),
`sh_degree < 5` (else `"harmonics degree N is not read, only 4"`), then
`num_points` (u32 @8), `fractional_bits` (u8 @0x0d), `flags` (@0x0e), stream
count (@0x0f), TOC offset (u32 @0x10).

Table of contents: `streams` records of `{u64 compressed_len, u64 raw_len}` at
`toc_offset`; the raw lengths must equal `9n`, `3n`, `4n`, `sh·3n` where
`sh ∈ {0,3,8,15}` for degree `{0,1,2,3}` (the `u64` table at `0x1021dc2a8`).
Position scale is `2^-fractional_bits`, built bit-exactly as
`(min(fb,31) << 23) ^ 0x3f800000`.  Decoded splats have a **92-byte stride**
(`0x5c`); positions store `x, -y, -z` (two `fnmul`s), and the rotation is
reconstructed with the smallest-three `1/√2` (`0x3f3504f3`) constant before
being written as a 3×3 basis at `+0x0c`.

### `mesh`, `uv`, `camera`, `draw`, `skin`, `shadow`, `layers`, `fall`, `hang`, `joints`, `baked`

| symbol | address | status |
|---|---|---|
| `mesh::load_weights` | `0x1009ddc78` | format INFERRED (`weights_thin.txt` literal survives) |
| `mesh::nearest_vertices` | `0x1009de5b0` | established shape (`Vec<Option<u32>>`, threaded grid) |
| `mesh::Mesh::fit_to_height` | `0x1009de744` | established intent (`0.5` centring) |
| `mesh::Mesh::smooth_normals` | `0x1009deaa4` | established intent |
| `mesh::Mesh::load_obj` | `0x1009dede4` | established shape (OBJ + hash-map merge) |
| `mesh::BodyMesh::made_splats` | `0x1009dfe9c` | established (`OnceLock` at `+0x100`) |
| `mesh::BodyMesh::splats_at` | `0x1009dfef4` | **established** (`< 0x6e` → half 0, `<= 0xef` → half 1, else `None`) |
| `mesh::BodyMeta::read` | `0x1009dff7c` | file format UNKNOWN |
| `mesh::BodyFrame::new` | `0x1009e0094` | outline established |
| `mesh::BoundMesh::with_weights` | `0x1009e0f50` | outline |
| `mesh::SplatBody::*` | `0x1009e1744`… | outlined (bind / thin / join / far-to-near) |
| `uv::UvLayout::new` | `0x1009cc73c` | **established**: built from the *rig*, 6.0/12.0 unit cells, scale clamped to `>= 1.0` |
| `uv::UvLayout::fit` | `0x1009cc5c8` | **established**: base 64 px/unit, rescale to `size·64/max` |
| `uv::UvLayout::locate/address/packed/matches/signature` | `0x1009cd034`… | partly (`48.0`, `±0.5` immediates) |
| `uv::Texture::sample` | `0x1009cb7c4` | bilinear, `0.5` texel offset, `1/255` |
| `uv::Texture::sample_normal` | `0x1009cb314` | `2c-1`, `127.5` immediate |
| `uv::bump_sheet` | `0x1009c9d18` | partly (`4.0` neighbourhood, `255.0`) |
| `camera::Camera::project` | `0x100a03bf0` | **fully established** |
| `draw::convex_hull` | `0x1009a818c` | **established** (in-place non-finite compaction, monotone chain) |
| `draw::severed` | `0x1009a8a7c` | established (calls `Puppet::subtree`) |
| `draw::Posed::draw_list` | `0x100a43040` | outline (layer types from drop glue) |
| `draw::Posed::ground_shadow` | `0x100a4285c` | established (convex hull of ground projection) |
| `skin::Effects::layered` | `0x1009b018c` | partly |
| `skin::SkinSurface::texel` | `0x1009aa4d4` | **established** (neutral `127.0` off-surface) |
| `skin::SkinSurface::scene_lit` | `0x1009aa5fc` | weights recovered (0.25 / 0.5 / 1.25 / 255.0), combination INFERRED |
| `skin::Posed::skin_*` | `0x100a50444`… | outline |
| `shadow::Posed::shadow_caster` | `0x100a549f8` | outline (ground hull + surfaces) |
| `shadow::ShadowCaster::heap_bytes` | `0x1009a5e04` | **established** (pure capacity sum) |
| `layers::LayerCloud::*` | `0x100a586a8`… | outlined; `motes` dispatch list recovered from its call sites |
| `layers::Particle::at` | `0x100a60734` | magnitudes recovered (`±24`, `±12`, `7`, `37`, `9`, `11`, `0.25`) |
| `fall::Fall::new` | `0x1009cfed4` | partly (`"leg"/"paw"/"foot"`, `-5.0`, `11.0`) |
| `fall::Fall::at` | `0x1009ce4ac` | partly (`-7..13`, `-6..15`, `±0.5`, `8.0`, `1/128`) |
| `hang::Puppet::arm_hangs_at` | `0x1009d173c` | outlined (subtree + downward test) |
| `joints::Posed::ring_xf` | `0x100a54760` | **established** (`quat_from_m3` + `slerp` + lerp) |
| `baked::BakedPuppet::from_toml` | `0x1009b09cc` | **schema recovered** (serde static pool) |
| `baked::Puppet::from_baked` | `0x1009d5bac` | outline |
| `baked::Puppet::from_baked_toml` | `0x1009d6658` | established (two-step wrapper) |

---

## Recovered constants (the acceptance evidence)

| constant | evidence | value |
|---|---|---|
| degrees→radians | `movk w8, #0x3c8e` | `0x3c8efa35` |
| quat degenerate cutoff | `movk w8, #0x3089` | `1e-9` |
| slerp nlerp threshold | `movk w8, #0x3f7f` | `0.9995` |
| flinch duration | `movk w8, #0x3f0c` | `0.55` s |
| flinch gain | `fmov s3, #6.75` | `6.75` (peaks at 1.0 at u = 1/3) |
| breathing period | `movk w10, #0x40c9` | `2π` |
| camera near clamp | `movk w8, #0x3d4c` | `0.05` |
| `SplatKey` radius scale | `mov w8, #0x447a0000` | 1000.0 (clamp 5..500) |
| `SplatKey` opacity scale | `mov w10, #0x42c80000` | 100.0 (clamp 20..200) |
| `SplatKey` detail scale | same `100.0`, clamp 5..100 | — |
| spz SH table | `u64` ×4 @ `0x1021dc2a8` | `[0, 3, 8, 15]` |
| spz smallest-three | `movk w26, #0x3f35` | `√½` |
| spz colour scale | `mov w8, #0x437f0000` | 255.0 |
| ply 9/4 pair | `u64` @ `0x1021dc000` | 18.0 / 4.0 |
| uv cell / stride | `fmov` in `UvLayout::new` | 6.0 / 12.0 |
| uv base px/unit | `mov w8, #0x42800000` | 64.0 |
| uv locate reach | `mov w8, #0x42400000` | 48.0 |
| skin neutral texel | `mov w8, #0x42fe0000` | 127.0 |
| skin blend weights | immediates in `scene_lit` | 0.25 / 0.5 / 1.25 |
| body mesh half split | `cmp w9, #0x6e` / `#0xef` | 110 / 239 |
| mote reach | `mov w8, #0x432a0000` | 170.0 |
| particle travel | immediates in `Particle::at` | ±24, ±12, 7, 37, 9, 11, 0.25, 0.5 |
| fall swing range | immediates in `Fall::at` | −7..13 (−6..15 sideways) |
| fall sampling step | double `0.0078125` | 1/128 |
| bit-mask table | bytes @ `0x1021dc9a8` | `[1,2,4,8,16,32,64,128]` |
| pose file table | `0x1043b60d8` | 13 × `(&str,&str)`, 0x1a0 bytes |
| splat `CORE_M` / `REACH_M` | embedded WGSL, `const CORE_M: f32 = 2.25;` / `REACH_M = 9.0` | 2.25 / 9.0 (`sqrt(REACH_M) = 3σ`) |
| splat `RIM_FROM` / `RIM_TO` | embedded WGSL, `0.3` / `0.8` | 0.3 / 0.8 |
| billboard half-extent | `let r = vec2(3.0 * sqrt(xx), 3.0 * sqrt(yy));` | `3σ` |
| variance / determinant floor | `xx0 + 0.3`, `max(…, 1e-6)` | 0.3 / 1e-6 |
| alpha cap | `min(v.alpha * exp(-0.5 * m), 0.99)` | 0.99 |
| GPU splat instance | `// one splat at rest (144 bytes)`, nine `vec4` | 144 (= 9·16) |
| draw `LIGHT_ABOVE` / `DARK_BELOW` | embedded WGSL, named as `gs_puppet::draw::…` | 0.62 / −0.08 |
| layers `FRESH_SECS` / `FALL_FT_S2` | embedded WGSL, named as `gs_puppet::layers::…` | 4.0 / 24.0 |

---

## The shipped pose data (recovered verbatim)

`poses/*.toml` — extracted from the 13 table entries, byte for byte.  The schema
(confirmed by the serde pool: `struct PoseFile with 10 elements`):

* `body_type = "biped"`;
* `[pose.<name>]` with `root = [x,y,z]` and per-bone
  `{ rot = [x,y,z], bend = [deg…], phase = f }`;
* `[limits]`, `[family_limits.<family>]`: `{ rot = [[lo,hi]×3], bend = [[lo,hi]…] }`;
* `[idle]` / `[family_idle.<family>]`: `period = f` plus bone amplitudes;
* `[flinch.<region>]`, `[guard.<name>]`, `[posture.<name>]`, `[family.<family>.<pose>]`.

The angle convention is documented in the files themselves: Euler in degrees,
applied `Rx · Rz · Ry`; a key with no `_l`/`_r` that names no bone is a *pair*
(`leg_r` = `leg` with y and z negated); `bend` carries one angle per inner
segment.  Tests assert real values out of the recovered data (biped `arms_up` =
`152°` spread with `-14°` elbow, `prone` root = `90°`, quadruped `hindleg`
limits `[[-140,75],[-30,30],[-60,35]]` with bends `[[-10,170],[-120,20]]`, the
elephantid `trunk` override, the 4.2 s biped idle with a `-0.16` arm phase, …).

---

## Phase 4: egui

### The binary has no egui UI here (the evidence)

* **No call edge.**  `docs/EGUI.md` §3.6 scanned the whole `__TEXT` disassembly
  (`objdump -d`) for `bl` edges into egui-family symbols: 1 940 edges exist, and
  **zero** come from `gs_puppet` (against 248 from `hydra_gui`, 7 from
  `gs_render`, 2 from `hydra_board`).
* **No named type.**  No `gs_puppet` symbol in `symbols_demangled.txt`, nor in
  `nm -an | rustfilt`, mentions `egui` / `epaint` / `ecolor` / `emath`.
* **No widget strings.**  Nothing in the shared string pool names an egui widget,
  layer or painter.

There is therefore no widget, layout, hotbar or window code to restore in this
crate, and none was invented.  What the crate *did* do was build the draw data the
`gs_render` GPU passes consumed — `splat::render` issued a draw, `draw_list`
produced the limb layers, `skin`/`shadow`/`uv` produced the textures — and those
seams now also emit `epaint` values (a `Mesh`, `Shape`s, `ColorImage`s).  The
seams themselves (`render`, `prepare`, `draw_list`, `ground_shadow`, `skin_*`,
`shadow_caster`) are unchanged; everything below is additive.

### The recovered evidence the new geometry comes from

The binary embeds its WGSL: `skin_gpu`'s three splat passes and `splat_gpu.wgsl`
("the same three passes for a cloud the CPU projected").  Both copies carry
comments naming this crate's items, and their code pins the billboard maths
exactly:

| shader line | what it pins |
|---|---|
| `// gs_puppet::splat::CORE_M and REACH_M: a splat's core, and its reach` | the two constants exist here; `2.25` and `9.0` are their values |
| `// gs_puppet::splat::RIM_FROM and RIM_TO: where a rim splat starts to show, / and is whole, by how edge-on it is` | `0.3` and `0.8` |
| `let r = vec2<f32>(3.0 * sqrt(xx), 3.0 * sqrt(yy));` / `let off = corner[k] * r * u.to_cam.w;` | the quad's half-extent is `3σ`, and the reach multiplier applied to it |
| `let corner = array<vec2<f32>, 6>((-1,-1), (1,-1), (-1,1), (-1,1), (1,-1), (1,1));` | the exact corner/triangle order |
| `let xx = xx0 + 0.3; let det = max(xx * yy - xy * xy, 1e-6);` | the variance floor and the determinant floor |
| `let spread = sqrt(clamp((xx0 * yy0 - xy*xy) / det, 0.0, 1.0));` | the sub-pixel fade ("as much fainter as it was made wider") |
| `o.inv = vec4(yy / det, -xy / det, -xy / det, xx / det);` + `gauss_m(d, inv)` | the inverse covariance and the falloff form |
| `let a = min(v.alpha * exp(-0.5 * m), 0.99);` | the peak alpha and its cap |
| `// one splat at rest (144 bytes)` + the nine `vec4` members | the GPU instance stride `9 × 16 = 144` (vs the 92-byte decoded record) |
| `// gs_puppet::draw::LIGHT_ABOVE / DARK_BELOW, as mesh::render shades` | `0.62` and `-0.08` |
| `// gs_puppet::layers::FRESH_SECS and FALL_FT_S2` | `4.0` and `24.0` |
| `// (gs_puppet::layers::Particle::at, line for line)` | confirms `Particle::at`'s formula (already recovered) |

`3.0 = sqrt(REACH_M)` is the consistency check: the quad reaches exactly the
`REACH_M` contour along each axis, which is why the shader can write `3.0 * sqrt`.

### New public API (all additive; nothing renamed or removed)

`splat`:

| item | what it is |
|---|---|
| `CORE_M`, `REACH_M`, `RIM_FROM`, `RIM_TO` | the four `gs_puppet::splat` constants the shaders name |
| `QUAD_SIGMAS` (3.0), `MIN_VARIANCE` (0.3), `MIN_DET` (1e-6), `MAX_ALPHA` (0.99) | the billboard/falloff numbers |
| `SPLAT_INSTANCE_STRIDE` (144) | the GPU instance stride, beside the existing `SPLAT_STRIDE` (92) |
| `gauss_m([f32;2], [f32;4]) -> f32`, `falloff(f32) -> f32` | the shader's quadratic form and `exp(-0.5 m)` |
| `struct Footprint { centre, half, inv, spread, depth, alpha }` | one splat's projected billboard |
| `Footprint::{corners, local, gauss_at, falloff_at, peak_alpha, area, is_finite}` | its geometry |
| `Footprint::of(&Splat, &Camera) -> Option<Footprint>` | the full three-axis covariance of `vs_cloud`, on the CPU |
| `Footprint::of_prepared(&PreparedSplat) -> Footprint` | the isotropic special case |
| `PreparedSplat::{footprint, mesh}` | per-splat convenience |
| `SPLAT_UVS` | the quad's UVs into a radial gaussian sheet |
| `mesh(&[PreparedSplat], TextureId) -> epaint::Mesh` | one textured billboard quad per splat, in the list's order |
| `solid_mesh(&[PreparedSplat]) -> epaint::Mesh` | the same, untextured (`WHITE_UV`) |
| `prepared_mesh(&[Splat], &Camera, budget, TextureId) -> epaint::Mesh` | `prepare` + `mesh` in one call |

`draw`: `LIGHT_ABOVE`, `DARK_BELOW`, `color32([u8;4])`, `color32_rgb([u8;3])`,
`Shape::{pos2s, to_fill, to_stroke}`, `Prim::to_epaint`,
`DrawList::{shapes, filled_shapes, layer_shapes, prim_shapes}`,
`Posed::{shapes, shadow_shapes}`.

`shadow`: `ShadowCaster::shapes`.

`skin`: `SkinSurface::color_image`.  `uv`: `Texture::color_image`.

`layers`: `FRESH_SECS`, `FALL_FT_S2`.

### What each seam emits, and how it is tested

* **splat → `epaint::Mesh`.**  `mesh` walks the far-to-near list `prepare`
  returns, so the mesh's quad order *is* the draw order.  Per splat: 4 vertices
  (the `3σ` box), 6 indices in the shader's triangle order, UVs `(0,0) (1,0) (0,1)
  (1,1)`, and one vertex colour (the splat's rgb, premultiplied — which is what
  the recovered blend pass writes — at `peak_alpha = min(alpha·spread, 0.99)`).
  The Gaussian falloff is per-fragment in the shader and `epaint` can only vary
  alpha per vertex, so it must come from the texture's alpha channel; that is why
  the UVs span the whole quad.  `solid_mesh` is the hard-edged variant.
* **`draw_list` → `epaint::Shape`s.**  `DrawList::shapes` emits `filled` (4-byte),
  `tinted` (3-byte, opaque) and then every `SegDraw` quad of every `layers` entry
  in the recovered far-to-near order; `prims` is the flat copy of the same quads,
  so it is not re-emitted (no double painting of the ground shadow, which lives in
  both `filled` and `prims`).  `prim_shapes` / `layer_shapes` expose the two views
  separately.
* **`ground_shadow` / `shadow_caster` → one hull polygon.**  `filled_shapes`, so
  the translucent colour keeps its alpha.
* **surfaces → `ColorImage`.**  `SkinSurface::color_image` and `Texture::color_image`
  are the values `Context::load_texture` consumes; both use the recovered `255.0`
  fixed point.

Tests (179 unit + 2 integration, all value-asserting; the pre-existing 151 are
untouched and still green):

| test | values pinned |
|---|---|
| `splat::tests::the_recovered_shader_constants_are_the_recovered_values` | 2.25 / 9.0 / 0.3 / 0.8 / 0.3 / 1e-6 / 0.99; `QUAD_SIGMAS == sqrt(REACH_M)`; `exp(-CORE_M/2) = 0.3246525`, `exp(-REACH_M/2) = 0.011109` |
| `splat::tests::the_gpu_instance_is_nine_vec4s` | 144 = 9·16, ≠ 92 |
| `splat::tests::gauss_m_is_the_shaders_quadratic_form` | the form at `(1,0)+(2,0)+(3,4)` offsets, and the cross term `(d.x+d.y)²` |
| `splat::tests::a_projected_splat_gets_the_shaders_billboard` | centre `(320,240)`, depth 8, `half = 3·sqrt(1.3)`, `spread = 1/1.3`, `inv = diag(1/1.3)`, `gauss_m(half) = REACH_M`, the four corners, `peak_alpha`, `area` |
| `splat::tests::a_sub_pixel_splat_is_spread_over_a_pixel_and_fades` | `half = 3·sqrt(0.55)`, `spread = 0.25/0.55`, a zero-size splat keeps a pixel of reach at alpha 0 |
| `splat::tests::a_stretched_splat_is_an_ellipse_on_the_screen` | `half = (3·sqrt(4.3), 3·sqrt(1.3))`, `spread = sqrt(4/5.59)`, the axis pointing at the camera contributes nothing |
| `splat::tests::rotating_a_splat_turns_the_quad_into_a_sheared_footprint` | a 45° basis gives `xx0 = yy0 = 2.5`, `xy = -1.5`, a symmetric `inv` with `det(inv)·det = 1`, and the *same* spread (rotation does not fade a splat) |
| `splat::tests::the_peak_alpha_is_capped_like_the_blend_pass` | cap 0.99, floor 0.0 |
| `splat::tests::the_mesh_is_one_billboard_quad_per_splat` | 8 vertices / 12 indices, `is_valid`, the literal index list, corner positions, the four UVs, the premultiplied colours (`255,128,0,196`), the bbox |
| `splat::tests::the_mesh_keeps_the_prepared_far_to_near_order` | depths `[8,6,4]` in that order, each quad centred on `prepared[i].screen`, quad width growing as depth shrinks |
| `splat::tests::the_solid_mesh_uses_the_untextured_white_uv` | `TextureId::default()`, `WHITE_UV`, `(51,102,153,196)` |
| `splat::tests::prepared_mesh_projects_and_builds_in_one_step` | the budget truncation and the near-splat-wider rule |
| `splat::tests::a_real_egui_frame_paints_the_splat_mesh` | one real `ctx.run_ui` frame: the `Shape::Mesh` is there, 8 vertices, 12 indices, the texture id, the frame's mesh bounds = `±(320,240) ∓ 3·sqrt(1.3)` |
| `draw::tests::a_bone_quad_becomes_one_epaint_polygon` | the four exact screen points `(185,120) (185,70) (135,70) (135,120)`, closed, the fill and the empty stroke |
| `draw::tests::shapes_come_out_far_to_near` | the trunk (depth 8) before the neck (depth 6), centroids `(160,95)` and `(160,46.67)` |
| `draw::tests::a_shadow_hull_is_emitted_once_not_twice` | `filled = 1`, `prims = 1`, `shapes() = 1`, alpha 90 |
| `draw::tests::the_layer_and_flat_views_hold_the_same_quads` | the layer depths are non-increasing, `layer_shapes` is exactly their prims, `shapes()` = `layer_shapes` here |
| `draw::tests::degenerate_shapes_paint_nothing` | 0/1/2-point shapes, the transparent fill of a line, the tint alpha |
| `draw::tests::byte_colours_become_premultiplied_epaint_colours` | `(200,100,40,128)` premultiplies channel by channel |
| `draw::tests::a_real_egui_frame_paints_the_limb_quads` | one real frame: two `Path`s, the trunk's four points, its clip-independent bounding rect `(135,70)..(185,120)` |
| `shadow::tests::the_caster_hull_becomes_one_epaint_polygon` | the hull point for point, alpha 80, the stroke, the bounds |
| `shadow::tests::a_flat_caster_paints_nothing` | a pitch-0 camera has no hull and yields no shape |
| `skin::tests::the_surface_becomes_an_epaint_color_image` | `[2,2]`, the four texels `(255,128,64,255)`, `(0,0,0,0)`, `(51,102,153,255)`, the premultiplied half-alpha white, agreement with `to_texture` |
| `uv::tests::the_atlas_becomes_an_epaint_color_image` | straight-in / premultiplied-out bytes, the blank atlas |
| `tests/headless_paint.rs::a_posed_figure_and_its_splats_paint_into_one_frame` | one real frame with a **loaded texture**: 3 shapes in paint order, the two limb quads' exact points, the mesh's texture id = the handle's, 8 vertices, bounds `±3·sqrt(1.6²+0.3)`, and the 4×4 sheet in `textures_delta.set` |
| `tests/headless_paint.rs::a_tilted_camera_paints_the_ground_shadow_too` | one hull polygon from a pitched camera, alpha 90, finite bounds |

No test asserts "it did not panic"; each one pins numbers.  Nothing opens a window
or creates a `wgpu` device: the only egui entry points used are
`Context::default`, `Context::load_texture`, `Context::run_ui` and
`Painter::add`.

### The `epaint` translation choices (INFERRED, and why)

1. **Falloff source.**  Per-fragment `exp(-0.5·m)` cannot live in a 4-vertex mesh,
   so it moves to the texture's alpha channel and the vertices carry the *peak*
   alpha.  `solid_mesh` exists for callers with no sheet.  (The shader's own corner
   value is `exp(-9) ≈ 1.2e-4`, i.e. effectively 0 — carrying the corner value on
   the vertices would paint nothing.)
2. **`PreparedSplat` has one radius, not the covariance.**  `Footprint::of_prepared`
   is the recovered formula's isotropic special case (`xx0 = yy0 = radius²`,
   `xy = 0`); [`Footprint::of`] uses the real three projected axes and is the
   faithful one when the caller still has the [`Splat`]s.
3. **Paint order for `DrawList::shapes`.**  `filled` → `tinted` → `layers`, i.e. the
   list's own structure; `prims` stands aside because it duplicates `layers`.
   Which of the binary's layers `gs_render` actually consumed is not recoverable
   from the symbols (only the types survive), so no other order was invented.

### Remaining UNKNOWNs (this phase)

1. **`splat::Gaussian::rim`.**  The shader names it
   (`// x: its rim (gs_puppet::splat::Gaussian::rim)`, `o.glow += RIM_STEP *
   round(clamp(i.fx.x, 0.0, 1.0) * 8.0)`), so a `Gaussian` type with a `rim()`
   method existed here — but no symbol and no drop glue pins its layout or its
   formula (only that it returns a `0..=1` value).  Not reconstructed; no
   placeholder type was invented.
2. **`layers::ARRIVAL_AGES`.**  The shader's uniform comment mentions
   `gs_puppet::layers::ARRIVAL_AGES` ("the ages of its frames"); the *values* never
   appear outside the CPU builder, so no array was added.
3. **Which layer `gs_render` consumed** (see choice 3 above).
4. **The skin/body GPU path stays a texture seam.**  `skin::skin_surface` still
   produces a `SkinSurface` (now also convertible to `ColorImage`); the shader that
   consumes it (`skin_gpu`) does the projection itself, so there is no `epaint`
   geometry to emit for it — converting the body mesh to 2-D triangles would be
   inventing a path the binary does not have.
5. **`layers::Dot` / `Hollow` / `Particle` → no `epaint` producer.**  They are
   instanced into the same shader (`vs_cloud`'s `thrown`/`mote`), not painted by
   the CPU; emitting 2-D shapes for them would be a second, invented renderer.

---

## std departures

| original | replacement | notes |
|---|---|---|
| `glam` | `math::{Vec3, Mat3, Quat, Xf}` | layouts and constants are the recovered ones |
| `wgpu` | data-returning functions, plus `epaint` values | `splat::render`, `skin::*` and `shadow::*` produce the buffers the GPU pass consumed (nothing is silently dropped), and the Phase 4 seam turns that data into an `epaint::Mesh` / `Shape`s / `ColorImage`s |
| `toml` + `serde` | `toml::parse` + `PoseSet::from_toml` / `BakedPuppet::from_toml` | subset: `[a.b]`, `[[a]]`, `key = "str" / num / bool / [..] / {..}`, `#` comments |
| `ruzstd` | `splat::zstd_decode` | frame header + **Raw/RLE** blocks; a `Compressed` block returns an error rather than garbage |
| `hashbrown` | `std::collections::HashMap` | `Mesh::parse_obj`'s vertex merge |
| `anyhow` | `model::PuppetError` | `Display` messages preserved where recovered |
| `std::thread::scope` (parallel splat/raster/skin work) | sequential loops | same results, no threads |

---

## Significant unknowns

1. **`splat::decode_spz` stream payloads.**  The header, TOC, stream count, the
   four raw lengths and the position fixed point are all established; the *layout
   inside* the `3n` and `4n` streams (colour / rotation / opacity packing) and the
   exact bit layout of the smallest-three rotation bytes are not.  The decoder
   implements the sizes and the position/colour handling and marks the rotation
   packing `UNKNOWN`.
2. **zstd.**  Streams whose compressed length equals their raw length are read
   verbatim; entropy-compressed blocks are refused with an error.  A real `.spz`
   from the shipping game is entropy-coded, so `decode_spz` reports that instead of
   guessing.
3. **`splat::load_ply` payload conventions** (`SH_C0`, logit opacity, log scales,
   `(w,x,y,z)` rotation order).  These are the 3-D Gaussian-splatting standard and
   the binary's own `quat_norm`/`quat_to_m3` calls, but the arithmetic itself was
   not disassembled constant-for-constant.
4. **`uv` layout semantics.**  `UvLayout::new` iterates the rig (224-byte stride)
   and uses 6/12-unit cells, `fit` rescales from 64 px/unit — but *how* the cells
   are assigned to bones, and what `packed`/`matches`/`signature` compare, is only
   partly recovered.
5. **`skin::scene_lit`'s blend.**  Weights `0.25/0.5/1.25` and the `255.0` fixed
   point are established; the 23 KB inlined loop's exact combination is not.
6. **`layers` / `layers::Particle::at` formulas.**  Magnitudes are recovered; the
   rates' internal scaling is INFERRED (`RATE/10` per second).
7. **`fall::Fall::at`'s wobble.**  The ranges and the 1/128 step are recovered;
   the exact damping curve is INFERRED.
8. **`mesh::BodyMeta::read`'s file format.**  The *behaviour* (kind flag +
   index `1` + finite height) is recovered, the sidecar format is not; a
   `body_meta.txt` `key value` reader is used instead.
9. **`pose::PoseSet::dead_hash`'s hasher.**  The binary uses
   `std::collections::hash_map::DefaultHasher`, which is not reproducible across
   Rust releases; a documented FNV-1a is used (stable per family, which is all the
   caller needs).
10. **`pose::Puppet::pin_arms`.**  The bone literals `"arm_l"`/`"arm_r"` and the
    clamp-to-limits behaviour survive, the reach limit itself does not.
11. **`baked::{BakedPuppet, BakedSegment, BakedEye}`'s remaining fields.**  The
    element counts (11 / 14 / 4) and the names `version height groups made_splats
    arms_hang body_type family segment pos normal radius group axis_w axis_d hinge`
    are recovered; the rest of the records is unknown and unknown TOML keys are
    ignored.

---

## Integration assumptions for `gs_render`

* `gs_render::puppet_cache::SplatKey`'s `From<gs_puppet::splat::SplatParams>`
  should keep the recovered packing (`a | b<<16 | c<<32`); the crate exposes
  `SplatParams::quantised()` and `SplatParams::key_bits()` so the impl can be a
  one-liner.
* `SplatParams` is `Copy`, three public `f32` fields in the recovered order
  (`radius`, `opacity`, `detail`), with `Default` inside every clamp range.
* `gs_puppet::pose::{Pose, PoseSet, Posed}` and `gs_puppet::model::Puppet`
  (`pub use`d from `pose` and `hang`) are the pose seam; `Posed::bone(&str)`
  returns an [`crate::math::Xf`] and `Posed::bounds()` a `(Vec3, Vec3)`.
* `gs_puppet::splat::{Splat, SplatCloud, BoundCloud, PreparedSplat}` are the draw
  seam: `splat::prepare(&[Splat], &Camera, budget) -> Vec<PreparedSplat>` replaces
  the GPU vertex-buffer build, and `splat::render` returns that list instead of
  issuing a draw.
* For a painter (egui or otherwise) the same list becomes geometry:
  `splat::mesh(&[PreparedSplat], TextureId) -> epaint::Mesh` (or `solid_mesh`,
  `prepared_mesh`), `PreparedSplat::footprint` / `Footprint::of` for one splat's billboard,
  `Posed::{shapes, shadow_shapes}`, `ShadowCaster::shapes`,
  `SkinSurface::color_image`, `Texture::color_image`.  All additive: the seam
  functions above are unchanged.
* `splat::{CORE_M, REACH_M, RIM_FROM, RIM_TO}` and `draw::{LIGHT_ABOVE,
  DARK_BELOW}` and `layers::{FRESH_SECS, FALL_FT_S2}` are the constants the shipped
  WGSL names, in case a renderer wants the same thresholds.
* The `pose::Posed` inherent methods are spread over modules (`draw`, `skin`,
  `shadow`, `joints`) exactly as the binary had them; they are *inherent*, so no
  extra `use` is needed to call them (only to name a module-level helper such as
  `draw::convex_hull`).
