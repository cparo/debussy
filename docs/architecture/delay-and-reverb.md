# Delay and Reverb

## Delay

`delay` writes each channel's input, plus its feedback, into a circular buffer and reads it back
at a whole-frame tap; there is no fractional read. Each buffer holds 4 s plus the 50 ms offset
range at the graph's rate, allocated when the node is prepared. Per frame and channel:

1. **Read.** `y` from the tap. While the tap changes, `y` is a linear crossfade from the old tap
   to the new over the parameter ramp time, with no pitch glide; a change during a crossfade
   waits for it to end.
2. **Feedback path.** `y` through a first-order TPT one-pole low-pass at `low-pass` Hz (see
   `docs/architecture/effects.md` "Filters"), weighted by its share; then `tanh`
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
([ADR-0112](../adrs/0112-the-delay-reads-whole-frame-taps-from-a-4-second.md),
[ADR-0114](../adrs/0114-the-delay-s-feedback-low-pass-is-a-first-order-one.md)).

## Reverb

`reverb` delays its input by `pre-delay`, then runs one of two engines, chosen by `algorithm`;
the output is `x·(1 − mix) + wet·mix`. The pre-delay is a whole-frame tap of at least 1 frame
into a buffer of 200 ms per channel, crossfading on a change as the delay's tap does (see
"Delay"). Every delay length is fixed for the sample rate; there is no size control. All buffers
are allocated when the node is prepared, under 4 MB at 192 kHz.

- **Room**, a feedback delay network. Each channel passes four Schroeder allpasses in series
  (113, 163, 241 and 337 frames at 48 kHz, gain 0.6). Eight delay lines, of 1031, 1327, 1523,
  1801, 2063, 2357, 2621 and 2903 frames at 48 kHz (scaled to the rate and raised to distinct
  primes), are read at their ends. Each line's end passes a one-pole low-pass at `damping` and a
  gain `10^(−3·mᵢ / (decay·rate))` for its length `mᵢ`; the eight are mixed by the 8×8 Hadamard
  matrix (a fast Walsh–Hadamard transform scaled by 1/√8) and written back, the left input added
  to the even lines and the right to the odd. The outputs are the lines' ends, before the matrix,
  summed with the sign patterns `+ + − − + + − −` (left) and `+ − + − − + − +` (right), × 1/√8.
- **Plate**, Dattorro's plate (JAES 45(9), 1997), with its lengths given at 29 761 Hz and scaled
  to the rate. The input, folded to mono, passes a one-pole band-limit (`y = 0.9995·x +
  0.0005·y`) and four allpasses (142 and 107 frames at gain 0.75, 379 and 277 at 0.625), then
  enters both halves of a figure-eight tank. Each half is an allpass of gain −0.7 (672 or 908
  frames) whose delay a 1 Hz sine moves by ±16 frames (cosine on the left, sine on the right),
  read by linear interpolation; a delay (4453 or 4217); the damping low-pass and × the tank gain;
  an allpass (1800 or 2656) of gain `clamp(gain + 0.15, 0.25, 0.5)`; and a delay (3720 or 3163),
  whose end × the tank gain feeds the other half. The tank gain is `10^(−3·L / (4·decay))`, `L`
  the tank's eight lengths in seconds. The outputs are Dattorro's seven fixed taps per side, each
  × 0.6.

`decay` and `damping` ramp in their logarithms, recomputing gains and filter coefficients each
frame while they move. `algorithm` is stopped-only: a change switches engines at once, clearing
both, and the two never run together. Latency is 0, and there is no curve.

| Name | Group | Range | Default |
|---|---|---|---|
| `algorithm` | type | room (0), plate (1); stopped-only | room |
| `pre-delay` | time | 0 to 200 ms | 10 |
| `decay` | time | 0.2 to 20 s, logarithmic, time to fall 60 dB | 2 |
| `damping` | tone | 1 to 20 kHz, logarithmic | 8 kHz |
| `mix` | output | 0 to 100 % | 25 |

There are no presets ([ADR-0113](../adrs/0113-the-reverb-is-a-fixed-length-feedback-delay.md)).
