# ADR-0106 — A node's parameters as shown come from one view over static metadata

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-12
- **Issue:** —
- **Architecture:** [Gestures](../architecture/user-interface.md#gestures)

## Context

The architecture review of 2026-11-12 (candidate 2) found that "this node's parameters right now"
was assembled in several places in `debussy_ui`. ADR-0105 removed the zip of display parameters;
what remained:

- The value precedence lived in `App::parameter_value`: the dragged value, else the model's, else
  the running node's default read through the bridge, else 0.
- Metadata had two sources. Effect controls read the parameters cached at launch (ADR-0087);
  the stopped-only check (ADR-0088) asked the running node. Mixers had no static metadata at all.
- Choosing a preset repeated the preset lookup by name and its own stopped-only check, and never
  checked that the preset was for the node's kind, so a preset of another kind was applied by name.
- Keyboard stepping and toggling (ADR-0096), the panel's frozen check and the display's values each
  pulled the same pieces together on their own (ADR-0093).

None of this could be tested without the full `App` fixture.

## Decision

A mixer's or effect's parameters as its dialog shows them are one borrowed view,
`NodeParameters`, built by `App::parameters(node)`. Every reader goes through it: the effect panel
and its display, the preset picker and preset choice, arrow-key stepping and toggle presses, and
the guard that refuses a stopped-only change while playing.

- A parameter's shown value is the dragged value, else the model's, else its kind's declared
  default, else 0. That rule exists only in the view.
- Metadata is static for both node types: an effect's from its kind, read once at launch; a
  mixer's from `audio_effects::mixer_parameter_infos(channels)`, which the running `Mixer` also
  builds its parameters from. The running graph is not asked for metadata or defaults.
- A preset of a kind other than the node's is ignored.

## Alternatives considered

- **Keep the bridge as the fallback for defaults and stopped-only.** Reads would depend on the
  running graph being in step with the model, and the view could not be tested without an engine.
  Static metadata gives the same values, which a drift test in `audio_effects` pins for mixers.
- **Leave mixers out of the view.** Mixers would keep reading defaults from the bridge, leaving
  two precedence rules. Declaring a mixer's parameters statically costs one function the running
  mixer already needed.
- **Build mixer metadata per lookup.** `mixer_parameter_infos` allocates; the mixer strip reads
  three values per channel per frame. `App` instead caches the parameters of a mixer of
  `MAX_CHANNELS` channels and borrows the first three per channel, so the view borrows in every
  case.
- **Make the view own its values (a snapshot).** Copying every value per read is waste for the
  single-value lookups the mixer strip makes; a borrow costs nothing.

## Consequences

- The view is a unit-test surface: precedence, stopped-only refusal, preset edits and key steps
  are tested with no `App`.
- A mixer's parameter layout now has one source; changing a channel's parameters changes the UI's
  metadata with it.
- The mixer dialog's own stepping and mode toggling still compute their edits in `App`, reading
  values through the view; folding them into it is review candidate 4.
