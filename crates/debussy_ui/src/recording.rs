//! Arming tracks, monitoring and recording passes (`docs/architecture/recording.md`,
//! `docs/architecture/user-interface.md` "Recording"). Arm, monitor and input settings belong to
//! the session and are not saved (ADR-0079).

use std::collections::BTreeMap;

use audio_buffers::Layout;
use audio_engine::ArmedTrack;
use project_model::{AudioFile, Clip, Source, TrackId};

use crate::app::App;
use crate::services::{TakeFiles, TakeStatus};

/// The quietest level an input meter shows, in dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;
/// How far a meter falls per display frame when the input drops, in dB.
const METER_FALL_DB: f32 = 1.5;

/// One armed track's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Arming {
    /// Whether its input is heard through the graph.
    pub(crate) monitor: bool,
    /// The device input channel (from 0) of its first channel; a stereo track takes the next one
    /// for its second.
    pub(crate) input: usize,
}

/// A recording pass whose files are still being written.
pub(crate) struct Pass {
    /// Each armed track, its layout and its files, in the order the files are numbered.
    tracks: Vec<(TrackId, Layout, Vec<String>)>,
    /// What became of each file, by number.
    status: Vec<Option<Result<u64, String>>>,
    /// The device latency when the pass started.
    latency: u64,
}

impl Pass {
    fn files(&self) -> Vec<String> {
        self.tracks.iter().flat_map(|(_, _, f)| f.clone()).collect()
    }
}

/// An input choice for a track, shown in its header: `In 3`, or `In 3–4` for a stereo track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputChoice {
    /// The device input channel (from 0) of the track's first channel.
    pub input: usize,
    /// Whether the track is stereo.
    pub stereo: bool,
}

impl std::fmt::Display for InputChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.stereo {
            write!(f, "In {}–{}", self.input + 1, self.input + 2)
        } else {
            write!(f, "In {}", self.input + 1)
        }
    }
}

impl App {
    /// The track's arm settings, if it is armed.
    pub fn arming(&self, track: TrackId) -> Option<(bool, InputChoice)> {
        let arming = self.arming.get(&track)?;
        let stereo = self.track_layout(track) == Some(Layout::Stereo);
        Some((
            arming.monitor,
            InputChoice {
                input: arming.input,
                stereo,
            },
        ))
    }

    /// The inputs a track can record from on the device in use: each channel, or each pair
    /// (1–2, 3–4, …) for a stereo track.
    pub fn input_choices(&self, track: TrackId) -> Vec<InputChoice> {
        let stereo = self.track_layout(track) == Some(Layout::Stereo);
        let step = if stereo { 2 } else { 1 };
        (0..self.inputs.saturating_sub(step - 1))
            .step_by(step)
            .map(|input| InputChoice { input, stereo })
            .collect()
    }

    /// The armed track's input meter: one level per channel in dBFS, from
    /// [`METER_FLOOR_DB`] to 0 and above when clipping. Empty until the first frame is measured.
    pub fn input_levels(&self, track: TrackId) -> &[f32] {
        self.meters.get(&track).map_or(&[], Vec::as_slice)
    }

    /// Reads the engine's input peaks for the armed tracks' meters: each meter jumps up to a
    /// louder peak and falls [`METER_FALL_DB`] per frame otherwise. Called each display frame.
    pub(crate) fn update_meters(&mut self) {
        let mut peaks: BTreeMap<usize, f32> = BTreeMap::new();
        let mut meters = BTreeMap::new();
        for (id, arming) in &self.arming {
            let channels = self.track_layout(*id).map_or(0, Layout::channels);
            let old = self.meters.get(id);
            let levels = (0..channels)
                .map(|c| {
                    let input = arming.input + c;
                    let peak = *peaks
                        .entry(input)
                        .or_insert_with(|| self.engine.take_input_peak(input));
                    let db = if peak > 0.0 {
                        (20.0 * peak.log10()).max(METER_FLOOR_DB)
                    } else {
                        METER_FLOOR_DB
                    };
                    let fallen = old
                        .and_then(|o| o.get(c))
                        .map_or(METER_FLOOR_DB, |l| (l - METER_FALL_DB).max(METER_FLOOR_DB));
                    db.max(fallen)
                })
                .collect();
            meters.insert(*id, levels);
        }
        self.meters = meters;
    }

    /// Whether the transport is recording.
    pub fn is_recording(&self) -> bool {
        self.engine.is_recording()
    }

    /// Whether Record can start now: stopped or paused, a track armed, the device's inputs open
    /// and the last pass saved.
    pub fn can_record(&self) -> bool {
        !self.engine.is_playing()
            && !self.arming.is_empty()
            && self.inputs > 0
            && self.pass.is_none()
    }

    /// Whether a recording pass's files are still being written.
    pub(crate) fn recording_pending(&self) -> bool {
        self.pass.is_some()
    }

    /// Arms the track, defaulting to input 1 (1–2 for stereo) without monitoring, or disarms
    /// it. Opens the device's inputs with the first armed track and closes them with the last.
    pub(crate) fn toggle_arm(&mut self, track: TrackId) {
        if self.refuse_while_playing("arm a track") {
            return;
        }
        if self.arming.remove(&track).is_none() {
            self.arming.insert(
                track,
                Arming {
                    monitor: false,
                    input: 0,
                },
            );
        }
        self.sync_arming();
    }

    /// Turns the armed track's monitoring on or off.
    pub(crate) fn toggle_monitor(&mut self, track: TrackId) {
        if self.refuse_while_playing("change monitoring") {
            return;
        }
        if let Some(arming) = self.arming.get_mut(&track) {
            arming.monitor = !arming.monitor;
            self.sync_arming();
        }
    }

    /// Sets the armed track's input.
    pub(crate) fn set_input(&mut self, track: TrackId, input: usize) {
        if self.refuse_while_playing("change a track's input") {
            return;
        }
        if let Some(arming) = self.arming.get_mut(&track) {
            arming.input = input;
            self.sync_arming();
        }
    }

    /// Brings the device and the engine in step with the arm settings, dropping tracks that no
    /// longer exist. Only while stopped or paused.
    pub(crate) fn sync_arming(&mut self) {
        let state = self.session.state();
        self.arming.retain(|id, _| state.track(*id).is_some());
        let want_input = !self.arming.is_empty();
        if want_input != self.input_open {
            let result = self.services.set_input(want_input);
            self.input_open = want_input;
            self.inputs = self.services.input_channels();
            if let Err(message) = result {
                self.status = Some(message);
            }
        }
        let armed = self.armed_tracks(None);
        if let Err(error) = self.engine.arm(armed) {
            self.status = Some(error.to_string());
        }
        if want_input && self.inputs == 0 && self.status.is_none() {
            self.status = Some("The audio device has no inputs to record from.".into());
        }
    }

    /// Starts recording the armed tracks from the playhead, with the others playing.
    pub(crate) fn record(&mut self) {
        if self.engine.is_playing() {
            self.status = Some("Stop playback to record.".into());
            return;
        }
        if self.pass.is_some() {
            self.status = Some("The last take is still being saved.".into());
            return;
        }
        if self.arming.is_empty() {
            self.status = Some("Arm a track to record.".into());
            return;
        }
        if self.inputs == 0 {
            self.status = Some("The audio device has no inputs to record from.".into());
            return;
        }
        if !self.load_feeds() {
            return;
        }
        let tracks: Vec<(TrackId, String, Layout)> = self
            .arming
            .keys()
            .filter_map(|id| {
                let track = self.session.state().track(*id)?;
                Some((*id, track.name.clone(), track.layout))
            })
            .collect();
        let requests: Vec<(String, usize)> = tracks
            .iter()
            .map(|(_, name, layout)| (name.clone(), layout.channels()))
            .collect();
        let takes = match self.services.start_takes(&requests) {
            Ok(takes) => takes,
            Err(message) => {
                self.status = Some(format!("Recording could not start: {message}"));
                return;
            }
        };
        let latency = self.services.device_latency();
        let mut pass_tracks = Vec::with_capacity(tracks.len());
        let mut sinks = BTreeMap::new();
        for ((id, _, layout), TakeFiles { files, sinks: s }) in tracks.into_iter().zip(takes) {
            pass_tracks.push((id, layout, files));
            sinks.insert(id, s);
        }
        let armed = self.armed_tracks(Some(sinks));
        let result = self
            .engine
            .arm(armed)
            .and_then(|()| self.engine.record(self.services.streams()));
        let pass = Pass {
            status: vec![None; pass_tracks.iter().map(|(_, _, f)| f.len()).sum()],
            tracks: pass_tracks,
            latency,
        };
        if let Err(error) = result {
            self.status = Some(format!("Recording could not start: {error}"));
            // Re-arming ends the unused takes.
            self.sync_arming();
            self.services.discard_takes(&pass.files());
            return;
        }
        self.pass = Some(pass);
    }

    /// Collects what has become of the pass's files and, once recording has stopped and every
    /// file is done, places the takes (`docs/architecture/recording.md` "Take placement").
    pub(crate) fn poll_recording(&mut self) {
        let Some(pass) = &mut self.pass else {
            return;
        };
        for status in self.services.take_status() {
            let (take, result) = match status {
                TakeStatus::Finished { take, frames } => (take, Ok(frames)),
                TakeStatus::Failed { take, message } => (take, Err(message)),
            };
            if let Some(slot) = pass.status.get_mut(take) {
                *slot = Some(result);
            }
        }
        if self.engine.is_recording() || pass.status.iter().any(Option::is_none) {
            return;
        }
        if let Some(pass) = self.pass.take() {
            self.place_takes(pass);
        }
    }

    /// Adds the pass's takes as one step, or discards them if nothing was recorded or a file
    /// failed.
    fn place_takes(&mut self, pass: Pass) {
        let files = pass.files();
        let overruns = self.services.take_overruns();
        let mut frames = u64::MAX;
        for status in &pass.status {
            match status {
                Some(Ok(n)) => frames = frames.min(*n),
                Some(Err(message)) => {
                    self.status = Some(format!("Recording failed: {message}"));
                    self.services.discard_takes(&files);
                    self.after_pass();
                    return;
                }
                None => frames = 0,
            }
        }
        let placement = self.engine.take().map(|info| info.placement(pass.latency));
        let Some((position, offset)) = placement.filter(|&(_, offset)| frames > offset) else {
            self.services.discard_takes(&files);
            self.after_pass();
            return;
        };
        let clips: Vec<(TrackId, Clip)> = pass
            .tracks
            .iter()
            .filter_map(|(id, _, names)| {
                let file = |name: &String| AudioFile {
                    name: name.clone(),
                    frames,
                };
                let source = match names.as_slice() {
                    [mono] => Source::Mono(file(mono)),
                    [left, right] => Source::Stereo {
                        left: file(left),
                        right: file(right),
                    },
                    _ => return None,
                };
                Some((*id, Clip::new(source, offset, frames - offset, position)))
            })
            .collect();
        if let Err(message) = self.services.keep_takes(&files) {
            self.status = Some(format!("The take's waveform could not be drawn: {message}"));
        }
        let result = self.bridge.edit(&mut self.session, |s| s.add_clips(clips));
        match result {
            Ok(_) if overruns > 0 => {
                self.status = Some(format!(
                    "The disk fell behind {overruns} times while recording; the gaps are silent."
                ));
            }
            Ok(_) => {}
            Err(error) => {
                self.status = Some(format!("The take could not be added: {error}"));
                self.services.discard_takes(&files);
            }
        }
        self.after_pass();
        self.refresh_lanes();
    }

    /// Re-arms without takes, releasing the spent ones, unless playback has started again.
    fn after_pass(&mut self) {
        if !self.engine.is_playing() {
            let armed = self.armed_tracks(None);
            let _ = self.engine.arm(armed);
        }
    }

    /// The engine's armed tracks for the arm settings, with each track's sinks from `sinks`.
    fn armed_tracks(
        &self,
        mut sinks: Option<BTreeMap<TrackId, Vec<Box<dyn audio_engine::TakeSink>>>>,
    ) -> Vec<ArmedTrack> {
        let sources = self.bridge.sources();
        self.arming
            .iter()
            .filter_map(|(id, arming)| {
                let source = *sources.get(id)?;
                let channels = self.track_layout(*id)?.channels();
                Some(ArmedTrack {
                    source,
                    inputs: (arming.input..arming.input + channels).collect(),
                    monitor: arming.monitor,
                    sinks: sinks
                        .as_mut()
                        .and_then(|s| s.remove(id))
                        .unwrap_or_default(),
                })
            })
            .collect()
    }

    fn track_layout(&self, track: TrackId) -> Option<Layout> {
        self.session.state().track(track).map(|t| t.layout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_choice_names_channels_from_one() {
        let mono = InputChoice {
            input: 2,
            stereo: false,
        };
        let stereo = InputChoice {
            input: 0,
            stereo: true,
        };
        assert_eq!(
            (mono.to_string(), stereo.to_string()),
            ("In 3".into(), "In 1–2".into())
        );
    }
}
