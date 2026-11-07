//! The real-time side: takes commands at a block boundary and runs each block. Nothing here
//! blocks, locks or allocates.

use std::sync::Arc;

use audio_buffers::{Consumer, Producer};
use signal_graph::{NodeId, Runner};

use crate::block;
use crate::engine::{Command, Shared, TransportState};
use crate::error::Error;
use crate::feed::TrackFeeds;

/// Runs the engine on the real-time thread: call [`Processor::process`] once per device block.
///
/// Drop it off the real-time thread: it owns the plan, the feeds and queue ends.
pub struct Processor {
    runner: Runner,
    output: NodeId,
    tracks: Vec<TrackFeeds>,
    commands: Consumer<Command>,
    retire: Producer<Vec<TrackFeeds>>,
    /// Replaced feeds the retire queue had no room for yet.
    waiting: Option<Vec<TrackFeeds>>,
    shared: Arc<Shared>,
    state: TransportState,
    playhead: u64,
    /// Frames every feed must have ready before playback starts: 100 ms.
    prime: u64,
}

impl Processor {
    pub(crate) fn new(
        runner: Runner,
        output: NodeId,
        sample_rate: u32,
        commands: Consumer<Command>,
        retire: Producer<Vec<TrackFeeds>>,
        shared: Arc<Shared>,
    ) -> Self {
        Processor {
            runner,
            output,
            tracks: Vec::new(),
            commands,
            retire,
            waiting: None,
            shared,
            state: TransportState::Stopped,
            playhead: 0,
            prime: u64::from(sample_rate / 10),
        }
    }

    /// Runs one block of `left.len()` frames into `left` and `right`: takes pending commands,
    /// starts playback once every feed is primed, fills the track sources (silence unless
    /// playing), runs the plan and advances the playhead. The graph runs while stopped too, so
    /// plans swap and tails ring out.
    ///
    /// # Errors
    ///
    /// [`Error::BufferLength`], or [`Error::Graph`] for a block longer than the graph's
    /// largest; the output is then silence and the playhead does not move.
    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) -> Result<(), Error> {
        self.take_commands();
        if self.state == TransportState::Starting
            && self
                .tracks
                .iter()
                .flat_map(|t| t.feeds.iter())
                .all(|f| f.is_primed(self.prime))
        {
            self.state = TransportState::Playing;
        }
        let playing = self.state == TransportState::Playing;
        let result = block::run(
            &mut self.runner,
            self.output,
            &mut self.tracks,
            playing,
            self.shared.mono(),
            left,
            right,
        );
        if playing && result.is_ok() {
            self.playhead = self.playhead.saturating_add(left.len() as u64);
        }
        self.shared.publish(self.state, self.playhead);
        result
    }

    /// The plan's output latency, in frames.
    pub fn latency(&self) -> usize {
        self.runner.latency()
    }

    fn take_commands(&mut self) {
        if let Some(tracks) = self.waiting.take() {
            self.retire_tracks(tracks);
        }
        while let Some(command) = self.commands.try_pop() {
            match command {
                Command::Tracks(tracks) => {
                    let old = std::mem::replace(&mut self.tracks, tracks);
                    self.retire_tracks(old);
                }
                Command::Play { from } => {
                    self.playhead = from;
                    self.state = TransportState::Starting;
                }
                Command::Pause => {
                    if matches!(
                        self.state,
                        TransportState::Starting | TransportState::Playing
                    ) {
                        self.state = TransportState::Paused;
                    }
                }
                Command::Stop { to } => {
                    self.playhead = to;
                    self.state = TransportState::Stopped;
                }
                Command::Locate { frame } => self.playhead = frame,
            }
        }
    }

    /// Sends replaced feeds back to be dropped off the real-time thread.
    fn retire_tracks(&mut self, tracks: Vec<TrackFeeds>) {
        if let Err(tracks) = self.retire.try_push(tracks) {
            // Only one can wait; the control side collects before sending more, so a second
            // never arrives while one waits.
            self.waiting = Some(tracks);
        }
    }
}
