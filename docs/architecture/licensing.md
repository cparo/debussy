# Licensing

## Project licence

Debussy is licensed under `MIT OR Apache-2.0` ([ADR-0001](../adrs/0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md)). Every crate in the workspace must inherit this with
`license.workspace = true`.

## Dependency licences

Every dependency, direct or transitive, built for an Apple target must carry a licence on the
allowlist in `deny.toml`. `cargo deny check` enforces this in CI, over the dependency graph of
`aarch64-apple-darwin` and `x86_64-apple-darwin` only ([ADR-0050](../adrs/0050-cpal-is-added-and-dependency-audits-cover-apple.md)).

An advisory may be ignored only by its ID, with its reason, in `deny.toml`, after human review.
The ignored IDs are RUSTSEC-2024-0436 (`paste`) and RUSTSEC-2026-0192 (`ttf-parser`), both under
`iced` ([ADR-0055](../adrs/0055-iced-0-14-is-added-the-msrv-rises-to-1-90-and-cc0.md)), and
RUSTSEC-2026-0206 (`rustybuzz`), under `iced`'s `svg` feature ([ADR-0070](../adrs/0070-icons-are-svg-files-drawn-with-iced-s-svg-feature.md)).

Copyleft licences (GPL, LGPL, AGPL, and weak copyleft such as MPL) must not be added to the
allowlist without human review, raised with the `flag-risk` skill ([ADR-0001](../adrs/0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md)).

## Bundled assets

Audio and artwork compiled into the application are the project's own work or CC0-1.0. Each
directory of bundled audio holds a `README.md` giving every file's licence and source; the click
sounds' is `crates/engine_bridge/assets/clicks/README.md` ([ADR-0083](../adrs/0083-the-click-is-mixed-at-the-output-stage-from.md)).
