//! A bounded, lock-free single-producer/single-consumer queue. Both ends are wait-free: every
//! operation finishes in a bounded number of steps, never blocks, and never allocates.
//!
//! This module is the crate's only `unsafe` code ([ADR-0028], `docs/architecture/crates.md`
//! "Dependency rules").
//!
//! # Design
//!
//! Items live in a ring of `capacity` slots. Two positions count from 0 up to `2 * capacity` and
//! wrap: `tail`, the next slot to write, owned by the [`Producer`], and `head`, the next slot to
//! read, owned by the [`Consumer`]. Counting to twice the capacity tells a full ring
//! (`tail - head == capacity`) from an empty one (`tail == head`).
//!
//! The invariant every `unsafe` block relies on: the slots from `head` up to `tail` hold
//! initialised items and belong to the consumer; every other slot is uninitialised and belongs to
//! the producer. Each side publishes a moved position with a `Release` store after touching the
//! slot, and reads the other side's position with an `Acquire` load before touching a slot, so a
//! slot is never accessed by both sides at once and its contents are visible when it changes
//! hands. Each side also caches the other's last known position to avoid touching the shared
//! cache line on every call.
//!
//! [ADR-0028]: https://github.com/cparo/debussy/blob/main/docs/adrs/0028-unsafe-is-allowed-in-audio-buffers-queue-module.md

// The ring's slots are `UnsafeCell<MaybeUninit<T>>` guarded by atomic positions (ADR-0028).
#![allow(unsafe_code)]

use std::cell::UnsafeCell;
use std::fmt;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::Error;

/// Creates a queue holding up to `capacity` items, returning its two ends. Allocates; call off
/// the real-time thread.
///
/// Dropping the second end frees the ring, along with any items still in it, so drop the ends off
/// the real-time thread.
///
/// # Errors
///
/// [`Error::ZeroCapacity`] if `capacity` is 0, or [`Error::QueueTooLarge`] if positions up to
/// `2 * capacity` would not fit in a `usize`.
pub fn channel<T>(capacity: usize) -> Result<(Producer<T>, Consumer<T>), Error> {
    if capacity == 0 {
        return Err(Error::ZeroCapacity);
    }
    if capacity > usize::MAX / 2 {
        return Err(Error::QueueTooLarge(capacity));
    }
    let slots = (0..capacity)
        .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
        .collect();
    let shared = Arc::new(Shared {
        slots,
        capacity,
        head: Padded(AtomicUsize::new(0)),
        tail: Padded(AtomicUsize::new(0)),
    });
    Ok((
        Producer {
            shared: Arc::clone(&shared),
            tail: 0,
            cached_head: 0,
        },
        Consumer {
            shared,
            head: 0,
            cached_tail: 0,
        },
    ))
}

/// Keeps each position on its own cache line, so the two threads do not contend for one.
#[repr(align(128))]
struct Padded(AtomicUsize);

/// The state both ends share.
struct Shared<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    capacity: usize,
    /// The next position to read; stored only by the consumer.
    head: Padded,
    /// The next position to write; stored only by the producer.
    tail: Padded,
}

// SAFETY: the two ends access `slots` from different threads, but by the module invariant a slot
// belongs to exactly one side at a time, and ownership passes with Release/Acquire on `head` and
// `tail`. Items move between threads, so `T` must be `Send`; no `&T` is ever shared, so `T` need
// not be `Sync`.
unsafe impl<T: Send> Sync for Shared<T> {}

impl<T> Shared<T> {
    /// The position after `position`.
    fn next(&self, position: usize) -> usize {
        let next = position + 1;
        if next == 2 * self.capacity { 0 } else { next }
    }

    /// The number of items between `head` and `tail`.
    fn len(&self, head: usize, tail: usize) -> usize {
        if tail >= head {
            tail - head
        } else {
            tail + 2 * self.capacity - head
        }
    }

    /// The slot a position refers to.
    fn slot(&self, position: usize) -> &UnsafeCell<MaybeUninit<T>> {
        let index = if position >= self.capacity {
            position - self.capacity
        } else {
            position
        };
        &self.slots[index]
    }
}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let mut head = *self.head.0.get_mut();
        let tail = *self.tail.0.get_mut();
        while head != tail {
            // SAFETY: `&mut self` means both ends are gone, and `Arc`'s drop synchronises with
            // their last stores, so the slots from `head` to `tail` hold initialised items that
            // nothing else can reach. Each is dropped exactly once.
            unsafe { (*self.slot(head).get()).assume_init_drop() };
            head = self.next(head);
        }
    }
}

/// The writing end of a queue. There is exactly one; it can be sent to another thread.
pub struct Producer<T> {
    shared: Arc<Shared<T>>,
    /// The next position to write (the producer's copy of `shared.tail`).
    tail: usize,
    /// The consumer's position when last read.
    cached_head: usize,
}

impl<T> Producer<T> {
    /// Adds `item` to the queue, or gives it back if the queue is full. Wait-free and
    /// allocation-free.
    ///
    /// # Errors
    ///
    /// Returns `item` if the queue is full.
    pub fn try_push(&mut self, item: T) -> Result<(), T> {
        let shared = &*self.shared;
        if shared.len(self.cached_head, self.tail) == shared.capacity {
            self.cached_head = shared.head.0.load(Ordering::Acquire);
            if shared.len(self.cached_head, self.tail) == shared.capacity {
                return Err(item);
            }
        }
        // SAFETY: the queue is not full, so the slot at `tail` is outside head..tail and belongs
        // to the producer: the Acquire load of `head` above (or an earlier one) saw the consumer
        // finish reading it. It is uninitialised, so writing does not leak an item.
        unsafe { (*shared.slot(self.tail).get()).write(item) };
        self.tail = shared.next(self.tail);
        shared.tail.0.store(self.tail, Ordering::Release);
        Ok(())
    }

    /// The most items the queue can hold.
    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }
}

impl<T> fmt::Debug for Producer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Producer")
            .field("capacity", &self.shared.capacity)
            .finish_non_exhaustive()
    }
}

/// The reading end of a queue. There is exactly one; it can be sent to another thread.
pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
    /// The next position to read (the consumer's copy of `shared.head`).
    head: usize,
    /// The producer's position when last read.
    cached_tail: usize,
}

impl<T> Consumer<T> {
    /// Removes the oldest item, or returns `None` if the queue is empty. Wait-free and
    /// allocation-free.
    pub fn try_pop(&mut self) -> Option<T> {
        let shared = &*self.shared;
        if self.head == self.cached_tail {
            self.cached_tail = shared.tail.0.load(Ordering::Acquire);
            if self.head == self.cached_tail {
                return None;
            }
        }
        // SAFETY: `head != tail`, so the slot at `head` is inside head..tail: the Acquire load of
        // `tail` saw the producer's write to it, it holds an initialised item, and it belongs to
        // the consumer. Moving `head` past it below hands the now-uninitialised slot back, so the
        // item is read exactly once.
        let item = unsafe { (*shared.slot(self.head).get()).assume_init_read() };
        self.head = shared.next(self.head);
        shared.head.0.store(self.head, Ordering::Release);
        Some(item)
    }

    /// Removes items until the queue is empty, as seen when each is taken. For a thread that may
    /// free memory, to drop what a real-time thread has sent it.
    pub fn drain(&mut self) -> impl Iterator<Item = T> + '_ {
        std::iter::from_fn(move || self.try_pop())
    }

    /// The most items the queue can hold.
    pub fn capacity(&self) -> usize {
        self.shared.capacity
    }
}

impl<T> fmt::Debug for Consumer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Consumer")
            .field("capacity", &self.shared.capacity)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;
    use std::rc::Rc;

    #[test]
    fn test_channel_rejects_bad_capacity() {
        assert!(matches!(channel::<u8>(0), Err(Error::ZeroCapacity)));
        assert!(matches!(
            channel::<()>(usize::MAX / 2 + 1),
            Err(Error::QueueTooLarge(_))
        ));
    }

    #[test]
    fn test_try_push_fails_when_full_and_try_pop_when_empty() -> Result<(), Box<dyn StdError>> {
        let (mut tx, mut rx) = channel(2)?;
        assert_eq!(rx.try_pop(), None);
        tx.try_push(1).map_err(|_| "full")?;
        tx.try_push(2).map_err(|_| "full")?;
        assert_eq!(tx.try_push(3), Err(3));
        assert_eq!(rx.try_pop(), Some(1));
        tx.try_push(3).map_err(|_| "full")?;
        assert_eq!(rx.drain().collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(rx.try_pop(), None);
        Ok(())
    }

    #[test]
    fn test_positions_wrap_many_times() -> Result<(), Box<dyn StdError>> {
        let (mut tx, mut rx) = channel(3)?;
        for i in 0..100 {
            tx.try_push(i).map_err(|_| "full")?;
            if i % 2 == 1 {
                assert_eq!(rx.try_pop(), Some(i - 1));
                assert_eq!(rx.try_pop(), Some(i));
            }
        }
        Ok(())
    }

    #[test]
    fn test_capacity_one() -> Result<(), Box<dyn StdError>> {
        let (mut tx, mut rx) = channel(1)?;
        for i in 0..5 {
            tx.try_push(i).map_err(|_| "full")?;
            assert_eq!(tx.try_push(99), Err(99));
            assert_eq!(rx.try_pop(), Some(i));
        }
        assert_eq!((tx.capacity(), rx.capacity()), (1, 1));
        Ok(())
    }

    #[test]
    fn test_drop_frees_items_left_in_queue() -> Result<(), Box<dyn StdError>> {
        let marker = Rc::new(());
        let (mut tx, mut rx) = channel(4)?;
        for _ in 0..4 {
            tx.try_push(Rc::clone(&marker)).map_err(|_| "full")?;
        }
        drop(rx.try_pop());
        assert_eq!(Rc::strong_count(&marker), 4);
        drop(tx);
        assert_eq!(
            Rc::strong_count(&marker),
            4,
            "consumer still holds the ring"
        );
        drop(rx);
        assert_eq!(Rc::strong_count(&marker), 1);
        Ok(())
    }

    #[test]
    fn test_drop_after_wrap_frees_exactly_the_held_items() -> Result<(), Box<dyn StdError>> {
        let marker = Rc::new(());
        let (mut tx, mut rx) = channel(3)?;
        for _ in 0..5 {
            tx.try_push(Rc::clone(&marker)).map_err(|_| "full")?;
            drop(rx.try_pop());
        }
        tx.try_push(Rc::clone(&marker)).map_err(|_| "full")?;
        tx.try_push(Rc::clone(&marker)).map_err(|_| "full")?;
        drop((tx, rx));
        assert_eq!(Rc::strong_count(&marker), 1);
        Ok(())
    }
}
