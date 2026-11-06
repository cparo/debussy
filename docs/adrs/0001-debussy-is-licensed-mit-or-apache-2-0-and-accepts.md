# ADR-0001 — Debussy is licensed MIT OR Apache-2.0 and accepts only permissively licensed dependencies

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Project licence](../architecture/licensing.md#project-licence)

## Context

The developer chose a permissive open-source licence with direct download distribution (a notarized
`.app`, not the Mac App Store). The project's own licence decides which dependency licences are
compatible, and CLAUDE.md's dependency policy needs a concrete allowlist in `deny.toml` to enforce.

## Decision

Debussy is dual-licensed `MIT OR Apache-2.0`. `deny.toml` allows only permissive licences (MIT,
Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0). Copyleft licences, weak copyleft
included, are not allowed without human review.

## Alternatives considered

- **MIT only** — simpler, but Apache-2.0 adds an explicit patent grant from contributors, which
  matters for an audio project with codec and DSP patent exposure.
- **Apache-2.0 only** — incompatible with GPLv2-only downstream users; the dual licence is the Rust
  ecosystem convention and costs nothing.
- **GPL** — would allow copyleft dependencies but rule out permissive reuse of the engine.

## Consequences

- Crates under MPL-2.0, LGPL or GPL (common in audio and in some GUI dependency trees) are rejected
  by `cargo deny` until a human approves an exception, recorded in a new ADR.
- Distribution outside the App Store means no sandbox entitlements to design around, but the app
  must be signed and notarized.
- The specific dual-licence form was the agent's choice under the developer's "permissive" direction
  and awaits ratification.
