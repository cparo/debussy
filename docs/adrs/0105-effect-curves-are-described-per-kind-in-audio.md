# ADR-0105 — Effect curves are described per kind in audio_effects and displays are chosen by the kind of curve

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Effect curves](../architecture/effects.md#effect-curves)

## Context

ADR-0104 left one question open: how each display finds its parameters. An architecture review
found three conventions for the curve a display plots.

- The equaliser display passed values by position to `frequency_response(kind, …)`, which matched
  on the kind id again after the UI's `DISPLAYS` table had already done so. `response.rs` also
  re-encoded the parametric equaliser's parameter layout, separate from the order `parametric.rs`
  declares.
- `dynamics_curve` and `amplifier_curve` took values through a by-name closure. They read string
  literals that repeated the names in each effect's `info()`, while processing reads through the
  effect's private `Control` enum.
- Both of those displays fell back to NaN for a name they could not find. The curve functions'
  doc comments promised the default instead.

A renamed or reordered parameter could blank a display without a compile error. The UI's display
table, keyed by kind id, was not checked against `EFFECTS`. More displays are coming (#61, #62,
#137).

## Decision

- `audio_effects` owns the curves. `Curve` is a closed enum, with the variants `Response`,
  `Transfer` and `Shaper`. `audio_effects::curve(kind, values, sample_rate)` builds one.
- Each `EffectKind` row has a `curve` field: the kind's curve function, or `None`. The function
  lives in the effect's module and reads the declaration-order `values` through the controls
  processing uses. A slice of the wrong length gives `None`.
- The row also builds the effect, through a `build` field, so a kind's registration in
  `audio_effects` is that one row. `effect(kind)` looks the row up.
- `debussy_ui` draws a `Curve` by matching on its variant, with one `render` per variant. It has no
  per-kind table, display trait or parameter-by-name lookup.
- The display's cache is keyed on the curve, the tone and the size, and no longer on the values.
- This supersedes ADR-0091. What it decided about output still holds: displays return
  non-premultiplied RGBA pixels, the area is 320 logical pixels tall, and a kind with no picture
  leaves it charcoal.

## Alternatives considered

- **Keep the per-kind display registry in the UI (ADR-0091)** and only change how values are
  read. It was rejected because the kind would still be dispatched in both crates, and nothing
  would tie the UI table to `EFFECTS`.
- **Pass values to curve functions by name.** It was rejected because names are string literals
  that a rename breaks silently. The `Control` order is the order processing already relies on.
- **A typed values struct per kind.** It was rejected for now as a much larger change that touches
  every consumer of parameter values. It remains open.
- **A `Curve` trait object instead of an enum.** It was rejected because an exhaustive match makes
  a new kind of curve fail to build until it has a display. A trait object would let it pass
  unnoticed.
- **A free `curve(kind, …)` match beside `effect()`.** It was rejected because it would be one
  more table keyed by kind id. A field on the row keeps a kind's registration in one place.

## Consequences

- To add a display for a new kind, write its curve function, set the row's `curve`, and, if the
  kind needs a new variant, add a `render` for it. The registry test checks that every kind's
  defaults give the variant it is expected to give.
- Changing a value the curve ignores, such as a dynamics attack, no longer redraws the display.
- Every curve function takes the sample rate. Only the response uses it.
- A display that needs more than parameter values, such as a live meter (#134), still needs its
  own inputs. A curve variant does not provide them.
