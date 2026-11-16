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

    /// The sequence a recorded take of `frames` frames makes from `events`: raw MIDI 1.0 channel
    /// messages (status, then data bytes; an unused data byte is 0) at frames from the take's
    /// start, in the order they arrived (`docs/architecture/recording.md` "MIDI recording").
    ///
    /// - A note-on with velocity 0 is a note-off. A note-off with no note held is ignored.
    /// - A note-on on a channel and pitch already held ends the held note there; one that would
    ///   then be no frames long is dropped.
    /// - Notes still held at `frames` end there; events at or past `frames` start nothing.
    /// - A control change becomes a [`Control`]. Pitch bend, pressure and program change, and
    ///   anything that is not a valid channel message, are dropped.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidSequence`] only if the result breaks [`Sequence::new`]'s rules, which
    /// these rules prevent.
    pub fn from_capture(events: &[(u64, [u8; 3])], frames: u64) -> Result<Sequence, Error> {
        // Each held key's note-on frame and velocity, by channel × 128 + pitch.
        let mut held: Vec<Option<(u64, u8)>> = vec![None; 16 * 128];
        let mut notes = Vec::new();
        let mut controls = Vec::new();
        let mut order: Vec<usize> = (0..events.len()).collect();
        // Stable, so events at one frame keep their arrival order.
        order.sort_by_key(|&i| events[i].0);
        let close = |notes: &mut Vec<Note>, slot: &mut Option<(u64, u8)>, key: usize, at: u64| {
            if let Some((start, velocity)) = slot.take()
                && at > start
            {
                notes.push(Note {
                    start,
                    length: at - start,
                    channel: (key / 128) as u8,
                    pitch: (key % 128) as u8,
                    velocity,
                });
            }
        };
        for i in order {
            let (frame, [status, first, second]) = events[i];
            if !(0x80..=0xEF).contains(&status) || first > 0x7F || second > 0x7F {
                continue;
            }
            let channel = usize::from(status & 0x0F);
            let key = channel * 128 + usize::from(first);
            let at = frame.min(frames);
            match status & 0xF0 {
                0x90 if second > 0 => {
                    close(&mut notes, &mut held[key], key, at);
                    if frame < frames {
                        held[key] = Some((frame, second));
                    }
                }
                0x80 | 0x90 => close(&mut notes, &mut held[key], key, at),
                0xB0 if frame < frames => controls.push(Control {
                    frame,
                    channel: channel as u8,
                    controller: first,
                    value: second,
                }),
                _ => {}
            }
        }
        for (key, slot) in held.iter_mut().enumerate() {
            close(&mut notes, slot, key, frames);
        }
        notes.sort_by_key(|n| (n.start, n.channel, n.pitch));
        Sequence::new(frames, notes, controls)
    }

    /// This sequence with each note starting in `from..to` moved to `target(start)`, its length
    /// kept (`docs/architecture/project.md` "Editing model"). Control changes stay where they
    /// are. Then:
    ///
    /// - A note moved to or past the end is dropped; one that would run past it ends there.
    /// - Where two notes on the same channel and pitch overlap, the earlier ends where the later
    ///   starts; one left no frames long is dropped.
    pub fn quantized(&self, from: u64, to: u64, target: impl Fn(u64) -> u64) -> Sequence {
        let mut notes: Vec<Note> = self
            .notes
            .iter()
            .filter_map(|&note| {
                let start = if (from..to).contains(&note.start) {
                    target(note.start)
                } else {
                    note.start
                };
                let length = note.length.min(self.frames.checked_sub(start)?);
                (length > 0).then_some(Note {
                    start,
                    length,
                    ..note
                })
            })
            .collect();
        notes.sort_by_key(|n| (n.start, n.channel, n.pitch));
        // The index in `notes` of the latest note on each key, by channel × 128 + pitch.
        let mut latest: Vec<Option<usize>> = vec![None; 16 * 128];
        for index in 0..notes.len() {
            let note = notes[index];
            let slot = &mut latest[usize::from(note.channel) * 128 + usize::from(note.pitch)];
            if let Some(earlier) = slot.replace(index) {
                let earlier = &mut notes[earlier];
                earlier.length = earlier.length.min(note.start - earlier.start);
            }
        }
        notes.retain(|note| note.length > 0);
        let quantized = Sequence {
            frames: self.frames,
            notes,
            controls: self.controls.clone(),
        };
        debug_assert!(
            Sequence::new(
                quantized.frames,
                quantized.notes.clone(),
                quantized.controls.clone()
            )
            .is_ok()
        );
        quantized
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

    const ON: u8 = 0x90;
    const OFF: u8 = 0x80;

    #[test]
    fn test_from_capture_pairs_note_on_and_off() -> Result<(), Error> {
        let events = [
            (10, [ON, 60, 100]),
            (12, [ON | 3, 64, 90]),
            (20, [OFF, 60, 40]),
            // A note-on with velocity 0 is a note-off.
            (30, [ON | 3, 64, 0]),
        ];
        let sequence = Sequence::from_capture(&events, 100)?;
        assert_eq!(
            sequence.notes(),
            [note(10, 10, 0, 60, 100), note(12, 18, 3, 64, 90)]
        );
        assert_eq!(sequence.frames(), 100);
        Ok(())
    }

    #[test]
    fn test_from_capture_retrigger_ends_the_held_note() -> Result<(), Error> {
        let events = [(0, [ON, 60, 100]), (5, [ON, 60, 80]), (9, [OFF, 60, 0])];
        let sequence = Sequence::from_capture(&events, 100)?;
        assert_eq!(
            sequence.notes(),
            [note(0, 5, 0, 60, 100), note(5, 4, 0, 60, 80)]
        );
        Ok(())
    }

    #[test]
    fn test_from_capture_duplicate_note_on_in_one_frame_keeps_the_later() -> Result<(), Error> {
        let events = [(5, [ON, 60, 100]), (5, [ON, 60, 80]), (9, [OFF, 60, 0])];
        let sequence = Sequence::from_capture(&events, 100)?;
        assert_eq!(sequence.notes(), [note(5, 4, 0, 60, 80)]);
        Ok(())
    }

    #[test]
    fn test_from_capture_closes_held_notes_at_the_end() -> Result<(), Error> {
        let events = [(90, [ON, 60, 100]), (100, [ON, 61, 100])];
        let sequence = Sequence::from_capture(&events, 100)?;
        assert_eq!(sequence.notes(), [note(90, 10, 0, 60, 100)]);
        Ok(())
    }

    #[test]
    fn test_from_capture_ignores_unmatched_and_unsupported_events() -> Result<(), Error> {
        let events = [
            (1, [OFF, 60, 0]),
            (2, [0xE0, 0, 64]),
            (3, [0xC0, 5, 0]),
            (4, [0xD0, 50, 0]),
            (5, [0xA0, 60, 50]),
            // Not channel messages.
            (6, [0x40, 60, 50]),
            (7, [0xF8, 0, 0]),
            (8, [ON, 200, 50]),
        ];
        let sequence = Sequence::from_capture(&events, 100)?;
        assert!(sequence.notes().is_empty());
        assert!(sequence.controls().is_empty());
        Ok(())
    }

    #[test]
    fn test_from_capture_keeps_control_changes_in_order() -> Result<(), Error> {
        let events = [
            (4, [0xB1, 64, 127]),
            (4, [0xB1, 64, 0]),
            (2, [0xB0, 7, 90]),
            (100, [0xB0, 7, 10]),
        ];
        let sequence = Sequence::from_capture(&events, 100)?;
        assert_eq!(
            sequence.controls(),
            [
                control(2, 0, 7, 90),
                control(4, 1, 64, 127),
                control(4, 1, 64, 0)
            ]
        );
        Ok(())
    }

    #[test]
    fn test_from_capture_sorts_notes_for_new() -> Result<(), Error> {
        let events = [
            (0, [ON | 2, 70, 100]),
            (0, [ON, 72, 100]),
            (0, [ON, 60, 100]),
            (3, [OFF | 2, 70, 0]),
        ];
        let sequence = Sequence::from_capture(&events, 10)?;
        assert_eq!(
            sequence.notes(),
            [
                note(0, 10, 0, 60, 100),
                note(0, 10, 0, 72, 100),
                note(0, 3, 2, 70, 100)
            ]
        );
        Ok(())
    }

    fn quantize_test_sequence() -> Result<Sequence, Error> {
        Sequence::new(
            100,
            vec![
                note(3, 5, 0, 60, 100),
                note(12, 4, 0, 62, 100),
                note(27, 10, 0, 64, 100),
            ],
            vec![control(13, 0, 7, 50)],
        )
    }

    /// Snaps to the nearest multiple of 10, `strength` of the way.
    fn snap(strength: f64) -> impl Fn(u64) -> u64 {
        move |start| {
            let line = (start as f64 / 10.0).round() * 10.0;
            (start as f64 + (strength * (line - start as f64)).round()) as u64
        }
    }

    #[test]
    fn test_quantized_full_strength_moves_starts_and_keeps_lengths() -> Result<(), Error> {
        let quantized = quantize_test_sequence()?.quantized(0, 100, snap(1.0));
        assert_eq!(
            quantized.notes(),
            [
                note(0, 5, 0, 60, 100),
                note(10, 4, 0, 62, 100),
                note(30, 10, 0, 64, 100)
            ]
        );
        assert_eq!(quantized.controls(), [control(13, 0, 7, 50)]);
        assert_eq!(quantized.frames(), 100);
        Ok(())
    }

    #[test]
    fn test_quantized_strength_scales_the_move() -> Result<(), Error> {
        let sequence = quantize_test_sequence()?;
        assert_eq!(sequence.quantized(0, 100, snap(0.0)), sequence);
        let half = sequence.quantized(0, 100, snap(0.5));
        let starts: Vec<u64> = half.notes().iter().map(|n| n.start).collect();
        // 3 → 1.5 rounds to 2 (away from zero), 12 → 11, 27 → 28.5 rounds to 29.
        assert_eq!(starts, [1, 11, 29]);
        Ok(())
    }

    #[test]
    fn test_quantized_moves_only_notes_starting_in_the_window() -> Result<(), Error> {
        let quantized = quantize_test_sequence()?.quantized(4, 27, snap(1.0));
        let starts: Vec<u64> = quantized.notes().iter().map(|n| n.start).collect();
        assert_eq!(starts, [3, 10, 27]);
        Ok(())
    }

    #[test]
    fn test_quantized_resolves_overlap_on_the_same_key() -> Result<(), Error> {
        let sequence = Sequence::new(
            100,
            vec![
                note(0, 15, 0, 60, 100),
                note(17, 1, 0, 60, 90),
                note(18, 5, 0, 60, 80),
            ],
            vec![],
        )?;
        // 17 and 18 both land on 20; the first of them is dropped, and the note at 0 is cut
        // short only if it reached the later start.
        let quantized = sequence.quantized(0, 100, snap(1.0));
        assert_eq!(quantized.notes().len(), 2);
        assert_eq!(quantized.notes()[0], note(0, 15, 0, 60, 100));
        assert_eq!(quantized.notes()[1].start, 20);
        // A long note is cut where a later one moved under it starts.
        let sequence = Sequence::new(
            100,
            vec![note(0, 32, 0, 60, 100), note(33, 5, 0, 60, 90)],
            vec![],
        )?;
        let quantized = sequence.quantized(20, 100, snap(1.0));
        assert_eq!(
            quantized.notes(),
            [note(0, 30, 0, 60, 100), note(30, 5, 0, 60, 90)]
        );
        Ok(())
    }

    #[test]
    fn test_quantized_cuts_notes_at_the_end() -> Result<(), Error> {
        let sequence = Sequence::new(
            100,
            vec![note(90, 7, 0, 60, 100), note(97, 3, 0, 61, 100)],
            vec![],
        )?;
        let quantized = sequence.quantized(0, 100, |start| start + 6);
        // 90 → 96, cut to 4 frames; 97 → 103, past the end, dropped.
        assert_eq!(quantized.notes(), [note(96, 4, 0, 60, 100)]);
        Ok(())
    }

    #[test]
    fn test_new_accepts_overlap_on_other_key() {
        let notes = vec![note(0, 10, 0, 60, 100), note(5, 10, 1, 60, 100)];
        assert!(Sequence::new(100, notes, vec![]).is_ok());
    }
}
