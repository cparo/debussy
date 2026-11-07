# ADR-0049 — Saved projects use their own format types, crash-safe renames, timestamped versions and checked loads

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #51
- **Architecture:** [Saved format](../architecture/project.md#saved-format)
  ([Saving](../architecture/project.md#saving), [Loading](../architecture/project.md#loading),
  [Cleanup](../architecture/project.md#cleanup), [Project bundle](../architecture/project.md#project-bundle))

## Context

ADR-0013 fixed the bundle's layout but not the JSON inside it, how it is written safely, how
named versions map to files, when unreferenced audio is removed, or how a file from elsewhere is
checked. Each is a persisted-format (`product`) or untrusted-input (`security`) decision, so they
were put to the developer for #51.

## Decision

- **Format types:** `project_storage` defines its own saved types and converts to and from
  `project_model`, so a change to the model never silently changes the file. Version 1 holds
  `format_version`, `sample_rate`, `tracks`, `nodes` and `connections`. Every field is required,
  unknown fields are refused, times are frames, gains dB, and kinds are tagged with `kind`.
- **Migrations:** each step turns version *n* into *n* + 1 over the parsed JSON before it is
  read into the format types. Version 1 is the first, so there are none yet.
- **Crash-safe saving:** write to `<file>.tmp`, sync, rename over the file, sync the directory.
  Leftover `.tmp` files are deleted on open.
- **Versions:** each is `versions/<stamp>.json`. The stamp is the moment the version was first
  saved, in ISO 8601 basic UTC with milliseconds (`20261107T153012.345Z`), stored in the file as
  `saved_at` with its `name`. The name can be localised for display alongside the stamp. The basic
  form has no colons, which Finder shows as slashes. A save writes only new or changed versions.
- **Cleanup:** audio that neither the state nor any version uses is deleted on open and on
  close, never during a session, since undo history is not saved but can refer to it.
- **Load limits and checks:** 64 MB per file, nesting depth 64, 10,000 tracks, 1,000,000 clips,
  10,000 nodes, 100,000 connections, 10,000 versions, names up to 1024 bytes. Audio file names
  must be plain names inside `audio/`. Clips, ids, connections, pans and the sample rate are
  checked, and any failure refuses the bundle.

## Alternatives considered

- **serde derives on `project_model`'s types:** less code, but the file format would follow the
  in-memory types, and a core crate would take on serde.
- **Keeping a `.bak` copy of `project.json`:** an extra file per save, for a case the rename
  already covers.
- **Version files named by a counter, or by the encoded name:** a counter carries no meaning; an
  encoded name needs escaping, length limits and care on case-insensitive file systems.
- **Cleanup only on command, or only on close:** bundles would grow, or a crash would leave
  files until the next clean close.
- **Validation without size limits:** a hostile file could exhaust memory.

## Consequences

- A newer file is refused rather than partly read.
- A bundle with one invalid version file does not open; nothing is deleted from it first.
- Writing `project_model` changes into the format is a deliberate step in `project_storage`; a
  new model field needs a format change, and perhaps a version bump.
