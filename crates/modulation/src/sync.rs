//! Tempo-sync arithmetic: note values and feels as lengths in frames, for the delay's time and
//! an LFO's period (`docs/architecture/modulation.md` "LFO", ADR-0118).

/// The feels' names, by index: straight (×1), dotted (×1.5) and triplet (×2/3). The order is
/// saved format.
pub const FEELS: [&str; 3] = ["straight", "dotted", "triplet"];

/// An LFO period's note values, by index: each is twice the one before, from a 16th note to four
/// whole notes. The order is saved format.
pub const LFO_NOTES: [&str; 7] = ["1/16", "1/8", "1/4", "1/2", "1/1", "2/1", "4/1"];

/// The factor of feel `feel`, an index into [`FEELS`] rounded and clamped: ×1, ×1.5 or ×2/3.
fn feel_factor(feel: f32) -> f64 {
    if feel >= 1.5 {
        2.0 / 3.0
    } else if feel >= 0.5 {
        1.5
    } else {
        1.0
    }
}

/// `2^note / 2^shift` of a whole note, `note` rounded and clamped to `0..=max`.
fn note_fraction(note: f32, max: f32, shift: u32) -> f64 {
    // Held in a small range first, so the cast is exact.
    #[allow(clippy::cast_possible_truncation)]
    let note = note.round().clamp(0.0, max) as i32;
    f64::from(2_i32.pow(note.unsigned_abs())) / f64::from(2_i32.pow(shift))
}

/// The length of a synced delay, in frames: note value `note` (an index into the delay's notes,
/// 1/32 (0) to 1/1 (5), rounded and clamped) times feel `feel` (an index into [`FEELS`]), for a
/// whole note of `frames_per_whole_note` frames. Not clamped to any buffer.
pub fn synced_frames(frames_per_whole_note: f64, note: f32, feel: f32) -> f64 {
    frames_per_whole_note * note_fraction(note, 5.0, 5) * feel_factor(feel)
}

/// The period of a synced LFO, in frames: note value `note` (an index into [`LFO_NOTES`],
/// rounded and clamped) times feel `feel` (an index into [`FEELS`]), for a whole note of
/// `frames_per_whole_note` frames.
pub fn period_frames(frames_per_whole_note: f64, note: f32, feel: f32) -> f64 {
    frames_per_whole_note * note_fraction(note, 6.0, 4) * feel_factor(feel)
}

/// The whole-note length to run at for `frames_per_whole_note` at `sample_rate`: the length
/// itself if it is positive, or 120 BPM in 4/4, two seconds, if it is not or none is given.
pub fn whole_note(frames_per_whole_note: Option<f64>, sample_rate: u32) -> f64 {
    frames_per_whole_note
        .filter(|f| f.is_finite() && *f > 0.0)
        .unwrap_or(2.0 * f64::from(sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synced_frames_follows_note_and_feel() {
        // 120 BPM in 4/4 at 48 kHz: a whole note is 96 000 frames.
        let whole = 96_000.0;
        assert_eq!(synced_frames(whole, 2.0, 0.0), 12_000.0);
        assert_eq!(synced_frames(whole, 0.0, 0.0), 3_000.0);
        assert_eq!(synced_frames(whole, 5.0, 0.0), 96_000.0);
        assert_eq!(synced_frames(whole, 2.0, 1.0), 18_000.0);
        assert_eq!(synced_frames(whole, 3.0, 2.0), 16_000.0);
        // Out-of-range indices are clamped.
        assert_eq!(synced_frames(whole, 9.0, 7.0), 64_000.0);
    }

    #[test]
    fn test_synced_frames_in_six_eight() {
        // 6/8 at 120 eighth notes per minute, 48 kHz: 24 000 frames a beat, beat unit 8.
        let whole = 24_000.0 * 8.0;
        assert_eq!(synced_frames(whole, 2.0, 0.0), 24_000.0);
        assert_eq!(synced_frames(whole, 2.0, 1.0), 36_000.0);
    }

    #[test]
    fn test_period_frames_spans_sixteenth_to_four_wholes() {
        let whole = 96_000.0;
        assert_eq!(period_frames(whole, 0.0, 0.0), 6_000.0);
        assert_eq!(period_frames(whole, 4.0, 0.0), 96_000.0);
        assert_eq!(period_frames(whole, 6.0, 0.0), 384_000.0);
        assert_eq!(period_frames(whole, 4.0, 1.0), 144_000.0);
        assert_eq!(period_frames(whole, 3.0, 2.0), 32_000.0);
        assert_eq!(period_frames(whole, -2.0, 0.0), 6_000.0);
        assert_eq!(period_frames(whole, 40.0, 0.0), 384_000.0);
        assert_eq!(period_frames(whole, f32::NAN, f32::NAN), 6_000.0);
        assert_eq!(LFO_NOTES.len(), 7);
    }

    #[test]
    fn test_whole_note_falls_back_to_two_seconds() {
        assert_eq!(whole_note(Some(192_000.0), 48_000), 192_000.0);
        assert_eq!(whole_note(None, 48_000), 96_000.0);
        assert_eq!(whole_note(Some(0.0), 48_000), 96_000.0);
        assert_eq!(whole_note(Some(-5.0), 44_100), 88_200.0);
        assert_eq!(whole_note(Some(f64::NAN), 48_000), 96_000.0);
        assert_eq!(whole_note(Some(f64::INFINITY), 48_000), 96_000.0);
    }
}
