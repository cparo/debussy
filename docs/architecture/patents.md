# Patents

## Patent policy

A codec or algorithm may be implemented in-house, or used through a crate, only when every patent
covering it has expired or its owners have declared it royalty-free, with that evidence recorded
in an ADR. Otherwise it must be used through a platform SDK that licenses it, or approved by a
human through the `flag-risk` skill (tag `patent`) ([ADR-0023](../adrs/0023-expired-or-royalty-free-patents-may-be-implemented.md)).

The algorithms implemented in-house under this policy, each with its evidence: TPDF dither
([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)), the xorshift64*
noise source seeded by SplitMix64 ([ADR-0035](../adrs/0035-the-dither-noise-source-is-xorshift64-seeded-by.md)),
exact rational polyphase resampling ([ADR-0036](../adrs/0036-sample-rate-conversion-is-exact-rational-polyphase.md)), the radix-2 FFT with
real-input packing and the Hann-windowed STFT ([ADR-0039](../adrs/0039-the-fft-is-radix-2-cooley-tukey-with-real-input.md)), the
intensity-graded waveform trace ([ADR-0064](../adrs/0064-the-intensity-graded-trace-may-be-built-in-house.md)), and the
trapezoidal state-variable and one-pole filters with their bell, shelf, notch and Butterworth
designers, prewarped bilinear transform and log-frequency smoothing
([ADR-0088](../adrs/0088-equalisers-are-built-on-a-trapezoidal-state.md)), and the feed-forward
dynamics processor: peak or RMS detection linked by the louder channel, attack, release and hold
on the level, the two-sided quadratic-knee curve with range and soft ceiling, and the lookahead
delay, without a windowed-peak limiter path
([ADR-0099](../adrs/0099-dynamics-is-one-feed-forward-effect-with-level.md)).
