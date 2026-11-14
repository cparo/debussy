//! The real-time side: takes commands at a block boundary and runs each block. Nothing here
//! blocks, locks or allocates.

use std::sync::Arc;

use audio_buffers::{Consumer, Producer};
use signal_graph::{NodeId, Runner};

use crate::block::{self, Live};
use crate::capture::{ArmedTrack, Input, end_takes};
use crate::click::ClickTrack;
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
            prime: u64::from(sample_rate / 10),
        }
    }

    /// Runs one block of `left.len()` frames into `left` and `right`, with the device's
    /// interleaved `input` of `input_channels` channels (empty, or too short, is silence): takes
    /// pending commands, starts playback (or recording's count-in) once every feed is primed,
    /// fills the track sources
    /// (silence unless playing; armed tracks' input or silence, see
    /// `docs/architecture/recording.md` "Input monitoring"), hands the input to the takes while
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
            if let Err(error) = self.count_in_part(count_left, count_right) {
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
                self.shared.publish(self.state, self.playhead);
                return Ok(());
            }
        }
        let result = self.play_part(left, right);
        self.shared.publish(self.state, self.playhead);
        result
    }

    /// Runs `left.len()` frames of the count-in: the tracks silent (armed ones as when
    /// recording), the playhead held, and the click at the count-in's position behind the
    /// playhead, whether or not the click is on.
    fn count_in_part(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), Error> {
        let position = self.playhead_frame();
        let mut live = Live {
            armed: &mut self.armed,
            input: &self.input,
            recording: self.recording,
            capturing: false,
        };
        block::run(
            &mut self.runner,
            self.output,
            &mut self.tracks,
            Some(&mut live),
            position,
            false,
            self.shared.mono(),
            left,
            right,
        )?;
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
    fn play_part(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), Error> {
        let take_starts =
            self.recording && !self.capturing && self.state == TransportState::Playing;
        self.capturing |= take_starts;
        let playing = self.state == TransportState::Playing;
        let frames = left.len();
        let position = self.playhead_frame();
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
            position,
            playing,
            self.shared.mono(),
            left,
            right,
        );
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
                Command::Click(click) => {
                    let old = self.click.replace(click);
                    self.send_back(Retired::Click(old));
                }
                Command::Arm(armed) => {
                    let mut old = std::mem::replace(&mut self.armed, armed);
                    end_takes(&mut old);
                    self.send_back(Retired::Armed(old));
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

    /// Ends the takes being recorded, if any, and any count-in.
    fn end_recording(&mut self) {
        if self.recording {
            end_takes(&mut self.armed);
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
