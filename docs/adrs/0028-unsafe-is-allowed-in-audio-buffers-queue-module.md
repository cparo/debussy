# ADR-0028 — `unsafe` is allowed in audio_buffers' queue module only

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #13
- **Architecture:** [Dependency rules](../architecture/crates.md#dependency-rules)

## Context

`audio_buffers` must hand sample blocks between the real-time thread and other threads through a
lock-free single-producer/single-consumer queue whose real-time side is wait-free. The conventional
ring buffer stores items in `UnsafeCell<MaybeUninit<T>>` slots guarded by atomic head and tail
indices, which needs `unsafe`. The crate rules allowed `unsafe` only in `macos_platform` and
`realtime_guard` ([ADR-0026](0026-the-workspace-is-split-into-seventeen-crates-in.md)), so the
choice was put to the developer as a `security` risk.

## Decision

- `audio_buffers` may contain `unsafe` in its queue module only. The crate root sets
  `#![deny(unsafe_code)]` and that module alone opts back in, with a `// SAFETY:` comment on every
  `unsafe` block and `unsafe impl`.
- The queue is tested inside `realtime_guard::assert_realtime` and by two-thread transfer tests
  that check order and contents.
- Every other crate outside `macos_platform` and `realtime_guard` still sets
  `#![forbid(unsafe_code)]`; `cargo xtask check` enforces both.

## Alternatives considered

- **Safe-only design** (queues of indices into a fixed slot table, samples held as `AtomicU32`
  bit patterns) — no `unsafe`, but every sample access is an atomic operation, which blocks
  vectorisation in DSP or forces a copy of each block into plain arrays.
- **Buy `rtrb`** — mature and dependency-free, but the dependency policy defaults to building
  real-time handoff, no correctness or performance gap was found, and real-time core crates
  depend on no third-party crate other than `thiserror`.
- **Defer `audio_buffers`** — blocks `dsp`, `disk_streaming` and everything above them.

## Consequences

- Shipped code now contains hand-written concurrent `unsafe`, so review of that module carries
  more weight; it must stay small and keep its invariants documented beside the code.
- Tests cannot prove the absence of data races. A model checker or Miri run would need a new
  dependency or a nightly toolchain, and would be a separate decision.
- Any other `unsafe` in `audio_buffers`, or in another real-time crate, needs its own record.
