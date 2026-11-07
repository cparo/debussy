# ADR-0055 — iced 0.14 is added, the MSRV rises to 1.90, and CC0 is allowed

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #55
- **Architecture:** [UI toolkit](../architecture/user-interface.md#ui-toolkit)
  ([Dependency licences](../architecture/licensing.md#dependency-licences))

## Context

ADR-0003 chose `iced`; #55 makes the first `Cargo.lock` change for it (`flag-risk`, tag
`license`). Three findings went to the developer. `iced` 0.14 needs Rust 1.88, and one of its
dependencies, `ordered-float` 5.5.1, needs 1.90, above the workspace MSRV of 1.85. Its GPU renderer
brings `hexf-parse` 0.2.1 under CC0-1.0, which was not in the allowlist. And `cargo deny` reports
two of its crates as unmaintained: `paste` (RUSTSEC-2024-0436) and `ttf-parser`
(RUSTSEC-2026-0192). The last is a security trade-off (tag `security`).

## Decision

- **`iced` 0.14.0** in `debussy_ui` only, with default features off and `wgpu`, `tiny-skia`,
  `canvas` and `thread-pool` on: about 179 third-party crates on macOS, without a bundled font.
- **MSRV 1.90:** the workspace `rust-version`, the CI MSRV job and CLAUDE.md move from 1.85 to
  1.90. The pinned development toolchain is unchanged.
- **CC0-1.0** joins the licence allowlist for any crate.
- **Advisories:** RUSTSEC-2024-0436 and RUSTSEC-2026-0192 are ignored by ID, each with its reason
  in `deny.toml`. `paste` is a compile-time macro under `wgpu`'s Metal backend. `ttf-parser`
  parses only the system's fonts, never project or imported files. Any other advisory still
  fails the check.

## Alternatives considered

- **`iced` 0.13.1 at MSRV 1.85:** builds today, but is the previous release and would need a
  migration later.
- **A CC0 exception for `hexf-parse` only, or the software renderer only:** the exception is
  narrower; the software renderer avoids CC0 but draws dense waveforms and spectrograms on the CPU.
- **Warning on every unmaintained advisory:** less upkeep, but future ones would pass silently.

## Consequences

- Raising the MSRV let clippy suggest newer APIs (`as_chunks`, let-chains, `is_multiple_of`);
  existing code was updated to them.
- The ignored advisories are revisited when `iced` moves off those crates.
- CC0-licensed crates no longer need a human review.
