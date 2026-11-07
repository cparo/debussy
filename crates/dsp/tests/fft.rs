//! FFT accuracy against a direct DFT, round trips, the real-input split, the windowed spectrum,
//! and real-time safety.

use std::error::Error;
use std::f64::consts::PI;

use dsp::{Complex, Fft, RealFft, Stft};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

/// A reproducible test signal in roughly [-1, 1].
fn signal(n: usize, seed: u64) -> Vec<f32> {
    let mut noise = dsp::Noise::new(seed);
    (0..n).map(|_| 2.0 * noise.uniform()).collect()
}

/// The DFT computed directly in f64.
fn dft(x: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let n = x.len();
    (0..n)
        .map(|k| {
            x.iter()
                .enumerate()
                .fold((0.0, 0.0), |(re, im), (j, &(a, b))| {
                    let w = -2.0 * PI * ((k * j) % n) as f64 / n as f64;
                    (
                        re + a * w.cos() - b * w.sin(),
                        im + a * w.sin() + b * w.cos(),
                    )
                })
        })
        .collect()
}

/// Largest error relative to the largest reference magnitude.
fn relative_error(got: &[Complex], want: &[(f64, f64)]) -> f64 {
    let scale = want.iter().map(|&(a, b)| a.hypot(b)).fold(0.0, f64::max);
    got.iter()
        .zip(want)
        .map(|(g, &(a, b))| (f64::from(g.re) - a).hypot(f64::from(g.im) - b))
        .fold(0.0, f64::max)
        / scale
}

#[test]
fn test_complex_fft_matches_direct_dft() -> Result<(), Box<dyn Error>> {
    for size in [64, 128, 256, 512, 1024, 2048] {
        let re = signal(size, 1);
        let im = signal(size, 2);
        let mut data: Vec<Complex> = re
            .iter()
            .zip(&im)
            .map(|(&a, &b)| Complex::new(a, b))
            .collect();
        let reference: Vec<(f64, f64)> = data
            .iter()
            .map(|c| (f64::from(c.re), f64::from(c.im)))
            .collect();
        Fft::new(size)?.forward(&mut data)?;
        let error = relative_error(&data, &dft(&reference));
        assert!(error < 1e-5, "size {size}: {error:e}");
    }
    Ok(())
}

#[test]
fn test_complex_round_trip_every_size() -> Result<(), Box<dyn Error>> {
    let mut size = 64;
    while size <= 16_384 {
        let fft = Fft::new(size)?;
        let original: Vec<Complex> = signal(size, 3)
            .iter()
            .zip(signal(size, 4))
            .map(|(&a, b)| Complex::new(a, b))
            .collect();
        let mut data = original.clone();
        fft.forward(&mut data)?;
        fft.inverse(&mut data)?;
        let error = data
            .iter()
            .zip(&original)
            .map(|(a, b)| (*a - *b).abs())
            .fold(0.0_f32, f32::max);
        assert!(error < 1e-5, "size {size}: {error:e}");
        size *= 2;
    }
    Ok(())
}

#[test]
fn test_real_fft_matches_complex_fft() -> Result<(), Box<dyn Error>> {
    let mut size = 64;
    while size <= 16_384 {
        let input = signal(size, 5);
        let mut full: Vec<Complex> = input.iter().map(|&x| Complex::new(x, 0.0)).collect();
        Fft::new(size)?.forward(&mut full)?;
        let mut real = RealFft::new(size)?;
        let mut bins = vec![Complex::default(); real.bins()];
        real.forward(&input, &mut bins)?;
        let want: Vec<(f64, f64)> = full[..real.bins()]
            .iter()
            .map(|c| (f64::from(c.re), f64::from(c.im)))
            .collect();
        let error = relative_error(&bins, &want);
        assert!(error < 1e-5, "size {size}: {error:e}");
        assert!(bins[0].im.abs() < 1e-3 && bins[size / 2].im.abs() < 1e-3);

        let mut back = vec![0.0_f32; size];
        real.inverse(&bins, &mut back)?;
        let error = back
            .iter()
            .zip(&input)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(error < 1e-5, "inverse size {size}: {error:e}");
        size *= 2;
    }
    Ok(())
}

#[test]
fn test_stft_reads_sine_amplitude_at_its_bin() -> Result<(), Box<dyn Error>> {
    let size = 1024;
    let bin = 37;
    let amplitude = 0.5;
    let frame: Vec<f32> = (0..size)
        .map(|n| (amplitude * (2.0 * PI * (bin * n) as f64 / size as f64).sin()) as f32)
        .collect();
    let mut stft = Stft::new(size)?;
    let mut magnitudes = vec![0.0; stft.bins()];
    stft.magnitudes(&frame, &mut magnitudes)?;
    assert!(
        (f64::from(magnitudes[bin]) - amplitude).abs() < 1e-4,
        "{}",
        magnitudes[bin]
    );
    // A Hann window spreads a bin-centred sine into its two neighbours only.
    assert!((f64::from(magnitudes[bin - 1]) - amplitude / 2.0).abs() < 1e-4);
    for (k, &m) in magnitudes.iter().enumerate() {
        if k + 1 < bin || k > bin + 1 {
            assert!(m < 1e-4, "bin {k}: {m}");
        }
    }
    Ok(())
}

#[test]
fn test_transforms_reject_wrong_lengths() -> Result<(), Box<dyn Error>> {
    let fft = Fft::new(64)?;
    assert!(fft.forward(&mut [Complex::default(); 63]).is_err());
    let mut real = RealFft::new(64)?;
    let mut bins = [Complex::default(); 33];
    assert!(real.forward(&[0.0; 64], &mut bins[..32]).is_err());
    assert!(real.inverse(&bins, &mut [0.0; 63]).is_err());
    let mut stft = Stft::new(64)?;
    assert!(stft.magnitudes(&[0.0; 64], &mut [0.0; 32]).is_err());
    Ok(())
}

#[test]
fn test_transforms_do_not_allocate() -> Result<(), Box<dyn Error>> {
    let fft = Fft::new(4096)?;
    let mut real = RealFft::new(4096)?;
    let mut stft = Stft::new(4096)?;
    let mut data = vec![Complex::new(0.5, -0.25); 4096];
    let input = signal(4096, 6);
    let mut bins = vec![Complex::default(); real.bins()];
    let mut back = vec![0.0_f32; 4096];
    let mut magnitudes = vec![0.0_f32; stft.bins()];
    assert_realtime(|| -> Result<(), dsp::Error> {
        fft.forward(&mut data)?;
        fft.inverse(&mut data)?;
        real.forward(&input, &mut bins)?;
        real.inverse(&bins, &mut back)?;
        stft.magnitudes(&input, &mut magnitudes)
    })?;
    Ok(())
}
