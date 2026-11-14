# ADR-0129 — MP3 export runs a host-installed LAME as a separate process

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-14
- **Issue:** #8
- **Architecture:** [MP3 export](../architecture/import-export.md#mp3-export)

## Context

The product brief lists MP3 export (#8). [ADR-0024](0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md) found that AudioToolbox decodes MP3
but cannot encode it, and deferred MP3 export. The usable encoder is LAME, which is LGPL: linking
or bundling it would put copyleft code in a distribution licensed `MIT OR Apache-2.0`, and LGPL is
not on `deny.toml`'s allowlist. The plan for this record raised the licence, security and
portability questions with the `flag-risk` skill; the developer approved the answers below.

## Decision

MP3 export runs a `lame` executable the user installed (for example with `brew install lame`) as a
separate process, and is offered only when one is found at startup. AudioToolbox still decodes MP3
and encodes and decodes AAC and FLAC, as ADR-0024 decided; only its deferral of MP3 export is
superseded.

- **Licence:** lame is run, not linked; Debussy distributes no LGPL code, `Cargo.lock` and
  `deny.toml` do not change. External programs are only ever run as separate processes, never
  bundled.
- **Security:** the fixed package-manager folders are searched before `PATH`; the binary must
  answer `--version` with `LAME` (within a timeout) before MP3 is offered; it runs with an argument
  list and no shell; it is given no file path (PCM in on standard input, MP3 out on standard
  output, into a file Debussy opened), so a file name cannot inject an option; standard error is
  read but capped at 4 KiB.
- **Portability:** the folders searched are macOS package managers' and live in
  `macos_platform::folders`; the process encoder uses only `std` and lives in `import_export`.
- **Format:** constant 128, 256 or 320 kbit/s (320 by default, chosen in the Export dialog of
  [ADR-0130](0130-export-asks-for-the-format-and-its-settings-before.md) or with `--bitrate`); 16-bit raw input, TPDF-dithered by export; lame resamples projects
  above 48 kHz, MP3's highest rate, to 44.1 or 48 kHz.

## Alternatives considered

- **Link LAME (`mp3lame-encoder` or a `-sys` crate)** — rejected: LGPL code in the binary, which
  the licence policy forbids without relicensing or dynamic-linking obligations.
- **Bundle a `lame` executable in the app** — rejected: distributing it brings the LGPL's
  source-offer obligations into every release.
- **A permissively licensed Rust MP3 encoder** — rejected for now: none mature enough was known,
  and writing one in-house is a large codec project with remaining patent questions to check.
- **Keep MP3 export deferred** — rejected: the product brief asks for it, and users can install
  LAME with one command.
- **Pass lame the output path** — rejected: a path beginning with `-` could be read as an option,
  and lame would open files on Debussy's behalf; piping keeps file handling in Debussy.
- **24-bit raw input** — not adopted: LAME 4.0 (Homebrew's) accepts `--bitwidth 24`, but 3.100,
  still common through MacPorts and older installs, was not confirmed to, and using it would mean
  checking the version as well. MP3's own noise floor is far above 16-bit dither, so 24-bit input
  would gain nothing audible.

## Consequences

- MP3 export works only where the user installed LAME; without it MP3 is absent from the export
  dialog and the command line prints the install hint.
- A `lame` installed after launch is noticed only at the next launch.
- Debussy runs a program found on disk; a compromised `/opt/homebrew/bin` or `PATH` entry could run
  in its place. The `--version` check guards against mistakes, not against a hostile binary.
- Bit rates are constant only; VBR, ID3 tags and other options are left for later issues.
