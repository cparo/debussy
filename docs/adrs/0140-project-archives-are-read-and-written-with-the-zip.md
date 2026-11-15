# ADR-0140 — Project archives are read and written with the zip crate, deflate only

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #129
- **Architecture:** [Project archives](../architecture/project.md#project-archives)

## Context

Share/Archive… (#129) must write and read zip files. Archives are I/O, where the dependency policy
defaults to buying a crate, and reading one is untrusted input.

## Decision

`project_storage` depends on `zip` `=9.5.1` (MIT, zip-rs/zip2) with default features off and only
`deflate-flate2-zlib-rs`: deflate through `flate2` on its pure-Rust `zlib-rs` backend. That adds
six crates to `Cargo.lock` (`zip`, `constant_time_eq`, `nix`, `typed-path`, `zeroize`,
`zeroize_derive`); `flate2`, `zlib-rs` and `crc32fast` were already there through `iced`. No
other crate may depend on `zip`.

## Alternatives considered

- **Build a zip reader and writer** — no new crates beyond `flate2`, but about 600 lines of ZIP64
  and central-directory parsing to own, including every untrusted-input edge case.
- **Default features** — adds AES, bzip2, LZMA, zstd, PPMd and `time`, none of which an archive we
  write uses, plus their C and crypto dependencies.
- **Not offering archives** — leaves users with no way to send a project through services that
  do not understand packages.

## Consequences

- Archives written by other tools using other methods (bzip2, LZMA, zstd, Deflate64, AES) cannot
  be read.
- `zip`'s MSRV (1.88) is below ours (1.90); an update that raises it past ours has to wait.
