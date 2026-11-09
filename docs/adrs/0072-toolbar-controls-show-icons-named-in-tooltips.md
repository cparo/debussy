# ADR-0072 — Toolbar controls show icons named in tooltips

- **Status:** Accepted
- **Decided by:** agent, unratified
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Icons](../architecture/user-interface.md#icons)

## Context

ADR-0070 planned icons for interface controls. A first set was drawn (undo, redo, cut, copy,
paste, play, pause, stop, record, and sun and moon for the light and dark tones) and approved by
the developer, who asked for it to replace the text on the matching buttons. That raised how an
icon is coloured on a button whose face changes with selection and disablement, how it is named
once its text is gone, and which buttons keep text.

## Decision

- Undo, redo, cut, copy, paste, play/pause, stop, record and the tone toggle show an icon in
  place of their text. The tone toggle shows the tone it switches to, as its text did.
- An icon is tinted with the ink its button's text would have, at the disabled text's opacity
  when the button is disabled.
- An icon button names its action in a tooltip that follows the cursor.
- An icon is 16 logical pixels square, padded so its button is as tall as a text button.
- Buttons with no icon in the set keep their text: the waveform/spectrogram and mono/stereo
  toggles, zoom, and the mixer and patch-board buttons.

## Alternatives considered

- **Icon and text together.** Rejected: it widens the bars, and the set was drawn to stand alone.
- **No tooltip.** Rejected: an icon alone gives a new user nothing to read, and the tooltip costs
  only `iced`'s built-in widget.
- **Tooltips placed above or below the button.** Rejected: the same helper serves the top and
  bottom bars, and a fixed side runs off one of them; following the cursor works in both.

## Consequences

- The tooltip text is English and in code, like every other label; it moves with them if the
  interface is ever translated.
- A new icon control needs an artwork file, an `Icon` variant and a tooltip name.
- Hover state does not change an icon's tint, as it does not change a text label's.
