//! The real-time end of one MIDI take. Nothing here blocks, locks or allocates.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use audio_buffers::Producer;

/// The most messages one MIDI take holds while waiting for the writer thread; beyond it, a
/// message is dropped and counted as an overrun.
pub const MIDI_TAKE_CAPACITY: usize = 4096;

/// A captured MIDI message: its take frame and its bytes (status, then data; an unused data byte
/// is 0).
pub type CapturedMessage = (u64, [u8; 3]);

/// State one MIDI take shares between the real-time side, the writer and the control side.
#[derive(Debug, Default)]
pub(crate) struct MidiTakeShared {
    /// Messages dropped because the queue was full.
    pub(crate) overruns: AtomicU64,
    /// The take's length in frames; set before `ended`.
    pub(crate) frames: AtomicU64,
    /// Whether the take has ended: every message it kept has been queued.
    pub(crate) ended: AtomicBool,
}

/// The real-time end of one MIDI take: queues each message for the writer thread, stamped with
/// its take frame (`docs/architecture/recording.md` "MIDI recording").
///
/// Drop it off the real-time thread: it owns a queue end. Dropping it ends the take at the frame
/// after the last message.
#[derive(Debug)]
pub struct MidiTake {
    shared: Arc<MidiTakeShared>,
    messages: Producer<CapturedMessage>,
    /// The frame after the last message queued.
    last: u64,
    ended: bool,
}

impl MidiTake {
    pub(crate) fn new(shared: Arc<MidiTakeShared>, messages: Producer<CapturedMessage>) -> Self {
        MidiTake {
            shared,
            messages,
            last: 0,
            ended: false,
        }
    }

    /// Queues `bytes` at take frame `frame` and returns whether it was kept: if the queue is
    /// full, because the writer has fallen behind, it is dropped and an overrun counted. Does
    /// nothing once ended. Real-time safe.
    pub fn write(&mut self, frame: u64, bytes: [u8; 3]) -> bool {
        if self.ended {
            return false;
        }
        if self.messages.try_push((frame, bytes)).is_err() {
            self.shared.overruns.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.last = self.last.max(frame.saturating_add(1));
        true
    }

    /// Ends the take, `frames` frames long, and tells the writer. Later writes are ignored.
    /// Real-time safe.
    pub fn end(&mut self, frames: u64) {
        if self.ended {
            return;
        }
        self.shared.frames.store(frames, Ordering::Relaxed);
        self.shared.ended.store(true, Ordering::Release);
        self.ended = true;
    }

    /// Overruns counted so far.
    pub fn overruns(&self) -> u64 {
        self.shared.overruns.load(Ordering::Relaxed)
    }
}

impl Drop for MidiTake {
    fn drop(&mut self) {
        self.end(self.last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use audio_buffers::queue;

    #[test]
    fn test_write_counts_an_overrun_when_the_queue_is_full()
    -> Result<(), Box<dyn std::error::Error>> {
        let shared = Arc::new(MidiTakeShared::default());
        let (tx, mut rx) = queue::channel(2)?;
        let mut take = MidiTake::new(Arc::clone(&shared), tx);
        assert!(take.write(1, [0x90, 60, 100]));
        assert!(take.write(2, [0x80, 60, 0]));
        assert!(!take.write(3, [0x90, 61, 100]));
        assert_eq!(take.overruns(), 1);
        assert_eq!(rx.try_pop(), Some((1, [0x90, 60, 100])));
        take.end(10);
        assert!(!take.write(4, [0x90, 62, 100]));
        assert_eq!(shared.frames.load(Ordering::Relaxed), 10);
        assert!(shared.ended.load(Ordering::Acquire));
        Ok(())
    }

    #[test]
    fn test_drop_ends_after_the_last_message() -> Result<(), Box<dyn std::error::Error>> {
        let shared = Arc::new(MidiTakeShared::default());
        let (tx, _rx) = queue::channel(4)?;
        let mut take = MidiTake::new(Arc::clone(&shared), tx);
        take.write(41, [0x90, 60, 100]);
        drop(take);
        assert_eq!(shared.frames.load(Ordering::Relaxed), 42);
        Ok(())
    }
}
