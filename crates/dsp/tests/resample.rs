//! Measured quality of each preset (ADR-0036), streaming behaviour, and real-time safety.

use std::f64::consts::PI;

use dsp::{Quality, Resampler};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

/// Converts one second of a sine at `freq` and returns the steady-state output.
fn convert_sine(from: u32, to: u32, quality: Quality, freq: f64) -> Result<Vec<f32>, dsp::Error> {
    let mut r = Resampler::new(from, to, quality)?;
    let input: Vec<f32> = (0..from)
        .map(|n| (2.0 * PI * freq * f64::from(n) / f64::from(from)).sin() as f32)
        .collect();
    let mut output = vec![0.0_f32; to as usize + 16];
    let (consumed, produced) = r.process(&input, &mut output);
    assert_eq!(consumed, input.len());
    output.truncate(produced);
    // Skip the filter's warm-up (twice its latency) and keep the rest, minus the tail.
    let skip = (2.0 * r.latency()).ceil() as usize + 16;
    Ok(output[skip..produced - 16].to_vec())
}

/// Fits `a·cos + b·sin` at `freq` by least squares; returns (amplitude, residual RMS).
fn fit(samples: &[f32], freq: f64, rate: f64) -> (f64, f64) {
    let (mut cc, mut cs, mut ss, mut yc, mut ys) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let basis = |n: usize| {
        let w = 2.0 * PI * freq * n as f64 / rate;
        (w.cos(), w.sin())
    };
    for (n, &y) in samples.iter().enumerate() {
        let (c, s) = basis(n);
        let y = f64::from(y);
        cc += c * c;
        cs += c * s;
        ss += s * s;
        yc += y * c;
        ys += y * s;
    }
    let det = cc * ss - cs * cs;
    let a = (yc * ss - ys * cs) / det;
    let b = (ys * cc - yc * cs) / det;
    let mut residual = 0.0;
    for (n, &y) in samples.iter().enumerate() {
        let (c, s) = basis(n);
        let e = f64::from(y) - a * c - b * s;
        residual += e * e;
    }
    (
        (a * a + b * b).sqrt(),
        (residual / samples.len() as f64).sqrt(),
    )
}

fn db(x: f64) -> f64 {
    20.0 * x.log10()
}

const RATIOS: [(u32, u32); 6] = [
    (44_100, 48_000),
    (48_000, 44_100),
    (44_100, 96_000),
    (96_000, 44_100),
    (48_000, 88_200),
    (22_050, 48_000),
];

/// In-band tones come out at unity gain (±0.01 dB) with everything else below `floor_db`.
fn check_passband(quality: Quality, floor_db: f64) -> Result<(), Box<dyn std::error::Error>> {
    for (from, to) in RATIOS {
        let f_min = f64::from(from.min(to));
        for fraction in [0.002, 0.05, 0.2, 0.4, 0.45] {
            let freq = fraction * f_min;
            let out = convert_sine(from, to, quality, freq)?;
            let (amplitude, residual) = fit(&out, freq, f64::from(to));
            let gain = db(amplitude);
            let noise = db(residual / (amplitude / 2.0_f64.sqrt()));
            assert!(
                gain.abs() <= 0.01,
                "{from}→{to} {freq} Hz: gain {gain:.4} dB"
            );
            assert!(
                noise <= -floor_db,
                "{from}→{to} {freq} Hz: residual {noise:.1} dB"
            );
        }
    }
    Ok(())
}

/// Tones in the stopband come out at least `floor_db` down.
fn check_stopband(quality: Quality, floor_db: f64) -> Result<(), Box<dyn std::error::Error>> {
    let mut tested = 0;
    for (from, to) in [
        (48_000, 44_100),
        (96_000, 44_100),
        (96_000, 48_000),
        (88_200, 44_100),
    ] {
        let f_min = f64::from(from.min(to));
        let stop = match quality {
            Quality::Import => f_min / 2.0,
            Quality::Device => f_min * (1.0 - 0.4535),
        };
        // Tones must lie below the input's Nyquist frequency, or they alias before conversion.
        let nyquist = f64::from(from) / 2.0;
        let tones = [stop + 50.0, (stop + nyquist) / 2.0, nyquist - 100.0];
        for freq in tones
            .into_iter()
            .filter(|&f| f > stop && f < nyquist - 50.0)
        {
            let out = convert_sine(from, to, quality, freq)?;
            let rms =
                (out.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / out.len() as f64).sqrt();
            let level = db(rms / (1.0 / 2.0_f64.sqrt()));
            assert!(
                level <= -floor_db,
                "{from}→{to} {freq:.0} Hz leaks at {level:.1} dB"
            );
            tested += 1;
        }
    }
    assert!(
        tested >= 4,
        "only {tested} stopband tones fell below the input Nyquist"
    );
    Ok(())
}

#[test]
fn test_import_passband_flat_and_clean() -> Result<(), Box<dyn std::error::Error>> {
    check_passband(Quality::Import, 120.0)
}

#[test]
fn test_import_stopband_at_least_120_db() -> Result<(), Box<dyn std::error::Error>> {
    check_stopband(Quality::Import, 120.0)
}

#[test]
fn test_device_passband_flat_and_clean() -> Result<(), Box<dyn std::error::Error>> {
    check_passband(Quality::Device, 96.0)
}

#[test]
fn test_device_stopband_at_least_96_db() -> Result<(), Box<dyn std::error::Error>> {
    check_stopband(Quality::Device, 96.0)
}

#[test]
fn test_chunked_processing_matches_one_call() -> Result<(), Box<dyn std::error::Error>> {
    let input: Vec<f32> = (0..20_000).map(|n| ((n as f32) * 0.013).sin()).collect();
    let mut whole = Resampler::new(44_100, 48_000, Quality::Device)?;
    let mut expected = vec![0.0_f32; 25_000];
    let (_, n) = whole.process(&input, &mut expected);
    expected.truncate(n);

    let mut chunked = Resampler::new(44_100, 48_000, Quality::Device)?;
    let mut got = Vec::new();
    let mut at = 0;
    let mut out = [0.0_f32; 97];
    let mut step = 1;
    while at < input.len() {
        let end = (at + step).min(input.len());
        let mut slice = &input[at..end];
        loop {
            let (consumed, produced) = chunked.process(slice, &mut out);
            got.extend_from_slice(&out[..produced]);
            slice = &slice[consumed..];
            if slice.is_empty() && produced < out.len() {
                break;
            }
        }
        at = end;
        step = step % 300 + 37;
    }
    assert_eq!(got, expected);
    Ok(())
}

#[test]
fn test_latency_matches_impulse_peak() -> Result<(), Box<dyn std::error::Error>> {
    for (from, to) in [(48_000, 48_000 * 2), (44_100, 48_000), (96_000, 48_000)] {
        let mut r = Resampler::new(from, to, Quality::Device)?;
        let mut input = vec![0.0_f32; 4000];
        input[0] = 1.0;
        let mut output = vec![0.0_f32; 10_000];
        let (_, n) = r.process(&input, &mut output);
        let peak = output[..n]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .ok_or("no output")?;
        assert!(
            (peak as f64 - r.latency()).abs() <= 1.0,
            "{from}→{to}: peak {peak}, latency {}",
            r.latency()
        );
    }
    Ok(())
}

#[test]
fn test_process_and_reset_do_not_allocate() -> Result<(), Box<dyn std::error::Error>> {
    let mut r = Resampler::new(48_000, 44_100, Quality::Device)?;
    let input = [0.25_f32; 128];
    let mut output = [0.0_f32; 128];
    let (consumed, produced) = assert_realtime(|| {
        let result = r.process(&input, &mut output);
        r.reset();
        result
    });
    assert!(consumed > 0 && produced > 0);
    Ok(())
}

#[test]
fn test_device_preset_cost_and_latency() -> Result<(), Box<dyn std::error::Error>> {
    // Bound the real-time cost between the supported project and device rates.
    let rates = [44_100, 48_000, 88_200, 96_000];
    for from in rates {
        for to in rates {
            let r = Resampler::new(from, to, Quality::Device)?;
            let ms = r.latency() / f64::from(to) * 1000.0;
            assert!(r.taps() <= 160, "{from}→{to}: {} taps", r.taps());
            assert!(ms <= 1.5, "{from}→{to}: {ms:.2} ms");
        }
    }
    Ok(())
}
