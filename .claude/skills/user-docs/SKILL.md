---
name: user-docs
description: Write or edit the user guide in docs/user/. Use when adding or changing a user-facing feature's docs or rewriting a section of docs/user/.
---

# Write the user guide

`docs/user/` tells people how to use Debussy. It describes what they see and hear, never how it is
built.

1. **Read the code, not your memory.** Every range and default comes from the component's
   `ParameterInfo::new(name, min, max, default)` lines in `crates/audio_effects/src/*.rs` or
   `crates/instruments/src/*.rs` (follow constants such as `MAX_DELAY_MS` to their values). Read
   the UI in `crates/debussy_ui/src/` for what the user sees: `effect_panel.rs` `label` turns a
   parameter name into its label (group prefix stripped, first letter capitalised), and groups
   become headings. `stopped_only()` and `sets_latency()` mean the control changes only while
   stopped or paused; `shown_while` means it appears only for another control's value.
2. **Give each component a heading** (effect, instrument, dialog) when it has two or more
   controls, then one or two sentences of prose: what it does and when to reach for it. Name its
   presets there. Sibling headings for like items (effects, instruments, dialogs) go in
   alphabetical order, as the app's menus list them; group unlike items under their own parent
   heading (**Effects**, **Instruments**) rather than interleaving them. Sections that follow a
   workflow (setting up, then using) keep that order.
3. **Choose the layout by the controls' shape.**
   - **Table** when every control has the same facts: a range, a default and an effect. Columns
     are always **Control | Range | Default | What it does**. The unit goes with the number
     (`20 Hz – 20 kHz`), never its own column; a choice lists its options (`room, plate`); a
     toggle reads `on / off`.
   - **Bullets** when they do not share a shape: a choice that reveals other controls, a picker,
     a button, a mode. One bullet per control; sub-bullets for its options or the controls it
     reveals, with range and default inline.
   - **Mixed:** a table for the uniform controls, then bullets for the rest. A component built of
     sections (a synth's oscillators, filter, envelopes) may be bullets with a small table under
     each.
4. **Keep notes out of cells.** Something that applies across rows (presets, what the picture
   shows) goes in a short paragraph or list after the table. Mark a stopped-only control with
   `*` after its name in the Control cell, and explain the mark once as a footnote at the
   bottom of the page, after a `---` rule (`\* Can only be changed while stopped or paused.`),
   as print would; never earlier in the page.
5. **Describe a shared control once** (chorus and flanger share Rate, Sync, Shape and Spread) and
   link to it from the others, giving only the defaults that differ.
6. **Keep the voice.** Plain second person, short sentences, **bold** for every UI label exactly
   as the user sees it, no internals (no type names, no DSP terms the panel does not show).
7. **Stay in budget.** A file under `docs/user/` may have 300 lines
   (`crates/xtask/src/budget.rs`). When one would pass that, move a topic to
   `docs/user/<topic>.md` and leave a short paragraph and a link in its place; never raise the
   limit. Sweep for links to a moved heading.
8. **Check.** Run `cargo xtask check`. Then compare each table value with its `ParameterInfo::new`
   line once more, and ask the user to look over the rendered page on GitHub.

## Skeletons

A table, for uniform controls:

```markdown
## Reverb

Adds a sense of space.

| Control | Range | Default | What it does |
|---|---|---|---|
| **Algorithm*** | room, plate | room | **Room** is dense and natural, **plate** smooth and bright. |
| **Decay** | 0.2 – 20 s | 2 s | How long the reverb takes to die away. |
```

Bullets, for controls that differ:

```markdown
## Delay

Repeats the sound. Presets: Slapback, Ping-pong and Dub echo.

- **Time** — set one of two ways by **Sync**:
  - **Sync** off: **Time**, 1 – 4000 ms (default 375 ms).
  - **Sync** on: **Note**, 1/32 – 1/1 (default 1/8), following the project tempo.
- **Feedback**, 0 – 100 % (default 35 %) — how much of each repeat comes back again.
  - **Saturation** (default off) lets loud repeats distort gently instead of growing.
```

Done when every control the component shows is listed with its range and default from the code,
`cargo xtask check` passes, and the user has been asked to review the rendered page.
