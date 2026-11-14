//! Point edits on an automation curve's points, as the curve lane's gestures make them
//! (`docs/architecture/user-interface.md` "Layout", ADR-0126). Each edit returns the new points,
//! or `None` when they would not make a valid [`Curve`], so a gesture that would break the curve
//! is refused and the shape stays as it was.

use modulation::{Curve, Handle, Point};

/// Which of a point's two handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The handle shaping the segment that ends at the point.
    In,
    /// The handle shaping the segment that starts at the point.
    Out,
}

/// The fraction of the gap to a neighbour that a new point's handle reaches, and that a
/// neighbour's facing handle is clamped to when a new point would otherwise break the curve.
const HANDLE_REACH: f64 = 1.0 / 3.0;

/// `points` if they make a valid curve.
fn valid(points: Vec<Point>) -> Option<Vec<Point>> {
    Curve::validate(&points).is_ok().then_some(points)
}

/// `handle` scaled towards its anchor, keeping its direction, so it reaches at most `reach`
/// frames from it.
fn clamp_reach(handle: Handle, reach: f64) -> Handle {
    let along = handle.d_frame.abs();
    if along <= reach || along == 0.0 {
        return handle;
    }
    let k = reach / along;
    Handle::new(handle.d_frame * k, (f64::from(handle.d_value) * k) as f32)
}

/// Clamps the handles facing each other across the segment from point `i` to point `i + 1` to
/// a third of its gap each, which always gives a valid segment between valid points.
fn clamp_segment(points: &mut [Point], i: usize) {
    let (Some(start), Some(end)) = (points.get(i), points.get(i + 1)) else {
        return;
    };
    let reach = (end.frame - start.frame) as f64 * HANDLE_REACH;
    points[i].out_handle = clamp_reach(points[i].out_handle, reach);
    points[i + 1].in_handle = clamp_reach(points[i + 1].in_handle, reach);
}

/// `points` with a new point at `frame` and `value` (clamped to 0–1) inserted in frame order,
/// and the new point's index. Its handles are horizontal and reach a third of the gap to each
/// neighbour, or lie on the anchor where it has none. If the neighbours' facing handles would
/// then break the curve, they are clamped to a third of their new gaps. `None` if a point is
/// already at `frame`.
pub fn add(points: &[Point], frame: i64, value: f32) -> Option<(Vec<Point>, usize)> {
    let index = points.partition_point(|p| p.frame < frame);
    if points.get(index).is_some_and(|p| p.frame == frame) {
        return None;
    }
    let gap = |a: i64, b: i64| (b - a) as f64 * HANDLE_REACH;
    let in_handle = match index.checked_sub(1).and_then(|i| points.get(i)) {
        Some(before) => Handle::new(-gap(before.frame, frame), 0.0),
        None => Handle::default(),
    };
    let out_handle = match points.get(index) {
        Some(after) => Handle::new(gap(frame, after.frame), 0.0),
        None => Handle::default(),
    };
    let point = Point::new(frame, value.clamp(0.0, 1.0)).with_handles(in_handle, out_handle);
    let mut next = points.to_vec();
    next.insert(index, point);
    if Curve::validate(&next).is_err() {
        if index > 0 {
            clamp_segment(&mut next, index - 1);
        }
        clamp_segment(&mut next, index);
    }
    valid(next).map(|next| (next, index))
}

/// `points` with point `i` moved to `frame` (rounded to a whole frame) and `value` (clamped to
/// 0–1), its handles moving with it. `None` if `i` is out of range or the move breaks the curve,
/// such as by passing a neighbour.
pub fn move_anchor(points: &[Point], i: usize, frame: f64, value: f32) -> Option<Vec<Point>> {
    if i >= points.len() || !frame.is_finite() {
        return None;
    }
    let mut next = points.to_vec();
    next[i].frame = frame.round() as i64;
    next[i].value = value.clamp(0.0, 1.0);
    valid(next)
}

/// `points` with point `i`'s `side` handle set to offset (`d_frame`, `d_value`). When `linked`,
/// the opposite handle turns to point the opposite way and keeps its length on screen, where
/// `scale` is the lane's pixels per frame and per unit of value; opposite directions survive
/// scaling each axis, so the two handles look collinear on screen. A handle with nothing on its
/// side (the first point's in-handle, the last point's out-handle) is never linked to. `None` if
/// `i` is out of range or the result breaks the curve, such as a handle pointing backwards in
/// time.
pub fn move_handle(
    points: &[Point],
    i: usize,
    side: Side,
    d_frame: f64,
    d_value: f32,
    linked: bool,
    scale: (f64, f64),
) -> Option<Vec<Point>> {
    if i >= points.len() {
        return None;
    }
    let mut next = points.to_vec();
    let moved = Handle::new(d_frame, d_value);
    let opposite_used = match side {
        Side::In => i + 1 < points.len(),
        Side::Out => i > 0,
    };
    let point = &mut next[i];
    let (handle, opposite) = match side {
        Side::In => (&mut point.in_handle, &mut point.out_handle),
        Side::Out => (&mut point.out_handle, &mut point.in_handle),
    };
    *handle = moved;
    if linked && opposite_used {
        let (sx, sy) = scale;
        let length = |h: Handle| (h.d_frame * sx).hypot(f64::from(h.d_value) * sy);
        let (moved_length, kept_length) = (length(moved), length(*opposite));
        if moved_length > 0.0 && kept_length > 0.0 {
            let k = kept_length / moved_length;
            *opposite = Handle::new(-d_frame * k, (-f64::from(d_value) * k) as f32);
        }
    }
    valid(next)
}

/// `points` without point `i`. If the handles that now face each other would break the curve,
/// they are clamped to a third of the new gap. `None` if `i` is out of range.
pub fn remove(points: &[Point], i: usize) -> Option<Vec<Point>> {
    if i >= points.len() {
        return None;
    }
    let mut next = points.to_vec();
    next.remove(i);
    if Curve::validate(&next).is_err() && i > 0 {
        clamp_segment(&mut next, i - 1);
    }
    valid(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Vec<Point> {
        let first = Point::new(0, 0.0).with_handles(Handle::default(), Handle::new(100.0, 0.0));
        let last = Point::new(300, 1.0).with_handles(Handle::new(-100.0, 0.0), Handle::default());
        vec![first, last]
    }

    #[test]
    fn test_add_gives_horizontal_handles_a_third_of_each_gap()
    -> Result<(), Box<dyn std::error::Error>> {
        let (points, index) = add(&line(), 150, 0.25).ok_or("refused")?;
        assert_eq!(index, 1);
        assert_eq!(points[1].frame, 150);
        assert_eq!(points[1].in_handle, Handle::new(-50.0, 0.0));
        assert_eq!(points[1].out_handle, Handle::new(50.0, 0.0));
        // Neighbours whose handles still fit are left alone.
        assert_eq!(points[0].out_handle, Handle::new(100.0, 0.0));
        Ok(())
    }

    #[test]
    fn test_add_at_the_ends_has_no_outer_handle() -> Result<(), Box<dyn std::error::Error>> {
        let (points, index) = add(&line(), 600, 2.0).ok_or("refused")?;
        assert_eq!(index, 2);
        assert_eq!(points[2].value, 1.0);
        assert_eq!(points[2].in_handle, Handle::new(-100.0, 0.0));
        assert_eq!(points[2].out_handle, Handle::default());
        let (points, index) = add(&[], 10, 0.5).ok_or("refused")?;
        assert_eq!(index, 0);
        assert_eq!(points[0], Point::new(10, 0.5));
        Ok(())
    }

    #[test]
    fn test_add_clamps_neighbours_that_would_overlap() -> Result<(), Box<dyn std::error::Error>> {
        // The first point's out-handle reaches 100 frames; a point at 30 leaves it a gap of 30.
        let (points, _) = add(&line(), 30, 0.5).ok_or("refused")?;
        assert_eq!(points[0].out_handle, Handle::new(10.0, 0.0));
        assert!(Curve::validate(&points).is_ok());
        Ok(())
    }

    #[test]
    fn test_add_on_an_existing_frame_is_refused() {
        assert_eq!(add(&line(), 300, 0.5), None);
    }

    #[test]
    fn test_move_anchor_rounds_clamps_and_carries_its_handles()
    -> Result<(), Box<dyn std::error::Error>> {
        let points = move_anchor(&line(), 1, 250.4, 1.5).ok_or("refused")?;
        assert_eq!(points[1].frame, 250);
        assert_eq!(points[1].value, 1.0);
        assert_eq!(points[1].in_handle, Handle::new(-100.0, 0.0));
        Ok(())
    }

    #[test]
    fn test_move_anchor_past_its_neighbour_is_refused() {
        assert_eq!(move_anchor(&line(), 1, -10.0, 0.5), None);
        assert_eq!(move_anchor(&line(), 0, 300.0, 0.5), None);
        assert_eq!(move_anchor(&line(), 5, 0.0, 0.5), None);
    }

    #[test]
    fn test_move_handle_backwards_is_refused() {
        assert_eq!(
            move_handle(&line(), 0, Side::Out, -5.0, 0.0, false, (1.0, 1.0)),
            None
        );
    }

    #[test]
    fn test_move_handle_linked_stays_opposite_on_screen() -> Result<(), Box<dyn std::error::Error>>
    {
        let (points, i) = add(&line(), 150, 0.5).ok_or("refused")?;
        // 0.1 px per frame and 100 px per unit of value: the in-handle is 5 px long on screen.
        let scale = (0.1, 100.0);
        let points = move_handle(&points, i, Side::Out, 30.0, 0.4, true, scale).ok_or("refused")?;
        let (out, inn) = (points[i].out_handle, points[i].in_handle);
        assert_eq!(out, Handle::new(30.0, 0.4));
        let screen = |h: Handle| (h.d_frame * scale.0, f64::from(h.d_value) * scale.1);
        let (ox, oy) = screen(out);
        let (ix, iy) = screen(inn);
        // Opposite directions: the cross product is zero and the dot product negative.
        assert!((ox * iy - oy * ix).abs() < 1e-6);
        assert!(ox * ix + oy * iy < 0.0);
        assert!((ix.hypot(iy) - 5.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_move_handle_unlinked_leaves_the_other_alone() -> Result<(), Box<dyn std::error::Error>>
    {
        let (points, i) = add(&line(), 150, 0.5).ok_or("refused")?;
        let moved =
            move_handle(&points, i, Side::Out, 30.0, 0.4, false, (0.1, 100.0)).ok_or("refused")?;
        assert_eq!(moved[i].in_handle, points[i].in_handle);
        Ok(())
    }

    #[test]
    fn test_remove_drops_the_point_and_clamps_overlaps() -> Result<(), Box<dyn std::error::Error>> {
        let (points, i) = add(&line(), 150, 0.5).ok_or("refused")?;
        assert_eq!(remove(&points, i).ok_or("refused")?, line());
        // A long out-handle that only fitted against a short in-handle overlaps across the
        // joined gap, so the facing handles are clamped to a third of it.
        let overlapping = vec![
            Point::new(0, 0.0).with_handles(Handle::default(), Handle::new(120.0, 0.0)),
            Point::new(100, 0.5).with_handles(Handle::new(-30.0, 0.0), Handle::new(0.25, 0.0)),
            Point::new(101, 1.0).with_handles(Handle::new(-0.25, 0.0), Handle::default()),
        ];
        assert!(Curve::validate(&overlapping).is_ok());
        let joined = remove(&overlapping, 1).ok_or("refused")?;
        assert!((joined[0].out_handle.d_frame - 101.0 / 3.0).abs() < 1e-9);
        assert_eq!(joined[1].in_handle, Handle::new(-0.25, 0.0));
        assert_eq!(remove(&line(), 2), None);
        Ok(())
    }
}
