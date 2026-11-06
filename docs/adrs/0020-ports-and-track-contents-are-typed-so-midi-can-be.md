# ADR-0020 — Ports and track contents are typed so MIDI can be added later

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Port types](../architecture/signal-graph.md#port-types)

## Context

MIDI, synthesis and sampling are later items in the brief. Adding event data to an audio-only graph
and timeline later could force a redesign of ports, connections and track contents.

## Decision

Ports, connections and track contents carry a type. Audio is the only type for now, and mismatched
connections are refused.

## Alternatives considered

- **Design MIDI now** — event model, MIDI clips and instruments up front: design effort spent on
  features well after the first release.
- **Defer with no provisions** — cheapest now, but risks a redesign of the graph and the project
  format later.

## Consequences

- Adding an event type later extends the type enum, the scheduler and the project format, without
  reshaping them.
- The project format records the type of each port and clip from the start; see
  [ADR-0013](0013-a-project-is-a-bundle-directory-with-versioned.md) for versioning.
