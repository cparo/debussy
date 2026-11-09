# ADR-0071 — The audio colour is orange on dark chrome and blue on light

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

One orange (`#FF8000`) coloured audio in both tones: waveforms, traces, spectrograms, patch-board
wires and ports, the selected toolbar button and the logo. It suits dark chrome, but on light
chrome it reads poorly, and black ink on the selected-button fill is the only legible pairing for
orange. A colour that works on one tone's chrome need not work on the other's.

## Decision

The audio colour is a per-tone token: orange (`#FF8000`) in the dark tone and a light, muted blue
(`#76A1CC`) in the light tone. Each tone also names the ink drawn on an audio fill; both are
black. Every audio-coloured element takes the token of the current tone. Control signals stay
lavender in both tones.

## Alternatives considered

- **One audio colour for both tones** (the previous rule) — orange on light chrome looks washed
  out, and no single hue sits well on both near-white and near-black chrome.
- **A darker orange in the light tone** — keeps one hue family but darkens towards brown, losing
  the signal's brightness; blue gives clearer contrast with light grey chrome.
- **A saturated mid blue (`#0073E6`)** — tried first; too heavy against light chrome, and it
  needed white ink.
- **White ink on the light-tone blue** — about 2.6:1 contrast on `#76A1CC`, against about 8:1
  for black.

## Consequences

- A signal's colour now depends on the tone, so screenshots and descriptions must name the tone.
- The selected-wire highlight mixes the audio colour towards the tone's text colour, which is
  unchanged in dark and darker rather than washed out in light.
- The light blue is a starting value, tuned by eye; changing it needs no new record.
