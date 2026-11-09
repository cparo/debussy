//! The real-time side: takes commands at a block boundary and runs each block. Nothing here
//! blocks, locks or allocates.

use std::sync::Arc;

use audio_buffers::{Consumer, Producer};
use signal_graph::{NodeId, Runner};

use crate::block::{self, Live};
use crate::capture::{ArmedTrack, Input, end_takes};
use crate::engine::{Command, Retired, Shared, TransportState};
use crate::error::Error;
use crate::feed::TrackFeeds;

/// Runs the engine on the real-time thread: call [`Processor::process`] once per device block.
///
/// Drop it off the real-time thread: it owns the plan, the feeds, the takes and queue ends.
pub struct Processor {
    runner: Runner,
    output: NodeId,
    tracks: Vec<TrackFeeds>,
    armed: Vec<ArmedTrack>,
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
            armed: Vec::new(),
            input,
            commands,
            retire,
            waiting: None,
            shared,
            state: TransportState::Stopped,
            playhead: 0,
            recording: false,
            capturing: false,
            prime: u64::from(sample_rate / 10),
        }
    }

    /// Runs one block of `left.len()` frames into `left` and `right`, with the device's
    /// interleaved `input` of `input_channels` channels (empty, or too short, is silence): takes
    /// pending commands, starts playback once every feed is primed, fills the track sources
    /// (silence unless playing; armed tracks' input or silence, see
    /// `docs/architecture/recording.md` "Input monitoring"), hands the input to the takes while
    /// recording, publishes each input's peak for the meters while armed, runs the plan and
    /// advances the playhead. The graph runs while stopped too, so
    /// plans swap, tails ring out and monitored input is heard.
    ///
    /// # Errors
    ///
    /// [`Error::BufferLength`], or [`Error::Graph`] for a block longer than the graph's
    /// largest; the output is then silence, nothing is captured and the playhead does not move.
    pub fn process(
        &mut self,
        input: &[f32],
        input_channels: usize,
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
            self.state = TransportState::Playing;
        }
        let take_starts =
            self.recording && !self.capturing && self.state == TransportState::Playing;
        self.capturing |= take_starts;
        let playing = self.state == TransportState::Playing;
        let frames = left.len();
        if !self.armed.is_empty() {
            self.input.load(input, input_channels, frames);
            for channel in 0..self.input.live() {
                self.shared
                    .raise_peak(channel, self.input.peak(channel, frames));
            }
        }
        let mut live = Live {
            armed: &mut self.armed,
            input: &self.input,
            recording: self.recording,
            capturing: self.capturing,
        };
        let result = block::run(
            &mut self.runner,
            self.output,
            &mut self.tracks,
            Some(&mut live),
            playing,
            self.shared.mono(),
            left,
            right,
        );
        if take_starts {
            // After the block, so the latency is that of the plan this block ran.
            self.shared
                .start_take(self.playhead, self.runner.latency() as u64);
        }
        if playing && result.is_ok() {
            self.playhead = self.playhead.saturating_add(frames as u64);
        }
        self.shared.publish(self.state, self.playhead);
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
                Command::Arm(armed) => {
                    let mut old = std::mem::replace(&mut self.armed, armed);
                    end_takes(&mut old);
                    self.send_back(Retired::Armed(old));
                }
                Command::Play { from, record } => {
                    self.playhead = from;
                    self.state = TransportState::Starting;
                    self.recording = record;
                }
                Command::Pause => {
                    if matches!(
                        self.state,
                        TransportState::Starting | TransportState::Playing
                    ) {
                        self.state = TransportState::Paused;
                    }
                    self.end_recording();
                }
                Command::Stop { to } => {
                    self.playhead = to;
                    self.state = TransportState::Stopped;
                    self.end_recording();
                }
                Command::Locate { frame } => self.playhead = frame,
            }
        }
    }

    /// Ends the takes being recorded, if any.
    fn end_recording(&mut self) {
        if self.recording {
            end_takes(&mut self.armed);
        }
        self.recording = false;
        self.capturing = false;
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
