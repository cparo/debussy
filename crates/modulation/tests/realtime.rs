//! The LFO and curve reads run inside the real-time guard: advancing, reading offsets, changing
//! settings and reading a curve make no allocator call.

use modulation::{Lfo, Shape, sync};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

#[test]
fn test_lfo_runs_without_allocating() {
    let mut lfo = Lfo::new(48_000, 11);
    let total = assert_realtime(|| {
        let mut total = 0.0_f32;
        for shape in [
            Shape::Sine,
            Shape::Triangle,
            Shape::SawUp,
            Shape::SawDown,
            Shape::Square,
            Shape::SampleAndHold,
        ] {
            lfo.set_shape(shape);
            lfo.set_frequency(3.0);
            lfo.set_period_frames(sync::period_frames(
                sync::whole_note(None, 48_000),
                2.0,
                1.0,
            ));
            for _ in 0..4_800 {
                total += lfo.value_at_offset(0.25, 1);
                total += lfo.next_value();
            }
            lfo.reset(0.5);
        }
        total
    });
    assert!(total.is_finite());
}

#[test]
fn test_curve_reads_without_allocating() -> Result<(), modulation::CurveError> {
    use modulation::{Cursor, Curve, Handle, Point};
    let points = (0..64_i64)
        .map(|i| {
            Point::new(i * 1_000, if i % 2 == 0 { 0.1 } else { 0.9 })
                .with_handles(Handle::new(-300.0, 0.0), Handle::new(300.0, 0.0))
        })
        .collect();
    let curve = Curve::new(points)?;
    let total = assert_realtime(|| {
        let mut cursor = Cursor::default();
        let mut total = 0.0_f32;
        for frame in (-1_000..70_000).step_by(32) {
            total += cursor.value_at(&curve, frame);
        }
        total += curve.value_at(31_234);
        total
    });
    assert!(total.is_finite());
    Ok(())
}
