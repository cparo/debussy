//! The test-facing entry point: run a closure with the current thread marked real-time.

use crate::state::{self, Saved, Violations};

/// Runs `f` with the current thread marked real-time and returns its result, panicking if `f`
/// allocated, reallocated or freed heap memory on this thread.
///
/// Calls may be nested; an outer call also fails on violations inside an inner one. If `f`
/// panics, the thread's real-time mark is still cleared and the panic propagates unchanged.
///
/// # Panics
///
/// - If [`crate::GuardAllocator`] is not the test binary's global allocator.
/// - If `f` made any allocator call on this thread; the message gives the count of each kind.
pub fn assert_realtime<R>(f: impl FnOnce() -> R) -> R {
    // `Box::new` makes an allocator call, which marks the allocator installed if it is.
    drop(std::hint::black_box(Box::new(0_u8)));
    assert!(
        state::installed(),
        "realtime_guard::GuardAllocator is not installed; add \
         `#[global_allocator] static ALLOCATOR: realtime_guard::GuardAllocator = \
         realtime_guard::GuardAllocator;` to the test binary"
    );

    let mut section = Section(Some(state::enter()));
    let result = f();
    let violations = section.leave();
    // The flag is cleared, so formatting the message may allocate.
    assert!(violations.total() == 0, "{violations}");
    result
}

/// A guarded section in progress. Dropping it, including while unwinding from a panic in the
/// closure, leaves the section.
struct Section(Option<Saved>);

impl Section {
    /// Leaves the section, returning the violations recorded in it.
    fn leave(&mut self) -> Violations {
        self.0.take().map_or(Violations::NONE, state::leave)
    }
}

impl Drop for Section {
    fn drop(&mut self) {
        self.leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn test_assert_realtime_clears_flag_after_panic() {
        let outcome = catch_unwind(|| assert_realtime(|| panic!("closure panicked")));
        assert!(outcome.is_err(), "the closure's panic propagates");
        assert!(!state::is_realtime());
    }

    #[test]
    fn test_assert_realtime_clears_flag_after_violation() {
        let outcome = catch_unwind(|| assert_realtime(|| drop(std::hint::black_box(Box::new(1)))));
        assert!(outcome.is_err());
        assert!(!state::is_realtime());
    }

    #[test]
    fn test_assert_realtime_nested_panic_restores_outer_flag() {
        let mut outer_still_realtime = false;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            assert_realtime(|| {
                let inner = catch_unwind(|| assert_realtime(|| panic!("inner")));
                outer_still_realtime = state::is_realtime();
                drop(inner);
            });
        }));
        assert!(
            outer_still_realtime,
            "the inner section restored the outer flag"
        );
        // Unwinding the inner panic allocated its payload, which the outer section must report.
        assert!(outcome.is_err());
        assert!(!state::is_realtime());
    }
}
