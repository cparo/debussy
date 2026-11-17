# ADR-0156 — Sampler zones are edited in rows in the sampler's dialog, added from files on C3 or as a kit from the General MIDI kick

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-16
- **Issue:** #174
- **Architecture:** [Sampler zones](../architecture/node-dialogs.md#sampler-zones)

## Context

PR 1 of #174 (ADR-0154, ADR-0155) gave instrument nodes a list of sampler zones, edited through
`Session::set_zones`, with nothing in the interface to make or change them. Issue #174 asks users
to map their own files to notes in two ways: one sample per note range, pitched across it, or a
drum kit of one-shots with choke groups.

## Decision

- **Rows in the node dialog.** The sampler's dialog shows its zones above the generated parameter
  controls, following the custom-section precedent of the mixer and the effect displays. Each row
  holds the file name, the low, high and root keys as pick lists, a One-shot/Gated button, a choke
  pick list and Remove. Each change replaces the whole zone list in one step.
- **Keys named in the track's key.** The key pick lists use the spelling of #192: pitch names
  in the key of the first MIDI track that feeds the sampler, or sharps if no track does.
- **Two ways to add zones.**
  - "Add sample…" takes one file and makes a gated zone on C3 (48), rooted on C3. That is a
    one-key zone to widen, a common starting point for a melodic instrument.
  - "Add kit…" takes several files and makes one-shot zones on consecutive single keys from
    36 (C2), the General MIDI kick, so a kit ordered kick, side stick, snare… plays General MIDI
    drum parts as recorded. Chris Paro chose 36 over the plan's C1 (24).

  Both import through `import_export::import_sample` behind the `Services` seam, and both are
  disabled while playing, like Import.
- **Load failures on the status line.** After zones are added, the engine bridge's latest warning
  (a zone file it could not read) is shown on the status line.

## Alternatives considered

- **A keyboard-map canvas with drag-to-resize zones.** It shows overlaps at a glance, but it is a
  new custom widget with its own hit testing and keyboard model. Rows with pick lists use existing
  widgets and are enough for the first version.
- **Starting a kit on C1 (24), as the plan had it.** In this project's naming (middle C = C4), that
  is two octaves below the General MIDI kick, so General MIDI drum parts would miss every kit
  until each zone was moved.
- **Choosing the zone's keys before importing.** This adds a step before every import. Fixed
  defaults followed by editing in the rows is quicker for the common case.

## Consequences

- Zones that overlap are allowed and resolve to the first match. The rows don't flag overlaps.
- The zone rows aren't in the dialog's Tab order; they are used with the pointer.
- Adding a kit stops at the top key (127), and the model's 128-zone limit refuses anything beyond
  it with a status message.
