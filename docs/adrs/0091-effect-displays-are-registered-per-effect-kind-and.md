# ADR-0091 — Effect displays are registered per effect kind and draw RGBA pixels from parameter values and the tone

- **Status:** Accepted
- **Decided by:** agent, unratified
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Effect displays](../architecture/user-interface.md#effect-displays)

## Context

Effect dialogs should show a picture of what the effect does (a filter's response curve, say)
above their controls. No effect has one yet, but the slot and the interface a display plugs into
are needed first, so displays can be added one by one.

## Decision

`debussy_ui` defines a trait `EffectDisplay` with one method, `draw`, given each parameter's name,
value and unit, the tone, and a size in physical pixels, and returning `Option<Pixels>`: a
validated buffer of non-premultiplied RGBA bytes. Displays are registered by effect kind in a
static table in `effect_display.rs`. The dialog draws the result as an `iced` image stretched to a
512-pixel-tall area spanning the dialog's width; with no display, or none drawn, the area is
charcoal grey.

## Alternatives considered

- **Return an `iced` element or canvas program.** Rejected: it ties every display to `iced`'s
  widget API, while plain pixels can be drawn and tested with no toolkit, as the lanes already are
  ([ADR-0068](0068-lanes-are-rasterised-in-memory-and-blurred-with-a.md)).
- **Return an encoded image (PNG).** Rejected: the build has no image codecs
  (`image-without-codecs`), and encoding only to decode again is waste.
- **A method on the effect's processing node in `signal_graph`.** Rejected: the display is
  interface work that needs the tone, which the graph knows nothing of.

## Consequences

A display is redrawn and its image re-uploaded each time the dialog is drawn; a display costly
enough to matter will need a cache keyed by its inputs. Displays see only parameter values, not the
audio passing through, so an analyser (a live spectrum, say) would need further inputs.
