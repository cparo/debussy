# ADR-0015 — Routing is one general processing graph per project

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Processing graph](../architecture/signal-graph.md#processing-graph)

## Context

The product brief calls for a "patch board" that arranges effects and the connections between them,
between recorded tracks and the project's outputs, alongside a conventional mixer with mute and solo.

## Decision

Routing is a single processing graph per project. Tracks, effects, buses, aux sends and outputs are
all nodes, and the mixer and patch board are two views onto it.

## Alternatives considered

- **Fixed channel strips** (track → inserts → master) — a simpler engine, but the patch board could
  only reorder inserts, and buses or sends would need a redesign.
- **Strips plus fixed buses and sends** — still a graph underneath, with constraints that the UI can
  impose instead.

## Consequences

- Buses, aux sends and parallel effect chains need no special cases.
- The engine needs graph scheduling: a processing order computed off the real-time thread, and
  graph changes handed to the real-time thread as a new, preallocated plan.
- The mixer view must map cleanly onto arbitrary graphs (for example, which strip a bus belongs to).
