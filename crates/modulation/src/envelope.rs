//! [`Adsr`]: an attack, decay, sustain and release envelope of exponential one-pole segments
//! (`docs/architecture/instruments.md` "Synthesizer", ADR-0148).

use dsp::smoothing_coefficient;

/// How far past the peak the attack aims, as a multiple of the peak, so that it reaches the peak
/// in a finite time with the rounded-off shape of a charging capacitor.
const ATTACK_OVERSHOOT: f32 = 1.3;
/// The share of the remaining distance a decay or release leaves after its set time: 1%, about
/// −40 dB.
const SEGMENT_REMAINDER: f32 = 0.01;
/// The level below which a release has finished and the envelope rests at 0.
const IDLE_LEVEL: f32 = 1e-5;
/// How close a decay must come to the sustain level to count as sustaining.
const SUSTAIN_TOLERANCE: f32 = 1e-4;

/// Where an [`Adsr`] is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    /// At rest at 0.
    #[default]
    Idle,
    /// Rising to the peak.
    Attack,
    /// Falling from the peak to the sustain level.
    Decay,
    /// Holding at the sustain level while the gate is on.
    Sustain,
    /// Falling to 0 after the gate went off.
    Release,
}

/// An ADSR envelope: each segment is a one-pole step towards a target, so a new gate starts its
/// attack from wherever the level is, with no jump. Times are set in seconds; the level runs
/// from 0 to the peak given at the gate. Never allocates.
#[derive(Debug, Clone, PartialEq)]
pub struct Adsr {
    stage: Stage,
    level: f32,
    /// The level the attack reaches, set by [`Adsr::gate_on`].
    peak: f32,
    /// The sustain level as a share of the peak, 0 to 1.
    sustain: f32,
    attack: f32,
    decay: f32,
    release: f32,
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

impl Adsr {
    /// An idle envelope with instant segments and full sustain.
    pub fn new() -> Self {
        Adsr {
            stage: Stage::Idle,
            level: 0.0,
            peak: 1.0,
            sustain: 1.0,
            attack: 1.0,
            decay: 1.0,
            release: 1.0,
        }
    }

    /// Sets the segment times in seconds and the sustain level (a share of the peak, clamped to
    /// 0–1) at `sample_rate`. The attack time is how long a rise from 0 takes to reach the peak;
    /// the decay and release times are how long each takes to cover 99% of its distance. A time
    /// of 0 or less is instant. Takes effect from the next sample, in whatever stage.
    pub fn set(&mut self, attack: f32, decay: f32, sustain: f32, release: f32, sample_rate: f32) {
        let to_peak = (ATTACK_OVERSHOOT / (ATTACK_OVERSHOOT - 1.0)).ln();
        let to_remainder = (1.0 / SEGMENT_REMAINDER).ln();
        self.attack = smoothing_coefficient(attack / to_peak, sample_rate);
        self.decay = smoothing_coefficient(decay / to_remainder, sample_rate);
        self.release = smoothing_coefficient(release / to_remainder, sample_rate);
        self.sustain = if sustain.is_nan() {
            1.0
        } else {
            sustain.clamp(0.0, 1.0)
        };
    }

    /// Opens the gate: attacks from the current level to `peak` (clamped to 0–1; NaN is 1).
    pub fn gate_on(&mut self, peak: f32) {
        self.peak = if peak.is_nan() {
            1.0
        } else {
            peak.clamp(0.0, 1.0)
        };
        self.stage = Stage::Attack;
    }

    /// Closes the gate: releases from the current level. Does nothing while idle.
    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    /// Returns to idle at 0 at once.
    pub fn reset(&mut self) {
        self.stage = Stage::Idle;
        self.level = 0.0;
    }

    /// Advances one sample and returns the new level.
    pub fn next_value(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                let target = self.peak * ATTACK_OVERSHOOT;
                self.level += self.attack * (target - self.level);
                if self.level >= self.peak {
                    self.level = self.peak;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay | Stage::Sustain => {
                let target = self.peak * self.sustain;
                self.level += self.decay * (target - self.level);
                if (self.level - target).abs() < SUSTAIN_TOLERANCE {
                    self.level = target;
                    self.stage = Stage::Sustain;
                } else {
                    self.stage = Stage::Decay;
                }
            }
            Stage::Release => {
                self.level -= self.release * self.level;
                if self.level < IDLE_LEVEL {
                    self.reset();
                }
            }
        }
        self.level
    }

    /// The level given for the latest sample.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// The stage.
    pub fn stage(&self) -> Stage {
        self.stage
    }

    /// Whether the envelope is at rest at 0.
    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    /// An envelope with `a`, `d`, `s`, `r`.
    fn envelope(a: f32, d: f32, s: f32, r: f32) -> Adsr {
        let mut env = Adsr::new();
        env.set(a, d, s, r, RATE);
        env
    }

    /// Samples until `env` leaves `stage`, at most `limit`.
    fn samples_in(env: &mut Adsr, stage: Stage, limit: usize) -> usize {
        (0..limit)
            .take_while(|_| {
                env.next_value();
                env.stage() == stage
            })
            .count()
            + 1
    }

    #[test]
    fn test_attack_reaches_the_peak_in_the_attack_time() {
        let mut env = envelope(0.01, 0.1, 0.5, 0.1);
        env.gate_on(1.0);
        let n = samples_in(&mut env, Stage::Attack, 10_000);
        assert!((n as f32 - 480.0).abs() <= 2.0, "{n}");
        assert_eq!(env.level(), 1.0);
    }

    #[test]
    fn test_decay_settles_on_sustain_and_release_covers_99_percent() {
        let mut env = envelope(0.0, 0.1, 0.5, 0.2);
        env.gate_on(0.8);
        env.next_value();
        assert_eq!(env.stage(), Stage::Decay);
        // After the decay time, 99% of the way from 0.8 to 0.4.
        for _ in 0..4_800 {
            env.next_value();
        }
        assert!(
            (env.level() - (0.4 + 0.004)).abs() < 1e-3,
            "{}",
            env.level()
        );
        for _ in 0..48_000 {
            env.next_value();
        }
        assert_eq!(env.stage(), Stage::Sustain);
        assert_eq!(env.level(), 0.4);
        env.gate_off();
        for _ in 0..9_600 {
            env.next_value();
        }
        assert!((env.level() - 0.004).abs() < 2e-4, "{}", env.level());
        for _ in 0..48_000 {
            env.next_value();
        }
        assert!(env.is_idle());
        assert_eq!(env.level(), 0.0);
    }

    #[test]
    fn test_gate_on_retriggers_from_the_current_level_without_a_jump() {
        let mut env = envelope(0.005, 0.05, 0.6, 0.3);
        env.gate_on(1.0);
        for _ in 0..4_800 {
            env.next_value();
        }
        env.gate_off();
        for _ in 0..2_000 {
            env.next_value();
        }
        let before = env.level();
        assert!(before > 0.05 && before < 0.6);
        env.gate_on(1.0);
        let after = env.next_value();
        assert!(
            after > before && after - before < 0.02,
            "{before} → {after}"
        );
        // And a lower peak on a retrigger still rises or falls smoothly.
        let mut previous = env.level();
        for _ in 0..4_800 {
            let level = env.next_value();
            assert!((level - previous).abs() < 0.02);
            previous = level;
        }
    }

    #[test]
    fn test_instant_times_and_gate_off_while_idle() {
        let mut env = envelope(0.0, 0.0, 0.25, 0.0);
        env.gate_off();
        assert!(env.is_idle());
        env.gate_on(1.0);
        assert_eq!(env.next_value(), 1.0);
        assert_eq!(env.next_value(), 0.25);
        env.gate_off();
        assert_eq!(env.next_value(), 0.0);
        assert!(env.is_idle());
    }

    #[test]
    fn test_set_clamps_sustain_and_peak() {
        let mut env = envelope(0.0, 0.0, 7.0, 0.0);
        env.gate_on(f32::NAN);
        env.next_value();
        assert_eq!(env.next_value(), 1.0);
        env.set(0.0, 0.0, -1.0, 0.0, RATE);
        env.next_value();
        assert_eq!(env.level(), 0.0);
        env.reset();
        assert!(env.is_idle());
    }
}
