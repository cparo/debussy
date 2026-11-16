//! Live MIDI input on its way to the real-time thread (`docs/architecture/engine.md` "Live MIDI
//! input", ADR-0146).
//!
//! The platform's MIDI thread pushes each [`TimedMessage`] into a [`LiveProducer`]; the audio
//! thread drains the matching [`LiveInput`] once per block with [`LiveInput::fill`], which turns
//! host time into frame offsets. The queue between them is bounded and lock-free, and neither end
//! allocates after [`channel`] has made it.

use audio_buffers::queue::{self, Consumer, Producer};

use crate::buffer::{Event, EventBuffer};
use crate::message::Message;

/// The most messages one live input holds between blocks; beyond it, input is dropped
/// (`docs/architecture/platform.md` "MIDI devices").
pub const LIVE_QUEUE_CAPACITY: usize = 1024;

/// How far ahead of the block a message may be stamped before it is treated as arriving now.
const MAX_AHEAD_NANOS: u64 = 1_000_000_000;

/// Nanoseconds in a second.
const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// A message and the host time it arrived at, in nanoseconds of the platform's host clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimedMessage {
    /// When it arrived, in host-clock nanoseconds.
    pub host_nanos: u64,
    /// The message.
    pub message: Message,
}

/// The producing end of a live input, used on the platform's MIDI thread. It never blocks or
/// allocates.
#[derive(Debug)]
pub struct LiveProducer {
    producer: Producer<TimedMessage>,
}

impl LiveProducer {
    /// Queues `message`. Returns `false`, dropping it, if the queue is full.
    pub fn push(&mut self, message: TimedMessage) -> bool {
        self.producer.try_push(message).is_ok()
    }
}

/// The consuming end of a live input, drained on the audio thread by [`LiveInput::fill`].
#[derive(Debug)]
pub struct LiveInput {
    consumer: Consumer<TimedMessage>,
    /// A message taken from the queue that falls after the last block filled.
    pending: Option<TimedMessage>,
}

/// A live input's two ends, with room for `capacity` messages. Allocates.
///
/// # Errors
///
/// [`audio_buffers::Error`] if `capacity` is 0 or too large.
pub fn channel(capacity: usize) -> Result<(LiveProducer, LiveInput), audio_buffers::Error> {
    let (producer, consumer) = queue::channel(capacity)?;
    Ok((
        LiveProducer { producer },
        LiveInput {
            consumer,
            pending: None,
        },
    ))
}

impl LiveInput {
    /// Moves the messages that arrived during the block before this one into `out`, stamped with
    /// their frame, and returns how many it moved. Real-time safe.
    ///
    /// The block covers `frames` frames at `sample_rate` and its callback runs at host time
    /// `block_host_nanos`. Messages are delayed by one block so that their spacing survives:
    /// one that arrived at host time `t` lands at frame
    /// `(t − (block_host_nanos − period)) × sample_rate / 10⁹`, where `period` is the block's
    /// length, clamped to the block. A message older than that window lands at frame 0; one at or
    /// after `block_host_nanos` waits for the next block, and so does everything queued after it;
    /// one more than a second ahead is treated as arriving now, at the block's last frame. At
    /// most `out`'s spare capacity is taken; the rest waits.
    pub fn fill(
        &mut self,
        out: &mut EventBuffer,
        block_host_nanos: u64,
        frames: u32,
        sample_rate: u32,
    ) -> usize {
        if frames == 0 || sample_rate == 0 {
            return 0;
        }
        let rate = u64::from(sample_rate);
        let period = u64::from(frames).saturating_mul(NANOS_PER_SECOND) / rate;
        let start = block_host_nanos.saturating_sub(period);
        let last = frames - 1;
        let mut moved = 0;
        while !out.is_full() {
            let Some(next) = self.pending.take().or_else(|| self.consumer.try_pop()) else {
                break;
            };
            let t = next.host_nanos;
            let far_ahead = t.saturating_sub(block_host_nanos) > MAX_AHEAD_NANOS;
            if t >= block_host_nanos && !far_ahead {
                self.pending = Some(next);
                break;
            }
            let frame = if far_ahead {
                last
            } else {
                let offset = t.saturating_sub(start).saturating_mul(rate) / NANOS_PER_SECOND;
                u32::try_from(offset).map_or(last, |f| f.min(last))
            };
            out.push(Event {
                frame,
                message: next.message,
            });
            moved += 1;
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use realtime_guard::assert_realtime;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const RATE: u32 = 48_000;
    const FRAMES: u32 = 480; // 10 ms
    const PERIOD: u64 = 10_000_000;
    const BLOCK: u64 = 5_000_000_000;

    fn timed(host_nanos: u64, note: u8) -> Result<TimedMessage, &'static str> {
        Ok(TimedMessage {
            host_nanos,
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
    fn test_fill_maps_host_time_to_frames_one_block_late() -> TestResult {
        let (mut tx, mut rx) = channel(16)?;
        // The start of the window, a quarter in, and its last nanosecond.
        assert!(tx.push(timed(BLOCK - PERIOD, 1)?));
        assert!(tx.push(timed(BLOCK - PERIOD + PERIOD / 4, 2)?));
        assert!(tx.push(timed(BLOCK - 1, 3)?));
        let mut out = EventBuffer::new(16);
        let moved = assert_realtime(|| rx.fill(&mut out, BLOCK, FRAMES, RATE));
        assert_eq!(moved, 3);
        assert_eq!(frames_and_notes(&out), [(0, 1), (120, 2), (479, 3)]);
        Ok(())
    }

    #[test]
    fn test_fill_clamps_stale_and_far_future_events() -> TestResult {
        let (mut tx, mut rx) = channel(16)?;
        tx.push(timed(0, 1)?);
        tx.push(timed(BLOCK + 2 * MAX_AHEAD_NANOS, 2)?);
        let mut out = EventBuffer::new(16);
        assert_realtime(|| rx.fill(&mut out, BLOCK, FRAMES, RATE));
        assert_eq!(frames_and_notes(&out), [(0, 1), (FRAMES - 1, 2)]);
        Ok(())
    }

    #[test]
    fn test_fill_carries_a_later_event_to_the_next_block() -> TestResult {
        let (mut tx, mut rx) = channel(16)?;
        tx.push(timed(BLOCK - PERIOD / 2, 1)?);
        tx.push(timed(BLOCK + PERIOD / 2, 2)?);
        tx.push(timed(BLOCK - PERIOD / 2, 3)?);
        let mut out = EventBuffer::new(16);
        assert_realtime(|| rx.fill(&mut out, BLOCK, FRAMES, RATE));
        assert_eq!(frames_and_notes(&out), [(240, 1)]);
        out.clear();
        assert_realtime(|| rx.fill(&mut out, BLOCK + PERIOD, FRAMES, RATE));
        // The held event keeps its place; the one queued behind it is stale by now.
        assert_eq!(frames_and_notes(&out), [(0, 3), (240, 2)]);
        Ok(())
    }

    #[test]
    fn test_fill_takes_no_more_than_spare_capacity() -> TestResult {
        let (mut tx, mut rx) = channel(16)?;
        for note in 0..5 {
            tx.push(timed(BLOCK - PERIOD, note)?);
        }
        let mut out = EventBuffer::new(3);
        out.push(Event {
            frame: 0,
            message: Message::note_off(0, 100, 0).ok_or("bad note")?,
        });
        assert_eq!(
            assert_realtime(|| rx.fill(&mut out, BLOCK, FRAMES, RATE)),
            2
        );
        out.clear();
        assert_eq!(
            assert_realtime(|| rx.fill(&mut out, BLOCK, FRAMES, RATE)),
            3
        );
        Ok(())
    }

    #[test]
    fn test_push_refuses_when_the_queue_is_full() -> TestResult {
        let (mut tx, _rx) = channel(LIVE_QUEUE_CAPACITY)?;
        let accepted = (0..LIVE_QUEUE_CAPACITY + 10)
            .map(|_| timed(1, 1).map(|m| tx.push(m)))
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(accepted.iter().filter(|&&a| a).count(), LIVE_QUEUE_CAPACITY);
        Ok(())
    }

    #[test]
    fn test_fill_ignores_an_empty_block() -> TestResult {
        let (mut tx, mut rx) = channel(4)?;
        tx.push(timed(BLOCK - 1, 1)?);
        let mut out = EventBuffer::new(4);
        assert_eq!(rx.fill(&mut out, BLOCK, 0, RATE), 0);
        assert_eq!(rx.fill(&mut out, BLOCK, FRAMES, 0), 0);
        assert_eq!(rx.fill(&mut out, BLOCK, FRAMES, RATE), 1);
        Ok(())
    }
}
