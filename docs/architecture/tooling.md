# Developer Tooling

## Repository tasks (xtask)

Repository maintenance tasks (documentation checks, ADR creation, the commit gate, hook
installation) live in `crates/xtask` and run as `cargo xtask <command>`.

`crates/xtask` must depend only on the standard library and `anyhow` ([ADR-0002](../adrs/0002-repository-tasks-are-built-in-house-on-std-and.md)).

Line budgets for each kind of document are set in `crates/xtask/src/budget.rs`.

`cargo xtask check` also enforces `docs/architecture/crates.md` "Dependency rules" on every
crate's direct dependencies, dev- and build-dependencies included. Each crate's kind is set in
`crates/xtask/src/crate_rules.rs`; a workspace crate missing from it fails the check.

## Commit gate

Every commit must pass `cargo xtask precommit`, run by the git `pre-commit` hook that
`cargo xtask install-hooks` installs. A Claude Code hook in `.claude/hooks/` blocks agent commits
until the git hook is installed, and blocks attempts to bypass it.

The Claude Code hook is a Python 3 script using only the standard library. It inspects only the
commands a Bash call would run (quoted text and heredoc bodies are not commands), and its
`--self-test` cases run in CI ([ADR-0027](../adrs/0027-the-commit-guard-parses-shell-commands-with-python.md)).

CI runs the same gate, and additionally builds and tests on both the MSRV and the pinned toolchain
and runs `cargo deny check`.

Every action a workflow uses is pinned to a full commit SHA, with its release tag in a comment.
Dependabot proposes updates to those pins, and to nothing else ([ADR-0033](../adrs/0033-github-actions-are-pinned-to-commit-shas-and.md)).
