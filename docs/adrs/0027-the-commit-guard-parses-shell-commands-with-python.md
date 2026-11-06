# ADR-0027 — The commit guard parses shell commands with Python's standard library

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Commit gate](../architecture/tooling.md#commit-gate)

## Context

The first commit guard was a shell script that grepped the raw JSON of each Bash call. It matched
`git commit`, `--no-verify` and `-n` anywhere in the text, so it blocked commands that only
mentioned them: a `gh pr create` whose body described the guard, a `grep -n` in the same
command as a commit, test inputs for the guard itself. Each false positive cost a retry and
encouraged workarounds such as moving text into files.

## Decision

The guard is a Python 3 script (standard library only, 3.9+). It drops heredoc bodies, tokenises
with shell quoting rules (`shlex`), splits on control operators, recurses into `sh -c` strings, and
parses `git` global options and `git commit` options, including which short options take values.
It blocks a commit before the pre-commit hook is installed, `--no-verify`, `-n`, `-c
core.hooksPath`, and `git config core.hooksPath`. Its `--self-test` cases run in CI.

## Alternatives considered

- **Tighten the shell regexes** — quoting and heredocs cannot be parsed reliably with `grep`.
- **A `cargo xtask` subcommand in Rust** — fits [ADR-0002](0002-repository-tasks-are-built-in-house-on-std-and.md)
  and is unit-testable, but the hook runs before every Bash call. It would pay cargo's startup,
  fail when `cargo` is not on the hook's `PATH`, and wait on the build lock during long builds.
- **`jq` with shell** — parses the JSON but not the shell command inside it.

## Consequences

- Mentions of `git commit` in text no longer block a command; real bypasses still do.
- The guard needs `python3` (shipped with macOS command-line tools, and on CI runners). If it is
  missing, the hook errors without blocking, so the guard fails open; the git hook itself still
  runs on every commit.
- Edge cases it does not see: commands built at runtime (`eval`, variables expanding to `git`),
  and `<<` appearing inside a quoted string, which can hide the lines after it.
