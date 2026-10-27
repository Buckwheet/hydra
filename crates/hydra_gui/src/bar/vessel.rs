//! The vessel bar — the segmented health/spirit gauge.
//!
//! `bar::vessel::running` @`0x1004c7f7c` is fully recovered: it turns a list of
//! segment sizes into cumulative stop positions in `0 ..= 1`.
//!
//! ```text
//! sum = Σ sizes
//! out[0] = 0.0
//! out[i + 1] = (Σ sizes[..=i]) / sum        // 0.0 for every i when sum == 0
//! out[len] = 1.0                            // only when sum != 0
//! ```
//!
//! `bar::vessel::paint` @`0x1004c7da0` was not reconstructed; the painting below
//! is the recovered stop list turned into one `rect_filled` per segment.

/// Cumulative stop positions for `sizes`, one longer than the input.
///
/// Recovered shape of `running` @`0x1004c7f7c`:
///
/// * the sizes are collected into a `Vec` (`from_iter` @`0x1004c7fbc`), summed
///   with a vectorised 64-bit accumulate (`add.2d` loop @`0x1004c8000`, result
///   in `x22`);
/// * a `Vec<f32>` of `len + 1` (capacity `x24 + 1`, `lsl #2` — 4-byte items) is
///   allocated and `out[0] = 0.0` (`str wzr, [x0]` @`0x1004c8074`);
/// * the running total is converted with `ucvtf` and divided by the sum
///   (`ucvtf s0, x24; fdiv s0, s0, s8` @`0x1004c80a8`) and pushed;
/// * when the sum is zero every pushed value is `0.0` instead
///   (`cbz x22` → the `str wzr` loop @`0x1004c80ec`);
/// * finally, only when the sum is non-zero, the last element is overwritten
///   with `1.0` (`mov w9, #0x3f800000` @`0x1004c8144`).
pub fn running<I>(sizes: I) -> Vec<f32>
where
    I: IntoIterator<Item = u64>,
{
    let sizes: Vec<u64> = sizes.into_iter().collect();
    let total: u64 = sizes.iter().copied().sum();
    let mut out = Vec::with_capacity(sizes.len() + 1);
    out.push(0.0);
    if total == 0 {
        out.extend(std::iter::repeat(0.0).take(sizes.len()));
        return out;
    }
    let scale = total as f32;
    let mut running_total: u64 = 0;
    for size in &sizes {
        running_total = running_total.wrapping_add(*size);
        out.push(running_total as f32 / scale);
    }
    if let Some(last) = out.last_mut() {
        *last = 1.0;
    }
    out
}

/// The segments [`running`] yields, as rectangles across `area`.
pub fn segments(area: crate::seam::Rect, sizes: &[u64]) -> Vec<crate::seam::Rect> {
    let stops = running(sizes.iter().copied());
    stops
        .windows(2)
        .map(|pair| {
            crate::seam::rect_from_min_max(
                crate::seam::Vec2::new(area.min.x + area.width() * pair[0], area.min.y),
                crate::seam::Vec2::new(area.min.x + area.width() * pair[1], area.max.y),
            )
        })
        .collect()
}

/// Paint the vessel bar: one filled rectangle per segment.
///
/// `bar::vessel::paint` @`0x1004c7da0` was not reconstructed, so the geometry is
/// the recovered [`running`] stop list and the primitive is the obvious one.
pub fn paint(
    painter: &crate::seam::Painter,
    area: crate::seam::Rect,
    sizes: &[u64],
    fill: crate::seam::Color32,
) {
    for segment in segments(area, sizes) {
        painter.rect_filled(segment, egui::CornerRadius::ZERO, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless;
    use crate::seam::{pos, Color32, Rect, Vec2};

    #[test]
    fn running_returns_cumulative_stops_with_a_trailing_one() {
        let stops = running([10u64, 30, 60]);
        assert_eq!(stops.len(), 4);
        assert_eq!(stops[0], 0.0);
        assert_eq!(stops[1], 0.1);
        assert_eq!(stops[2], 0.4);
        assert_eq!(stops[3], 1.0);
    }

    #[test]
    fn running_with_no_sizes_is_just_the_origin() {
        assert_eq!(running(Vec::<u64>::new()), vec![0.0]);
    }

    #[test]
    fn running_with_a_zero_sum_stays_flat() {
        // The trailing 1.0 is only written when the sum is non-zero
        // (@0x1004c8130: `cbz x22, 0x1004c814c` skips it).
        assert_eq!(running([0u64, 0, 0]), vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn running_matches_hand_computed_fractions() {
        let stops = running([1u64, 1, 2]);
        assert_eq!(stops, vec![0.0, 0.25, 0.5, 1.0]);
    }

    #[test]
    fn segments_split_the_area_by_the_recovered_stops() {
        let area = Rect::from_min_size(pos(Vec2::ZERO), Vec2::new(100.0, 10.0));
        let rects = segments(area, &[1, 1, 2]);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].min.x, 0.0);
        assert_eq!(rects[0].max.x, 25.0);
        assert_eq!(rects[1].min.x, 25.0);
        assert_eq!(rects[1].max.x, 50.0);
        assert_eq!(rects[2].min.x, 50.0);
        assert_eq!(rects[2].max.x, 100.0);
        for rect in &rects {
            assert_eq!(rect.height(), 10.0);
        }
    }

    #[test]
    fn paint_draws_one_rect_per_segment() {
        let area = Rect::from_min_size(pos(Vec2::ZERO), Vec2::new(100.0, 10.0));
        let (_, out) = headless::one_frame(|ui| {
            paint(ui.painter(), area, &[1, 1], Color32::WHITE);
        });
        let rects: Vec<Rect> = headless::rect_fills(&out)
            .iter()
            .map(|rect| rect.rect)
            .collect();
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].min.x, 0.0);
        assert_eq!(rects[0].max.x, 50.0);
        assert_eq!(rects[1].min.x, 50.0);
        assert_eq!(rects[1].max.x, 100.0);
        assert!(headless::rect_fills(&out)
            .iter()
            .all(|rect| rect.fill == Color32::WHITE));
    }
}
