# ADR-0112 — The delay reads whole-frame taps from a 4-second line, free or tempo-synced, with a crossfade on time change and an optional low-pass and saturation in its feedback

- **Status:** Superseded by ADR-0114
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-13
- **Issue:** #61
- **Architecture:** [Delay](../architecture/delay-and-reverb.md#delay)

## Context

Issue #61 asks for a delay effect node in `audio_effects`, distinct from the routing feedback
delay (ADR-0016). Chris Paro's design: tempo sync, stereo and ping-pong modes with a left/right
offset, saturation in the feedback path, and an optional low-pass in the feedback path.

Patent evidence is in [`docs/research/delay-patents.md`](../research/delay-patents.md). One
candidate's printed term runs on: US9711119B2 (Casio, to 2036-12-13). Its status was checked by
Claude from Google Patents on 2026-11-13, under a rule delegated by Chris Paro in place of a human
check: a page with no expired or revoked note is In force. The page shows "Active". Its
independent claims 1, 9, 10 and 12 do not read on the design: claims 1, 9 and 10 need a second
processor counting a sequence clock, adjusted from a detected time difference against the
sampling clock, and claim 12 needs a controller that changes music playback timing and speed to
follow the effect. Here the effect follows the tempo on the one sample clock. Crossfaded taps
(US4644400A, 1984, and later) and a feedback loop with distortion and equalisation (US6351540B1,
1998) are expired. The search used web search and Google Patents pages, not patent-database
queries, and the claim quotes come from Google's text.

## Decision

- One `dsp::TapDelay` per channel: a ring of 4 s plus the 50 ms offset range at the graph's rate,
  allocated in `prepare`, read at whole-frame taps. No fractional read.
- The time is free (1 to 4000 ms) or a note value (1/32 to 1/1) × feel (straight, dotted,
  triplet) of the whole note the graph gives (ADR-0111), clamped to 1 frame and 4 s; the right
  channel adds −50 to +50 ms.
- A tap change crossfades linearly from old to new over the parameter ramp time, and a further
  change waits for the fade to end. There is no pitch glide.
- Feedback: read, low-pass (the ADR-0088 SVF, Q 0.707), `tanh` (ADR-0102's crunched shape),
  × feedback, added to the input. Each option, and the stereo/ping-pong routing, crossfades its
  share over the ramp time. Ping-pong folds the input to mono with the ADR-0030 gain into the
  left line and cross-feeds the lines.
- The kind id is `delay`, with three presets; its parameter names are new saved names under
  `SavedNodeKind::Effect`, and `format_version` is unchanged.

## Alternatives considered

- **Fractional (interpolated) reads with a glide on time change.** It gives the tape-style pitch
  bend some delays have, but needs an interpolator and a slew design; a crossfade is
  click-free at the cost of the bend, and the reverb's modulated allpasses (#61's second part)
  bring a fractional read anyway.
- **Restarting a crossfade mid-fade from the current mix.** Three taps would be heard at once;
  waiting is simpler, and a dragged time steps through fades 10 ms apart.
- **Switching the options and mode instantly.** A jump in the feedback path clicks on a loud
  repeat.
- **Feeding the input to both lines in ping-pong.** The repeats would not alternate sides.

## Consequences

- Memory is two lines of about 4.05 s each: about 6.2 MB at 192 kHz, 1.6 MB at 48 kHz.
- The low-pass and `tanh` always run, so a toggle never starts from a cold filter state.
- The algorithm is listed in `docs/architecture/patents.md` "Patent policy". The reverb is the
  second part of #61.
