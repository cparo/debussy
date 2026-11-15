# ADR-0141 — A project archive is a plain zip of the bundle without its cache, opened only under strict limits

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #129
- **Architecture:** [Project archives](../architecture/project.md#project-archives)

## Context

#129 asks for a single-file form of a project bundle (ADR-0013) that can travel through cloud
drives, chat apps and other systems, and that Debussy can open again. The file is a new persisted
format and, when opened, untrusted input. The issue also suggested compressing audio to FLAC, but
stored audio is 32-bit float (ADR-0014) and FLAC holds at most 24-bit integers.

## Decision

An archive is `<name>.debussy.zip`: a plain zip holding one `<name>.debussy/` directory with
`project.json`, `versions/` and `audio/`, without `cache/`, `lock` or `recording.json`. JSON is
deflated and audio stored. Audio is archived exactly as stored; there is no FLAC option.

Opening one accepts only those entries, refuses unsafe names, symbolic links, encryption and
methods other than stored and deflate, caps entries at 100,000, JSON entries at the 64 MB load limit
and the bytes written at the volume's free space less 1 GB, unpacks into a hidden directory renamed
into place, and then loads the project with the usual checks.

## Alternatives considered

- **A custom extension registered with the Finder** (`.debussyarchive`) — a double-click would
  open Debussy, but other tools would need the file renamed before they could unzip it.
- **Lossy FLAC audio as an option** — smaller archives, but audio quantized to 24 bits and float
  peaks above 0 dBFS clipped. Left for a follow-up issue.
- **Skipping unknown entries** (`__MACOSX/`, `.DS_Store`) instead of refusing the archive — more
  forgiving of re-zipped archives, but accepts content we did not write. The developer chose to
  refuse.

## Consequences

- An archive the Finder unzips is a working bundle, with no Debussy involved.
- Re-zipping a project with the Finder's Compress, which can add `__MACOSX/` entries, gives an
  archive Debussy refuses; unzipping it first and opening the bundle still works.
- Archives are about as large as the bundle, since WAV barely deflates.
