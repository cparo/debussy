# ADR-0090 — The interface opens in the dark tone and the light-tone logo is sage green

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

The tone followed the system's appearance at launch and whenever it changed
([ADR-0060](0060-the-window-is-laid-out-as-top-bar-track-pane.md)). The dark tone is the one the
interface is designed around, and the user asked for it at every launch. In the light tone the logo
took the light audio blue ([ADR-0071](0071-the-audio-colour-is-orange-on-dark-chrome-and-blue.md)),
and the user asked for sage green instead.

## Decision

The interface opens in the dark tone whatever the system's appearance, and ignores later changes
to it; only the light/dark toggle changes the tone, and it is not saved. This replaces ADR-0060's
rule that the tone follows the system. In the light tone the logo is sage green (`#84A07C`); in the
dark tone it stays the selected-button accent. The light/dark toggle keeps the accent in both.

## Alternatives considered

- **Open dark but follow system changes afterwards.** Rejected: a tone that flips on its own after
  launch contradicts asking for dark at every launch.
- **Save the toggle.** Rejected for now: not asked for, and it would add a setting to persist.

## Consequences

Users of a light system appearance must toggle the tone each session. The logo no longer always
matches the selected-button accent, so it has its own style (`theme::logo`).
