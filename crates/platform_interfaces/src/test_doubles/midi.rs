//! [`FakeMidi`]: MIDI endpoints in memory.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use midi::{Message, TimedMessage};

use super::host_nanos;
use crate::error::Error;
use crate::midi_io::{
    MidiDevices, MidiDirection, MidiEndpointId, MidiEndpointInfo, MidiInput, MidiInputCallback,
    MidiOutput,
};

/// A message a [`FakeMidi`] output was asked to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMessage {
    /// The output endpoint.
    pub endpoint: MidiEndpointId,
    /// When it was to go out; `None` for now.
    pub at_host_nanos: Option<u64>,
    /// The message.
    pub message: Message,
}

/// Fake MIDI endpoints. Clones share the same endpoints. A test adds endpoints, unplugs and
/// replugs them, injects input with [`FakeMidi::inject`] or [`FakeMidi::inject_bytes`], and reads
/// what outputs sent with [`FakeMidi::sent`]. Listeners run on the thread that made the change.
#[derive(Clone, Default)]
pub struct FakeMidi {
    state: Arc<Mutex<State>>,
}

type Listener = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct State {
    endpoints: Vec<MidiEndpointInfo>,
    listeners: Vec<Listener>,
    inputs: Vec<Weak<InputShared>>,
    sent: Vec<SentMessage>,
}

struct InputShared {
    id: MidiEndpointId,
    callback: Mutex<Box<dyn MidiInputCallback>>,
    dropped: AtomicU64,
    stopped: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl std::fmt::Debug for FakeMidi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = lock(&self.state);
        f.debug_struct("FakeMidi")
            .field("endpoints", &state.endpoints)
            .field("sent", &state.sent)
            .finish_non_exhaustive()
    }
}

impl FakeMidi {
    /// No endpoints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an online endpoint and tells the listeners.
    pub fn add_endpoint(&self, id: &str, name: &str, direction: MidiDirection) {
        lock(&self.state).endpoints.push(MidiEndpointInfo {
            id: MidiEndpointId(id.to_owned()),
            name: name.to_owned(),
            direction,
            online: true,
        });
        self.notify();
    }

    /// Takes endpoint `id` offline: its open inputs report [`Error::StreamStopped`], its outputs
    /// refuse to send, and the listeners are told.
    pub fn unplug(&self, id: &str) {
        self.set_online(id, false);
    }

    /// Brings endpoint `id` back online; its open inputs and outputs work again.
    pub fn replug(&self, id: &str) {
        self.set_online(id, true);
    }

    /// Delivers `message` to the open inputs on endpoint `id`, as the platform's MIDI thread
    /// would. Returns how many inputs kept it.
    pub fn inject(&self, id: &str, message: TimedMessage) -> usize {
        self.open_inputs(id)
            .iter()
            .filter(|input| {
                let kept = lock(&input.callback).receive(message);
                if !kept {
                    input.dropped.fetch_add(1, Ordering::AcqRel);
                }
                kept
            })
            .count()
    }

    /// Like [`FakeMidi::inject`], but with raw bytes, which are dropped and counted unless they
    /// are one valid channel message.
    pub fn inject_bytes(&self, id: &str, host_nanos: u64, bytes: &[u8]) -> usize {
        match Message::from_bytes(bytes) {
            Some(message) => self.inject(
                id,
                TimedMessage {
                    host_nanos,
                    message,
                },
            ),
            None => {
                for input in self.open_inputs(id) {
                    input.dropped.fetch_add(1, Ordering::AcqRel);
                }
                0
            }
        }
    }

    /// Everything outputs have sent so far, in order.
    pub fn sent(&self) -> Vec<SentMessage> {
        lock(&self.state).sent.clone()
    }

    fn online(&self, id: &MidiEndpointId) -> bool {
        lock(&self.state)
            .endpoints
            .iter()
            .any(|e| e.id == *id && e.online)
    }

    fn open_inputs(&self, id: &str) -> Vec<Arc<InputShared>> {
        let mut state = lock(&self.state);
        state.inputs.retain(|w| w.strong_count() > 0);
        let online = state.endpoints.iter().any(|e| e.id.0 == id && e.online);
        if !online {
            return Vec::new();
        }
        state
            .inputs
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|input| input.id.0 == id)
            .collect()
    }

    fn set_online(&self, id: &str, online: bool) {
        {
            let mut state = lock(&self.state);
            for endpoint in state.endpoints.iter_mut().filter(|e| e.id.0 == id) {
                endpoint.online = online;
            }
            if !online {
                for input in state.inputs.iter().filter_map(Weak::upgrade) {
                    if input.id.0 == id {
                        input.stopped.store(true, Ordering::Release);
                    }
                }
            }
        }
        self.notify();
    }

    fn notify(&self) {
        let listeners = lock(&self.state).listeners.clone();
        for listener in listeners {
            listener();
        }
    }

    fn find(&self, id: &MidiEndpointId, direction: MidiDirection) -> Result<(), Error> {
        let found = lock(&self.state)
            .endpoints
            .iter()
            .any(|e| e.id == *id && e.direction == direction);
        if found {
            Ok(())
        } else {
            Err(Error::MidiEndpointNotFound(id.0.clone()))
        }
    }
}

impl MidiDevices for FakeMidi {
    fn endpoints(&self) -> Result<Vec<MidiEndpointInfo>, Error> {
        Ok(lock(&self.state).endpoints.clone())
    }

    fn on_change(&self, listener: Box<dyn Fn() + Send + Sync>) {
        lock(&self.state).listeners.push(Arc::from(listener));
    }

    fn open_input(
        &self,
        id: &MidiEndpointId,
        callback: Box<dyn MidiInputCallback>,
    ) -> Result<Box<dyn MidiInput>, Error> {
        self.find(id, MidiDirection::Input)?;
        let shared = Arc::new(InputShared {
            id: id.clone(),
            callback: Mutex::new(callback),
            dropped: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
        });
        lock(&self.state).inputs.push(Arc::downgrade(&shared));
        Ok(Box::new(FakeInput { shared }))
    }

    fn open_output(&self, id: &MidiEndpointId) -> Result<Box<dyn MidiOutput>, Error> {
        self.find(id, MidiDirection::Output)?;
        Ok(Box::new(FakeOutput {
            id: id.clone(),
            midi: self.clone(),
        }))
    }

    fn host_nanos(&self) -> u64 {
        host_nanos()
    }
}

struct FakeInput {
    shared: Arc<InputShared>,
}

impl MidiInput for FakeInput {
    fn endpoint(&self) -> &MidiEndpointId {
        &self.shared.id
    }

    fn take_error(&mut self) -> Option<Error> {
        self.shared.stopped.swap(false, Ordering::AcqRel).then(|| {
            Error::StreamStopped(format!("MIDI endpoint {:?} went away", self.shared.id.0))
        })
    }

    fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Acquire)
    }
}

struct FakeOutput {
    id: MidiEndpointId,
    midi: FakeMidi,
}

impl MidiOutput for FakeOutput {
    fn endpoint(&self) -> &MidiEndpointId {
        &self.id
    }

    fn send(&mut self, at_host_nanos: Option<u64>, messages: &[Message]) -> Result<(), Error> {
        if !self.midi.online(&self.id) {
            return Err(Error::StreamStopped(format!(
                "MIDI endpoint {:?} is offline",
                self.id.0
            )));
        }
        lock(&self.midi.state)
            .sent
            .extend(messages.iter().map(|&message| SentMessage {
                endpoint: self.id.clone(),
                at_host_nanos,
                message,
            }));
        Ok(())
    }
}
