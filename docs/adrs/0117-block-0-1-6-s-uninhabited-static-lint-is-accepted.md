# ADR-0117 — block 0.1.6's uninhabited-static lint is accepted until iced moves to wgpu 29

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Dependency licences](../architecture/licensing.md#dependency-licences)

## Context

Cargo reports `block` 0.1.6 as triggering rustc's `uninhabited_static` future-incompatibility lint
([rust-lang/rust#74840](https://github.com/rust-lang/rust/issues/74840)): it declares
`static _NSConcreteStackBlock: Class`, where `Class` is uninhabited, and rustc will reject that in a
future release. `cargo report future-incompatibilities` shows the warning. RustSec has no advisory
for it, so `cargo deny check advisories` passes.

`block` reaches us only through wgpu's Metal backend:
`iced` 0.14.0 → `iced_wgpu` / `cryoglyph` → `wgpu` 27 → `wgpu-hal` 27.0.4 → `metal` 0.32 → `block`.
`wgpu-hal` replaced `block` with `block2` and `objc2-metal` in version 29. `iced` 0.14.0 is the
latest release and requires `wgpu = "27"`, so we cannot take wgpu 29 without leaving iced 0.14
([ADR-0055](0055-iced-0-14-is-added-the-msrv-rises-to-1-90-and-cc0.md), which accepted the other
unmaintained crates under iced on the same terms).

## Decision

`block` 0.1.6 stays in the graph, with its `uninhabited_static` warning, until an `iced` release
depends on `wgpu` 29 or later. No `[patch]` and no `deny.toml` entry is added: there is no advisory
to ignore.

## Alternatives considered

- **Patch `iced` to its git `master`, which takes a newer wgpu** — rejected: it is unreleased, its API
  is still changing, and a git dependency cannot be pinned to a published version.
- **Drop wgpu and render with iced's tiny-skia backend** — rejected: it draws every frame on the CPU,
  which costs the UI's frame budget for the waveform and meter displays.
- **`[patch]` `block` with a local fork that fixes the static** — rejected: it carries a fork of
  FFI code for a lint that is not yet an error.

## Consequences

- Builds print a future-incompatibility note for `block`. It is a warning, not a lint clippy's
  `-D warnings` denies, so CI is unaffected.
- If a Rust release makes the lint a hard error before iced moves, raising the pinned toolchain in
  `rust-toolchain.toml` past that release will fail to build on macOS. The MSRV build is
  unaffected. At that point the decision must be revisited, with the `[patch]` fork as the likely
  fallback.
- Revisit when an `iced` release depends on `wgpu` 29 or later; upgrading then drops `block`.
