//! The real-time guard: fails a test when code that must be real-time safe allocates or frees heap
//! memory (`docs/architecture/engine.md` "Real-time thread", ADR-0010).
//!
//! # Usage
//!
//! This crate is a **dev-dependency only** (`docs/architecture/crates.md` "Dependency rules").
//! Each test binary that uses the guard must install [`GuardAllocator`] as its global allocator;
//! [`assert_realtime`] panics if it has not been installed, so a missing line cannot make the
//! guard pass silently.
//!
//! ```ignore
//! use realtime_guard::{GuardAllocator, assert_realtime};
//!
//! #[global_allocator]
//! static ALLOCATOR: GuardAllocator = GuardAllocator;
//!
//! #[test]
//! fn test_process_block_does_not_allocate() {
//!     let mut block = [0.0_f32; 32];
//!     assert_realtime(|| block.iter_mut().for_each(|s| *s *= 0.5));
//! }
//! ```
//!
//! Inside the closure, every `alloc`, `dealloc`, `realloc` and `alloc_zeroed` on the calling thread
//! is counted as a violation; allocations on other threads are not. Values the closure must not
//! free (a buffer it fills, say) should be created before the call and dropped after it.
//!
//! # Modules
//!
//! - `allocator`: the [`GuardAllocator`], the only `unsafe` code in the crate.
//! - `state`: the per-thread real-time flag and violation counters the allocator records into.
//! - `guard`: [`assert_realtime`], which marks a thread real-time for the length of a closure.

// `unsafe` is confined to the `allocator` module, which opts back in
// (`docs/architecture/crates.md` "Dependency rules").
#![deny(unsafe_code)]

mod allocator;
mod guard;
mod state;

pub use allocator::GuardAllocator;
pub use guard::assert_realtime;

// The crate's own unit tests run under the guard, as every user's tests must.
#[cfg(test)]
#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;
