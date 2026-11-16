//! MIDI clip playback: [`MidiTrack`], one MIDI track's notes and control changes flattened onto
//! the timeline, played into its event source a block at a time with frame-accurate events
//! (`docs/architecture/engine.md` "MIDI clip playback", ADR-0145).
//!
//! The layer above flattens the track's clips into [`NoteSpan`]s and [`ControlPoint`]s in
//! timeline frames. [`MidiTrack::new`] sorts them and builds every index off the real-time
//! thread; [`MidiTrack::fill`] then runs once per block without allocating.

use signal_graph::{Event, EventBuffer, Message, NodeId};

/// MIDI channels.
const CHANNELS: usize = 16;
/// Note numbers on a channel.
const KEYS: usize = 128;
/// Marks a key with no note sounding in [`MidiTrack::sounding`].
const SILENT: u64 = u64::MAX;

/// One note on the timeline: held from `start` to just before `end`, in timeline frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteSpan {
    /// The frame of its note-on.
    pub start: u64,
    /// The frame of its note-off, after `start`.
    pub end: u64,
    /// Its MIDI channel, 0–15.
    pub channel: u8,
    /// Its note number, 0–127.
    pub pitch: u8,
    /// Its note-on velocity, 1–127.
    pub velocity: u8,
}

/// One control change on the timeline, at a timeline frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ControlPoint {
    /// When it is sent.
    pub frame: u64,
    /// Its MIDI channel, 0–15.
    pub channel: u8,
    /// The controller number, 0–127.
    pub controller: u8,
    /// The value, 0–127.
    pub value: u8,
}

/// One MIDI track as the engine plays it: its notes and control changes on the timeline, the
/// event source they go to, and which notes are sounding.
///
/// Each block, [`MidiTrack::fill`] sends the block's events at their frames. Whenever the block
/// does not follow on from the last one played (a start, a locate, a loop), it first sends
/// note-offs for every sounding note, then chases: each controller's last value before the
/// block, and note-ons for the notes sounding there. While not playing it sends note-offs for
/// every sounding note. A note-off the buffer refuses is sent at the next block; every refused
/// event is counted (ADR-0143).
///
/// Build and drop it off the real-time thread.
#[derive(Debug, Clone)]
pub struct MidiTrack {
    source: NodeId,
    /// Notes sorted by (start, channel, pitch); no two on one key overlap.
    notes: Vec<NoteSpan>,
    /// `max_end[i]`: the latest end of `notes[..=i]`.
    max_end: Vec<u64>,
    /// Indices into `notes`, sorted by end.
    by_end: Vec<usize>,
    /// Control changes sorted by frame, keeping their given order at one frame.
    controls: Vec<ControlPoint>,
    /// Control changes sorted by (channel, controller, frame).
    by_controller: Vec<ControlPoint>,
    /// One range of `by_controller` per controller present.
    controllers: Vec<(usize, usize)>,
    /// Each sounding note's end frame by channel and key, or [`SILENT`].
    sounding: Box<[[u64; KEYS]; CHANNELS]>,
    /// How many keys are sounding.
    active: usize,
    /// Whether a note-off at a note's end was refused, so a sounding note may have ended.
    stale: bool,
    /// The next note to start, in `notes`.
    next_on: usize,
    /// The next note to end, in `by_end`.
    next_off: usize,
    /// The next control change, in `controls`.
    next_control: usize,
    /// The timeline frame the next block should start at to follow on; `None` after a stop.
    expected: Option<u64>,
    /// Events the event buffer refused.
    overflows: u64,
    /// The part of `overflows` already reported.
    reported: u64,
}

impl MidiTrack {
    /// A track playing `notes` and `controls` into event source `source`. Drops notes with no
    /// length or an invalid channel, pitch or velocity, and control changes out of range; where
    /// two notes on one key overlap, the earlier is cut off where the later starts. Allocates.
    pub fn new(source: NodeId, notes: Vec<NoteSpan>, controls: Vec<ControlPoint>) -> MidiTrack {
        let mut notes: Vec<NoteSpan> = notes
            .into_iter()
            .filter(|n| {
                n.end > n.start
                    && n.velocity > 0
                    && Message::note_on(n.channel, n.pitch, n.velocity).is_some()
            })
            .collect();
        notes.sort_by_key(|n| (n.start, n.channel, n.pitch));
        // Cut each note off where the next on its key starts.
        let mut last: Vec<Option<usize>> = vec![None; CHANNELS * KEYS];
        for i in 0..notes.len() {
            let key = usize::from(notes[i].channel) * KEYS + usize::from(notes[i].pitch);
            if let Some(previous) = last[key]
                && notes[previous].end > notes[i].start
            {
                notes[previous].end = notes[i].start;
            }
            last[key] = Some(i);
        }
        notes.retain(|n| n.end > n.start);
        let max_end = notes
            .iter()
            .scan(0, |max, n| {
                *max = n.end.max(*max);
                Some(*max)
            })
            .collect();
        let mut by_end: Vec<usize> = (0..notes.len()).collect();
        by_end.sort_by_key(|&i| (notes[i].end, i));

        let mut controls: Vec<ControlPoint> = controls
            .into_iter()
            .filter(|c| Message::control_change(c.channel, c.controller, c.value).is_some())
            .collect();
        controls.sort_by_key(|c| c.frame);
        let mut by_controller = controls.clone();
        by_controller.sort_by_key(|c| (c.channel, c.controller, c.frame));
        let mut controllers = Vec::new();
        let mut start = 0;
        for i in 1..=by_controller.len() {
            let key = |c: &ControlPoint| (c.channel, c.controller);
            if i == by_controller.len() || key(&by_controller[i]) != key(&by_controller[start]) {
                controllers.push((start, i));
                start = i;
            }
        }

        MidiTrack {
            source,
            notes,
            max_end,
            by_end,
            controls,
            by_controller,
            controllers,
            sounding: Box::new([[SILENT; KEYS]; CHANNELS]),
            active: 0,
            stale: false,
            next_on: 0,
            next_off: 0,
            next_control: 0,
            expected: None,
            overflows: 0,
            reported: 0,
        }
    }

    /// The event source the track plays into.
    pub fn source(&self) -> NodeId {
        self.source
    }

    /// The notes it plays, sorted by (start, channel, pitch).
    pub fn notes(&self) -> &[NoteSpan] {
        &self.notes
    }

    /// The control changes it plays, sorted by frame.
    pub fn controls(&self) -> &[ControlPoint] {
        &self.controls
    }

    /// How many events the event buffer has refused since the track was made.
    pub fn overflows(&self) -> u64 {
        self.overflows
    }

    /// The refused events not yet taken by an earlier call.
    pub(crate) fn take_overflows(&mut self) -> u64 {
        let new = self.overflows - self.reported;
        self.reported = self.overflows;
        new
    }

    /// Takes over which notes `previous`, the track this one replaces, left sounding, so their
    /// note-offs are still sent. Allocation-free.
    pub(crate) fn carry_over(&mut self, previous: &MidiTrack) {
        *self.sounding = *previous.sounding;
        self.active = previous.active;
        self.stale = previous.stale;
    }

    /// Pushes this block's events into `events` (the event source's buffer, for a block of
    /// `frames` frames from timeline frame `position`), each at its frame in the block. See
    /// [`MidiTrack`]. Real-time safe.
    pub fn fill(&mut self, events: &mut EventBuffer, position: u64, frames: u32, playing: bool) {
        let jump = self.expected != Some(position);
        let mut follows = true;
        if !playing || jump {
            follows = self.flush(events);
        } else if self.stale {
            self.end_stale(events, position);
        }
        if !playing {
            self.expected = None;
            return;
        }
        if jump {
            self.seek(position);
            self.chase(events, position);
        }
        self.play(events, position, frames);
        // A note left sounding by a refused flush is flushed at the next block, which then chases
        // again.
        self.expected = follows.then(|| position.saturating_add(u64::from(frames)));
    }

    /// Pushes `message` at `frame`, counting it if refused.
    fn push(&mut self, events: &mut EventBuffer, frame: u32, message: Option<Message>) -> bool {
        let Some(message) = message else {
            return false;
        };
        let pushed = events.push(Event { frame, message });
        if !pushed {
            self.overflows += 1;
        }
        pushed
    }

    /// Sends a note-off at frame 0 for every sounding note for which `ends(end)` holds, marking
    /// it silent if sent. Returns whether every one was sent.
    fn note_offs_where(&mut self, events: &mut EventBuffer, ends: impl Fn(u64) -> bool) -> bool {
        let mut all = true;
        for channel in 0..CHANNELS {
            for key in 0..KEYS {
                if self.active == 0 {
                    return all;
                }
                let end = self.sounding[channel][key];
                if end == SILENT || !ends(end) {
                    continue;
                }
                if self.push(events, 0, Message::note_off(channel as u8, key as u8, 0)) {
                    self.sounding[channel][key] = SILENT;
                    self.active -= 1;
                } else {
                    all = false;
                }
            }
        }
        all
    }

    /// Sends a note-off at frame 0 for every sounding note. Returns whether every one was sent.
    fn flush(&mut self, events: &mut EventBuffer) -> bool {
        if self.active == 0 {
            return true;
        }
        let all = self.note_offs_where(events, |_| true);
        if all {
            self.stale = false;
        }
        all
    }

    /// Sends a note-off at frame 0 for every sounding note that ended before `position`.
    fn end_stale(&mut self, events: &mut EventBuffer, position: u64) {
        self.stale = !self.note_offs_where(events, |end| end <= position);
    }

    /// Moves the cursors to `position`.
    fn seek(&mut self, position: u64) {
        self.next_on = self.notes.partition_point(|n| n.start < position);
        self.next_off = self
            .by_end
            .partition_point(|&i| self.notes[i].end < position);
        self.next_control = self.controls.partition_point(|c| c.frame < position);
    }

    /// Sends, at frame 0, each controller's last value before `position`, then note-ons for the
    /// notes sounding at `position`.
    fn chase(&mut self, events: &mut EventBuffer, position: u64) {
        for r in 0..self.controllers.len() {
            let (start, end) = self.controllers[r];
            let before = self.by_controller[start..end].partition_point(|c| c.frame < position);
            if before > 0 {
                let c = self.by_controller[start + before - 1];
                self.push(
                    events,
                    0,
                    Message::control_change(c.channel, c.controller, c.value),
                );
            }
        }
        let first = self.max_end.partition_point(|&end| end <= position);
        for i in first..self.next_on {
            let note = self.notes[i];
            if note.end > position {
                self.note_on(events, 0, note);
            }
        }
    }

    /// Sends `note`'s note-on at `frame`, marking it sounding if sent.
    fn note_on(&mut self, events: &mut EventBuffer, frame: u32, note: NoteSpan) {
        let message = Message::note_on(note.channel, note.pitch, note.velocity);
        if self.push(events, frame, message) {
            let slot = &mut self.sounding[usize::from(note.channel)][usize::from(note.pitch)];
            if *slot == SILENT {
                self.active += 1;
            }
            *slot = note.end;
        }
    }

    /// Sends the events in `position..position + frames` at their frames: at one frame,
    /// note-offs, then control changes, then note-ons. A note-off is sent only for a note
    /// sounding with that end.
    fn play(&mut self, events: &mut EventBuffer, position: u64, frames: u32) {
        let end = position.saturating_add(u64::from(frames));
        loop {
            let off = self
                .by_end
                .get(self.next_off)
                .map(|&i| self.notes[i].end)
                .filter(|&f| f < end);
            let control = self
                .controls
                .get(self.next_control)
                .map(|c| c.frame)
                .filter(|&f| f < end);
            let on = self
                .notes
                .get(self.next_on)
                .map(|n| n.start)
                .filter(|&f| f < end);
            let next = [off, control, on].into_iter().flatten().min();
            let Some(frame) = next else {
                break;
            };
            // Within the block, so it fits.
            let at = (frame.saturating_sub(position)) as u32;
            if off == Some(frame) {
                let note = self.notes[self.by_end[self.next_off]];
                self.next_off += 1;
                let (channel, key) = (usize::from(note.channel), usize::from(note.pitch));
                if self.sounding[channel][key] == note.end {
                    if self.push(events, at, Message::note_off(note.channel, note.pitch, 0)) {
                        self.sounding[channel][key] = SILENT;
                        self.active -= 1;
                    } else {
                        self.stale = true;
                    }
                }
            } else if control == Some(frame) {
                let c = self.controls[self.next_control];
                self.next_control += 1;
                self.push(
                    events,
                    at,
                    Message::control_change(c.channel, c.controller, c.value),
                );
            } else {
                let note = self.notes[self.next_on];
                self.next_on += 1;
                self.note_on(events, at, note);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_graph::EVENT_CAPACITY;
    use signal_graph::{Config, Graph};

    fn source() -> Result<NodeId, Box<dyn std::error::Error>> {
        let (mut graph, _runner) = Graph::new(Config::new(48_000, 512))?;
        Ok(graph.add_event_source())
    }

    fn note(start: u64, end: u64, pitch: u8) -> NoteSpan {
        NoteSpan {
            start,
            end,
            channel: 0,
            pitch,
            velocity: 100,
        }
    }

    #[test]
    fn test_new_cuts_overlapping_notes_on_one_key() -> Result<(), Box<dyn std::error::Error>> {
        let track = MidiTrack::new(
            source()?,
            vec![note(10, 30, 60), note(0, 20, 60), note(5, 5, 61)],
            Vec::new(),
        );
        assert_eq!(track.notes, vec![note(0, 10, 60), note(10, 30, 60)]);
        Ok(())
    }

    #[test]
    fn test_fill_note_starting_and_ending_in_one_block() -> Result<(), Box<dyn std::error::Error>> {
        let mut track = MidiTrack::new(source()?, vec![note(3, 7, 60)], Vec::new());
        let mut events = EventBuffer::new(EVENT_CAPACITY);
        track.fill(&mut events, 0, 16, true);
        let frames: Vec<u32> = events.iter().map(|e| e.frame).collect();
        assert_eq!(frames, vec![3, 7]);
        assert_eq!(track.active, 0);
        Ok(())
    }
}
