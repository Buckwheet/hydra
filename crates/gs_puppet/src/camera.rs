//! The puppet camera: a yaw/pitch projector onto a 2-D screen rectangle.
//!
//! Fully recovered from `gs_puppet::camera::Camera::project` (`@0x100a03bf0`,
//! 296 bytes — the 92 KB that follows is an unrelated inlined neighbour).  The
//! field offsets are the ones the body reads:
//!
//! | offset | field |
//! |---|---|
//! | `+0x00` | `centre_x` (screen pixels) |
//! | `+0x04` | `centre_y` |
//! | `+0x08` | `yaw`, **degrees** (multiplied by `0x3c8efa35`) |
//! | `+0x0c` | `pitch`, degrees |
//! | `+0x10` | `height` — the depth of the camera plane along the view axis |
//! | `+0x14` | `focal` — pixels per unit at depth 1 |
//! | `+0x18` | `eye` (`Vec3`) |
//!
//! The projection itself:
//!
//! ```text
//! d   = p - eye
//! x1  =  d.x·cos(yaw) + d.z·sin(yaw)
//! z1  =  d.z·cos(yaw) − d.x·sin(yaw)          ; the sin/cos pair comes from one
//! y1  =  d.y·cos(pitch) − z1·sin(pitch)       ; __sincosf_stret per angle, and
//! up  =  d.y·sin(pitch) + z1·cos(pitch)       ; all d.y terms through the
//! depth = max(height − up, 0.05)              ; yaw rotation are multiplied by 0
//! scale = focal / depth
//! screen = (centre_x + x1·scale, centre_y − y1·scale)
//! ```
//!
//! `0x3d4ccccd` is the `0.05` near-plane clamp (`fmaxnm`), so the returned depth
//! is never zero and never negative.

use crate::math::{Vec3, DEG_TO_RAD};

/// Projection camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Screen centre in pixels, `+0x00`.
    pub centre_x: f32,
    /// Screen centre in pixels, `+0x04`.
    pub centre_y: f32,
    /// Yaw about the world y axis, in degrees, `+0x08`.
    pub yaw: f32,
    /// Pitch about the camera x axis, in degrees, `+0x0c`.
    pub pitch: f32,
    /// Depth of the camera plane along the view axis, `+0x10`.
    pub height: f32,
    /// Pixels per unit at depth 1, `+0x14`.
    pub focal: f32,
    /// Camera position in model space, `+0x18`.
    pub eye: Vec3,
}

impl Default for Camera {
    fn default() -> Camera {
        Camera::new(0.0, 0.0, 0.0, 0.0, 8.0, 500.0)
    }
}

impl Camera {
    /// Depth clamp applied before the perspective divide (`0x3d4ccccd`).
    pub const NEAR: f32 = 0.05;

    pub fn new(
        centre_x: f32,
        centre_y: f32,
        yaw: f32,
        pitch: f32,
        height: f32,
        focal: f32,
    ) -> Camera {
        Camera {
            centre_x,
            centre_y,
            yaw,
            pitch,
            height,
            focal,
            eye: Vec3::ZERO,
        }
    }

    pub fn with_eye(mut self, eye: Vec3) -> Camera {
        self.eye = eye;
        self
    }

    /// Project a model-space point to `(screen_x, screen_y, depth)`.
    ///
    /// `@0x100a03bf0` — `Camera::project(&self, &Vec3) -> (f32, f32, f32)`.
    pub fn project(&self, p: &Vec3) -> (f32, f32, f32) {
        let d = *p - self.eye;
        let (sin_yaw, cos_yaw) = (self.yaw * DEG_TO_RAD).sin_cos();
        let x1 = d.x * cos_yaw + d.z * sin_yaw;
        let z1 = d.z * cos_yaw - d.x * sin_yaw;
        let (sin_pitch, cos_pitch) = (self.pitch * DEG_TO_RAD).sin_cos();
        let y1 = d.y * cos_pitch - z1 * sin_pitch;
        let up = d.y * sin_pitch + z1 * cos_pitch;
        let depth = (self.height - up).max(Self::NEAR);
        let scale = self.focal / depth;
        (
            self.centre_x + x1 * scale,
            self.centre_y - y1 * scale,
            depth,
        )
    }

    /// A closure form of [`Camera::project`] — the recovered binary has a
    /// `gs_puppet::camera::Camera::projector::{{closure}}` symbol, i.e. this
    /// function hands a capturing closure to the draw loop.
    pub fn projector(&self) -> impl Fn(&Vec3) -> (f32, f32, f32) + '_ {
        move |p: &Vec3| self.project(p)
    }

    /// `true` when a projected depth is in front of the near plane.
    pub fn in_front(&self, depth: f32) -> bool {
        depth > Self::NEAR
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_clamp_is_the_recovered_constant() {
        assert_eq!(Camera::NEAR.to_bits(), 0x3d4c_cccd);
        assert!((Camera::NEAR - 0.05).abs() < 1e-9);
    }

    #[test]
    fn a_point_on_the_axis_lands_on_the_screen_centre() {
        let cam = Camera::new(320.0, 240.0, 0.0, 0.0, 8.0, 500.0);
        let (x, y, depth) = cam.project(&Vec3::new(0.0, 0.0, 0.0));
        assert_eq!((x, y, depth), (320.0, 240.0, 8.0));
    }

    #[test]
    fn depth_is_height_minus_the_up_component() {
        let cam = Camera::new(0.0, 0.0, 0.0, 0.0, 8.0, 500.0);
        // yaw = pitch = 0 -> up = d.z, so depth = height - z.
        let (_, _, depth) = cam.project(&Vec3::new(0.0, 0.0, 2.0));
        assert!((depth - 6.0).abs() < 1e-6, "{depth}");
        // Behind the camera the depth clamps to the near plane instead of going
        // negative.
        let (_, _, depth) = cam.project(&Vec3::new(0.0, 0.0, 100.0));
        assert_eq!(depth, Camera::NEAR);
    }

    #[test]
    fn scale_is_focal_over_depth() {
        let cam = Camera::new(0.0, 0.0, 0.0, 0.0, 10.0, 500.0);
        // depth = 10 - 0 = 10 -> scale 50; x = 1 unit off axis.
        let (x, _, depth) = cam.project(&Vec3::new(1.0, 0.0, 0.0));
        assert!((depth - 10.0).abs() < 1e-6);
        assert!((x - 50.0).abs() < 1e-4, "{x}");
    }

    #[test]
    fn screen_y_grows_downwards() {
        let cam = Camera::new(0.0, 0.0, 0.0, 0.0, 10.0, 500.0);
        // A point above the camera (y > 0) draws *higher* on screen, i.e. with a
        // smaller y pixel value.
        let (_, y_up, _) = cam.project(&Vec3::new(0.0, 1.0, 0.0));
        let (_, y_down, _) = cam.project(&Vec3::new(0.0, -1.0, 0.0));
        assert!(y_up < 0.0 && y_down > 0.0, "{y_up} {y_down}");
        assert!((y_up + y_down).abs() < 1e-4);
    }

    #[test]
    fn yaw_rotates_the_view() {
        // A camera yawed 90° sees +x as its forward axis.
        let cam = Camera::new(0.0, 0.0, 90.0, 0.0, 10.0, 100.0);
        let (x, _, depth) = cam.project(&Vec3::new(0.0, 0.0, -1.0));
        // x1 = d.z*sin(90) = -1 -> x = -1 * (100/(10 - (-(-1)*cos90=0))) = -10
        assert!((x + 10.0).abs() < 1e-4, "{x} {depth}");
        // The eye offset shifts the origin.
        let cam = cam.with_eye(Vec3::new(1.0, 0.0, 0.0));
        let (x, _, _) = cam.project(&Vec3::new(1.0, 0.0, 0.0));
        assert!(x.abs() < 1e-4, "{x}");
    }

    #[test]
    fn pitch_tilts_the_up_axis() {
        // Looking 90° down: world y becomes depth.
        let cam = Camera::new(0.0, 0.0, 0.0, 90.0, 10.0, 100.0);
        let (_, _, depth) = cam.project(&Vec3::new(0.0, 2.0, 0.0));
        // up = d.y*sin(90) = 2 -> depth = 8
        assert!((depth - 8.0).abs() < 1e-5, "{depth}");
    }

    #[test]
    fn projector_closure_matches_project() {
        let cam = Camera::new(320.0, 240.0, 12.0, -7.0, 9.0, 480.0).with_eye(Vec3::new(0.5, 1.5, 2.5));
        let p = Vec3::new(-1.0, 0.25, 3.0);
        let proj = cam.projector();
        assert_eq!(proj(&p), cam.project(&p));
    }
}
