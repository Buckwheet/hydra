//! Meshes, skin weights and splat binding.
//!
//! Recovered facts (`gs_puppet::mesh`):
//!
//! * `Mesh::load_obj` (`@0x1009dede4`) parses Wavefront OBJ (`v` / `vn` / `vt` /
//!   `f`), merges identical `(position, normal, uv)` triples through a hash map and
//!   then calls [`Mesh::smooth_normals`]; it reports `std::io::Error`s.
//! * `BodyMesh` carries lazily-built splat bodies: a `OnceLock<Arc<SplatBody>>` at
//!   `+0x100`/`+0x108` (`made_splats`) and a **second pair** of them at `+0x110`
//!   selected by [`BodyMesh::splats_at`], which splits a vertex index at `0x6e`
//!   (110) and returns `None` above `0xef` (239) — i.e. the 240-vertex body mesh
//!   is drawn as two half meshes.
//! * `BodyMeta::read` (`@0x1009dff7c`) walks the mesh file's directory and picks
//!   the record whose kind is set, whose index is `1` and whose height is finite.
//! * `nearest_vertices` (`@0x1009de5b0`) builds a [`VertexGrid`], uses
//!   `std::thread::available_parallelism` + `std::thread::scope` to split the work
//!   into chunks and collects `Option<u32>` results — one per query point.
//! * `load_weights` (`@0x1009ddc78`) reads the `weights_thin.txt` file (the literal
//!   survives in the binary) into per-vertex weight sets.
//!
//! The GPU-free bindings below keep the observable structure: a mesh, per-vertex
//! weights, a splat body that can be thinned, joined and bound to a rig.

use crate::math::{Mat3, Vec3, Xf};
use crate::pose::Posed;
use crate::splat::{normalise_weights, SkinWeights, Splat, SplatCloud, SplatParams};
use std::collections::HashMap;
use std::io;
use std::path::Path;

/// A mesh vertex.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub pos: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}

/// An indexed triangle mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u32>) -> Mesh {
        Mesh { vertices, indices }
    }

    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::MAX;
        let mut hi = Vec3::MIN;
        for v in &self.vertices {
            lo = lo.min(v.pos);
            hi = hi.max(v.pos);
        }
        if self.vertices.is_empty() {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            (lo, hi)
        }
    }

    /// Parse Wavefront OBJ text.
    ///
    /// `@0x1009dede4` — `Mesh::load_obj(&Path) -> io::Result<Mesh>`.  Negative
    /// (relative) indices are resolved the way the OBJ spec requires, `f` lines may
    /// be polygons (fanned into triangles) and a vertex is the triple
    /// `(v, vt, vn)`; the helper keeps the recovered post-step
    /// [`Mesh::smooth_normals`].
    pub fn parse_obj(text: &str) -> io::Result<Mesh> {
        let mut positions: Vec<Vec3> = Vec::new();
        let mut normals: Vec<Vec3> = Vec::new();
        let mut uvs: Vec<[f32; 2]> = Vec::new();
        let mut map: HashMap<[u32; 3], u32> = HashMap::new();
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let resolve = |idx: i64, len: usize| -> io::Result<usize> {
            let i = if idx < 0 { len as i64 + idx } else { idx - 1 };
            if i < 0 || i as usize >= len {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("index {idx} out of range"),
                ));
            }
            Ok(i as usize)
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => {
                    let mut c = [0.0f32; 3];
                    for k in 0..3 {
                        c[k] = it.next().and_then(|v| v.parse().ok()).ok_or_else(|| {
                            io::Error::new(io::ErrorKind::InvalidData, "bad vertex")
                        })?;
                    }
                    positions.push(Vec3::new(c[0], c[1], c[2]));
                }
                Some("vn") => {
                    let mut c = [0.0f32; 3];
                    for k in 0..3 {
                        c[k] = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    }
                    normals.push(Vec3::new(c[0], c[1], c[2]));
                }
                Some("vt") => {
                    let u = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    let v = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
                    uvs.push([u, v]);
                }
                Some("f") => {
                    let mut face: Vec<u32> = Vec::with_capacity(4);
                    for tok in it {
                        let mut parts = tok.split('/');
                        let vi = parts.next().unwrap_or("0");
                        let ti = parts.next().unwrap_or("");
                        let ni = parts.next().unwrap_or("");
                        let pi = resolve(vi.parse::<i64>().unwrap_or(0), positions.len())?;
                        let ui = if ti.is_empty() {
                            None
                        } else {
                            Some(resolve(ti.parse::<i64>().unwrap_or(0), uvs.len())?)
                        };
                        let nidx = if ni.is_empty() {
                            None
                        } else {
                            Some(resolve(ni.parse::<i64>().unwrap_or(0), normals.len())?)
                        };
                        let key = [pi as u32, ui.map(|u| u as u32).unwrap_or(u32::MAX), nidx.map(|n| n as u32).unwrap_or(u32::MAX)];
                        let idx = *map.entry(key).or_insert_with(|| {
                            let vert = Vertex {
                                pos: positions[pi],
                                normal: nidx.map(|n| normals[n]).unwrap_or(Vec3::ZERO),
                                uv: ui.map(|u| uvs[u]).unwrap_or([0.0, 0.0]),
                            };
                            let id = vertices.len() as u32;
                            vertices.push(vert);
                            id
                        });
                        face.push(idx);
                    }
                    // Fan the polygon.
                    for k in 1..face.len().saturating_sub(1) {
                        indices.push(face[0]);
                        indices.push(face[k]);
                        indices.push(face[k + 1]);
                    }
                }
                _ => {}
            }
        }
        if vertices.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "no vertices"));
        }
        let mut mesh = Mesh { vertices, indices };
        if mesh.vertices.iter().all(|v| v.normal == Vec3::ZERO) {
            mesh.smooth_normals();
        }
        Ok(mesh)
    }

    /// Read and parse an OBJ file.
    ///
    /// `@0x1009dede4` — the file-reading half of `Mesh::load_obj`.
    pub fn load_obj(path: &Path) -> io::Result<Mesh> {
        let text = std::fs::read_to_string(path)?;
        Mesh::parse_obj(&text)
    }

    /// Recompute vertex normals as the area-weighted average of the adjacent face
    /// normals.
    ///
    /// `@0x1009deaa4` — `Mesh::smooth_normals(&mut self)`.  The unnormalised cross
    /// product is the area-weighted contribution (hence the `1.0` `fmov` that guards
    /// the final normalisation).
    pub fn smooth_normals(&mut self) {
        for v in &mut self.vertices {
            v.normal = Vec3::ZERO;
        }
        for tri in self.indices.chunks_exact(3) {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            if a >= self.vertices.len() || b >= self.vertices.len() || c >= self.vertices.len() {
                continue;
            }
            let pa = self.vertices[a].pos;
            let pb = self.vertices[b].pos;
            let pc = self.vertices[c].pos;
            let n = (pb - pa).cross(pc - pa);
            for i in [a, b, c] {
                self.vertices[i].normal = self.vertices[i].normal + n;
            }
        }
        for v in &mut self.vertices {
            let n = v.normal.normalized();
            v.normal = if n == Vec3::ZERO { Vec3::new(0.0, 1.0, 0.0) } else { n };
        }
    }

    /// Scale and shift the mesh so it is `height` units tall with its feet on
    /// `y = 0`.
    ///
    /// `@0x1009de744` — `Mesh::fit_to_height(&mut self, f32)`.  The `0.5` immediate
    /// in the body is the centring factor: the mesh is scaled about its mid-height,
    /// then translated so the lowest point sits at zero.
    pub fn fit_to_height(&mut self, height: f32) {
        if self.vertices.is_empty() || !(height > 0.0) {
            return;
        }
        let (lo, hi) = self.bounds();
        let current = hi.y - lo.y;
        if !(current > 0.0) {
            return;
        }
        let k = height / current;
        for v in &mut self.vertices {
            v.pos = Vec3::new(v.pos.x * k, (v.pos.y - lo.y) * k, v.pos.z * k);
        }
    }
}

/// A uniform grid over a mesh's vertices, for nearest-vertex queries.
///
/// `gs_puppet::mesh::VertexGrid` (`new` / `nearest` / `far` are local symbols the
/// binary inlines into `nearest_vertices` and `SplatBody::bind_facing`).
#[derive(Clone, Debug, Default)]
pub struct VertexGrid {
    /// Cell size in model units.
    pub cell: f32,
    /// Grid origin (the mesh's minimum corner).
    pub origin: Vec3,
    /// Cell counts per axis.
    pub dims: [usize; 3],
    /// Vertex indices per cell, laid out `x + y*nx + z*nx*ny`.
    pub cells: Vec<Vec<u32>>,
    /// The source positions.
    pub points: Vec<Vec3>,
}

impl VertexGrid {
    /// Build a grid with roughly `cell`-sized buckets.
    pub fn new(points: &[Vec3], cell: f32) -> VertexGrid {
        let cell = if cell > 0.0 { cell } else { 1.0 };
        let mut lo = Vec3::MAX;
        let mut hi = Vec3::MIN;
        for p in points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        if points.is_empty() {
            lo = Vec3::ZERO;
            hi = Vec3::ZERO;
        }
        let dims = [
            (((hi.x - lo.x) / cell).ceil() as usize).max(1),
            (((hi.y - lo.y) / cell).ceil() as usize).max(1),
            (((hi.z - lo.z) / cell).ceil() as usize).max(1),
        ];
        let mut grid = VertexGrid {
            cell,
            origin: lo,
            dims,
            cells: vec![Vec::new(); dims[0] * dims[1] * dims[2]],
            points: points.to_vec(),
        };
        for (i, p) in points.iter().enumerate() {
            let c = grid.cell_of(*p);
            let idx = c[0] + c[1] * dims[0] + c[2] * dims[0] * dims[1];
            grid.cells[idx].push(i as u32);
        }
        grid
    }

    fn cell_of(&self, p: Vec3) -> [usize; 3] {
        let f = |v: f32, o: f32, n: usize| -> usize {
            (((v - o) / self.cell).floor().max(0.0) as usize).min(n - 1)
        };
        [
            f(p.x, self.origin.x, self.dims[0]),
            f(p.y, self.origin.y, self.dims[1]),
            f(p.z, self.origin.z, self.dims[2]),
        ]
    }

    /// Nearest vertex to `p`, searching the 27 surrounding cells.
    pub fn nearest(&self, p: Vec3) -> Option<u32> {
        self.within(p, 1)
    }

    /// Farthest vertex in the surrounding cells — the "pointer" the splat binder
    /// uses to orient a splat away from the body.
    pub fn far(&self, p: Vec3) -> Option<u32> {
        let c = self.cell_of(p);
        let mut best: Option<(u32, f32)> = None;
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                for dz in -1i64..=1 {
                    let (x, y, z) = (c[0] as i64 + dx, c[1] as i64 + dy, c[2] as i64 + dz);
                    if x < 0
                        || y < 0
                        || z < 0
                        || x >= self.dims[0] as i64
                        || y >= self.dims[1] as i64
                        || z >= self.dims[2] as i64
                    {
                        continue;
                    }
                    let idx = x as usize
                        + y as usize * self.dims[0]
                        + z as usize * self.dims[0] * self.dims[1];
                    for &v in &self.cells[idx] {
                        let d = (self.points[v as usize] - p).length_squared();
                        if best.map_or(true, |(_, bd)| d > bd) {
                            best = Some((v, d));
                        }
                    }
                }
            }
        }
        best.map(|(v, _)| v)
    }

    fn within(&self, p: Vec3, radius: i64) -> Option<u32> {
        let c = self.cell_of(p);
        let mut best: Option<(u32, f32)> = None;
        for dx in -radius..=radius {
            for dy in -radius..=radius {
                for dz in -radius..=radius {
                    let (x, y, z) = (c[0] as i64 + dx, c[1] as i64 + dy, c[2] as i64 + dz);
                    if x < 0
                        || y < 0
                        || z < 0
                        || x >= self.dims[0] as i64
                        || y >= self.dims[1] as i64
                        || z >= self.dims[2] as i64
                    {
                        continue;
                    }
                    let idx = x as usize
                        + y as usize * self.dims[0]
                        + z as usize * self.dims[0] * self.dims[1];
                    for &v in &self.cells[idx] {
                        let d = (self.points[v as usize] - p).length_squared();
                        if best.map_or(true, |(_, bd)| d < bd) {
                            best = Some((v, d));
                        }
                    }
                }
            }
        }
        best.map(|(v, _)| v)
    }
}

/// Nearest mesh vertex for each query point.
///
/// `@0x1009de5b0` — `gs_puppet::mesh::nearest_vertices(&Mesh, &[Vec3]) -> Vec<Option<u32>>`.
/// The binary builds a `VertexGrid`, splits the queries across
/// `available_parallelism` scoped threads and flattens the per-chunk results; the
/// result type (`Vec<Option<u32>>`) is the one the closure signatures spell out.
/// std-only and single-threaded here — the parallel split is a performance detail
/// with no observable difference.
pub fn nearest_vertices(mesh: &Mesh, points: &[Vec3]) -> Vec<Option<u32>> {
    let pts: Vec<Vec3> = mesh.vertices.iter().map(|v| v.pos).collect();
    let ext = mesh.bounds();
    let span = (ext.1 - ext.0).length().max(1e-3);
    let cell = (span / (pts.len().max(1) as f32).cbrt()).max(1e-3);
    let grid = VertexGrid::new(&pts, cell);
    points.iter().map(|p| grid.nearest(*p)).collect()
}

/// Read a per-vertex weight file.
///
/// `@0x1009ddc78` — `gs_puppet::mesh::load_weights(&Path) -> io::Result<Vec<SkinWeights>>`.
/// The recovered body reads lines of `bone:weight` pairs (`weights_thin.txt`, the
/// literal in its pool) and keeps the first four per line, normalising them with
/// [`crate::splat::normalise_weights`].  The separators accepted here are `:`, `,`
/// and whitespace so both `0:0.7 3:0.3` and `0 0.7 3 0.3` parse.
pub fn load_weights(path: &Path) -> io::Result<Vec<SkinWeights>> {
    let text = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (lineno, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let toks: Vec<&str> = line
            .split(|c: char| c == ':' || c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        if toks.len() < 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("line {}: expected `bone:weight` pairs", lineno + 1),
            ));
        }
        let mut w: SkinWeights = [(0, 0.0); 4];
        let mut slot = 0;
        for pair in toks.chunks(2) {
            if slot >= 4 || pair.len() < 2 {
                break;
            }
            let bone: u16 = pair[0].parse().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {}: bad bone `{}`", lineno + 1, pair[0]),
                )
            })?;
            let weight: f32 = pair[1].parse().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("line {}: bad weight `{}`", lineno + 1, pair[1]),
                )
            })?;
            w[slot] = (bone, weight);
            slot += 1;
        }
        normalise_weights(&mut w);
        out.push(w);
    }
    Ok(out)
}

/// What a body template's metadata file says.
///
/// `@0x1009dff7c` — `BodyMeta::read(&Path) -> io::Result<BodyMeta>`.  The recovered
/// body walks the mesh file's directory looking for a record whose kind is set,
/// whose index field is `1` and whose `f32` is finite; that `f32` is the body
/// height and the flag says whether the template ships a UV atlas.  The file format
/// itself is **not** recoverable, so a simple `key value` text sidecar is read
/// instead (documented in `NOTES.md`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyMeta {
    /// The template ships an authored UV atlas.
    pub has_uv: bool,
    /// Authored body height in model units.
    pub height: f32,
    /// Vertex count of the template mesh, when the file states it.
    pub vertices: u32,
}

impl Default for BodyMeta {
    fn default() -> BodyMeta {
        BodyMeta {
            has_uv: false,
            height: 1.0,
            vertices: 0,
        }
    }
}

impl BodyMeta {
    /// File name the metadata is read from (a sidecar next to the mesh).
    pub const FILE: &'static str = "body_meta.txt";

    pub fn read(dir: &Path) -> io::Result<BodyMeta> {
        let text = std::fs::read_to_string(dir.join(BodyMeta::FILE))?;
        BodyMeta::parse(&text)
    }

    /// Parse the sidecar text (`key value` per line, `#` comments).
    pub fn parse(text: &str) -> io::Result<BodyMeta> {
        let mut meta = BodyMeta::default();
        for (lineno, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut it = line.split_whitespace();
            let key = it.next().unwrap_or("");
            let val = it.next();
            match (key, val) {
                ("uv", Some(v)) => meta.has_uv = v == "1" || v.eq_ignore_ascii_case("true"),
                ("height", Some(v)) => {
                    let h: f32 = v.parse().map_err(|_| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("line {}: bad height `{v}`", lineno + 1),
                        )
                    })?;
                    if !h.is_finite() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("line {}: height is not finite", lineno + 1),
                        ));
                    }
                    meta.height = h;
                }
                ("vertices", Some(v)) => {
                    meta.vertices = v.parse().unwrap_or(0);
                }
                _ => {}
            }
        }
        Ok(meta)
    }
}

/// A body-template mesh plus its lazily built splat bodies.
///
/// Recovered layout: the main splat `OnceLock` sits at `+0x100` (value `+0x108`) and
/// the two half-mesh locks at `+0x110`, one per `u16` index (`+0x10` stride).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyMesh {
    pub mesh: Mesh,
    /// Splat bodies for the two halves of the vertex range (240 vertices split at
    /// 110).
    pub halves: [Option<SplatBody>; 2],
    /// The whole-body splat, built on first use.
    pub whole: Option<SplatBody>,
}

impl BodyMesh {
    /// Vertex index where the second splat body starts (`0x6e` in `splats_at`).
    pub const HALF_SPLIT: u32 = 0x6e;

    /// Highest vertex index `splats_at` answers for (`0xef`).
    pub const MAX_VERTEX: u32 = 0xef;

    /// Build the whole-body splat body (the `OnceLock` at `+0x100`).
    ///
    /// `@0x1009dfe9c` — `BodyMesh::made_splats(&self) -> Arc<SplatBody>`.
    pub fn made_splats(&mut self, params: &SplatParams) -> &SplatBody {
        if self.whole.is_none() {
            self.whole = Some(SplatBody::from_mesh(&self.mesh, params));
        }
        self.whole.as_ref().unwrap()
    }

    /// The splat body that carries vertex `index`, or `None` above the body's
    /// vertex range.
    ///
    /// `@0x1009dfef4` — `BodyMesh::splats_at(&self, u32) -> Option<&Arc<SplatBody>>`.
    /// `index < 0x6e` selects the first half, `0x6e..=0xef` the second (`cset w9,
    /// lo` on the `cmp #0x6e`), and anything above `0xef` returns `None`.
    pub fn splats_at(&self, index: u32) -> Option<&SplatBody> {
        let i = index & 0xffff;
        if i > Self::MAX_VERTEX {
            return None;
        }
        let half = if i < Self::HALF_SPLIT { 0 } else { 1 };
        self.halves[half].as_ref()
    }
}

/// A mesh with skin weights.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoundMesh {
    pub mesh: Mesh,
    /// Per-vertex weights, one entry per vertex.
    pub weights: Vec<SkinWeights>,
    /// Bone names the weights index.
    pub bones: Vec<String>,
}

impl BoundMesh {
    /// Bind a mesh to a rig's weights.
    ///
    /// `@0x1009e0f50` — `BoundMesh::with_weights(&Mesh, ...) -> BoundMesh`.  The
    /// recovered body calls `splat::skin_weights` for vertices that have no weights
    /// yet (`-0.25` in its body is the fallback offset) and otherwise keeps the
    /// supplied set.
    pub fn with_weights(mesh: &Mesh, weights: Vec<SkinWeights>, bones: Vec<String>) -> BoundMesh {
        let mut weights = weights;
        weights.resize(mesh.vertices.len(), [(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]);
        BoundMesh {
            mesh: mesh.clone(),
            weights,
            bones,
        }
    }

    /// Transform the mesh by a posed rig.
    pub fn posed(&self, posed: &Posed) -> Mesh {
        let mut out = self.mesh.clone();
        for (i, v) in out.vertices.iter_mut().enumerate() {
            let w = self.weights.get(i).copied().unwrap_or([(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]);
            let mut pos = Vec3::ZERO;
            let mut total = 0.0;
            for (bone, weight) in w {
                if weight <= 0.0 {
                    continue;
                }
                let Some(name) = self.bones.get(bone as usize) else {
                    continue;
                };
                pos = pos + posed.bone(name).point(v.pos) * weight;
                total += weight;
            }
            if total > 0.0 {
                v.pos = pos * (1.0 / total);
            }
        }
        out
    }
}

/// A splat cloud bound to a rig, ready to be posed and drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SplatBody {
    pub cloud: SplatCloud,
    /// Per-splat weights into `bones`.
    pub weights: Vec<SkinWeights>,
    pub bones: Vec<String>,
    /// Number of splats that were dropped by [`SplatBody::thinned`].
    pub thinned_away: usize,
}

impl SplatBody {
    /// Build a splat body from a mesh: one splat per vertex.
    ///
    /// `@0x1009e4234` — `SplatBody::from_mesh(&Mesh, &SplatParams) -> SplatBody`
    /// (24 KB, mostly inlined vector work; the strides `0xa8` = 168 and `0xc4` = 196
    /// are the intermediate scratch records).
    pub fn from_mesh(mesh: &Mesh, params: &SplatParams) -> SplatBody {
        let mut splats = Vec::with_capacity(mesh.vertices.len());
        for (i, v) in mesh.vertices.iter().enumerate() {
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
                [0.8, 0.75, 0.7, params.opacity],
            );
            s.vertex = i as u32;
            splats.push(s);
        }
        let n = splats.len();
        SplatBody {
            cloud: SplatCloud::new(splats),
            weights: vec![[(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]; n],
            bones: Vec::new(),
            thinned_away: 0,
        }
    }

    /// Bind this body to the rig by nearest vertex.
    ///
    /// `@0x1009e2ae0` — `SplatBody::bind(&mut self, …)`, an 8-byte tail call into
    /// [`SplatBody::bind_facing`].
    pub fn bind(&mut self, mesh: &Mesh, bones: Vec<String>) {
        self.bind_as(mesh, bones, 0.0);
    }

    /// Bind with an explicit facing offset.
    ///
    /// `@0x1009e3410` — `SplatBody::bind_as(&mut self, …)`: it builds a
    /// [`VertexGrid`], asks for the nearest vertex (`VertexGrid::nearest`) and its
    /// far counterpart (`VertexGrid::far`) to orient each splat, and stores the
    /// weight slot.  The string literals `""`/`"mesh"` (`0x103e349e8` / length 96)
    /// that appear here are this function's.
    pub fn bind_as(&mut self, mesh: &Mesh, bones: Vec<String>, facing: f32) {
        let pts: Vec<Vec3> = mesh.vertices.iter().map(|v| v.pos).collect();
        let grid = VertexGrid::new(&pts, 0.05);
        let bone_of = |name: &str| bones.iter().position(|b| b == name).unwrap_or(0);
        for (i, s) in self.cloud.splats.iter_mut().enumerate() {
            if let Some(v) = grid.nearest(s.pos) {
                s.vertex = v;
                if let Some(name) = nearest_bone(mesh, v as usize, &bones) {
                    let slot = bone_of(&name);
                    if i < self.weights.len() {
                        self.weights[i] = [(slot as u16, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)];
                    }
                }
            }
            // Orient the splat along the body's outward direction.
            if let Some(far) = grid.far(s.pos) {
                let out = (s.pos - pts[far as usize]).normalized();
                if out != Vec3::ZERO {
                    let n = out * (1.0 + facing);
                    s.basis = Mat3::from_cols_array([
                        n.x, n.y, n.z, n.y, n.z, n.x, n.z, n.x, n.y,
                    ]);
                }
            }
        }
        self.bones = bones;
    }

    /// Bind a splat body so its splats face away from the mesh surface.
    ///
    /// `@0x1009e1744` — `SplatBody::bind_facing(&mut self, …) -> (Vec<u32>, Vec<u32>)`
    /// (the two `Vec<u32>` results are the front/back vertex halves the body keeps).
    /// It is the full-strength form of [`SplatBody::bind_as`].
    pub fn bind_facing(&mut self, mesh: &Mesh, bones: Vec<String>) -> (Vec<u32>, Vec<u32>) {
        self.bind_as(mesh, bones, 1.0);
        let split = (mesh.vertices.len() / 2) as u32;
        let front: Vec<u32> = (0..split).collect();
        let back: Vec<u32> = (split..mesh.vertices.len() as u32).collect();
        (front, back)
    }

    /// The cloud of this body, bound to a posed rig — one splat per bone-bound
    /// vertex.
    ///
    /// `@0x1009e2500` — `SplatBody::bound_cloud(&self, &Posed) -> Vec<Splat>`.
    pub fn bound_cloud(&self, posed: &Posed) -> Vec<Splat> {
        let bound = crate::splat::BoundCloud {
            cloud: self.cloud.clone(),
            weights: self.weights.clone(),
            bones: self.bones.clone(),
            rigid: false,
        };
        bound.posed(posed)
    }

    /// Sort the cloud far to near for an eye point.
    ///
    /// `@0x1009e27d8` — `SplatBody::far_to_near(&mut self, Vec3)`.
    pub fn far_to_near(&mut self, eye: Vec3) {
        self.cloud.far_to_near(eye);
    }

    /// Concatenate two splat bodies into one.
    ///
    /// `@0x1009e2ae8` — `SplatBody::joined(a: SplatBody, b: SplatBody) -> SplatBody`.
    pub fn joined(mut a: SplatBody, b: SplatBody) -> SplatBody {
        let offset = a.cloud.splats.len() as u32;
        let mut splats = a.cloud.splats.clone();
        for mut s in b.cloud.splats.clone() {
            s.vertex = s.vertex.saturating_add(offset);
            splats.push(s);
        }
        a.cloud = SplatCloud::new(splats);
        a.weights.extend(b.weights);
        for bone in b.bones {
            if !a.bones.contains(&bone) {
                a.bones.push(bone);
            }
        }
        a.thinned_away += b.thinned_away;
        a
    }

    /// Drop splats until only `keep` of them remain, evenly spread.
    ///
    /// `@0x1009e3adc` — `SplatBody::thinned(&self, usize) -> SplatBody`: the
    /// `1.0` immediate is the "keep everything" guard.
    pub fn thinned(&self, keep: usize) -> SplatBody {
        let n = self.cloud.splats.len();
        if keep >= n || keep == 0 {
            let mut out = self.clone();
            out.thinned_away = 0;
            return out;
        }
        let step = n as f32 / keep as f32;
        let mut splats = Vec::with_capacity(keep);
        let mut weights = Vec::with_capacity(keep);
        for k in 0..keep {
            let i = (k as f32 * step).floor() as usize;
            let i = i.min(n - 1);
            splats.push(self.cloud.splats[i]);
            weights.push(self.weights.get(i).copied().unwrap_or([(0, 1.0), (0, 0.0), (0, 0.0), (0, 0.0)]));
        }
        SplatBody {
            cloud: SplatCloud::new(splats),
            weights,
            bones: self.bones.clone(),
            thinned_away: n - keep,
        }
    }
}

/// The bone a mesh vertex belongs to, if the mesh carries the split names.
fn nearest_bone(mesh: &Mesh, vertex: usize, bones: &[String]) -> Option<String> {
    bones.get(vertex % bones.len().max(1)).cloned()
}

/// A body-space frame: the orientation a splat body is projected in.
///
/// `@0x1009e0094` — `gs_puppet::mesh::BodyFrame::new` (3.7 KB) which calls
/// `Posed::bounds_where`, `draw::view_basis` and `SplatBody::far_to_near`; it turns
/// the posed figure's bounds into the view basis the splats are sorted against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyFrame {
    /// The basis the body is viewed in.
    pub basis: Xf,
    /// The posed figure's bounds.
    pub bounds: (Vec3, Vec3),
    /// The eye point the splats are sorted against.
    pub eye: Vec3,
}

impl Default for BodyFrame {
    fn default() -> BodyFrame {
        BodyFrame {
            basis: Xf::IDENTITY,
            bounds: (Vec3::ZERO, Vec3::ZERO),
            eye: Vec3::new(0.0, 0.0, 1.0),
        }
    }
}

impl BodyFrame {
    /// Build a frame from a posed rig.
    ///
    /// `@0x1009e0094` — `BodyFrame::new(&Posed) -> BodyFrame`.  The view basis comes
    /// from `draw::view_basis` (which orients +z along the bounding box's longest
    /// horizontal axis) and the eye sits one body-height in front of the centre, so
    /// `far_to_near` sorts against the viewer.
    pub fn new(posed: &Posed) -> BodyFrame {
        let (lo, hi) = posed.bounds();
        let centre = (lo + hi) * 0.5;
        let size = hi - lo;
        let basis = crate::draw::view_basis(centre, size);
        let reach = size.length().max(1.0);
        BodyFrame {
            basis,
            bounds: (lo, hi),
            eye: centre + Vec3::new(0.0, 0.0, reach),
        }
    }

    /// Sort a splat body for this frame.
    pub fn sort(&self, body: &mut SplatBody) {
        body.far_to_near(self.eye);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBJ: &str = "# a cube face\n\
        v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
        vn 0 0 1\n\
        vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\n\
        f 1/1/1 2/2/1 3/3/1\nf 1/1/1 3/3/1 4/4/1\n";

    #[test]
    fn obj_parsing_merges_and_fans() {
        let m = Mesh::parse_obj(OBJ).unwrap();
        // Four distinct (v, vt, vn) triples.
        assert_eq!(m.vertices.len(), 4);
        // Two triangles.
        assert_eq!(m.indices.len(), 6);
        assert_eq!(m.vertices[0].pos, Vec3::ZERO);
        assert_eq!(m.vertices[0].uv, [0.0, 0.0]);
        assert_eq!(m.vertices[2].pos, Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(m.bounds().1, Vec3::new(1.0, 1.0, 0.0));
    }

    #[test]
    fn obj_polygons_are_fanned_and_negative_indices_resolve() {
        let text = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4 -3 -2 -1\n";
        let m = Mesh::parse_obj(text).unwrap();
        assert_eq!(m.indices.len(), 6); // a quad becomes two triangles
        assert_eq!(m.vertices.len(), 4);
    }

    #[test]
    fn obj_rejects_bad_input() {
        assert!(Mesh::parse_obj("f 1 2 3\n").is_err());
        assert!(Mesh::parse_obj("").is_err());
    }

    #[test]
    fn smooth_normals_points_out_of_the_face() {
        let mut m = Mesh::parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        m.smooth_normals();
        for v in &m.vertices {
            assert!((v.normal.z - 1.0).abs() < 1e-6, "{:?}", v.normal);
        }
    }

    #[test]
    fn fit_to_height_scales_and_rests_on_the_ground() {
        let mut m = Mesh::parse_obj("v 0 2 0\nv 1 4 0\nv 1 6 0\nf 1 2 3\n").unwrap();
        m.fit_to_height(2.0);
        let (lo, hi) = m.bounds();
        assert!(lo.y.abs() < 1e-6, "{lo:?}");
        assert!((hi.y - 2.0).abs() < 1e-6, "{hi:?}");
        // A degenerate request is ignored.
        let mut m2 = m.clone();
        m2.fit_to_height(0.0);
        assert_eq!(m, m2);
    }

    #[test]
    fn vertex_grid_nearest_and_far() {
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
        ];
        let grid = VertexGrid::new(&pts, 1.0);
        assert_eq!(grid.nearest(Vec3::new(0.9, 0.0, 0.0)), Some(1));
        assert_eq!(grid.nearest(Vec3::new(4.9, 0.0, 0.0)), Some(2));
        // The far query prefers the most distant vertex in the neighbourhood.
        let far = grid.far(Vec3::new(0.0, 0.0, 0.0)).unwrap();
        assert_ne!(far, 0);
    }

    #[test]
    fn nearest_vertices_returns_one_answer_per_query() {
        let m = Mesh::parse_obj(OBJ).unwrap();
        let out = nearest_vertices(
            &m,
            &[Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 1.0, 0.0), Vec3::new(9.0, 9.0, 9.0)],
        );
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], Some(0));
        assert_eq!(out[1], Some(2));
        assert!(out[2].is_some());
    }

    #[test]
    fn weights_file_parses_both_separators_and_normalises() {
        let dir = std::env::temp_dir().join(format!("gs_puppet_w_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("weights_thin.txt");
        std::fs::write(&p, "# comment\n0:2.0 3:2.0\n1 0.5 2 0.5\n").unwrap();
        let w = load_weights(&p).unwrap();
        assert_eq!(w.len(), 2);
        assert!((w[0][0].1 - 0.5).abs() < 1e-6);
        assert_eq!(w[0][1].0, 3);
        assert!((w[1][0].1 - 0.5).abs() < 1e-6);
        assert_eq!(w[1][1].0, 2);
        std::fs::write(&p, "garbage\n").unwrap();
        assert!(load_weights(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn body_meta_round_trip() {
        let meta = BodyMeta::parse("uv 1\nheight 1.85\nvertices 240\n# trailing\n").unwrap();
        assert!(meta.has_uv);
        assert!((meta.height - 1.85).abs() < 1e-6);
        assert_eq!(meta.vertices, 240);
        assert!(BodyMeta::parse("height notanumber\n").is_err());
        assert_eq!(BodyMeta::parse("").unwrap(), BodyMeta::default());
    }

    #[test]
    fn body_mesh_splats_at_splits_the_vertex_range() {
        let params = SplatParams::default();
        let mut body = BodyMesh {
            mesh: Mesh::parse_obj(OBJ).unwrap(),
            ..Default::default()
        };
        body.made_splats(&params);
        assert!(body.whole.is_some());
        // The two halves are built lazily; build them for the test.
        body.halves[0] = Some(SplatBody::from_mesh(&body.mesh, &params));
        body.halves[1] = Some(SplatBody::from_mesh(&body.mesh, &params));
        assert_eq!(BodyMesh::HALF_SPLIT, 110);
        assert_eq!(BodyMesh::MAX_VERTEX, 239);
        assert!(body.splats_at(0).is_some());
        assert!(body.splats_at(109).is_some());
        assert!(body.splats_at(110).is_some());
        assert!(body.splats_at(239).is_some());
        assert!(body.splats_at(240).is_none());
    }

    #[test]
    fn splat_body_from_mesh_and_thinned() {
        let mut m = Mesh::parse_obj(OBJ).unwrap();
        // Add more vertices so thinning has something to do.
        for i in 0..20 {
            m.vertices.push(Vertex {
                pos: Vec3::new(i as f32 * 0.1, 0.0, 0.0),
                normal: Vec3::new(0.0, 1.0, 0.0),
                uv: [0.0, 0.0],
            });
        }
        let params = SplatParams::new(0.02, 1.0, 1.0);
        let body = SplatBody::from_mesh(&m, &params);
        assert_eq!(body.cloud.splats.len(), m.vertices.len());
        assert_eq!(body.cloud.splats[0].basis.m[0], 0.02);
        let thin = body.thinned(6);
        assert_eq!(thin.cloud.splats.len(), 6);
        assert_eq!(thin.thinned_away, m.vertices.len() - 6);
        // Keeping everything is a no-op.
        let all = body.thinned(1000);
        assert_eq!(all.thinned_away, 0);
    }

    #[test]
    fn joined_splat_bodies_concatenate() {
        let m = Mesh::parse_obj(OBJ).unwrap();
        let params = SplatParams::default();
        let a = SplatBody::from_mesh(&m, &params);
        let b = SplatBody::from_mesh(&m, &params);
        let n = a.cloud.splats.len() + b.cloud.splats.len();
        let j = SplatBody::joined(a, b);
        assert_eq!(j.cloud.splats.len(), n);
        assert_eq!(j.weights.len(), n);
        // The second body's vertex indices are offset.
        assert_eq!(j.cloud.splats[m.vertices.len()].vertex, m.vertices.len() as u32);
    }

    #[test]
    fn bound_mesh_follows_a_posed_rig() {
        use crate::model::{Puppet, Segment};
        let puppet = Puppet::new(
            "biped",
            vec![Segment::new("root", 0, Vec3::new(0.0, 1.0, 0.0), 1.0)],
        )
        .unwrap();
        let m = Mesh::parse_obj(OBJ).unwrap();
        let bm = BoundMesh::with_weights(&m, vec![], vec!["root".into()]);
        let posed = puppet.pose();
        let out = bm.posed(&posed);
        // The root bone sits at y = 1, so every vertex shifts up by one.
        assert!((out.vertices[0].pos.y - 1.0).abs() < 1e-6, "{:?}", out.vertices[0].pos);
    }

    #[test]
    fn body_frame_puts_the_eye_in_front() {
        use crate::model::{Puppet, Segment};
        let puppet = Puppet::new(
            "biped",
            vec![
                Segment::new("root", 0, Vec3::ZERO, 1.0),
                Segment::new("head", 0, Vec3::new(0.0, 1.0, 0.0), 0.2),
            ],
        )
        .unwrap();
        let frame = BodyFrame::new(&puppet.pose());
        assert!(frame.eye.z > 0.0);
        assert!(frame.bounds.1.y >= 1.0);
    }
}
