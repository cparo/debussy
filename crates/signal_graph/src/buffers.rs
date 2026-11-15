//! A plan's per-port buffers, and the delay that compensates an event connection
//! (`docs/architecture/signal-graph.md` "Port types" and "Delay compensation").

use std::sync::atomic::{AtomicU64, Ordering};

use audio_buffers::Block;
use midi::{EVENT_CAPACITY, Event, EventBuffer};

/// The events one compensated event connection can hold in flight (ADR-0143).
pub(crate) const PENDING_CAPACITY: usize = 4 * EVENT_CAPACITY;

/// The buffer behind one port, indexed by the port's index among its node's inputs or outputs.
pub(crate) enum PortBuffer {
    /// An audio port's samples.
    Audio(Block),
    /// An event port's events, stamped with frames into the block.
    Events(EventBuffer),
    /// A control port, which has no buffer.
    None,
}

impl PortBuffer {
    /// The audio block, if this is an audio port.
    pub(crate) fn audio(&self) -> Option<&Block> {
        match self {
            PortBuffer::Audio(block) => Some(block),
            _ => None,
        }
    }

    /// The audio block, mutably, if this is an audio port.
    pub(crate) fn audio_mut(&mut self) -> Option<&mut Block> {
        match self {
            PortBuffer::Audio(block) => Some(block),
            _ => None,
        }
    }

    /// The events, if this is an event port.
    pub(crate) fn events(&self) -> Option<&EventBuffer> {
        match self {
            PortBuffer::Events(events) => Some(events),
            _ => None,
        }
    }

    /// The events, mutably, if this is an event port.
    pub(crate) fn events_mut(&mut self) -> Option<&mut EventBuffer> {
        match self {
            PortBuffer::Events(events) => Some(events),
            _ => None,
        }
    }

    /// Starts a block of `frames`, which must fit the buffer: sets an audio block's length, and
    /// empties an event buffer.
    pub(crate) fn begin_block(&mut self, frames: usize) {
        match self {
            // Every block holds the graph's largest, which `frames` was checked against.
            PortBuffer::Audio(block) => {
                let _ = block.set_frames(frames);
            }
            PortBuffer::Events(events) => events.clear(),
            PortBuffer::None => {}
        }
    }
}

/// Counts an event dropped because a buffer was full.
pub(crate) fn count_overflow(overflows: &AtomicU64) {
    overflows.fetch_add(1, Ordering::Relaxed);
}

/// A frame offset as an event frame. Blocks are far shorter than `u32::MAX` frames.
pub(crate) fn frame(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

/// An event connection's compensation: events wait in a queue, stamped with the frame they are
/// due, relative to the current block's start.
pub(crate) struct EventDelay {
    compensation: u32,
    /// Empty and never used when `compensation` is 0.
    pending: EventBuffer,
}

impl EventDelay {
    /// A delay of `compensation` frames. Allocates its queue unless `compensation` is 0.
    pub(crate) fn new(compensation: usize) -> Self {
        let capacity = if compensation == 0 {
            0
        } else {
            PENDING_CAPACITY
        };
        EventDelay {
            compensation: frame(compensation),
            pending: EventBuffer::new(capacity),
        }
    }

    /// Passes `source`'s events in `start..start + frames` into `target`, delayed. Events due in
    /// that stretch are added to `target` after those already at the same frame. A full queue or
    /// target drops the event and counts it in `overflows`.
    pub(crate) fn carry(
        &mut self,
        source: &EventBuffer,
        target: &mut EventBuffer,
        start: usize,
        frames: usize,
        overflows: &AtomicU64,
    ) {
        let range = frame(start)..frame(start + frames);
        let mut deliver = |event: Event| {
            if !target.push(event) {
                count_overflow(overflows);
            }
        };
        if self.compensation == 0 {
            source.range(range).iter().copied().for_each(deliver);
            return;
        }
        for event in source.range(range.clone()) {
            let due = Event {
                frame: event.frame.saturating_add(self.compensation),
                message: event.message,
            };
            if !self.pending.push(due) {
                count_overflow(overflows);
            }
        }
        self.pending.take_before(range.end, &mut deliver);
    }

    /// Ends a block of `frames`: what is still pending becomes due that much sooner.
    pub(crate) fn end_block(&mut self, frames: usize) {
        self.pending.shift_earlier(frame(frames));
    }

    /// Drops every pending event.
    pub(crate) fn clear(&mut self) {
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use midi::Message;

    fn note(frame: u32, note: u8) -> Result<Event, &'static str> {
        Ok(Event {
            frame,
            message: Message::note_on(0, note, 1).ok_or("bad note")?,
        })
    }

    #[test]
    fn test_carry_delays_across_a_block() -> Result<(), &'static str> {
        let overflows = AtomicU64::new(0);
        let mut delay = EventDelay::new(40);
        let mut source = EventBuffer::new(4);
        let mut target = EventBuffer::new(4);
        source.push(note(10, 1)?);
        source.push(note(30, 2)?);
        delay.carry(&source, &mut target, 0, 64, &overflows);
        assert_eq!(target.iter().map(|e| e.frame).collect::<Vec<_>>(), [50]);
        delay.end_block(64);
        source.clear();
        target.clear();
        delay.carry(&source, &mut target, 0, 64, &overflows);
        assert_eq!(target.iter().map(|e| e.frame).collect::<Vec<_>>(), [6]);
        assert_eq!(overflows.load(Ordering::Relaxed), 0);
        Ok(())
    }

    #[test]
    fn test_carry_counts_a_full_target() -> Result<(), &'static str> {
        let overflows = AtomicU64::new(0);
        let mut delay = EventDelay::new(0);
        let mut source = EventBuffer::new(4);
        let mut target = EventBuffer::new(1);
        source.push(note(1, 1)?);
        source.push(note(2, 2)?);
        delay.carry(&source, &mut target, 0, 64, &overflows);
        assert_eq!(target.len(), 1);
        assert_eq!(overflows.load(Ordering::Relaxed), 1);
        Ok(())
    }
}
