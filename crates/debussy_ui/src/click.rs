//! The click, the tempo and the meter (`docs/architecture/recording.md` "Click track",
//! `docs/architecture/project.md` "Musical time"): the engine's click kept in step with the
//! project's tempo and the machine's click preferences.

use std::sync::Arc;

use engine_bridge::{ClickSound, click_track, prepare_click};
use project_model::Tempo;
use project_model::tempo::{MAX_BPM, MIN_BPM};

use crate::app::App;
use crate::services::{ClickPreferences, MAX_CLICK_LEVEL_DB, MIN_CLICK_LEVEL_DB};

/// The click's state in the interface.
#[derive(Debug, Default)]
pub(crate) struct Click {
    /// The sound and level, as remembered on this machine.
    pub(crate) preferences: ClickPreferences,
    /// The sound prepared at the project rate, once prepared.
    samples: Option<(ClickSound, Arc<[f32]>)>,
    /// The sound and tempo the engine's click was last made from.
    sent: Option<(ClickSound, Tempo)>,
    /// The tempo being typed, in beats per minute.
    pub(crate) tempo_text: Option<String>,
    /// Bars of count-in before recording: 0, 1 or 2. Session state.
    pub(crate) count_in_bars: u32,
}

impl Click {
    /// The click with `preferences`, not yet sent to the engine.
    pub(crate) fn new(preferences: ClickPreferences) -> Click {
        Click {
            preferences,
            count_in_bars: DEFAULT_COUNT_IN_BARS,
            ..Click::default()
        }
    }

    /// Forgets what the engine was sent, so the next sync sends it again, as to a rebuilt
    /// engine.
    pub(crate) fn resend(&mut self) {
        self.sent = None;
    }
}

/// The count-in lengths offered, in bars.
pub(crate) const COUNT_IN_BARS: [u32; 3] = [0, 1, 2];
/// The count-in at launch, in bars.
const DEFAULT_COUNT_IN_BARS: u32 = 1;

/// `bpm` for display: up to two decimals, without trailing zeros.
pub(crate) fn format_bpm(bpm: f64) -> String {
    let text = format!("{bpm:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A typed tempo, rounded to 0.01 BPM, if it is a number within the limits.
pub(crate) fn parse_bpm(text: &str) -> Option<f64> {
    let bpm = text.trim().parse::<f64>().ok()?;
    let bpm = (bpm * 100.0).round() / 100.0;
    (bpm.is_finite() && (MIN_BPM..=MAX_BPM).contains(&bpm)).then_some(bpm)
}

impl App {
    /// Gives the engine a click made from the current sound and the project's tempo if either
    /// changed since it was last given one. A busy engine is tried again on the next update.
    pub(crate) fn sync_click(&mut self) {
        let sound = self.click.preferences.sound;
        let tempo = self.session.state().tempo;
        if self.click.sent == Some((sound, tempo)) {
            return;
        }
        let rate = self.sample_rate();
        let samples = match &self.click.samples {
            Some((prepared, samples)) if *prepared == sound => Arc::clone(samples),
            _ => match prepare_click(sound, rate) {
                Ok(samples) => {
                    self.click.samples = Some((sound, Arc::clone(&samples)));
                    samples
                }
                Err(error) => {
                    self.status = Some(error.to_string());
                    // Not retried until the sound or tempo changes.
                    self.click.sent = Some((sound, tempo));
                    return;
                }
            },
        };
        let click = match click_track(&samples, rate, &tempo) {
            Ok(click) => click,
            Err(error) => {
                self.status = Some(error.to_string());
                self.click.sent = Some((sound, tempo));
                return;
            }
        };
        match self.engine.set_click(click) {
            Ok(()) => self.click.sent = Some((sound, tempo)),
            Err(audio_engine::Error::Busy) => {}
            Err(error) => {
                self.status = Some(error.to_string());
                self.click.sent = Some((sound, tempo));
            }
        }
    }

    /// The count-in before recording, in frames: the chosen bars at the project's tempo.
    pub(crate) fn count_in_frames(&self) -> u64 {
        self.session
            .state()
            .tempo
            .bars_length(self.click.count_in_bars, self.sample_rate())
    }

    /// Whether recording is counting in.
    pub fn is_counting_in(&self) -> bool {
        self.engine.real_time_state() == audio_engine::TransportState::CountingIn
    }

    /// Chooses the count-in before recording, in bars (0, 1 or 2).
    pub(crate) fn set_count_in(&mut self, bars: u32) {
        if COUNT_IN_BARS.contains(&bars) {
            self.click.count_in_bars = bars;
        }
    }

    /// Chooses the click's sound and remembers it.
    pub(crate) fn set_click_sound(&mut self, sound: ClickSound) {
        self.click.preferences.sound = sound;
        self.save_click_preferences();
    }

    /// Sets the click's level while its slider moves; remembered on release.
    pub(crate) fn set_click_level(&mut self, db: f32) {
        let db = db.clamp(MIN_CLICK_LEVEL_DB, MAX_CLICK_LEVEL_DB);
        self.click.preferences.level_db = db;
        self.engine.set_click_level(db);
    }

    /// Remembers the click preferences on this machine.
    pub(crate) fn save_click_preferences(&mut self) {
        let result = self.services.set_click_preferences(self.click.preferences);
        self.report(result);
    }

    /// Records the typed tempo, if any, as one step.
    pub(crate) fn finish_tempo(&mut self) {
        let Some(text) = self.click.tempo_text.take() else {
            return;
        };
        let Some(bpm) = parse_bpm(&text) else {
            self.status = Some(format!(
                "{:?} is not a tempo: give {} to {} BPM.",
                text.trim(),
                format_bpm(MIN_BPM),
                format_bpm(MAX_BPM)
            ));
            return;
        };
        let tempo = Tempo {
            bpm,
            ..self.session.state().tempo
        };
        self.set_tempo(tempo);
    }

    /// Sets the meter's beats per bar, its beat unit, or both, as one step.
    pub(crate) fn set_meter(&mut self, beats_per_bar: Option<u8>, beat_unit: Option<u8>) {
        let current = self.session.state().tempo;
        self.set_tempo(Tempo {
            beats_per_bar: beats_per_bar.unwrap_or(current.beats_per_bar),
            beat_unit: beat_unit.unwrap_or(current.beat_unit),
            ..current
        });
    }

    /// Records `tempo` as one step; refused while playing.
    fn set_tempo(&mut self, tempo: Tempo) {
        if self.refuse_while_playing("change the tempo") {
            return;
        }
        let result = self.bridge.edit(&mut self.session, |s| s.set_tempo(tempo));
        self.report(result.map_err(|e| e.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bpm_drops_trailing_zeros() {
        assert_eq!(format_bpm(120.0), "120");
        assert_eq!(format_bpm(92.5), "92.5");
        assert_eq!(format_bpm(123.45), "123.45");
    }

    #[test]
    fn test_parse_bpm_rounds_and_checks_the_limits() {
        assert_eq!(parse_bpm(" 97.126 "), Some(97.13));
        assert_eq!(parse_bpm("20"), Some(20.0));
        assert_eq!(parse_bpm("300"), Some(300.0));
        assert_eq!(parse_bpm("19.99"), None);
        assert_eq!(parse_bpm("300.01"), None);
        assert_eq!(parse_bpm("fast"), None);
        assert_eq!(parse_bpm("NaN"), None);
    }
}
