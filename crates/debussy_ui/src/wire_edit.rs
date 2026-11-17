//! A patch-board wire's geometry and the edits to its shape: the cubic segments it is drawn
//! as, the grips a selected wire shows, and what dragging, adding or deleting one does
//! (`docs/architecture/patch-board.md` "Patch board", ADR-0134).
//!
//! A wire runs from its output pin through the connection's points to its input pin. The
//! second wire of a whole-port stereo connection passes through the same points moved by its
//! `shift`, so the pair runs parallel.

use iced::{Point, Vector};
use project_model::{Position, WireHandle, WirePoint, WireShape};

/// A cubic bezier segment: start, its control point, the end's control point, end.
pub type Segment = [Point; 4];

/// Something on a selected wire that can be caught and dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    /// The handle at the output end.
    OutHandle,
    /// The handle at the input end.
    InHandle,
    /// Point `i` (from the output end).
    Point(usize),
    /// Point `i`'s handle towards the output end.
    PointIn(usize),
    /// Point `i`'s handle towards the input end.
    PointOut(usize),
}

impl Grip {
    /// The point it is or belongs to, for a point or one of its handles.
    pub fn point(self) -> Option<usize> {
        match self {
            Grip::Point(i) | Grip::PointIn(i) | Grip::PointOut(i) => Some(i),
            Grip::OutHandle | Grip::InHandle => None,
        }
    }
}

/// The automatic control points between a wire end or point at `a` and the next at `b`: level
/// with each, half their horizontal distance out, and at least 40 pixels.
pub fn automatic(a: Point, b: Point) -> (Point, Point) {
    let reach = ((b.x - a.x).abs() / 2.0).max(40.0);
    (Point::new(a.x + reach, a.y), Point::new(b.x - reach, b.y))
}

fn plus(at: Point, handle: WireHandle) -> Point {
    Point::new(at.x + handle.dx, at.y + handle.dy)
}

fn handle(from: Point, to: Point) -> WireHandle {
    WireHandle {
        dx: to.x - from.x,
        dy: to.y - from.y,
    }
}

fn place(point: &WirePoint, shift: Vector) -> Point {
    Point::new(point.at.x, point.at.y) + shift
}

/// The segments of a wire from `a` to `b` shaped by `shape`, its points moved by `shift`.
pub fn segments(a: Point, b: Point, shape: &WireShape, shift: Vector) -> Vec<Segment> {
    let knots: Vec<Point> = std::iter::once(a)
        .chain(shape.points.iter().map(|p| place(p, shift)))
        .chain(std::iter::once(b))
        .collect();
    let last = knots.len() - 2;
    knots
        .windows(2)
        .enumerate()
        .map(|(k, pair)| {
            let (start, end) = (pair[0], pair[1]);
            let (auto1, auto2) = automatic(start, end);
            let c1 = if k == 0 {
                shape.out_handle.map_or(auto1, |h| plus(a, h))
            } else {
                plus(start, shape.points[k - 1].out_handle)
            };
            let c2 = if k == last {
                shape.in_handle.map_or(auto2, |h| plus(b, h))
            } else {
                plus(end, shape.points[k].in_handle)
            };
            [start, c1, c2, end]
        })
        .collect()
}

/// The point `t` (0 to 1) along `segment`.
pub fn along(segment: &Segment, t: f32) -> Point {
    let [a, c1, c2, b] = *segment;
    let u = 1.0 - t;
    let mix = |p: f32, q: f32, r: f32, s: f32| {
        u * u * u * p + 3.0 * u * u * t * q + 3.0 * u * t * t * r + t * t * t * s
    };
    Point::new(mix(a.x, c1.x, c2.x, b.x), mix(a.y, c1.y, c2.y, b.y))
}

/// Samples taken along each segment to find the nearest place on a wire.
const SAMPLES: usize = 32;

/// The segment and parameter of the place on `segments` nearest `at`, if within `reach`.
pub fn nearest(segments: &[Segment], at: Point, reach: f32) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32, f32)> = None;
    for (k, segment) in segments.iter().enumerate() {
        for i in 0..=SAMPLES {
            let t = i as f32 / SAMPLES as f32;
            let distance = along(segment, t).distance(at);
            if distance <= reach && best.is_none_or(|(_, _, d)| distance < d) {
                best = Some((k, t, distance));
            }
        }
    }
    best.map(|(k, t, _)| (k, t))
}

/// The grips a selected wire shows, each with where it is and the point its stem starts from
/// (`None` for a point, which has no stem): both end handles, every point, and the handles of
/// point `open`.
pub fn grips(segments: &[Segment], open: Option<usize>) -> Vec<(Grip, Point, Option<Point>)> {
    let (Some(first), Some(last)) = (segments.first(), segments.last()) else {
        return Vec::new();
    };
    let mut grips = vec![
        (Grip::OutHandle, first[1], Some(first[0])),
        (Grip::InHandle, last[2], Some(last[3])),
    ];
    for (i, pair) in segments.windows(2).enumerate() {
        let at = pair[0][3];
        if open == Some(i) {
            grips.push((Grip::PointIn(i), pair[0][2], Some(at)));
            grips.push((Grip::PointOut(i), pair[1][1], Some(at)));
        }
        grips.push((Grip::Point(i), at, None));
    }
    grips
}

/// `shape` with a point added at `t` along segment `k` of the wire from `a` to `b` (its points
/// moved by `shift`), splitting that segment so the wire keeps its course, and the new point's
/// index. `None` if `k` is not a segment.
pub fn split(
    a: Point,
    b: Point,
    shape: &WireShape,
    shift: Vector,
    k: usize,
    t: f32,
) -> Option<(WireShape, usize)> {
    let segments = segments(a, b, shape, shift);
    let [p0, p1, p2, p3] = *segments.get(k)?;
    let lerp = |p: Point, q: Point| p + (q - p) * t;
    let (q0, q1, q2) = (lerp(p0, p1), lerp(p1, p2), lerp(p2, p3));
    let (r0, r1) = (lerp(q0, q1), lerp(q1, q2));
    let m = lerp(r0, r1);
    let mut shape = shape.clone();
    if k == 0 {
        shape.out_handle = Some(handle(a, q0));
    } else {
        shape.points[k - 1].out_handle = handle(p0, q0);
    }
    if k == segments.len() - 1 {
        shape.in_handle = Some(handle(b, q2));
    } else {
        shape.points[k].in_handle = handle(p3, q2);
    }
    let at = m - shift;
    shape.points.insert(
        k,
        WirePoint {
            at: Position { x: at.x, y: at.y },
            in_handle: handle(m, r0),
            out_handle: handle(m, r1),
        },
    );
    Some((shape, k))
}

/// A point's other handle after one was dragged to `moved`: pointing the opposite way at its
/// own length.
fn opposite(moved: WireHandle, other: WireHandle) -> WireHandle {
    let length = moved.dx.hypot(moved.dy);
    if length == 0.0 {
        return other;
    }
    let scale = -other.dx.hypot(other.dy) / length;
    WireHandle {
        dx: moved.dx * scale,
        dy: moved.dy * scale,
    }
}

/// `shape` with `grip` of the wire from `a` to `b` (its points moved by `shift`) dragged to
/// `to`. Dragging a point's handle keeps its other handle pointing the opposite way, unless
/// `alt`. `None` if `grip` names a point the shape does not have.
pub fn drag(
    shape: &WireShape,
    grip: Grip,
    to: Point,
    (a, b): (Point, Point),
    shift: Vector,
    alt: bool,
) -> Option<WireShape> {
    let mut shape = shape.clone();
    match grip {
        Grip::OutHandle => shape.out_handle = Some(handle(a, to)),
        Grip::InHandle => shape.in_handle = Some(handle(b, to)),
        Grip::Point(i) => {
            let at = to - shift;
            shape.points.get_mut(i)?.at = Position { x: at.x, y: at.y };
        }
        Grip::PointIn(i) | Grip::PointOut(i) => {
            let point = shape.points.get_mut(i)?;
            let moved = handle(place(point, shift), to);
            let (mine, other) = if matches!(grip, Grip::PointIn(_)) {
                (&mut point.in_handle, &mut point.out_handle)
            } else {
                (&mut point.out_handle, &mut point.in_handle)
            };
            *mine = moved;
            if !alt {
                *other = opposite(moved, *other);
            }
        }
    }
    Some(shape)
}

/// `shape` with `grip` deleted: an end handle goes back to automatic, and a point, or a point
/// whose handle is selected, is removed. `None` if `grip` names a point the shape does not have.
pub fn remove(shape: &WireShape, grip: Grip) -> Option<WireShape> {
    let mut shape = shape.clone();
    match grip {
        Grip::OutHandle => shape.out_handle = None,
        Grip::InHandle => shape.in_handle = None,
        Grip::Point(i) | Grip::PointIn(i) | Grip::PointOut(i) => {
            if i >= shape.points.len() {
                return None;
            }
            shape.points.remove(i);
        }
    }
    Some(shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    const A: Point = Point::new(0.0, 100.0);
    const B: Point = Point::new(400.0, 300.0);

    fn close(p: Point, q: Point) -> bool {
        p.distance(q) < 1e-3
    }

    #[test]
    fn test_segments_unshaped_is_one_automatic_curve() {
        let segments = segments(A, B, &WireShape::default(), Vector::ZERO);
        let (c1, c2) = automatic(A, B);
        assert_eq!(segments, vec![[A, c1, c2, B]]);
    }

    #[test]
    fn test_split_keeps_the_course_and_adds_a_point() -> Result {
        let shape = WireShape::default();
        let shift = Vector::new(0.0, 48.0);
        let before = segments(A, B, &shape, shift);
        let (split_shape, index) = split(A, B, &shape, shift, 0, 0.25).ok_or("no split")?;
        assert_eq!(index, 0);
        assert_eq!(split_shape.points.len(), 1);
        let after = segments(A, B, &split_shape, shift);
        assert_eq!(after.len(), 2);
        // The new point is where the old curve was, and the halves trace the old curve.
        assert!(close(after[0][3], along(&before[0], 0.25)));
        assert!(close(along(&after[0], 0.5), along(&before[0], 0.125)));
        assert!(close(along(&after[1], 0.5), along(&before[0], 0.625)));
        // Saved without the stereo shift.
        assert!(close(
            Point::new(split_shape.points[0].at.x, split_shape.points[0].at.y) + shift,
            after[0][3]
        ));
        // Splitting again between the ends and the point.
        let (again, index) = split(A, B, &split_shape, shift, 1, 0.5).ok_or("no split")?;
        assert_eq!((index, again.points.len()), (1, 2));
        assert!(split(A, B, &split_shape, shift, 2, 0.5).is_none());
        Ok(())
    }

    #[test]
    fn test_nearest_finds_the_segment_or_nothing() {
        let segments = segments(A, B, &WireShape::default(), Vector::ZERO);
        let midway = along(&segments[0], 0.5);
        assert_eq!(nearest(&segments, midway, 6.0), Some((0, 0.5)));
        assert_eq!(nearest(&segments, Point::new(0.0, 400.0), 6.0), None);
    }

    #[test]
    fn test_drag_point_handle_mirrors_unless_alt() -> Result {
        let (shape, _) = split(A, B, &WireShape::default(), Vector::ZERO, 0, 0.5).ok_or("split")?;
        let point = shape.points[0];
        let at = Point::new(point.at.x, point.at.y);
        let length = point.in_handle.dx.hypot(point.in_handle.dy);
        let to = at + Vector::new(30.0, 40.0);
        let dragged =
            drag(&shape, Grip::PointOut(0), to, (A, B), Vector::ZERO, false).ok_or("drag")?;
        let p = dragged.points[0];
        assert_eq!(p.out_handle, WireHandle { dx: 30.0, dy: 40.0 });
        assert!((p.in_handle.dx - -30.0 * length / 50.0).abs() < 1e-3);
        assert!((p.in_handle.dy - -40.0 * length / 50.0).abs() < 1e-3);
        let free = drag(&shape, Grip::PointOut(0), to, (A, B), Vector::ZERO, true).ok_or("drag")?;
        assert_eq!(free.points[0].in_handle, point.in_handle);
        assert!(drag(&shape, Grip::Point(3), to, (A, B), Vector::ZERO, false).is_none());
        Ok(())
    }

    #[test]
    fn test_drag_end_handle_and_point_store_offsets_and_positions() -> Result {
        let shift = Vector::new(0.0, 48.0);
        let (shape, _) = split(A, B, &WireShape::default(), shift, 0, 0.5).ok_or("split")?;
        let to = Point::new(50.0, 60.0);
        let ended = drag(&shape, Grip::OutHandle, to, (A, B), shift, false).ok_or("drag")?;
        assert_eq!(
            ended.out_handle,
            Some(WireHandle {
                dx: 50.0,
                dy: -40.0
            })
        );
        let moved = drag(&shape, Grip::Point(0), to, (A, B), shift, false).ok_or("drag")?;
        assert_eq!(moved.points[0].at, Position { x: 50.0, y: 12.0 });
        Ok(())
    }

    #[test]
    fn test_remove_resets_end_handles_and_drops_points() -> Result {
        let (shape, _) = split(A, B, &WireShape::default(), Vector::ZERO, 0, 0.5).ok_or("split")?;
        let reset = remove(&shape, Grip::OutHandle).ok_or("remove")?;
        assert_eq!((reset.out_handle, reset.points.len()), (None, 1));
        let dropped = remove(&shape, Grip::PointIn(0)).ok_or("remove")?;
        assert!(dropped.points.is_empty());
        assert!(remove(&shape, Grip::Point(1)).is_none());
        Ok(())
    }

    #[test]
    fn test_grips_show_open_points_handles() -> Result {
        let (shape, _) = split(A, B, &WireShape::default(), Vector::ZERO, 0, 0.5).ok_or("split")?;
        let segments = segments(A, B, &shape, Vector::ZERO);
        let kinds = |open| -> Vec<Grip> { grips(&segments, open).iter().map(|g| g.0).collect() };
        assert_eq!(
            kinds(None),
            vec![Grip::OutHandle, Grip::InHandle, Grip::Point(0)]
        );
        assert_eq!(kinds(Some(0)).len(), 5);
        Ok(())
    }
}
