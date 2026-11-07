# ADR-0050 — cpal is added and dependency audits cover Apple targets only

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #52
- **Architecture:** [Dependency licences](../architecture/licensing.md#dependency-licences)

## Context

ADR-0009 chose `cpal` for device I/O; #52 makes the first `Cargo.lock` change for it, which was
put to the developer (`flag-risk`, tag `license`). On macOS `cpal` 0.18.2 builds 15 crates, all
MIT, Apache-2.0, BSD-2-Clause or Zlib. But `Cargo.lock` records every platform's dependencies, so
it also lists about 60 crates for Linux (ALSA), Windows, Android and the web that are never built
here. Auditing them added duplicate-version warnings and licence and advisory checks on code
Debussy does not ship. That was put to the developer as a second question (tags `license`,
`security`).

## Decision

- `macos_platform` depends on `cpal` 0.18.2 with default features. No other crate does.
- `deny.toml`'s `[graph] targets` is the two Apple targets, `aarch64-apple-darwin` and
  `x86_64-apple-darwin`. `cargo deny` checks licences, advisories, bans and sources over what
  those targets build.

## Alternatives considered

- **Auditing every platform's dependencies:** stricter, but it checks code that never ships and
  reports harmless duplicates.
- **Driving the CoreAudio HAL through `objc2-core-audio` directly:** two fewer crates, but far
  more `unsafe` device code to own, against ADR-0009.

## Consequences

- An advisory against a crate used only on another platform is not reported. If Debussy is
  ported, that platform's target is added to `targets`.
- The lock file carries entries that never build on macOS.
