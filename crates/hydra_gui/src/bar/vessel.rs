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

/// Paint the vessel bar as its segments.
///
/// `bar::vessel::paint` @`0x1004c7da0` was not reconstructed; the seam records
/// the segments [`running`] yields so tests can assert the stop positions.
pub fn paint(
    painter: &mut dyn crate::seam::Painter,
    area: crate::seam::Rect,
    sizes: &[u64],
    fill: crate::seam::Color32,
) {
    let stops = running(sizes.iter().copied());
    for pair in stops.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        painter.rect_filled(
            crate::seam::Rect {
                min: crate::seam::Vec2::new(area.min.x + area.width() * from, area.min.y),
                max: crate::seam::Vec2::new(area.min.x + area.width() * to, area.max.y),
            },
            0.0,
            fill,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::{RecordingPainter, Rect, Vec2};

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
    fn paint_records_one_rect_per_segment() {
        let mut painter = RecordingPainter::new();
        paint(
            &mut painter,
            Rect::from_min_size(Vec2::ZERO, Vec2::new(100.0, 10.0)),
            &[1, 1],
            crate::seam::Color32::WHITE,
        );
        let rects: Vec<Rect> = painter.rects().map(|(r, _)| *r).collect();
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].min.x, 0.0);
        assert_eq!(rects[0].max.x, 50.0);
        assert_eq!(rects[1].min.x, 50.0);
        assert_eq!(rects[1].max.x, 100.0);
    }
}
