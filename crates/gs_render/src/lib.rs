#![allow(dead_code, unused)]
//! `gs_render` — the GPU/caching boundary between the puppet model and the
//! renderer.
//!
//! Recovered from the arm64 Mach-O binary at `~/Code/hydra-inspect/hydra`
//! (`hydra/0.1.0-beta.7`, no DWARF). Every item in this crate carries a
//! `/// @0x...` note with the address of the function it was recovered from;
//! `NOTES.md` holds the evidence table and the `established` / `INFERRED` /
//! `UNKNOWN` legend.
//!
//! ## Shape of the original crate
//!
//! Four modules survived as exported symbols:
//!
//! | module | role |
//! |---|---|
//! | [`puppet_cache`] | keying and caching of posed puppets, bodies, textures and pose sets |
//! | [`figure_image`] | CPU-side image building: ground quad geometry, skin atlas slicing, glow/alpha merge |
//! | [`skin_gpu`] | upload/draw of skin bodies + a process-wide job table |
//! | [`field_gpu`] | the creature "field" lighting pass (egui/wgpu callback) |
//!
//! A fifth module, `relight_gpu`, only appears as local `t` symbols
//! (`RelightGpu::new/run/copy/measure/light_pass` at `0x10096d838` ...); it is
//! not part of the stub set and is therefore not reconstructed here.
//!
//! ## Standard-library-only departure
//!
//! The original depended on `wgpu`, `egui`/`egui_wgpu`, `image`, `tracing`,
//! `parking_lot`, `toml_edit` and (through `gs_puppet`) a mesh/pose model.
//! None of those are available here (no network, no vendor cache), so:
//!
//! * every GPU touch point became a call into the no-op seam [`gpu::Gpu`]
//!   ([`gpu::NullGpu`] is the default implementation), and
//! * the model types that used to come from `gs_puppet` are defined locally
//!   with the field offsets observed in the disassembly and marked
//!   `// INTEGRATION:`.
//!
//! Nothing here fabricates GPU behaviour: uploads and draws are *described* by
//! faithful layout descriptors ([`gpu::TextureUpload`], [`gpu::DrawCall`]) and
//! handed to the seam, which a real backend can implement later.

pub mod field_gpu;
pub mod figure_image;
pub mod puppet_cache;
pub mod skin_gpu;

/// The trait seam for the parts of the original that actually touched `wgpu`.
///
/// Every method has a no-op default so [`gpu::NullGpu`] — and any test double —
/// needs no boilerplate. The descriptors passed in are reconstructed from the
/// binary (buffer strides, texture formats and row pitches are the recovered
/// constants, see [`gpu`]).
pub mod gpu {
    /// Texture formats the binary asks `wgpu` for.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TexFormat {
        /// `wgpu::TextureFormat::Rgba8UnormSrgb` — the skin sheet path.
        Rgba8UnormSrgb,
        /// `wgpu::TextureFormat::Rgba8Unorm` — the relight/field targets.
        Rgba8Unorm,
        /// `wgpu::TextureFormat::Rgba16Float` — the HDR field frame.
        Rgba16Float,
    }

    impl TexFormat {
        /// Bytes per pixel. `Rgba16Float` is 4 x 2 bytes, the 8-bit formats 4.
        pub const fn bytes_per_pixel(self) -> u32 {
            match self {
                TexFormat::Rgba8UnormSrgb | TexFormat::Rgba8Unorm => 4,
                TexFormat::Rgba16Float => 8,
            }
        }
    }

    /// A texture upload request (`Queue::write_texture` in the original).
    ///
    /// `bytes_per_row` is `width * bytes_per_pixel` for every caller in this
    /// crate: `upload_rgba` computes it with `mul w8, w25, w24` at
    /// `0x10091d2c8`, i.e. `depth_or_layers * bytes_per_pixel`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TextureUpload<'a> {
        pub label: &'a str,
        pub width: u32,
        pub height: u32,
        pub format: TexFormat,
        pub mip_levels: u32,
        /// Row pitch in bytes.
        pub bytes_per_row: u32,
        pub data: &'a [u8],
    }

    impl<'a> TextureUpload<'a> {
        /// Build the descriptor and derive `bytes_per_row` the way the binary does.
        pub fn new(
            label: &'a str,
            width: u32,
            height: u32,
            format: TexFormat,
            mip_levels: u32,
            data: &'a [u8],
        ) -> Self {
            let bytes_per_row = width.saturating_mul(format.bytes_per_pixel());
            TextureUpload {
                label,
                width,
                height,
                format,
                mip_levels,
                bytes_per_row,
                data,
            }
        }

        /// True when `data` can hold `height` rows of `bytes_per_row` bytes.
        pub fn is_consistent(&self) -> bool {
            (self.data.len() as u64) >= self.bytes_per_row as u64 * self.height as u64
        }
    }

    /// One draw of a vertex buffer (`RenderPass::set_pipeline` + `draw`).
    #[derive(Debug, Clone, PartialEq)]
    pub struct DrawCall {
        /// Pipeline label, e.g. `"field_gpu opaque"` / `"skin_gpu"`.
        pub pipeline: &'static str,
        /// Index of the bound texture, when the pass samples one.
        pub texture: Option<u64>,
        pub vertex_offset: u32,
        pub vertex_count: u32,
        /// Depth span of the batch, used for the shadow/bounce ordering
        /// (see [`crate::skin_gpu::SkinJob::depth_span`]).
        pub depth: (f32, f32),
    }

    /// The no-op GPU seam.
    pub trait Gpu {
        /// Upload or update a texture.
        fn write_texture(&mut self, _upload: &TextureUpload<'_>) {}
        /// Record a draw.
        fn draw(&mut self, _call: &DrawCall) {}
        /// Create a render target (`Device::create_texture` in the original).
        fn create_target(&mut self, _label: &str, _width: u32, _height: u32, _format: TexFormat) {}
    }

    /// The default backend: does nothing, records nothing.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct NullGpu;

    impl Gpu for NullGpu {}
}

/// Errors used by the fallible loaders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn msg(m: impl Into<String>) -> Self {
        Error(m.into())
    }
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::gpu::*;

    #[test]
    fn texformat_pixel_sizes() {
        assert_eq!(TexFormat::Rgba8UnormSrgb.bytes_per_pixel(), 4);
        assert_eq!(TexFormat::Rgba8Unorm.bytes_per_pixel(), 4);
        assert_eq!(TexFormat::Rgba16Float.bytes_per_pixel(), 8);
    }

    #[test]
    fn upload_row_pitch_is_width_times_bpp() {
        let data = vec![0u8; 16 * 8 * 4];
        let up = TextureUpload::new("sheet", 16, 8, TexFormat::Rgba8UnormSrgb, 1, &data);
        assert_eq!(up.bytes_per_row, 64);
        assert!(up.is_consistent());
        let short = TextureUpload::new("sheet", 16, 8, TexFormat::Rgba8UnormSrgb, 1, &data[..63]);
        assert!(!short.is_consistent());
    }

    #[test]
    fn null_gpu_accepts_everything() {
        let mut g = NullGpu;
        let data = [0u8; 4];
        g.write_texture(&TextureUpload::new(
            "t",
            1,
            1,
            TexFormat::Rgba8Unorm,
            1,
            &data,
        ));
        g.create_target("f", 4, 4, TexFormat::Rgba16Float);
        g.draw(&DrawCall {
            pipeline: "field_gpu opaque",
            texture: None,
            vertex_offset: 0,
            vertex_count: 3,
            depth: (0.0, 1.0),
        });
    }
}
