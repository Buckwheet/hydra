//! Gaussian-splat clouds: file discovery, `.spz` / `.ply` decoding, projection
//! and the (seamed) draw pass.
//!
//! This module is the public seam `gs_render` consumes: its
//! `puppet_cache::SplatKey` has a `From<gs_puppet::splat::SplatParams>` impl, and
//! the quantisation that impl performs pins down [`SplatParams`] completely — see
//! [`SplatParams::quantised`].
//!
//! Recovered file formats:
//!
//! **[`decode_spz`]** (`@0x1009ff05c`, `gs_puppet::splat::decode_spz`).  The
//! header is 32 bytes and is validated field by field:
//!
//! | offset | type | meaning |
//! |---|---|---|
//! | `0x00` | `[u8;4]` | magic `b"NGSP"` (compared as the `u32` `0x5053474e`) |
//! | `0x04` | `u32` | version, **must be 4** (`cmp w8, #0x4`) |
//! | `0x08` | `u32` | point count (`num_points`) |
//! | `0x0c` | `u8` | SH degree, must be `< 5` |
//! | `0x0d` | `u8` | fractional bits for the fixed-point positions |
//! | `0x0e` | `u8` | flags |
//! | `0x0f` | `u8` | number of streams in the table of contents |
//! | `0x10` | `u32` | byte offset of the table of contents |
//!
//! Rejections, with the exact messages the binary builds:
//! `"too short for a header"` (`len < 0x20`), `"not an spz file (no NGSP
//! magic)"`, `"version N is not read, only 4"`, `"harmonics degree N is not
//! read, only 4"` (degrees 0..=4), `"a gzip spz (version 1..3) is not read"`
//! (gzip magic `1f 8b`), `"N streams, expected M"`, `"table of contents past the
//! end"`, `"stream past the end"`, `"gap > 0 has a next"`.
//!
//! The table of contents is a list of 16-byte records `{ u64 compressed_len, u64
//! raw_len }` starting at `toc_offset`, and each raw stream length is checked
//! against the per-point sizes the code computes once (`9·n`, `3·n`, `4·n`,
//! `sh·3·n`, where `sh` is `0 / 3 / 8 / 15` for degree `0 / 1 / 2 / 3` — the
//! `u64` table at `0x1021dc2a8`).
//!
//! # The draw seam
//!
//! [`render`] is the seam where the original issued a `wgpu` draw.  With no GPU
//! here, the CPU side of that draw is expressed as `epaint` values instead:
//! [`Footprint`] is one splat's projected billboard (the exact maths of the
//! embedded `splat_gpu.wgsl` / `skin_gpu` vertex shader, constants included), and
//! [`mesh`] / [`solid_mesh`] / [`prepared_mesh`] turn the far-to-near list
//! [`prepare`] returns into an [`epaint::Mesh`] of one billboard quad per splat —
//! the value an egui painter consumes.  The per-fragment Gaussian falloff stays in
//! the texture's alpha channel; see [`mesh`].
//!
//! **[`load_ply`]** (`@0x100a02a90`) is the 3-D Gaussian-splatting `.ply`:
//! `no end_header`, `only float properties are read`, and the property names
//! `xyz f_dc_0 f_dc_1 f_dc_2 opacity scale_0 scale_1 scale_2 rot_0 rot_1 rot_2
//! rot_3` all survive as literals.

use crate::math::{quat_norm, quat_to_m3, Mat3, Quat, Vec3, Xf};
use crate::model::PuppetError;
use crate::pose::Posed;
use std::io;
use std::path::{Path, PathBuf};

/// Size of one decoded splat, in bytes.
///
/// `decode_spz` multiplies the point count by `0x5c` (= 92) to allocate the
/// splat vector, and stores: position at `+0x00`, a 36-byte row-major basis at
/// `+0x0c`, then eight more bytes at `+0x30` and a colour word at `+0x3c`.
pub const SPLAT_STRIDE: usize = 0x5c;

/// Size of one splat as the draw pass uploads it: **144 bytes, nine `vec4`s**.
///
/// The embedded `splat_gpu.wgsl` declares the instance and labels it
/// `// one splat at rest (144 bytes)`; its nine `vec4<f32>` members are `c`
/// (centre, `w` glow), `ax0` (`w` film), `ax1` (`w` arrival frame), `ax2`
/// (`w` own-facing mark), `color` (rgb + alpha), `n` (facing, `w` wet), `bones`,
/// `shares` and `fx` (`x` the rim).  `9 * 16 = 144`, so the decoded
/// [`SPLAT_STRIDE`] record is repacked into this before the draw — the two
/// `bones`/`shares` members are the [`SkinWeights`] pair.
pub const SPLAT_INSTANCE_STRIDE: usize = 144;

/// Per-point byte counts of the four `.spz` streams, in table-of-contents order.
///
/// From `decode_spz`: `add x9, x20, x20, lsl #3` (= `9n`), `add x10, x20, x20,
/// lsl #1` (= `3n`), `lsl x11, x20, #2` (= `4n`) and `mul x8, x8, x10` where `x8`
/// comes from the `[0, 3, 8, 15][sh_degree]` table (= `sh·3n`).
pub const SPZ_STREAM_FACTORS: [u64; 4] = [9, 3, 4, 1];

/// SH coefficient counts per degree, the `u64` table at `0x1021dc2a8`.
pub const SPZ_SH_COEFFS: [u64; 5] = [0, 3, 8, 15, 15];

/// The SH "DC" factor `0.28209479177387814` (`1 / (2·sqrt(π))`): the 3-D
/// Gaussian-splatting `.ply` files store `f_dc_*` pre-divided by it.
pub const SH_C0: f32 = 0.282_094_8;

/// The knobs the render cache keys a splat cloud on.
///
/// # Recovered shape
///
/// `gs_render::puppet_cache`'s `impl From<SplatParams> for SplatKey`
/// (`@0x1008f1cb4`) reads **exactly three `f32`s**, at offsets `0`, `4` and `8`,
/// and packs them into one `u64`:
///
/// ```text
/// a = clamp(round(p0 * 1000.0), 5.0, 500.0)  as u32, capped at 0xffff
/// b = clamp(round(p1 *  100.0), 20.0, 200.0) as u32, capped at 0xffff
/// c = clamp(round(p2 *  100.0), 5.0, 100.0)  as u32, capped at 0xff
/// key = a | b << 16 | c << 32
/// ```
///
/// (`0x447a0000` = 1000.0, `0x43fa0000` = 500.0, `0x42c80000` = 100.0,
/// `0x43480000` = 200.0, `0xffff`, `0xff`.)  The three ranges tell us what the
/// fields mean:
///
/// * `radius` is a world-space size in model units, `0.005 ..= 0.5`;
/// * `opacity` is a multiplier around 1.0, `0.2 ..= 2.0`;
/// * `detail` is a kept fraction, `0.05 ..= 1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplatParams {
    /// World-space radius of one splat, in model units.
    pub radius: f32,
    /// Opacity multiplier (1.0 keeps the authoring value).
    pub opacity: f32,
    /// Fraction of the splats to keep (`1.0` keeps all of them).
    pub detail: f32,
}

impl Default for SplatParams {
    fn default() -> SplatParams {
        SplatParams {
            radius: 0.05,
            opacity: 1.0,
            detail: 1.0,
        }
    }
}

impl SplatParams {
    pub fn new(radius: f32, opacity: f32, detail: f32) -> SplatParams {
        SplatParams {
            radius,
            opacity,
            detail,
        }
    }

    /// The quantised triple the render cache keys on.
    ///
    /// This is the *entire* body of `From<SplatParams> for SplatKey`; the
    /// recovered clamps and shifts are reproduced exactly, including the `u16` /
    /// `u8` saturation.
    pub fn quantised(&self) -> (u32, u32, u32) {
        let r = (self.radius * 1000.0).round().clamp(5.0, 500.0) as u32;
        let o = (self.opacity * 100.0).round().clamp(20.0, 200.0) as u32;
        let d = (self.detail * 100.0).round().clamp(5.0, 100.0) as u32;
        (r.min(0xffff), o.min(0xffff), d.min(0xff))
    }

    /// The packed `u64` the `SplatKey` stores (`a | b << 16 | c << 32`).
    pub fn key_bits(&self) -> u64 {
        let (a, b, c) = self.quantised();
        a as u64 | (b as u64) << 16 | (c as u64) << 32
    }
}

/// One Gaussian splat.
///
/// Field offsets are the recovered ones; the trailing part of the 92-byte record
/// is not fully pinned down, so only the evidenced fields are modelled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Splat {
    /// Centre, `+0x00`.  `.spz` positions are fixed point and are stored with
    /// **y and z negated** (`scvtf` then `fnmul` on the 2nd and 3rd component),
    /// which flips the right-handed file space into the rig's front-is-+z space.
    pub pos: Vec3,
    /// Orientation times per-axis scale, row-major at `+0x0c`.
    pub basis: Mat3,
    /// RGBA, `+0x30` (the region `.spz` decodes colours into).
    pub color: [f32; 4],
    /// Mesh vertex (or bone) this splat is bound to; `u32::MAX` when unbound.
    pub vertex: u32,
    /// Depth cached by [`prepare`] for far-to-near sorting.
    pub depth: f32,
}

impl Default for Splat {
    fn default() -> Splat {
        Splat::new(Vec3::ZERO, Mat3::IDENTITY, [1.0, 1.0, 1.0, 1.0])
    }
}

impl Splat {
    pub fn new(pos: Vec3, basis: Mat3, color: [f32; 4]) -> Splat {
        Splat {
            pos,
            basis,
            color,
            vertex: u32::MAX,
            depth: 0.0,
        }
    }

    /// Build a splat from a position, a quaternion and an opacity.
    pub fn from_quat(pos: Vec3, q: &Quat, opacity: f32) -> Splat {
        Splat::new(pos, quat_to_m3(q), [1.0, 1.0, 1.0, opacity])
    }

    /// Apply a rigid transform to position and basis.
    pub fn transformed(&self, xf: &Xf) -> Splat {
        Splat {
            pos: xf.point(self.pos),
            basis: xf.m.mul_mat3(self.basis),
            color: self.color,
            vertex: self.vertex,
            depth: self.depth,
        }
    }

    /// Largest half-extent of the basis — the radius the GPU pass dilates by.
    pub fn radius(&self) -> f32 {
        let m = &self.basis.m;
        let c0 = Vec3::new(m[0], m[3], m[6]).length();
        let c1 = Vec3::new(m[1], m[4], m[7]).length();
        let c2 = Vec3::new(m[2], m[5], m[8]).length();
        c0.max(c1).max(c2)
    }

    pub fn is_finite(&self) -> bool {
        self.pos.is_finite() && self.color.iter().all(|c| c.is_finite())
    }
}

/// A whole decoded cloud plus its bounds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SplatCloud {
    pub splats: Vec<Splat>,
    /// Axis-aligned bounds `(min, max)`; `(ZERO, ZERO)` when empty.
    pub bounds: (Vec3, Vec3),
}

impl SplatCloud {
    pub fn new(splats: Vec<Splat>) -> SplatCloud {
        let mut lo = Vec3::MAX;
        let mut hi = Vec3::MIN;
        for s in &splats {
            lo = lo.min(s.pos);
            hi = hi.max(s.pos);
        }
        if splats.is_empty() {
            lo = Vec3::ZERO;
            hi = Vec3::ZERO;
        }
        SplatCloud {
            splats,
            bounds: (lo, hi),
        }
    }

    pub fn len(&self) -> usize {
        self.splats.len()
    }

    pub fn is_empty(&self) -> bool {
        self.splats.is_empty()
    }

    /// Sort far to near for the given eye point (`SplatBody::far_to_near`).
    pub fn far_to_near(&mut self, eye: Vec3) {
        for s in &mut self.splats {
            s.depth = (s.pos - eye).length_squared();
        }
        self.splats
            .sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    }
}

// ---------------------------------------------------------------------------
// file discovery
// ---------------------------------------------------------------------------

/// Find the cloud file for `stem` inside `dir`.
///
/// `@0x1009fef00` — `gs_puppet::splat::cloud_file(&Path, &str) -> PathBuf`.  It
/// formats `"{stem}.spz"` (literal `.spz` at `0x103e3550e`), joins it onto the
/// directory, returns it when `is_file()` succeeds and otherwise falls back to
/// `"{stem}.ply"` (literal `.ply` at `0x103e35515`) — note the fallback path is
/// **not** existence-checked, so a missing file surfaces as an I/O error from the
/// reader.
pub fn cloud_file(dir: &Path, stem: &str) -> PathBuf {
    let spz = dir.join(format!("{stem}.spz"));
    if spz.is_file() {
        return spz;
    }
    dir.join(format!("{stem}.ply"))
}

/// Decode a cloud from disk, choosing the decoder by extension.
///
/// `@0x100a00290` — `load_cloud(&Path) -> io::Result<…>`; the binary compares the
/// extension (`.ply` goes to [`load_ply`], everything else to [`decode_spz`]).
pub fn load_cloud(path: &Path) -> io::Result<SplatCloud> {
    let bytes = std::fs::read(path)?;
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("ply"))
    {
        Some(true) => load_ply(&bytes),
        _ => decode_spz(&bytes),
    }
}

fn bad(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

fn u64_at(b: &[u8], off: usize) -> u64 {
    let mut w = [0u8; 8];
    w.copy_from_slice(&b[off..off + 8]);
    u64::from_le_bytes(w)
}

// ---------------------------------------------------------------------------
// .spz
// ---------------------------------------------------------------------------

/// Header of a version-4 `.spz` file — the 32 bytes [`decode_spz`] validates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpzHeader {
    pub version: u32,
    pub num_points: u32,
    /// Spherical-harmonics degree, `0..=4`.
    pub sh_degree: u8,
    /// Fixed-point fractional bits for the positions.
    pub fractional_bits: u8,
    pub flags: u8,
    /// Number of records in the table of contents (4 in every v4 file).
    pub streams: u8,
    /// Byte offset of the table of contents.
    pub toc_offset: u32,
}

impl SpzHeader {
    /// Byte length of the fixed header.
    pub const LEN: usize = 0x20;

    /// Parse and validate a header.
    ///
    /// The error strings are the recovered ones.
    pub fn parse(bytes: &[u8]) -> io::Result<SpzHeader> {
        if bytes.len() < SpzHeader::LEN {
            return Err(bad("too short for a header"));
        }
        if &bytes[0..4] != b"NGSP" {
            if bytes[0] == 0x1f && bytes[1] == 0x8b {
                return Err(bad("a gzip spz (version 1..3) is not read"));
            }
            return Err(bad("not an spz file (no NGSP magic)"));
        }
        let version = u32_at(bytes, 4);
        if version != 4 {
            return Err(bad(format!(
                "version {version} is not read, only 4"
            )));
        }
        let num_points = u32_at(bytes, 8);
        let sh_degree = bytes[0x0c];
        if sh_degree >= 5 {
            return Err(bad(format!(
                "harmonics degree {sh_degree} is not read, only 4"
            )));
        }
        Ok(SpzHeader {
            version,
            num_points,
            sh_degree,
            fractional_bits: bytes[0x0d],
            flags: bytes[0x0e],
            streams: bytes[0x0f],
            toc_offset: u32_at(bytes, 0x10),
        })
    }

    /// Position scale: `2^-fractional_bits`.
    ///
    /// `decode_spz` builds this bit-exactly as `(min(fb,31) << 23) ^ 0x3f800000`,
    /// i.e. it decrements the `1.0f32` exponent by the fractional-bit count.
    pub fn position_scale(&self) -> f32 {
        let fb = self.fractional_bits.min(31) as u32;
        f32::from_bits((fb << 23) ^ 0x3f80_0000)
    }

    /// The expected raw byte length of stream `i` for a point count.
    ///
    /// Streams are `positions` (`9n`), `colours` (`3n`), `rotation + opacity`
    /// (`4n`) and spherical harmonics (`sh·3n`).  The last factor is `1` in
    /// [`SPZ_STREAM_FACTORS`] because the SH count already carries the `3`.
    pub fn stream_len(&self, i: usize, num_points: u32) -> u64 {
        let n = num_points as u64;
        match i {
            0 => SPZ_STREAM_FACTORS[0] * n,
            1 => SPZ_STREAM_FACTORS[1] * n,
            2 => SPZ_STREAM_FACTORS[2] * n,
            _ => {
                let sh = SPZ_SH_COEFFS[self.sh_degree as usize];
                sh * 3 * n
            }
        }
    }
}

/// Decode a `.spz` version-4 cloud.
///
/// `@0x1009ff05c` — `gs_puppet::splat::decode_spz(&[u8]) -> io::Result<Vec<Splat>>`
/// (the caller wraps it in a cloud).  The header and table-of-contents handling is
/// established; the *contents* of each stream are only partly recovered, which is
/// marked below.
pub fn decode_spz(bytes: &[u8]) -> io::Result<SplatCloud> {
    let h = SpzHeader::parse(bytes)?;
    let n = h.num_points as usize;
    if n == 0 {
        return Ok(SplatCloud::default());
    }
    // Table of contents: `streams` records of { u64 compressed_len, u64 raw_len }.
    let toc_start = h.toc_offset as usize;
    let toc_bytes = h.streams as usize * 16;
    if toc_start.checked_add(toc_bytes).map_or(true, |e| e > bytes.len()) {
        return Err(bad("table of contents past the end"));
    }
    let mut streams: Vec<(u64, u64)> = Vec::with_capacity(h.streams as usize);
    for i in 0..h.streams as usize {
        let off = toc_start + i * 16;
        streams.push((u64_at(bytes, off), u64_at(bytes, off + 8)));
    }
    // The binary requires exactly four streams and checks each raw length against
    // the arithmetic sizes; anything else is a hard error.
    if streams.len() != SPZ_STREAM_FACTORS.len() {
        return Err(bad(format!(
            "{} streams, expected {}",
            streams.len(),
            SPZ_STREAM_FACTORS.len()
        )));
    }
    let mut raw: Vec<Vec<u8>> = Vec::with_capacity(streams.len());
    let mut cursor = toc_start + toc_bytes;
    for (i, (compressed_len, raw_len)) in streams.iter().enumerate() {
        let expect = h.stream_len(i, h.num_points);
        if *raw_len != expect {
            return Err(bad(format!(
                "stream {i} is {} bytes, expected {}",
                raw_len, expect
            )));
        }
        let end = cursor
            .checked_add(*compressed_len as usize)
            .ok_or_else(|| bad("stream past the end"))?;
        if end > bytes.len() {
            return Err(bad("stream past the end"));
        }
        let body = &bytes[cursor..end];
        cursor = end;
        // A stream whose compressed length equals its raw length is stored
        // verbatim; otherwise it is a Zstandard frame (the binary links `ruzstd`
        // and calls `FrameDecoder::decode_all_to_vec`).
        let data = if *compressed_len == *raw_len {
            body.to_vec()
        } else {
            zstd_decode(body, *raw_len as usize)?
        };
        raw.push(data);
    }

    let scale = h.position_scale();
    let pos = &raw[0];
    let col = &raw[1];
    let roto = &raw[2];
    let mut splats = Vec::with_capacity(n);
    for i in 0..n {
        // Positions: three little-endian 24-bit signed fixed-point values.
        let p = i * 9;
        let x = read_i24(&pos[p..p + 3]) as f32 * scale;
        let y = read_i24(&pos[p + 3..p + 6]) as f32 * scale;
        let z = read_i24(&pos[p + 6..p + 9]) as f32 * scale;
        // The recovered stores negate the 2nd and 3rd component (`fnmul` against
        // the scale), flipping the file's right-handed space into the rig's.
        let pos_v = Vec3::new(x, -y, -z);
        // Colours: three bytes, `/255`.
        let c = i * 3;
        let color = [
            col[c] as f32 / 255.0,
            col[c + 1] as f32 / 255.0,
            col[c + 2] as f32 / 255.0,
            1.0,
        ];
        // Rotation + opacity: UNKNOWN exact split.  The decoder reconstructs the
        // quaternion with the ±√½ (`0x3f3504f3`) constant that the smallest-three
        // encoding needs and stores a 3x3 basis, so the rotation is the first three
        // bytes and opacity the fourth.
        let r = i * 4;
        let q = smallest_three(&roto[r..r + 3]);
        let opacity = roto[r + 3] as f32 / 255.0;
        let mut splat = Splat::from_quat(pos_v, &q, opacity);
        splat.color[3] = opacity;
        splats.push(splat);
    }
    Ok(SplatCloud::new(splats))
}

fn read_i24(b: &[u8]) -> i32 {
    let v = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16);
    // Sign-extend from bit 23.
    (v << 8) >> 8
}

/// Rebuild a quaternion from three bytes using the smallest-three encoding.
///
/// `decode_spz` relocates the implicit largest component using `1/√2`
/// (`0x3f3504f3`) and `1.0`, then normalises through [`crate::math::quat_norm`]
/// and converts with [`crate::math::quat_to_m3`]; the exact bit packing of the
/// three bytes is **not** recoverable, so the straightforward
/// `component << 2 | index` split is used and documented as inferred.
fn smallest_three(b: &[u8]) -> Quat {
    let which = (b[0] & 0x3) as usize;
    let q = [
        ((b[0] >> 2) as f32 / 63.0) * 2.0 - 1.0,
        ((b[1] >> 2) as f32 / 63.0) * 2.0 - 1.0,
        ((b[2] >> 2) as f32 / 63.0) * 2.0 - 1.0,
    ];
    // Fill the missing component so the vector is unit length.
    let rest = (1.0 - q[0] * q[0] - q[1] * q[1] - q[2] * q[2]).max(0.0).sqrt();
    let mut xyzw = [0.0f32; 4];
    let mut k = 0;
    for (i, slot) in xyzw.iter_mut().enumerate() {
        if i == which {
            *slot = rest;
        } else {
            *slot = q[k];
            k += 1;
        }
    }
    // The stream stores (x, y, z, w); the crate's Quat is (w, x, y, z).
    quat_norm(Quat::new(xyzw[3], xyzw[0], xyzw[1], xyzw[2]))
}

// ---------------------------------------------------------------------------
// .ply
// ---------------------------------------------------------------------------

/// A parsed `.ply` header: the vertex element's float properties, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlyHeader {
    /// Property names of the `vertex` element, in file order.
    pub properties: Vec<String>,
    /// Vertex count.
    pub count: usize,
    /// Byte offset just past `end_header`.
    pub data_offset: usize,
    /// True for `format binary_little_endian`.
    pub binary_le: bool,
    /// True for `format ascii`.
    pub ascii: bool,
    /// Offset of each property inside one vertex record, for binary files.
    pub offsets: Vec<usize>,
}

impl PlyHeader {
    /// Parse a header out of `bytes`.
    ///
    /// Errors mirror the recovered literals: `"no end_header"` and
    /// `"only float properties are read"`.
    pub fn parse(bytes: &[u8]) -> io::Result<PlyHeader> {
        let mut h = PlyHeader::default();
        let mut in_vertex = false;
        let mut off = 0usize;
        let mut cursor = 0usize;
        loop {
            let rest = &bytes[cursor..];
            let nl = rest
                .iter()
                .position(|b| *b == b'\n')
                .ok_or_else(|| bad("no end_header"))?;
            let line = String::from_utf8_lossy(&rest[..nl]).trim().to_string();
            cursor += nl + 1;
            if line == "end_header" {
                h.data_offset = cursor;
                break;
            }
            let mut it = line.split_whitespace();
            match it.next() {
                Some("format") => match it.next() {
                    Some("ascii") => h.ascii = true,
                    Some("binary_little_endian") => h.binary_le = true,
                    Some(other) => {
                        return Err(bad(format!("format {other} is not read")));
                    }
                    None => return Err(bad("no end_header")),
                },
                Some("element") => {
                    let name = it.next().unwrap_or_default();
                    let count = it.next().and_then(|c| c.parse::<usize>().ok());
                    in_vertex = name == "vertex";
                    if in_vertex {
                        h.count = count.unwrap_or(0);
                    }
                }
                Some("property") => {
                    let ty = it.next().unwrap_or_default();
                    let name = it.next().unwrap_or_default().to_string();
                    if in_vertex {
                        if ty != "float" && ty != "float32" {
                            return Err(bad("only float properties are read"));
                        }
                        h.offsets.push(off);
                        off += 4;
                        h.properties.push(name);
                    }
                }
                _ => {}
            }
        }
        Ok(h)
    }

    /// Offset of a named property, if present.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.properties.iter().position(|p| p == name)
    }
}

/// Decode a 3-D Gaussian-splatting `.ply`.
///
/// `@0x100a02a90` — `gs_puppet::splat::load_ply(&[u8]) -> io::Result<…>`.  The
/// conventions are the standard ones for this format, and the binary's own calls
/// (`quat_norm`, `quat_to_m3`) confirm the rotation handling:
///
/// * `x, y, z` are positions;
/// * `f_dc_0..2` are SH DC coefficients, `colour = 0.5 + SH_C0 · dc`;
/// * `opacity` is a logit, `alpha = 1 / (1 + e^-opacity)`;
/// * `scale_0..2` are **log** scales, `s = e^scale`;
/// * `rot_0..3` are a quaternion stored `(w, x, y, z)`, normalised before use.
pub fn load_ply(bytes: &[u8]) -> io::Result<SplatCloud> {
    let h = PlyHeader::parse(bytes)?;
    let mut splats = Vec::with_capacity(h.count);
    let get = |name: &str| h.index_of(name);
    let (Some(ix), Some(iy), Some(iz)) = (get("x"), get("y"), get("z")) else {
        return Err(bad("no vertex positions"));
    };
    let f = |name: &str| get(name);
    if h.ascii {
        let text = String::from_utf8_lossy(&bytes[h.data_offset..]);
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let vals: Vec<f32> = line
                .split_whitespace()
                .map(|v| v.parse::<f32>().unwrap_or(0.0))
                .collect();
            if vals.len() < h.properties.len() {
                continue;
            }
            splats.push(ply_splat(&vals, &h, ix, iy, iz, f));
        }
    } else if h.binary_le {
        let stride: usize = h.properties.len() * 4;
        for i in 0..h.count {
            let base = h.data_offset + i * stride;
            if base + stride > bytes.len() {
                break;
            }
            let mut vals = Vec::with_capacity(h.properties.len());
            for &o in &h.offsets {
                let b = &bytes[base + o..base + o + 4];
                vals.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            }
            splats.push(ply_splat(&vals, &h, ix, iy, iz, f));
        }
    } else {
        return Err(bad("format is not read"));
    }
    Ok(SplatCloud::new(splats))
}

fn ply_splat(
    vals: &[f32],
    h: &PlyHeader,
    ix: usize,
    iy: usize,
    iz: usize,
    f: impl Fn(&str) -> Option<usize>,
) -> Splat {
    let get = |name: &str, dflt: f32| f(name).map(|i| vals[i]).unwrap_or(dflt);
    let pos = Vec3::new(vals[ix], vals[iy], vals[iz]);
    let color = [
        0.5 + SH_C0 * get("f_dc_0", 0.0),
        0.5 + SH_C0 * get("f_dc_1", 0.0),
        0.5 + SH_C0 * get("f_dc_2", 0.0),
        1.0,
    ];
    let alpha = 1.0 / (1.0 + (-get("opacity", 4.0)).exp());
    let sx = get("scale_0", 0.0).exp();
    let sy = get("scale_1", 0.0).exp();
    let sz = get("scale_2", 0.0).exp();
    let q = quat_norm(Quat::new(
        get("rot_0", 1.0),
        get("rot_1", 0.0),
        get("rot_2", 0.0),
        get("rot_3", 0.0),
    ));
    let mut basis = quat_to_m3(&q);
    for r in 0..3 {
        basis.m[r * 3] *= sx;
        basis.m[r * 3 + 1] *= sy;
        basis.m[r * 3 + 2] *= sz;
    }
    let mut s = Splat::new(pos, basis, color);
    s.color[3] = alpha;
    let _ = h;
    s
}

// ---------------------------------------------------------------------------
// zstd (raw / RLE blocks only)
// ---------------------------------------------------------------------------

/// Decode a Zstandard frame.
///
/// `.spz` streams are Zstandard-compressed (the binary links `ruzstd` and calls
/// `FrameDecoder::decode_all_to_vec`).  This `std`-only stand-in understands the
/// frame header and *Raw* / *RLE* blocks, which is enough to round-trip a stream
/// that was stored without entropy coding; a `Compressed` block returns an error
/// rather than garbage.  **std departure** (was `ruzstd`).
pub fn zstd_decode(src: &[u8], expected: usize) -> io::Result<Vec<u8>> {
    if src.len() < 4 {
        return Err(bad("not a zstd frame"));
    }
    let magic = u32_at(src, 0);
    if magic != 0xFD2F_B528 {
        return Err(bad("not a zstd frame"));
    }
    let fhd = src[4];
    let single_segment = fhd & 0x20 != 0;
    let dict_flag = fhd & 0x03;
    let checksum = fhd & 0x04 != 0;
    let fcs_flag = (fhd >> 6) & 0x03;
    let mut p = 5;
    // Window descriptor, unless the frame is a single segment.
    if !single_segment {
        p += 1;
    }
    // Dictionary id.
    p += [0usize, 1, 2, 4][dict_flag as usize];
    // Frame content size.
    p += [if single_segment { 1 } else { 0 }, 2, 4, 8][fcs_flag as usize];
    let mut out: Vec<u8> = Vec::with_capacity(expected);
    loop {
        if p + 3 > src.len() {
            return Err(bad("truncated zstd block header"));
        }
        let bh = src[p] as u32 | (src[p + 1] as u32) << 8 | (src[p + 2] as u32) << 16;
        p += 3;
        let last = bh & 1 != 0;
        let btype = (bh >> 1) & 0x03;
        let size = (bh >> 3) as usize;
        match btype {
            0 => {
                // Raw block.
                if p + size > src.len() {
                    return Err(bad("truncated zstd raw block"));
                }
                out.extend_from_slice(&src[p..p + size]);
                p += size;
            }
            1 => {
                // RLE block: one byte repeated `size` times.
                if p >= src.len() {
                    return Err(bad("truncated zstd rle block"));
                }
                out.extend(std::iter::repeat(src[p]).take(size));
                p += 1;
            }
            2 => return Err(bad("compressed zstd blocks are not decoded")),
            _ => return Err(bad("reserved zstd block type")),
        }
        if last {
            break;
        }
    }
    if checksum && p + 4 <= src.len() {
        p += 4;
    }
    let _ = p;
    Ok(out)
}

// ---------------------------------------------------------------------------
// synthesis, binding and the draw pass
// ---------------------------------------------------------------------------

/// Build a splat cloud from a mesh, its UV layout and a texture.
///
/// `@0x100a00948` — `gs_puppet::splat::cloud(…)`.  The recovered body samples the
/// UV atlas (`UvLayout::locate` / `Texture::sample`) once per splat, so the cloud
/// carries per-splat colours taken from the skin texture.  The exact sampling
/// weights could not be recovered; the structure (one splat per vertex, radius
/// from [`SplatParams`], colour from the texture) is kept.
pub fn cloud(
    mesh: &crate::mesh::Mesh,
    layout: &crate::uv::UvLayout,
    texture: Option<&crate::uv::Texture>,
    params: &SplatParams,
) -> SplatCloud {
    let mut splats = Vec::with_capacity(mesh.vertices.len());
    let keep = params.detail.clamp(0.0, 1.0);
    for (i, v) in mesh.vertices.iter().enumerate() {
        if keep < 1.0 {
            // Deterministic thinning: keep every k-th splat.
            let step = (1.0 / keep.max(1e-3)).round() as usize;
            if step > 1 && i % step != 0 {
                continue;
            }
        }
        let color = match (texture, layout.address(v.pos)) {
            (Some(t), Some((_, uv))) => {
                let c = t.sample(uv);
                [c[0], c[1], c[2], params.opacity]
            }
            _ => [0.8, 0.75, 0.7, params.opacity],
        };
        let mut s = Splat::new(
            v.pos,
            Mat3::from_cols_array([
                params.radius,
                0.0,
                0.0,
                0.0,
                params.radius,
                0.0,
                0.0,
                0.0,
                params.radius,
            ]),
            color,
        );
        s.vertex = i as u32;
        splats.push(s);
    }
    SplatCloud::new(splats)
}

/// A splat after projection: screen position, radius in pixels and depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedSplat {
    /// Screen `(x, y)` in pixels.
    pub screen: [f32; 2],
    /// Radius in pixels.
    pub radius: f32,
    /// View-space depth (the camera's `project` third value).
    pub depth: f32,
    pub color: [f32; 4],
}

/// Project every splat and sort far to near.
///
/// `@0x100a01c14` — `gs_puppet::splat::prepare(&cloud, &Camera, &mut Vec<…>)`.
/// The recovered body projects with [`crate::camera::Camera::project`], scales the
/// splat radius by the projected depth (`focal / depth`) and sorts descending by
/// depth (`SplatBody::far_to_near`).
pub fn prepare(
    splats: &[Splat],
    camera: &crate::camera::Camera,
    budget: usize,
) -> Vec<PreparedSplat> {
    let mut out: Vec<PreparedSplat> = Vec::with_capacity(splats.len().min(budget.max(1)));
    for s in splats {
        let (x, y, depth) = camera.project(&s.pos);
        if depth <= 0.0 || !x.is_finite() || !y.is_finite() {
            continue;
        }
        let scale = camera.focal / depth;
        out.push(PreparedSplat {
            screen: [x, y],
            radius: s.radius() * scale,
            depth,
            color: s.color,
        });
    }
    out.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(budget);
    out
}

/// Draw a cloud.  **GPU seam.**
///
/// `@0x100a01444` — `gs_puppet::splat::render(…)` builds the GPU vertex buffer from
/// [`prepare`] and issues the splat draw.  With `wgpu` gone this returns the
/// projected splats so a CPU rasteriser (or the real renderer after the branches
/// merge) can consume them; nothing is silently dropped.
pub fn render(
    splats: &[Splat],
    camera: &crate::camera::Camera,
    budget: usize,
) -> Vec<PreparedSplat> {
    prepare(splats, camera, budget)
}

// ---------------------------------------------------------------------------
// the per-splat geometry the draw pass needs, and the `epaint` values it becomes
// ---------------------------------------------------------------------------
//
// `splat::render` issued a WGSL draw, and the two shaders the binary carries
// (`skin_gpu`'s splat passes and `splat_gpu.wgsl`, "the same three passes for a
// cloud the CPU projected") spell out, in their own comments and code, exactly
// what one projected splat is:
//
// ```wgsl
// let r = vec2<f32>(3.0 * sqrt(xx), 3.0 * sqrt(yy));   // half-extents, pixels
// let off = corner[k] * r * u.to_cam.w;                // reach = this scale
// let p = c.xy + off;
// o.inv = vec4<f32>(yy / det, -xy / det, -xy / det, xx / det);
// o.alpha = d.alpha * spread * weight;
// ```
//
// with `corner` the six-vertex triangle list
// `(-1,-1) (1,-1) (-1,1) (-1,1) (1,-1) (1,1)`, the covariance
// `xx = xx0 + 0.3`, `det = max(xx*yy - xy*xy, 1e-6)` and the falloff
// `a = min(v.alpha * exp(-0.5 * gauss_m(local, inv)), 0.99)`.  Everything below
// is that maths: [`Footprint`] is one splat's projected billboard, and
// [`mesh`] turns a prepared list into an `epaint::Mesh`.
//
// UNKNOWN: the same shaders name `gs_puppet::splat::Gaussian::rim` (a splat's
// rim, `0..=1`, scaled by `RIM_STEP`) and `gs_puppet::layers::ARRIVAL_AGES`.
// Neither has a symbol, a drop-glue layout or a literal that pins it down, so
// neither is reconstructed here — see NOTES.md, "Remaining UNKNOWNs".

/// `CORE_M` — a splat's core, in the shaders' Gaussian-M units.
///
/// The literal `2.25` is in both embedded WGSL copies (`skin_gpu`'s splat passes
/// and `splat_gpu.wgsl`), and each carries the comment naming its origin:
/// `// gs_puppet::splat::CORE_M and REACH_M: a splat's core, and its reach`.  The
/// front pass keeps `gauss_m <= CORE_M`.
pub const CORE_M: f32 = 2.25;

/// `REACH_M` — where a splat stops showing: the blend pass discards fragments
/// with `gauss_m > REACH_M`.  The quad's half-extent is `sqrt(REACH_M)` = 3
/// standard deviations, which is [`QUAD_SIGMAS`].
pub const REACH_M: f32 = 9.0;

/// `RIM_FROM` — where a rim splat (an invisible figure's skin) starts to show,
/// by how edge-on it is: `smoothstep(RIM_FROM, RIM_TO, 1 - |dot(n, back)|)`.
pub const RIM_FROM: f32 = 0.3;

/// `RIM_TO` — where a rim splat is whole (see [`RIM_FROM`]).
pub const RIM_TO: f32 = 0.8;

/// The billboard's half-extent in standard deviations: the shader's
/// `r = 3.0 * sqrt(xx)`, and `sqrt(REACH_M)`.
pub const QUAD_SIGMAS: f32 = 3.0;

/// The variance added to the projected covariance before the extent is taken
/// (the shader's `let xx = xx0 + 0.3`): a splat covers at least a pixel, and one
/// smaller than a pixel is spread over it — as much fainter as it was made wider
/// (`spread`, below).
pub const MIN_VARIANCE: f32 = 0.3;

/// The determinant floor of the inverse covariance: `max(xx*yy - xy*xy, 1e-6)`.
pub const MIN_DET: f32 = 1e-6;

/// The blend pass' alpha cap: `min(v.alpha * exp(-0.5 * m), 0.99)`.
pub const MAX_ALPHA: f32 = 0.99;

/// The shaders' `gauss_m(d, inv)`: the squared Mahalanobis distance of a
/// fragment from the splat's centre, in Gaussian-M units.
pub fn gauss_m(d: [f32; 2], inv: [f32; 4]) -> f32 {
    d[0] * (inv[0] * d[0] + inv[1] * d[1]) + d[1] * (inv[2] * d[0] + inv[3] * d[1])
}

/// The shaders' Gaussian falloff `exp(-0.5 * m)` (`m` in Gaussian-M units).
pub fn falloff(m: f32) -> f32 {
    (-0.5 * m).exp()
}

/// One splat as the draw pass sees it: the screen-space billboard the vertex
/// shader builds for it.
///
/// Recovered field by field from `vs_cloud`:
///
/// | field | shader |
/// |---|---|
/// | `centre` | `let c = seen(world); … p = c.xy + off` |
/// | `half` | `let r = vec2(3.0 * sqrt(xx), 3.0 * sqrt(yy))` |
/// | `inv` | `o.inv = vec4(yy / det, -xy / det, -xy / det, xx / det)` |
/// | `spread` | `sqrt(clamp((xx0 * yy0 - xy * xy) / det, 0.0, 1.0))` |
/// | `depth` | `o.z = c.z` |
/// | `alpha` | `o.alpha = d.alpha * spread * weight` (this is `d.alpha`) |
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footprint {
    /// Screen centre of the splat, in pixels (`c.xy`).
    pub centre: [f32; 2],
    /// Half-extents of the billboard quad, in pixels (`r`, three sigma).
    pub half: [f32; 2],
    /// The inverse of the screen covariance, row-major (`o.inv`).
    pub inv: [f32; 4],
    /// How much of the splat survived the sub-pixel spread, `0.0..=1.0`.
    pub spread: f32,
    /// View-space depth (`o.z`), as [`crate::camera::Camera::project`] reports it.
    pub depth: f32,
    /// The splat's own alpha, before `spread` and the Gaussian falloff.
    pub alpha: f32,
}

impl Footprint {
    /// The four billboard corners, in the shaders' order:
    /// `(-1,-1) (1,-1) (-1,1) (1,1)` scaled by [`Footprint::half`] about
    /// [`Footprint::centre`] — the corners of the shader's six-vertex triangle
    /// list, which is the two triangles `(0,1,2)` and `(2,1,3)`.
    pub fn corners(&self) -> [[f32; 2]; 4] {
        let [cx, cy] = self.centre;
        let [hx, hy] = self.half;
        [
            [cx - hx, cy - hy],
            [cx + hx, cy - hy],
            [cx - hx, cy + hy],
            [cx + hx, cy + hy],
        ]
    }

    /// A corner's offset from the centre, in pixels — the shader's `off`, which
    /// is the `local` the falloff is evaluated at.
    pub fn local(&self, corner: [f32; 2]) -> [f32; 2] {
        [corner[0] * self.half[0], corner[1] * self.half[1]]
    }

    /// The Gaussian-M distance of a local offset — [`gauss_m`] against [`Footprint::inv`].
    pub fn gauss_at(&self, local: [f32; 2]) -> f32 {
        gauss_m(local, self.inv)
    }

    /// The falloff at a local offset, `exp(-0.5 * gauss_at(local))`.
    pub fn falloff_at(&self, local: [f32; 2]) -> f32 {
        falloff(self.gauss_at(local))
    }

    /// The peak alpha a fragment reaches at the centre:
    /// `min(alpha * spread, 0.99)`.
    pub fn peak_alpha(&self) -> f32 {
        (self.alpha * self.spread).clamp(0.0, MAX_ALPHA)
    }

    /// The screen-space area of the quad, in square pixels.
    pub fn area(&self) -> f32 {
        4.0 * self.half[0] * self.half[1]
    }

    pub fn is_finite(&self) -> bool {
        self.centre.iter().all(|v| v.is_finite())
            && self.half.iter().all(|v| v.is_finite())
            && self.inv.iter().all(|v| v.is_finite())
            && self.spread.is_finite()
            && self.depth.is_finite()
            && self.alpha.is_finite()
    }

    /// The shader's covariance-to-billboard step, shared by both constructors.
    ///
    /// `xx0` / `yy0` / `xy` are the sums of squares and products of the splat's
    /// three projected axes; `xx = xx0 + MIN_VARIANCE` and `yy = yy0 +
    /// MIN_VARIANCE` as in `vs_cloud`.
    fn from_covariance(
        centre: [f32; 2],
        xx0: f32,
        yy0: f32,
        xy: f32,
        depth: f32,
        alpha: f32,
    ) -> Footprint {
        let xx = xx0 + MIN_VARIANCE;
        let yy = yy0 + MIN_VARIANCE;
        let det = (xx * yy - xy * xy).max(MIN_DET);
        let spread = (((xx0 * yy0 - xy * xy) / det).clamp(0.0, 1.0)).sqrt();
        Footprint {
            centre,
            half: [QUAD_SIGMAS * xx.sqrt(), QUAD_SIGMAS * yy.sqrt()],
            inv: [yy / det, -xy / det, -xy / det, xx / det],
            spread,
            depth,
            alpha,
        }
    }

    /// Project one splat through the camera: the full three-axis covariance of
    /// `vs_cloud`, evaluated on the CPU.
    ///
    /// Returns `None` for a splat that is not in front of the camera or whose
    /// projection is not finite (the shader parks such a vertex off-image).
    pub fn of(splat: &Splat, camera: &crate::camera::Camera) -> Option<Footprint> {
        let (cx, cy, depth) = camera.project(&splat.pos);
        if !cx.is_finite() || !cy.is_finite() || !(depth > 0.0) {
            return None;
        }
        let mut xx0 = 0.0;
        let mut yy0 = 0.0;
        let mut xy = 0.0;
        for k in 0..3 {
            let axis = splat.basis.col(k);
            let (ax, ay, _) = camera.project(&(splat.pos + axis));
            let (dx, dy) = (ax - cx, ay - cy);
            if !dx.is_finite() || !dy.is_finite() {
                return None;
            }
            xx0 += dx * dx;
            yy0 += dy * dy;
            xy += dx * dy;
        }
        Some(Footprint::from_covariance(
            [cx, cy],
            xx0,
            yy0,
            xy,
            depth,
            splat.color[3],
        ))
    }

    /// The same for an already-projected splat.
    ///
    /// [`PreparedSplat`] keeps one radius, not the covariance, so the recovered
    /// formulas run on the isotropic special case `xx0 = yy0 = radius²`, `xy = 0`
    /// (`xx0 * yy0 - xy * xy = radius⁴`, so `spread = radius² / (radius² + 0.3)`).
    pub fn of_prepared(p: &PreparedSplat) -> Footprint {
        let r = p.radius;
        Footprint::from_covariance(p.screen, r * r, r * r, 0.0, p.depth, p.color[3])
    }
}

impl PreparedSplat {
    /// This splat's billboard — see [`Footprint::of_prepared`].
    pub fn footprint(&self) -> Footprint {
        Footprint::of_prepared(self)
    }

    /// One splat's quad as its own `epaint::Mesh` (4 vertices, 6 indices).
    pub fn mesh(&self, texture: epaint::TextureId) -> epaint::Mesh {
        mesh(std::slice::from_ref(self), texture)
    }
}

/// The quad's UVs, in [`Footprint::corners`] order: the whole of a radial
/// gaussian texture, top-left first.
pub const SPLAT_UVS: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]];

fn byte(v: f32) -> u8 {
    // The recovered `255.0` byte scale (`0x437f0000`), rounded as the shader's
    // `round(… * 255.0) / 255.0` does.
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

/// A splat's colour at a given alpha, as an `epaint` vertex colour.
///
/// `epaint` wants premultiplied sRGB, which is also what the recovered blend
/// pass emits (`return vec4<f32>(v.rgb / 255.0 * a, a)`).
fn vertex_color(color: [f32; 4], alpha: f32) -> epaint::Color32 {
    let a = byte(alpha);
    let (r, g, b) = (byte(color[0]), byte(color[1]), byte(color[2]));
    epaint::Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn build_prepared(
    prepared: &[PreparedSplat],
    texture: epaint::TextureId,
    textured: bool,
) -> epaint::Mesh {
    let mut m = epaint::Mesh::with_texture(texture);
    m.vertices.reserve(prepared.len() * 4);
    m.indices.reserve(prepared.len() * 6);
    for p in prepared {
        let f = Footprint::of_prepared(p);
        if !f.is_finite() {
            continue;
        }
        let color = vertex_color(p.color, f.peak_alpha());
        let base = m.vertices.len() as u32;
        for (corner, uv) in f.corners().iter().zip(SPLAT_UVS.iter()) {
            m.vertices.push(epaint::Vertex {
                pos: epaint::pos2(corner[0], corner[1]),
                uv: if textured {
                    epaint::pos2(uv[0], uv[1])
                } else {
                    epaint::WHITE_UV
                },
                color,
            });
        }
        // The shaders' corner order is the triangle list
        // (-1,-1) (1,-1) (-1,1) / (-1,1) (1,-1) (1,1).
        m.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
    }
    m
}

/// Turn a prepared splat list into one `epaint::Mesh`: a textured billboard quad
/// per splat, in the list's order.
///
/// This is the CPU equivalent of the draw `splat::render` issued: `vs_cloud`'s
/// billboard (`Footprint`) with the recovered corner order and UVs into the
/// gaussian falloff texture `texture`, the splat's colour and its peak alpha on
/// every vertex.  **The Gaussian falloff itself is per-fragment in the shader**
/// (`min(alpha * exp(-0.5 * m), 0.99)`) and an `epaint` mesh can only vary alpha
/// per vertex, so it has to come from that texture's alpha channel — that is why
/// the UVs span the whole quad.  Pass [`solid_mesh`] for hard-edged quads with no
/// texture.  Splats that are not finite are dropped, as the shader parks them
/// off-image.
///
/// The order is [`prepare`]'s: sorted far to near, so a painter's-algorithm
/// consumer draws them in index order.
pub fn mesh(prepared: &[PreparedSplat], texture: epaint::TextureId) -> epaint::Mesh {
    build_prepared(prepared, texture, true)
}

/// The same quads with no texture: `TextureId::default()` and `WHITE_UV`, which
/// is `epaint`'s way of saying "use the vertex colour alone".
pub fn solid_mesh(prepared: &[PreparedSplat]) -> epaint::Mesh {
    build_prepared(prepared, epaint::TextureId::default(), false)
}

/// Project a cloud and build its `epaint::Mesh` in one step — [`prepare`] then
/// [`mesh`] — which is the whole CPU side of the recovered splat draw.
pub fn prepared_mesh(
    splats: &[Splat],
    camera: &crate::camera::Camera,
    budget: usize,
    texture: epaint::TextureId,
) -> epaint::Mesh {
    mesh(&prepare(splats, camera, budget), texture)
}

/// Bone weights for one splat: up to four `(bone, weight)` pairs.
///
/// `gs_puppet::splat::skin_weights` (a local symbol at `0x100a003cc`) picks the
/// nearest mesh vertices through `mesh::nearest_vertices` and normalises their
/// weights.  Modelled as a fixed four-slot weight set, which is what the
/// splat-binding code stores.
pub type SkinWeights = [(u16, f32); 4];

/// Normalise a weight set so the used slots sum to 1.0.
pub fn normalise_weights(w: &mut SkinWeights) {
    let sum: f32 = w.iter().map(|(_, k)| *k).sum();
    if sum > 0.0 && sum.is_finite() {
        for slot in w.iter_mut() {
            slot.1 /= sum;
        }
    } else {
        *w = [(0, 0.0); 4];
    }
}

/// A splat cloud bound to a rig: every splat knows which bone carries it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoundCloud {
    /// The unposed splats, in model space.
    pub cloud: SplatCloud,
    /// Per-splat weights into the bone list.
    pub weights: Vec<SkinWeights>,
    /// Bone names the weights index, in order.
    pub bones: Vec<String>,
    /// `true` when the cloud was bound rigidly (the flag byte at `+0x48` the
    /// `posed` walk tests with `and w8, w8, #1`).
    pub rigid: bool,
}

impl BoundCloud {
    pub fn new(cloud: SplatCloud, bones: Vec<String>) -> BoundCloud {
        let weights = vec![[(0u16, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]; cloud.len()];
        BoundCloud {
            cloud,
            weights,
            bones,
            rigid: false,
        }
    }

    /// Pose the cloud: transform every splat by its bone's world transform.
    ///
    /// `@0x1009fed98` — `gs_puppet::splat::BoundCloud::posed(&self, &[Xf]) -> Vec<Splat>`.
    /// The recovered body walks the 92-byte splats and the 48-byte transforms
    /// together (`csel x9, x12, x9, lo` clamps to the shorter of the two) in two
    /// branches: a rigid branch that copies through the `OnceLock` body, and the
    /// general weighted branch taken here.
    pub fn posed(&self, posed: &Posed) -> Vec<Splat> {
        let mut out = Vec::with_capacity(self.cloud.splats.len());
        for (i, s) in self.cloud.splats.iter().enumerate() {
            let w = self.weights.get(i).copied().unwrap_or([(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]);
            let mut acc = Splat::new(Vec3::ZERO, Mat3::default(), [0.0; 4]);
            let mut total = 0.0;
            for (bone, weight) in w {
                if weight <= 0.0 {
                    continue;
                }
                let Some(name) = self.bones.get(bone as usize) else {
                    continue;
                };
                let xf = posed.bone(name);
                let t = s.transformed(&xf);
                acc.pos = acc.pos + t.pos * weight;
                acc.basis.m.iter_mut().zip(t.basis.m.iter()).for_each(|(a, b)| {
                    *a += b * weight;
                });
                for k in 0..4 {
                    acc.color[k] += s.color[k] * weight;
                }
                total += weight;
            }
            if total <= 0.0 {
                acc = *s;
            }
            acc.vertex = s.vertex;
            out.push(acc);
        }
        out
    }
}

impl From<PuppetError> for io::Error {
    fn from(e: PuppetError) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splat_params_quantisation_matches_the_render_key() {
        // The three recovered scales and their clamps.
        let p = SplatParams::new(0.05, 1.0, 1.0);
        assert_eq!(p.quantised(), (50, 100, 100));
        assert_eq!(p.key_bits(), 50 | 100 << 16 | 100 << 32);
        // Lower clamps: 5 / 20 / 5.
        let lo = SplatParams::new(0.0, 0.0, 0.0);
        assert_eq!(lo.quantised(), (5, 20, 5));
        // Upper clamps: 500 / 200 / 100.
        let hi = SplatParams::new(10.0, 10.0, 10.0);
        assert_eq!(hi.quantised(), (500, 200, 100));
        // Rounding is half-away-from-zero (`frinta`).
        let r = SplatParams::new(0.0505, 1.005, 0.505);
        assert_eq!(r.quantised(), (51, 101, 51));
    }

    #[test]
    fn splat_params_default_is_inside_every_range() {
        let p = SplatParams::default();
        let (a, b, c) = p.quantised();
        assert!((5..=500).contains(&a));
        assert!((20..=200).contains(&b));
        assert!((5..=100).contains(&c));
    }

    #[test]
    fn spz_header_validation_uses_the_recovered_messages() {
        assert_eq!(
            SpzHeader::parse(&[0u8; 8]).unwrap_err().to_string(),
            "too short for a header"
        );
        let mut b = vec![0u8; 32];
        b[0..4].copy_from_slice(b"NGSP");
        b[4] = 4;
        let h = SpzHeader::parse(&b).unwrap();
        assert_eq!(h.version, 4);
        assert_eq!(h.num_points, 0);

        // gzip magic first.
        let mut g = vec![0u8; 32];
        g[0] = 0x1f;
        g[1] = 0x8b;
        assert_eq!(
            SpzHeader::parse(&g).unwrap_err().to_string(),
            "a gzip spz (version 1..3) is not read"
        );
        // Bad magic.
        let bad_magic = vec![0u8; 32];
        assert_eq!(
            SpzHeader::parse(&bad_magic).unwrap_err().to_string(),
            "not an spz file (no NGSP magic)"
        );
        // Wrong version.
        let mut v3 = vec![0u8; 32];
        v3[0..4].copy_from_slice(b"NGSP");
        v3[4] = 3;
        assert_eq!(
            SpzHeader::parse(&v3).unwrap_err().to_string(),
            "version 3 is not read, only 4"
        );
        // Degree 5 is rejected.
        let mut d5 = vec![0u8; 32];
        d5[0..4].copy_from_slice(b"NGSP");
        d5[4] = 4;
        d5[0x0c] = 5;
        assert_eq!(
            SpzHeader::parse(&d5).unwrap_err().to_string(),
            "harmonics degree 5 is not read, only 4"
        );
    }

    #[test]
    fn spz_position_scale_is_two_to_the_minus_fractional_bits() {
        let mut b = vec![0u8; 32];
        b[0..4].copy_from_slice(b"NGSP");
        b[4] = 4;
        for (fb, expect) in [(0u8, 1.0f32), (1, 0.5), (8, 1.0 / 256.0), (12, 1.0 / 4096.0)] {
            b[0x0d] = fb;
            let h = SpzHeader::parse(&b).unwrap();
            assert!((h.position_scale() - expect).abs() < 1e-12, "fb={fb}");
        }
        // Clamped at 31 bits, never negative/NaN.
        b[0x0d] = 40;
        assert!(SpzHeader::parse(&b).unwrap().position_scale() > 0.0);
    }

    #[test]
    fn spz_stream_sizes_match_the_recovered_factors() {
        let mut b = vec![0u8; 32];
        b[0..4].copy_from_slice(b"NGSP");
        b[4] = 4;
        b[0x0c] = 1; // SH degree 1 -> 3 coefficients
        let h = SpzHeader::parse(&b).unwrap();
        let n = 100u32;
        assert_eq!(h.stream_len(0, n), 900); // 9 bytes of position per point
        assert_eq!(h.stream_len(1, n), 300); // 3 colour bytes
        assert_eq!(h.stream_len(2, n), 400); // rotation + opacity
        assert_eq!(h.stream_len(3, n), 3 * 3 * 100); // sh * 3 * n
        // Degree 0 has no SH stream at all.
        b[0x0c] = 0;
        let h0 = SpzHeader::parse(&b).unwrap();
        assert_eq!(h0.stream_len(3, n), 0);
        // Degrees 2 and 3 use 8 and 15 coefficients.
        assert_eq!(SPZ_SH_COEFFS[2], 8);
        assert_eq!(SPZ_SH_COEFFS[3], 15);
    }

    /// Build a minimal v4 `.spz` with raw (uncompressed) streams.
    fn build_spz(fb: u8, points: &[[i32; 3]]) -> Vec<u8> {
        let n = points.len();
        let mut pos = Vec::new();
        for p in points {
            for v in p {
                let b = v.to_le_bytes();
                pos.extend_from_slice(&b[0..3]);
            }
        }
        let col = vec![255u8; n * 3];
        let mut roto = Vec::new();
        for _ in 0..n {
            roto.extend_from_slice(&[0, 0, 0, 255]);
        }
        let streams = [pos, col, roto, Vec::new()];
        let mut out = vec![0u8; 0x20];
        out[0..4].copy_from_slice(b"NGSP");
        out[4..8].copy_from_slice(&4u32.to_le_bytes());
        out[8..12].copy_from_slice(&(n as u32).to_le_bytes());
        out[0x0c] = 0; // sh degree
        out[0x0d] = fb;
        out[0x0f] = 4; // four streams
        out[0x10..0x14].copy_from_slice(&0x20u32.to_le_bytes());
        // table of contents
        let mut body = Vec::new();
        for s in &streams {
            out.extend_from_slice(&(s.len() as u64).to_le_bytes());
            out.extend_from_slice(&(s.len() as u64).to_le_bytes());
            body.extend_from_slice(s);
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn spz_round_trip_decodes_fixed_point_positions() {
        // fractional_bits = 4 => scale 1/16.
        let bytes = build_spz(4, &[[16, 32, -16], [0, 0, 0]]);
        let cloud = decode_spz(&bytes).unwrap();
        assert_eq!(cloud.len(), 2);
        // y and z are negated on the way in (recovered `fnmul`).
        assert_eq!(cloud.splats[0].pos, Vec3::new(1.0, -2.0, 1.0));
        assert_eq!(cloud.splats[1].pos, Vec3::ZERO);
        assert_eq!(cloud.bounds.0, Vec3::new(0.0, -2.0, 0.0));
        assert_eq!(cloud.bounds.1, Vec3::new(1.0, 0.0, 1.0));
        // Colours come through as 1.0 and the opacity byte as 1.0.
        assert_eq!(cloud.splats[0].color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn spz_rejects_a_wrong_stream_count_and_truncated_body() {
        let mut bytes = build_spz(0, &[[1, 2, 3]]);
        bytes[0x0f] = 3;
        let err = decode_spz(&bytes).unwrap_err().to_string();
        assert!(err.contains("3 streams, expected 4"), "{err}");

        let mut bytes = build_spz(0, &[[1, 2, 3]]);
        bytes.truncate(0x40 + 8);
        assert!(decode_spz(&bytes).is_err());
    }

    #[test]
    fn zstd_raw_blocks_round_trip_and_compressed_blocks_are_refused() {
        // Frame header: magic, FHD = single-segment + 1-byte content size.
        let payload = b"hello splats";
        let mut frame = Vec::new();
        frame.extend_from_slice(&0xFD2F_B528u32.to_le_bytes());
        frame.push(0b0010_0000); // single segment, fcs_flag = 0 -> 1 byte size
        frame.push(payload.len() as u8);
        let size = payload.len() as u32;
        let header = size << 3 | 0 << 1 | 1; // raw block, last
        frame.extend_from_slice(&header.to_le_bytes()[0..3]);
        frame.extend_from_slice(payload);
        assert_eq!(zstd_decode(&frame, payload.len()).unwrap(), payload);

        // A compressed block is refused instead of guessed at.
        let mut frame = Vec::new();
        frame.extend_from_slice(&0xFD2F_B528u32.to_le_bytes());
        frame.push(0b0010_0000);
        frame.push(4);
        let header = 4u32 << 3 | 2 << 1 | 1;
        frame.extend_from_slice(&header.to_le_bytes()[0..3]);
        frame.extend_from_slice(&[0, 0, 0, 0]);
        assert!(zstd_decode(&frame, 4).is_err());
    }

    #[test]
    fn ply_header_parsing_and_rejections() {
        let ascii = b"ply\nformat ascii 1.0\nelement vertex 2\nproperty float x\nproperty float y\nproperty float z\nproperty float opacity\nend_header\n0 0 0 10\n1 2 3 0\n";
        let h = PlyHeader::parse(ascii).unwrap();
        assert!(h.ascii);
        assert_eq!(h.count, 2);
        assert_eq!(h.properties, vec!["x", "y", "z", "opacity"]);
        assert_eq!(h.data_offset, ascii.len() - "0 0 0 10\n1 2 3 0\n".len());

        // Missing end_header.
        let e = PlyHeader::parse(b"ply\nformat ascii 1.0\n").unwrap_err();
        assert_eq!(e.to_string(), "no end_header");
        // Non-float property.
        let e = PlyHeader::parse(
            b"ply\nformat ascii 1.0\nelement vertex 1\nproperty uchar red\nend_header\n",
        )
        .unwrap_err();
        assert_eq!(e.to_string(), "only float properties are read");
    }

    #[test]
    fn ply_ascii_decoding_applies_the_gs_conventions() {
        let text = "ply\nformat ascii 1.0\nelement vertex 2\n\
             property float x\nproperty float y\nproperty float z\n\
             property float f_dc_0\nproperty float f_dc_1\nproperty float f_dc_2\n\
             property float opacity\n\
             property float scale_0\nproperty float scale_1\nproperty float scale_2\n\
             property float rot_0\nproperty float rot_1\nproperty float rot_2\nproperty float rot_3\n\
             end_header\n\
             0 0 0 0 0 0 0 0 0 0 1 0 0 0\n\
             1 2 3 0 0 0 100 0 0 0 1 0 0 0\n";
        let cloud = load_ply(text.as_bytes()).unwrap();
        assert_eq!(cloud.len(), 2);
        assert_eq!(cloud.splats[0].pos, Vec3::ZERO);
        // opacity 0 (logit) -> alpha 0.5.
        assert!((cloud.splats[0].color[3] - 0.5).abs() < 1e-6);
        // f_dc 0 -> 0.5 grey.
        assert!((cloud.splats[0].color[0] - 0.5).abs() < 1e-6);
        // opacity 100 -> alpha ~1.
        assert!(cloud.splats[1].color[3] > 0.99);
        // Identity rotation, unit scale -> identity basis.
        assert!((cloud.splats[0].basis.m[0] - 1.0).abs() < 1e-6);
        assert!(cloud.splats[0].basis.m[1].abs() < 1e-6);
    }

    #[test]
    fn ply_binary_little_endian_decoding() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            b"ply\nformat binary_little_endian 1.0\nelement vertex 1\n\
              property float x\nproperty float y\nproperty float z\n\
              property float opacity\nend_header\n",
        );
        for v in [4.0f32, 5.0, 6.0, 0.0] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let cloud = load_ply(&bytes).unwrap();
        assert_eq!(cloud.len(), 1);
        assert_eq!(cloud.splats[0].pos, Vec3::new(4.0, 5.0, 6.0));
        assert!((cloud.splats[0].color[3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn cloud_file_prefers_spz_then_falls_back_to_ply() {
        let dir = std::env::temp_dir().join(format!("gs_puppet_cloud_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // With no `.spz` present the `.ply` path is returned, whether or not the
        // `.ply` exists (the recovered fallback is not existence-checked).
        assert_eq!(cloud_file(&dir, "body"), dir.join("body.ply"));
        std::fs::write(dir.join("body.ply"), b"x").unwrap();
        assert_eq!(cloud_file(&dir, "body"), dir.join("body.ply"));
        std::fs::write(dir.join("body.spz"), b"x").unwrap();
        assert_eq!(cloud_file(&dir, "body"), dir.join("body.spz"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_cloud_picks_the_decoder_by_extension() {
        let dir = std::env::temp_dir().join(format!("gs_puppet_lc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ply = dir.join("a.ply");
        std::fs::write(
            &ply,
            b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n1 2 3\n",
        )
        .unwrap();
        let cloud = load_cloud(&ply).unwrap();
        assert_eq!(cloud.splats[0].pos, Vec3::new(1.0, 2.0, 3.0));
        // A .spz that is not an spz reports the recovered message.
        let spz = dir.join("b.spz");
        std::fs::write(&spz, vec![0u8; 40]).unwrap();
        assert_eq!(
            load_cloud(&spz).unwrap_err().to_string(),
            "not an spz file (no NGSP magic)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn far_to_near_sorts_by_decreasing_distance() {
        let mut cloud = SplatCloud::new(vec![
            Splat::new(Vec3::new(1.0, 0.0, 0.0), Mat3::IDENTITY, [1.0; 4]),
            Splat::new(Vec3::new(9.0, 0.0, 0.0), Mat3::IDENTITY, [1.0; 4]),
            Splat::new(Vec3::new(5.0, 0.0, 0.0), Mat3::IDENTITY, [1.0; 4]),
        ]);
        cloud.far_to_near(Vec3::ZERO);
        let xs: Vec<f32> = cloud.splats.iter().map(|s| s.pos.x).collect();
        assert_eq!(xs, vec![9.0, 5.0, 1.0]);
    }

    #[test]
    fn bound_cloud_posed_follows_the_bone() {
        use crate::model::{Puppet, Segment};
        use crate::pose::{Pose, PoseSet};
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("arm_l", 0, Vec3::new(0.5, 0.0, 0.0), 0.2),
            ],
        )
        .unwrap();
        let cloud = SplatCloud::new(vec![Splat::new(
            Vec3::new(0.0, 0.1, 0.0),
            Mat3::IDENTITY,
            [1.0; 4],
        )]);
        let mut bound = BoundCloud::new(cloud, vec!["root".into(), "arm_l".into()]);
        // Weight the splat fully onto the arm.
        bound.weights[0] = [(1, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)];
        let posed = puppet.pose();
        let out = bound.posed(&posed);
        assert_eq!(out.len(), 1);
        assert!((out[0].pos.x - 0.5).abs() < 1e-6, "{:?}", out[0].pos);
        let _ = PoseSet::for_family("biped", "").unwrap();
        let _ = Pose::new();
    }

    #[test]
    fn weights_normalise() {
        let mut w: SkinWeights = [(0, 2.0), (1, 2.0), (0, 0.0), (0, 0.0)];
        normalise_weights(&mut w);
        assert!((w[0].1 - 0.5).abs() < 1e-6);
        assert!((w[1].1 - 0.5).abs() < 1e-6);
        let mut zero: SkinWeights = [(0, 0.0); 4];
        normalise_weights(&mut zero);
        assert_eq!(zero, [(0, 0.0); 4]);
    }

    #[test]
    fn prepare_projects_and_sorts_near_to_far_order() {
        use crate::camera::Camera;
        let cam = Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 500.0);
        let cloud = SplatCloud::new(vec![
            Splat::new(Vec3::new(0.0, 0.0, 10.0), Mat3::IDENTITY, [1.0; 4]),
            Splat::new(Vec3::new(0.0, 0.0, 2.0), Mat3::IDENTITY, [1.0; 4]),
        ]);
        let out = prepare(&cloud.splats, &cam, 16);
        assert_eq!(out.len(), 2);
        // Far to near: the 10.0-away splat is drawn first.
        assert!(out[0].depth > out[1].depth, "{out:?}");
        // A splat at the centre projects to the screen centre.
        assert!((out[1].screen[0] - 320.0).abs() < 1e-3, "{out:?}");
    }

    // -- the recovered draw-pass geometry, and the `epaint` values -----------

    /// A camera at the origin looking down +z: `focal / depth` is `400 / 8 = 50`
    /// pixels per unit at the origin.
    fn cam() -> crate::camera::Camera {
        crate::camera::Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 400.0)
    }

    /// An isotropic splat of `sigma` model units at `pos`.
    fn ball(pos: Vec3, sigma: f32, color: [f32; 4]) -> Splat {
        Splat::new(
            pos,
            Mat3::from_cols_array([sigma, 0.0, 0.0, 0.0, sigma, 0.0, 0.0, 0.0, sigma]),
            color,
        )
    }

    #[test]
    fn the_recovered_shader_constants_are_the_recovered_values() {
        assert_eq!(CORE_M, 2.25);
        assert_eq!(REACH_M, 9.0);
        // The quad reaches exactly sqrt(REACH_M) standard deviations, which is
        // why the vertex shader can write `3.0 * sqrt(xx)`.
        assert_eq!(QUAD_SIGMAS, REACH_M.sqrt());
        assert_eq!(RIM_FROM, 0.3);
        assert_eq!(RIM_TO, 0.8);
        assert_eq!(MIN_VARIANCE, 0.3);
        assert_eq!(MIN_DET, 1e-6);
        assert_eq!(MAX_ALPHA, 0.99);
        // CORE_M is the -2.25 M contour: exp(-0.5 * CORE_M) = 0.3247.
        assert!((falloff(CORE_M) - 0.324_652_5).abs() < 1e-6, "{}", falloff(CORE_M));
        // REACH_M is the -9 M contour, invisible but not zero.
        assert!((falloff(REACH_M) - 0.011_109).abs() < 1e-6, "{}", falloff(REACH_M));
    }

    #[test]
    fn the_gpu_instance_is_nine_vec4s() {
        // The shader's comment: "one splat at rest (144 bytes)".
        assert_eq!(SPLAT_INSTANCE_STRIDE, 144);
        assert_eq!(SPLAT_INSTANCE_STRIDE, 9 * 16);
        // It is a repacking of the 92-byte decoded record, not the same record.
        assert_eq!(SPLAT_STRIDE, 0x5c);
        assert_ne!(SPLAT_STRIDE, SPLAT_INSTANCE_STRIDE);
    }

    #[test]
    fn gauss_m_is_the_shaders_quadratic_form() {
        // An isotropic inverse covariance of 2: gauss_m(d) = 2 * |d|^2.
        let inv = [2.0, 0.0, 0.0, 2.0];
        assert_eq!(gauss_m([0.0, 0.0], inv), 0.0);
        assert!((gauss_m([1.0, 0.0], inv) - 2.0).abs() < 1e-6);
        assert!((gauss_m([0.0, 2.0], inv) - 8.0).abs() < 1e-6);
        assert!((gauss_m([3.0, 4.0], inv) - 50.0).abs() < 1e-5);
        // The off-diagonal terms are the cross product: inv = [[1, 1], [1, 1]]
        // makes the form (d.x + d.y)^2.
        let diag = [1.0, 1.0, 1.0, 1.0];
        assert!((gauss_m([1.0, 2.0], diag) - 9.0).abs() < 1e-6);
        assert!((gauss_m([1.0, -1.0], diag)).abs() < 1e-6);
    }

    #[test]
    fn a_projected_splat_gets_the_shaders_billboard() {
        // sigma 0.02 model units at depth 8 with focal 400 is 1.0 screen pixel.
        let s = ball(Vec3::ZERO, 0.02, [1.0, 0.5, 0.0, 1.0]);
        let f = Footprint::of(&s, &cam()).unwrap();
        assert_eq!(f.centre, [320.0, 240.0]);
        assert_eq!(f.depth, 8.0);
        assert_eq!(f.alpha, 1.0);
        // xx = yy = 1 + 0.3, so the half-extent is 3 * sqrt(1.3).
        let h = 3.0 * 1.3f32.sqrt();
        assert!((f.half[0] - h).abs() < 1e-5, "{:?}", f.half);
        assert!((f.half[1] - h).abs() < 1e-5);
        // The quad really does reach the REACH_M contour along both axes:
        // gauss_m at the half-extent is exactly the variance-reduced 9.0.
        assert!((f.gauss_at([f.half[0], 0.0]) - REACH_M).abs() < 1e-4);
        assert!((f.gauss_at([0.0, f.half[1]]) - REACH_M).abs() < 1e-4);
        // spread = sigma^2 / (sigma^2 + 0.3) = 1 / 1.3, and the inverse
        // covariance is the diagonal 1 / 1.3.
        assert!((f.spread - 1.0 / 1.3).abs() < 1e-6, "{}", f.spread);
        assert!((f.inv[0] - 1.0 / 1.3).abs() < 1e-6, "{:?}", f.inv);
        assert!((f.inv[3] - 1.0 / 1.3).abs() < 1e-6);
        // An isotropic footprint has no cross term.
        assert_eq!((f.inv[1], f.inv[2]), (0.0, 0.0));
        // The peak alpha is the splat's alpha attenuated by the spread.
        assert!((f.peak_alpha() - 1.0 / 1.3).abs() < 1e-6);
        // The quad is a square about the centre, in the shaders' corner order.
        let c = f.corners();
        assert!((c[0][0] - (320.0 - h)).abs() < 1e-4);
        assert!((c[0][1] - (240.0 - h)).abs() < 1e-4);
        assert!((c[3][0] - (320.0 + h)).abs() < 1e-4);
        assert!((c[3][1] - (240.0 + h)).abs() < 1e-4);
        assert_eq!(f.local(c[3].map(|v| 0.0))[0], 0.0);
        // local() of the (1, 1) corner is the half-extent itself.
        let l = f.local([1.0, 1.0]);
        assert!((l[0] - h).abs() < 1e-4 && (l[1] - h).abs() < 1e-4);
        assert!((f.area() - 4.0 * h * h).abs() < 1e-3);
        assert!(f.is_finite());
    }

    #[test]
    fn a_sub_pixel_splat_is_spread_over_a_pixel_and_fades() {
        // Half a pixel of sigma: xx = 0.25 + 0.3.
        let f = Footprint::of(&ball(Vec3::ZERO, 0.01, [1.0; 4]), &cam()).unwrap();
        assert!((f.half[0] - 3.0 * 0.55f32.sqrt()).abs() < 1e-5, "{:?}", f.half);
        // spread = 0.25 / 0.55 -- as much fainter as it was made wider.
        assert!((f.spread - 0.25 / 0.55).abs() < 1e-6, "{}", f.spread);
        // A splat with no size at all keeps a pixel of reach and no opacity.
        let none = Footprint::of(&ball(Vec3::ZERO, 0.0, [1.0; 4]), &cam()).unwrap();
        assert!((none.half[0] - 3.0 * MIN_VARIANCE.sqrt()).abs() < 1e-6);
        assert_eq!(none.spread, 0.0);
        assert_eq!(none.peak_alpha(), 0.0);
        // A zero-alpha splat stays invisible however large it is.
        let clear = Footprint::of(&ball(Vec3::ZERO, 0.02, [1.0, 1.0, 1.0, 0.0]), &cam()).unwrap();
        assert_eq!(clear.peak_alpha(), 0.0);
    }

    #[test]
    fn the_peak_alpha_is_capped_like_the_blend_pass() {
        let f = Footprint::of(&ball(Vec3::ZERO, 0.02, [1.0; 4]), &cam()).unwrap();
        // alpha 2.0 * spread 0.769 -> capped at MAX_ALPHA.
        let hot = Footprint { alpha: 2.0, ..f };
        assert_eq!(hot.peak_alpha(), MAX_ALPHA);
        // A negative alpha never becomes an opaque pixel.
        let cold = Footprint { alpha: -1.0, ..f };
        assert_eq!(cold.peak_alpha(), 0.0);
    }

    #[test]
    fn a_stretched_splat_is_an_ellipse_on_the_screen() {
        // A splat twice as wide as it is tall, still 1 pixel of sigma per axis:
        // the covariance sums the three projected axes, so xx = 4 + 0.3 and
        // yy = 1 + 0.3.
        let sigma = 0.02;
        let basis = Mat3::from_cols_array([
            2.0 * sigma, 0.0, 0.0, 0.0, sigma, 0.0, 0.0, 0.0, sigma,
        ]);
        let s = Splat::new(Vec3::ZERO, basis, [1.0; 4]);
        let f = Footprint::of(&s, &cam()).unwrap();
        assert!((f.half[0] - 3.0 * 4.3f32.sqrt()).abs() < 1e-5, "{:?}", f.half);
        assert!((f.half[1] - 3.0 * 1.3f32.sqrt()).abs() < 1e-5, "{:?}", f.half);
        // det = 4.3 * 1.3; the sub-pixel spread factor is sqrt(4 / 5.59).
        assert!((f.spread - (4.0f32 / 5.59).sqrt()).abs() < 1e-6, "{}", f.spread);
        // The third axis points at the camera and contributes no screen area.
        assert!((f.gauss_at([f.half[0], 0.0]) - REACH_M).abs() < 1e-4);
        assert!((f.gauss_at([0.0, f.half[1]]) - REACH_M).abs() < 1e-4);
        // The quad is that ellipse's bounding box, four times its half-extents.
        assert!((f.area() - 4.0 * f.half[0] * f.half[1]).abs() < 1e-4);
    }

    #[test]
    fn rotating_a_splat_turns_the_quad_into_a_sheared_footprint() {
        let sigma = 0.02;
        let stretched = Mat3::from_cols_array([
            2.0 * sigma, 0.0, 0.0, 0.0, sigma, 0.0, 0.0, 0.0, sigma,
        ]);
        let rot = Mat3::from_rotation_z(45.0 * crate::math::DEG_TO_RAD);
        let s = Splat::new(Vec3::ZERO, rot.mul_mat3(stretched), [1.0; 4]);
        let f = Footprint::of(&s, &cam()).unwrap();
        // A 45 degree turn in the image plane: xx0 = yy0 = 2.5, xy = -1.5.
        let e = 3.0 * 2.8f32.sqrt();
        assert!((f.half[0] - e).abs() < 1e-4, "{:?}", f.half);
        assert!((f.half[1] - e).abs() < 1e-4, "{:?}", f.half);
        // The inverse covariance is symmetric, and det(inv) = 1 / det.
        assert!((f.inv[1] - f.inv[2]).abs() < 1e-7);
        assert!(f.inv[1] > 0.0, "xy = -1.5 flips the sign: {:?}", f.inv);
        let det = 2.8f32 * 2.8 - 1.5 * 1.5;
        let det_inv = f.inv[0] * f.inv[3] - f.inv[1] * f.inv[2];
        assert!((det_inv * det - 1.0).abs() < 1e-4, "{det_inv} {det}");
        assert!((f.inv[0] - 2.8 / det).abs() < 1e-4, "{:?}", f.inv);
        // Turning a splat does not fade it: the spread is a determinant, so it is
        // the same as the unrotated ellipse's.
        assert!((f.spread - (4.0f32 / 5.59).sqrt()).abs() < 1e-4, "{}", f.spread);
        // The quad still reaches the reach contour along each axis of the ellipse
        // (the corners of the bounding box reach further: (3, 3) sigma).
        assert!(f.gauss_at([e, e]) > REACH_M);
    }

    #[test]
    fn a_non_finite_splat_has_no_footprint() {
        let nan = ball(Vec3::new(f32::NAN, 0.0, 0.0), 0.02, [1.0; 4]);
        assert!(Footprint::of(&nan, &cam()).is_none());
        let inf = ball(Vec3::new(0.0, f32::INFINITY, 0.0), 0.02, [1.0; 4]);
        assert!(Footprint::of(&inf, &cam()).is_none());
    }

    #[test]
    fn prepared_splat_footprints_are_the_isotropic_special_case() {
        let p = PreparedSplat {
            screen: [320.0, 240.0],
            radius: 1.0,
            depth: 8.0,
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let f = p.footprint();
        // Exactly the same numbers as the full three-axis projection of an
        // isotropic splat of the same screen radius.
        let full = Footprint::of(&ball(Vec3::ZERO, 0.02, [1.0; 4]), &cam()).unwrap();
        assert!((f.half[0] - full.half[0]).abs() < 1e-5);
        assert!((f.half[1] - full.half[1]).abs() < 1e-5);
        assert!((f.spread - full.spread).abs() < 1e-6);
        assert!((f.inv[0] - full.inv[0]).abs() < 1e-6);
        assert_eq!((f.inv[1], f.inv[2]), (0.0, 0.0));
        assert_eq!(f.depth, 8.0);
    }

    #[test]
    fn the_mesh_is_one_billboard_quad_per_splat() {
        use crate::camera::Camera;
        let cam = Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 400.0);
        let splats = vec![
            ball(Vec3::new(0.0, 0.0, 0.0), 0.02, [1.0, 0.5, 0.0, 1.0]),
            ball(Vec3::new(0.5, 0.0, 0.0), 0.02, [0.0, 1.0, 0.0, 1.0]),
        ];
        let prepared = prepare(&splats, &cam, 16);
        assert_eq!(prepared.len(), 2);
        let tex = epaint::TextureId::User(7);
        let m = mesh(&prepared, tex);
        assert_eq!(m.texture_id, tex);
        assert_eq!(m.vertices.len(), 8, "four vertices per splat");
        assert_eq!(m.indices.len(), 12, "six indices per splat");
        assert!(m.is_valid(), "every index is inside the vertex list");
        assert_eq!(m.indices, vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7]);
        for v in &m.vertices {
            assert!(v.pos.x.is_finite() && v.pos.y.is_finite());
        }
        // The first splat: a square of half-extent 3 * sqrt(1.3) about (320, 240).
        let h = 3.0 * 1.3f32.sqrt();
        let want = epaint::pos2(320.0 - h, 240.0 - h);
        assert!((m.vertices[0].pos - want).length() < 1e-4, "{:?}", m.vertices[0].pos);
        assert!((m.vertices[3].pos.x - (320.0 + h)).abs() < 1e-4);
        assert!((m.vertices[3].pos.y - (240.0 + h)).abs() < 1e-4);
        // The UVs span a radial gaussian texture, top-left first.
        assert_eq!(m.vertices[0].uv, epaint::pos2(0.0, 0.0));
        assert_eq!(m.vertices[1].uv, epaint::pos2(1.0, 0.0));
        assert_eq!(m.vertices[2].uv, epaint::pos2(0.0, 1.0));
        assert_eq!(m.vertices[3].uv, epaint::pos2(1.0, 1.0));
        // The colour is the splat's, carrying the recovered peak alpha
        // (1.0 * spread 1/1.3 = 0.769 -> 196/255).
        let a = (255.0f32 / 1.3).round() as u8;
        assert_eq!(a, 196);
        assert_eq!(
            m.vertices[0].color,
            epaint::Color32::from_rgba_unmultiplied(255, 128, 0, 196)
        );
        // The second splat is the same quad shifted by 0.5 * 50 px, green.
        assert!((m.vertices[4].pos.x - (345.0 - h)).abs() < 1e-4, "{:?}", m.vertices[4].pos);
        assert_eq!(
            m.vertices[4].color,
            epaint::Color32::from_rgba_unmultiplied(0, 255, 0, 196)
        );
        // The mesh bbox is the union of the quads.
        let b = m.calc_bounds();
        assert!((b.min.x - (320.0 - h)).abs() < 1e-4);
        assert!((b.max.x - (345.0 + h)).abs() < 1e-4);
    }

    #[test]
    fn the_mesh_keeps_the_prepared_far_to_near_order() {
        use crate::camera::Camera;
        let cam = Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 400.0);
        // Three splats up the view axis: the far one is prepared (and drawn) first.
        let splats = vec![
            ball(Vec3::new(0.0, 0.0, 4.0), 0.02, [1.0; 4]),
            ball(Vec3::new(0.0, 0.0, 0.0), 0.02, [1.0; 4]),
            ball(Vec3::new(0.0, 0.0, 2.0), 0.02, [1.0; 4]),
        ];
        let prepared = prepare(&splats, &cam, 16);
        let depths: Vec<f32> = prepared.iter().map(|p| p.depth).collect();
        assert_eq!(depths, vec![8.0, 6.0, 4.0]);
        let m = solid_mesh(&prepared);
        assert_eq!(m.vertices.len(), 12);
        // Each quad's centre is prepared[i].screen, and the radii grow as the
        // depth shrinks (the same sigma, projected closer).
        let mut widths = Vec::new();
        for (i, p) in prepared.iter().enumerate() {
            let quad = &m.vertices[i * 4..i * 4 + 4];
            let cx = (quad[0].pos.x + quad[3].pos.x) * 0.5;
            let cy = (quad[0].pos.y + quad[3].pos.y) * 0.5;
            assert!((cx - p.screen[0]).abs() < 1e-4, "quad {i} at {cx}");
            assert!((cy - p.screen[1]).abs() < 1e-4);
            widths.push(quad[3].pos.x - quad[0].pos.x);
        }
        assert!(widths[0] < widths[1] && widths[1] < widths[2], "{widths:?}");
    }

    #[test]
    fn the_solid_mesh_uses_the_untextured_white_uv() {
        let prepared = vec![PreparedSplat {
            screen: [10.0, 20.0],
            radius: 1.0,
            depth: 4.0,
            color: [0.2, 0.4, 0.6, 1.0],
        }];
        let m = solid_mesh(&prepared);
        assert_eq!(m.texture_id, epaint::TextureId::default());
        assert_eq!(m.vertices.len(), 4);
        for v in &m.vertices {
            assert_eq!(v.uv, epaint::WHITE_UV);
        }
        // (r, g, b) at 0.2 / 0.4 / 0.6 -> 51 / 102 / 153.
        assert_eq!(
            m.vertices[0].color,
            epaint::Color32::from_rgba_unmultiplied(51, 102, 153, (255.0f32 / 1.3).round() as u8)
        );
        // The textured mesh differs only in the texture id and the UVs.
        let t = mesh(&prepared, epaint::TextureId::User(1));
        assert_eq!(t.vertices[0].color, m.vertices[0].color);
        assert_eq!(t.vertices[1].uv, epaint::pos2(1.0, 0.0));
    }

    #[test]
    fn a_single_splat_can_build_its_own_mesh() {
        let p = PreparedSplat {
            screen: [100.0, 50.0],
            radius: 2.0,
            depth: 3.0,
            color: [1.0; 4],
        };
        let m = p.mesh(epaint::TextureId::Managed(3));
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.indices, vec![0, 1, 2, 2, 1, 3]);
        assert!(m.is_valid());
        // Half-extent 3 * sqrt(4 + 0.3).
        let h = 3.0 * 4.3f32.sqrt();
        assert!((m.vertices[3].pos.x - (100.0 + h)).abs() < 1e-4, "{h}");
        assert!((m.vertices[0].pos.y - (50.0 - h)).abs() < 1e-4);
    }

    #[test]
    fn prepared_mesh_projects_and_builds_in_one_step() {
        use crate::camera::Camera;
        let cam = Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 400.0);
        let splats = vec![
            ball(Vec3::ZERO, 0.02, [1.0; 4]),
            ball(Vec3::new(0.0, 0.0, 3.0), 0.02, [1.0; 4]),
        ];
        let m = prepared_mesh(&splats, &cam, 8, epaint::TextureId::User(2));
        assert_eq!(m.vertices.len(), 8);
        assert!(m.is_valid());
        // The nearer splat (depth 5) is drawn last and is the wider quad.
        let near = &m.vertices[4..8];
        assert!((near[0].pos.x + near[3].pos.x) * 0.5 - 320.0 < 1e-4);
        assert!(near[3].pos.x - near[0].pos.x > m.vertices[3].pos.x - m.vertices[0].pos.x);
        // The budget still applies: only the nearest one survives a budget of 1.
        let one = prepared_mesh(&splats, &cam, 1, epaint::TextureId::User(2));
        assert_eq!(one.vertices.len(), 4);
    }

    #[test]
    fn a_real_egui_frame_paints_the_splat_mesh() {
        use crate::camera::Camera;
        let cam = Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 400.0);
        let splats = vec![
            ball(Vec3::ZERO, 0.02, [1.0, 0.5, 0.0, 1.0]),
            ball(Vec3::new(0.5, 0.0, 0.0), 0.02, [0.0, 1.0, 0.0, 1.0]),
        ];
        let m = prepared_mesh(&splats, &cam, 16, epaint::TextureId::User(4));

        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(640.0, 480.0),
        ));
        let out = ctx.run_ui(input, |ui| {
            ui.painter().add(epaint::Shape::mesh(m.clone()));
        });
        let meshes: Vec<&epaint::Mesh> = out
            .shapes
            .iter()
            .filter_map(|cs| match &cs.shape {
                epaint::Shape::Mesh(mesh) => Some(&**mesh),
                _ => None,
            })
            .collect();
        assert_eq!(meshes.len(), 1, "the frame carries our one mesh");
        assert_eq!(meshes[0].vertices.len(), 8);
        assert_eq!(meshes[0].indices.len(), 12);
        assert_eq!(meshes[0].texture_id, epaint::TextureId::User(4));
        assert!(meshes[0].vertices[0].color.a() > 0);
        // The frame's mesh is our mesh, bounds and all.
        let want = m.calc_bounds();
        let got = meshes[0].calc_bounds();
        assert!((got.min - want.min).length() < 1e-4, "{got:?} {want:?}");
        assert!((got.max - want.max).length() < 1e-4);
        // And it is the first splat's quad that lands at the screen centre +-3 sigma.
        let h = 3.0 * 1.3f32.sqrt();
        assert!((got.min.x - (320.0 - h)).abs() < 1e-4);
        assert!((got.max.x - (345.0 + h)).abs() < 1e-4);
    }
}
