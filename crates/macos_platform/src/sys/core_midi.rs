//! Safe wrappers over the CoreMIDI and CoreAudio host-time calls that MIDI devices need
//! (`docs/architecture/platform.md` "MIDI devices", ADR-0146).
//!
//! - [`Client`]: the process's one CoreMIDI client, made on a thread of its own that runs a
//!   `CFRunLoop`, so that setup notifications never depend on the UI's run loop.
//! - [`sources`], [`destinations`], [`find`]: endpoint enumeration by unique ID.
//! - [`InputPort`] and [`OutputPort`]: MIDI 1.0 Universal MIDI Packet input and output.
//! - [`VirtualSource`] and [`VirtualDestination`]: in-process endpoints, for tests.
//! - [`now_nanos`], [`ticks_to_nanos`], [`nanos_to_ticks`]: the host clock.
//!
//! Every object made here is disposed on drop, except the client: Apple discourages disposing a
//! process's last client, because the MIDI server may then refuse new ones.

use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::mpsc;

use block2::RcBlock;
use objc2_core_audio::{
    AudioConvertHostTimeToNanos, AudioConvertNanosToHostTime, AudioGetCurrentHostTime,
};
use objc2_core_foundation::{
    CFRetained, CFRunLoop, CFRunLoopRunResult, CFString, kCFRunLoopDefaultMode,
};
use objc2_core_midi::{
    MIDIClientCreateWithBlock, MIDIClientRef, MIDIEndpointRef, MIDIEventList, MIDIEventPacket,
    MIDIGetDestination, MIDIGetNumberOfDestinations, MIDIGetNumberOfSources, MIDIGetSource,
    MIDIInputPortCreateWithProtocol, MIDINotification, MIDINotificationMessageID,
    MIDIObjectFindByUniqueID, MIDIObjectGetIntegerProperty, MIDIObjectGetStringProperty,
    MIDIObjectRef, MIDIObjectType, MIDIOutputPortCreate, MIDIPortConnectSource,
    MIDIPortDisconnectSource, MIDIPortDispose, MIDIPortRef, MIDIProtocolID, MIDISendEventList,
    kMIDIPropertyDisplayName, kMIDIPropertyOffline, kMIDIPropertyUniqueID,
};
#[cfg(test)]
use objc2_core_midi::{
    MIDIDestinationCreateWithProtocol, MIDIEndpointDispose, MIDIObjectSetIntegerProperty,
    MIDIReceivedEventList, MIDISourceCreateWithProtocol,
};

use super::ext_audio_file::{Status, check};

/// The most 32-bit words one event packet holds.
pub(crate) const PACKET_WORDS: usize = 64;

/// What each received packet is handed to: its timestamp in host ticks (0 for now) and its
/// Universal MIDI Packet words.
pub(crate) type Receiver = Box<dyn Fn(u64, &[u32]) + Send + Sync>;

/// The host clock now, in nanoseconds.
pub(crate) fn now_nanos() -> u64 {
    // SAFETY: both functions take and return plain integers and have no preconditions.
    unsafe { AudioConvertHostTimeToNanos(AudioGetCurrentHostTime()) }
}

/// Host-clock ticks as nanoseconds.
pub(crate) fn ticks_to_nanos(ticks: u64) -> u64 {
    // SAFETY: takes and returns a plain integer; no preconditions.
    unsafe { AudioConvertHostTimeToNanos(ticks) }
}

/// Nanoseconds as host-clock ticks.
pub(crate) fn nanos_to_ticks(nanos: u64) -> u64 {
    // SAFETY: takes and returns a plain integer; no preconditions.
    unsafe { AudioConvertNanosToHostTime(nanos) }
}

/// The process's CoreMIDI client.
#[derive(Debug)]
pub(crate) struct Client {
    client: MIDIClientRef,
}

impl Client {
    /// Makes the client on a new thread that runs a `CFRunLoop` for the rest of the process, and
    /// calls `notify` there whenever endpoints are added, removed, or change a property. Call it
    /// once per process.
    pub(crate) fn start(name: &str, notify: Box<dyn Fn() + Send>) -> Result<Client, Status> {
        let (reply, made) = mpsc::channel();
        let name = name.to_owned();
        std::thread::Builder::new()
            .name("coremidi-notify".into())
            .spawn(move || {
                let block = RcBlock::new(move |message: NonNull<MIDINotification>| {
                    // SAFETY: CoreMIDI passes a valid notification for the call's duration, and
                    // every notification begins with its message id.
                    let id = unsafe { message.as_ref() }.messageID;
                    if [
                        MIDINotificationMessageID::MsgSetupChanged,
                        MIDINotificationMessageID::MsgObjectAdded,
                        MIDINotificationMessageID::MsgObjectRemoved,
                        MIDINotificationMessageID::MsgPropertyChanged,
                    ]
                    .contains(&id)
                    {
                        notify();
                    }
                });
                let mut client: MIDIClientRef = 0;
                // SAFETY: `client` is a valid place to write the result; the block is valid and
                // CoreMIDI copies it. It is called on this thread's run loop, which runs below.
                let code = unsafe {
                    MIDIClientCreateWithBlock(
                        &CFString::from_str(&name),
                        NonNull::from(&mut client),
                        RcBlock::as_ptr(&block).cast(),
                    )
                };
                let ok = code == 0;
                // The caller waits for this; if it has gone, there is nobody to serve.
                if reply
                    .send(check("MIDIClientCreateWithBlock", code).map(|()| client))
                    .is_err()
                    || !ok
                {
                    return;
                }
                // SAFETY: the mode is CoreFoundation's own constant.
                let mode = unsafe { kCFRunLoopDefaultMode };
                loop {
                    let result = CFRunLoop::run_in_mode(mode, 1.0e9, false);
                    // A run loop with no sources returns at once; wait rather than spin.
                    if result == CFRunLoopRunResult::Finished {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            })
            .map_err(|_| Status {
                call: "std::thread::spawn",
                code: -1,
            })?;
        let client = made.recv().map_err(|_| Status {
            call: "MIDIClientCreateWithBlock",
            code: -1,
        })??;
        Ok(Client { client })
    }

    /// An input port receiving MIDI 1.0 Universal MIDI Packets, handing each packet to
    /// `receive` on CoreMIDI's receive thread.
    pub(crate) fn input_port(&self, name: &str, receive: Receiver) -> Result<InputPort, Status> {
        let block = receive_block(receive);
        let mut port: MIDIPortRef = 0;
        // SAFETY: `port` is a valid place to write the result; CoreMIDI copies the block, which
        // reads only the event list it is passed.
        let code = unsafe {
            MIDIInputPortCreateWithProtocol(
                self.client,
                &CFString::from_str(name),
                MIDIProtocolID::Protocol_1_0,
                NonNull::from(&mut port),
                RcBlock::as_ptr(&block).cast(),
            )
        };
        check("MIDIInputPortCreateWithProtocol", code)?;
        Ok(InputPort { port })
    }

    /// An output port.
    pub(crate) fn output_port(&self, name: &str) -> Result<OutputPort, Status> {
        let mut port: MIDIPortRef = 0;
        // SAFETY: `port` is a valid place to write the result.
        let code = unsafe {
            MIDIOutputPortCreate(
                self.client,
                &CFString::from_str(name),
                NonNull::from(&mut port),
            )
        };
        check("MIDIOutputPortCreate", code)?;
        Ok(OutputPort { port })
    }

    /// A virtual source other clients can receive from.
    #[cfg(test)]
    pub(crate) fn virtual_source(&self, name: &str) -> Result<VirtualSource, Status> {
        let mut endpoint: MIDIEndpointRef = 0;
        // SAFETY: `endpoint` is a valid place to write the result.
        let code = unsafe {
            MIDISourceCreateWithProtocol(
                self.client,
                &CFString::from_str(name),
                MIDIProtocolID::Protocol_1_0,
                NonNull::from(&mut endpoint),
            )
        };
        check("MIDISourceCreateWithProtocol", code)?;
        Ok(VirtualSource { endpoint })
    }

    /// A virtual destination other clients can send to, handing each packet to `receive`.
    #[cfg(test)]
    pub(crate) fn virtual_destination(
        &self,
        name: &str,
        receive: Receiver,
    ) -> Result<VirtualDestination, Status> {
        let block = receive_block(receive);
        let mut endpoint: MIDIEndpointRef = 0;
        // SAFETY: `endpoint` is a valid place to write the result; CoreMIDI copies the block.
        let code = unsafe {
            MIDIDestinationCreateWithProtocol(
                self.client,
                &CFString::from_str(name),
                MIDIProtocolID::Protocol_1_0,
                NonNull::from(&mut endpoint),
                RcBlock::as_ptr(&block).cast(),
            )
        };
        check("MIDIDestinationCreateWithProtocol", code)?;
        Ok(VirtualDestination { endpoint })
    }
}

/// A receive block that walks the event list and hands each MIDI 1.0 packet to `receive`.
fn receive_block(receive: Receiver) -> RcBlock<dyn Fn(NonNull<MIDIEventList>, *mut c_void)> {
    RcBlock::new(move |list: NonNull<MIDIEventList>, _: *mut c_void| {
        let list = list.as_ptr();
        // SAFETY: CoreMIDI passes a valid event list for the call's duration. Its header fields
        // are 4-byte aligned. Packets follow one another, each a 12-byte header (an 8-byte
        // timestamp at 4-byte alignment, then a word count) and its words; a packet never holds
        // more than `PACKET_WORDS`, and the walk stops if one claims to. Unaligned reads are used
        // for the timestamp, which is only 4-byte aligned.
        unsafe {
            if (*list).protocol != MIDIProtocolID::Protocol_1_0 {
                return;
            }
            let count = (*list).numPackets;
            let mut packet = ptr::addr_of!((*list).packet).cast::<u8>();
            for _ in 0..count {
                let timestamp = packet.cast::<u64>().read_unaligned();
                let words = packet.add(8).cast::<u32>().read_unaligned() as usize;
                if words > PACKET_WORDS {
                    return;
                }
                let data = std::slice::from_raw_parts(packet.add(12).cast::<u32>(), words);
                receive(timestamp, data);
                packet = packet.add(12 + 4 * words);
            }
        }
    })
}

/// A source or destination and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EndpointProps {
    /// The endpoint, valid while it exists.
    pub endpoint: MIDIEndpointRef,
    /// Its unique ID, stable across replugs.
    pub unique_id: i32,
    /// Its display name.
    pub name: String,
    /// Whether it is offline.
    pub offline: bool,
}

/// The sources present now.
pub(crate) fn sources() -> Vec<EndpointProps> {
    // SAFETY: both functions take and return plain integers; an index past the end yields 0.
    let count = unsafe { MIDIGetNumberOfSources() };
    (0..count)
        // SAFETY: as above.
        .filter_map(|i| props(unsafe { MIDIGetSource(i) }))
        .collect()
}

/// The destinations present now.
pub(crate) fn destinations() -> Vec<EndpointProps> {
    // SAFETY: as in `sources`.
    let count = unsafe { MIDIGetNumberOfDestinations() };
    (0..count)
        // SAFETY: as in `sources`.
        .filter_map(|i| props(unsafe { MIDIGetDestination(i) }))
        .collect()
}

/// Whether to look for a source or a destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A source.
    Source,
    /// A destination.
    Destination,
}

/// The endpoint of `kind` with `unique_id`, if one exists now.
pub(crate) fn find(unique_id: i32, kind: Kind) -> Option<EndpointProps> {
    let mut object: MIDIObjectRef = 0;
    let mut found = MIDIObjectType(-1);
    // SAFETY: both out-pointers are valid places to write.
    let code = unsafe { MIDIObjectFindByUniqueID(unique_id, &mut object, &mut found) };
    let wanted = match kind {
        Kind::Source => MIDIObjectType::Source,
        Kind::Destination => MIDIObjectType::Destination,
    };
    if code != 0 || found != wanted {
        return None;
    }
    props(object)
}

/// Which endpoint property to read.
#[derive(Debug, Clone, Copy)]
enum Property {
    UniqueId,
    Offline,
    DisplayName,
}

impl Property {
    fn name(self) -> &'static CFString {
        // SAFETY: CoreMIDI's property-name constants are immutable CFStrings that live for the
        // process.
        unsafe {
            match self {
                Property::UniqueId => kMIDIPropertyUniqueID,
                Property::Offline => kMIDIPropertyOffline,
                Property::DisplayName => kMIDIPropertyDisplayName,
            }
        }
    }
}

/// An endpoint's properties, or `None` if it has none (it has gone).
fn props(endpoint: MIDIEndpointRef) -> Option<EndpointProps> {
    if endpoint == 0 {
        return None;
    }
    let unique_id = integer_property(endpoint, Property::UniqueId)?;
    let offline = integer_property(endpoint, Property::Offline).unwrap_or(0) != 0;
    let name = string_property(endpoint, Property::DisplayName).unwrap_or_default();
    Some(EndpointProps {
        endpoint,
        unique_id,
        name,
        offline,
    })
}

fn integer_property(object: MIDIObjectRef, property: Property) -> Option<i32> {
    let mut value = 0;
    // SAFETY: `value` is a valid place to write; the property name is a live CFString.
    let code =
        unsafe { MIDIObjectGetIntegerProperty(object, property.name(), NonNull::from(&mut value)) };
    (code == 0).then_some(value)
}

fn string_property(object: MIDIObjectRef, property: Property) -> Option<String> {
    let mut value: *const CFString = ptr::null();
    // SAFETY: `value` is a valid place to write; the property name is a live CFString.
    let code =
        unsafe { MIDIObjectGetStringProperty(object, property.name(), NonNull::from(&mut value)) };
    let value = NonNull::new(value.cast_mut()).filter(|_| code == 0)?;
    // SAFETY: on success CoreMIDI returns a string the caller must release (its documentation
    // says so, despite the "Get" in the name), so it is adopted and released exactly once here.
    let value = unsafe { CFRetained::from_raw(value) };
    Some(value.to_string())
}

/// An input port, disposed on drop.
#[derive(Debug)]
pub(crate) struct InputPort {
    port: MIDIPortRef,
}

impl InputPort {
    /// Starts receiving from `source`.
    pub(crate) fn connect(&self, source: MIDIEndpointRef) -> Result<(), Status> {
        // SAFETY: a null connection reference is allowed.
        check("MIDIPortConnectSource", unsafe {
            MIDIPortConnectSource(self.port, source, ptr::null_mut())
        })
    }

    /// Stops receiving from `source`.
    pub(crate) fn disconnect(&self, source: MIDIEndpointRef) -> Result<(), Status> {
        // SAFETY: takes plain integers; an unknown source returns an error.
        check("MIDIPortDisconnectSource", unsafe {
            MIDIPortDisconnectSource(self.port, source)
        })
    }
}

impl Drop for InputPort {
    fn drop(&mut self) {
        // SAFETY: the port was made by this client and is disposed once. CoreMIDI waits for a
        // receive block in flight before the call returns.
        unsafe { MIDIPortDispose(self.port) };
    }
}

/// An output port, disposed on drop.
#[derive(Debug)]
pub(crate) struct OutputPort {
    port: MIDIPortRef,
}

/// A one-packet event list of MIDI 1.0 words at `timestamp`.
fn event_list(timestamp: u64, words: &[u32], call: &'static str) -> Result<MIDIEventList, Status> {
    if words.len() > PACKET_WORDS {
        return Err(Status { call, code: -50 });
    }
    let mut packet = MIDIEventPacket {
        timeStamp: timestamp,
        // At most `PACKET_WORDS`, so the cast is exact.
        wordCount: words.len() as u32,
        words: [0; PACKET_WORDS],
    };
    packet.words[..words.len()].copy_from_slice(words);
    Ok(MIDIEventList {
        protocol: MIDIProtocolID::Protocol_1_0,
        numPackets: 1,
        packet: [packet],
    })
}

impl OutputPort {
    /// Sends up to [`PACKET_WORDS`] MIDI 1.0 words to `destination` at `timestamp` host ticks,
    /// or now if it is 0.
    pub(crate) fn send(
        &self,
        destination: MIDIEndpointRef,
        timestamp: u64,
        words: &[u32],
    ) -> Result<(), Status> {
        let mut list = event_list(timestamp, words, "MIDISendEventList")?;
        // SAFETY: `list` is a complete one-packet event list that outlives the call.
        check("MIDISendEventList", unsafe {
            MIDISendEventList(self.port, destination, NonNull::from(&mut list))
        })
    }
}

impl Drop for OutputPort {
    fn drop(&mut self) {
        // SAFETY: the port was made by this client and is disposed once.
        unsafe { MIDIPortDispose(self.port) };
    }
}

/// A virtual source, disposed on drop.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct VirtualSource {
    endpoint: MIDIEndpointRef,
}

#[cfg(test)]
impl VirtualSource {
    /// Its unique ID.
    pub(crate) fn unique_id(&self) -> Option<i32> {
        integer_property(self.endpoint, Property::UniqueId)
    }

    /// Gives it `unique_id`, as a device keeps its ID across a replug.
    pub(crate) fn set_unique_id(&self, unique_id: i32) -> Result<(), Status> {
        // SAFETY: takes a live endpoint, CoreMIDI's constant property name and an integer.
        check("MIDIObjectSetIntegerProperty", unsafe {
            MIDIObjectSetIntegerProperty(self.endpoint, Property::UniqueId.name(), unique_id)
        })
    }

    /// Delivers up to [`PACKET_WORDS`] MIDI 1.0 words, stamped `timestamp` host ticks, to the
    /// ports connected to it.
    pub(crate) fn received(&self, timestamp: u64, words: &[u32]) -> Result<(), Status> {
        let mut list = event_list(timestamp, words, "MIDIReceivedEventList")?;
        // SAFETY: `list` is a complete one-packet event list that outlives the call.
        check("MIDIReceivedEventList", unsafe {
            MIDIReceivedEventList(self.endpoint, NonNull::from(&mut list))
        })
    }
}

#[cfg(test)]
impl Drop for VirtualSource {
    fn drop(&mut self) {
        // SAFETY: the endpoint was made by this client and is disposed once.
        unsafe { MIDIEndpointDispose(self.endpoint) };
    }
}

/// A virtual destination, disposed on drop.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct VirtualDestination {
    endpoint: MIDIEndpointRef,
}

#[cfg(test)]
impl VirtualDestination {
    /// Its unique ID.
    pub(crate) fn unique_id(&self) -> Option<i32> {
        integer_property(self.endpoint, Property::UniqueId)
    }
}

#[cfg(test)]
impl Drop for VirtualDestination {
    fn drop(&mut self) {
        // SAFETY: the endpoint was made by this client and is disposed once; CoreMIDI waits for
        // a receive block in flight.
        unsafe { MIDIEndpointDispose(self.endpoint) };
    }
}
