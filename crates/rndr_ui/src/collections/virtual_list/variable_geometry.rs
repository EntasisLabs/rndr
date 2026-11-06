//! Integer geometry for variable-height lists.
//!
//! The scroll identity is an item plus a milli-point remainder. Prefix sums
//! stay in `i64`. `Scroll::offset` is only the f32 paint projection of that
//! identity. One logical point is [`MILLI_PER_POINT`] milli-points.

use crate::collections::ItemId;

/// Milli-points in one logical point.
pub(super) const MILLI_PER_POINT: i64 = 1000;

/// Inputs for one variable window decision. The caller owns world mutation.
pub(super) struct WindowQuery {
    /// Current paint offset in points, positive downward (`-Scroll::offset[1]`).
    pub paint_points: f32,
    /// Viewport height in points. Non-positive realizes no rows.
    pub viewport_points: f32,
    /// When set, keep the integer anchor instead of reading `paint_points`.
    pub follow_anchor: bool,
    /// Paint offset last written from the anchor. A different paint value is input.
    pub projected: f32,
    /// Index of the anchored item, when the list has one.
    pub anchor_index: Option<usize>,
    /// Milli-points into the anchored item.
    pub remainder_mp: i64,
    /// Focused row to reveal after a viewport-height change. `None` leaves the anchor.
    pub resize_focus_index: Option<usize>,
}

/// Pure result of [`decide_window`]. Indexes refer to the same height slice.
pub(super) struct WindowDecision {
    /// Anchored item, or `None` when the list is empty.
    pub anchor_index: Option<usize>,
    /// Milli-points into [`Self::anchor_index`], or 0 when there is no anchor.
    pub remainder_mp: i64,
    /// First realized row.
    pub first: usize,
    /// Number of realized rows, including a partial edge.
    pub len: usize,
    /// Paint offset in points, positive downward.
    pub offset_points: f32,
    /// Spacer above the first realized row, in points.
    pub top: f32,
    /// Spacer below the last realized row, in points.
    pub bottom: f32,
}

/// Convert a strictly positive finite height into milli-points.
/// A value that rounds below 1 milli-point is refused.
pub(super) fn height_to_milli(points: f32) -> Option<i64> {
    if !points.is_finite() || points <= 0.0 {
        return None;
    }
    let milli = f64::from(points) * MILLI_PER_POINT as f64;
    if !milli.is_finite() {
        return None;
    }
    let rounded = milli.round();
    if rounded < 1.0 || rounded > i64::MAX as f64 {
        return None;
    }
    Some(rounded as i64)
}

/// Validate every supplied height and reject a prefix that does not fit in `i64`.
pub(super) fn heights_in_milli(points: &[f32]) -> Option<Vec<i64>> {
    let mut heights = Vec::with_capacity(points.len());
    let mut total = 0i64;
    for point in points {
        let milli = height_to_milli(*point)?;
        total = total.checked_add(milli)?;
        heights.push(milli);
    }
    Some(heights)
}

/// Exclusive prefix sums. `prefix[0] == 0` and `prefix[len] == total`.
pub(super) fn prefix_sums(heights: &[i64]) -> Vec<i64> {
    let mut prefix = Vec::with_capacity(heights.len() + 1);
    let mut sum = 0i64;
    prefix.push(0);
    for height in heights {
        sum = sum.saturating_add(*height);
        prefix.push(sum);
    }
    prefix
}

/// Paint projection of an integer milli-point position. The `f32` step is lossy.
pub(super) fn mp_to_points(mp: i64) -> f32 {
    (mp as f64 / MILLI_PER_POINT as f64) as f32
}

/// Inverse of [`mp_to_points`] for input that moved the paint offset.
pub(super) fn points_to_mp(points: f32) -> i64 {
    if !points.is_finite() {
        return 0;
    }
    let milli = f64::from(points) * MILLI_PER_POINT as f64;
    if !milli.is_finite() {
        return 0;
    }
    milli.round().clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

/// Absolute milli-point gap between an integer position and its paint projection.
pub(super) fn paint_error_mp(position_mp: i64, paint_points: f32) -> f64 {
    let projected = f64::from(paint_points) * MILLI_PER_POINT as f64;
    (projected - position_mp as f64).abs()
}

/// Largest item with `prefix[index] <= position`. Remainder is `position - prefix[index]`.
pub(super) fn locate(prefix: &[i64], position_mp: i64) -> Option<(usize, i64)> {
    let count = prefix.len().saturating_sub(1);
    if count == 0 {
        return None;
    }
    let total = prefix[count];
    let position = position_mp.clamp(0, total);
    let mut lower = 0usize;
    let mut upper = count;
    while lower + 1 < upper {
        let mid = lower + (upper - lower) / 2;
        if prefix[mid] <= position {
            lower = mid;
        } else {
            upper = mid;
        }
    }
    Some((lower, position - prefix[lower]))
}

/// Rows overlapping the viewport, always including the anchor row.
pub(super) fn visible_extent(
    prefix: &[i64],
    anchor_index: usize,
    remainder_mp: i64,
    viewport_mp: i64,
) -> (usize, usize) {
    let count = prefix.len().saturating_sub(1);
    if count == 0 || viewport_mp <= 0 || anchor_index >= count {
        return (0, 0);
    }
    let start = prefix[anchor_index].saturating_add(remainder_mp.max(0));
    let end = start.saturating_add(viewport_mp);
    let mut last = anchor_index + 1;
    while last < count && prefix[last] < end {
        last += 1;
    }
    (anchor_index, last - anchor_index)
}

/// Move `target` fully into the viewport when a reveal or a height change asks.
/// An already visible target keeps the current anchor and remainder.
pub(super) fn ensure_visible(
    prefix: &[i64],
    target: usize,
    anchor_index: usize,
    remainder_mp: i64,
    viewport_mp: i64,
) -> (usize, i64) {
    let count = prefix.len().saturating_sub(1);
    if count == 0 || target >= count || anchor_index >= count {
        return (
            anchor_index.min(count.saturating_sub(1)),
            remainder_mp.max(0),
        );
    }
    let top = prefix[anchor_index].saturating_add(remainder_mp.max(0));
    let start = prefix[target];
    let end = prefix[target + 1];
    let new_top = if start < top {
        start
    } else if viewport_mp > 0 && end > top.saturating_add(viewport_mp) {
        end.saturating_sub(viewport_mp).max(0)
    } else {
        return (anchor_index, remainder_mp.max(0));
    };
    locate(prefix, new_top).unwrap_or((target, 0))
}

/// How many logical rows a PageUp/PageDown step covers from `index`.
/// Equal heights match `floor(viewport / stride)`, and the step is at least 1.
pub(super) fn page_span(prefix: &[i64], index: usize, viewport_mp: i64, forward: bool) -> usize {
    let count = prefix.len().saturating_sub(1);
    if count == 0 || index >= count || viewport_mp <= 0 {
        return 1;
    }
    if forward {
        let start = prefix[index];
        let mut cursor = index;
        while cursor + 1 < count && prefix[cursor + 1] - start <= viewport_mp {
            cursor += 1;
        }
        (cursor - index).max(1)
    } else {
        let start = prefix[index];
        let mut cursor = index;
        while cursor > 0 && start - prefix[cursor - 1] <= viewport_mp {
            cursor -= 1;
        }
        (index - cursor).max(1)
    }
}

/// Keep a still-present id. Otherwise the next surviving id, then the previous,
/// then the first row of a non-empty replacement, then a clear anchor.
///
/// A removed id's remainder does not transfer: the repaired row starts at 0.
/// A surviving id keeps its remainder, clamped to the new height.
pub(super) fn repair_anchor(
    old_order: &[ItemId],
    old_anchor: Option<ItemId>,
    old_remainder: i64,
    new_ids: &[ItemId],
    new_heights: &[i64],
) -> (Option<ItemId>, i64) {
    debug_assert_eq!(new_ids.len(), new_heights.len());
    if new_ids.is_empty() {
        return (None, 0);
    }
    let Some(id) = old_anchor else {
        return (Some(new_ids[0]), 0);
    };
    if let Some(index) = new_ids.iter().position(|item| *item == id) {
        return (Some(id), old_remainder.clamp(0, new_heights[index]));
    }
    if let Some(old_index) = old_order.iter().position(|item| *item == id) {
        if let Some(next) = old_order[old_index + 1..]
            .iter()
            .find(|item| new_ids.contains(item))
        {
            return (Some(*next), 0);
        }
        if let Some(previous) = old_order[..old_index]
            .iter()
            .rev()
            .find(|item| new_ids.contains(item))
        {
            return (Some(*previous), 0);
        }
    }
    (Some(new_ids[0]), 0)
}

/// Resolve the anchor, the realized range, and the paint spacers.
pub(super) fn decide_window(heights: &[i64], prefix: &[i64], query: WindowQuery) -> WindowDecision {
    debug_assert_eq!(prefix.len(), heights.len() + 1);
    let count = heights.len();
    if count == 0 {
        return WindowDecision {
            anchor_index: None,
            remainder_mp: 0,
            first: 0,
            len: 0,
            offset_points: 0.0,
            top: 0.0,
            bottom: 0.0,
        };
    }
    let viewport_points = if query.viewport_points.is_finite() {
        query.viewport_points.max(0.0)
    } else {
        0.0
    };
    let viewport_mp = points_to_mp(viewport_points).max(0);
    let mut index = query.anchor_index.filter(|index| *index < count);
    let mut remainder = query.remainder_mp.max(0);
    let paint = if query.paint_points.is_finite() {
        query.paint_points
    } else {
        query.projected
    };
    if !query.follow_anchor && paint != query.projected {
        if let Some((found, found_remainder)) = locate(prefix, points_to_mp(paint)) {
            index = Some(found);
            remainder = found_remainder;
        }
    }
    if let Some(focus) = query.resize_focus_index.filter(|index| *index < count) {
        let (anchor_index, anchor_remainder) = match index {
            Some(current) => (current, remainder.min(heights[current])),
            None => (0, 0),
        };
        let (found, found_remainder) =
            ensure_visible(prefix, focus, anchor_index, anchor_remainder, viewport_mp);
        index = Some(found);
        remainder = found_remainder;
    }
    let mut index = index.unwrap_or(0);
    remainder = remainder.clamp(0, heights[index]);
    let mut position = prefix[index] + remainder;
    let total = prefix[count];
    let max_position = total.saturating_sub(viewport_mp).max(0);
    // A caller-owned anchor is allowed to sit past a stale viewport. The next
    // realization, after layout has the real height, clamps through paint input.
    if !query.follow_anchor && position > max_position {
        if let Some((found, found_remainder)) = locate(prefix, max_position) {
            index = found;
            remainder = found_remainder;
            position = prefix[index] + remainder;
        }
    }
    let (first, len) = if viewport_mp <= 0 {
        (0, 0)
    } else {
        visible_extent(prefix, index, remainder, viewport_mp)
    };
    let (top, bottom) = spacers(prefix, heights, first, len, position, viewport_points);
    WindowDecision {
        anchor_index: Some(index),
        remainder_mp: remainder,
        first,
        len,
        offset_points: mp_to_points(position),
        top,
        bottom,
    }
}

/// Top and bottom spacers. The bottom grows when f32 rounding would let
/// `Scroll::clamp_to` pull the paint offset off the integer anchor.
fn spacers(
    prefix: &[i64],
    heights: &[i64],
    first: usize,
    len: usize,
    position_mp: i64,
    viewport_points: f32,
) -> (f32, f32) {
    let count = heights.len();
    let total = prefix.last().copied().unwrap_or(0);
    let first = first.min(count);
    let end = (first + len).min(count);
    let top = mp_to_points(prefix.get(first).copied().unwrap_or(0));
    let mut rows = 0.0f32;
    for height in &heights[first..end] {
        rows += mp_to_points(*height);
    }
    let after = total - prefix.get(end).copied().unwrap_or(total);
    let mut bottom = mp_to_points(after.max(0));
    let offset = mp_to_points(position_mp.max(0));
    let content = top + rows + bottom;
    let required = offset + viewport_points.max(0.0);
    if content < required {
        // One milli-point of slack covers the f32 sum landing an ulp short.
        bottom += required - content + mp_to_points(1);
    }
    (top, bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(heights: &[i64]) -> (Vec<i64>, Vec<i64>) {
        (heights.to_vec(), prefix_sums(heights))
    }

    #[test]
    fn height_conversion_refuses_non_positive_and_non_finite() {
        assert_eq!(height_to_milli(8.0), Some(8_000));
        assert_eq!(height_to_milli(0.001), Some(1));
        assert_eq!(height_to_milli(0.0004), None);
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(height_to_milli(bad), None, "{bad}");
        }
        assert!(heights_in_milli(&[8.0, 24.0, 96.0]).is_some());
        assert!(heights_in_milli(&[8.0, 0.0]).is_none());
    }

    #[test]
    fn repair_follows_id_then_next_then_previous_then_empty() {
        let old = [ItemId(1), ItemId(2), ItemId(3), ItemId(4)];
        let (id, remainder) = repair_anchor(
            &old,
            Some(ItemId(2)),
            2_500,
            &[ItemId(4), ItemId(1), ItemId(2), ItemId(3)],
            &[10_000, 10_000, 4_000, 10_000],
        );
        assert_eq!((id, remainder), (Some(ItemId(2)), 2_500));

        let (id, remainder) = repair_anchor(
            &old,
            Some(ItemId(2)),
            9_000,
            &[ItemId(1), ItemId(2)],
            &[10_000, 4_000],
        );
        assert_eq!((id, remainder), (Some(ItemId(2)), 4_000));

        let (id, remainder) = repair_anchor(
            &old,
            Some(ItemId(2)),
            100,
            &[ItemId(1), ItemId(3), ItemId(4)],
            &[10_000, 10_000, 10_000],
        );
        assert_eq!((id, remainder), (Some(ItemId(3)), 0));

        let (id, remainder) = repair_anchor(
            &old,
            Some(ItemId(4)),
            100,
            &[ItemId(1), ItemId(2)],
            &[10_000, 10_000],
        );
        assert_eq!((id, remainder), (Some(ItemId(2)), 0));

        assert_eq!(
            repair_anchor(&old, Some(ItemId(2)), 100, &[], &[]),
            (None, 0)
        );
        assert_eq!(
            repair_anchor(&old, Some(ItemId(2)), 100, &[ItemId(9)], &[10_000]),
            (Some(ItemId(9)), 0)
        );
    }

    #[test]
    fn locate_and_visible_range_include_partial_edges() {
        let (_, prefix) = sample(&[10_000, 20_000, 5_000]);
        assert_eq!(locate(&prefix, 0), Some((0, 0)));
        assert_eq!(locate(&prefix, 10_000), Some((1, 0)));
        assert_eq!(locate(&prefix, 15_000), Some((1, 5_000)));
        assert_eq!(locate(&prefix, 35_000), Some((2, 5_000)));
        assert_eq!(visible_extent(&prefix, 1, 5_000, 20_000), (1, 2));
        assert_eq!(visible_extent(&prefix, 0, 0, 10_000), (0, 1));
    }

    #[test]
    fn shrink_keeps_the_id_and_input_follows_paint() {
        let (heights, prefix) = sample(&[10_000, 40_000, 10_000]);
        let held = decide_window(
            &heights,
            &prefix,
            WindowQuery {
                paint_points: 0.0,
                viewport_points: 10.0,
                follow_anchor: true,
                projected: 0.0,
                anchor_index: Some(1),
                remainder_mp: 40_000,
                resize_focus_index: None,
            },
        );
        assert_eq!(held.anchor_index, Some(1));
        assert_eq!(held.remainder_mp, 40_000);

        let scrolled = decide_window(
            &heights,
            &prefix,
            WindowQuery {
                paint_points: 25.0,
                viewport_points: 20.0,
                follow_anchor: false,
                projected: 0.0,
                anchor_index: Some(0),
                remainder_mp: 0,
                resize_focus_index: None,
            },
        );
        assert_eq!(scrolled.anchor_index, Some(1));
        assert_eq!(scrolled.remainder_mp, 15_000);
    }

    #[test]
    fn caller_owned_anchor_is_not_clamped_until_paint_input() {
        let (heights, prefix) = sample(&[10_000, 10_000, 10_000]);
        let held = decide_window(
            &heights,
            &prefix,
            WindowQuery {
                paint_points: 0.0,
                viewport_points: 20.0,
                follow_anchor: true,
                projected: 0.0,
                anchor_index: Some(2),
                remainder_mp: 0,
                resize_focus_index: None,
            },
        );
        assert_eq!(held.anchor_index, Some(2));
        let clamped = decide_window(
            &heights,
            &prefix,
            WindowQuery {
                paint_points: held.offset_points,
                viewport_points: 20.0,
                follow_anchor: false,
                projected: held.offset_points,
                anchor_index: held.anchor_index,
                remainder_mp: held.remainder_mp,
                resize_focus_index: None,
            },
        );
        assert_eq!(clamped.anchor_index, Some(1));
        assert_eq!(clamped.remainder_mp, 0);
    }

    #[test]
    fn equal_stride_page_matches_floor_division() {
        let heights = vec![32_000i64; 100];
        let prefix = prefix_sums(&heights);
        assert_eq!(page_span(&prefix, 20, 320_000, true), 10);
        assert_eq!(page_span(&prefix, 20, 320_000, false), 10);
        assert_eq!(page_span(&prefix, 0, 320_000, false), 1);
    }

    #[test]
    fn large_milli_position_projects_with_a_bounded_f32_error() {
        let position = 250_000_333i64;
        let paint = mp_to_points(position);
        let error = paint_error_mp(position, paint);
        assert!(error > 0.0, "conversion error was {error}");
        assert!(error < 1_000.0, "conversion error was {error}");
    }
}
