# ADR-0150 — MIDI tracks are recorded and monitored through a live event source

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #175
- **Architecture:** [MIDI recording](../architecture/recording.md#midi-recording)

## Context

ADR-0139 scopes MIDI as performance capture first, and #175 is the step where a player arms a MIDI
track, picks an input, hears their playing through the track's instrument and gets a MIDI clip on
stop. The pieces existed below the interface: live input queues stamped with host time
(ADR-0146), event sources and clip playback (ADR-0145), live sources left out of delay
compensation (ADR-0136), and the audio recording pass with its count-in, writer thread and
placement (ADR-0078, ADR-0084). Several choices had no precedent in the audio path: what the
player hears of the track's own clips while playing in, when a note's frame is fixed, how the
captured messages become a `Sequence`, which latency places the take, and what a crash does to it.

## Decision

- **Merge, and mute on record.** An armed MIDI track is monitored by default. Its live input is
  merged with its clip playback while stopped or playing; while counting in or recording, its
  clips are muted and only live input sounds. Audio tracks keep replace-on-monitor (ADR-0021).
- **One live event source.** Live input goes into the track's existing event source, after the
  clip player's events for the block. The bridge marks that source live while the track is armed
  and monitored, so the instrument it feeds is not delayed for compensation.
- **One platform input per armed track.** The engine carries a track's input across re-arms on
  the same event source, so arming takes for a pass does not reopen the device. Notes left
  sounding by a disarm or a stop get note-offs in the next block.
- **Capture rules.** Messages are stamped with their take frame on the audio thread, queued to the
  existing `disk-recording` writer (one writer per pass) and collected in memory. On stop they
  become a `Sequence` by the rules in `docs/architecture/recording.md` "MIDI recording". Messages
  heard during the count-in are not captured.
- **Placement.** A MIDI take shifts back by the output latency, one device block (the stamping
  delay) and the plan latency: the note lands where the player heard the timeline. Input latency
  plays no part; the message's host time already marks when it was played.
- **Crash safety later.** PR 1 keeps MIDI takes out of the crash marker; a fixed-record journal and
  `recording.json` v2 follow in a second change.

## Alternatives considered

- **Replace clips while monitored, as audio does** — playing along with an existing MIDI part on
  the same track would be impossible, and the instrument would fall silent whenever the track is
  armed without input.
- **Merge while recording too** — the old clip would sound under the new take, so the player hears
  something the take does not hold; muting matches what an audio take does.
- **A second event source per MIDI track for live input** — keeps clips delay-compensated while
  armed, but needs a merge node in the graph and a per-track routing change. The cost of the
  chosen design (an armed, monitored track's clips are uncompensated) lasts only while armed.
- **Reopen the platform input at every arm change** — simpler, but a note-off arriving during the
  reopen is lost, and the record press re-arms; carrying the input over avoids both.
- **Capture the count-in's notes as notes held at frame 0** — would turn a pickup into a note at
  the take's start; dropping their note-ons is simpler and predictable.
- **Write MIDI to disk in PR 1** — a new persisted format; decided separately (journal, PR 2).

## Consequences

- While a MIDI track is armed and monitored, its clips are not delay-compensated and may sound
  early against compensated tracks by the plan latency.
- A note held across the end of the count-in is lost from the take; its note-off is ignored.
- Pitch bend, pressure and program change are heard live but not recorded, since `Sequence` has
  no slot for them.
- Until the crash journal lands (#175, PR 2), a crash during a pass loses its MIDI takes.
