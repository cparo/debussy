//! The guard as a user crate sees it: installed as the test binary's global allocator.

use std::error::Error;
use std::hint::black_box;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

/// Runs `f` under the guard and returns the guard's panic message, or `None` if it passed.
fn violation_message(f: impl FnOnce()) -> Option<String> {
    let payload = catch_unwind(AssertUnwindSafe(|| assert_realtime(f))).err()?;
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    Some(message)
}

/// Asserts that the guard failed `f` with at least one violation of `kind`.
fn assert_caught(kind: &str, f: impl FnOnce()) -> Result<(), Box<dyn Error>> {
    let message = violation_message(f).ok_or("the guard did not fail the closure")?;
    // The message ends "...: <n> alloc, <n> alloc_zeroed, <n> realloc, <n> dealloc".
    let counts = message.rsplit(": ").next().unwrap_or_default();
    let caught = counts.split(", ").any(|part| {
        part.split_once(' ')
            .is_some_and(|(count, name)| name == kind && count != "0")
    });
    if caught {
        Ok(())
    } else {
        Err(format!("expected a {kind} violation, got: {message}").into())
    }
}

#[test]
fn test_assert_realtime_allocation_free_closure_passes() {
    let mut block = [0.25_f32; 32];
    let peak = assert_realtime(|| {
        block.iter_mut().for_each(|sample| *sample *= 0.5);
        block.iter().copied().fold(0.0_f32, f32::max)
    });
    assert_eq!(peak, 0.125);
}

#[test]
fn test_assert_realtime_preallocated_buffer_passes() {
    let mut buffer: Vec<f32> = Vec::with_capacity(64);
    assert_realtime(|| buffer.extend(std::iter::repeat_n(0.0, 64)));
    assert_eq!(buffer.len(), 64);
}

#[test]
fn test_assert_realtime_catches_vec_with_capacity() -> Result<(), Box<dyn Error>> {
    let mut kept = None;
    assert_caught("alloc", || kept = Some(Vec::<f32>::with_capacity(16)))
}

#[test]
fn test_assert_realtime_catches_box_new() -> Result<(), Box<dyn Error>> {
    let mut kept = None;
    assert_caught("alloc", || kept = Some(black_box(Box::new(1_u64))))
}

#[test]
fn test_assert_realtime_catches_string_growth() -> Result<(), Box<dyn Error>> {
    let mut name = String::with_capacity(1);
    name.push('a');
    assert_caught("realloc", || name.push_str("nother track"))
}

#[test]
fn test_assert_realtime_catches_box_drop() -> Result<(), Box<dyn Error>> {
    let boxed = black_box(Box::new([0_u8; 16]));
    assert_caught("dealloc", move || drop(boxed))
}

#[test]
fn test_assert_realtime_catches_alloc_zeroed() -> Result<(), Box<dyn Error>> {
    let mut kept = None;
    assert_caught("alloc_zeroed", || kept = Some(black_box(vec![0_u8; 256])))
}

#[test]
fn test_assert_realtime_reports_counts() -> Result<(), Box<dyn Error>> {
    let mut kept = Vec::new();
    kept.reserve_exact(4);
    let message = violation_message(|| {
        for i in 0..3 {
            kept.push(black_box(Box::new(i)));
        }
    })
    .ok_or("the guard did not fail the closure")?;
    assert!(message.starts_with("3 allocator call(s)"), "{message}");
    assert!(message.contains("3 alloc,"), "{message}");
    Ok(())
}

#[test]
fn test_assert_realtime_ignores_other_threads() -> Result<(), Box<dyn Error>> {
    let start = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let worker = {
        let (start, done) = (Arc::clone(&start), Arc::clone(&done));
        thread::spawn(move || {
            while !start.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            let mut strings = Vec::new();
            for i in 0..1000 {
                strings.push(i.to_string());
            }
            drop(black_box(strings));
            done.store(true, Ordering::Release);
        })
    };
    assert_realtime(|| {
        start.store(true, Ordering::Release);
        while !done.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
    });
    worker.join().map_err(|_| "worker thread panicked")?;
    Ok(())
}

#[test]
fn test_assert_realtime_returns_closure_result() {
    assert_eq!(assert_realtime(|| 6 * 7), 42);
}
