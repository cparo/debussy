# ADR-0157 — Refused imports open an alert with the reason and the files concerned

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #174
- **Architecture:** [Files and devices](../architecture/user-interface.md#files-and-devices)

## Context

Refused imports used to report only on the status line, and some reported nothing at all:

- an empty folder for "Add kit…";
- a kit that would take a sampler past its 128 zones, which surfaced as the model's terse
  `InvalidZone` text.

The status line is small, sits at the bottom of the window, and is replaced by the next action.
While trying sample packs, Chris Paro saw imports fail with no clear message and asked for a
meaningful pop-up when an import is refused.

## Decision

- **An alert for refused imports.** When an import is refused, or skips files, the window opens a
  modal alert over everything else. It shows:
  - a title naming what was not done ("Kit not added", "Sample not added", "Import failed",
    "Some files were skipped");
  - the reason and what to do about it;
  - the files concerned, each with its reason: the first 8, then a count of the rest.

  The reason and the list form one read-only `text_editor`. Its text wraps to the alert's width,
  so all of it is visible at once, and it can be selected and copied with ⌘C. An edit is
  ignored, so the text stays as written. It has one OK button. Enter, Space, Escape or a press
  outside dismisses it. While open, it takes the focus keys and the window's shortcuts, so ⌘C
  copies only the alert's text and Undo or Delete can't change the project behind it.
- **Reasons that say what to do.** The cases worded for users are:
  - an empty folder;
  - a folder of more than 128 files (choose a single kit's folder);
  - a kit that would pass the zone limit, with the current zone count and the number the kit
    would add;
  - a file over 60 s;
  - no free key left above the drum map.

  The zone limit is checked before the edit, so the model's own error text isn't shown.
- **The status line keeps successes.** "Added N samples." stays on the status line.

## Alternatives considered

- **A longer status-line message.** It costs nothing to add, but it is easy to miss, it is replaced
  by the next action, and it can't hold a list of files.
- **Selectable single-line `text_input`s, as the status line uses.** They can be copied, but
  they don't wrap, so a long reason or file name runs out of sight.
- **Plain wrapped `text`.** It shows everything, but it can't be selected or copied.
- **A system dialog through `rfd`.** It is native, but it would put platform UI calls behind the
  `Services` seam for every refusal, and its text can't be selected in iced's theme. It is also
  awkward to test.
- **Refusing the whole kit when any file fails.** This is simpler to explain, but one bad file in
  a large pack would block the rest.

## Consequences

- Other refusals (export, open, device) still use the status line. They can move to `Alert` later
  without new widgets.
