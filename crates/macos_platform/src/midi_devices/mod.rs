//! [`MacMidi`]: MIDI devices through CoreMIDI (`docs/architecture/platform.md` "MIDI devices",
//! ADR-0146).
//!
//! The process has one CoreMIDI client, made on first use. Each [`MacMidi`] subscribes to its
//! setup notifications: on each one it first reconnects or disconnects its open inputs, then
//! calls its listeners. An endpoint is named by its CoreMIDI unique ID, so an input follows its
//! device across a replug, and an output looks its destination up on every send.

mod ump;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

use midi::{Message, TimedMessage};
use platform_interfaces::{
    Error, MidiDevices, MidiDirection, MidiEndpointId, MidiEndpointInfo, MidiInput,
    MidiInputCallback, MidiOutput,
};

use crate::sys::Status;
use crate::sys::core_midi::{self, Client, EndpointProps, InputPort, Kind, OutputPort};

/// The name the client and its ports show to other MIDI software.
const CLIENT_NAME: &str = "Debussy";

/// Every live [`MacMidi`], told of each setup change.
static SUBSCRIBERS: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());

/// The process's client, or why it could not be made.
static CLIENT: OnceLock<Result<Client, Status>> = OnceLock::new();

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn platform(status: Status) -> Error {
    Error::Platform(status.to_string())
}

/// The process's client, made on first use.
fn client() -> Result<&'static Client, Error> {
    CLIENT
        .get_or_init(|| Client::start(CLIENT_NAME, Box::new(notify_all)))
        .as_ref()
        .map_err(|&status| platform(status))
}

/// Tells every live [`MacMidi`] that the setup changed. Runs on the client's notification thread.
fn notify_all() {
    let subscribers: Vec<Arc<Shared>> = {
        let mut list = lock(&SUBSCRIBERS);
        list.retain(|s| s.strong_count() > 0);
        list.iter().filter_map(Weak::upgrade).collect()
    };
    for shared in subscribers {
        shared.changed();
    }
}

/// An endpoint's identifier from its unique ID.
fn endpoint_id(unique_id: i32) -> MidiEndpointId {
    MidiEndpointId(unique_id.to_string())
}

/// The unique ID an identifier names, or [`Error::MidiEndpointNotFound`].
fn unique_id(id: &MidiEndpointId) -> Result<i32, Error> {
    id.0.parse()
        .map_err(|_| Error::MidiEndpointNotFound(id.0.clone()))
}

/// The online endpoint of `kind` with `unique_id`, if there is one.
fn online(unique_id: i32, kind: Kind) -> Option<EndpointProps> {
    core_midi::find(unique_id, kind).filter(|p| !p.offline)
}

/// MIDI devices through CoreMIDI.
#[derive(Debug)]
pub struct MacMidi {
    shared: Arc<Shared>,
}

type Listener = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Shared {
    listeners: Mutex<Vec<Listener>>,
    inputs: Mutex<Vec<Weak<InputState>>>,
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
    }
}

impl Shared {
    fn changed(&self) {
        let inputs: Vec<Arc<InputState>> = {
            let mut list = lock(&self.inputs);
            list.retain(|i| i.strong_count() > 0);
            list.iter().filter_map(Weak::upgrade).collect()
        };
        for input in inputs {
            input.refresh();
        }
        let listeners = lock(&self.listeners).clone();
        for listener in listeners {
            listener();
        }
    }
}

impl MacMidi {
    /// Connects to CoreMIDI, making the process's client on first use.
    ///
    /// # Errors
    ///
    /// [`Error::Platform`] if CoreMIDI refuses a client.
    pub fn new() -> Result<Self, Error> {
        client()?;
        let shared = Arc::new(Shared::default());
        lock(&SUBSCRIBERS).push(Arc::downgrade(&shared));
        Ok(MacMidi { shared })
    }
}

impl MidiDevices for MacMidi {
    fn endpoints(&self) -> Result<Vec<MidiEndpointInfo>, Error> {
        let info = |direction| {
            move |p: EndpointProps| MidiEndpointInfo {
                id: endpoint_id(p.unique_id),
                name: p.name,
                direction,
                online: !p.offline,
            }
        };
        Ok(core_midi::sources()
            .into_iter()
            .map(info(MidiDirection::Input))
            .chain(
                core_midi::destinations()
                    .into_iter()
                    .map(info(MidiDirection::Output)),
            )
            .collect())
    }

    fn on_change(&self, listener: Box<dyn Fn() + Send + Sync>) {
        lock(&self.shared.listeners).push(Arc::from(listener));
    }

    fn open_input(
        &self,
        id: &MidiEndpointId,
        callback: Box<dyn MidiInputCallback>,
    ) -> Result<Box<dyn MidiInput>, Error> {
        let unique_id = unique_id(id)?;
        let source = core_midi::find(unique_id, Kind::Source)
            .ok_or_else(|| Error::MidiEndpointNotFound(id.0.clone()))?;
        let dropped = Arc::new(AtomicU64::new(0));
        let port = client()?
            .input_port(CLIENT_NAME, receiver(callback, Arc::clone(&dropped)))
            .map_err(platform)?;
        let connected = if source.offline {
            None
        } else {
            port.connect(source.endpoint).map_err(platform)?;
            Some(source.endpoint)
        };
        let state = Arc::new(InputState {
            id: id.clone(),
            unique_id,
            port,
            connected: Mutex::new(connected),
            stopped: AtomicBool::new(false),
            dropped,
        });
        lock(&self.shared.inputs).push(Arc::downgrade(&state));
        Ok(Box::new(MacInput { state }))
    }

    fn open_output(&self, id: &MidiEndpointId) -> Result<Box<dyn MidiOutput>, Error> {
        let unique_id = unique_id(id)?;
        core_midi::find(unique_id, Kind::Destination)
            .ok_or_else(|| Error::MidiEndpointNotFound(id.0.clone()))?;
        let port = client()?.output_port(CLIENT_NAME).map_err(platform)?;
        Ok(Box::new(MacOutput {
            id: id.clone(),
            unique_id,
            port,
        }))
    }

    fn host_nanos(&self) -> u64 {
        core_midi::now_nanos()
    }
}

/// The receive closure for an input: validates each packet's messages and hands the valid ones
/// to `callback`, counting the rest and any it refuses in `dropped`. It runs on CoreMIDI's
/// receive thread and neither blocks nor allocates; the lock is only ever tried, and it is never
/// contended because CoreMIDI calls a port's receiver from one thread.
fn receiver(callback: Box<dyn MidiInputCallback>, dropped: Arc<AtomicU64>) -> core_midi::Receiver {
    let callback = Mutex::new(callback);
    Box::new(move |ticks: u64, words: &[u32]| {
        let host_nanos = if ticks == 0 {
            core_midi::now_nanos()
        } else {
            core_midi::ticks_to_nanos(ticks)
        };
        ump::parse(words, |message| {
            let kept = message.is_some_and(|message| {
                callback.try_lock().is_ok_and(|mut c| {
                    c.receive(TimedMessage {
                        host_nanos,
                        message,
                    })
                })
            });
            if !kept {
                dropped.fetch_add(1, Ordering::Relaxed);
            }
        });
    })
}

struct InputState {
    id: MidiEndpointId,
    unique_id: i32,
    port: InputPort,
    /// The source the port is connected to now, if any.
    connected: Mutex<Option<u32>>,
    /// Set when the source went away, until reported.
    stopped: AtomicBool,
    dropped: Arc<AtomicU64>,
}

impl InputState {
    /// Follows the source: connects to it if it has come back, notes if it has gone.
    fn refresh(&self) {
        let now = online(self.unique_id, Kind::Source).map(|p| p.endpoint);
        let mut connected = lock(&self.connected);
        if *connected == now {
            return;
        }
        if let Some(old) = connected.take() {
            // The source may already be gone, which disconnects it anyway.
            let _ = self.port.disconnect(old);
        }
        match now {
            Some(source) if self.port.connect(source).is_ok() => *connected = Some(source),
            _ => self.stopped.store(true, Ordering::Release),
        }
    }
}

struct MacInput {
    state: Arc<InputState>,
}

impl MidiInput for MacInput {
    fn endpoint(&self) -> &MidiEndpointId {
        &self.state.id
    }

    fn take_error(&mut self) -> Option<Error> {
        self.state
            .stopped
            .swap(false, Ordering::AcqRel)
            .then(|| Error::StreamStopped(format!("MIDI source {} went away", self.state.id.0)))
    }

    fn dropped(&self) -> u64 {
        self.state.dropped.load(Ordering::Relaxed)
    }
}

struct MacOutput {
    id: MidiEndpointId,
    unique_id: i32,
    port: OutputPort,
}

impl MidiOutput for MacOutput {
    fn endpoint(&self) -> &MidiEndpointId {
        &self.id
    }

    fn send(&mut self, at_host_nanos: Option<u64>, messages: &[Message]) -> Result<(), Error> {
        let destination = online(self.unique_id, Kind::Destination).ok_or_else(|| {
            Error::StreamStopped(format!("MIDI destination {} is offline", self.id.0))
        })?;
        let timestamp = at_host_nanos.map_or(0, core_midi::nanos_to_ticks);
        let mut words = [0_u32; core_midi::PACKET_WORDS];
        for chunk in messages.chunks(core_midi::PACKET_WORDS) {
            for (word, message) in words.iter_mut().zip(chunk) {
                *word = ump::word(message);
            }
            self.port
                .send(destination.endpoint, timestamp, &words[..chunk.len()])
                .map_err(platform)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
