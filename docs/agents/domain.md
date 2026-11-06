# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the
codebase. This repo is **single-context**. `CLAUDE.md` ("Documentation Boundaries") says what each
document is for; this file only says when to read them.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root, for domain vocabulary.
- **`docs/adrs/`**: read ADRs that touch the area you're about to work in. `docs/adrs/README.md`
  indexes them by architecture section.
- **`docs/architecture/`**: start at `README.md` (a generated table of contents) and read the
  section file for the subsystem you're touching.
- **`docs/product/`**: the product brief, when a question turns on who the product is for or what
  is out of scope.

If any of these files don't exist, **proceed silently**. Don't flag their absence; don't suggest
creating them upfront. The `/domain-modeling` skill creates `GLOSSARY.md` lazily when terms
actually get resolved. To record a decision, use the project's `new-adr` skill; don't write the
ADR file by hand.

## File structure

```
/
├── GLOSSARY.md
└── docs/
    ├── adrs/
    │   ├── README.md          ← index
    │   └── NNNN-slug.md
    ├── architecture/
    │   ├── README.md          ← generated table of contents
    │   └── <subsystem>.md
    └── product/
        └── README.md
```

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor proposal, a hypothesis, a
test name), use the term as defined in `GLOSSARY.md`. Don't drift to synonyms the glossary
explicitly avoids.

If the concept you need isn't in the glossary yet, that's a signal: either you're inventing
language the project doesn't use (reconsider) or there's a real gap (note it for
`/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding:

> _Contradicts ADR-0007 (lock-free handoff to the audio thread), but worth reopening because…_

ADRs are append-only: reopening one means a new record that supersedes it, never an edit.
