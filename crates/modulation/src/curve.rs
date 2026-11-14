//! [`Curve`]: an automation curve of cubic Bézier segments that is a function of time
//! (`docs/architecture/automation.md` "Curves").

use crate::error::CurveError;

/// The number of bisection steps that find a segment's curve parameter for a frame. Fixed, so an
/// evaluation always costs the same and gives the same answer; 52 steps narrow the parameter to
/// within 2⁻⁵², the precision of an `f64` near 1.
const BISECTION_STEPS: u32 = 52;

/// The value of a curve with no points.
pub const EMPTY_VALUE: f32 = 0.5;

/// A handle: a Bézier control point given as an offset from its anchor.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Handle {
    /// The offset in timeline frames.
    pub d_frame: f64,
    /// The offset in normalised value.
    pub d_value: f32,
}

impl Handle {
    /// A handle at offset (`d_frame`, `d_value`).
    pub fn new(d_frame: f64, d_value: f32) -> Self {
        Handle { d_frame, d_value }
    }
}

/// An anchor point on a curve, with the handles that shape the segments either side of it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Point {
    /// The anchor's timeline frame.
    pub frame: i64,
    /// The anchor's value, from 0 to 1.
    pub value: f32,
    /// The handle shaping the segment that ends here; unused on the first point.
    pub in_handle: Handle,
    /// The handle shaping the segment that starts here; unused on the last point.
    pub out_handle: Handle,
}

impl Point {
    /// An anchor at `frame` and `value` with both handles on the anchor.
    pub fn new(frame: i64, value: f32) -> Self {
        Point {
            frame,
            value,
            ..Point::default()
        }
    }

    /// This, with in-handle `in_handle` and out-handle `out_handle`.
    #[must_use]
    pub fn with_handles(self, in_handle: Handle, out_handle: Handle) -> Self {
        Point {
            in_handle,
            out_handle,
            ..self
        }
    }
}

/// An automation curve: anchor points in strictly increasing frame order, each pair joined by a
/// cubic Bézier segment, with every segment a function of time. The value is normalised, from 0
/// to 1. A `Curve` is always valid: it can only be made through [`Curve::new`]. Evaluating it
/// allocates nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Curve {
    points: Vec<Point>,
}

impl Curve {
    /// A curve through `points`, or the first reason they do not make a valid curve (see
    /// [`Curve::validate`]).
    pub fn new(points: Vec<Point>) -> Result<Self, CurveError> {
        Curve::validate(&points)?;
        Ok(Curve { points })
    }

    /// Checks that `points` make a valid curve: every value is finite and from 0 to 1, every
    /// handle offset is finite, frames strictly increase, and every segment is a function of
    /// time. Segment `i`, from point `i` to point `i + 1`, has the x control values
    /// `x0 = frame[i]`, `x1 = x0 + out_handle[i]`, `x2 = x3 + in_handle[i+1]`,
    /// `x3 = frame[i+1]`; with `a = x1 − x0`, `b = x2 − x1` and `c = x3 − x2`, its `x′(t)` is
    /// never negative on `[0, 1]` exactly when `a ≥ 0`, `c ≥ 0`, and `b ≥ 0` or `b² ≤ ac`.
    pub fn validate(points: &[Point]) -> Result<(), CurveError> {
        for (point, p) in points.iter().enumerate() {
            if !(0.0..=1.0).contains(&p.value) {
                return Err(CurveError::ValueOutOfRange { point });
            }
            let handles = [p.in_handle, p.out_handle];
            if handles
                .iter()
                .any(|h| !h.d_frame.is_finite() || !h.d_value.is_finite())
            {
                return Err(CurveError::NonFiniteHandle { point });
            }
        }
        for (segment, pair) in points.windows(2).enumerate() {
            let (start, end) = (&pair[0], &pair[1]);
            if end.frame <= start.frame {
                return Err(CurveError::NotIncreasing { point: segment + 1 });
            }
            if !is_function_of_time(&Segment::new(start, end)) {
                return Err(CurveError::NotFunctionOfTime { segment });
            }
        }
        Ok(())
    }

    /// The curve's points, in frame order.
    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// The curve's points, given back.
    pub fn into_points(self) -> Vec<Point> {
        self.points
    }

    /// The value at timeline frame `frame`, from 0 to 1. Before the first point it is the first
    /// point's value, from the last point on it is the last point's, and a curve with no points
    /// is [`EMPTY_VALUE`]. Finds the segment by binary search; a [`Cursor`] makes sequential reads
    /// constant-time.
    pub fn value_at(&self, frame: i64) -> f32 {
        Cursor::default().value_at(self, frame)
    }
}

/// A read position on a [`Curve`]: remembers the segment last read, so reading frames in order
/// costs constant time per read. Any order of reads gives the same values as
/// [`Curve::value_at`]. A cursor holds no reference, so one may be kept across curve changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    segment: usize,
}

impl Cursor {
    /// The value of `curve` at timeline frame `frame`, as [`Curve::value_at`] gives it.
    pub fn value_at(&mut self, curve: &Curve, frame: i64) -> f32 {
        let points = curve.points();
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return EMPTY_VALUE;
        };
        if frame <= first.frame {
            return first.value;
        }
        if frame >= last.frame {
            return last.value;
        }
        // Here there are at least two points and `first.frame < frame < last.frame`.
        let spans = |s: usize| points[s].frame <= frame && frame < points[s + 1].frame;
        let count = points.len() - 1;
        if !(self.segment < count && spans(self.segment)) {
            self.segment = if self.segment + 1 < count && spans(self.segment + 1) {
                self.segment + 1
            } else {
                // The number of points at or before `frame`, at least 1 and at most `count`.
                points.partition_point(|p| p.frame <= frame) - 1
            };
        }
        let segment = Segment::new(&points[self.segment], &points[self.segment + 1]);
        segment.value_at(frame)
    }
}

/// One segment's control values, with x measured from its start frame.
struct Segment {
    /// The x control values `x1`, `x2` and `x3`, relative to `x0`, which is 0.
    x: [f64; 3],
    /// The y control values `y0` to `y3`.
    y: [f64; 4],
    /// The start frame.
    start: i64,
}

impl Segment {
    fn new(start: &Point, end: &Point) -> Self {
        // A frame difference within ±2⁵³ is exact in `f64`.
        let span = (end.frame - start.frame) as f64;
        Segment {
            x: [start.out_handle.d_frame, span + end.in_handle.d_frame, span],
            y: [
                f64::from(start.value),
                f64::from(start.value) + f64::from(start.out_handle.d_value),
                f64::from(end.value) + f64::from(end.in_handle.d_value),
                f64::from(end.value),
            ],
            start: start.frame,
        }
    }

    /// The value at `frame`, which is inside the segment: the curve parameter `t` with
    /// `x(t) = frame` found by bisection, then `y(t)`, clamped to 0 to 1.
    fn value_at(&self, frame: i64) -> f32 {
        let target = (frame - self.start) as f64;
        let [x1, x2, x3] = self.x;
        let (mut low, mut high) = (0.0_f64, 1.0_f64);
        for _ in 0..BISECTION_STEPS {
            let mid = 0.5 * (low + high);
            if bezier(0.0, x1, x2, x3, mid) < target {
                low = mid;
            } else {
                high = mid;
            }
        }
        let [y0, y1, y2, y3] = self.y;
        let value = bezier(y0, y1, y2, y3, 0.5 * (low + high));
        value.clamp(0.0, 1.0) as f32
    }
}

/// Whether the segment's x never decreases (see [`Curve::validate`]).
fn is_function_of_time(segment: &Segment) -> bool {
    let [x1, x2, x3] = segment.x;
    let (a, b, c) = (x1, x2 - x1, x3 - x2);
    a >= 0.0 && c >= 0.0 && (b >= 0.0 || b * b <= a * c)
}

/// The cubic Bézier with control values `p0` to `p3` at parameter `t`, in Bernstein form.
fn bezier(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let s = 1.0 - t;
    s * s * s * p0 + 3.0 * s * s * t * p1 + 3.0 * s * t * t * p2 + t * t * t * p3
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A two-point curve from (0, `v0`) to (`span`, `v3`) with out-handle `out` on the first point
    /// and in-handle `inh` on the second.
    fn segment_points(span: i64, v0: f32, v3: f32, out: Handle, inh: Handle) -> Vec<Point> {
        vec![
            Point::new(0, v0).with_handles(Handle::default(), out),
            Point::new(span, v3).with_handles(inh, Handle::default()),
        ]
    }

    /// A dense de Casteljau reference for a two-point curve: the points `(x, y)` at `samples + 1`
    /// evenly spaced curve parameters.
    fn casteljau_samples(points: &[Point], samples: u32) -> Vec<(f64, f64)> {
        let (p, q) = (points[0], points[1]);
        let control = [
            (p.frame as f64, f64::from(p.value)),
            (
                p.frame as f64 + p.out_handle.d_frame,
                f64::from(p.value) + f64::from(p.out_handle.d_value),
            ),
            (
                q.frame as f64 + q.in_handle.d_frame,
                f64::from(q.value) + f64::from(q.in_handle.d_value),
            ),
            (q.frame as f64, f64::from(q.value)),
        ];
        (0..=samples)
            .map(|k| {
                let t = f64::from(k) / f64::from(samples);
                let mut c = control;
                for level in (1..4).rev() {
                    for i in 0..level {
                        c[i] = (
                            c[i].0 + t * (c[i + 1].0 - c[i].0),
                            c[i].1 + t * (c[i + 1].1 - c[i].1),
                        );
                    }
                }
                c[0]
            })
            .collect()
    }

    /// The reference value at `frame`: linear interpolation between the two samples around it,
    /// clamped to 0 to 1.
    fn reference(samples: &[(f64, f64)], frame: f64) -> f64 {
        let after = samples
            .partition_point(|s| s.0 < frame)
            .clamp(1, samples.len() - 1);
        let (previous, next) = (samples[after - 1], samples[after]);
        let span = next.0 - previous.0;
        let along = if span > 0.0 {
            ((frame - previous.0) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (previous.1 + along * (next.1 - previous.1)).clamp(0.0, 1.0)
    }

    #[test]
    fn test_validate_accepts_empty_and_single_point() -> Result<(), CurveError> {
        Curve::validate(&[])?;
        Curve::validate(&[Point::new(-5, 1.0)])?;
        Ok(())
    }

    #[test]
    fn test_validate_refuses_frames_not_increasing() {
        let points = [Point::new(0, 0.5), Point::new(10, 0.5), Point::new(10, 0.5)];
        assert_eq!(
            Curve::validate(&points),
            Err(CurveError::NotIncreasing { point: 2 })
        );
        let backwards = [Point::new(10, 0.5), Point::new(0, 0.5)];
        assert_eq!(
            Curve::validate(&backwards),
            Err(CurveError::NotIncreasing { point: 1 })
        );
    }

    #[test]
    fn test_validate_refuses_values_outside_range() {
        for value in [-0.01, 1.01, f32::NAN, f32::INFINITY] {
            let points = [Point::new(0, 0.5), Point::new(1, value)];
            assert_eq!(
                Curve::validate(&points),
                Err(CurveError::ValueOutOfRange { point: 1 })
            );
        }
    }

    #[test]
    fn test_validate_refuses_non_finite_handles() {
        let bad = [Handle::new(f64::NAN, 0.0), Handle::new(0.0, f32::INFINITY)];
        for handle in bad {
            // Even an unused handle must be finite.
            let points = [Point::new(0, 0.5).with_handles(handle, Handle::default())];
            assert_eq!(
                Curve::validate(&points),
                Err(CurveError::NonFiniteHandle { point: 0 })
            );
        }
    }

    #[test]
    fn test_validate_b_negative_boundary() {
        // a = 100, b = −100, c = 100: b² = ac, the boundary, which x′ touches 0 at t = ½.
        let edge = segment_points(
            100,
            0.0,
            1.0,
            Handle::new(100.0, 0.0),
            Handle::new(-100.0, 0.0),
        );
        assert_eq!(Curve::validate(&edge), Ok(()));
        // a = 101, b = −101, c = 100: b² > ac, so x turns back.
        let past = segment_points(
            100,
            0.0,
            1.0,
            Handle::new(101.0, 0.0),
            Handle::new(-100.0, 0.0),
        );
        assert_eq!(
            Curve::validate(&past),
            Err(CurveError::NotFunctionOfTime { segment: 0 })
        );
    }

    #[test]
    fn test_validate_accepts_handles_past_the_neighbour_when_b_is_small() {
        // a = 80, b = −20, c = 40: b² = 400 ≤ ac = 3200, so the crossed handles still never turn
        // back.
        let points = segment_points(
            100,
            0.0,
            1.0,
            Handle::new(80.0, 0.0),
            Handle::new(-40.0, 0.0),
        );
        assert_eq!(Curve::validate(&points), Ok(()));
    }

    #[test]
    fn test_validate_accepts_vertical_tangent() {
        // A handle straight up from its anchor (a = 0) leaves the curve a function of time.
        let points = segment_points(90, 0.0, 1.0, Handle::new(0.0, 0.9), Handle::new(-30.0, 0.0));
        assert_eq!(Curve::validate(&points), Ok(()));
    }

    #[test]
    fn test_validate_refuses_handles_pointing_backwards() {
        let out_back = segment_points(
            90,
            0.0,
            1.0,
            Handle::new(-1.0, 0.0),
            Handle::new(-30.0, 0.0),
        );
        assert_eq!(
            Curve::validate(&out_back),
            Err(CurveError::NotFunctionOfTime { segment: 0 })
        );
        let in_forward =
            segment_points(90, 0.0, 1.0, Handle::new(30.0, 0.0), Handle::new(1.0, 0.0));
        assert_eq!(
            Curve::validate(&in_forward),
            Err(CurveError::NotFunctionOfTime { segment: 0 })
        );
    }

    #[test]
    fn test_new_names_the_failing_segment() {
        let mut points = vec![Point::new(0, 0.0), Point::new(10, 0.5), Point::new(20, 1.0)];
        points[1].out_handle = Handle::new(-2.0, 0.0);
        assert_eq!(
            Curve::new(points),
            Err(CurveError::NotFunctionOfTime { segment: 1 })
        );
    }

    #[test]
    fn test_value_at_empty_curve_is_half() {
        assert_eq!(Curve::default().value_at(123), EMPTY_VALUE);
    }

    #[test]
    fn test_value_at_holds_outside_the_points() -> Result<(), CurveError> {
        let curve = Curve::new(vec![Point::new(100, 0.25), Point::new(200, 0.75)])?;
        assert_eq!(curve.value_at(i64::MIN), 0.25);
        assert_eq!(curve.value_at(100), 0.25);
        assert_eq!(curve.value_at(200), 0.75);
        assert_eq!(curve.value_at(i64::MAX), 0.75);
        let single = Curve::new(vec![Point::new(0, 0.9)])?;
        assert_eq!(single.value_at(-1), 0.9);
        assert_eq!(single.value_at(1), 0.9);
        Ok(())
    }

    #[test]
    fn test_value_at_third_handles_give_a_straight_line() -> Result<(), CurveError> {
        // Handles at ⅓ of the gap along the chord make the segment a straight line.
        let curve = Curve::new(segment_points(
            300,
            0.0,
            0.6,
            Handle::new(100.0, 0.2),
            Handle::new(-100.0, -0.2),
        ))?;
        for frame in 0..=300 {
            let expected = 0.6 * frame as f32 / 300.0;
            assert!(
                (curve.value_at(frame) - expected).abs() < 1e-6,
                "frame {frame}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_value_at_matches_de_casteljau_reference() -> Result<(), CurveError> {
        let cases = [
            (Handle::new(400.0, 0.0), Handle::new(-400.0, 0.0)),
            (Handle::new(0.0, 0.9), Handle::new(-300.0, 0.0)),
            (Handle::new(1000.0, 0.0), Handle::new(-1000.0, 0.0)),
            (Handle::new(800.0, 1.5), Handle::new(-400.0, -1.5)),
            (Handle::new(10.0, -0.4), Handle::new(0.0, 0.4)),
        ];
        for (out, inh) in cases {
            let points = segment_points(1000, 0.1, 0.8, out, inh);
            let samples = casteljau_samples(&points, 200_000);
            let curve = Curve::new(points)?;
            for frame in 0..=1000 {
                let expected = reference(&samples, frame as f64);
                let got = f64::from(curve.value_at(frame));
                assert!(
                    (got - expected).abs() < 1e-4,
                    "{out:?} {inh:?} frame {frame}: {got} vs {expected}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn test_value_at_clamps_overshoot() -> Result<(), CurveError> {
        let curve = Curve::new(segment_points(
            100,
            1.0,
            1.0,
            Handle::new(30.0, 0.8),
            Handle::new(-30.0, 0.8),
        ))?;
        assert_eq!(curve.value_at(50), 1.0);
        Ok(())
    }

    #[test]
    fn test_cursor_matches_fresh_reads_in_any_order() -> Result<(), CurveError> {
        let mut points = Vec::new();
        for i in 0..20_i64 {
            let value = if i % 2 == 0 { 0.2 } else { 0.9 };
            points.push(
                Point::new(i * 50, value)
                    .with_handles(Handle::new(-15.0, 0.05), Handle::new(15.0, -0.05)),
            );
        }
        let curve = Curve::new(points)?;
        let mut cursor = Cursor::default();
        let frames = (-10..1_010)
            .chain((-10..1_010).rev())
            .chain([500, 3, 999, 0, 49, 50]);
        for frame in frames {
            assert_eq!(
                cursor.value_at(&curve, frame),
                curve.value_at(frame),
                "frame {frame}"
            );
        }
        // A cursor kept across a change to a shorter curve still reads correctly.
        let short = Curve::new(vec![Point::new(0, 0.0), Point::new(10, 1.0)])?;
        assert_eq!(cursor.value_at(&short, 5), short.value_at(5));
        Ok(())
    }
}
