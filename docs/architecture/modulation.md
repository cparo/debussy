# Modulation

## LFO

`modulation::Lfo` is a low-frequency oscillator for the real-time core. It allocates nothing,
takes no locks and does no I/O after it is made (see `docs/architecture/engine.md`
"Real-time thread").

- **Clock.** A phase in cycles, `0 ≤ phase < 1`, held in `f64`, advanced by
  `frequency / sample rate` each frame and wrapped by subtracting its whole part. A frequency that
  is negative or NaN is 0; one above half the sample rate is half the sample rate. Each wrap
  counts one cycle.
- **Shapes** (`modulation::Shape`), each a value from −1 to 1 at phase `p`:

  | Shape | Value |
  |---|---|
  | `Sine` | `sin(2πp)` |
  | `Triangle` | 0 at `p = 0`, rising to 1 at ¼, −1 at ¾, back to 0 |
  | `SawUp` | `2p − 1` |
  | `SawDown` | `1 − 2p` |
  | `Square` | 1 below `p = ½`, −1 from it |
  | `SampleAndHold` | one uniform random value from −1 to 1 per cycle |

  No shape is band-limited.
- **Offset reads.** A reader asks for the value at a phase offset (in cycles) from the clock and
  a stream number. Periodic shapes read the clock's phase plus the offset; `SampleAndHold`
  reads the random value of the offset phase's cycle in its stream. A random value is a pure
  function of the LFO's seed, the cycle and the stream, drawn from `dsp::Noise`, so the same seed
  gives the same values, and different streams are uncorrelated.
- **Tempo sync.** `modulation::sync` holds the note arithmetic that tempo-synced parameters share:
  the whole-note length a node runs at (the graph's, or two seconds when none is given or it is
  not positive), the delay's note values (`synced_frames`), and an LFO period (`period_frames`)
  of note value × feel (see `docs/architecture/delay-and-reverb.md` "Delay"). LFO note values
  are, by index, 1/16 (0), 1/8, 1/4, 1/2, 1/1, 2/1 and 4/1 (6) of a whole note. A synced LFO's period follows
  the tempo; its phase runs free and is not aligned to bars.

No smooth-random shape (random points joined by interpolation) is built
([ADR-0118](../adrs/0118-modulation-sources-live-in-a-new-real-time-core.md),
[ADR-0119](../adrs/0119-lfo-shapes-are-sine-triangle-saws-square-and.md)).

## Modulated delay

The `chorus` and `flanger` share one modulated-delay core in `audio_effects`. Each channel writes
its input into a circular buffer (`dsp::TapDelay`) and reads it at a fractional delay by
`TapDelay::read_cubic`: 4-point, 3rd-order Hermite (Catmull-Rom) interpolation across the taps
`⌊d⌋ − 1` to `⌊d⌋ + 2`, with `d` clamped to `2 ..= max tap − 1` and NaN read as 2. One LFO drives
both channels; the right channel reads it at `spread / 360` cycles from the left. The LFO's
frequency is `rate` in hertz, shown while `sync` is off, or the reciprocal of a synced LFO period
(see "LFO"), with `note` and `feel` shown while it is on. The base delay, depth, spread, feedback
and `mix` ramp; the LFO's frequency and shape change at once, its phase continuing. Buffers are
allocated when the node is prepared. Latency is 0. Both kinds' curve is a `Spectrogram`
(`docs/architecture/effect-displays.md` "Modulation display").

## Chorus

`chorus` reads 1 to 4 voices per channel and outputs `x·(1 − mix) + wet·mix`, with no feedback.
Voice `i` of `n` reads the LFO at offset `i / n` (plus the spread on the right) and delays by
`delay + depth·(1 + lfo) / 2` milliseconds, so it sweeps from `delay` up to `delay + depth`. The
wet signal is the voices' sum × 1/√n. All four voices always run; on a change of `voices`, each
voice's gain and LFO offset ramp to their new values over the ramp time, so a voice whose offset
moves glides briefly in pitch.

| Name | Group | Range | Default |
|---|---|---|---|
| `sync` | lfo | toggle | off |
| `rate` | lfo | 0.05 to 5 Hz, logarithmic | 0.8 |
| `note` | lfo | 1/16 (0) to 4/1 (6) | 1/1 |
| `feel` | lfo | straight (0), dotted (1), triplet (2) | straight |
| `shape` | lfo | sine (0), triangle (1) | sine |
| `delay` | delay | 5 to 30 ms | 15 |
| `depth` | delay | 0 to 10 ms | 3 |
| `voices` | delay | 1 to 4, whole numbers | 2 |
| `spread` | stereo | 0 to 180° | 90 |
| `mix` | output | 0 to 100 % | 50 |

Presets: Subtle (2 voices, 2 ms depth, 35 % mix), Wide (3 voices, 4 ms depth, 180° spread)
([ADR-0120](../adrs/0120-chorus-and-flanger-are-lfo-modulated-delays-read.md)).

## Flanger

`flanger` reads one tap per channel at `delay·(1 + depth·lfo)` milliseconds, so at 100 % depth it
sweeps from 0 (held at 2 frames) to twice `delay`. It writes `x + feedback·y` into the line and
outputs `x·(1 − mix) + y·mix`. The feedback stays inside the node, below 1 in magnitude; a value
that is not finite writes the input alone. Through-zero flanging is not built.

| Name | Group | Range | Default |
|---|---|---|---|
| `sync` | lfo | toggle | off |
| `rate` | lfo | 0.05 to 5 Hz, logarithmic | 0.25 |
| `note` | lfo | 1/16 (0) to 4/1 (6) | 2/1 |
| `feel` | lfo | straight (0), dotted (1), triplet (2) | straight |
| `shape` | lfo | sine (0), triangle (1) | triangle |
| `delay` | delay | 0.1 to 10 ms, logarithmic | 1 |
| `depth` | delay | 0 to 100 % of `delay` | 70 |
| `feedback` | delay | −95 to +95 % | 50 |
| `spread` | stereo | 0 to 180° | 90 |
| `mix` | output | 0 to 100 % | 50 |

Presets: Jet (0.1 Hz, 2 ms, 90 % depth, 70 % feedback), Metallic (0.5 Hz, 0.5 ms, 50 % depth,
−85 % feedback)
([ADR-0120](../adrs/0120-chorus-and-flanger-are-lfo-modulated-delays-read.md)).
