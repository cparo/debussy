---
name: new-adr
description: Record a design decision as an ADR and add its result to the architecture docs. Use when a design decision is made or ratified, a dependency buy/build choice is settled, or a flag-risk question is answered.
---

# New ADR

An ADR records *why*; the architecture section it links records the resulting *rule*. CLAUDE.md
("Documentation Boundaries") says which belongs where. Finish every step: a record with only half
its parts is worse than none.

0. **Branch from the latest `main`.** ADR numbers are assigned per branch, so two open ADR PRs
   collide. If one is still open, stack on its branch instead.
1. **Name who decided.** A human's name, `agent, ratified by <name>` once a human has approved
   it, or `agent, unratified`. The answer to a `flag-risk` question is the human's decision.
2. **Find the section.** Pick the `docs/architecture/<file>.md` heading the decision's rule belongs
   under. If none fits, add the heading now (a new file per subsystem), stating only the rule.
3. **Create the record:**
   `cargo xtask new-adr "<the decision, as a declarative sentence>" --decided-by "<who>" --section docs/architecture/<file>.md#<anchor> [--issue <n>]`
   It numbers the record, fills the header, and adds the index row.
4. **Write the body.** Replace every template prompt: Context (what forced the choice), Decision
   (the rule), Alternatives considered (each with why it lost), Consequences (costs, what it
   forecloses, follow-up issue numbers).
5. **State the result in the architecture section**: the rule only, no reasoning, ending with a
   link `([ADR-NNNN](../adrs/NNNN-slug.md))`.
6. **Reversing an earlier decision?** Set the old record's Status to `Superseded by ADR-NNNN` and
   update its index row; never edit its decision.
7. **Verify:** `cargo xtask generate-architecture-index`, then `cargo xtask check` passes.

Done when `cargo xtask check` passes and the record, index row, and architecture rule all exist.
