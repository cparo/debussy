//! The real-time side: takes commands at a block boundary and runs each block. Nothing here
//! blocks, locks or allocates.

use std::sync::Arc;

use audio_buffers::{Consumer, Producer};
use signal_graph::{NodeId, Runner};

use crate::block::{self, Live};
use crate::capture::{ArmedMidi, ArmedTrack, HeldNotes, Input, MAX_RELEASING, end_takes};
use crate::click::ClickTrack;
use crate::engine::{Command, Retired, Shared, TransportState};
use crate::error::Error;
use crate::feed::TrackFeeds;
use crate::midi::MidiTrack;

/// Runs the engine on the real-time thread: call [`Processor::process`] once per device block.
///
/// Drop it off the real-time thread: it owns the plan, the feeds, the takes and queue ends.
pub struct Processor {
    runner: Runner,
    output: NodeId,
    tracks: Vec<TrackFeeds>,
    midi: Vec<MidiTrack>,
    armed: Vec<ArmedTrack>,
    armed_midi: Vec<ArmedMidi>,
    /// Live notes of MIDI tracks no longer armed, still to be released; holds at most
    /// [`MAX_RELEASING`], allocated once.
    releasing: Vec<(NodeId, HeldNotes)>,
    click: Option<ClickTrack>,
    input: Input,
    commands: Consumer<Command>,
    retire: Producer<Retired>,
    /// A replaced set the retire queue had no room for yet.
    waiting: Option<Retired>,
    shared: Arc<Shared>,
    state: TransportState,
    playhead: u64,
    /// Whether the transport was started by record; cleared by stop and pause.
    recording: bool,
    /// Whether the armed takes are being captured: recording and playing.
    capturing: bool,
    /// Frames of count-in still to run before recording plays.
    count_in: u64,
    /// The take frame of the next frame captured.
    capture_frame: u64,
    sample_rate: u32,
    /// Frames every feed must have ready before playback starts: 100 ms.
    prime: u64,
}

impl Processor {
    pub(crate) fn new(
        runner: Runner,
        output: NodeId,
        sample_rate: u32,
        commands: Consumer<Command>,
        retire: Producer<Retired>,
        shared: Arc<Shared>,
    ) -> Self {
        let input = Input::new(runner.max_frames());
        Processor {
            runner,
            output,
            tracks: Vec::new(),
            midi: Vec::new(),
            armed: Vec::new(),
            armed_midi: Vec::new(),
            releasing: Vec::with_capacity(MAX_RELEASING),
            click: None,
            input,
            commands,
            retire,
            waiting: None,
            shared,
            state: TransportState::Stopped,
            playhead: 0,
            recording: false,
            capturing: false,
            count_in: 0,
            capture_frame: 0,
            sample_rate,
            prime: u64::from(sample_rate / 10),
        }
    }

    /// Runs one block of `left.len()` frames into `left` and `right`, with the device's
    /// interleaved `input` of `input_channels` channels (empty, or too short, is silence), whose
    /// callback ran at host time `host_nanos` (the clock live MIDI input is stamped with): takes
    /// pending commands, starts playback (or recording's count-in) once every feed is primed,
    /// fills the track sources
    /// (silence unless playing; armed tracks' input or silence, see
    /// `docs/architecture/recording.md` "Input monitoring") and the MIDI tracks' event sources
    /// (`docs/architecture/engine.md` "MIDI clip playback") and armed MIDI tracks' live input
    /// (`docs/architecture/engine.md` "Live MIDI input"), hands the input to the takes while
    /// recording, publishes each input's peak for the meters while armed, runs the plan, adds
    /// the click while playing with it on (`docs/architecture/recording.md` "Click track") and
    /// advances the playhead. The graph runs while stopped too, so plans swap, tails ring out
    /// and monitored input is heard. The block in which a count-in ends is split: the count-in's
    /// frames, then the rest playing, so a take starts at the playhead's frame exactly
    /// (`docs/architecture/recording.md` "Count-in").
    ///
    /// # Errors
    ///
    /// [`Error::BufferLength`], or [`Error::Graph`] for a block longer than the graph's
    /// largest; the output is then silence, nothing is captured and the playhead does not move.
    pub fn process(
        &mut self,
        input: &[f32],
        input_channels: usize,
        host_nanos: u64,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), Error> {
        self.take_commands();
        if self.state == TransportState::Starting
            && self
                .tracks
                .iter_mut()
                .flat_map(|t| t.feeds.iter_mut())
                .all(|f| f.is_primed(self.prime))
        {
            self.state = if self.count_in > 0 {
                TransportState::CountingIn
            } else {
                TransportState::Playing
            };
        }
        let frames = left.len();
        if !self.armed.is_empty() {
            self.input.load(input, input_channels, frames);
            for channel in 0..self.input.live() {
                self.shared
                    .raise_peak(channel, self.input.peak(channel, frames));
            }
        }
        // The count-in's part of the block; mismatched buffers are left to `block::run` to refuse.
        let counted = match self.state {
            TransportState::CountingIn if right.len() == frames => {
                frames.min(usize::try_from(self.count_in).unwrap_or(usize::MAX))
            }
            _ => 0,
        };
        let (count_left, left) = left.split_at_mut(counted);
        let (count_right, right) = right.split_at_mut(counted);
        if counted > 0 {
            if let Err(error) = self.count_in_part(host_nanos, count_left, count_right) {
                left.fill(0.0);
                right.fill(0.0);
                self.shared.publish(self.state, self.playhead);
                return Err(error);
            }
            self.count_in -= counted as u64;
            if self.count_in == 0 {
                // The rest of the block plays, capturing from the playhead's frame
                // (ADR-0084).
                self.state = TransportState::Playing;
            }
            self.input.skip(counted);
            if left.is_empty() {
                self.report_midi_overflows();
                self.shared.publish(self.state, self.playhead);
                return Ok(());
            }
        }
        let offset =
            (counted as u64).saturating_mul(1_000_000_000) / u64::from(self.sample_rate.max(1));
        let result = self.play_part(host_nanos.saturating_add(offset), left, right);
        self.report_midi_overflows();
        self.shared.publish(self.state, self.playhead);
        result
    }

    /// Runs `left.len()` frames of the count-in: the tracks silent (armed ones as when
    /// recording), the playhead held, and the click at the count-in's position behind the
    /// playhead, whether or not the click is on. Live MIDI input is heard but not captured.
    fn count_in_part(
        &mut self,
        host_nanos: u64,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), Error> {
        let position = self.playhead_frame();
        let mut live = Live {
            armed: &mut self.armed,
            input: &self.input,
            midi: &mut self.armed_midi,
            releasing: &mut self.releasing,
            host_nanos,
            sample_rate: self.sample_rate,
            recording: self.recording,
            capturing: false,
            capture_frame: 0,
            overflows: 0,
        };
        let result = block::run(
            &mut self.runner,
            self.output,
            &mut self.tracks,
            &mut self.midi,
            Some(&mut live),
            position,
            false,
            self.shared.mono(),
            left,
            right,
        );
        let overflows = live.overflows;
        self.add_live_overflows(overflows);
        result?;
        if let Some(click) = &self.click {
            let position = self.playhead as i64 - self.count_in as i64;
            let heard = position - self.runner.latency() as i64;
            click.mix(heard, self.shared.click_gain(), left, right);
        }
        Ok(())
    }

    /// The playhead as a signed timeline frame.
    fn playhead_frame(&self) -> i64 {
        i64::try_from(self.playhead).unwrap_or(i64::MAX)
    }

    /// Runs `left.len()` frames as the transport stands: playing (capturing if recording) or
    /// not, with the click while playing with it on, and advances the playhead if playing.
    fn play_part(
        &mut self,
        host_nanos: u64,
        left: &mut [f32],
        right: &mut [f32],
    ) -> Result<(), Error> {
        let take_starts =
            self.recording && !self.capturing && self.state == TransportState::Playing;
        self.capturing |= take_starts;
        if take_starts {
            self.capture_frame = 0;
        }
        let playing = self.state == TransportState::Playing;
        let frames = left.len();
        let position = self.playhead_frame();
        let mut live = Live {
            armed: &mut self.armed,
            input: &self.input,
            midi: &mut self.armed_midi,
            releasing: &mut self.releasing,
            host_nanos,
            sample_rate: self.sample_rate,
            recording: self.recording,
            capturing: self.capturing,
            capture_frame: self.capture_frame,
            overflows: 0,
        };
        let result = block::run(
            &mut self.runner,
            self.output,
            &mut self.tracks,
            &mut self.midi,
            Some(&mut live),
            position,
            playing,
            self.shared.mono(),
            left,
            right,
        );
        let overflows = live.overflows;
        self.add_live_overflows(overflows);
        if self.capturing && result.is_ok() {
            self.capture_frame = self.capture_frame.saturating_add(frames as u64);
        }
        if playing
            && result.is_ok()
            && self.shared.click()
            && let Some(click) = &self.click
        {
            // At the timeline position heard, behind the playhead by the plan's latency.
            let heard = self.playhead as i64 - self.runner.latency() as i64;
            click.mix(heard, self.shared.click_gain(), left, right);
        }
        if take_starts {
            // After the block, so the latency is that of the plan this block ran.
            self.shared
                .start_take(self.playhead, self.runner.latency() as u64);
        }
        if playing && result.is_ok() {
            self.playhead = self.playhead.saturating_add(frames as u64);
        }
        result
    }

    /// The plan's output latency, in frames.
    pub fn latency(&self) -> usize {
        self.runner.latency()
    }

    fn take_commands(&mut self) {
        if let Some(retired) = self.waiting.take() {
            self.send_back(retired);
        }
        while let Some(command) = self.commands.try_pop() {
            match command {
                Command::Tracks(tracks) => {
                    let old = std::mem::replace(&mut self.tracks, tracks);
                    self.send_back(Retired::Tracks(old));
                }
                Command::Midi(mut midi) => {
                    for track in &mut midi {
                        if let Some(old) = self.midi.iter().find(|t| t.source() == track.source()) {
                            track.carry_over(old);
                        }
                    }
                    let old = std::mem::replace(&mut self.midi, midi);
                    self.send_back(Retired::Midi(old));
                }
                Command::Click(click) => {
                    let old = self.click.replace(click);
                    self.send_back(Retired::Click(old));
                }
                Command::Arm { audio, mut midi } => {
                    let mut old = std::mem::replace(&mut self.armed, audio);
                    for track in &mut midi {
                        if let Some(previous) = self
                            .armed_midi
                            .iter_mut()
                            .find(|p| p.source() == track.source())
                        {
                            track.carry_over(previous);
                        }
                    }
                    let mut old_midi = std::mem::replace(&mut self.armed_midi, midi);
                    for track in &mut old_midi {
                        let held = track.take_held();
                        // Beyond the room made for them, notes are left to the next reset.
                        if !held.is_empty() && self.releasing.len() < self.releasing.capacity() {
                            self.releasing.push((track.source(), held));
                        }
                    }
                    // Takes not recorded are empty; those recorded have ended already.
                    end_takes(&mut old, &mut old_midi, 0);
                    self.send_back(Retired::Armed(old, old_midi));
                }
                Command::Play {
                    from,
                    record,
                    count_in,
                } => {
                    self.playhead = from;
                    self.state = TransportState::Starting;
                    self.recording = record;
                    self.count_in = if record { count_in } else { 0 };
                }
                Command::Pause => {
                    if matches!(
                        self.state,
                        TransportState::Starting
                            | TransportState::Playing
                            | TransportState::CountingIn
                    ) {
                        self.state = TransportState::Paused;
                    }
                    self.end_recording();
                    // Effect tails and compensation delays stop with the transport.
                    self.reset_graph();
                }
                Command::Stop { to } => {
                    self.playhead = to;
                    self.state = TransportState::Stopped;
                    self.end_recording();
                    self.reset_graph();
                }
                Command::Locate { frame } => self.playhead = frame,
            }
        }
    }

    /// Adds the events the MIDI tracks' buffers refused since the last call to the shared count.
    fn report_midi_overflows(&mut self) {
        let refused: u64 = self.midi.iter_mut().map(MidiTrack::take_overflows).sum();
        if refused > 0 {
            self.shared.add_midi_overflows(refused);
        }
    }

    /// Silences the graph: every node's state and delay. Live notes sounding are also sent
    /// note-offs in the next block, for nodes that keep notes through a reset.
    fn reset_graph(&mut self) {
        self.runner.reset();
        for track in &mut self.armed_midi {
            let held = track.take_held();
            if !held.is_empty() && self.releasing.len() < self.releasing.capacity() {
                self.releasing.push((track.source(), held));
            }
        }
    }

    /// Adds live MIDI events the event sources refused to the shared count.
    fn add_live_overflows(&mut self, refused: u64) {
        if refused > 0 {
            self.shared.add_midi_overflows(refused);
        }
    }

    /// Ends the takes being recorded, if any, and any count-in.
    fn end_recording(&mut self) {
        if self.recording {
            let frames = if self.capturing {
                self.capture_frame
            } else {
                0
            };
            end_takes(&mut self.armed, &mut self.armed_midi, frames);
        }
        self.recording = false;
        self.capturing = false;
        self.count_in = 0;
    }

    /// Sends a replaced set back to be dropped off the real-time thread.
    fn send_back(&mut self, retired: Retired) {
        if let Err(retired) = self.retire.try_push(retired) {
            // Only one can wait; the control side collects before sending more, so a second
            // never arrives while one waits.
            self.waiting = Some(retired);
        }
    }
}
