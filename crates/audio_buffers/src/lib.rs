//! The bottom of the real-time core: sample blocks, preallocated block pools, and lock-free
//! single-producer/single-consumer queues for handing them between threads.
//!
//! Everything a real-time thread calls here is free of allocation, locks and blocking
//! (`docs/architecture/engine.md` "Real-time thread"). Allocation happens only in constructors,
//! which run off the real-time thread.
//!
//! # Modules
//!
//! - `block`: [`Block`], one buffer of `f32` samples, mono or stereo.
//! - `pool`: [`BlockPool`], a free list of preallocated blocks.
//! - `queue`: [`queue::channel`], the wait-free [`Producer`]/[`Consumer`] pair. Its module is the
//!   crate's only `unsafe` code (ADR-0028).
//! - `error`: [`Error`].
//!
//! # Handing blocks between threads
//!
//! A block moves by value, so sending one through a queue copies only its handle, never its
//! samples, and never allocates. To free something without freeing on the real-time thread, send
//! it through a queue to a thread that may free, and drop it there, for example with
//! [`Consumer::drain`].

#![deny(unsafe_code)]

mod block;
mod error;
mod pool;
pub mod queue;

pub use block::{Block, Layout, MAX_FRAMES, MIN_FRAMES};
pub use error::Error;
pub use pool::BlockPool;
pub use queue::{Consumer, Producer};
