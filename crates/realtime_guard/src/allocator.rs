//! The guard allocator: the system allocator, with every call recorded against the calling thread.
//!
//! This is the crate's only `unsafe` code (`docs/architecture/crates.md` "Dependency rules").

// `unsafe` is required to implement `GlobalAlloc`; this module is the one place it is allowed.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};

use crate::state::{self, Kind};

/// A global allocator that forwards to [`System`] and records each call made on a thread inside
/// [`crate::assert_realtime`]. Install it in a test binary with `#[global_allocator]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct GuardAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, which upholds the
// `GlobalAlloc` contract, and returns `System`'s result unchanged. `state::record` neither
// allocates (so it cannot re-enter this allocator) nor panics, and it does not touch the memory.
unsafe impl GlobalAlloc for GuardAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        state::record(Kind::Alloc);
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`, which is the
        // same for `System`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        state::record(Kind::AllocZeroed);
        // SAFETY: as for `alloc`; the contract of `alloc_zeroed` is the same.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        state::record(Kind::Dealloc);
        // SAFETY: `ptr` was allocated by this allocator with `layout`, and this allocator only
        // ever obtains memory from `System`, so it is a `System` allocation with that layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        state::record(Kind::Realloc);
        // SAFETY: as for `dealloc`, `ptr` is a `System` allocation with `layout`; the caller
        // upholds `realloc`'s requirements on `new_size`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
