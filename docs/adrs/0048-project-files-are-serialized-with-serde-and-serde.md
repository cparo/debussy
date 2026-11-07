# ADR-0048 — Project files are serialized with serde and serde_json

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #51
- **Architecture:** [Dependency rules](../architecture/crates.md#dependency-rules)

## Context

`project_storage` (#51) reads and writes `project.json` and the version files. Adding a crate
changes `Cargo.lock`, so the buy/build choice was put to the developer (`flag-risk`, tag
`license`). Serialization defaults to buy (CLAUDE.md "Dependency Policy").

## Decision

`project_storage` uses `serde` 1.0.229 with the `derive` feature and `serde_json` 1.0.154, both
MIT OR Apache-2.0 and maintained by the serde-rs project, caret-pinned as library dependencies.
They add seven third-party crates to the lock file: `serde`, `serde_core`, `serde_derive`,
`serde_json`, `itoa`, `memchr` and `zmij` (MIT; float formatting). The proc-macro crates
`serde_derive` needs were already present through `thiserror`. No other crate depends on them.

## Alternatives considered

- **serde and serde_json without derive:** five crates instead of seven, but a hand-written
  `Serialize`/`Deserialize` impl for every saved type.
- **An in-house JSON writer and parser:** no dependencies, but against the buy default, and the
  parser is untrusted-input code we would own.
- **Deferring #51.**

## Consequences

- `cargo deny` checks the new crates' licences; all are in the allowlist.
- The saved types derive serde; `project_model` stays free of it (ADR-0049).
