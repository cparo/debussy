# ADR-0142 — Release notices are generated in xtask and releases are published by a tag-triggered workflow

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-15
- **Issue:** #168
- **Architecture:** [Releases](../architecture/platform.md#releases)

## Context

[ADR-0138](0138-the-first-release-is-an-unsigned-github-build.md) defines the first release: a tag
and a GitHub Release carrying the zipped, ad-hoc-signed `Debussy.app` with third-party licence
notices inside. Most of the licences we ship (MIT, Apache-2.0, Zlib) require their text and
copyright notice to travel with binaries, so the notices must be complete and must track
`Cargo.lock`. Building, zipping and uploading by hand is easy to get subtly wrong (a stale bundle,
a mismatched version, `zip` stripping the signature's extended attributes).

When first run against the real dependency tree, 52 of the shipped crates had no licence file in
their published package, though each declares an SPDX expression in its manifest.

## Decision

- `cargo xtask bundle` writes `Contents/Resources/THIRD-PARTY-NOTICES.txt` from `cargo metadata`
  for `aarch64-apple-darwin`: every crate reached from `debussy` along normal dependency edges,
  with its licence files copied verbatim. A crate that ships none gets the standard text of a
  licence its expression offers (MIT, its copyright line naming the crate's authors, else
  Apache-2.0); a crate with neither fails the bundle unless an override in `notices.rs` says why
  it needs no text. Debussy's own `LICENSE-MIT` and `LICENSE-APACHE` go beside it.
- `cargo xtask bundle --zip` zips the bundle with `ditto`.
- `.github/workflows/release.yml` publishes a release when a `v*` tag is pushed, after checking
  that the tag equals the workspace version and the notes file `docs/releases/<tag>.md` exists.
- Every crate inherits one workspace version.

## Alternatives considered

- **cargo-about** — the established tool, but it is a dependency to install and configure (its
  own licence allowlist duplicating `deny.toml`), and xtask already parses `cargo metadata`.
- **An override per crate with no licence file** — the original plan; 52 entries would have been
  a hand-kept list that silently goes stale. The standard-text fallback is what cargo-about does.
- **Publishing locally with `gh release`** — depends on the releaser's machine and toolchain, and
  nothing ensures tests ran or the version matched.
- **Notes only in the release body** — they would not be reviewed in a pull request or kept in
  the repository.

## Consequences

- The release job holds `contents: write`; it runs only on tag pushes, which only maintainers can
  make.
- A new dependency whose expression offers neither MIT nor Apache-2.0 and ships no licence file
  stops the bundle until someone reviews it.
- Proc-macro crates are listed although they generate code rather than link; that errs towards
  completeness.
- Signing with a Developer ID and notarizing remain out of scope, as ADR-0138 says.
