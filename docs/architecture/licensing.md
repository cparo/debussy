# Licensing

## Project licence

Debussy is licensed under `MIT OR Apache-2.0` ([ADR-0001](../adrs/0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md)). Every crate in the workspace must inherit this with
`license.workspace = true`.

## Dependency licences

Every dependency, direct or transitive, built for an Apple target must carry a licence on the
allowlist in `deny.toml`. `cargo deny check` enforces this in CI, over the dependency graph of
`aarch64-apple-darwin` and `x86_64-apple-darwin` only ([ADR-0050](../adrs/0050-cpal-is-added-and-dependency-audits-cover-apple.md)).

Copyleft licences (GPL, LGPL, AGPL, and weak copyleft such as MPL) must not be added to the
allowlist without human review, raised with the `flag-risk` skill ([ADR-0001](../adrs/0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md)).
