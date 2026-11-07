# ADR-0033 — GitHub Actions are pinned to commit SHAs and updated by Dependabot

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** #19
- **Architecture:** [Commit gate](../architecture/tooling.md#commit-gate)

## Context

CI referenced `actions/checkout@v4` by tag. A tag can be moved by the action's owner, or by
whoever compromises their account, so CI would run code nobody here reviewed. Pinning trades that
risk for manual updates. The choice was put to the developer as a `security` risk.

## Decision

- Every `uses:` in `.github/workflows/` names a full 40-character commit SHA, with the release tag
  in a trailing comment.
- Dependabot (`.github/dependabot.yml`) checks the `github-actions` ecosystem weekly and opens a
  pull request that updates the SHA and comment together. It does not manage Cargo dependencies,
  which stay a buy/build decision for a human.

## Alternatives considered

- **Pin by SHA only** — nothing changes without a human edit, but pins go stale silently and
  miss the action's own security fixes.
- **Keep tags** — updates arrive automatically, but a moved tag runs unreviewed code in CI.

## Consequences

- Action updates arrive as pull requests to review and merge, including major-version bumps
  (`checkout` is at v7; this change keeps v4).
- A new workflow step must be added with a SHA pin; review should reject a tag or branch
  reference.
