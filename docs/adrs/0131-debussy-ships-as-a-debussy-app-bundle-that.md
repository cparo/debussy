# ADR-0131 — Debussy ships as a Debussy.app bundle that declares the project as a package type

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #128
- **Architecture:** [App bundle](../architecture/platform.md#app-bundle)

## Context

A project is a `<name>.debussy` directory ([ADR-0013](0013-a-project-is-a-bundle-directory-with-versioned.md)).
Without a declaration of the type, the Finder shows it as a folder that can be opened and edited
file by file, and AirDrop and Mail send it as many files (#128). macOS reads type declarations only
from an application bundle's `Info.plist`, and Debussy was built only as a bare executable.

## Decision

Debussy ships as `Debussy.app`, assembled by an in-house `cargo xtask bundle`: it builds
`debussy` in release mode, copies it to `Contents/MacOS/debussy`, writes `Contents/Info.plist`
from the template `crates/macos_platform/bundle/Info.plist` with the `debussy` crate's version
substituted, and signs the bundle ad hoc (`codesign --sign -`).

- Bundle identifier `io.github.cparo.debussy`; project type identifier
  `io.github.cparo.debussy.project`, under the repository's GitHub Pages domain, which the project
  controls.
- The `Info.plist` exports the project type (`UTExportedTypeDeclarations`) with filename extension
  `debussy`, conforming to `com.apple.package` (shown and sent as one item) and
  `public.composite-content` (a document holding other content), and claims it
  (`CFBundleDocumentTypes`) with role Editor, rank Owner, `LSTypeIsPackage`.
- It carries `NSMicrophoneUsageDescription`, which macOS requires before a bundled app may record.
- The working format is unchanged: a project is still the same directory.
- The template lives in the platform crate, beside the other macOS-specific code.

## Alternatives considered

- **`cargo-bundle`** — rejected: an external tool to install with its own dependency tree, for a
  job of one copy, one template substitution and one `codesign` call; xtask builds on `std` alone
  ([ADR-0002](0002-repository-tasks-are-built-in-house-on-std-and.md)).
- **A `.dmg` with a Developer ID signature and notarisation** — deferred: it needs a paid Apple
  developer account and credentials in CI. Ad-hoc signing is enough to run locally and to register
  the type with Launch Services.
- **Leave the type undeclared** — rejected: it leaves #128's problems in place and invites edits
  inside the bundle from the Finder.
- **Conform to `public.directory` only** — rejected: that is what the Finder already shows.

## Consequences

- Finder associations need the bundle registered with Launch Services, which happens when it is
  launched or copied to Applications; running the bare `target/*/debussy` binary declares nothing.
- An ad-hoc signed app downloaded from elsewhere is quarantined by Gatekeeper; distribution needs a
  later notarisation step.
- Changing either identifier later would orphan existing associations, so they are fixed.
- Opening a project from the Finder needs a handler for the open-documents event
  ([ADR-0132](0132-finder-open-document-events-arrive-through-an.md)).
