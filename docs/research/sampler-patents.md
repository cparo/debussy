# Patent status of the sampler instrument's playback, zones and choke groups

Research note, 2026-11-16. This is not legal advice. It records what primary sources show, so a
reviewer can check it.

## Planned design

A polyphonic sampler instrument node (issue #174) that plays the user's own WAV files, mapped to
MIDI notes. Each element as it will be built:

1. **Varispeed playback.** Each voice keeps a playhead as a fractional frame position (`f64`) and
   advances it by a constant step per output sample:
   `step = 2^((note − root + tune + bend · range) / 12) · sr_file / sr_project`. Imported files are
   already resampled to the project rate, so `sr_file / sr_project = 1`. There is no
   time-stretching and no formant preservation: pitch and duration change together.
2. **Interpolated reads.** The sample is read at the fractional playhead with 4-point (cubic)
   Hermite interpolation over the frames `⌊p⌋−1 … ⌊p⌋+2`; frames before the start or past the end
   read as zero. This is the same kernel the delay line uses (cleared in ADR-0120,
   `docs/research/delay-patents.md`). No anti-alias filtering is applied when pitching up.
3. **Key-range zones.** A zone holds a low key, a high key, a root key, a playback mode, an
   optional choke group, and a mono or stereo sample held fully in memory (capped at 60 s). A
   note-on plays the first zone whose `low ≤ note ≤ high`; a note in no zone is ignored. There is
   no velocity layering, no round-robin and no crossfading between zones.
4. **One-shot or gated playback.** A one-shot zone ignores note-off and plays to the end of the
   sample. A gated zone releases through the amplitude ADSR on note-off. Either way a voice frees
   itself when its playhead passes the end of the sample. There are no loop points.
5. **Choke groups.** Groups are numbered 1–16. A note-on in group g fades out every other sounding
   voice in group g with a linear fade over about 3 ms (the open/closed hi-hat case).
6. **Velocity to gain.** Gain is `1 − amount + amount · velocity / 127`, with `amount` a 0–1
   parameter.
7. **Voice allocation, envelope and sustain pedal.** Reused unchanged from the synth (issue #173):
   a fixed pool of 16 voices, free voice first, then the quietest releasing voice, then the oldest
   held one, with a 3 ms steal fade; an exponential-segment ADSR; CC 64 holds released notes.
   Cleared in ADR-0148 (`docs/research/synth-patents.md`); no new research.

Output is stereo; a mono zone is written to both channels.
