# ADR-0156 — Sampler zones are edited in rows in the sampler's dialog, added from a file on C3 or a folder mapped by name to General MIDI drum keys

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-16
- **Issue:** #174
- **Architecture:** [Sampler zones](../architecture/node-dialogs.md#sampler-zones)

## Context

PR 1 of #174 (ADR-0154, ADR-0155) gave instrument nodes a list of sampler zones, edited through
`Session::set_zones`, with nothing in the interface to make or change them. Issue #174 asks users
to map their own files to notes in two ways: one sample per note range, pitched across it, or a
drum kit of one-shots with choke groups.

## Decision

- **Rows in the node dialog.** The sampler's dialog shows its zones above the generated parameter
  controls, following the custom-section precedent of the mixer and the effect displays. Each row
  holds the file name, the low, high and root keys as pick lists, a One-shot/Gated button, a choke
  pick list and Remove. Each change replaces the whole zone list in one step.
- **Keys named in the track's key.** The key pick lists use the spelling of #192: pitch names
  in the key of the first MIDI track that feeds the sampler, or sharps if no track does.
- **Two ways to add zones.**
  - "Add sample…" takes one file and makes a gated zone on C3 (48), rooted on C3. That is a
    one-key zone to widen, a common starting point for a melodic instrument.
  - "Add kit…" takes a folder and imports every audio file in it and its subfolders. Each file
    becomes a one-shot single-key zone on the General MIDI drum key its path names: kick 36,
    side stick 37, snare 38, clap 39, hi-hats 42, 44 and 46, toms, cymbals and percussion. A second
    crash or ride takes the second crash or ride key, and a tom with no position in its name takes
    the next free tom key. Any other file, or a second file for a drum that is already taken, goes
    on the next free key from 82, above the drum map. Hi-hats share choke group 1 when there is
    more than one. Chris Paro chose name-based General MIDI placement, the fallback above the map,
    and recursive folder import, after first moving kits from the plan's C1 (24) to 36.
  - Problem files don't stop the import: one that is too long or unreadable is skipped, and the
    status line counts what a kit added and names the first file skipped.

  Both import through `import_export::import_sample` behind the `Services` seam, and both are
  disabled while playing, like Import. The folder walk skips hidden files and doesn't follow
  links, and a folder with more audio files than a sampler has zones (128) is refused.
- **Load failures on the status line.** After zones are added, the engine bridge's latest warning
  (a zone file it could not read) is shown on the status line.

## Alternatives considered

- **A keyboard-map canvas with drag-to-resize zones.** It shows overlaps at a glance, but it is a
  new custom widget with its own hit testing and keyboard model. Rows with pick lists use existing
  widgets and are enough for the first version.
- **Consecutive keys in file order, from C1 (24) as the plan had it, or from 36.** C1 is two
  octaves below the General MIDI kick in this project's naming (middle C = C4). From 36, a kit
  lines up with General MIDI only if its files happen to sort as kick, side stick, snare… Neither
  maps a real sample pack.
- **Leaving out files no rule names.** This keeps the drum map clean but silently loses sounds;
  placing them from 82 up keeps them playable.
- **The top folder only, or a multi-file picker.** Packs are often laid out as `Kicks/`,
  `Snares/` and so on, and the folder names carry the drum.
- **Choosing the zone's keys before importing.** This adds a step before every import. Fixed
  defaults followed by editing in the rows is quicker for the common case.

## Consequences

- Zones that overlap are allowed and resolve to the first match. The rows don't flag overlaps.
- The zone rows aren't in the dialog's Tab order; they are used with the pointer.
- Matching is by English words and common abbreviations (BD, SD, HH, OHH). Names in other
  languages, or packs that number their files without names, land from 82 up and need their keys
  set by hand.
- A pack with several variants of each drum puts only the first, in path order, on the drum's
  key.
- Adding a kit stops at the top key (127), and the model's 128-zone limit refuses anything beyond
  it with a status message.
