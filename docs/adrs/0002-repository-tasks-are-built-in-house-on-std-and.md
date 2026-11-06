# ADR-0002 — Repository tasks are built in-house on std and anyhow

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Repository tasks (xtask)](../architecture/tooling.md#repository-tasks-xtask)

## Context

CLAUDE.md requires repository tasks (`check`, `new-adr`, `precommit`, `install-hooks`,
`generate-architecture-index`) and makes every dependency a buy/build choice. Tooling defaults to
**buy**, so building needs a stated reason.

## Decision

`crates/xtask` is built on the standard library plus `anyhow` (pre-approved for binaries). Argument
parsing, directory walking, Markdown heading and link parsing, and temp directories for tests are
written in-house.

## Alternatives considered

- **`clap`** for arguments — five commands with a handful of flags do not need it, and it brings a
  dependency tree that would need its own review.
- **`pulldown-cmark`** for Markdown — the checks only need ATX headings, fenced code blocks and
  inline links; a ~200-line parser covers that and is unit tested.
- **`walkdir`, `tempfile`, `regex`** — each replaced by a few dozen lines of std code.

## Consequences

- No new buy/build decisions or licence reviews for tooling; `Cargo.lock` holds only `anyhow`.
- The Markdown parser handles a subset of CommonMark (no setext headings, no reference links). If
  the docs come to need more, buying `pulldown-cmark` is the next step, recorded in a new ADR.
