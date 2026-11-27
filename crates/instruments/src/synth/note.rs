//! The synth's note preview (`docs/architecture/effect-displays.md` "Synth note display",
//! ADR-0172): one note rendered offline by the voice code [`Synth`](super::Synth) plays with, on
//! state of its own, so it never touches a running synth.

use dsp::{MinBlep, OnePoleCoefficients, db_to_gain};
use modulation::Lfo;
use signal_graph::Parameter;

use super::{
    Block, DC_BLOCK_HZ, LFO_SEED, Parameters, Tone, Values, configure, configure_lfo, render,
};
use crate::note_curve::{NOTE, NoteCurve, Source};
use crate::voices::Pool;

/// A fresh set of parameters at `values` (in declaration order, clamped and rounded as a
/// running synth's are); `None` for values of the wrong length.
fn parameters_at(values: &[f32]) -> Option<Parameters> {
    let p = Parameters::new().ok()?;
    let all = p.all();
    if all.len() != values.len() {
        return None;
    }
    for (parameter, &value) in all.iter().zip(values) {
        parameter.set(value);
    }
    Some(p)
}

/// Sets each of `parameters` to its default.
fn to_default<'a>(parameters: impl IntoIterator<Item = &'a Parameter>) {
    for parameter in parameters {
        parameter.set(parameter.info().default);
    }
}

/// The note curve of a synth whose parameters stand at `values` (in declaration order), at
/// `sample_rate`; `None` for values of the wrong length. The values that cannot change a lone
/// note at full velocity and no bend are set to their defaults, so changing one leaves the
/// curve equal.
pub(crate) fn note_curve(values: &[f32], sample_rate: u32) -> Option<NoteCurve> {
    let p = parameters_at(values)?;
    to_default([&p.voices, &p.bend_range, &p.amp_velocity]);
    if p.lfo_depth.get() == 0.0 {
        to_default([&p.lfo_rate, &p.lfo_shape, &p.lfo_destination]);
    }
    let settle = |env: &crate::voices::EnvelopeParameters| env.attack.get() + env.decay.get();
    let settle = settle(&p.amp_env).max(settle(&p.filter_env));
    let values = p.all().iter().map(Parameter::get).collect();
    Some(NoteCurve::new(
        Source::Synth,
        values,
        sample_rate,
        settle,
        p.amp_env.release.get(),
    ))
}

/// `frames` samples of an A4 at velocity 127 from a synth at `values`, at `sample_rate`,
/// released at frame `gate_off`, after the output gain: what a running synth with the same
/// values, prepared at the same rate, plays for that note from rest. Silence for values of the
/// wrong length. Allocates.
pub(crate) fn render_note(
    values: &[f32],
    sample_rate: u32,
    gate_off: usize,
    frames: usize,
) -> Vec<f32> {
    let mut out = vec![0.0; frames];
    let (Some(p), Ok(table)) = (parameters_at(values), MinBlep::new()) else {
        return out;
    };
    let sample_rate = sample_rate.max(1);
    // Sample rates are exact in `f32` well past any audio rate.
    #[allow(clippy::cast_precision_loss)]
    let rate = sample_rate as f32;
    let mut pool = Pool::new(1, rate, Tone::new);
    for voice in pool.voices.iter_mut() {
        configure(voice, &p, rate);
    }
    let mut lfo = Lfo::new(sample_rate, LFO_SEED);
    configure_lfo(&mut lfo, &p);
    let block = Block::new(&p, rate);
    let values = Values::of(&p);
    let dc = OnePoleCoefficients::high_pass(DC_BLOCK_HZ, rate);
    let (gain, amp_velocity) = (db_to_gain(p.gain.get()), p.amp_velocity.get());
    pool.note_on(NOTE, 1.0, 1, amp_velocity, false, ());
    for (n, out) in out.iter_mut().enumerate() {
        if n == gate_off {
            pool.note_off(NOTE);
        }
        let frame = block.frame(&values, lfo.next_value());
        if pool.voices[0].is_free() {
            continue;
        }
        let sample = render(&mut pool.voices[0], &frame, &table, &dc);
        *out = sample * pool.step_fade(0, amp_velocity) * gain;
    }
    out
}

#[cfg(test)]
mod tests {
    use dsp::Stft;

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const RATE: u32 = 48_000;

    /// The synth's default values with `changes` set by name.
    fn values_with(changes: &[(&str, f32)]) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        Ok(crate::registry::values_with("synth", changes).ok_or("no synth")?)
    }

    /// The curve of a synth at its defaults with `changes`, at [`RATE`].
    fn curve(changes: &[(&str, f32)]) -> Result<NoteCurve, Box<dyn std::error::Error>> {
        Ok(note_curve(&values_with(changes)?, RATE).ok_or("no curve")?)
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |m, s| m.max(s.abs()))
    }

    #[test]
    fn test_note_curve_kind_unknown_or_wrong_length() -> TestResult {
        let mut values = values_with(&[])?;
        assert!(crate::note_curve("synth", &values, RATE).is_some());
        assert!(crate::note_curve("sampler", &values, RATE).is_none());
        assert!(crate::note_curve("no-such-instrument", &values, RATE).is_none());
        values.push(0.0);
        assert!(crate::note_curve("synth", &values, RATE).is_none());
        values.truncate(values.len() - 2);
        assert!(crate::note_curve("synth", &values, RATE).is_none());
        Ok(())
    }

    #[test]
    fn test_note_curve_span_follows_the_slower_envelope() -> TestResult {
        let amp = curve(&[
            ("amp attack", 1.0),
            ("amp decay", 0.5),
            ("amp release", 2.0),
            ("filter attack", 0.1),
            ("filter decay", 0.1),
        ])?;
        assert!((amp.span() - 3.75).abs() < 1e-5, "{}", amp.span());
        assert!((amp.gate_off() - 1.75).abs() < 1e-5, "{}", amp.gate_off());
        let filter = curve(&[
            ("amp attack", 0.01),
            ("amp decay", 0.01),
            ("amp release", 0.01),
            ("filter attack", 2.0),
            ("filter decay", 1.0),
        ])?;
        assert!((filter.span() - 3.26).abs() < 1e-5, "{}", filter.span());
        assert!(filter.gate_off() <= 0.75 * filter.span());
        Ok(())
    }

    #[test]
    fn test_note_curve_ignores_values_that_cannot_change_the_note() -> TestResult {
        let base = curve(&[])?;
        for change in [("voices", 3.0), ("bend range", 12.0), ("amp velocity", 0.1)] {
            assert_eq!(curve(&[change])?, base, "{}", change.0);
        }
        // The LFO's settings matter only while it has depth.
        assert_eq!(curve(&[("lfo rate", 9.0)])?, base);
        let deep = [("lfo depth", 0.5)];
        assert_ne!(curve(&[deep[0], ("lfo rate", 9.0)])?, curve(&deep)?);
        assert_ne!(curve(&[("cutoff", 500.0)])?, base);
        Ok(())
    }

    #[test]
    fn test_render_is_deterministic() -> TestResult {
        let changes = [("lfo depth", 0.5), ("lfo shape", 5.0)];
        let (a, b) = (curve(&changes)?.render(), curve(&changes)?.render());
        assert_eq!(a, b);
        assert!(peak(a.samples()) > 0.0);
        Ok(())
    }

    #[test]
    fn test_render_silent_when_both_levels_are_zero() -> TestResult {
        let note = curve(&[("osc 1 level", 0.0), ("osc 2 level", 0.0)])?.render();
        assert!(note.samples().iter().all(|&s| s == 0.0));
        Ok(())
    }

    #[test]
    fn test_render_peaks_near_the_amp_envelope_peak() -> TestResult {
        // One saw, the filter open, full sustain: a full-scale wave, over-shooting at its edges
        // and offset while the DC blocker settles, then scaled by the gain.
        let plain = |gain| {
            curve(&[
                ("osc 2 level", 0.0),
                ("cutoff", 20_000.0),
                ("envelope amount", 0.0),
                ("amp sustain", 1.0),
                ("gain", gain),
            ])
        };
        let unity = peak(plain(0.0)?.render().samples());
        assert!((0.9..1.5).contains(&unity), "{unity}");
        let quieter = peak(plain(-12.0)?.render().samples());
        assert!(
            (quieter / unity - db_to_gain(-12.0)).abs() < 1e-4,
            "{quieter}"
        );
        Ok(())
    }

    #[test]
    fn test_render_pitch_is_440_hz() -> TestResult {
        let note = curve(&[("osc 2 level", 0.0), ("amp sustain", 1.0)])?.render();
        let mut stft = Stft::new(8192)?;
        let mut magnitudes = vec![0.0; stft.bins()];
        stft.magnitudes(&note.samples()[2_048..2_048 + 8192], &mut magnitudes)?;
        let strongest = (0..magnitudes.len())
            .max_by(|&a, &b| magnitudes[a].total_cmp(&magnitudes[b]))
            .ok_or("no bins")?;
        let bin_hz = RATE as f32 / 8192.0;
        let hz = strongest as f32 * bin_hz;
        assert!((hz - 440.0).abs() < bin_hz, "{hz}");
        Ok(())
    }

    #[test]
    fn test_levels_nan_above_nyquist() -> TestResult {
        let note = note_curve(&values_with(&[])?, 22_050)
            .ok_or("no curve")?
            .render();
        assert_eq!(note.sample_rate(), 22_050);
        let (columns, rows) = (4, 30);
        let levels = note.levels(columns, rows, 20.0, 20_000.0);
        assert_eq!(levels.len(), columns * rows);
        // 11.025 kHz lies in row 26 of 30.
        assert!(levels[27 * columns..].iter().all(|l| l.is_nan()));
        // The fundamental's row, mid-note, is loud.
        let row = ((440.0_f32 / 20.0).log10() / 3.0 * 30.0) as usize;
        assert!(
            levels[row * columns + 1] > -30.0,
            "{}",
            levels[row * columns + 1]
        );
        assert!(note.levels(0, rows, 20.0, 20_000.0).is_empty());
        Ok(())
    }

    #[test]
    fn test_note_curve_rate_is_capped() -> TestResult {
        let curve = note_curve(&values_with(&[])?, 192_000).ok_or("no curve")?;
        assert_eq!(curve.sample_rate(), crate::NOTE_MAX_RATE);
        Ok(())
    }

    #[test]
    #[ignore = "timing: run in release with --ignored --nocapture"]
    // It prints its timings; it runs off the real-time thread.
    #[allow(clippy::disallowed_macros)]
    fn test_render_and_levels_worst_case_time() -> TestResult {
        // The longest span, the 24 dB order and the LFO on, with a display at scale 2's columns
        // and rows.
        let curve = curve(&[
            ("amp attack", 10.0),
            ("amp release", 10.0),
            ("filter order", 4.0),
            ("lfo depth", 1.0),
        ])?;
        assert_eq!(curve.span(), crate::NOTE_MAX_SPAN);
        let start = std::time::Instant::now();
        let note = curve.render();
        let rendered = start.elapsed();
        let levels = note.levels(crate::NOTE_MAX_SPAN as usize * 48, 280, 20.0, 20_000.0);
        println!(
            "render {rendered:?}, levels {:?}",
            start.elapsed() - rendered
        );
        assert_eq!(note.samples().len(), 480_000);
        assert_eq!(levels.len(), 480 * 280);
        Ok(())
    }
}
