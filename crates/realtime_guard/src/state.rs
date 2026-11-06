//! Per-thread guard state: whether the thread is marked real-time, and the violations recorded
//! while it was.
//!
//! The allocator calls [`record`] from inside `GlobalAlloc`, so nothing here may allocate. The
//! thread-locals use `const` initialisers and types without `Drop`, which the standard library
//! stores without lazy registration or heap allocation.

use std::cell::Cell;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the allocator on every call, so the guard can tell whether it has been installed.
static INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Whether this thread is currently inside a guarded section.
    static REALTIME: Cell<bool> = const { Cell::new(false) };
    /// Violations recorded on this thread since the current section began.
    static COUNTS: Cell<Violations> = const { Cell::new(Violations::NONE) };
}

/// A kind of allocator call that is forbidden on a real-time thread.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Kind {
    /// `GlobalAlloc::alloc`.
    Alloc,
    /// `GlobalAlloc::alloc_zeroed`.
    AllocZeroed,
    /// `GlobalAlloc::realloc`.
    Realloc,
    /// `GlobalAlloc::dealloc`.
    Dealloc,
}

/// How many forbidden allocator calls of each kind happened.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Violations {
    /// Calls to `alloc`.
    pub(crate) alloc: u64,
    /// Calls to `alloc_zeroed`.
    pub(crate) alloc_zeroed: u64,
    /// Calls to `realloc`.
    pub(crate) realloc: u64,
    /// Calls to `dealloc`.
    pub(crate) dealloc: u64,
}

impl Violations {
    /// No violations.
    pub(crate) const NONE: Self = Self {
        alloc: 0,
        alloc_zeroed: 0,
        realloc: 0,
        dealloc: 0,
    };

    /// The total number of violations of every kind.
    pub(crate) fn total(self) -> u64 {
        self.alloc
            .saturating_add(self.alloc_zeroed)
            .saturating_add(self.realloc)
            .saturating_add(self.dealloc)
    }

    /// The element-wise sum of two sets of counts.
    fn plus(self, other: Self) -> Self {
        Self {
            alloc: self.alloc.saturating_add(other.alloc),
            alloc_zeroed: self.alloc_zeroed.saturating_add(other.alloc_zeroed),
            realloc: self.realloc.saturating_add(other.realloc),
            dealloc: self.dealloc.saturating_add(other.dealloc),
        }
    }

    /// These counts with one more call of `kind`.
    fn with(mut self, kind: Kind) -> Self {
        let count = match kind {
            Kind::Alloc => &mut self.alloc,
            Kind::AllocZeroed => &mut self.alloc_zeroed,
            Kind::Realloc => &mut self.realloc,
            Kind::Dealloc => &mut self.dealloc,
        };
        *count = count.saturating_add(1);
        self
    }
}

impl fmt::Display for Violations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} allocator call(s) on a real-time thread: \
             {} alloc, {} alloc_zeroed, {} realloc, {} dealloc",
            self.total(),
            self.alloc,
            self.alloc_zeroed,
            self.realloc,
            self.dealloc
        )
    }
}

/// Records one allocator call of `kind` on the current thread. Called from the allocator; must not
/// allocate.
pub(crate) fn record(kind: Kind) {
    INSTALLED.store(true, Ordering::Relaxed);
    // `try_with` fails only while the thread's locals are being torn down, when no guarded section
    // can be running, so the call is ignored.
    let _ = REALTIME.try_with(|realtime| {
        if realtime.get() {
            let _ = COUNTS.try_with(|counts| counts.set(counts.get().with(kind)));
        }
    });
}

/// Whether [`crate::GuardAllocator`] has handled any allocation in this process.
pub(crate) fn installed() -> bool {
    INSTALLED.load(Ordering::Relaxed)
}

/// Whether the current thread is inside a guarded section.
#[cfg(test)]
pub(crate) fn is_realtime() -> bool {
    REALTIME.with(Cell::get)
}

/// The state of the current thread saved on entering a guarded section, restored on leaving it.
pub(crate) struct Saved {
    /// The flag before entry (true when sections are nested).
    realtime: bool,
    /// The enclosing section's counts before entry.
    counts: Violations,
}

/// Marks the current thread real-time with fresh counts, returning what to restore on leaving.
pub(crate) fn enter() -> Saved {
    let saved = Saved {
        realtime: REALTIME.with(Cell::get),
        counts: COUNTS.with(Cell::get),
    };
    COUNTS.with(|counts| counts.set(Violations::NONE));
    REALTIME.with(|realtime| realtime.set(true));
    saved
}

/// Restores the state from before [`enter`] and returns the violations recorded since. An
/// enclosing section keeps the inner section's violations in its own counts.
pub(crate) fn leave(saved: Saved) -> Violations {
    REALTIME.with(|realtime| realtime.set(saved.realtime));
    let inner = COUNTS.with(Cell::get);
    COUNTS.with(|counts| counts.set(saved.counts.plus(inner)));
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_violations_with_counts_each_kind() {
        let v = Violations::NONE
            .with(Kind::Alloc)
            .with(Kind::AllocZeroed)
            .with(Kind::Realloc)
            .with(Kind::Dealloc)
            .with(Kind::Dealloc);
        assert_eq!(
            v,
            Violations {
                alloc: 1,
                alloc_zeroed: 1,
                realloc: 1,
                dealloc: 2
            }
        );
        assert_eq!(v.total(), 5);
    }

    #[test]
    fn test_violations_total_saturates() {
        let v = Violations {
            alloc: u64::MAX,
            alloc_zeroed: 1,
            realloc: 0,
            dealloc: 0,
        };
        assert_eq!(v.total(), u64::MAX);
        assert_eq!(v.with(Kind::Alloc).alloc, u64::MAX);
    }

    #[test]
    fn test_record_outside_section_is_ignored() {
        record(Kind::Alloc);
        assert_eq!(COUNTS.with(Cell::get), Violations::NONE);
    }

    #[test]
    fn test_leave_restores_and_accumulates_nested_counts() {
        let outer = enter();
        record(Kind::Alloc);
        let inner = enter();
        record(Kind::Dealloc);
        let inner_counts = leave(inner);
        assert!(REALTIME.with(Cell::get), "outer section still real-time");
        assert_eq!(inner_counts.dealloc, 1);
        assert_eq!(inner_counts.alloc, 0);
        let outer_counts = leave(outer);
        assert!(!REALTIME.with(Cell::get));
        assert_eq!(outer_counts.total(), 2);
    }
}
