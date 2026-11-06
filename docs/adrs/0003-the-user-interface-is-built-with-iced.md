# ADR-0003 — The user interface is built with iced

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [UI toolkit](../architecture/user-interface.md#ui-toolkit)

## Context

The product brief does not mention the user interface. The interface needs custom widgets
(waveforms, spectrograms, a mixer, a patch board, curve editors) and must keep platform-specific
code behind a seam so other platforms stay possible.

## Decision

The GUI is written in Rust with `iced`.

## Alternatives considered

- **Swift/AppKit shell over a Rust core** — the most native feel, but two languages, a C interface
  to maintain, and a UI rewrite for any other platform.
- **Webview (for example Tauri)** — rich UI tooling, but adds a JavaScript toolchain and an IPC
  boundary between the UI and the engine.
- **Other pure-Rust toolkits (`egui`, Slint)** — considered alongside `iced`; the developer chose
  `iced`.

## Consequences

- Single language and toolchain; the UI is portable along with the core.
- Native macOS look and behaviour (menus, accessibility) depend on what `iced` provides.
- Adding `iced` will change `Cargo.lock` with a large transitive tree; `cargo deny` must pass on it,
  and any licence outside the allowlist ([ADR-0001](0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md))
  goes to human review.
