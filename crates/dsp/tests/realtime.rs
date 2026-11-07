//! Every operation, run inside the real-time guard.

use dsp::{
    Depth, Ditherer, Noise, apply_gain, db_to_gain, fold_to_mono, gain_to_db, pan_gains, pan_mono,
};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

#[test]
fn test_dither_does_not_allocate() {
    let mut ditherer = Ditherer::new(Depth::Int24, 42);
    let mut block = [0.25_f32; 1024];
    assert_realtime(|| ditherer.process(&mut block));
    let mut noise = Noise::new(1);
    let sum = assert_realtime(|| (0..256).map(|_| noise.triangular()).sum::<f32>());
    assert!(sum.is_finite());
}

#[test]
fn test_level_operations_do_not_allocate() {
    let mono = [0.5_f32; 128];
    let (mut left, mut right, mut folded) = ([0.0_f32; 128], [0.0_f32; 128], [0.0_f32; 128]);
    let gains = assert_realtime(|| {
        let gain = db_to_gain(-6.0);
        pan_mono(&mono, 0.3, &mut left, &mut right);
        apply_gain(&mut left, gain);
        fold_to_mono(&left, &right, &mut folded);
        (pan_gains(-0.5), gain_to_db(gain))
    });
    assert!(gains.0.0 > gains.0.1);
}
