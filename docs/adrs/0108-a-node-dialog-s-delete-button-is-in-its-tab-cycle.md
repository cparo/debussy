# ADR-0108 — A node dialog's Delete button is in its Tab cycle and only its confirmation deletes a node

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-13
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

Trying the node dialog after ADR-0107, the developer asked for three changes to how a node is
deleted from the keyboard:

- The delete confirmation held every focus key and Space or Enter only cancelled it, so Delete
  could not be reached without the mouse.
- The patch board's Delete or Backspace removed a selected node at once. That bypassed the
  confirmation the dialog's "Delete" button asks for, and duplicated the button.
- Tab in a mixer's or effect's dialog cycled its controls but never reached the "Delete" button.

## Decision

- A node dialog's "Delete" button is the last stop in its Tab cycle, after a mixer's or effect's
  controls; a dialog with no other controls (a bus, a feedback delay) has only it. Space or Enter
  on it opens the confirmation, as a press does.
- In the confirmation, Tab and Shift-Tab move between "Cancel" and "Delete", the focused one lit;
  Space or Enter presses it. "Cancel" is focused when it opens. The arrows do nothing; Escape or a
  press outside still cancels.
- The confirmation's state and focus belong to the node dialog module with the rest of the
  dialog's focus, replacing ADR-0107's guard in `App` that swallowed the focus keys.
- Delete or Backspace on the patch board removes a selected wire only. A selected node is
  deleted only through its dialog's confirmation.

## Alternatives considered

- **Keep the confirmation's state in `App`, with a second focus field.** The focus rule of
  ADR-0107 is that the dialog module owns the dialog's keyboard focus; a second owner would split
  it again and need the `App` fixture to test.
- **Ring the focused confirmation button, as dialog controls are.** The confirmation already lit
  its default to show what Enter does; lighting whichever is focused keeps that one meaning.
- **Let Delete on a selected node open the confirmation.** It would still duplicate the dialog's
  button, which the developer asked to remove.

## Consequences

- Deleting a node from the keyboard is Tab to "Delete", Space, Tab, Space; no key deletes one at
  once.
- Every node dialog now answers Tab, so leaving a feedback delay's time field with Tab takes the
  text focus from it.
