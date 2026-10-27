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
//!   (`fcmgt.2s v0, v11, v8` @`0x10065a884`).

use crate::seam::{Color32, Painter, Rect, Vec2};

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

    /// Paint the ring.
    ///
    /// The original draws egui arc paths; the seam records one circle per known
    /// vital, its radius taken from the recovered inscribed square and the
    /// percentage, so a caller (or a test) can see what was asked for.
    pub fn paint(&self, painter: &mut dyn Painter, rect: Rect, color: Color32) {
        let Some((center, radius)) = Stack::square(rect) else {
            return;
        };
        for (index, vital) in self.vitals().iter().enumerate() {
            let Some(percent) = vital.clamped() else {
                continue;
            };
            // Four concentric rings, outermost first.
            let inset = radius * (index as f32 / 4.0) * 0.5;
            let ring = (radius - inset) * (percent as f32 / 100.0);
            painter.circle_filled(center, ring, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::RecordingPainter;

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
    }

    #[test]
    fn square_is_centred_and_bounded_by_the_short_side() {
        let rect = Rect::from_min_size(Vec2::new(10.0, 20.0), Vec2::new(100.0, 40.0));
        let (center, radius) = Stack::square(rect).unwrap();
        assert_eq!(center, Vec2::new(60.0, 40.0));
        assert_eq!(radius, 20.0);
        let empty = Rect::from_min_size(Vec2::ZERO, Vec2::new(0.0, 40.0));
        assert_eq!(Stack::square(empty), None);
    }

    #[test]
    fn paint_draws_one_ring_per_known_vital() {
        let stack = Stack {
            spirit: Vital::new(100),
            health: Vital::new(50),
            mana: Vital::unknown(),
            stamina: Vital::new(0),
        };
        let mut painter = RecordingPainter::new();
        stack.paint(
            &mut painter,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(40.0, 40.0)),
            Color32::WHITE,
        );
        let drawn: Vec<f32> = painter
            .draws
            .iter()
            .filter_map(|d| match d {
                crate::seam::Draw::Circle { radius, .. } => Some(*radius),
                _ => None,
            })
            .collect();
        assert_eq!(drawn.len(), 3);
        assert_eq!(drawn[0], 20.0); // 100% of the outer radius
        assert_eq!(drawn[1], 8.75); // 50% of 17.5 (radius 20 - 2.5 inset)
        assert_eq!(drawn[2], 0.0); // 0%
    }
}
