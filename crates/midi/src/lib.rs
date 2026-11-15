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

#![forbid(unsafe_code)]

mod buffer;
mod message;

pub use buffer::{EVENT_CAPACITY, Event, EventBuffer};
pub use message::{Kind, Message};
