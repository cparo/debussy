# Delay and Reverb

## Delay

`delay` writes each channel's input, plus its feedback, into a circular buffer and reads it back
at a whole-frame tap; there is no fractional read. Each buffer holds 4 s plus the 50 ms offset
range at the graph's rate, allocated when the node is prepared. Per frame and channel:

1. **Read.** `y` from the tap. While the tap changes, `y` is a linear crossfade from the old tap
   to the new over the parameter ramp time, with no pitch glide; a change during a crossfade
   waits for it to end.
2. **Feedback path.** `y` through a 2nd-order TPT state-variable low-pass at `low-pass` Hz,
   Q 0.707 (see `docs/architecture/effects.md` "Filters"), weighted by its share; then `tanh`
   (`audio_effects::Shape::Crunched`), weighted by its share; then × `feedback`.
3. **Write.** In stereo mode, each channel writes its input plus its own feedback. In ping-pong,
   the left line writes the input folded to mono, `(L + R) × 0.7071`, plus the right line's
   feedback, and the right line writes the left line's feedback.
4. **Output.** `x·(1 − mix) + y·mix`.

The left tap is the time in frames, rounded, clamped to 1 frame and 4 s; the right tap adds
`offset`, clamped to the buffer. A free time is `time` in milliseconds. A synced time is the note
value × feel (straight ×1, dotted ×1.5, triplet ×2/3) of a whole note, given by the graph (see
`docs/architecture/signal-graph.md` "Effect nodes"). `feedback`, `mix` and the low-pass cutoff
(in its logarithm) ramp; the mode, `low-pass enabled` and `saturation` crossfade their shares
linearly over the ramp time. The low-pass and `tanh` run whether or not they are heard. Latency
is 0, and there is no curve.

| Name | Group | Range | Default |
|---|---|---|---|
| `sync` | time | toggle | off |
| `time` | time | 1 to 4000 ms, logarithmic | 375 |
| `note` | time | 1/32 (0), 1/16, 1/8, 1/4, 1/2, 1/1 (5) | 1/8 |
| `feel` | time | straight (0), dotted (1), triplet (2) | straight |
| `mode` | stereo | stereo (0), ping-pong (1) | stereo |
| `offset` | stereo | −50 to +50 ms, added to the right channel | 0 |
| `feedback` | feedback | 0 to 100 % | 35 |
| `low-pass enabled` | feedback | toggle | off |
| `low-pass` | feedback | 200 Hz to 20 kHz, logarithmic | 6 kHz |
| `saturation` | feedback | toggle | off |
| `mix` | output | 0 to 100 % | 30 |

Presets: Slapback (110 ms, 10 % feedback, 35 % mix), Ping-pong (synced dotted 1/8, ping-pong,
45 % feedback), Dub echo (synced 1/4, 70 % feedback, low-pass at 2 kHz, saturation, 40 % mix)
([ADR-0112](../adrs/0112-the-delay-reads-whole-frame-taps-from-a-4-second.md)).
