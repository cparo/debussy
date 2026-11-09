# ADR-0070 — Icons are SVG files drawn with iced's svg feature

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-08
- **Issue:** —
- **Architecture:** [Icons](../architecture/user-interface.md#icons)
  ([Dependency licences](../architecture/licensing.md#dependency-licences))

## Context

The project now has a logo, an SVG drawn as a few filled outlines plus a mask. It replaces the
placeholder "D" in the top bar and has to be tinted with the accent of a selected toolbar
button. More icons for controls and other interface elements are planned. `iced` draws SVG only
through its `svg` feature, which changes `Cargo.lock`, so the buy/build choice went to the
developer (`flag-risk`, tag `license`). Enabling the feature then made `cargo deny check
advisories` fail on `rustybuzz` 0.20.1, which RustSec marks as unmaintained (RUSTSEC-2026-0206,
no fixed version). `iced`'s tiny-skia renderer turns on `resvg`'s default text support, which
`iced`'s features cannot turn off. Accepting the advisory is a security trade-off (tag
`security`), so it went to the developer as well.

## Decision

- **Buy:** `iced`'s `svg` feature is enabled in `debussy_ui`. It brings `resvg`/`usvg` 0.45: 34 new
  crates on macOS in all, every one under a licence on the allowlist.
- **Icons are SVG files** in `crates/debussy_ui/assets/icons/`, compiled in with
  `include_bytes!`. Each is a single-colour shape on a transparent background, and the interface
  tints it with a theme colour through `svg::Style::color`.
- **The logo** (`debussy.svg`) is drawn in `theme::AUDIO`, the selected-button accent, directly on
  the top bar with no box or background.
- **RUSTSEC-2026-0206** is ignored by ID in `deny.toml`. `rustybuzz` only shapes text inside SVG
  documents, and the only SVGs drawn are our own compiled-in icons, which hold no text.

## Alternatives considered

- **Build: draw icons with `iced`'s canvas** from the SVG's outline data with an in-house parser.
  No new crates and no advisory. Rejected because more icons are coming: each would need the
  parser to grow (masks, curves, strokes), and the logo's crossing gap could only be faked by
  painting it in the bar's colour.
- **Pause until `resvg` moves off `rustybuzz`** (its suggested replacement is `harfrust`).
  Rejected: the advisory concerns maintenance, not a known vulnerability, and the crate never sees
  untrusted input here.
- **Keep the text placeholder.** Rejected: the logo now exists.

## Consequences

- 34 new third-party crates, including image decoders (PNG, GIF, JPEG, WebP) and a text shaper
  that the icons do not use.
- Drawing an SVG that came from outside the app (a project file, an import, a download) would
  expose `rustybuzz` to untrusted input. That would need a new decision, and would end this
  exception.
- The exception is revisited when `iced` moves to a `resvg` that no longer depends on `rustybuzz`.
- Icon artwork must stay single-colour, since tinting replaces every colour in the file.
