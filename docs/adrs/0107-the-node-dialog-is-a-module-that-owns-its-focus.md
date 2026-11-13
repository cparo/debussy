# ADR-0107 — The node dialog is a module that owns its focus and drag and returns edits

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Gestures](../architecture/user-interface.md#gestures)

## Context

The architecture review of 2026-11-12 (candidate 3) found the node dialogs' behaviour spread
through `App`. ADR-0106 moved a node's parameters as shown behind `NodeParameters`, but `App`
still held the dialog's keyboard focus (ADR-0089, ADR-0096) and the slider being dragged, about a
dozen arms of `App::handle` (slide, release, the focus keys, presets, parameter sets, mute and
solo), and the helpers behind them. The effect panel was `impl App` view code. Testing a key
press therefore needed the full `App` fixture with an engine and a bridge, and tests named
controls by index (`DialogFocus::Parameter(6)`).

## Decision

The mixer and effect dialogs share one module, `node_dialog`, which owns their keyboard focus and
the slider being dragged.

- Their messages are a nested enum, `Message::Dialog(DialogMessage)`, forwarded by `App` in one
  arm. `App` keeps only the delete confirmation's hold on the focus keys, checked before
  forwarding.
- `NodeDialog::update` reads the project through a borrowed `Context` (the parameter catalog, the
  state, whether playing, and the open node) and returns `Command`s: an edit to record, a live
  parameter value, a status line, a mixer channel step, or committing the typed field. `App`
  carries them out through the bridge. The module never touches the session or the bridge.
- The launch caches of effect and mixer parameters are a `ParameterCatalog`, which builds the
  `NodeParameters` view; `App` and the dialog borrow it from separate fields.
- Focus stays an index into the node's parameters (`DialogFocus::Parameter(usize)`), and `Copy`.
  Tests turn parameter names into indices.
- The effect panel is a free function over `NodeParameters` and the focused index. The mixer
  strip's view stays on `App`.

## Alternatives considered

- **Effects only, the mixer left in `App`.** The two dialogs share one focus (ADR-0096); splitting
  it would leave `App` deciding which dialog a key belongs to, and the "Channels" field's commit on
  leaving would straddle both.
- **A flat `Message` with the dialog variants kept at the top level.** It works, but `App::handle`
  would still list every dialog variant, and the status and menu filter would repeat the focus
  keys. A nested enum gives the module one entry point and lets it say which of its messages are
  focus keys.
- **Let the module mutate `App` (a `&mut App` or a trait over it).** That keeps the coupling the
  review set out to remove and needs the `App` fixture to test. Returned commands are plain data
  that tests compare.
- **Focus by parameter name.** Readable in tests, but `DialogFocus` would stop being `Copy` and
  every frame's ring check would compare strings. A test helper gives the readability.

## Consequences

- Dialog behaviour is unit-tested with a `Session`, a `ParameterCatalog` and the module: Tab
  order, arrow steps, presses, drag and release, stopped-only refusals, presets and mute and solo.
  `tests/app.rs` keeps one end-to-end case per path.
- Mixer stepping and mode toggling moved in as they were; unifying them with the effect path
  inside the module is review candidate 4.
- A new dialog control adds a `DialogMessage` variant and, if it changes the project, returns a
  `Command`; a new kind of side effect needs a new `Command` variant that `App::run` handles.
