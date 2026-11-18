//! MIDI on its way from the real-time thread to an output port (`docs/architecture/engine.md`
//! "MIDI output", ADR-0162).
//!
//! The audio thread pushes each message leaving an event sink, stamped with the host time it is
//! due, into an [`OutProducer`]; a sender thread drains the matching [`OutConsumer`] and hands
//! the messages to the platform. [`Outgoing::Silence`] asks the sender to silence the port. The
//! queue between them is bounded and lock-free, and neither end allocates after [`channel`] has
//! made it.

use audio_buffers::queue::{self, Consumer, Producer};

use crate::message::Message;

/// The most items one MIDI output holds between the audio thread and its sender; beyond it,
/// messages are dropped and counted.
pub const OUT_QUEUE_CAPACITY: usize = 4096;

/// One item on its way to an output port.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outgoing {
    /// A message to send.
    Message {
        /// When it is due to leave, before the sender's offsets, in host-clock nanoseconds.
        host_nanos: u64,
        /// The message.
        message: Message,
    },
    /// The transport stopped or paused: release every note and pedal on the port.
    Silence,
}

/// The producing end of an output queue, used on the real-time thread. It never blocks or
/// allocates.
#[derive(Debug)]
pub struct OutProducer {
    producer: Producer<Outgoing>,
    /// Messages refused because the queue was full, since [`OutProducer::take_dropped`].
    dropped: u64,
    /// Whether a [`Outgoing::Silence`] found the queue full and is still to be queued.
    silence_owed: bool,
}

impl OutProducer {
    /// Queues `message`, due at host time `host_nanos`. Returns `false`, dropping and counting
    /// it, if the queue is full. An owed silence is queued first.
    pub fn push(&mut self, host_nanos: u64, message: Message) -> bool {
        if !self.flush_silence() {
            self.dropped += 1;
            return false;
        }
        let queued = self
            .producer
            .try_push(Outgoing::Message {
                host_nanos,
                message,
            })
            .is_ok();
        if !queued {
            self.dropped += 1;
        }
        queued
    }

    /// Queues [`Outgoing::Silence`]. It is never lost: if the queue is full it is owed and
    /// queued before the next message.
    pub fn silence(&mut self) {
        self.silence_owed = true;
        self.flush_silence();
    }

    /// Queues an owed silence if there is one. Returns `false` if one is still owed.
    fn flush_silence(&mut self) -> bool {
        if self.silence_owed && self.producer.try_push(Outgoing::Silence).is_ok() {
            self.silence_owed = false;
        }
        !self.silence_owed
    }

    /// The messages dropped since the last call.
    pub fn take_dropped(&mut self) -> u64 {
        std::mem::take(&mut self.dropped)
    }
}

/// The consuming end of an output queue, drained by the sender thread.
#[derive(Debug)]
pub struct OutConsumer {
    consumer: Consumer<Outgoing>,
}

impl OutConsumer {
    /// The next item, oldest first, if any.
    pub fn pop(&mut self) -> Option<Outgoing> {
        self.consumer.try_pop()
    }
}

/// An output queue's two ends, with room for `capacity` items. Allocates.
///
/// # Errors
///
/// [`audio_buffers::Error`] if `capacity` is 0 or too large.
pub fn out_channel(capacity: usize) -> Result<(OutProducer, OutConsumer), audio_buffers::Error> {
    let (producer, consumer) = queue::channel(capacity)?;
    Ok((
        OutProducer {
            producer,
            dropped: 0,
            silence_owed: false,
        },
        OutConsumer { consumer },
    ))
}

impl Message {
    /// Control change 123, All Notes Off, on `channel` (0–15), or `None` above 15.
    pub fn all_notes_off(channel: u8) -> Option<Message> {
        Message::control_change(channel, 123, 0)
    }

    /// Control change 64 at 0, sustain pedal up, on `channel` (0–15), or `None` above 15.
    pub fn sustain_off(channel: u8) -> Option<Message> {
        Message::control_change(channel, 64, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Kind;
    use realtime_guard::assert_realtime;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn note(n: u8) -> Result<Message, &'static str> {
        Message::note_on(0, n, 100).ok_or("bad note")
    }

    fn drain(rx: &mut OutConsumer) -> Vec<Outgoing> {
        std::iter::from_fn(|| rx.pop()).collect()
    }

    #[test]
    fn test_push_keeps_order_without_allocating() -> TestResult {
        let (mut tx, mut rx) = out_channel(8)?;
        let (a, b) = (note(1)?, note(2)?);
        assert_realtime(|| {
            assert!(tx.push(10, a));
            tx.silence();
            assert!(tx.push(20, b));
        });
        assert_eq!(
            drain(&mut rx),
            [
                Outgoing::Message {
                    host_nanos: 10,
                    message: a
                },
                Outgoing::Silence,
                Outgoing::Message {
                    host_nanos: 20,
                    message: b
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn test_push_counts_overflow() -> TestResult {
        let (mut tx, mut rx) = out_channel(2)?;
        let n = note(1)?;
        let accepted = assert_realtime(|| (0..5).filter(|&t| tx.push(t, n)).count());
        assert_eq!(accepted, 2);
        assert_eq!(tx.take_dropped(), 3);
        assert_eq!(tx.take_dropped(), 0);
        assert_eq!(drain(&mut rx).len(), 2);
        Ok(())
    }

    #[test]
    fn test_silence_on_a_full_queue_is_owed_not_lost() -> TestResult {
        let (mut tx, mut rx) = out_channel(1)?;
        let n = note(1)?;
        assert!(tx.push(1, n));
        tx.silence();
        // Still owed: the next message is refused until the silence is queued.
        assert!(!tx.push(2, n));
        assert_eq!(
            rx.pop(),
            Some(Outgoing::Message {
                host_nanos: 1,
                message: n
            })
        );
        // The silence takes the freed slot; the message behind it finds the queue full.
        assert!(!tx.push(3, n));
        assert_eq!(rx.pop(), Some(Outgoing::Silence));
        assert!(rx.pop().is_none());
        assert!(tx.push(4, n));
        assert_eq!(
            rx.pop(),
            Some(Outgoing::Message {
                host_nanos: 4,
                message: n
            })
        );
        assert_eq!(tx.take_dropped(), 2);
        Ok(())
    }

    #[test]
    fn test_all_notes_off_and_sustain_off() {
        assert_eq!(
            Message::all_notes_off(15).map(|m| m.kind()),
            Some(Kind::ControlChange {
                controller: 123,
                value: 0
            })
        );
        assert_eq!(
            Message::sustain_off(0).map(|m| m.kind()),
            Some(Kind::ControlChange {
                controller: 64,
                value: 0
            })
        );
        assert!(Message::all_notes_off(16).is_none());
        assert!(Message::sustain_off(16).is_none());
    }
}
