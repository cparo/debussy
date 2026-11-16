//! MIDI devices: endpoint enumeration, hot-plug, time-stamped input and scheduled output
//! (`docs/architecture/platform.md` "MIDI devices").

use midi::{LiveProducer, Message, TimedMessage};

use crate::error::Error;

/// An endpoint's identifier, stable across replugs and restarts while the device keeps its
/// platform identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MidiEndpointId(pub String);

/// Which way MIDI flows through an endpoint, from the application's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MidiDirection {
    /// The application receives from it.
    Input,
    /// The application sends to it.
    Output,
}

/// What an endpoint is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiEndpointInfo {
    /// Its identifier.
    pub id: MidiEndpointId,
    /// Its name, for display.
    pub name: String,
    /// Which way it carries MIDI.
    pub direction: MidiDirection,
    /// Whether it can be used now.
    pub online: bool,
}

/// Code run on the platform's MIDI thread for each valid message an open input receives. It
/// must not allocate, lock, block, do I/O or print.
pub trait MidiInputCallback: Send + 'static {
    /// Takes `message`. Returns `false` if it dropped it, which the input counts.
    fn receive(&mut self, message: TimedMessage) -> bool;
}

impl MidiInputCallback for LiveProducer {
    fn receive(&mut self, message: TimedMessage) -> bool {
        self.push(message)
    }
}

/// An open input. Dropping it closes it and drops its callback.
pub trait MidiInput: Send {
    /// The endpoint it is open on.
    fn endpoint(&self) -> &MidiEndpointId;
    /// An error since the last call, if there was one: [`Error::StreamStopped`] when the
    /// endpoint went away. The input reconnects by itself when the endpoint returns.
    fn take_error(&mut self) -> Option<Error>;
    /// Messages dropped since it opened: those that were not valid channel messages, and those
    /// the callback refused.
    fn dropped(&self) -> u64;
}

/// An open output. Dropping it closes it.
pub trait MidiOutput: Send {
    /// The endpoint it is open on.
    fn endpoint(&self) -> &MidiEndpointId;
    /// Sends `messages`, in order, now if `at_host_nanos` is `None` or else at that host time.
    ///
    /// # Errors
    ///
    /// [`Error::StreamStopped`] while the endpoint is offline, or [`Error::Platform`].
    fn send(&mut self, at_host_nanos: Option<u64>, messages: &[Message]) -> Result<(), Error>;
}

/// The platform's MIDI endpoints.
pub trait MidiDevices: Send + Sync {
    /// The endpoints present now, inputs and outputs.
    ///
    /// # Errors
    ///
    /// [`Error::Platform`] if the platform cannot list them.
    fn endpoints(&self) -> Result<Vec<MidiEndpointInfo>, Error>;

    /// Calls `listener`, on a platform thread, after the set of endpoints or their state
    /// changes. Listeners are kept until the devices are dropped.
    fn on_change(&self, listener: Box<dyn Fn() + Send + Sync>);

    /// Opens the input endpoint `id`, calling `callback` on the platform's MIDI thread with each
    /// valid channel message.
    ///
    /// # Errors
    ///
    /// [`Error::MidiEndpointNotFound`] or [`Error::Platform`].
    fn open_input(
        &self,
        id: &MidiEndpointId,
        callback: Box<dyn MidiInputCallback>,
    ) -> Result<Box<dyn MidiInput>, Error>;

    /// Opens the output endpoint `id`.
    ///
    /// # Errors
    ///
    /// [`Error::MidiEndpointNotFound`] or [`Error::Platform`].
    fn open_output(&self, id: &MidiEndpointId) -> Result<Box<dyn MidiOutput>, Error>;

    /// The host clock now, in nanoseconds: the clock of [`TimedMessage::host_nanos`],
    /// [`MidiOutput::send`] and [`crate::Buffers::host_nanos`].
    fn host_nanos(&self) -> u64;
}
