//! The bundled click sounds, prepared for the engine (`docs/architecture/recording.md` "Click
//! track", ADR-0083).
//!
//! Each sound is a WAV compiled in from `assets/clicks/`. [`prepare`] decodes it, folds it to
//! mono, converts it to the project rate, peak-normalises it to −1 dBFS and trims the tail
//! below −60 dBFS. [`click_track`] puts a prepared sound on the project's beat grid.

use std::io::Cursor;
use std::sync::Arc;

use audio_engine::ClickTrack;
use dsp::{FOLD_DOWN_GAIN, Quality, Resampler};
use project_model::Tempo;
use wav_files::{Channels, WavReader};

use crate::error::Error;

/// The peak a prepared sound is normalised to: −1 dBFS.
const PEAK: f32 = 0.891_250_9;
/// The level below which a prepared sound's tail is trimmed: −60 dBFS.
const TAIL: f32 = 0.001;
/// The longest sound decoded, in frames: ten seconds at 48 kHz.
const MAX_FRAMES: u64 = 480_000;

/// One of the bundled click sounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClickSound {
    /// A wood block.
    Block,
    /// A click.
    #[default]
    Click,
    /// A drumstick.
    Stick,
}

impl ClickSound {
    /// Every sound, in menu order.
    pub const ALL: [ClickSound; 3] = [ClickSound::Block, ClickSound::Click, ClickSound::Stick];

    /// The sound's name, for display.
    pub fn name(self) -> &'static str {
        match self {
            ClickSound::Block => "Block",
            ClickSound::Click => "Click",
            ClickSound::Stick => "Stick",
        }
    }

    /// The compiled-in WAV.
    fn wav(self) -> &'static [u8] {
        match self {
            ClickSound::Block => include_bytes!("../assets/clicks/block.wav"),
            ClickSound::Click => include_bytes!("../assets/clicks/click.wav"),
            ClickSound::Stick => include_bytes!("../assets/clicks/stick.wav"),
        }
    }
}

impl std::fmt::Display for ClickSound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// `sound`, ready for the engine at `rate` Hz: mono, normalised and trimmed. Call it off the
/// real-time thread.
///
/// # Errors
///
/// [`Error::ClickSound`] if the sound cannot be decoded or converted.
pub fn prepare(sound: ClickSound, rate: u32) -> Result<Arc<[f32]>, Error> {
    prepare_wav(sound.wav(), rate).map_err(|reason| Error::ClickSound {
        sound: sound.name(),
        reason,
    })
}

/// A click playing `prepared` (from [`prepare`]) on `tempo`'s beats at `rate` Hz.
///
/// # Errors
///
/// [`Error::Engine`] if the engine refuses it, as for an empty sound.
pub fn click_track(prepared: &Arc<[f32]>, rate: u32, tempo: &Tempo) -> Result<ClickTrack, Error> {
    Ok(ClickTrack::new(
        Arc::clone(prepared),
        tempo.frames_per_beat(rate),
        u32::from(tempo.beats_per_bar),
    )?)
}

/// Decodes, folds, converts, normalises and trims the WAV in `bytes` for `rate` Hz.
fn prepare_wav(bytes: &[u8], rate: u32) -> Result<Arc<[f32]>, String> {
    let mut reader = WavReader::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let frames = reader.frames().min(MAX_FRAMES) as usize;
    let mut mono = match spec.channels {
        Channels::Mono => {
            let mut samples = vec![0.0; frames];
            read_all(&mut reader, &mut [&mut samples])?;
            samples
        }
        Channels::Stereo => {
            let (mut left, mut right) = (vec![0.0; frames], vec![0.0; frames]);
            read_all(&mut reader, &mut [&mut left, &mut right])?;
            left.iter()
                .zip(&right)
                .map(|(l, r)| (l + r) * FOLD_DOWN_GAIN)
                .collect()
        }
    };
    if spec.sample_rate != rate {
        mono = resample(&mono, spec.sample_rate, rate)?;
    }
    let peak = mono.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
    if peak <= 0.0 || !peak.is_finite() {
        return Err("the sound is silent".to_owned());
    }
    let gain = PEAK / peak;
    for sample in &mut mono {
        *sample *= gain;
    }
    let end = mono
        .iter()
        .rposition(|s| s.abs() >= TAIL)
        .map_or(0, |i| i + 1);
    mono.truncate(end);
    Ok(mono.into())
}

/// Reads every frame into `channels`, which are as long as the file.
fn read_all(
    reader: &mut WavReader<Cursor<&[u8]>>,
    channels: &mut [&mut [f32]],
) -> Result<(), String> {
    let frames = channels.first().map_or(0, |c| c.len());
    let mut done = 0;
    while done < frames {
        let mut rest: Vec<&mut [f32]> = channels.iter_mut().map(|c| &mut c[done..]).collect();
        let read = reader.read(&mut rest).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        done += read;
    }
    Ok(())
}

/// `input` at `from` Hz converted to `to` Hz at import quality, with the converter's delay
/// removed and its tail kept.
fn resample(input: &[f32], from: u32, to: u32) -> Result<Vec<f32>, String> {
    let mut resampler = Resampler::new(from, to, Quality::Import).map_err(|e| e.to_string())?;
    let skip = resampler.latency().round() as usize;
    let wanted = (input.len() as u64 * u64::from(to)).div_ceil(u64::from(from)) as usize;
    let mut output = vec![0.0; wanted + skip];
    let (mut consumed, mut produced) = (0, 0);
    let silence = vec![0.0; resampler.taps() + 1];
    while produced < output.len() {
        let (c, p) = if consumed < input.len() {
            resampler.process(&input[consumed..], &mut output[produced..])
        } else {
            // Flush the converter's tail with silence.
            resampler.process(&silence, &mut output[produced..])
        };
        consumed += c;
        produced += p;
        if c == 0 && p == 0 {
            break;
        }
    }
    output.drain(..skip.min(output.len()));
    output.truncate(wanted);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wav_files::{SampleFormat, Spec, WavWriter};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A WAV of `channels` at `rate` Hz.
    fn wav(rate: u32, channels: &[&[f32]]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let spec = Spec {
            channels: if channels.len() == 1 {
                Channels::Mono
            } else {
                Channels::Stereo
            },
            sample_rate: rate,
            format: SampleFormat::Float32,
        };
        let mut writer = WavWriter::new(Cursor::new(Vec::new()), spec)?;
        writer.write(channels)?;
        Ok(writer.finish()?.into_inner())
    }

    /// A decaying 1 kHz tone of `frames` at `rate` Hz, peaking at `peak`.
    fn tone(frames: usize, rate: u32, peak: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                let t = i as f32 / rate as f32;
                peak * (-t * 200.0).exp() * (std::f32::consts::TAU * 1000.0 * t).sin()
            })
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |p, s| p.max(s.abs()))
    }

    #[test]
    fn test_prepare_wav_normalises_to_minus_one_dbfs_and_trims_the_tail() -> TestResult {
        let mut quiet = tone(4_800, 48_000, 0.1);
        quiet.extend(std::iter::repeat_n(0.0, 4_800));
        let prepared = prepare_wav(&wav(48_000, &[&quiet])?, 48_000)?;
        assert!((peak(&prepared) - PEAK).abs() < 1e-6);
        // The silence and the decay below −60 dBFS are gone.
        assert!(prepared.len() < 4_800);
        assert!(prepared.last().is_some_and(|s| s.abs() >= TAIL));
        Ok(())
    }

    #[test]
    fn test_prepare_wav_converts_the_rate_and_folds_stereo() -> TestResult {
        let left = tone(4_410, 44_100, 0.5);
        let right: Vec<f32> = left.iter().map(|s| -s).collect();
        // Opposite channels cancel when folded.
        assert!(prepare_wav(&wav(44_100, &[&left, &right])?, 48_000).is_err());
        let prepared = prepare_wav(&wav(44_100, &[&left, &left])?, 48_000)?;
        // The tone starts at once: the converter's delay is removed.
        let onset = prepared
            .iter()
            .position(|s| s.abs() > 0.1)
            .unwrap_or(usize::MAX);
        assert!(onset < 20, "onset at {onset}");
        assert!(prepared.len() <= 4_800);
        assert!((peak(&prepared) - PEAK).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_prepare_every_bundled_sound_at_every_rate() -> TestResult {
        for sound in ClickSound::ALL {
            for rate in [44_100, 48_000, 88_200, 96_000] {
                let prepared = prepare(sound, rate)?;
                assert!(!prepared.is_empty(), "{sound} at {rate}");
                assert!((peak(&prepared) - PEAK).abs() < 1e-6);
                // Trimmed to well under half a second.
                assert!(prepared.len() < (rate / 2) as usize, "{sound} at {rate}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_click_track_follows_the_tempo() -> TestResult {
        let prepared = prepare(ClickSound::Stick, 48_000)?;
        assert!(click_track(&prepared, 48_000, &Tempo::default()).is_ok());
        let empty: Arc<[f32]> = Arc::from(Vec::new());
        assert!(click_track(&empty, 48_000, &Tempo::default()).is_err());
        Ok(())
    }
}
