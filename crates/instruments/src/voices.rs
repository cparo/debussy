//! A fixed pool of voices shared by the instruments: which voice a new note takes, stealing with
//! a short fade, note-off and the sustain pedal (`docs/architecture/instruments.md`
//! "Synthesizer", ADR-0148), and the amplitude-envelope parameters every instrument has.
//!
//! What a voice sounds like is the instrument's: a [`Sound`] the pool starts, releases and resets.

use modulation::{Adsr, EnvelopeStage};
use signal_graph::{Parameter, ParameterInfo, Scale};

use crate::error::Error;

/// How long a stolen voice fades out before its new note starts, in seconds.
pub(crate) const STEAL_FADE_SECONDS: f32 = 0.003;

/// The controller number of the sustain pedal.
pub(crate) const SUSTAIN_PEDAL: u8 = 64;

/// The instrument-specific part of a voice.
pub(crate) trait Sound {
    /// What a note-on carries to the sound besides its note and velocity, kept while a stolen
    /// voice fades.
    type Start: Copy;

    /// Starts `note` at `velocity` (0 to 1).
    fn start(&mut self, note: u8, velocity: f32, start: Self::Start);

    /// Releases the note's other envelopes, if any.
    fn release(&mut self);

    /// Returns to silence at once.
    fn reset(&mut self);

    /// Whether a note-off releases the note; `false` for a sound that plays to its end.
    fn gated(&self) -> bool {
        true
    }
}

/// A note waiting for its stolen voice to fade out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pending<T> {
    pub note: u8,
    pub velocity: f32,
    /// When the note was started, counted in notes started.
    pub age: u64,
    /// Whether its key is still down.
    pub held: bool,
    /// What the sound starts it with.
    pub start: T,
}

/// One voice: its sound, its amplitude envelope and the note it plays.
#[derive(Debug, Clone)]
pub(crate) struct Voice<S: Sound> {
    pub sound: S,
    pub amp: Adsr,
    pub note: u8,
    /// The strike velocity, 0 to 1.
    pub velocity: f32,
    /// When the note started, counted in notes started.
    pub age: u64,
    /// Whether the key is down.
    pub held: bool,
    /// Whether the key is up but the sustain pedal holds the note.
    pub sustained: bool,
    /// Samples left in a fade-out; 0 when not fading.
    pub fade: u32,
    /// The note to start when the fade ends; `None` to fall silent.
    pub pending: Option<Pending<S::Start>>,
}

impl<S: Sound> Voice<S> {
    fn new(sound: S) -> Self {
        Voice {
            sound,
            amp: Adsr::new(),
            note: 0,
            velocity: 0.0,
            age: 0,
            held: false,
            sustained: false,
            fade: 0,
            pending: None,
        }
    }

    /// Whether the voice is silent and free.
    pub fn is_free(&self) -> bool {
        self.amp.is_idle() && self.pending.is_none()
    }

    /// Returns to silence at once.
    pub fn reset(&mut self) {
        self.sound.reset();
        self.amp.reset();
        self.held = false;
        self.sustained = false;
        self.fade = 0;
        self.pending = None;
    }

    /// Starts `note` at `velocity` from the envelope's current level, peaking at `peak`.
    fn start(&mut self, note: u8, velocity: f32, peak: f32, age: u64, start: S::Start) {
        self.note = note;
        self.velocity = velocity;
        self.age = age;
        self.held = true;
        self.sustained = false;
        self.amp.gate_on(peak);
        self.sound.start(note, velocity, start);
    }

    /// Releases the note.
    pub fn release(&mut self) {
        self.held = false;
        self.sustained = false;
        self.amp.gate_off();
        self.sound.release();
    }
}

/// The amplitude envelope's peak for a note at `velocity` (0 to 1), with `amount` (0 to 1) of
/// velocity sensitivity.
pub(crate) fn peak(velocity: f32, amount: f32) -> f32 {
    1.0 - amount * (1.0 - velocity)
}

/// A fixed pool of voices, made ahead of the audio thread; nothing here allocates.
pub(crate) struct Pool<S: Sound> {
    pub voices: Box<[Voice<S>]>,
    /// Samples a fade lasts.
    fade: u32,
    /// Notes started so far, for the voices' ages.
    started: u64,
    /// Whether the sustain pedal is down.
    pedal: bool,
}

impl<S: Sound> Pool<S> {
    /// A pool of the voices `sound` makes, `count` of them, fading over
    /// [`STEAL_FADE_SECONDS`] at `sample_rate`. Allocates.
    pub fn new(count: usize, sample_rate: f32, mut sound: impl FnMut() -> S) -> Self {
        // A few hundred samples at most.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let fade = (STEAL_FADE_SECONDS * sample_rate).ceil().max(1.0) as u32;
        Pool {
            voices: (0..count).map(|_| Voice::new(sound())).collect(),
            fade,
            started: 0,
            pedal: false,
        }
    }

    /// The voice for a new note `note`: the voice already playing it, else a free voice among
    /// the first `limit`, else the quietest released one, else the oldest held one. The second
    /// value says whether the voice must be stolen.
    pub fn choose(&self, note: u8, limit: usize) -> Option<(usize, bool)> {
        let playing = |v: &Voice<S>| !v.is_free() && v.fade == 0;
        if let Some(i) = self
            .voices
            .iter()
            .position(|v| playing(v) && v.note == note)
        {
            return Some((i, false));
        }
        let candidates = || self.voices.iter().enumerate().take(limit);
        if let Some((i, _)) = candidates().find(|(_, v)| v.is_free()) {
            return Some((i, false));
        }
        let quietest_released = candidates()
            .filter(|(_, v)| v.fade == 0 && v.amp.stage() == EnvelopeStage::Release)
            .min_by(|a, b| a.1.amp.level().total_cmp(&b.1.amp.level()));
        if let Some((i, _)) = quietest_released {
            return Some((i, true));
        }
        candidates()
            .filter(|(_, v)| v.fade == 0)
            .min_by_key(|(_, v)| v.age)
            .or_else(|| candidates().min_by_key(|(_, v)| v.fade))
            .map(|(i, _)| (i, true))
    }

    /// Handles a note-on of `note` at `velocity` (0 to 1) among the first `limit` voices, with
    /// `amount` of velocity sensitivity. With `retrigger_fades`, a voice already playing the note
    /// fades out before it restarts, as a stolen one does; otherwise it restarts from its
    /// envelope's current level.
    pub fn note_on(
        &mut self,
        note: u8,
        velocity: f32,
        limit: usize,
        amount: f32,
        retrigger_fades: bool,
        start: S::Start,
    ) {
        let Some((i, steal)) = self.choose(note, limit) else {
            return;
        };
        self.started += 1;
        let voice = &mut self.voices[i];
        let retrigger = !voice.is_free() && voice.note == note;
        if steal || (retrigger && retrigger_fades) {
            voice.fade = self.fade;
            voice.pending = Some(Pending {
                note,
                velocity,
                age: self.started,
                held: true,
                start,
            });
        } else {
            voice.start(note, velocity, peak(velocity, amount), self.started, start);
        }
    }

    /// Handles a note-off. A sound that is not [gated](Sound::gated) ignores it.
    pub fn note_off(&mut self, note: u8) {
        let pedal = self.pedal;
        for voice in self.voices.iter_mut() {
            if let Some(pending) = voice.pending.as_mut().filter(|p| p.note == note && p.held) {
                pending.held = false;
            }
            if voice.held && voice.note == note && voice.fade == 0 {
                if pedal {
                    voice.held = false;
                    voice.sustained = true;
                } else if voice.sound.gated() {
                    voice.release();
                } else {
                    voice.held = false;
                }
            }
        }
    }

    /// Handles the sustain pedal going down (`true`) or up.
    pub fn set_pedal(&mut self, down: bool) {
        self.pedal = down;
        if !down {
            for voice in self.voices.iter_mut().filter(|v| v.sustained) {
                if voice.sound.gated() {
                    voice.release();
                } else {
                    voice.sustained = false;
                }
            }
        }
    }

    /// The gain of voice `i` for this sample, 1 unless it is fading, and advances its fade; a
    /// fade that ends starts its pending note from silence, or frees the voice.
    pub fn step_fade(&mut self, i: usize, amount: f32) -> f32 {
        let total = self.fade;
        let Some(voice) = self.voices.get_mut(i).filter(|v| v.fade > 0) else {
            return 1.0;
        };
        #[allow(clippy::cast_precision_loss)] // a few hundred samples
        let share = voice.fade as f32 / total as f32;
        voice.fade -= 1;
        if voice.fade == 0 {
            let pending = voice.pending.take();
            voice.reset();
            if let Some(p) = pending {
                voice.start(p.note, p.velocity, peak(p.velocity, amount), p.age, p.start);
                if !p.held {
                    voice.release();
                }
            }
        }
        share
    }

    /// Every voice falls silent at once and the pedal lifts.
    pub fn reset(&mut self) {
        for voice in self.voices.iter_mut() {
            voice.reset();
        }
        self.pedal = false;
    }
}

/// An envelope's parameters, named `prefix` and grouped as `group`.
pub(crate) struct EnvelopeParameters {
    pub attack: Parameter,
    pub decay: Parameter,
    pub sustain: Parameter,
    pub release: Parameter,
}

impl EnvelopeParameters {
    /// The four parameters, at `defaults`: attack, decay and release in seconds (1 ms to 10 s),
    /// and sustain (0 to 1).
    pub fn new(prefix: &str, group: &str, defaults: [f32; 4]) -> Result<Self, Error> {
        let time = |name: &str, default| {
            Parameter::new(
                ParameterInfo::new(format!("{prefix} {name}"), 0.001, 10.0, default)
                    .with_unit("s")
                    .with_scale(Scale::Logarithmic)
                    .with_group(group),
            )
        };
        Ok(EnvelopeParameters {
            attack: time("attack", defaults[0])?,
            decay: time("decay", defaults[1])?,
            sustain: Parameter::new(
                ParameterInfo::new(format!("{prefix} sustain"), 0.0, 1.0, defaults[2])
                    .with_group(group),
            )?,
            release: time("release", defaults[3])?,
        })
    }

    pub fn all(&self) -> [&Parameter; 4] {
        [&self.attack, &self.decay, &self.sustain, &self.release]
    }

    /// Sets `env`'s times and sustain from the parameters, at `sample_rate`.
    pub fn apply(&self, env: &mut Adsr, sample_rate: f32) {
        env.set(
            self.attack.get(),
            self.decay.get(),
            self.sustain.get(),
            self.release.get(),
            sample_rate,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sound that only records whether it is gated.
    #[derive(Debug, Clone, Default)]
    struct Probe {
        gated: bool,
        starts: u32,
    }

    impl Sound for Probe {
        type Start = bool;

        fn start(&mut self, _note: u8, _velocity: f32, gated: bool) {
            self.gated = gated;
            self.starts += 1;
        }

        fn release(&mut self) {}

        fn reset(&mut self) {}

        fn gated(&self) -> bool {
            self.gated
        }
    }

    fn pool() -> Pool<Probe> {
        let mut pool = Pool::new(4, 48_000.0, Probe::default);
        for v in pool.voices.iter_mut() {
            v.amp.set(0.0, 0.0, 1.0, 0.5, 48_000.0);
        }
        pool
    }

    #[test]
    fn test_choose_prefers_same_note_then_free_then_quietest_released_then_oldest() {
        let mut p = pool();
        for note in [60, 62, 64] {
            p.note_on(note, 1.0, 3, 0.7, false, true);
        }
        // A repeated note takes its own voice.
        assert_eq!(p.choose(62, 3), Some((1, false)));
        // All three held: the oldest (note 60) is stolen.
        assert_eq!(p.choose(65, 3), Some((0, true)));
        for v in p.voices.iter_mut() {
            v.amp.next_value();
        }
        // Release 64 and 62; let 62 fall further: it is the quietest released.
        p.note_off(62);
        for _ in 0..2_000 {
            p.voices[1].amp.next_value();
        }
        p.note_off(64);
        p.voices[2].amp.next_value();
        assert_eq!(p.choose(65, 3), Some((1, true)));
        // A higher voice limit offers a free voice first.
        assert_eq!(p.choose(65, 4), Some((3, false)));
    }

    #[test]
    fn test_sustain_pedal_holds_released_notes_until_lifted() {
        let mut p = pool();
        p.note_on(60, 1.0, 4, 0.0, false, true);
        p.voices[0].amp.next_value();
        p.set_pedal(true);
        p.note_off(60);
        assert!(p.voices[0].sustained);
        assert_ne!(p.voices[0].amp.stage(), EnvelopeStage::Release);
        p.set_pedal(false);
        assert_eq!(p.voices[0].amp.stage(), EnvelopeStage::Release);
    }

    #[test]
    fn test_note_off_leaves_an_ungated_voice_playing() {
        let mut p = pool();
        p.note_on(60, 1.0, 4, 0.0, false, false);
        p.voices[0].amp.next_value();
        p.note_off(60);
        assert!(!p.voices[0].held);
        assert_ne!(p.voices[0].amp.stage(), EnvelopeStage::Release);
    }

    #[test]
    fn test_retrigger_fades_then_restarts() {
        let mut p = pool();
        p.note_on(60, 1.0, 4, 0.0, true, true);
        p.voices[0].amp.next_value();
        p.note_on(60, 0.5, 4, 0.0, true, true);
        assert!(p.voices[0].fade > 0);
        while p.voices[0].fade > 0 {
            p.step_fade(0, 0.0);
        }
        assert_eq!(p.voices[0].sound.starts, 2);
        assert_eq!(p.voices[0].velocity, 0.5);
    }
}
