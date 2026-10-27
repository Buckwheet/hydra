//! Small, `std`-only linear algebra for the puppet rig.
//!
//! Recovered from the arm64 binary (`gs_puppet::math::*`).  The original crate
//! used `glam`, but every layout and constant below is read back out of the
//! instruction stream, so the types here are the recovered ones:
//!
//! * [`Vec3`] is three `f32` (12 bytes, align 4) — `ldr s` at `+0`, `+4`, `+8`.
//! * [`Mat3`] is nine `f32` (36 bytes) in **row-major** order: `quat_from_m3`
//!   reads the diagonal at `+0`, `+16`, `+32` and the off-diagonals as
//!   consecutive triples starting at `+0`, `+12`, `+24`, and [`Xf::dir`] /
//!   [`Xf::point`] compute `dst[i] = dot(m[3*i..3*i+3], v)`.
//! * [`Quat`] is four `f32` with **`w` first** (16 bytes): the identity constant
//!   at `0x1021b4d40` and the pose-override default at `0x1021dc4a8` are both
//!   `[1.0, 0.0, 0.0, 0.0]`, and [`quat_norm`] streams that same constant when
//!   the length is degenerate.
//! * [`Xf`] is a rigid transform, 48 bytes: the 3x3 rotation at `+0` (row-major,
//!   as above) and the translation at `+0x24`.
//!
//! Rotation convention: the crate is **row-vector**.  [`quat_from_m3`] returns
//! the conjugate of the quaternion a column-major reader would extract from the
//! same 9 floats, which is exactly what a row-major array means.  Concretely,
//! for the row-major `Rz(+90°)` (`m = [0,-1,0, 1,0,0, 0,0,1]`) the returned
//! quaternion is `(w = √½, x = 0, y = 0, z = +√½)`; see the tests.

use std::ops::{Add, Mul, Neg, Sub};

/// Degrees to radians, `0x3c8efa35`.  Every authored angle in the puppet data
/// (pose TOML, [`crate::camera::Camera`]) is in degrees and is converted with
/// this constant.
pub const DEG_TO_RAD: f32 = 0.017_453_292;

/// `π / 180` as `f64`, for the same conversion done in double precision.
pub const DEG_TO_RAD_F64: f64 = 0.017_453_292_519_943_295;

/// Degenerate-length cutoff used by [`quat_norm`] and [`quat_from_m3`]: a
/// quaternion whose length is `<= 1e-9` (or not finite) normalises to the
/// identity.  Immediate `0x3089705f`.
pub const QUAT_EPSILON: f32 = 1.0e-9;

/// [`slerp`] switches to normalised-lerp once the (sign-corrected) dot product
/// exceeds this, immediate `0x3f7fdf3b` = `0.9995`.
pub const SLERP_DOT_THRESHOLD: f32 = 0.9995;

/// A 3-vector of `f32`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = Vec3::new(1.0, 1.0, 1.0);
    /// `f32::MAX` per axis — the seed value `Limit::clamp` uses for a min-fold
    /// (table at `0x1021b4d70` is `0x7f7fffff` twelve times).
    pub const MAX: Vec3 = Vec3::new(f32::MAX, f32::MAX, f32::MAX);
    /// `f32::MIN` per axis — the seed value for a max-fold (table at
    /// `0x1021b4d60`).
    pub const MIN: Vec3 = Vec3::new(f32::MIN, f32::MIN, f32::MIN);

    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        Vec3 { x, y, z }
    }

    pub const fn splat(v: f32) -> Vec3 {
        Vec3::new(v, v, v)
    }

    pub fn from_slice(s: &[f32]) -> Vec3 {
        Vec3::new(s[0], s[1], s[2])
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    /// Normalise, returning `Vec3::ZERO` for a degenerate vector (the same
    /// guard `quat_norm` uses: `<= 1e-9` or non-finite).
    pub fn normalized(self) -> Vec3 {
        let len = self.length();
        if !(len > QUAT_EPSILON) || !len.is_finite() {
            return Vec3::ZERO;
        }
        self * (1.0 / len)
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    pub fn min(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }

    pub fn max(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }

    /// Per-axis clamp, used by [`crate::pose::Limit`].
    pub fn clamp(self, lo: Vec3, hi: Vec3) -> Vec3 {
        self.max(lo).min(hi)
    }

    /// Mirror across the sagittal plane: `(x, -y, -z)`.
    ///
    /// The shipped pose TOML documents the rule ("leg poses `leg_l` as written
    /// and `leg_r` mirrored (y, z negated)") and [`crate::pose::PoseSet`]
    /// applies it whenever a pair key has to be split.
    pub fn mirrored(self) -> Vec3 {
        Vec3::new(self.x, -self.y, -self.z)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f32) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

/// A 3x3 matrix of `f32`, **row-major** (`m[row * 3 + col]`), 36 bytes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat3 {
    pub m: [f32; 9],
}

impl Default for Mat3 {
    fn default() -> Mat3 {
        Mat3::IDENTITY
    }
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 {
        m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };

    pub const fn from_cols_array(m: [f32; 9]) -> Mat3 {
        Mat3 { m }
    }

    /// Row `i`.
    pub fn row(self, i: usize) -> Vec3 {
        Vec3::new(self.m[i * 3], self.m[i * 3 + 1], self.m[i * 3 + 2])
    }

    /// Column `i`.
    pub fn col(self, i: usize) -> Vec3 {
        Vec3::new(self.m[i], self.m[3 + i], self.m[6 + i])
    }

    pub fn transpose(self) -> Mat3 {
        let m = self.m;
        Mat3 {
            m: [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]],
        }
    }

    pub fn mul_vec3(self, v: Vec3) -> Vec3 {
        Vec3::new(self.row(0).dot(v), self.row(1).dot(v), self.row(2).dot(v))
    }

    pub fn mul_mat3(self, o: Mat3) -> Mat3 {
        let mut m = [0.0f32; 9];
        for r in 0..3 {
            for c in 0..3 {
                m[r * 3 + c] = self.row(r).dot(o.col(c));
            }
        }
        Mat3 { m }
    }

    /// Rotation about `x` by `rad` (row-vector convention, same flat layout as
    /// [`quat_to_m3`]).
    pub fn from_rotation_x(rad: f32) -> Mat3 {
        let (s, c) = rad.sin_cos();
        Mat3 {
            m: [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c],
        }
    }

    pub fn from_rotation_y(rad: f32) -> Mat3 {
        let (s, c) = rad.sin_cos();
        Mat3 {
            m: [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c],
        }
    }

    pub fn from_rotation_z(rad: f32) -> Mat3 {
        let (s, c) = rad.sin_cos();
        Mat3 {
            m: [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0],
        }
    }
}

/// A unit quaternion, fields in the recovered order `(w, x, y, z)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub w: f32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Quat {
    fn default() -> Quat {
        Quat::IDENTITY
    }
}

impl Quat {
    /// `[1.0, 0.0, 0.0, 0.0]` — the literal at `0x1021b4d40` / `0x1021dc4a8`.
    pub const IDENTITY: Quat = Quat {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(w: f32, x: f32, y: f32, z: f32) -> Quat {
        Quat { w, x, y, z }
    }

    pub fn from_array(a: [f32; 4]) -> Quat {
        Quat::new(a[0], a[1], a[2], a[3])
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.w, self.x, self.y, self.z]
    }

    pub fn dot(self, o: Quat) -> f32 {
        self.w * o.w + self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn conjugate(self) -> Quat {
        Quat::new(self.w, -self.x, -self.y, -self.z)
    }

    pub fn normalized(self) -> Quat {
        quat_norm(self)
    }

    /// Rotation of `rad` about a (not necessarily normalised) axis.
    pub fn from_axis_angle(axis: Vec3, rad: f32) -> Quat {
        let a = axis.normalized();
        let (s, c) = (rad * 0.5).sin_cos();
        Quat::new(c, a.x * s, a.y * s, a.z * s)
    }

    /// Rotation of `deg` degrees about an axis, the form the pose data uses.
    pub fn from_axis_deg(axis: Vec3, deg: f32) -> Quat {
        Quat::from_axis_angle(axis, deg * DEG_TO_RAD)
    }

    pub fn mul_quat(self, o: Quat) -> Quat {
        Quat::new(
            self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        )
    }

    /// Rotate a vector (row-vector convention, matching [`quat_to_m3`]).
    pub fn mul_vec3(self, v: Vec3) -> Vec3 {
        quat_to_m3(&self).mul_vec3(v)
    }
}

/// Normalise a quaternion; degenerate or non-finite input yields the identity.
///
/// `@0x1009feab8` — `gs_puppet::math::quat_norm`.  The length is compared with
/// `1e-9` (`0x3089705f`) and with `+inf`; anything outside `(1e-9, +inf]` (which
/// includes NaN, since the ordered compares both fail) returns the identity
/// constant at `0x1021b4d40`.
pub fn quat_norm(q: Quat) -> Quat {
    let len = q.length();
    if !(len > QUAT_EPSILON) || !len.is_finite() {
        return Quat::IDENTITY;
    }
    let inv = 1.0 / len;
    Quat::new(q.w * inv, q.x * inv, q.y * inv, q.z * inv)
}

/// Convert a row-major rotation matrix to a quaternion.
///
/// `@0x1009fe4c8` — `gs_puppet::math::quat_from_m3(&Mat3) -> Quat`.  Shepperd's
/// method over the four branches (trace, largest `m00`, largest `m11`, largest
/// `m22`); the recovered formulas are reproduced verbatim, including the fact
/// that the **trace branch negates the vector part** while the three
/// diagonal-dominant branches do not (both are the same rotation up to sign, so
/// this is `conj` of the column-major reading — the row-vector convention).
pub fn quat_from_m3(m: &Mat3) -> Quat {
    let m = &m.m;
    let tr = m[0] + m[4] + m[8];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        Quat::new(
            0.25 * s,          // w
            (m[7] - m[5]) / s, // x
            (m[2] - m[6]) / s, // y
            (m[3] - m[1]) / s, // z
        )
    } else if m[0] >= m[4] && m[0] >= m[8] {
        let s = (1.0 + m[0] - m[4] - m[8]).sqrt() * 2.0;
        Quat::new(
            (m[7] - m[5]) / s,
            0.25 * s,
            (m[1] + m[3]) / s,
            (m[2] + m[6]) / s,
        )
    } else if m[4] > m[8] {
        let s = (1.0 + m[4] - m[0] - m[8]).sqrt() * 2.0;
        Quat::new(
            (m[2] - m[6]) / s,
            (m[1] + m[3]) / s,
            0.25 * s,
            (m[5] + m[7]) / s,
        )
    } else {
        let s = (1.0 + m[8] - m[0] - m[4]).sqrt() * 2.0;
        Quat::new(
            (m[3] - m[1]) / s,
            (m[2] + m[6]) / s,
            (m[5] + m[7]) / s,
            0.25 * s,
        )
    };
    quat_norm(q)
}

/// Convert a quaternion to a row-major rotation matrix — the inverse of
/// [`quat_from_m3`].
///
/// `@0x1009fe41c` — `gs_puppet::math::quat_to_m3(&Quat) -> Mat3` (a local
/// symbol; the body is NEON-scheduled but it is the textbook
/// `1 - 2(y²+z²)` form).  `quat_from_m3(&quat_to_m3(&q)) == quat_norm(q)` is
/// asserted in the tests, which pins the convention.
pub fn quat_to_m3(q: &Quat) -> Mat3 {
    let (w, x, y, z) = (q.w, q.x, q.y, q.z);
    Mat3 {
        m: [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    }
}

/// Spherical linear interpolation.
///
/// `@0x1009fe8b0` — `gs_puppet::math::slerp(&Quat, &Quat, f32) -> Quat`.
/// Recovered behaviour, in order:
///
/// 1. `dot = a · b`; if `dot < 0` the *second* quaternion is negated and `dot`
///    is flipped (shortest arc).
/// 2. if `dot > 0.9995` (`0x3f7fdf3b`) fall back to a normalised lerp;
/// 3. otherwise `dot` is clamped up to `-1.0` (`acos` guard), `θ = acos(dot)`,
///    and the result is `a·sin((1-t)θ)/sin θ + b·sin(tθ)/sin θ`.
///
/// Both paths finish through [`quat_norm`], so a degenerate result becomes the
/// identity.
pub fn slerp(a: &Quat, b: &Quat, t: f32) -> Quat {
    let mut b = *b;
    let mut dot = a.dot(b);
    if dot < 0.0 {
        b = Quat::new(-b.w, -b.x, -b.y, -b.z);
        dot = -dot;
    }
    if dot > SLERP_DOT_THRESHOLD {
        let r = Quat::new(
            a.w + (b.w - a.w) * t,
            a.x + (b.x - a.x) * t,
            a.y + (b.y - a.y) * t,
            a.z + (b.z - a.z) * t,
        );
        return quat_norm(r);
    }
    let dot = dot.max(-1.0);
    let theta_0 = dot.acos();
    let sin_theta = theta_0.sin();
    let s0 = ((1.0 - t) * theta_0).sin() / sin_theta;
    let s1 = (t * theta_0).sin() / sin_theta;
    quat_norm(Quat::new(
        a.w * s0 + b.w * s1,
        a.x * s0 + b.x * s1,
        a.y * s0 + b.y * s1,
        a.z * s0 + b.z * s1,
    ))
}

/// Euler angles in degrees to a rotation matrix.
///
/// The shipped pose data spells the convention out: "`rot = [x, y, z]` Euler …
/// applied `Rx * Rz * Ry`: twist (y), then spread (z), then swing (x)."  The
/// binary agrees about the *evaluation order*: `Pose::set_euler` (`0x100a482dc`)
/// and `Limit::clamp` (`0x100a486a4`) call `sincos` on component 0 first, then
/// component 2, then component 1, and both then call [`quat_from_m3`] on the
/// composed matrix.
pub fn euler_m3(deg: Vec3) -> Mat3 {
    let rx = Mat3::from_rotation_x(deg.x * DEG_TO_RAD);
    let ry = Mat3::from_rotation_y(deg.y * DEG_TO_RAD);
    let rz = Mat3::from_rotation_z(deg.z * DEG_TO_RAD);
    rx.mul_mat3(rz).mul_mat3(ry)
}

/// Euler angles in degrees to a quaternion (via [`euler_m3`]).
pub fn euler_quat(deg: Vec3) -> Quat {
    quat_from_m3(&euler_m3(deg))
}

/// A rigid transform: 3x3 rotation (row-major) plus translation.  48 bytes —
/// `Xf::point` reads `self` up to `+0x2c` and `Xf::then_inner` loads/stores
/// three 16-byte vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xf {
    /// Row-major rotation.
    pub m: Mat3,
    /// Translation applied after the rotation (`+0x24` in the binary).
    pub t: Vec3,
}

impl Default for Xf {
    fn default() -> Xf {
        Xf::IDENTITY
    }
}

impl Xf {
    pub const IDENTITY: Xf = Xf {
        m: Mat3::IDENTITY,
        t: Vec3::ZERO,
    };

    pub fn new(m: Mat3, t: Vec3) -> Xf {
        Xf { m, t }
    }

    pub fn from_translation(t: Vec3) -> Xf {
        Xf {
            m: Mat3::IDENTITY,
            t,
        }
    }

    pub fn from_rotation(q: &Quat) -> Xf {
        Xf {
            m: quat_to_m3(q),
            t: Vec3::ZERO,
        }
    }

    /// Transform a direction: rotation only.
    ///
    /// `@0x1009fe7d8` — three dot products of the *rows* of `self` with `v`,
    /// which is what fixes the row-major layout.
    pub fn dir(&self, v: Vec3) -> Vec3 {
        self.m.mul_vec3(v)
    }

    /// Transform a point: rotation then translation.
    ///
    /// `@0x1009fe83c` — identical dot products with `self.t` (`+0x24`, `+0x28`,
    /// `+0x2c`) added per axis.
    pub fn point(&self, v: Vec3) -> Vec3 {
        self.m.mul_vec3(v) + self.t
    }

    /// Compose: "apply `self`, then `inner`".
    ///
    /// `@0x1009fe688` — `gs_puppet::math::Xf::then_inner(&self, &Xf) -> Xf`.
    /// The result satisfies `self.then_inner(inner).point(v) ==
    /// inner.point(self.point(v))`, which is asserted in the tests; the NEON
    /// body is a fully inlined 3x3 x 3x3 + translation product.
    pub fn then_inner(&self, inner: &Xf) -> Xf {
        Xf {
            m: inner.m.mul_mat3(self.m),
            t: inner.m.mul_vec3(self.t) + inner.t,
        }
    }

    /// Interpolate the rotation with [`slerp`] and the translation linearly.
    pub fn lerp(&self, o: &Xf, t: f32) -> Xf {
        let qa = quat_from_m3(&self.m);
        let qb = quat_from_m3(&o.m);
        Xf {
            m: quat_to_m3(&slerp(&qa, &qb, t)),
            t: self.t * (1.0 - t) + o.t * t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn recovered_constants() {
        assert_eq!(DEG_TO_RAD.to_bits(), 0x3c8e_fa35);
        assert_eq!(QUAT_EPSILON.to_bits(), 0x3089_705f);
        assert_eq!(SLERP_DOT_THRESHOLD.to_bits(), 0x3f7f_df3b);
        assert!((SLERP_DOT_THRESHOLD - 0.9995).abs() < 1e-7);
        assert!((DEG_TO_RAD - std::f32::consts::PI / 180.0).abs() < 1e-8);
    }

    #[test]
    fn identity_quat_is_w_first() {
        assert_eq!(Quat::IDENTITY.to_array(), [1.0, 0.0, 0.0, 0.0]);
        // Round trip through the matrix keeps the leading 1.0.
        let m = quat_to_m3(&Quat::IDENTITY);
        assert_eq!(m, Mat3::IDENTITY);
        assert_eq!(quat_from_m3(&Mat3::IDENTITY), Quat::IDENTITY);
    }

    #[test]
    fn quat_from_m3_z_rotation() {
        // Row-major Rz(+90°).
        let m = Mat3::from_cols_array([0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        let q = quat_from_m3(&m);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert!(close(q.w, h, 1e-6), "{q:?}");
        assert!(close(q.x, 0.0, 1e-6));
        assert!(close(q.y, 0.0, 1e-6));
        assert!(close(q.z, h, 1e-6), "{q:?}");
    }

    #[test]
    fn quat_from_m3_x_rotation_takes_the_diagonal_branch() {
        // 180° about x: trace == -1, m00 dominates.
        let m = Mat3::from_cols_array([1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, -1.0]);
        let q = quat_from_m3(&m);
        assert!(close(q.w, 0.0, 1e-6), "{q:?}");
        assert!(close(q.x, 1.0, 1e-6), "{q:?}");
        assert!(close(q.y, 0.0, 1e-6));
        assert!(close(q.z, 0.0, 1e-6));
    }

    #[test]
    fn quat_from_m3_round_trips_through_quat_to_m3() {
        for (ax, ay, az) in [
            (0.0f32, 0.0f32, 37.0f32),
            (12.0, 0.0, 0.0),
            (0.0, 61.0, 0.0),
            (33.0, -21.0, 8.0),
            (-152.0, 5.0, 90.0),
        ] {
            let m = euler_m3(Vec3::new(ax, ay, az));
            let q = quat_from_m3(&m);
            let back = quat_to_m3(&q);
            for i in 0..9 {
                assert!(
                    close(m.m[i], back.m[i], 1e-5),
                    "axis {ax},{ay},{az}: {m:?} {back:?}"
                );
            }
        }
    }

    #[test]
    fn quat_norm_degenerate_returns_identity() {
        assert_eq!(quat_norm(Quat::new(0.0, 0.0, 0.0, 0.0)), Quat::IDENTITY);
        assert_eq!(
            quat_norm(Quat::new(f32::NAN, 0.0, 0.0, 0.0)),
            Quat::IDENTITY
        );
        // Just under the 1e-9 cutoff is also identity.
        assert_eq!(quat_norm(Quat::new(5e-10, 0.0, 0.0, 0.0)), Quat::IDENTITY);
        let q = quat_norm(Quat::new(2.0, 0.0, 0.0, 0.0));
        assert!(close(q.w, 1.0, 1e-6));
    }

    #[test]
    fn slerp_endpoints_and_midpoint() {
        let a = Quat::IDENTITY;
        let b = Quat::from_axis_deg(Vec3::new(0.0, 0.0, 1.0), 90.0);
        let at0 = slerp(&a, &b, 0.0);
        assert!(close(at0.w, 1.0, 1e-6), "{at0:?}");
        let at1 = slerp(&a, &b, 1.0);
        assert!(
            close(at1.z, std::f32::consts::FRAC_1_SQRT_2, 1e-6),
            "{at1:?}"
        );
        let mid = slerp(&a, &b, 0.5);
        // Half of 90° is 45°.
        let expect = Quat::from_axis_deg(Vec3::new(0.0, 0.0, 1.0), 45.0);
        assert!(close(mid.w, expect.w, 1e-5), "{mid:?} vs {expect:?}");
        assert!(close(mid.z, expect.z, 1e-5));
    }

    #[test]
    fn slerp_takes_the_short_arc() {
        let a = Quat::from_axis_deg(Vec3::new(0.0, 1.0, 0.0), 10.0);
        // Same rotation, negated: must interpolate the short way, not through 0.
        let b = {
            let q = Quat::from_axis_deg(Vec3::new(0.0, 1.0, 0.0), 20.0);
            Quat::new(-q.w, -q.x, -q.y, -q.z)
        };
        let mid = slerp(&a, &b, 0.5);
        assert!(mid.dot(a) > 0.99, "{mid:?}");
        let expect = Quat::from_axis_deg(Vec3::new(0.0, 1.0, 0.0), 15.0);
        assert!(close(mid.w.abs(), expect.w, 1e-5), "{mid:?} {expect:?}");
    }

    #[test]
    fn slerp_uses_nlerp_above_the_threshold() {
        // Two nearly identical quaternions: dot > 0.9995.
        let a = Quat::from_axis_deg(Vec3::new(1.0, 0.0, 0.0), 10.0);
        let b = Quat::from_axis_deg(Vec3::new(1.0, 0.0, 0.0), 10.5);
        assert!(a.dot(b) > SLERP_DOT_THRESHOLD);
        let mid = slerp(&a, &b, 0.5);
        let expect = Quat::from_axis_deg(Vec3::new(1.0, 0.0, 0.0), 10.25);
        assert!(close(mid.w, expect.w, 1e-4), "{mid:?} {expect:?}");
    }

    #[test]
    fn xf_dir_ignores_translation_and_point_does_not() {
        let x = Xf::new(
            Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2),
            Vec3::new(3.0, 4.0, 5.0),
        );
        let d = x.dir(Vec3::new(1.0, 0.0, 0.0));
        assert!(
            close(d.x, 0.0, 1e-6) && close(d.y, 1.0, 1e-6) && close(d.z, 0.0, 1e-6),
            "{d:?}"
        );
        let p = x.point(Vec3::new(1.0, 0.0, 0.0));
        assert!(
            close(p.x, 3.0, 1e-6) && close(p.y, 5.0, 1e-6) && close(p.z, 5.0, 1e-6),
            "{p:?}"
        );
    }

    #[test]
    fn xf_then_inner_composes_like_function_composition() {
        let a = Xf::new(Mat3::from_rotation_z(0.7), Vec3::new(1.0, 2.0, 3.0));
        let b = Xf::new(Mat3::from_rotation_x(-0.3), Vec3::new(-4.0, 0.5, 2.0));
        let v = Vec3::new(0.25, -1.5, 4.0);
        let chained = a.then_inner(&b).point(v);
        let manual = b.point(a.point(v));
        assert!(close(chained.x, manual.x, 1e-5), "{chained:?} {manual:?}");
        assert!(close(chained.y, manual.y, 1e-5));
        assert!(close(chained.z, manual.z, 1e-5));
        // And identity is neutral.
        let id = Xf::IDENTITY;
        assert_eq!(a.then_inner(&id), a);
        assert_eq!(id.then_inner(&a), a);
    }

    #[test]
    fn vec3_mirror_negates_y_and_z() {
        assert_eq!(
            Vec3::new(1.0, 2.0, 3.0).mirrored(),
            Vec3::new(1.0, -2.0, -3.0)
        );
        // Mirrored twice is the identity (the pose-pair rule).
        let v = Vec3::new(-4.0, 5.0, -6.0);
        assert_eq!(v.mirrored().mirrored(), v);
    }

    #[test]
    fn vec3_normalized_guards_degenerate_input() {
        assert_eq!(Vec3::ZERO.normalized(), Vec3::ZERO);
        assert_eq!(Vec3::new(f32::INFINITY, 0.0, 0.0).normalized(), Vec3::ZERO);
        let n = Vec3::new(3.0, 4.0, 0.0).normalized();
        assert!(close(n.length(), 1.0, 1e-6));
        assert!(close(n.x, 0.6, 1e-6) && close(n.y, 0.8, 1e-6));
    }
}
