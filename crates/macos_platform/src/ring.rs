//! [`SampleRing`]: a lock-free single-producer, single-consumer ring of `f32` samples, carrying
//! input from cpal's input callback to its output callback (ADR-0052). Safe code: each slot is an
//! `AtomicU32` holding a sample's bits.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// A fixed-capacity ring of samples. One thread pushes, one pops; neither blocks or allocates.
#[derive(Debug)]
pub(crate) struct SampleRing {
    slots: Box<[AtomicU32]>,
    /// Samples ever pushed.
    written: AtomicUsize,
    /// Samples ever popped.
    read: AtomicUsize,
}

impl SampleRing {
    /// A ring of `capacity` samples (at least 1). Allocates.
    pub(crate) fn new(capacity: usize) -> Self {
        SampleRing {
            slots: (0..capacity.max(1)).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    /// Samples waiting to be popped.
    pub(crate) fn len(&self) -> usize {
        self.written
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Acquire))
    }

    /// Pushes as many of `samples` as fit, returning how many. Producer only.
    pub(crate) fn push(&self, samples: &[f32]) -> usize {
        let written = self.written.load(Ordering::Relaxed);
        let free = self.slots.len() - written.wrapping_sub(self.read.load(Ordering::Acquire));
        let n = samples.len().min(free);
        for (i, sample) in samples[..n].iter().enumerate() {
            let slot = written.wrapping_add(i) % self.slots.len();
            self.slots[slot].store(sample.to_bits(), Ordering::Relaxed);
        }
        self.written
            .store(written.wrapping_add(n), Ordering::Release);
        n
    }

    /// Pops up to `out.len()` samples into `out`, returning how many. Consumer only.
    pub(crate) fn pop(&self, out: &mut [f32]) -> usize {
        let read = self.read.load(Ordering::Relaxed);
        let available = self.written.load(Ordering::Acquire).wrapping_sub(read);
        let n = out.len().min(available);
        for (i, sample) in out[..n].iter_mut().enumerate() {
            let slot = read.wrapping_add(i) % self.slots.len();
            *sample = f32::from_bits(self.slots[slot].load(Ordering::Relaxed));
        }
        self.read.store(read.wrapping_add(n), Ordering::Release);
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_ring_wraps_and_refuses_overflow() {
        let ring = SampleRing::new(4);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0; 2];
        assert_eq!(ring.pop(&mut out), 2);
        assert_eq!(out, [1.0, 2.0]);
        assert_eq!(ring.push(&[4.0, 5.0, 6.0, 7.0]), 3);
        assert_eq!(ring.len(), 4);
        let mut out = [0.0; 8];
        assert_eq!(ring.pop(&mut out), 4);
        assert_eq!(&out[..4], &[3.0, 4.0, 5.0, 6.0]);
        assert_eq!(ring.pop(&mut out), 0);
    }

    #[test]
    fn test_ring_carries_every_sample_in_order_across_threads() -> Result<(), String> {
        let ring = Arc::new(SampleRing::new(64));
        let producer = Arc::clone(&ring);
        let total = 200_000usize;
        let thread = std::thread::spawn(move || {
            let mut next = 0usize;
            while next < total {
                let chunk: Vec<f32> = (next..(next + 7).min(total)).map(|i| i as f32).collect();
                next += producer.push(&chunk);
            }
        });
        let mut expected = 0usize;
        let mut out = [0.0; 5];
        while expected < total {
            let n = ring.pop(&mut out);
            for &sample in &out[..n] {
                if sample != expected as f32 {
                    return Err(format!("got {sample}, expected {expected}"));
                }
                expected += 1;
            }
        }
        thread.join().map_err(|_| "producer panicked".to_owned())
    }
}
