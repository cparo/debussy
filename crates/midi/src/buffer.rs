//! Frame-stamped events and the fixed-capacity buffers that hold them.

use std::ops::Range;

use crate::message::Message;

/// The events one event port can carry in one block (ADR-0143). DIN MIDI moves about 31
/// messages in 10 ms, and the largest block is 1024 frames (about 23 ms at 44.1 kHz).
pub const EVENT_CAPACITY: usize = 512;

/// A message at a frame: the offset into the block it falls in, or, in a delay, the frame it is
/// due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Event {
    /// The frame.
    pub frame: u32,
    /// The message.
    pub message: Message,
}

/// Events kept in frame order, with a capacity fixed when the buffer is made. Events at the same
/// frame keep the order they were pushed in. Only [`EventBuffer::new`] allocates; every other
/// method is real-time safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBuffer {
    events: Vec<Event>,
    capacity: usize,
}

impl EventBuffer {
    /// An empty buffer with room for `capacity` events. Allocates.
    pub fn new(capacity: usize) -> Self {
        EventBuffer {
            events: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// The most events the buffer holds.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The number of events held.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether no event is held.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Whether the buffer is at its capacity.
    pub fn is_full(&self) -> bool {
        self.events.len() >= self.capacity
    }

    /// Adds `event` after every event at or before its frame. Returns `false`, leaving the buffer
    /// unchanged, if it is full.
    pub fn push(&mut self, event: Event) -> bool {
        if self.is_full() {
            return false;
        }
        let at = self.events.partition_point(|e| e.frame <= event.frame);
        // Within capacity, so neither call reallocates.
        if at == self.events.len() {
            self.events.push(event);
        } else {
            self.events.insert(at, event);
        }
        true
    }

    /// Removes every event.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Every event, in frame order.
    pub fn iter(&self) -> std::slice::Iter<'_, Event> {
        self.events.iter()
    }

    /// Every event, in frame order, as a slice.
    pub fn as_slice(&self) -> &[Event] {
        &self.events
    }

    /// The events whose frames fall in `frames`, in frame order.
    pub fn range(&self, frames: Range<u32>) -> &[Event] {
        let start = self.events.partition_point(|e| e.frame < frames.start);
        let end = self.events.partition_point(|e| e.frame < frames.end);
        self.events.get(start..end.max(start)).unwrap_or_default()
    }

    /// Removes every event before frame `end`, earliest first, handing each to `take`.
    pub fn take_before(&mut self, end: u32, mut take: impl FnMut(Event)) {
        let count = self.events.partition_point(|e| e.frame < end);
        self.events.drain(..count).for_each(&mut take);
    }

    /// Moves every event `frames` earlier, stopping at frame 0.
    pub fn shift_earlier(&mut self, frames: u32) {
        for event in &mut self.events {
            event.frame = event.frame.saturating_sub(frames);
        }
    }
}

impl<'a> IntoIterator for &'a EventBuffer {
    type Item = &'a Event;
    type IntoIter = std::slice::Iter<'a, Event>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), &'static str>;

    fn event(frame: u32, note: u8) -> Result<Event, &'static str> {
        Ok(Event {
            frame,
            message: Message::note_on(0, note, 100).ok_or("bad note")?,
        })
    }

    fn frames_and_notes(buffer: &EventBuffer) -> Vec<(u32, u8)> {
        buffer
            .iter()
            .map(|e| (e.frame, e.message.bytes()[1]))
            .collect()
    }

    #[test]
    fn test_push_refuses_when_full() -> TestResult {
        let mut buffer = EventBuffer::new(2);
        assert!(buffer.push(event(0, 1)?));
        assert!(buffer.push(event(1, 2)?));
        assert!(buffer.is_full());
        assert!(!buffer.push(event(0, 3)?));
        assert_eq!(frames_and_notes(&buffer), [(0, 1), (1, 2)]);
        Ok(())
    }

    #[test]
    fn test_push_keeps_frame_order_and_ties_in_push_order() -> TestResult {
        let mut buffer = EventBuffer::new(8);
        for (frame, note) in [(5, 1), (2, 2), (5, 3), (0, 4), (2, 5)] {
            assert!(buffer.push(event(frame, note)?));
        }
        assert_eq!(
            frames_and_notes(&buffer),
            [(0, 4), (2, 2), (2, 5), (5, 1), (5, 3)]
        );
        Ok(())
    }

    #[test]
    fn test_push_does_not_grow_capacity() -> TestResult {
        let mut buffer = EventBuffer::new(EVENT_CAPACITY);
        let mut accepted = 0;
        for i in 0..EVENT_CAPACITY + 10 {
            accepted += usize::from(buffer.push(event((i % 7) as u32, 1)?));
        }
        assert_eq!(accepted, EVENT_CAPACITY);
        assert_eq!(buffer.capacity(), EVENT_CAPACITY);
        Ok(())
    }

    #[test]
    fn test_range_selects_half_open_frames() -> TestResult {
        let mut buffer = EventBuffer::new(8);
        for frame in [0, 3, 3, 7, 9] {
            buffer.push(event(frame, 1)?);
        }
        let frames = |r: Range<u32>| buffer.range(r).iter().map(|e| e.frame).collect::<Vec<_>>();
        assert_eq!(frames(3..9), [3, 3, 7]);
        assert_eq!(frames(0..1), [0]);
        assert_eq!(frames(4..7), Vec::<u32>::new());
        assert_eq!(frames(10..20), Vec::<u32>::new());
        Ok(())
    }

    #[test]
    fn test_take_before_removes_only_earlier_events() -> TestResult {
        let mut buffer = EventBuffer::new(8);
        for (frame, note) in [(1, 1), (4, 2), (4, 3), (8, 4)] {
            buffer.push(event(frame, note)?);
        }
        let mut taken = Vec::new();
        buffer.take_before(5, |e| taken.push((e.frame, e.message.bytes()[1])));
        assert_eq!(taken, [(1, 1), (4, 2), (4, 3)]);
        assert_eq!(frames_and_notes(&buffer), [(8, 4)]);
        Ok(())
    }

    #[test]
    fn test_shift_earlier_saturates_at_zero() -> TestResult {
        let mut buffer = EventBuffer::new(4);
        buffer.push(event(3, 1)?);
        buffer.push(event(70, 2)?);
        buffer.shift_earlier(64);
        assert_eq!(frames_and_notes(&buffer), [(0, 1), (6, 2)]);
        buffer.clear();
        assert!(buffer.is_empty());
        Ok(())
    }
}
