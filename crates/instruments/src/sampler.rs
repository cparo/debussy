//! [`Sampler`]: plays the project's own audio files, mapped to notes by key-range zones
//! (`docs/architecture/instruments.md` "Sampler", ADR-0154).
//!
//! Each voice steps a fractional playhead through its zone's sample by the ratio of its note to
//! the zone's root key and reads it by 4-point Hermite interpolation, then scales it by the
//! amplitude envelope.

use std::sync::Arc;

use audio_buffers::Layout;
use dsp::{db_to_gain, hermite};
use midi::Kind;
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;
use crate::voices::{EnvelopeParameters, Pool, SUSTAIN_PEDAL, Sound};

/// The most voices a sampler plays at once; the `voices` parameter limits it further.
pub const SAMPLER_VOICES: usize = 16;
/// The pitch-bend value at rest, and its half range.
const BEND_CENTRE: f32 = 8192.0;

/// How a zone plays once its note starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    /// Plays to the end of the sample; note-off is ignored.
    OneShot,
    /// Releases through the amplitude envelope on note-off.
    Gated,
}

/// One zone: a sample played across a key range, at its own pitch on the root key.
#[derive(Debug, Clone)]
pub struct Zone {
    /// The lowest key the zone plays.
    pub low: u8,
    /// The highest key the zone plays.
    pub high: u8,
    /// The key at which the sample plays at its own pitch; it may lie outside the range.
    pub root: u8,
    /// One-shot or gated.
    pub playback: Playback,
    /// The choke group: a note in it fades out every other voice in it.
    pub choke: Option<u8>,
    /// Semitones the zone plays above (or below) the pitch its root gives it.
    pub tune: i8,
    /// The left channel, or the only one, at the project rate.
    pub left: Arc<[f32]>,
    /// The right channel, as long as `left`; `None` for a mono sample.
    pub right: Option<Arc<[f32]>>,
}

impl Zone {
    fn contains(&self, note: u8) -> bool {
        (self.low..=self.high).contains(&note)
    }
}

/// What a voice is started with: its zone, by index.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ZoneStart {
    zone: usize,
    gated: bool,
    choke: Option<u8>,
    /// The playback rate at no bend or tune: `2^((note − root)/12)`.
    ratio: f64,
}

/// The sampler's part of a voice: which zone it plays and where its playhead is.
#[derive(Debug, Clone, Default)]
struct Playhead {
    zone: usize,
    gated: bool,
    choke: Option<u8>,
    ratio: f64,
    /// The frame position in the sample.
    position: f64,
}

impl Sound for Playhead {
    type Start = ZoneStart;

    fn start(&mut self, _note: u8, _velocity: f32, start: ZoneStart) {
        self.zone = start.zone;
        self.gated = start.gated;
        self.choke = start.choke;
        self.ratio = start.ratio;
        self.position = 0.0;
    }

    fn release(&mut self) {}

    fn reset(&mut self) {
        self.position = 0.0;
    }

    fn gated(&self) -> bool {
        self.gated
    }
}

/// The value of `samples` at fractional frame `position`, by 4-point Hermite interpolation;
/// frames before the start or past the end read as zero.
fn read(samples: &[f32], position: f64) -> f32 {
    let at = |i: i64| {
        usize::try_from(i)
            .ok()
            .and_then(|i| samples.get(i))
            .copied()
            .unwrap_or(0.0)
    };
    let whole = position.floor();
    // Positions stay below the sample's length, far inside `i64`.
    #[allow(clippy::cast_possible_truncation)]
    let i = whole as i64;
    // A fraction from 0 to 1.
    #[allow(clippy::cast_possible_truncation)]
    let x = (position - whole) as f32;
    hermite(at(i - 1), at(i), at(i + 1), at(i + 2), x)
}

/// Every parameter, in the order they are declared.
struct Parameters {
    amp_env: EnvelopeParameters,
    amp_velocity: Parameter,
    tune: Parameter,
    bend_range: Parameter,
    voices: Parameter,
    gain: Parameter,
}

impl Parameters {
    fn new() -> Result<Self, Error> {
        Ok(Parameters {
            amp_env: EnvelopeParameters::new("amp", "amp envelope", [0.001, 0.1, 1.0, 0.1])?,
            amp_velocity: Parameter::new(
                ParameterInfo::new("amp velocity", 0.0, 1.0, 0.7).with_group("amp envelope"),
            )?,
            tune: Parameter::new(ParameterInfo::new("tune", -12.0, 12.0, 0.0).with_unit("st"))?,
            bend_range: Parameter::new(
                ParameterInfo::new("bend range", 0.0, 24.0, 2.0)
                    .with_unit("st")
                    .with_step(1.0),
            )?,
            voices: Parameter::new(
                ParameterInfo::new("voices", 1.0, 16.0, 16.0).with_scale(Scale::Integer),
            )?,
            gain: Parameter::new(ParameterInfo::new("gain", -48.0, 12.0, -6.0).with_unit("dB"))?,
        })
    }

    fn all(&self) -> Vec<Parameter> {
        let mut all: Vec<Parameter> = self.amp_env.all().into_iter().cloned().collect();
        all.extend([
            self.amp_velocity.clone(),
            self.tune.clone(),
            self.bend_range.clone(),
            self.voices.clone(),
            self.gain.clone(),
        ]);
        all
    }
}

/// The ramped values read once per block and stepped every sample.
struct Ramps {
    amp_velocity: Ramp,
    /// Tune plus bend, in semitones.
    pitch: Ramp,
    /// The output gain, linear.
    gain: Ramp,
}

/// The running state, made in `prepare`.
struct Running {
    sample_rate: f32,
    pool: Pool<Playhead>,
    ramps: Ramps,
    /// The pitch-bend position, −1 to 1.
    bend: f32,
}

/// The built-in sampler, kind id `"sampler"`: event input 0 (notes, on any channel) and stereo
/// audio output 0. See `docs/architecture/instruments.md` "Sampler".
pub struct Sampler {
    zones: Arc<[Zone]>,
    parameters: Parameters,
    running: Option<Running>,
}

impl Sampler {
    /// A sampler playing `zones`, the first zone containing a note playing it. With no zones it
    /// is silent. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new(zones: Arc<[Zone]>) -> Result<Self, Error> {
        Ok(Sampler {
            zones,
            parameters: Parameters::new()?,
            running: None,
        })
    }
}

/// The index a whole-number parameter value names, from 1 to `max`.
fn count(value: f32, max: usize) -> usize {
    // Rounded, non-negative and small; `as` saturates and NaN becomes 0.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = value.round().max(0.0) as usize;
    n.clamp(1, max)
}

impl Node for Sampler {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::event("notes")]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.all()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let rate = sample_rate.max(1) as f32;
        let p = &self.parameters;
        self.running = Some(Running {
            sample_rate: rate,
            pool: Pool::new(SAMPLER_VOICES, rate, Playhead::default),
            ramps: Ramps {
                amp_velocity: Ramp::new(p.amp_velocity.get(), sample_rate),
                pitch: Ramp::new(p.tune.get(), sample_rate),
                gain: Ramp::new(db_to_gain(p.gain.get()), sample_rate),
            },
            bend: 0.0,
        });
    }

    fn reset(&mut self) {
        if let Some(running) = self.running.as_mut() {
            running.pool.reset();
            running.bend = 0.0;
            let tune = self.parameters.tune.get();
            running.ramps.pitch.set_target_over(tune, 1);
            running.ramps.pitch.next_value();
        }
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        left.fill(0.0);
        right.fill(0.0);
        let p = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // Read once per block (ADR-0128).
        for voice in running.pool.voices.iter_mut() {
            p.amp_env.apply(&mut voice.amp, running.sample_rate);
        }
        let r = &mut running.ramps;
        r.amp_velocity.approach(p.amp_velocity.read());
        r.gain.approach(p.gain.read().map(db_to_gain));
        let tune = p.tune.get();
        let bend_range = p.bend_range.get();
        let limit = count(p.voices.get(), SAMPLER_VOICES);

        let mut events = inputs.events(0).peekable();
        for n in 0..left.len() {
            while let Some(event) = events.next_if(|e| e.frame as usize <= n) {
                match event.message.kind() {
                    Kind::NoteOn { note, velocity } if velocity > 0 => {
                        let Some(start) = zone_start(&self.zones, note) else {
                            continue;
                        };
                        if let Some(group) = start.choke {
                            choke(&mut running.pool, group);
                        }
                        let velocity = f32::from(velocity) / 127.0;
                        let amount = running.ramps.amp_velocity.target();
                        running
                            .pool
                            .note_on(note, velocity, limit, amount, true, start);
                    }
                    Kind::NoteOn { note, .. } | Kind::NoteOff { note, .. } => {
                        running.pool.note_off(note);
                    }
                    Kind::ControlChange { controller, value } if controller == SUSTAIN_PEDAL => {
                        running.pool.set_pedal(value >= 64);
                    }
                    Kind::PitchBend { value } => {
                        running.bend =
                            ((f32::from(value) - BEND_CENTRE) / BEND_CENTRE).clamp(-1.0, 1.0);
                    }
                    _ => {}
                }
            }
            let r = &mut running.ramps;
            r.pitch.set_target(tune + running.bend * bend_range);
            let shift = f64::from(r.pitch.next_value() / 12.0).exp2();
            let amount = r.amp_velocity.next_value();
            let gain = r.gain.next_value();
            let (mut l, mut rr) = (0.0, 0.0);
            for i in 0..running.pool.voices.len() {
                let voice = &mut running.pool.voices[i];
                if voice.is_free() {
                    continue;
                }
                let Some(zone) = self.zones.get(voice.sound.zone) else {
                    voice.reset();
                    continue;
                };
                let head = &mut voice.sound;
                // Samples are far shorter than 2^53 frames.
                #[allow(clippy::cast_precision_loss)]
                let ended = head.position >= zone.left.len() as f64;
                let (sl, sr) = if ended {
                    (0.0, 0.0)
                } else {
                    let sl = read(&zone.left, head.position);
                    let sr = zone.right.as_ref().map_or(sl, |r| read(r, head.position));
                    head.position += head.ratio * shift;
                    (sl, sr)
                };
                let amp = voice.amp.next_value();
                if ended && voice.pending.is_none() {
                    voice.reset();
                    continue;
                }
                let share = running.pool.step_fade(i, amount);
                l += sl * amp * share;
                rr += sr * amp * share;
            }
            left[n] = l * gain;
            right[n] = rr * gain;
        }
    }
}

/// Fades out every sounding voice in choke group `group`, and drops a note in it waiting for a
/// stolen voice.
fn choke(pool: &mut Pool<Playhead>, group: u8) {
    for i in 0..pool.voices.len() {
        let voice = &pool.voices[i];
        let waiting = voice.pending.is_some_and(|p| p.start.choke == Some(group));
        let sounding = voice.fade == 0 && voice.sound.choke == Some(group);
        if waiting || sounding {
            pool.fade_out(i);
        }
    }
}

/// The start of a note `note` on the first of `zones` containing it, if any.
fn zone_start(zones: &[Zone], note: u8) -> Option<ZoneStart> {
    let zone = zones.iter().position(|z| z.contains(note))?;
    let z = &zones[zone];
    let semitones = f64::from(i16::from(note) - i16::from(z.root) + i16::from(z.tune));
    Some(ZoneStart {
        zone,
        gated: z.playback == Playback::Gated,
        choke: z.choke,
        ratio: (semitones / 12.0).exp2(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_interpolates_and_reads_zero_outside() {
        let s = [0.0, 1.0, 2.0, 3.0];
        assert_eq!(read(&s, 1.0), 1.0);
        assert!((read(&s, 1.5) - 1.5).abs() < 1e-6);
        assert_eq!(read(&s, 10.0), 0.0);
        // Past the last frame, it falls towards the zero beyond.
        assert!(read(&s, 3.5).abs() < 3.0);
    }

    #[test]
    fn test_zone_start_takes_the_first_match_and_pitches_from_the_root() {
        let samples: Arc<[f32]> = Arc::from(vec![0.0; 4]);
        let zone = |low, high, root| Zone {
            low,
            high,
            root,
            playback: Playback::Gated,
            choke: None,
            tune: 0,
            left: Arc::clone(&samples),
            right: None,
        };
        let zones = [zone(60, 72, 60), zone(48, 72, 48)];
        let start = zone_start(&zones, 72).map(|s| (s.zone, s.ratio));
        assert_eq!(start, Some((0, 2.0)));
        assert_eq!(zone_start(&zones, 50).map(|s| s.zone), Some(1));
        assert!(zone_start(&zones, 20).is_none());
    }
}
