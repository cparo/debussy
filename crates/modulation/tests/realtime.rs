//! The LFO run inside the real-time guard: advancing, reading offsets and changing settings make
//! no allocator call.

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
