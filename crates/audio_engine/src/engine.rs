//! The control side: transport commands, track feeds and armed tracks, sent to the [`Processor`]
//! over a lock-free queue (`docs/architecture/engine.md` "Transport",
//! `docs/architecture/recording.md` "Capture").
//!
//! These calls run off the real-time thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use audio_buffers::{Consumer, Producer, queue};
use signal_graph::{NodeId, Runner};

use crate::capture::{ArmedTrack, MAX_INPUTS, TakeInfo};
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

/// Each device input's peak since the control side last read it, as `f32` bits. For
/// non-negative floats the bits order as the values do, so the largest is kept with
/// `fetch_max`.
#[derive(Debug)]
pub(crate) struct Peaks(Box<[AtomicU32]>);

impl Default for Peaks {
    fn default() -> Self {
        Peaks((0..MAX_INPUTS).map(|_| AtomicU32::new(0)).collect())
    }
}

/// What the real-time side publishes, and the mono switch the control side sets.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    input_peaks: Peaks,
    playhead: AtomicU64,
    state: AtomicU8,
    mono: AtomicBool,
    /// Whether the last recording's capture has begun; `take_start` and `take_latency` are set
    /// before it.
    take_started: AtomicBool,
    take_start: AtomicU64,
    take_latency: AtomicU64,
}

impl Shared {
    /// Raises device input `channel`'s peak to `peak` (a magnitude) if it is higher.
    pub(crate) fn raise_peak(&self, channel: usize, peak: f32) {
        if let Some(slot) = self.input_peaks.0.get(channel)
            && peak.is_finite()
        {
            slot.fetch_max(peak.abs().to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn start_take(&self, start_frame: u64, latency: u64) {
        self.take_start.store(start_frame, Ordering::Relaxed);
        self.take_latency.store(latency, Ordering::Relaxed);
        self.take_started.store(true, Ordering::Release);
    }

    pub(crate) fn publish(&self, state: TransportState, playhead: u64) {
        self.playhead.store(playhead, Ordering::Release);
        self.state.store(state.to_u8(), Ordering::Release);
    }

    pub(crate) fn mono(&self) -> bool {
        self.mono.load(Ordering::Relaxed)
    }
}

/// Something the real-time side replaced, sent back to be dropped.
pub(crate) enum Retired {
    Tracks(#[allow(dead_code)] Vec<TrackFeeds>), // Held only to be dropped.
    Armed(#[allow(dead_code)] Vec<ArmedTrack>),  // Held only to be dropped.
}

/// A request from the control side to the real-time side.
pub(crate) enum Command {
    /// Replace the track feeds.
    Tracks(Vec<TrackFeeds>),
    /// Replace the armed tracks, ending the old ones' takes.
    Arm(Vec<ArmedTrack>),
    /// Play from `from` once every feed is primed, recording the armed tracks if `record`; the
    /// feeds have been moved there.
    Play { from: u64, record: bool },
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
    retired: Consumer<Retired>,
    shared: Arc<Shared>,
    /// The transport as the control side last set it.
    state: TransportState,
    /// The playhead while stopped, as last set.
    position: u64,
    /// Where playback last started, for stop to return to.
    started_from: u64,
    /// Whether the transport was started by record and not since stopped or paused.
    recording: bool,
    /// Whether takes have been armed since the last recording.
    takes_armed: bool,
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
            recording: false,
            takes_armed: false,
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

    /// Replaces the armed tracks (`docs/architecture/recording.md` "Capture"), ending any takes
    /// the old ones held. Only while stopped or paused. Monitoring takes effect at once.
    ///
    /// # Errors
    ///
    /// [`Error::Playing`]; [`Error::Busy`].
    pub fn arm(&mut self, armed: Vec<ArmedTrack>) -> Result<(), Error> {
        if self.is_playing() {
            return Err(Error::Playing);
        }
        self.collect();
        let takes = armed.iter().any(|a| !a.sinks.is_empty());
        self.send(Command::Arm(armed))?;
        self.takes_armed = takes;
        Ok(())
    }

    /// Starts recording from the playhead: plays as [`Engine::play`] does, and captures the
    /// armed tracks' input into their takes from the first block played. Stop or pause ends the
    /// takes; arm new ones before recording again. [`Engine::take`] then tells where the take
    /// began.
    ///
    /// # Errors
    ///
    /// [`Error::NotArmed`] if no takes are armed; [`Error::Playing`] if already playing;
    /// otherwise as [`Engine::play`].
    pub fn record(&mut self, streams: &mut dyn Streams) -> Result<(), Error> {
        if self.is_playing() {
            return Err(Error::Playing);
        }
        if !self.takes_armed {
            return Err(Error::NotArmed);
        }
        let from = self.resume_point()?;
        self.shared.take_started.store(false, Ordering::Release);
        self.begin(from, true, streams)?;
        self.takes_armed = false;
        Ok(())
    }

    /// Where the last recording's capture began, once it has.
    pub fn take(&self) -> Option<TakeInfo> {
        self.shared
            .take_started
            .load(Ordering::Acquire)
            .then(|| TakeInfo {
                start_frame: self.shared.take_start.load(Ordering::Relaxed),
                latency: self.shared.take_latency.load(Ordering::Relaxed),
            })
    }

    /// Device input `channel`'s (from 0) peak magnitude since the last call for it, for an input
    /// meter; 0 for a channel beyond [`MAX_INPUTS`]. Inputs are measured only while a track is
    /// armed.
    pub fn take_input_peak(&self, channel: usize) -> f32 {
        self.shared
            .input_peaks
            .0
            .get(channel)
            .map_or(0.0, |slot| f32::from_bits(slot.swap(0, Ordering::Relaxed)))
    }

    /// Whether the transport is recording, as last set here.
    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Starts playing from the playhead: moves every feed there, then plays once they are
    /// primed. Does nothing if already playing.
    ///
    /// # Errors
    ///
    /// [`Error::Streams`]; [`Error::Busy`], also while a pause has not yet taken effect.
    pub fn play(&mut self, streams: &mut dyn Streams) -> Result<(), Error> {
        if self.is_playing() {
            return Ok(());
        }
        let from = self.resume_point()?;
        self.start(from, streams)
    }

    /// Where play or record starts: the playhead, or where a pause held it.
    fn resume_point(&self) -> Result<u64, Error> {
        Ok(match self.state {
            TransportState::Starting | TransportState::Playing => self.playhead(),
            TransportState::Stopped => self.position,
            TransportState::Paused => {
                if self.real_time_state() != TransportState::Paused {
                    return Err(Error::Busy);
                }
                self.shared.playhead.load(Ordering::Acquire)
            }
        })
    }

    /// Moves the playhead to `frame`. While playing, playback jumps there (after priming) and
    /// stop then returns there.
    ///
    /// # Errors
    ///
    /// [`Error::Recording`]; [`Error::Streams`]; [`Error::Busy`].
    pub fn locate(&mut self, frame: u64, streams: &mut dyn Streams) -> Result<(), Error> {
        if self.recording {
            return Err(Error::Recording);
        }
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
        self.recording = false;
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
        self.recording = false;
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

    /// Drops track feeds and armed tracks the real-time side has replaced.
    pub fn collect(&mut self) {
        while let Some(retired) = self.retired.try_pop() {
            drop(retired);
        }
    }

    /// Moves the feeds to `from` and asks the real-time side to play from there.
    fn start(&mut self, from: u64, streams: &mut dyn Streams) -> Result<(), Error> {
        self.begin(from, false, streams)
    }

    /// Moves the feeds to `from` and asks the real-time side to play, or record, from there.
    fn begin(&mut self, from: u64, record: bool, streams: &mut dyn Streams) -> Result<(), Error> {
        streams.seek(from).map_err(Error::Streams)?;
        self.send(Command::Play { from, record })?;
        self.started_from = from;
        self.state = TransportState::Starting;
        self.recording = record;
        Ok(())
    }

    fn send(&mut self, command: Command) -> Result<(), Error> {
        self.commands.try_push(command).map_err(|_| Error::Busy)
    }
}
