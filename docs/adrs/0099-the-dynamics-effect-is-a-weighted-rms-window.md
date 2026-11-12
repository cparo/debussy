# ADR-0099 — The dynamics effect is a weighted-RMS window driving a tanh transfer curve

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Dynamics](../architecture/effects.md#dynamics)

## Context

Chris Paro drafted a spec for one generalised dynamics effect that could serve as a compressor,
limiter, gate or overdrive, depending on its settings. It has a preamp, a level envelope shaped by
attack and release times and exponents with a lookahead, a `tanh` scaling function, optional
asymmetric shaping and a hard clip. A review found four problems that had to be settled before
building it:

- The spec did not say what the scaling function multiplies. Taken as a gain, `tanh` rises with
  level, which expands instead of compressing.
- The peak formula `attack − lookahead` placed the window up to twice the attack time in the
  future.
- Weighting up to 5.1 s of samples with arbitrary exponents is a FIR of about 490 000 taps per
  sample at 96 kHz, and has no recursive form.
- `tanh` and clipping alias.

The graph also reads a node's latency only when the node is added (`docs/architecture/
signal-graph.md` "Processing graph"), but lookahead sets this effect's latency.

## Decision

Chris Paro answered the review questions in two rounds and approved the plan's defaults for the
smaller ones:

- **Transfer curve.** The curve is an output level, `f(x) = tanh((s·x)^c)`, and the gain is
  `f(x)/x`. With `c` = 1 it compresses towards a ceiling, with `c` > 1 it expands or gates, and
  with the instantaneous level it is a waveshaper. `c` is at least 1, so the gain at silence stays
  finite.
- **Window.** The peak is at `lookahead − attack`, so the window ends at the newest input and the
  latency is the lookahead. A side of zero length has no weight. With both sides zero, each sample
  is its own level (the waveshaper). The weight at the peak is 1. Exponents run from 0.1 to 16.
- **Cost.** The level is computed at a reduced rate: squares are summed over 0.5 ms blocks, the
  exact weights are taken at block centres, and the resulting gain is interpolated per sample.
- **Normalisation.** The weighted sum is divided by the weight sum, so that a full-scale square
  wave reads 1.
- **Oversampling.** An optional oversampling factor (1×, 2×, 4× or 8×) applies around the
  shaping and the clip. It uses an in-house linear-phase oversampler with a whole-frame delay
  (16 frames each way).
- **Channels.** Linking the channels is a toggle, on by default; linked, the channels share one
  level.
- **DC blocker.** When asymmetric shaping is on, a DC blocker at 7 Hz follows the shaping.
- **Makeup gain** comes after the curve and before the clip. There is no dry/wet mix and there are
  no presets.
- **Asymmetric shaping** shares the envelope. Only the steepness, exponent and ceiling are
  duplicated for negative samples.
- **Stopped-only parameters:** lookahead and oversampling, because they set the latency, and the
  attack and release times and exponents, chosen over recomputing the weight table on the audio
  thread or building an off-thread handoff.
- **Node replacement.** A change to any stopped-only value replaces the node, so its latency is
  read again. The new node is prepared with the new values.
- **Saved format.** The kind id `dynamics` and the parameter names in the architecture section are
  added to the saved format. Chris Paro approved this when the `product` flag was raised.

## Alternatives considered

- **Scaling factor as a direct gain** (`output = input × tanh(…)`). Rejected because the gain rises
  with level, which neither compresses nor saturates.
- **`effect_peak = attack − lookahead`, as drafted.** Rejected because at zero lookahead it needs
  up to 200 ms of latency.
- **Exact per-sample FIR, partitioned FFT convolution, or an IIR approximation of the window.** The
  per-sample FIR costs far too much. FFT convolution adds latency and complexity. An IIR
  approximation cannot represent arbitrary exponents.
- **Detecting the level from the block peak.** Left out because of patent exposure
  ([ADR-0100](0100-weighted-rms-dynamics-may-be-built-in-house-no.md)).
- **Reusing `dsp::Resampler` for oversampling.** Rejected because its delay is fractional at
  integer ratios and its streaming phase carries leftover input, but a node's latency must be whole
  frames.
- **Reporting a fixed maximum latency.** Rejected because 100 ms of output delay would apply to
  every project that used the effect.
- **Live attack and release changes,** by recomputing up to about 10 000 weights on the audio
  thread or handing them over from the interface thread. Rejected in favour of stopped-only
  parameters, which need no new mechanism.

## Consequences

- No threshold, ratio or knee is shown. A compressor is set with steepness and exponent, which is
  unfamiliar to people used to conventional compressors.
- The ceiling is not a true-peak limit. RMS detection lets transients through until the clip, and
  when oversampling is on, the down-sampling filter can ring slightly past the clip.
- The gain lags the level by up to one block (0.5 ms), on top of whatever lookahead is set.
- At the longest window, each block costs about 10 000 multiply-adds per channel, which is about
  20 million per second per channel.
- Any stopped-only parameter on any effect now replaces its node, which restarts that node from
  rest. That is the existing behaviour of `order`.
