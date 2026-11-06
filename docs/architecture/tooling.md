# Developer Tooling

## Repository tasks (xtask)

Repository maintenance tasks (documentation checks, ADR creation, the commit gate, hook
installation) live in `crates/xtask` and run as `cargo xtask <command>`.

`crates/xtask` must depend only on the standard library and `anyhow` ([ADR-0002](../adrs/0002-repository-tasks-are-built-in-house-on-std-and.md)).

Line budgets for each kind of document are set in `crates/xtask/src/budget.rs`.

## Commit gate

Every commit must pass `cargo xtask precommit`, run by the git `pre-commit` hook that
`cargo xtask install-hooks` installs. A Claude Code hook in `.claude/hooks/` blocks agent commits
until the git hook is installed, and blocks attempts to bypass it.

CI runs the same gate, and additionally builds and tests on both the MSRV and the pinned toolchain
and runs `cargo deny check`.
