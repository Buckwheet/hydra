//! Skin/splat job bookkeeping and the depth span used for batch ordering.
//!
//! Recovered from `gs_render::skin_gpu::*`. The original module built `wgpu`
//! pipelines (`SkinGpu::new/draw/draw_body/finish`, `upload_rgba`,
//! `upload_mipped`); those are represented here by layout descriptors handed to
//! [`crate::gpu::Gpu`].

use std::sync::{Arc, Mutex};

use crate::gpu::{Gpu, TexFormat, TextureUpload};

/// Upper bound of the process-wide job table.
///
/// `submit` compares the length with `0x5f` and drops the oldest entry
/// (`cmp x23, #0x5f; b.ls` then `Vec::remove(0)` at `0x10091dab0..0x10091dac8`),
/// so the table holds at most `96` jobs.
pub const MAX_JOBS: usize = 96;

/// One vertex of a skinned body.
///
/// The stride is `0x50` (`mov w8, #0x50; madd x8, x22, x8, x21` at
/// `0x10092ad8c`) and the iterator reads the position from `+0x18`
/// (`ldr x10, [x9, #0x18]`, `ldr w9, [x9, #0x20]` — the `Map<..>::next` at
/// `0x10091af8c`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex80 {
    pub pos: [f32; 3],
    pub rest: [u8; 80 - 12],
}

/// Stride of [`Vertex80`].
pub const VERTEX80_STRIDE: usize = 0x50;

/// One vertex of a splat body: stride `0x54`, position at `+0x34`
/// (`ldur x10, [x9, #0x34]`, `ldr w9, [x9, #0x3c]` in the `Map<..>::next` at
/// `0x10091af54`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex84 {
    pub lead: [u8; 0x34],
    pub pos: [f32; 3],
    pub tail: [u8; 0x54 - 0x40],
}

/// Stride of [`Vertex84`].
pub const VERTEX84_STRIDE: usize = 0x54;

/// A skinned body (job tag `0`).
///
/// `depth_span` reads the light direction at `+0x1b0`
/// (`ldr s8, [x8, #0x1b0]; add x9, x8, #0x1b4`) and the vertex vector at `+0x10`
/// (`ldp x21, x22, [x8, #0x18]` = ptr/len).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BodyMesh {
    pub vertices: Vec<Vertex80>,
    /// `+0x1b0` in the binary's layout.
    pub light: [f32; 3],
}

/// A splat body (job tag `1`).
///
/// Light at `+0x48` (`ldr s8, [x8, #0x48]; ldur d9, [x8, #0x4c]`), vertices at
/// `+0x10`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SplatBody {
    pub vertices: Vec<Vertex84>,
    /// `+0x48` in the binary's layout.
    pub light: [f32; 3],
}

/// A loose point set (job tag `>= 2`).
///
/// The binary iterates a *fixed* slice: `add x9, x21, #0x98` (begin) and
/// `add x8, x21, #0xf8` (end), i.e. `0x60` bytes = 8 `[f32; 3]` elements — a
/// `[Vec3; 8]`. Light at `+0x108`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointBody {
    pub points: [[f32; 3]; POSE_SLOTS],
    /// `+0x108` in the binary's layout.
    pub light: [f32; 3],
}

/// Number of fixed point slots, `(0xf8 - 0x98) / 12`.
pub const POSE_SLOTS: usize = 8;

impl Default for PointBody {
    fn default() -> Self {
        PointBody {
            points: [[0.0; 3]; POSE_SLOTS],
            light: [0.0; 3],
        }
    }
}

/// What a [`SkinJob`] is currently drawing.
///
/// The discriminant is the word at `+0x00` of the job (`ldr x8, [x0]` in
/// `depth_span`, compared with `0` and `1`).
#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    /// tag `0` — payload pointer at `+0x10`.
    Body(BodyMesh),
    /// tag `1` — payload pointer at `+0x08`.
    Splats(SplatBody),
    /// tag `>= 2` — payload pointer at `+0x08`.
    Points(PointBody),
}

impl Payload {
    /// The tag the binary would store at `+0x00`.
    pub fn tag(&self) -> u64 {
        match self {
            Payload::Body(_) => 0,
            Payload::Splats(_) => 1,
            Payload::Points(_) => 2,
        }
    }

    fn light(&self) -> [f32; 3] {
        match self {
            Payload::Body(b) => b.light,
            Payload::Splats(s) => s.light,
            Payload::Points(p) => p.light,
        }
    }
}

/// A batch handed to the GPU pipeline.
///
/// `depth_scale` is the `f32` at `+0xa0` (`ldr s12, [x20, #0xa0]` in
/// `depth_span`).
#[derive(Debug, Clone, PartialEq)]
pub struct SkinJob {
    pub payload: Payload,
    pub depth_scale: f32,
}

impl SkinJob {
    pub fn new(payload: Payload, depth_scale: f32) -> Self {
        SkinJob {
            payload,
            depth_scale,
        }
    }

    /// `SkinJob::depth_span` @0x10092acec — the depth range of the batch along
    /// the negative light direction.
    ///
    /// Fully recovered arithmetic (`0x10092ae08..0x10092ae40`):
    ///
    /// ```text
    /// for p in positions {
    ///     d = -(p.x * l.x + p.y * l.y + p.z * l.z) * job.depth_scale;
    ///     lo = lo.min(d);            // fminnm
    ///     hi = hi.max(d);            // fmaxnm
    /// }
    /// ```
    ///
    /// with `lo` seeded to `+inf` (`0x7f7fffff`) and `hi` to `-inf`
    /// (`0xff7fffff`), so an empty batch returns `(+inf, -inf)` exactly like the
    /// binary. The returned pair is packed into one 64-bit register
    /// (`orr x0, x8, x9, lsl #32`).
    pub fn depth_span(&self) -> (f32, f32) {
        let light = self.payload.light();
        let (lx, ly, lz) = (light[0], light[1], light[2]);
        let scale = self.depth_scale;
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        let mut take = |p: [f32; 3]| {
            let d = -((p[0] * lx) + (p[1] * ly) + (p[2] * lz)) * scale;
            lo = lo.min(d);
            hi = hi.max(d);
        };
        match &self.payload {
            Payload::Body(b) => {
                for v in &b.vertices {
                    take(v.pos);
                }
            }
            Payload::Splats(s) => {
                for v in &s.vertices {
                    take(v.pos);
                }
            }
            Payload::Points(p) => {
                for v in &p.points {
                    take(*v);
                }
            }
        }
        (lo, hi)
    }

    /// Number of positions in the batch.
    pub fn len(&self) -> usize {
        match &self.payload {
            Payload::Body(b) => b.vertices.len(),
            Payload::Splats(s) => s.vertices.len(),
            Payload::Points(p) => p.points.len(),
        }
    }

    /// The vertex stride in bytes (`0x50`, `0x54` or `0x0c`).
    pub fn vertex_stride(&self) -> usize {
        match &self.payload {
            Payload::Body(_) => VERTEX80_STRIDE,
            Payload::Splats(_) => VERTEX84_STRIDE,
            Payload::Points(_) => 12,
        }
    }
}

/// The process-wide job table.
///
/// The binary keeps `Mutex<Vec<(u64, Arc<SkinJob>)>>` (drop glue
/// `MutexGuard<Vec<(u64, Arc<SkinJob>)>>`, entry stride `0x10`) behind a lazily
/// initialised static (`OnceLock::initialize` + `OnceBox` at
/// `0x1045862b8`/`0x1045862c0`). `Vec::new()` is const, so a plain `static`
/// reproduces it.
static JOBS: Mutex<Vec<(u64, Arc<SkinJob>)>> = Mutex::new(Vec::new());

/// `skin_gpu::job` @0x10091d8fc — look a job up by id and clone its `Arc`.
///
/// Recovered: linear scan over the `(u64, Arc)` entries comparing `x11` with the
/// id, then a single `ldadd` strong-count bump (`0x10091d960..0x10091d978`).
/// `None` is the niche null pointer.
pub fn job(id: u64) -> Option<Arc<SkinJob>> {
    let jobs = JOBS.lock().unwrap();
    jobs.iter()
        .find(|(jid, _)| *jid == id)
        .map(|(_, j)| Arc::clone(j))
}

/// `skin_gpu::submit` @0x10091da20 — (re)register a job for this frame.
///
/// Recovered:
///
/// 1. `retain(|(jid, _)| *jid != id)` drops an older job with the same id
///    (`bl Vec::retain` at `0x10091da94`);
/// 2. if the table is full (`len > 95`) the *oldest* entry is removed with
///    `Vec::remove(0)` (`memmove` at `0x10091dac4`);
/// 3. the new `(id, job)` pair is pushed.
pub fn submit(id: u64, job: Arc<SkinJob>) {
    let mut jobs = JOBS.lock().unwrap();
    jobs.retain(|(jid, _)| *jid != id);
    while jobs.len() > MAX_JOBS - 1 {
        jobs.remove(0);
    }
    jobs.push((id, job));
}

/// The ids currently resident, oldest first.
pub fn resident_ids() -> Vec<u64> {
    JOBS.lock().unwrap().iter().map(|(id, _)| *id).collect()
}

/// Drop every registered job (used when a body is unloaded).
pub fn clear_jobs() {
    JOBS.lock().unwrap().clear();
}

/// `skin_gpu::upload_rgba` @0x10091d228 — upload an 8-bit RGBA image with no
/// mip chain.
///
/// Recovered descriptor: `width`/`height` are clamped to at least 1
/// (`csinc w8, w3, wzr, hi`), the format is `Rgba8UnormSrgb` (`mov w8, #6`
/// selects the format/sampler word at `0x10091d298`), there is exactly one mip
/// level (`stp x8, xzr, [sp, #0xb0]` with `x8 = 4` selects
/// `mip_level_count = 1`) and the row pitch is `bytes_per_pixel * width`
/// (`mul w8, w25, w24` at `0x10091d2c8`, `w25` being the 4-byte pixel size).
pub fn upload_rgba<'a, G: Gpu>(
    gpu: &mut G,
    label: &'a str,
    width: u32,
    height: u32,
    data: &'a [u8],
) -> TextureUpload<'a> {
    let upload = TextureUpload::new(
        label,
        width.max(1),
        height.max(1),
        TexFormat::Rgba8UnormSrgb,
        1,
        data,
    );
    gpu.write_texture(&upload);
    upload
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(light: [f32; 3], scale: f32, pts: &[[f32; 3]]) -> SkinJob {
        let vertices = pts
            .iter()
            .map(|p| Vertex80 {
                pos: *p,
                rest: [0; 80 - 12],
            })
            .collect();
        SkinJob::new(Payload::Body(BodyMesh { vertices, light }), scale)
    }

    #[test]
    fn vertex_strides_are_the_recovered_ones() {
        assert_eq!(VERTEX80_STRIDE, 80);
        assert_eq!(VERTEX84_STRIDE, 84);
        assert_eq!(std::mem::size_of::<Vertex80>(), 80);
        assert_eq!(std::mem::size_of::<Vertex84>(), 84);
        assert_eq!(std::mem::offset_of!(Vertex84, pos), 0x34);
        assert_eq!(std::mem::offset_of!(Vertex80, pos), 0);
        assert_eq!(POSE_SLOTS, 8);
        assert_eq!(POSE_SLOTS * 12, 0xf8 - 0x98);
    }

    #[test]
    fn depth_span_is_negative_light_dot_times_scale() {
        let j = body([0.0, 0.0, 1.0], 2.0, &[[0.0, 0.0, 1.0], [0.0, 0.0, 3.0]]);
        // d = -(p.z * 1.0) * 2.0 -> -2 and -6.
        assert_eq!(j.depth_span(), (-6.0, -2.0));
    }

    #[test]
    fn depth_span_of_an_empty_batch_keeps_the_seeds() {
        let j = body([1.0, 0.0, 0.0], 1.0, &[]);
        let (lo, hi) = j.depth_span();
        assert_eq!(lo, f32::INFINITY);
        assert_eq!(hi, f32::NEG_INFINITY);
        assert_eq!(j.len(), 0);
    }

    #[test]
    fn depth_span_covers_all_three_payloads() {
        let s = SkinJob::new(
            Payload::Splats(SplatBody {
                vertices: vec![Vertex84 {
                    lead: [0; 0x34],
                    pos: [1.0, 0.0, 0.0],
                    tail: [0; 0x54 - 0x40],
                }],
                light: [1.0, 0.0, 0.0],
            }),
            1.0,
        );
        assert_eq!(s.depth_span(), (-1.0, -1.0));
        assert_eq!(s.vertex_stride(), 84);

        let mut p = PointBody::default();
        p.points[3] = [0.0, 2.0, 0.0];
        p.light = [0.0, 1.0, 0.0];
        let j = SkinJob::new(Payload::Points(p), 0.5);
        // Slot 3 is the only non-zero point: d = -(2.0 * 1.0) * 0.5 = -1.
        assert_eq!(j.depth_span(), (-1.0, 0.0));
        assert_eq!(j.vertex_stride(), 12);
        assert_eq!(j.len(), 8);
    }

    #[test]
    fn payload_tags_match_the_binary_discriminants() {
        assert_eq!(Payload::Body(BodyMesh::default()).tag(), 0);
        assert_eq!(Payload::Splats(SplatBody::default()).tag(), 1);
        assert_eq!(Payload::Points(PointBody::default()).tag(), 2);
    }

    #[test]
    fn job_table_dedupes_and_caps_at_96() {
        clear_jobs();
        let mk = |id: u64| Arc::new(body([0.0, 0.0, 1.0], 1.0, &[[0.0, 0.0, id as f32]]));
        submit(7, mk(7));
        assert!(job(7).is_some());
        // Re-submitting the same id replaces the entry instead of duplicating it.
        submit(7, mk(8));
        assert_eq!(resident_ids(), vec![7]);
        assert_eq!(job(7).unwrap().depth_span().0, -8.0);
        assert!(job(8).is_none());

        // Fill past the cap: the oldest entries fall off the front.
        for id in 0..(MAX_JOBS as u64 + 4) {
            submit(1000 + id, mk(id));
        }
        let ids = resident_ids();
        assert_eq!(ids.len(), MAX_JOBS);
        assert_eq!(ids[0], 1004);
        assert_eq!(*ids.last().unwrap(), 1000 + MAX_JOBS as u64 + 3);
        clear_jobs();
        assert!(resident_ids().is_empty());
    }

    #[test]
    fn upload_rgba_reports_the_row_pitch() {
        struct Recorder {
            got: Option<(u32, u32, u32, u32, TexFormat)>,
        }
        impl Gpu for Recorder {
            fn write_texture(&mut self, up: &TextureUpload<'_>) {
                self.got = Some((
                    up.width,
                    up.height,
                    up.mip_levels,
                    up.bytes_per_row,
                    up.format,
                ));
            }
        }
        let mut rec = Recorder { got: None };
        let data = vec![0u8; 8 * 4 * 4];
        let up = upload_rgba(&mut rec, "skin", 8, 4, &data);
        assert_eq!(rec.got, Some((8, 4, 1, 32, TexFormat::Rgba8UnormSrgb)));
        assert!(up.is_consistent());
        // Zero dimensions are clamped to 1, as the binary does with `csinc`.
        let up2 = upload_rgba(&mut rec, "skin", 0, 0, &[]);
        assert_eq!((up2.width, up2.height), (1, 1));
    }
}
