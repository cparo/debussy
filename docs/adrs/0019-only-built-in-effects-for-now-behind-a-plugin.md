# ADR-0019 — Only built-in effects for now, behind a plugin-capable node interface

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Effect nodes](../architecture/signal-graph.md#effect-nodes)

## Context

The brief lists a core set of built-in effects (EQ, dynamics, amp simulation, delay/reverb,
chorus/flanger). Third-party plugins (AU, CLAP, VST3) would widen the palette, but hosting them
means `unsafe` FFI, crash isolation, and per-format quirks.

## Decision

Ship built-in effects only. The effect node interface (latency reporting, port declaration,
parameters) is shaped so a plugin wrapper could implement it later.

## Alternatives considered

- **Audio Units now** — the native macOS ecosystem, but needs `unsafe` FFI, and a crashing plugin
  takes the app down without out-of-process isolation.
- **CLAP now** — an open, MIT-licensed standard that is simple to host, but has fewer plugins.
- **Never** — would mean recording plugin hosting as a product non-goal, which nobody wants yet.

## Consequences

- No plugin `unsafe` code or sandboxing work before the first release.
- Built-in effects must stay honest to the general interface, with no back doors into engine
  internals that a plugin could not use.
- Adding a plugin format later is a new decision: it touches `security` (untrusted code in
  process) and `license`.
