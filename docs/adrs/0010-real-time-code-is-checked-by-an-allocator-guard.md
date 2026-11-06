# ADR-0010 — Real-time code is checked by an allocator guard and clippy bans

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Real-time thread](../architecture/engine.md#real-time-thread)

## Context

CLAUDE.md ("Real-Time Audio Thread") forbids allocation, locks, blocking calls, I/O, logging and
printing on the audio thread, and requires tests to run audio-thread code inside an in-house guard
that fails on any of them. Catching allocation at runtime requires `unsafe` (a custom
`GlobalAlloc`), so the design was put to the developer as a `security` risk before writing it.

## Decision

- **Runtime:** a test-support crate provides a global allocator wrapping the system allocator. A
  thread-local flag marks a thread as real-time; any allocation or deallocation while it is set
  fails the test. Real-time crates use it only as a dev-dependency.
- **Static:** every crate with real-time code carries a `clippy.toml` banning locks, file and
  network I/O, sleeping and printing through `disallowed-types` and `disallowed-methods`, enforced
  by `-D warnings`.

## Alternatives considered

- **Also intercept locks and syscalls at runtime** (dyld interposition) — catches calls hidden in
  third-party code, but needs far more `unsafe`, is macOS-specific, and is fragile under System
  Integrity Protection.
- **Allocator guard only** — leaves locks and I/O to review alone.
- **A crate such as `assert_no_alloc`** — CLAUDE.md requires the guard to be built in-house.

## Consequences

- The `unsafe` is confined to one small test-only crate, with `// SAFETY:` comments; nothing
  shipped contains it.
- Clippy sees only direct calls. A lock or blocking call inside a dependency is caught only by
  review, so real-time crates should keep their dependencies minimal.
- The guard does not detect priority inversion or long-running computation; deadline checks
  against the 32-frame worst case are separate benchmarks.
