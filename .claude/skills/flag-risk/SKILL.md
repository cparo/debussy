---
name: flag-risk
description: Stop and put a risky decision to a human before acting. Use when about to do anything CLAUDE.md "When to Stop and Ask a Human" lists, such as a Cargo.lock change, patent-exposed codec, security trade-off, personal data, platform code outside its boundary, or persisted-format change.
---

# Flag a risk

The risky action waits until a human answers. Work that does not depend on the answer may go on.

1. **Halt the action.** Leave `Cargo.lock`, formats, security settings and the rest untouched until
   the answer comes back.
2. **Tag it** with the matching tag from CLAUDE.md "When to Stop and Ask a Human": `license`,
   `patent`, `security`, `privacy`, `portability`, `product`, or `stakeholder`. Several may apply.
3. **Write the brief.** Each item in a sentence or two:
   - **Action:** what you were about to do, and where.
   - **Trigger:** which rule it hits.
   - **Options:** at least two, including not doing it. For a dependency, the crate *and* building
     it, with the default for that area from CLAUDE.md "Dependency Policy" marked, plus licence,
     transitive dependency count, and maintenance state.
   - **Recommendation:** your pick and the trade-off it accepts.
4. **Ask.**
   - A human is in the session: put the brief to them with `AskUserQuestion`, the recommended
     option first.
   - No human is present: open a GitHub issue titled `[risk:<tag>] <action>` with the brief as its
     body and the `ready-for-human` label (see `docs/agents/issue-tracker.md`), report its number,
     and stop the risky work.
5. **Record the answer** with the `new-adr` skill, `--decided-by` the human who answered, linking
   the issue if there is one. A rejected option still goes in the record's alternatives.

Done when a human has answered and the ADR exists, or when the issue is open and the risky work
has stopped.
