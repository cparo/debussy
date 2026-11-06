//! A test binary that forgot to install the guard allocator must not pass silently.

use std::panic::catch_unwind;

use realtime_guard::assert_realtime;

#[test]
fn test_assert_realtime_without_allocator_panics() {
    let outcome = catch_unwind(|| assert_realtime(|| ()));
    assert!(outcome.is_err());
}
