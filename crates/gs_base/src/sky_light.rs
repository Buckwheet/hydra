//! Sky lighting: moon phases, arcs and the day/night share curve.
//!
//! Recovered from `gs_base::sky_light`:
//! * `moon_light`     @0x100a893d0 (tail-calls `moon_light_of` @0x100a89538)
//! * `lornon_light`   @0x100a893dc (tail-calls `moon_light_of`)
//! * `moon_sky_arc`   @0x100a893e4
//! * `night_share_in` @0x100a89710
//!
//! plus the inlined helper the original imported from the vendored
//! `vellum_light` crate, `vellum_light::phase_keyframes` @0x100ac3dcc, which is
//! reproduced here as [`phase_keyframes`] (**std departure**: the original
//! depended on an external crate).
//!
//! ## `phase_keyframes`
//!
//! `times` is `[dawn_start, dawn_end, dusk_start, dusk_end]` — four hours.
//! With `mid1 = (t0+t1)/2`, `mid2 = (t2+t3)/2` and `h = hour mod 24`, the result
//! is a two-entry blend (or a single full-weight entry):
//!
//! | range | keyframes |
//! |---|---|
//! | `t0 <= h < mid1` | `[{3, 1-s}, {0, s}]`, `s = (h-t0)/(mid1-t0)` |
//! | `mid1 <= h < t1` | `[{0, 1-s}, {1, s}]`, `s = (h-mid1)/(t1-mid1)` |
//! | `t1 <= h < t2`   | `[{1, 1.0}]` |
//! | `t2 <= h < mid2` | `[{1, 1-s}, {2, s}]`, `s = (h-t2)/(mid2-t2)` |
//! | `mid2 <= h < t3` | `[{2, 1-s}, {3, s}]`, `s = (h-mid2)/(t3-mid2)` |
//! | otherwise        | `[{3, 1.0}]` |
//!
//! The `tag` byte indexes [`KEYFRAME_WEIGHTS`] = `[0.5, 0.0, 0.5, 1.0]`
//! (table at 0x1021f0560), so the curve is: full night (1.0) outside the
//! twilight window, 0.5 at half-dawn, 0.0 all day, 0.5 at half-dusk.
//!
//! ## `moon_light_of`
//!
//! ```text
//! t = ((hour - (transit - 6)) mod 24) / 12        // 0..2, up while t <= 1
//! if t not in [0, 1]                     -> 0
//! raw = 55 * sin(pi * t); if raw <= 0    -> 0
//! f   = min(raw / 12, 1)
//! w   = sum(keyframe.value * KEYFRAME_WEIGHTS[keyframe.tag])
//! dim = 1 - 0.9 * clamp(cloud, 0, 1)
//! f*f * (3 - 2*f) * clamp(phase, 0, 1) * w * dim
//! ```
//!
//! `f*f*(3-2f)` is `smoothstep(f)`, i.e. the light fades smoothly as the moon
//! sinks.

use std::f32::consts::PI;

use crate::sky::Sky;

/// Stand-in for `vellum_light::phase_keyframes`.
///
/// **std departure.**  Returns a `Vec` like the original (heap allocated, built
/// with `alloc(8, 4)` / `alloc(16, 4)`), but the tags and weights are the
/// recovered ones.
pub fn phase_keyframes(times: &[f32; 4], hour: f32) -> Vec<Keyframe> {
    let h = hour.rem_euclid(24.0);
    let (t0, t1, t2, t3) = (times[0], times[1], times[2], times[3]);
    let mid1 = (t0 + t1) * 0.5;

    if t0 <= h && h < mid1 {
        let s = (h - t0) / (mid1 - t0);
        return vec![
            Keyframe {
                tag: 3,
                value: 1.0 - s,
            },
            Keyframe { tag: 0, value: s },
        ];
    }
    if mid1 <= h && h < t1 {
        let s = (h - mid1) / (t1 - mid1);
        return vec![
            Keyframe {
                tag: 0,
                value: 1.0 - s,
            },
            Keyframe { tag: 1, value: s },
        ];
    }
    if t1 <= h && h < t2 {
        return vec![Keyframe { tag: 1, value: 1.0 }];
    }
    let mid2 = (t2 + t3) * 0.5;
    if t2 <= h && h < mid2 {
        let s = (h - t2) / (mid2 - t2);
        return vec![
            Keyframe {
                tag: 1,
                value: 1.0 - s,
            },
            Keyframe { tag: 2, value: s },
        ];
    }
    if mid2 <= h && h < t3 {
        let s = (h - mid2) / (t3 - mid2);
        return vec![
            Keyframe {
                tag: 2,
                value: 1.0 - s,
            },
            Keyframe { tag: 3, value: s },
        ];
    }
    vec![Keyframe { tag: 3, value: 1.0 }]
}

/// One blend keyframe: an 8-byte `{ tag: u8, value: f32 }` pair, matching the
/// element layout the binary writes (`strb` at +0, `str s` at +4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe {
    /// Index into [`KEYFRAME_WEIGHTS`].
    pub tag: u8,
    /// Blend weight, `0.0 ..= 1.0`.
    pub value: f32,
}

/// Weight of each keyframe tag.  Table @0x1021f0560 = `{0.5, 0.0, 0.5, 1.0}`.
pub static KEYFRAME_WEIGHTS: [f32; 4] = [0.5, 0.0, 0.5, 1.0];

/// The flat `[6, 8, 18, 20]` dawn/dusk table at 0x1021b45e0 — used when
/// `Sky::x10 != 1`.
pub static FLAT_DAY: [f32; 4] = [6.0, 8.0, 18.0, 20.0];

/// Default dawn/dusk window hours (alias of [`FLAT_DAY`] for readability).
pub static DAY_WINDOWS: [f32; 4] = FLAT_DAY;

/// How much of the sky is "night" at `hour`, given the four key hours.
///
/// `@0x100a89710` — sums `value * KEYFRAME_WEIGHTS[tag]` over
/// [`phase_keyframes`].
pub fn night_share_in(times: &[f32; 4], hour: f32) -> f32 {
    let mut acc = 0.0f32;
    for kf in phase_keyframes(times, hour) {
        acc += kf.value * KEYFRAME_WEIGHTS[(kf.tag as usize) & 3];
    }
    acc
}

/// Illuminated fraction for a moon phase in `[0, 1)`.
///
/// `(1 - cos(2*pi*phase)) / 2` — 0 at new, 1 at full.  The disassembly folds
/// this with `0x40c90fdb` (`2*pi` as `f32`).
pub fn illum_from_phase(phase: f32) -> f32 {
    (1.0 - (2.0 * PI * phase).cos()) * 0.5
}

/// Light from the *fast* moon.
///
/// `@0x100a893d0` — `moon_light(&self, hour)` loads `self[0x1c]` (illumination)
/// and `self[0x24]` (transit hour) and tail-calls `moon_light_of`.
pub fn moon_light(sky: &Sky, hour: f32) -> f32 {
    moon_light_of(
        sky.kind,
        sky.x10,
        [sky.dawn, sky.dusk],
        sky.cloud,
        hour,
        sky.fast_moon_phase,
        sky.fast_moon_hour,
    )
}

/// Light from the *slow* (Lornon) moon.
///
/// `@0x100a893dc` — `lornon_light(&self, hour)` loads the `f32` pair at
/// `self[0x28]` (illumination) and `self[0x2c]` (transit hour).
pub fn lornon_light(sky: &Sky, hour: f32) -> f32 {
    moon_light_of(
        sky.kind,
        sky.x10,
        [sky.dawn, sky.dusk],
        sky.cloud,
        hour,
        sky.slow_moon_phase,
        sky.slow_moon_hour,
    )
}

/// Shared moon-light body.  `@0x100a89538`.
///
/// * `kind` is read from the sky state and compared with `1`, but both arms of
///   that test converge, so it does not affect the result.  Kept for fidelity.
/// * `window_kind` is `Sky::x10`: `1` selects the seasonal
///   `[dawn-1, dawn-1, dusk-1, dusk-1]` window, anything else the flat
///   [`FLAT_DAY`] table.
pub fn moon_light_of(
    kind: u32,
    window_kind: u32,
    dawn_dusk: [f32; 2],
    cloud: f32,
    hour: f32,
    phase: f32,
    transit: f32,
) -> f32 {
    let _ = kind;
    let t = (hour - (transit - 6.0)).rem_euclid(24.0) / 12.0;
    if !(0.0..=1.0).contains(&t) {
        return 0.0;
    }
    let raw = 55.0 * (PI * t).sin();
    if raw <= 0.0 {
        return 0.0;
    }
    let f = (raw / 12.0).min(1.0);
    let phase = phase.clamp(0.0, 1.0);
    let windows = if window_kind == 1 {
        let a = dawn_dusk[0] - 1.0;
        let b = dawn_dusk[1] - 1.0;
        [a, a, b, b]
    } else {
        FLAT_DAY
    };
    let mut acc = 0.0f32;
    for kf in phase_keyframes(&windows, hour) {
        acc += kf.value * KEYFRAME_WEIGHTS[(kf.tag as usize) & 3];
    }
    let base = f * f * (3.0 - 2.0 * f) * phase;
    let dim = 1.0 - 0.9 * cloud.clamp(0.0, 1.0);
    base * acc * dim
}

/// A moon's position on the sky dome: `(angle_deg, elevation_deg)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoonArc {
    /// Azimuth-ish angle in `(-180, 180]`.
    pub angle: f32,
    /// Elevation in degrees; `55 * sin(pi * t)` peaks at 55° at transit.
    pub elevation: f32,
}

/// Where a moon sits at `hour`.
///
/// `@0x100a893e4` — `moon_sky_arc(kind: u32, hour: f32, transit: f32, offset: f32)`
/// returns `Option<MoonArc>` (a 12-byte `{tag, angle, elevation}`).
///
/// ```text
/// t = ((hour - (transit - 6)) mod 24) / 12        // 0..2
/// if t not in [0, 1]                              -> None
/// elevation = 55 * sin(pi * t); if <= 0           -> None
/// angle = match kind {
///     1 => wrap180(offset - (t*180 + 90) + 180),
///     _ => (2*t - 1) * 70,
/// }
/// ```
///
/// `wrap180` reduces modulo 360 into `(-180, 180]`.
pub fn moon_sky_arc(kind: u32, hour: f32, transit: f32, offset: f32) -> Option<MoonArc> {
    let t = (hour - (transit - 6.0)).rem_euclid(24.0) / 12.0;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }
    let elevation = 55.0 * (PI * t).sin();
    if elevation <= 0.0 {
        return None;
    }
    let angle = if kind == 1 {
        let mut a = offset - (t * 180.0 + 90.0) + 180.0;
        a = a.rem_euclid(360.0);
        if a > 180.0 {
            a -= 360.0;
        }
        a
    } else {
        (t + t - 1.0) * 70.0
    };
    Some(MoonArc { angle, elevation })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::GameSky;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn keyframe_windows() {
        let w = FLAT_DAY; // dawn 6..8 (mid 7), dusk 18..20 (mid 19)
        assert_eq!(
            phase_keyframes(&w, 5.0),
            vec![Keyframe { tag: 3, value: 1.0 }]
        );
        assert_eq!(
            phase_keyframes(&w, 6.0),
            vec![
                Keyframe {
                    tag: 3,
                    value: 1.0
                },
                Keyframe { tag: 0, value: 0.0 }
            ]
        );
        assert_eq!(
            phase_keyframes(&w, 7.0),
            vec![
                Keyframe { tag: 0, value: 1.0 },
                Keyframe { tag: 1, value: 0.0 }
            ]
        );
        assert_eq!(
            phase_keyframes(&w, 12.0),
            vec![Keyframe { tag: 1, value: 1.0 }]
        );
        assert_eq!(
            phase_keyframes(&w, 19.0),
            vec![
                Keyframe { tag: 2, value: 1.0 },
                Keyframe { tag: 3, value: 0.0 }
            ]
        );
        assert_eq!(
            phase_keyframes(&w, 22.0),
            vec![Keyframe { tag: 3, value: 1.0 }]
        );
        // Wraps: 25.5 == 1.5 -> full night.
        assert_eq!(
            phase_keyframes(&w, 25.5),
            vec![Keyframe { tag: 3, value: 1.0 }]
        );
    }

    #[test]
    fn night_share_values() {
        let w = FLAT_DAY;
        assert_eq!(night_share_in(&w, 0.0), 1.0);
        assert_eq!(night_share_in(&w, 6.5), 0.75); // tags 3 (0.5) + 0 (0.5)
        assert_eq!(night_share_in(&w, 7.5), 0.25); // tags 0 (0.5) -> half
        assert_eq!(night_share_in(&w, 12.0), 0.0);
        assert_eq!(night_share_in(&w, 18.5), 0.25);
        assert_eq!(night_share_in(&w, 19.5), 0.75);
        assert_eq!(night_share_in(&w, 23.0), 1.0);
    }

    #[test]
    fn illumination_curve() {
        assert!(close(illum_from_phase(0.0), 0.0));
        assert!(close(illum_from_phase(0.25), 0.5));
        assert!(close(illum_from_phase(0.5), 1.0));
        assert!(close(illum_from_phase(0.75), 0.5));
    }

    #[test]
    fn moon_arc_shape() {
        // Fast-moon branch: angle is (2t-1)*70, elevation 55 sin(pi t).
        // At the very start of the arc the elevation is exactly 0 -> None.
        assert!(moon_sky_arc(0, 6.0, 12.0, 0.0).is_none());
        let start = moon_sky_arc(0, 7.0, 12.0, 0.0).unwrap(); // t = 1/12
        assert!(close(start.angle, -70.0 + 2.0 * 70.0 / 12.0));
        assert!(close(start.elevation, 55.0 * (std::f32::consts::PI / 12.0).sin()));
        let mid = moon_sky_arc(0, 12.0, 12.0, 0.0).unwrap();
        assert!(close(mid.angle, 0.0));
        assert!(close(mid.elevation, 55.0));
        assert!(moon_sky_arc(0, 18.0, 12.0, 0.0).is_none()); // t == 1 -> elevation 0
        assert!(moon_sky_arc(0, 2.0, 12.0, 0.0).is_none()); // down
        // kind == 1 branch: wrap180(offset - (t*180 + 90) + 180)
        // offset - (0.5*180 + 90) + 180 == 0
        let a = moon_sky_arc(1, 12.0, 12.0, 0.0).unwrap();
        assert!(close(a.angle, 0.0));
        let b = moon_sky_arc(1, 7.0, 12.0, 0.0).unwrap();
        assert!(close(b.angle, 75.0)); // 0 - (15 + 90) + 180
    }

    #[test]
    fn moon_light_is_zero_when_down() {
        let gs = GameSky::default();
        let sky = gs.sky(1, 1, 0.0, 0.0); // both transit hours are 1.0
        // Up window is [transit-6, transit+6] = [-5, 7] i.e. 19..7.
        assert_eq!(moon_light(&sky, 12.0), 0.0);
        assert_eq!(lornon_light(&sky, 12.0), 0.0);
        assert!(moon_light(&sky, 1.0) > 0.0);
        assert!(lornon_light(&sky, 3.0) > 0.0);
    }

    #[test]
    fn moon_light_at_transit_is_the_peak() {
        let gs = GameSky::default();
        let sky = gs.sky(1, 1, 0.0, 0.0);
        // t = 0.5 -> f = min(55/12, 1) = 1 -> smoothstep(1) = 1.
        // At t = 0 the phase is 0.5, so the stored illumination is 1.0 (full);
        // the night share at hour 1 is 1.0.
        let peak = moon_light(&sky, 1.0);
        assert!(close(peak, 1.0), "peak={peak}");
        // Further along the arc (t = 0.25): 55*sin(pi/4)/12 = 3.24 -> clamped to 1.
        assert!(close(moon_light(&sky, 4.0), 1.0));
    }

    #[test]
    fn cloud_dims_the_moon() {
        let mut sky = GameSky::default().sky(1, 1, 0.0, 0.0);
        assert!(close(moon_light(&sky, 1.0), 1.0));
        sky.cloud = 1.0;
        // dim = 1 - 0.9 = 0.1
        assert!(close(moon_light(&sky, 1.0), 0.1));
    }
}
