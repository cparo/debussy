//! MIDI 1.0 channel messages and the fixed-capacity event buffers that carry them through the
//! processing graph (`docs/architecture/signal-graph.md` "Port types", ADR-0143).
//!
//! Everything here is real-time safe once built: an [`EventBuffer`] allocates only in
//! [`EventBuffer::new`], and nothing else allocates, locks or does I/O
//! (`docs/architecture/engine.md` "Real-time thread").
//!
//! # Modules
//!
//! - `message`: [`Message`], a validated channel message, and [`Kind`], its decoded form.
//! - `buffer`: [`Event`], a message stamped with a frame, and [`EventBuffer`], a frame-ordered
//!   buffer of them with a fixed capacity.
//! - `live`: [`TimedMessage`] and the lock-free [`LiveProducer`]/[`LiveInput`] pair that carries
//!   live input to the audio thread and stamps it with frames (ADR-0146).
//! - `outgoing`: [`Outgoing`] and the lock-free [`OutProducer`]/[`OutConsumer`] pair that
//!   carries events leaving the graph to a MIDI output's sender (ADR-0162).
//! - `journal`: the byte format of a MIDI take's crash journal, `take-N.midilog` (ADR-0151).

#![forbid(unsafe_code)]

mod buffer;
mod journal;
mod live;
mod message;
mod outgoing;

/// The real-time guard's allocator, for the unit tests that run code inside the guard.
#[cfg(test)]
#[global_allocator]
static ALLOCATOR: realtime_guard::GuardAllocator = realtime_guard::GuardAllocator;

pub use buffer::{EVENT_CAPACITY, Event, EventBuffer};
pub use journal::{
    JOURNAL_HEADER_BYTES, JOURNAL_MAGIC, JOURNAL_RECORD_BYTES, JOURNAL_VERSION, JournalError,
    journal_header, journal_record, parse_journal_header, parse_journal_record,
};
pub use live::{LIVE_QUEUE_CAPACITY, LiveInput, LiveProducer, TimedMessage, channel};
pub use message::{Kind, Message};
pub use outgoing::{OUT_QUEUE_CAPACITY, OutConsumer, OutProducer, Outgoing, out_channel};
