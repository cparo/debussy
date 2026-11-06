# ADR-0023 — Expired or royalty-free patents may be implemented; others need a licensing SDK or review

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Patent policy](../architecture/patents.md#patent-policy)

## Context

CLAUDE.md required a patent policy in the architecture and, until one existed, treated every codec
or algorithm with any patent history as exposed. That interim rule would block even algorithms
whose patents expired long ago (MP3, for example) and much of classic DSP.

## Decision

A codec or algorithm may be implemented in-house or used through a crate only if all covering
patents have expired, or its owners have declared it royalty-free, with that evidence in an ADR.
Otherwise it goes through a platform SDK that licenses it, or needs human approval through
`flag-risk` (tag `patent`).

## Alternatives considered

- **Platform SDK or review only** — keeps the strict interim rule; MP3 encoding and common DSP
  would need review despite expired patents.
- **Human review for every codec and named algorithm** — safest, but turns routine DSP work
  into review queues.

## Consequences

- Each in-house codec or patented-history algorithm needs an ADR citing the expiry or royalty-free
  declaration. The agent gathers the evidence; uncertain cases go to `flag-risk`.
- AudioToolbox remains the route for AAC ([ADR-0024](0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md)).
- This is not legal advice; doubtful cases still go to a human.
