//! Arming tracks, monitoring and recording passes (`docs/architecture/recording.md`,
//! `docs/architecture/user-interface.md` "Recording"). Arm, monitor and input settings belong to
//! the session and are not saved (ADR-0079).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use audio_buffers::Layout;
use audio_engine::{ArmedMidi, ArmedTrack, NoteSink, TakeInfo, TakeSink};
use project_model::{AudioFile, Clip, Sequence, Source, TrackId, TrackKind};
use signal_graph::NodeId;

use crate::app::App;
use crate::services::{MidiInputChoice, PassMarker, Recovered, TakeFiles, TakeStatus};

/// The quietest level an input meter shows, in dBFS.
pub const METER_FLOOR_DB: f32 = -60.0;
/// How far a meter falls per display frame when the input drops, in dB.
const METER_FALL_DB: f32 = 1.5;
/// How many display frames a MIDI track's activity light stays lit after a message.
const ACTIVITY_FRAMES: u8 = 6;

/// One armed track's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Arming {
    /// Whether its input is heard through the graph.
    pub(crate) monitor: bool,
    /// What it records from.
    pub(crate) input: ArmedInput,
}

/// What an armed track records from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArmedInput {
    /// An audio track: the device input channel (from 0) of its first channel; a stereo track
    /// takes the next one for its second.
    Audio(usize),
    /// A MIDI track: the id of its MIDI input endpoint, if one is chosen.
    Midi(Option<String>),
}

/// An armed MIDI track's activity light: the engine's count of messages it has received.
#[derive(Debug, Default)]
pub(crate) struct Activity {
    count: Arc<AtomicU64>,
    seen: u64,
    /// Display frames it stays lit for.
    lit: u8,
}

/// The MIDI input the engine has for an armed MIDI track: its endpoint and the event source it
/// plays into.
pub(crate) type OpenMidi = (String, NodeId);

/// A MIDI take's captured messages: take frame and bytes, as they arrived.
type Captured = Vec<(u64, [u8; 3])>;

/// One armed track's take in a recording pass.
enum PassTrack {
    /// An audio track's files.
    Audio(TrackId, Vec<String>),
    /// A MIDI track's take, numbered after every audio file, and its journal.
    Midi(TrackId, String),
}

/// A recording pass whose files are still being written.
pub(crate) struct Pass {
    /// Each armed track's take: the audio ones in the order their files are numbered, then the
    /// MIDI ones.
    tracks: Vec<PassTrack>,
    /// What became of each take, by number.
    status: Vec<Option<Result<u64, String>>>,
    /// The device latency when the pass started.
    latency: u64,
    /// The MIDI placement latency when the pass started.
    midi_latency: u64,
    /// Where capture began, once it has; kept here in case the engine is rebuilt.
    info: Option<TakeInfo>,
}

impl Pass {
    /// Every file of the pass: the audio files, then the MIDI journals.
    fn files(&self) -> Vec<String> {
        let mut files: Vec<String> = self.audio().into_iter().flat_map(|(_, f)| f).collect();
        files.extend(self.journals());
        files
    }

    /// The MIDI takes' journals.
    fn journals(&self) -> Vec<String> {
        self.tracks
            .iter()
            .filter_map(|t| match t {
                PassTrack::Midi(_, journal) => Some(journal.clone()),
                PassTrack::Audio(..) => None,
            })
            .collect()
    }

    /// Every track and its files, as the crash marker records them.
    fn marked(&self) -> Vec<(TrackId, Vec<String>)> {
        self.tracks
            .iter()
            .map(|t| match t {
                PassTrack::Audio(id, files) => (*id, files.clone()),
                PassTrack::Midi(id, journal) => (*id, vec![journal.clone()]),
            })
            .collect()
    }

    /// The audio tracks and their files.
    fn audio(&self) -> Vec<(TrackId, Vec<String>)> {
        self.tracks
            .iter()
            .filter_map(|t| match t {
                PassTrack::Audio(id, files) => Some((*id, files.clone())),
                PassTrack::Midi(..) => None,
            })
            .collect()
    }

    /// The MIDI tracks and their take numbers.
    fn midi(&self) -> Vec<(TrackId, usize)> {
        let first = self.audio().iter().map(|(_, f)| f.len()).sum::<usize>();
        self.tracks
            .iter()
            .filter_map(|t| match t {
                PassTrack::Midi(id, _) => Some(*id),
                PassTrack::Audio(..) => None,
            })
            .enumerate()
            .map(|(i, id)| (id, first + i))
            .collect()
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
    /// The audio track's arm settings, if it is armed.
    pub fn arming(&self, track: TrackId) -> Option<(bool, InputChoice)> {
        let arming = self.arming.get(&track)?;
        let ArmedInput::Audio(input) = arming.input else {
            return None;
        };
        let stereo = self.track_layout(track) == Some(Layout::Stereo);
        Some((arming.monitor, InputChoice { input, stereo }))
    }

    /// Whether the track is armed, audio or MIDI.
    pub fn is_armed(&self, track: TrackId) -> bool {
        self.arming.contains_key(&track)
    }

    /// The MIDI track's arm settings, if it is armed: whether it is monitored, and its MIDI
    /// input's endpoint id if one is chosen.
    pub fn midi_arming(&self, track: TrackId) -> Option<(bool, Option<&str>)> {
        let arming = self.arming.get(&track)?;
        match &arming.input {
            ArmedInput::Midi(input) => Some((arming.monitor, input.as_deref())),
            ArmedInput::Audio(_) => None,
        }
    }

    /// The MIDI inputs present when last read, for an armed MIDI track's input list.
    pub fn midi_input_choices(&self) -> &[MidiInputChoice] {
        &self.midi_inputs
    }

    /// Whether the armed MIDI track has received a message in the last few display frames.
    pub fn midi_active(&self, track: TrackId) -> bool {
        self.midi_activity.get(&track).is_some_and(|a| a.lit > 0)
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

    /// Reads the engine's input peaks for the armed audio tracks' meters: each meter jumps up to
    /// a louder peak and falls [`METER_FALL_DB`] per frame otherwise; and the armed MIDI tracks'
    /// message counts for their activity lights. Called each display frame.
    pub(crate) fn update_meters(&mut self) {
        for activity in self.midi_activity.values_mut() {
            let count = activity.count.load(Ordering::Relaxed);
            if count != activity.seen {
                activity.seen = count;
                activity.lit = ACTIVITY_FRAMES;
            } else {
                activity.lit = activity.lit.saturating_sub(1);
            }
        }
        let mut peaks: BTreeMap<usize, f32> = BTreeMap::new();
        let mut meters = BTreeMap::new();
        for (id, arming) in &self.arming {
            let ArmedInput::Audio(first) = arming.input else {
                continue;
            };
            let channels = self.track_layout(*id).map_or(0, Layout::channels);
            let old = self.meters.get(id);
            let levels = (0..channels)
                .map(|c| {
                    let input = first + c;
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
    /// if an audio track is armed, and the last pass saved.
    pub fn can_record(&self) -> bool {
        !self.engine.is_playing()
            && !self.arming.is_empty()
            && (self.inputs > 0 || !self.audio_armed())
            && self.pass.is_none()
    }

    /// Whether an audio track is armed.
    fn audio_armed(&self) -> bool {
        self.arming
            .values()
            .any(|a| matches!(a.input, ArmedInput::Audio(_)))
    }

    /// Whether a recording pass's files are still being written.
    pub(crate) fn recording_pending(&self) -> bool {
        self.pass.is_some()
    }

    /// Arms the track or disarms it. An audio track defaults to input 1 (1–2 for stereo)
    /// without monitoring; the device's inputs open with the first armed audio track and close
    /// with the last. A MIDI track defaults to the first MIDI input, monitored
    /// (`docs/architecture/recording.md` "MIDI recording").
    pub(crate) fn toggle_arm(&mut self, track: TrackId) {
        if self.refuse_while_playing("arm a track") {
            return;
        }
        if self.arming.remove(&track).is_none() {
            let kind = self.session.state().track(track).map(|t| t.kind);
            let arming = match kind {
                Some(TrackKind::Audio(_)) => Some(Arming {
                    monitor: false,
                    input: ArmedInput::Audio(0),
                }),
                Some(TrackKind::Midi) => {
                    self.midi_inputs = self.services.midi_inputs();
                    let input = self.midi_inputs.first().map(|c| c.id.clone());
                    if input.is_none() {
                        self.status = Some("No MIDI input is connected.".into());
                    }
                    Some(Arming {
                        monitor: true,
                        input: ArmedInput::Midi(input),
                    })
                }
                None => None,
            };
            if let Some(arming) = arming {
                self.arming.insert(track, arming);
            }
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

    /// Sets the armed audio track's input.
    pub(crate) fn set_input(&mut self, track: TrackId, input: usize) {
        if self.refuse_while_playing("change a track's input") {
            return;
        }
        if let Some(arming) = self.arming.get_mut(&track)
            && let ArmedInput::Audio(current) = &mut arming.input
        {
            *current = input;
            self.sync_arming();
        }
    }

    /// Sets the armed MIDI track's MIDI input.
    pub(crate) fn set_midi_input(&mut self, track: TrackId, choice: MidiInputChoice) {
        if self.refuse_while_playing("change a track's input") {
            return;
        }
        if let Some(arming) = self.arming.get_mut(&track)
            && let ArmedInput::Midi(current) = &mut arming.input
        {
            *current = Some(choice.id);
            self.sync_arming();
        }
    }

    /// Re-reads the MIDI inputs if they have changed. Called each display frame while a MIDI
    /// track is armed.
    pub(crate) fn refresh_midi_inputs(&mut self) {
        let midi_armed = self
            .arming
            .values()
            .any(|a| matches!(a.input, ArmedInput::Midi(_)));
        if midi_armed && self.services.midi_inputs_changed() {
            self.midi_inputs = self.services.midi_inputs();
        }
    }

    /// Forgets the MIDI inputs the engine had, closing them: the engine has been rebuilt.
    pub(crate) fn forget_midi_inputs(&mut self) {
        for track in std::mem::take(&mut self.midi_open).into_keys() {
            self.services.close_midi_input(track);
        }
    }

    /// Brings the device and the engine in step with the arm settings, dropping tracks that no
    /// longer exist. Only while stopped or paused.
    pub(crate) fn sync_arming(&mut self) {
        if self.services.audio_lost() {
            // The device went while nothing was watching; recovering arms the new engine.
            self.recover_audio(None);
            return;
        }
        let state = self.session.state();
        self.arming.retain(|id, _| state.track(*id).is_some());
        let want_input = self.audio_armed();
        if want_input != self.input_open {
            let result = self.services.set_input(want_input);
            self.input_open = want_input;
            self.inputs = self.services.input_channels();
            if let Err(message) = result {
                self.status = Some(message);
            }
        }
        // Monitored input is left out of delay compensation (ADR-0136).
        let monitored = self
            .arming
            .iter()
            .filter(|(_, arming)| arming.monitor)
            .map(|(&id, _)| id)
            .collect();
        if let Err(error) = self.bridge.set_monitored(monitored) {
            self.status = Some(error.to_string());
        }
        let (audio, midi) = self.armed_tracks(None, None);
        if let Err(error) = self.engine.arm(audio, midi) {
            self.status = Some(error.to_string());
            self.forget_midi_inputs();
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
        if self.inputs == 0 && self.audio_armed() {
            self.status = Some("The audio device has no inputs to record from.".into());
            return;
        }
        if !self.load_feeds() {
            return;
        }
        let state = self.session.state();
        let audio: Vec<(TrackId, String, Layout)> = self
            .arming
            .iter()
            .filter(|(_, a)| matches!(a.input, ArmedInput::Audio(_)))
            .filter_map(|(id, _)| {
                let track = state.track(*id)?;
                Some((*id, track.name.clone(), track.layout()?))
            })
            .collect();
        let midi: Vec<TrackId> = self
            .arming
            .iter()
            .filter(|(id, a)| matches!(a.input, ArmedInput::Midi(_)) && state.track(**id).is_some())
            .map(|(id, _)| *id)
            .collect();
        let requests: Vec<(String, usize)> = audio
            .iter()
            .map(|(_, name, layout)| (name.clone(), layout.channels()))
            .collect();
        let takes = match self.services.start_takes(&requests, midi.len()) {
            Ok(takes) => takes,
            Err(message) => {
                self.status = Some(format!("Recording could not start: {message}"));
                return;
            }
        };
        let latency = self.services.device_latency();
        let midi_latency = self.services.midi_latency();
        let mut pass_tracks = Vec::with_capacity(audio.len() + midi.len());
        let mut sinks = BTreeMap::new();
        for ((id, _, _), TakeFiles { files, sinks: s }) in audio.into_iter().zip(takes.audio) {
            pass_tracks.push(PassTrack::Audio(id, files));
            sinks.insert(id, s);
        }
        let mut note_sinks = BTreeMap::new();
        for (id, take) in midi.into_iter().zip(takes.midi) {
            pass_tracks.push(PassTrack::Midi(id, take.file));
            note_sinks.insert(id, take.sink);
        }
        let mut pass = Pass {
            status: Vec::new(),
            tracks: pass_tracks,
            latency,
            midi_latency,
            info: None,
        };
        pass.status = vec![None; pass.files().len()];
        let marker = PassMarker {
            device_latency: latency,
            midi_latency,
            tracks: pass.marked(),
        };
        if let Err(message) = self.services.mark_pass(&marker) {
            self.status = Some(unprotected(&message));
        }
        let (audio, midi) = self.armed_tracks(Some(sinks), Some(note_sinks));
        let count_in = self.count_in_frames();
        let result = self
            .engine
            .arm(audio, midi)
            .and_then(|()| self.engine.record(self.services.streams(), count_in));
        if let Err(error) = result {
            self.status = Some(format!("Recording could not start: {error}"));
            // Re-arming ends the unused takes.
            self.forget_midi_inputs();
            self.sync_arming();
            self.services.discard_takes(&pass.files());
            self.services.clear_pass_marker();
            return;
        }
        self.pass = Some(pass);
    }

    /// Notes where the pending pass's capture began, if the engine says, before the engine is
    /// replaced, and adds it to the crash marker.
    pub(crate) fn keep_take_info(&mut self) {
        if let Some(pass) = &mut self.pass
            && pass.info.is_none()
        {
            pass.info = self.engine.take();
            if let Some(info) = pass.info
                && let Err(message) = self.services.mark_pass_started(info)
            {
                self.status = Some(unprotected(&message));
            }
        }
    }

    /// Collects what has become of the pass's files and, once recording has stopped and every
    /// file is done, places the takes (`docs/architecture/recording.md` "Take placement").
    pub(crate) fn poll_recording(&mut self) {
        self.keep_take_info();
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
    /// failed, then clears the crash marker.
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
                    self.services.clear_pass_marker();
                    self.after_pass();
                    return;
                }
                None => frames = 0,
            }
        }
        let midi: Vec<(TrackId, Captured, u64)> = pass
            .midi()
            .into_iter()
            .map(|(id, take)| (id, self.services.take_messages(take), frames))
            .collect();
        let takes = Takes {
            audio: pass.audio(),
            midi,
            frames,
            info: pass.info,
            latency: pass.latency,
            midi_latency: pass.midi_latency,
        };
        match self.add_takes(takes) {
            Ok(true) => {
                self.services.discard_takes(&pass.journals());
                if overruns > 0 {
                    self.status = Some(format!(
                        "The disk fell behind {overruns} times while recording; the gaps are \
                         silent."
                    ));
                }
            }
            Ok(false) => self.services.discard_takes(&files),
            Err(message) => {
                self.status = Some(format!("The take could not be added: {message}"));
                self.services.discard_takes(&files);
            }
        }
        self.services.clear_pass_marker();
        self.after_pass();
        self.refresh_lanes();
    }

    /// Places the takes of a pass a crash cut off, as one step that undo removes, saves, and
    /// clears the crash marker (`docs/architecture/recording.md` "Take recovery"). Called once,
    /// when the project opens.
    pub(crate) fn recover_takes(&mut self) {
        let Some(recovered) = self.services.take_recovered() else {
            return;
        };
        let placed = match recovered {
            Recovered::Discarded => false,
            Recovered::Takes(pass) => {
                let journals: Vec<String> = pass.midi.iter().map(|m| m.file.clone()).collect();
                let added = self.add_takes(Takes {
                    audio: pass.tracks.clone(),
                    midi: pass
                        .midi
                        .into_iter()
                        .map(|m| (m.track, m.messages, m.frames))
                        .collect(),
                    frames: pass.frames,
                    info: Some(pass.info),
                    latency: pass.device_latency,
                    midi_latency: pass.midi_latency,
                });
                let added = matches!(added, Ok(true));
                if !added {
                    let files: Vec<String> = pass.tracks.into_iter().flat_map(|(_, f)| f).collect();
                    self.services.discard_takes(&files);
                }
                self.services.discard_takes(&journals);
                added
            }
        };
        self.services.clear_pass_marker();
        self.status = Some(if placed {
            "Recovered a take interrupted by a crash.".into()
        } else {
            "A take interrupted by a crash could not be recovered.".into()
        });
        self.refresh_lanes();
    }

    /// Adds a clip for each take as one step, placed where `takes.info` says
    /// (`docs/architecture/recording.md` "Take placement", "MIDI recording"), summarises the
    /// audio files, and saves at once, so the step is on disk before the crash marker goes.
    /// Tracks no longer in the project, or whose kind or layout does not match their take, and
    /// MIDI takes with no notes, are skipped. Returns whether anything was added; the caller
    /// discards the files otherwise.
    fn add_takes(&mut self, takes: Takes) -> Result<bool, String> {
        let Takes {
            audio,
            midi,
            frames,
            info,
            latency,
            midi_latency,
        } = takes;
        let Some(info) = info else {
            return Ok(false);
        };
        let state = self.session.state();
        let (position, offset) = info.placement(latency);
        let audio = if frames > offset { audio } else { Vec::new() };
        let mut placed = Vec::new();
        let mut clips: Vec<(TrackId, Clip)> = audio
            .iter()
            .filter(|(id, names)| {
                state
                    .track(*id)
                    .and_then(|t| t.layout())
                    .is_some_and(|layout| layout.channels() == names.len())
            })
            .filter_map(|(id, names)| {
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
                placed.extend(names.iter().cloned());
                Some((*id, Clip::new(source, offset, frames - offset, position)))
            })
            .collect();
        let (position, offset) = info.midi_placement(midi_latency);
        for (id, messages, frames) in midi {
            let Some(track) = state.track(id).filter(|t| t.kind == TrackKind::Midi) else {
                continue;
            };
            let Ok(sequence) = Sequence::from_capture(&messages, frames) else {
                continue;
            };
            if sequence.notes().is_empty() || frames <= offset {
                continue;
            }
            let source = Source::Midi(Arc::new(sequence));
            let clip = Clip::new(source, offset, frames - offset, position);
            // A recovered take placed just before the crash is not placed again.
            if track.clips.iter().any(|(_, c)| *c == clip) {
                continue;
            }
            clips.push((id, clip));
        }
        if clips.is_empty() {
            return Ok(false);
        }
        if !placed.is_empty()
            && let Err(message) = self.services.keep_takes(&placed)
        {
            self.status = Some(format!("The take's waveform could not be drawn: {message}"));
        }
        self.bridge
            .edit(&mut self.session, |s| s.add_clips(clips))
            .map_err(|e| e.to_string())?;
        self.autosave();
        Ok(true)
    }

    /// Discards the pending pass's takes and disarms every track. Only while stopped.
    pub(crate) fn discard_pass(&mut self) {
        if let Some(pass) = self.pass.take() {
            self.services.discard_takes(&pass.files());
            self.services.clear_pass_marker();
        }
        if !self.arming.is_empty() {
            self.arming.clear();
            self.sync_arming();
        }
    }

    /// Re-arms without takes, releasing the spent ones, unless playback has started again.
    fn after_pass(&mut self) {
        if !self.engine.is_playing() {
            let (audio, midi) = self.armed_tracks(None, None);
            if self.engine.arm(audio, midi).is_err() {
                self.forget_midi_inputs();
            }
        }
    }

    /// The engine's armed tracks for the arm settings, audio then MIDI, with each audio track's
    /// sinks from `sinks` and each MIDI track's from `note_sinks`. Opens each armed MIDI
    /// track's input unless the engine already has it, and closes those of tracks no longer
    /// armed; one the engine has is carried over by the engine.
    fn armed_tracks(
        &mut self,
        mut sinks: Option<BTreeMap<TrackId, Vec<Box<dyn TakeSink>>>>,
        mut note_sinks: Option<BTreeMap<TrackId, Box<dyn NoteSink>>>,
    ) -> (Vec<ArmedTrack>, Vec<ArmedMidi>) {
        let sources = self.bridge.sources();
        let event_sources = self.bridge.event_sources();
        let mut audio = Vec::new();
        let mut midi = Vec::new();
        let mut wanted: BTreeMap<TrackId, OpenMidi> = BTreeMap::new();
        for (id, arming) in &self.arming {
            match &arming.input {
                ArmedInput::Audio(first) => {
                    let (Some(&source), Some(layout)) = (sources.get(id), self.track_layout(*id))
                    else {
                        continue;
                    };
                    audio.push(ArmedTrack {
                        source,
                        inputs: (*first..*first + layout.channels()).collect(),
                        monitor: arming.monitor,
                        sinks: sinks
                            .as_mut()
                            .and_then(|s| s.remove(id))
                            .unwrap_or_default(),
                    });
                }
                ArmedInput::Midi(endpoint) => {
                    let Some(&source) = event_sources.get(id) else {
                        continue;
                    };
                    if let Some(endpoint) = endpoint {
                        wanted.insert(*id, (endpoint.clone(), source));
                    }
                    let sink = note_sinks.as_mut().and_then(|s| s.remove(id));
                    midi.push((*id, source, arming.monitor, sink));
                }
            }
        }
        let gone: Vec<TrackId> = self
            .midi_open
            .iter()
            .filter(|(id, open)| wanted.get(id) != Some(open))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.midi_open.remove(&id);
            self.services.close_midi_input(id);
        }
        self.midi_activity.retain(|id, _| wanted.contains_key(id));
        let midi = midi
            .into_iter()
            .map(|(id, source, monitor, sink)| {
                let input = match wanted.get(&id) {
                    Some(open) if !self.midi_open.contains_key(&id) => {
                        match self.services.open_midi_input(id, &open.0) {
                            Ok(input) => {
                                self.midi_open.insert(id, open.clone());
                                Some(input)
                            }
                            Err(message) => {
                                self.status =
                                    Some(format!("The MIDI input could not be opened: {message}"));
                                None
                            }
                        }
                    }
                    _ => None,
                };
                let activity = Arc::clone(&self.midi_activity.entry(id).or_default().count);
                ArmedMidi::new(source, input, monitor, sink, activity)
            })
            .collect();
        (audio, midi)
    }

    /// An audio track's layout; `None` for a MIDI track or one that is gone.
    fn track_layout(&self, track: TrackId) -> Option<Layout> {
        self.session.state().track(track).and_then(|t| t.layout())
    }
}

/// A pass's takes to place: see [`App::add_takes`].
struct Takes {
    /// Each audio track and its files, left first.
    audio: Vec<(TrackId, Vec<String>)>,
    /// Each MIDI track, the messages captured, and the take's length in frames.
    midi: Vec<(TrackId, Captured, u64)>,
    /// The length every audio take is placed with, in frames.
    frames: u64,
    /// Where capture began; nothing is placed without it.
    info: Option<TakeInfo>,
    /// The device latency, for audio takes.
    latency: u64,
    /// The MIDI placement latency, for MIDI takes.
    midi_latency: u64,
}

/// The status line's warning when the crash marker could not be written.
fn unprotected(message: &str) -> String {
    format!("If Debussy crashes, this take cannot be recovered: {message}")
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
