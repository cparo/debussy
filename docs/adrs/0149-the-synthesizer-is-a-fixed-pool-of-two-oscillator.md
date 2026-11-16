# ADR-0149 — The synthesizer is a fixed pool of two-oscillator voices summed to mono

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-16
- **Issue:** #173
- **Architecture:** [Synthesizer](../architecture/instruments.md#synthesizer)

## Context

Issue #173 asks for the first instrument: polyphonic voices with band-limited oscillators, an
amplitude envelope, a resonant low-pass with its own envelope, an LFO, voice stealing, velocity and
pitch bend, with parameters automatable like an effect's. ADR-0148 fixed the DSP (MinBLEP
oscillators, exponential ADSR, the state-variable low-pass) and the patent limits. What remained
was the node: how many voices and when they are made, how notes find voices, what reaches the
output, and which parameters exist. The real-time rules forbid allocating in `process`
(`docs/architecture/engine.md` "Real-time thread").

## Decision

- A fixed pool of 16 voices is made in `prepare`. The `voices` parameter limits allocation and
  never reallocates.
- Each voice is two oscillators, a DC blocker, a state-variable low-pass, a filter envelope and an
  amplitude envelope. One LFO is shared by all voices.
- Voices are summed to mono and written to both output channels. Notes on every channel are
  played (omni).
- A note-on takes the voice already playing its note, else a free voice, else the quietest
  released voice, else the oldest; a stolen voice fades out over 3 ms first.
- The sustain pedal (CC 64) is handled; other controllers are ignored.
- The parameters and their ranges are those in the architecture section. Choices and whole-number
  parameters are stepped, so not automatable; all the rest are.

## Alternatives considered

- **Voices made on demand.** Fewer idle voices, but it allocates on the real-time thread.
- **Per-voice pan or stereo spread.** A wider sound, but a per-voice pan is a voice parameter the
  issue does not ask for, and spread by detune drift needs a fresh patent reading (ADR-0148).
- **Channel-aware (multi-timbral) voices.** A keyboard sends one channel; a single patch per node
  is the simpler rule, and several synth nodes give several timbres.
- **Ignoring the sustain pedal.** Smaller, but a keyboard player's pedal moves arrive with their notes
  (#175), and a synth that ignored them would cut sustained notes short.
- **No steal fade.** A voice cut off mid-cycle clicks. A 3 ms fade is short enough not to be
  heard as a gap and passes the steal test's click check.

## Consequences

- Sixteen voices cost memory whether or not they play, a few kilobytes each.
- Mono output means the synth's image comes only from routing: a mixer pan or a stereo effect.
- Live playing of the synth from a keyboard waits for #175; until then it plays MIDI clips, which
  can only be made outside the interface (#177).
