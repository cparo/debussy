---
name: patent-research
description: Gather and record patent evidence for an algorithm before implementing it in-house, as docs/architecture/patents.md "Patent policy" requires. Use when a ticket plans a new in-house algorithm, codec, or DSP technique.
---

# Patent research

Evidence comes in two stages with a human between them: candidates are cheap to list and most
turn out lapsed, so claims are read only for patents still in force. The implementation waits
until step 5 is done.

1. **Describe the design.** Write the planned technique element by element: each step,
   constant and option, as it will be built. Pick the note path
   `docs/research/<topic>-patents.md`.
2. **List candidates.** Dispatch the `patent-candidates` agent with the design and the note path.
   Done when the note's **Candidates** table is complete apart from **Status**.
3. **Status check by a human.** Show the candidates (number, assignee, element it touches, link)
   and ask the human to check each one's status in Google Patents or USPTO Patent Center. Record
   the answers in the **Status** column, with who checked and the date. Done when every row has a
   status.
4. **Read claims.** If any candidate is **In force**, dispatch the `patent-claims` agent with the
   note path. Done when each in-force candidate has a verdict. With none in force, skip to step 5.
5. **Decide and record.**
   - Every in-force candidate **Does not read**, or none is in force: record the evidence with
     `new-adr`, naming the human who checked statuses, and list the algorithm in
     `docs/architecture/patents.md` "Patent policy".
   - Any **Reads** or **Unclear**: raise it with `flag-risk` (tag `patent`). Options include
     designing around the claim, waiting for expiry, a platform SDK, or accepting a prior-art
     position. Then record the answer with `new-adr`.
