//! The vitals stack — the little ring of health/mana/spirit/stamina numbers.
//!
//! Recovered from `hydra_gui::bar::stack`:
//!
//! * `Stack::words` @`0x10065abe4` — four labelled percentages joined with
//!   `", "` (`join_generic_copy` @`0x10065ae3c`, separator `", "` = the 2-byte
//!   literal at `0x10214036a`).
//! * `Stack::paint` @`0x10065a828` — the geometry is an inscribed square:
//!   the centre is `(min + max) * 0.5`, the radius `min(width, height) * 0.5`,
//!   and nothing is drawn when that square is empty
//!   (`fcmgt.2s v0, v11, v8` @`0x10065a884`). Its body looks the style up
//!   (`Context::global_style` @`0x10065a890`) and calls
//!   `hydra_gui::bar::shape::paint` once per vital — four `bl`s to
//!   `0x100524814`, one per record. That helper is not reconstructed, so the
//!   arcs it drew are drawn here instead (see [`Stack::paint`]).

use crate::seam::{pos, Color32, Painter, Rect, Vec2};
use egui::{Pos2, Stroke};

/// One percentage of the stack.
///
/// Layout: a 24-byte record at `0x50`, `0x68`, `0x80` and `0x98` of the original
/// `Stack`, whose first word is compared with `2` (the "unknown" tag — see
/// `Stack::words` @`0x10065ac2c`) and whose word at `+0x10` is the percentage,
/// clamped to 100 (`csel w8, w8, w9(=100), lo` @`0x10065a96c`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vital {
    /// `None` when the recovered "unknown" tag (2) is present.
    pub percent: Option<u32>,
    /// The extra word at `+0x14` (recovered, not interpreted).
    // UNKNOWN: `Stack::paint` @`0x10065a8d0` loads this word and hands it to the
    // style code; its meaning (probably a clock/timer for the ring) is unproven.
    pub clock: u32,
}

impl Vital {
    pub const UNKNOWN_TAG: u32 = 2;

    pub fn new(percent: u32) -> Self {
        Vital {
            percent: Some(percent),
            clock: 0,
        }
    }

    pub fn unknown() -> Self {
        Vital {
            percent: None,
            clock: 0,
        }
    }

    /// The clamped percentage, `0 ..= 100`.
    pub fn clamped(&self) -> Option<u32> {
        self.percent.map(|p| p.min(100))
    }

    /// `format!("{} {}%", label, value)` for a known value, `format!("{} ?",
    /// label)` otherwise — the two format strings recovered at `0x103e2b84d`
    /// and `0x103e2b848`.
    pub fn word(&self, label: &str) -> String {
        match self.clamped() {
            Some(p) => format!("{label} {p}%"),
            None => format!("{label} ?"),
        }
    }
}

/// The four vitals, in the order the recovered code writes them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stack {
    /// Field at `0x50`, label `Sp` (`0x102140362`).
    pub spirit: Vital,
    /// Field at `0x68`, label `HP` (`0x102140364`).
    pub health: Vital,
    /// Field at `0x80`, label `MP` (`0x102140366`).
    pub mana: Vital,
    /// Field at `0x98`, label `SP` (`0x102140368`).
    pub stamina: Vital,
}

impl Stack {
    /// The labels in the recovered order.
    pub const LABELS: [&'static str; 4] = ["Sp", "HP", "MP", "SP"];

    /// The separator: the 2-byte literal `", "` at `0x10214036a`.
    pub const SEPARATOR: &'static str = ", ";

    /// How many segments an arc is drawn with. Fixed so the drawn geometry is
    /// reproducible: an arc of `n` steps has `n + 1` points.
    pub const ARC_STEPS: usize = 24;

    pub fn vitals(&self) -> [Vital; 4] {
        [self.spirit, self.health, self.mana, self.stamina]
    }

    /// `Sp 50%, HP 100%, MP ?, SP 20%` — the four words joined with `", "`.
    pub fn words(&self) -> String {
        self.vitals()
            .iter()
            .zip(Stack::LABELS)
            .map(|(vital, label)| vital.word(label))
            .collect::<Vec<_>>()
            .join(Stack::SEPARATOR)
    }

    /// The square this stack draws inside: centre, then half the shortest side.
    ///
    /// `Stack::paint` @`0x10065a854`: `v2 = (min + max) * 0.5`,
    /// `s0 = min(width, height) * 0.5`, `v8 = v2 - s0`, `v11 = v2 + s0`.
    pub fn square(rect: Rect) -> Option<(Vec2, f32)> {
        let center = Vec2::new(
            (rect.min.x + rect.max.x) * 0.5,
            (rect.min.y + rect.max.y) * 0.5,
        );
        let radius = rect.width().min(rect.height()) * 0.5;
        if radius > 0.0 {
            Some((center, radius))
        } else {
            None
        }
    }

    /// The radius ring `index` is drawn at inside a square of `radius`.
    ///
    /// `Stack::paint` hands each vital to `bar::shape::paint` in turn; the four
    /// rings are inset by half a ring each
    /// (`inset = radius * (index / 4) * 0.5`), so they nest.
    pub fn ring_at(radius: f32, index: usize) -> f32 {
        let inset = radius * (index as f32 / 4.0) * 0.5;
        radius - inset
    }

    /// The points of the arc that covers `fraction` of the ring, starting at the
    /// top (12 o'clock) and running clockwise.
    pub fn arc(center: Vec2, radius: f32, fraction: f32) -> Vec<Pos2> {
        let steps = Stack::ARC_STEPS;
        let swept = fraction.clamp(0.0, 1.0) * std::f32::consts::TAU;
        (0..=steps)
            .map(|step| {
                let angle = -std::f32::consts::FRAC_PI_2 + swept * (step as f32 / steps as f32);
                pos(Vec2::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                ))
            })
            .collect()
    }

    /// Paint the ring.
    ///
    /// The recovered body draws one arc per vital through
    /// `bar::shape::paint`; that helper is not reconstructed, so each vital is
    /// painted here as its track plus an arc for its percentage, and its
    /// [`Vital::word`] — `Sp 50%`, `HP 100%`, `MP ?` for the unknown tag — is
    /// written to the right of the rings.
    pub fn paint(&self, painter: &Painter, rect: Rect, color: Color32) {
        let Some((center, radius)) = Stack::square(rect) else {
            return;
        };
        let font = crate::bar::body_font();
        let stroke = Stroke::new(2.0, color);
        let track = Stroke::new(2.0, color.gamma_multiply(0.25));
        let row = font.size + 4.0;
        for (index, vital) in self.vitals().iter().enumerate() {
            let ring = Stack::ring_at(radius, index);
            painter.circle_stroke(pos(center), ring, track);
            if let Some(percent) = vital.clamped() {
                let points = Stack::arc(center, ring, percent as f32 / 100.0);
                painter.add(egui::Shape::line(points, stroke));
            }
            painter.text(
                pos(Vec2::new(
                    center.x + radius + 4.0,
                    center.y - radius + row * index as f32,
                )),
                egui::Align2::LEFT_TOP,
                vital.word(Stack::LABELS[index]),
                font.clone(),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless;

    #[test]
    fn words_use_the_recovered_labels_and_separator() {
        let stack = Stack {
            spirit: Vital::new(50),
            health: Vital::new(100),
            mana: Vital::unknown(),
            stamina: Vital::new(20),
        };
        assert_eq!(stack.words(), "Sp 50%, HP 100%, MP ?, SP 20%");
        assert_eq!(Stack::LABELS, ["Sp", "HP", "MP", "SP"]);
        assert_eq!(Stack::SEPARATOR, ", ");
    }

    #[test]
    fn percentages_clamp_to_one_hundred() {
        assert_eq!(Vital::new(250).clamped(), Some(100));
        assert_eq!(Vital::new(0).clamped(), Some(0));
        assert_eq!(Vital::unknown().clamped(), None);
        assert_eq!(Vital::UNKNOWN_TAG, 2);
        assert_eq!(Vital::unknown().word("MP"), "MP ?");
        assert_eq!(Vital::new(42).word("HP"), "HP 42%");
    }

    #[test]
    fn square_is_centred_and_bounded_by_the_short_side() {
        let rect = Rect::from_min_size(pos(Vec2::new(10.0, 20.0)), Vec2::new(100.0, 40.0));
        let (center, radius) = Stack::square(rect).unwrap();
        assert_eq!(center, Vec2::new(60.0, 40.0));
        assert_eq!(radius, 20.0);
        let empty = Rect::from_min_size(pos(Vec2::ZERO), Vec2::new(0.0, 40.0));
        assert_eq!(Stack::square(empty), None);
    }

    #[test]
    fn the_rings_nest_inside_the_inscribed_square() {
        // radius 20: 20.0, 17.5, 15.0, 12.5.
        assert_eq!(Stack::ring_at(20.0, 0), 20.0);
        assert_eq!(Stack::ring_at(20.0, 1), 17.5);
        assert_eq!(Stack::ring_at(20.0, 2), 15.0);
        assert_eq!(Stack::ring_at(20.0, 3), 12.5);
    }

    #[test]
    fn an_arc_starts_at_the_top_and_sweeps_clockwise() {
        let points = Stack::arc(Vec2::new(0.0, 0.0), 10.0, 1.0);
        assert_eq!(points.len(), Stack::ARC_STEPS + 1);
        // 12 o'clock.
        assert!((points[0].x - 0.0).abs() < 1e-5);
        assert!((points[0].y + 10.0).abs() < 1e-5);
        // Quarter way round: 3 o'clock, to the right.
        let quarter = &points[Stack::ARC_STEPS / 4];
        assert!((quarter.x - 10.0).abs() < 1e-4);
        assert!(quarter.y.abs() < 1e-4);
        // Half a turn: 6 o'clock.
        let half = &points[Stack::ARC_STEPS / 2];
        assert!(half.x.abs() < 1e-4);
        assert!((half.y - 10.0).abs() < 1e-4);
        // A zero fraction collapses to a single point at the top.
        let none = Stack::arc(Vec2::ZERO, 10.0, 0.0);
        assert!(none.iter().all(|p| p.x.abs() < 1e-5 && (p.y + 10.0).abs() < 1e-5));
    }

    #[test]
    fn paint_draws_a_track_and_an_arc_per_known_vital() {
        let stack = Stack {
            spirit: Vital::new(100),
            health: Vital::new(50),
            mana: Vital::unknown(),
            stamina: Vital::new(0),
        };
        let (_, out) = headless::one_frame(|ui| {
            stack.paint(
                ui.painter(),
                Rect::from_min_size(pos(Vec2::ZERO), Vec2::new(40.0, 40.0)),
                Color32::WHITE,
            );
        });
        // Four tracks, at the four nesting radii, all centred on the square.
        let circles = headless::circles(&out);
        assert_eq!(circles.len(), 4);
        let radii: Vec<f32> = circles.iter().map(|c| c.radius).collect();
        assert_eq!(radii, vec![20.0, 17.5, 15.0, 12.5]);
        for circle in &circles {
            assert_eq!(circle.center, Pos2::new(20.0, 20.0));
            assert_eq!(circle.stroke.width, 2.0);
            // A quarter-strength white track: (255 * 0.25 + 0.5) = 64.
            assert_eq!(
                circle.stroke.color,
                Color32::from_rgba_premultiplied(64, 64, 64, 64)
            );
            assert_eq!(circle.fill, Color32::TRANSPARENT);
        }
        // Three arcs: the unknown vital has no percentage to sweep.
        let arcs = headless::shapes(&out)
            .into_iter()
            .filter_map(|shape| match shape {
                egui::Shape::Path(path) => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(arcs.len(), 3);
        assert_eq!(arcs[0].points.len(), Stack::ARC_STEPS + 1);
        // 100 % closes the ring: first and last point coincide.
        assert!((arcs[0].points[0].x - arcs[0].points[Stack::ARC_STEPS].x).abs() < 1e-3);
        // 50 % stops at 6 o'clock (centre + radius).
        let half = &arcs[1].points[Stack::ARC_STEPS];
        assert!((half.y - (20.0 + 17.5)).abs() < 1e-3);
        // 0 % is a dot at the top of its ring — the innermost one, radius 12.5.
        let dot = &arcs[2].points[0];
        assert!((dot.x - 20.0).abs() < 1e-3);
        assert!((dot.y - (20.0 - 12.5)).abs() < 1e-3);
        // The four labels, `?` for the unknown tag.
        assert_eq!(
            headless::texts(&out),
            vec![
                "Sp 100%".to_string(),
                "HP 50%".to_string(),
                "MP ?".to_string(),
                "SP 0%".to_string()
            ]
        );
    }

    #[test]
    fn paint_draws_nothing_in_an_empty_square() {
        let stack = Stack::default();
        let (_, out) = headless::one_frame(|ui| {
            stack.paint(
                ui.painter(),
                Rect::from_min_size(pos(Vec2::ZERO), Vec2::new(0.0, 40.0)),
                Color32::WHITE,
            );
        });
        assert!(headless::shapes(&out).is_empty());
    }
}
