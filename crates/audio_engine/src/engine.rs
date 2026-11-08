//! The control side: transport commands and track feeds, sent to the [`Processor`] over a
//! lock-free queue (`docs/architecture/engine.md` "Transport").
//!
//! These calls run off the real-time thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use audio_buffers::{Consumer, Producer, queue};
use signal_graph::{NodeId, Runner};

use crate::error::Error;
use crate::feed::{Streams, TrackFeeds};
use crate::processor::Processor;

/// Commands waiting for the real-time side at once.
const COMMANDS: usize = 8;

/// Where the transport is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    /// Not playing; the playhead is where it was stopped or moved to.
    Stopped,
    /// Asked to play, waiting until every track has 100 ms ready; outputs silence meanwhile.
    Starting,
    /// Playing.
    Playing,
    /// Not playing; the playhead holds where it paused.
    Paused,
}

impl TransportState {
    fn to_u8(self) -> u8 {
        match self {
            TransportState::Stopped => 0,
            TransportState::Starting => 1,
            TransportState::Playing => 2,
            TransportState::Paused => 3,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => TransportState::Starting,
            2 => TransportState::Playing,
            3 => TransportState::Paused,
            _ => TransportState::Stopped,
        }
    }
}

/// What the real-time side publishes, and the mono switch the control side sets.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    playhead: AtomicU64,
    state: AtomicU8,
    mono: AtomicBool,
}

impl Shared {
    pub(crate) fn publish(&self, state: TransportState, playhead: u64) {
        self.playhead.store(playhead, Ordering::Release);
        self.state.store(state.to_u8(), Ordering::Release);
    }

    pub(crate) fn mono(&self) -> bool {
        self.mono.load(Ordering::Relaxed)
    }
}

/// A request from the control side to the real-time side.
pub(crate) enum Command {
    /// Replace the track feeds.
    Tracks(Vec<TrackFeeds>),
    /// Play from `from` once every feed is primed; the feeds have been moved there.
    Play { from: u64 },
    /// Hold the playhead.
    Pause,
    /// Stop and move the playhead to `to`.
    Stop { to: u64 },
    /// Move the playhead while not playing.
    Locate { frame: u64 },
}

/// The control side of the engine.
pub struct Engine {
    commands: Producer<Command>,
    retired: Consumer<Vec<TrackFeeds>>,
    shared: Arc<Shared>,
    /// The transport as the control side last set it.
    state: TransportState,
    /// The playhead while stopped, as last set.
    position: u64,
    /// Where playback last started, for stop to return to.
    started_from: u64,
}

impl Engine {
    /// An engine running `runner`'s plans at `sample_rate`, taking the project output from sink
    /// `output`, with no tracks and the transport stopped at frame 0. Returns the control side
    /// and the real-time side.
    ///
    /// # Errors
    ///
    /// [`Error::Buffers`] if the queues cannot be made.
    pub fn new(
        runner: Runner,
        output: NodeId,
        sample_rate: u32,
    ) -> Result<(Engine, Processor), Error> {
        let (commands, commands_rx) = queue::channel(COMMANDS)?;
        let (retire, retired) = queue::channel(COMMANDS + 1)?;
        let shared = Arc::new(Shared::default());
        let processor = Processor::new(
            runner,
            output,
            sample_rate,
            commands_rx,
            retire,
            Arc::clone(&shared),
        );
        let engine = Engine {
            commands,
            retired,
            shared,
            state: TransportState::Stopped,
            position: 0,
            started_from: 0,
        };
        Ok((engine, processor))
    }

    /// Replaces the track feeds. Only while stopped or paused; the next [`Engine::play`] moves
    /// them to the playhead.
    ///
    /// # Errors
    ///
    /// [`Error::Playing`]; [`Error::Busy`].
    pub fn set_tracks(&mut self, tracks: Vec<TrackFeeds>) -> Result<(), Error> {
        if self.is_playing() {
            return Err(Error::Playing);
        }
        self.collect();
        self.send(Command::Tracks(tracks))
    }

    /// Starts playing from the playhead: moves every feed there, then plays once they are
    /// primed. Does nothing if already playing.
    ///
    /// # Errors
    ///
    /// [`Error::Streams`]; [`Error::Busy`], also while a pause has not yet taken effect.
    pub fn play(&mut self, streams: &mut dyn Streams) -> Result<(), Error> {
        let from = match self.state {
            TransportState::Starting | TransportState::Playing => return Ok(()),
            TransportState::Stopped => self.position,
            TransportState::Paused => {
                if self.real_time_state() != TransportState::Paused {
                    return Err(Error::Busy);
                }
                self.shared.playhead.load(Ordering::Acquire)
            }
        };
        self.start(from, streams)
    }

    /// Moves the playhead to `frame`. While playing, playback jumps there (after priming) and
    /// stop then returns there.
    ///
    /// # Errors
    ///
    /// [`Error::Streams`]; [`Error::Busy`].
    pub fn locate(&mut self, frame: u64, streams: &mut dyn Streams) -> Result<(), Error> {
        if self.is_playing() {
            return self.start(frame, streams);
        }
        self.send(Command::Locate { frame })?;
        self.position = frame;
        self.state = TransportState::Stopped;
        Ok(())
    }

    /// Pauses, holding the playhead.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`].
    pub fn pause(&mut self) -> Result<(), Error> {
        if !self.is_playing() {
            return Ok(());
        }
        self.send(Command::Pause)?;
        self.state = TransportState::Paused;
        Ok(())
    }

    /// Stops, returning the playhead to where playback last started.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`].
    pub fn stop(&mut self) -> Result<(), Error> {
        if self.state == TransportState::Stopped {
            return Ok(());
        }
        self.send(Command::Stop {
            to: self.started_from,
        })?;
        self.position = self.started_from;
        self.state = TransportState::Stopped;
        Ok(())
    }

    /// Turns mono listening on or off: both outputs carry `(L + R) × 0.7071`. Not saved.
    pub fn set_mono(&self, mono: bool) {
        self.shared.mono.store(mono, Ordering::Relaxed);
    }

    /// Whether mono listening is on.
    pub fn mono(&self) -> bool {
        self.shared.mono()
    }

    /// The playhead: while stopped, where stop or locate left it; otherwise the frame the
    /// real-time side will play next.
    pub fn playhead(&self) -> u64 {
        match self.state {
            TransportState::Stopped => self.position,
            _ => self.shared.playhead.load(Ordering::Acquire),
        }
    }

    /// The transport as the real-time side last reported it.
    pub fn real_time_state(&self) -> TransportState {
        TransportState::from_u8(self.shared.state.load(Ordering::Acquire))
    }

    /// Whether the transport is starting or playing, as last set here. Clip editing is disabled
    /// while it is.
    pub fn is_playing(&self) -> bool {
        matches!(
            self.state,
            TransportState::Starting | TransportState::Playing
        )
    }

    /// Drops track feeds the real-time side has replaced.
    pub fn collect(&mut self) {
        while let Some(tracks) = self.retired.try_pop() {
            drop(tracks);
        }
    }

    /// Moves the feeds to `from` and asks the real-time side to play from there.
    fn start(&mut self, from: u64, streams: &mut dyn Streams) -> Result<(), Error> {
        streams.seek(from).map_err(Error::Streams)?;
        self.send(Command::Play { from })?;
        self.started_from = from;
        self.state = TransportState::Starting;
        Ok(())
    }

    fn send(&mut self, command: Command) -> Result<(), Error> {
        self.commands.try_push(command).map_err(|_| Error::Busy)
    }
}
