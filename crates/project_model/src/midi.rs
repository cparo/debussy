//! MIDI note sequences: what a MIDI clip plays (`docs/architecture/project.md` "Editing model",
//! ADR-0144).
//!
//! A [`Sequence`] is valid by construction: [`Sequence::new`] refuses notes and control changes
//! out of MIDI 1.0 range, out of order, past the sequence's end, or overlapping on the same channel
//! and pitch.

use crate::error::Error;

/// One note: a key held from `start` for `length` frames. Times are frames from the start of the
/// sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note {
    /// When the note starts.
    pub start: u64,
    /// How long it is held, at least one frame.
    pub length: u64,
    /// Its MIDI channel, 0–15.
    pub channel: u8,
    /// Its MIDI note number, 0–127.
    pub pitch: u8,
    /// Its note-on velocity, 1–127.
    pub velocity: u8,
}

impl Note {
    /// The frame just after the note; `None` if it overflows.
    pub fn end(&self) -> Option<u64> {
        self.start.checked_add(self.length)
    }
}

/// One control change at a frame from the start of the sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Control {
    /// When it is sent.
    pub frame: u64,
    /// Its MIDI channel, 0–15.
    pub channel: u8,
    /// The controller number, 0–127.
    pub controller: u8,
    /// The value, 0–127.
    pub value: u8,
}

/// A run of notes and control changes, `frames` long. Every `Sequence` is valid; see
/// [`Sequence::new`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sequence {
    frames: u64,
    notes: Vec<Note>,
    controls: Vec<Control>,
}

impl Sequence {
    /// A sequence `frames` long holding `notes` and `controls`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidSequence`] if a value is out of MIDI 1.0 range, a velocity is 0, a note is
    /// empty or ends past `frames`, a control is at or past `frames`, notes are not sorted by
    /// (start, channel, pitch) or controls by frame, or two notes on the same channel and pitch
    /// overlap.
    pub fn new(frames: u64, notes: Vec<Note>, controls: Vec<Control>) -> Result<Sequence, Error> {
        let invalid = |what: &str| Err(Error::InvalidSequence(what.to_owned()));
        for note in &notes {
            if midi::Message::note_on(note.channel, note.pitch, note.velocity).is_none() {
                return invalid("a note's channel, pitch or velocity is out of range");
            }
            if note.velocity == 0 {
                return invalid("a note has velocity 0");
            }
            if note.length == 0 {
                return invalid("a note is no frames long");
            }
            if note.end().is_none_or(|end| end > frames) {
                return invalid("a note ends past the end of the sequence");
            }
        }
        let key = |n: &Note| (n.start, n.channel, n.pitch);
        if notes.windows(2).any(|w| key(&w[0]) > key(&w[1])) {
            return invalid("notes are not sorted by start, channel and pitch");
        }
        for control in &controls {
            if midi::Message::control_change(control.channel, control.controller, control.value)
                .is_none()
            {
                return invalid("a control change's channel, controller or value is out of range");
            }
            if control.frame >= frames {
                return invalid("a control change is past the end of the sequence");
            }
        }
        if controls.windows(2).any(|w| w[0].frame > w[1].frame) {
            return invalid("control changes are not sorted by frame");
        }
        // Notes are sorted by start, so a note overlaps an earlier one on its key exactly when it
        // starts before the latest end seen on that key.
        let mut ends = vec![0u64; 16 * 128];
        for note in &notes {
            let slot = &mut ends[usize::from(note.channel) * 128 + usize::from(note.pitch)];
            if note.start < *slot {
                return invalid("two notes on the same channel and pitch overlap");
            }
            *slot = note.start + note.length;
        }
        Ok(Sequence {
            frames,
            notes,
            controls,
        })
    }

    /// Its length, in frames.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Its notes, sorted by (start, channel, pitch).
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Its control changes, sorted by frame.
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: u64, length: u64, channel: u8, pitch: u8, velocity: u8) -> Note {
        Note {
            start,
            length,
            channel,
            pitch,
            velocity,
        }
    }

    fn control(frame: u64, channel: u8, controller: u8, value: u8) -> Control {
        Control {
            frame,
            channel,
            controller,
            value,
        }
    }

    fn refused(frames: u64, notes: Vec<Note>, controls: Vec<Control>) -> bool {
        matches!(
            Sequence::new(frames, notes, controls),
            Err(Error::InvalidSequence(_))
        )
    }

    #[test]
    fn test_new_accepts_valid_sequence() -> Result<(), Error> {
        let notes = vec![
            note(0, 10, 0, 60, 100),
            note(0, 10, 0, 64, 1),
            note(5, 5, 15, 127, 127),
            note(10, 90, 0, 60, 80),
        ];
        let controls = vec![control(0, 0, 7, 0), control(99, 15, 127, 127)];
        let sequence = Sequence::new(100, notes.clone(), controls.clone())?;
        assert_eq!(sequence.frames(), 100);
        assert_eq!(sequence.notes(), notes.as_slice());
        assert_eq!(sequence.controls(), controls.as_slice());
        Ok(())
    }

    #[test]
    fn test_new_accepts_empty_sequence() {
        assert!(Sequence::new(0, Vec::new(), Vec::new()).is_ok());
    }

    #[test]
    fn test_new_refuses_out_of_range_values() {
        assert!(refused(100, vec![note(0, 1, 16, 60, 100)], vec![]));
        assert!(refused(100, vec![note(0, 1, 0, 128, 100)], vec![]));
        assert!(refused(100, vec![note(0, 1, 0, 60, 128)], vec![]));
        assert!(refused(100, vec![], vec![control(0, 16, 7, 0)]));
        assert!(refused(100, vec![], vec![control(0, 0, 128, 0)]));
        assert!(refused(100, vec![], vec![control(0, 0, 7, 128)]));
    }

    #[test]
    fn test_new_refuses_velocity_zero() {
        assert!(refused(100, vec![note(0, 1, 0, 60, 0)], vec![]));
    }

    #[test]
    fn test_new_refuses_empty_note() {
        assert!(refused(100, vec![note(0, 0, 0, 60, 100)], vec![]));
    }

    #[test]
    fn test_new_refuses_note_past_end() {
        assert!(refused(100, vec![note(95, 6, 0, 60, 100)], vec![]));
        assert!(refused(100, vec![note(u64::MAX, 2, 0, 60, 100)], vec![]));
        assert!(Sequence::new(100, vec![note(95, 5, 0, 60, 100)], vec![]).is_ok());
    }

    #[test]
    fn test_new_refuses_control_past_end() {
        assert!(refused(100, vec![], vec![control(100, 0, 7, 0)]));
    }

    #[test]
    fn test_new_refuses_unsorted() {
        assert!(refused(
            100,
            vec![note(5, 1, 0, 60, 100), note(4, 1, 0, 61, 100)],
            vec![]
        ));
        assert!(refused(
            100,
            vec![note(5, 1, 1, 60, 100), note(5, 1, 0, 61, 100)],
            vec![]
        ));
        assert!(refused(
            100,
            vec![note(5, 1, 0, 61, 100), note(5, 1, 0, 60, 100)],
            vec![]
        ));
        assert!(refused(
            100,
            vec![],
            vec![control(5, 0, 7, 0), control(4, 0, 7, 0)]
        ));
    }

    #[test]
    fn test_new_refuses_overlapping_same_key() {
        assert!(refused(
            100,
            vec![note(0, 10, 0, 60, 100), note(9, 10, 0, 60, 100)],
            vec![]
        ));
        assert!(refused(
            100,
            vec![note(0, 10, 0, 60, 100), note(0, 10, 0, 60, 100)],
            vec![]
        ));
        // A long note still overlaps one that starts after a short note in between.
        assert!(refused(
            100,
            vec![
                note(0, 50, 0, 60, 100),
                note(10, 1, 0, 61, 100),
                note(20, 1, 0, 60, 100)
            ],
            vec![]
        ));
    }

    #[test]
    fn test_new_accepts_overlap_on_other_key() {
        let notes = vec![note(0, 10, 0, 60, 100), note(5, 10, 1, 60, 100)];
        assert!(Sequence::new(100, notes, vec![]).is_ok());
    }
}
