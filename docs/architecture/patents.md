# Patents

## Patent policy

A codec or algorithm may be implemented in-house, or used through a crate, only when every patent
covering it has expired or its owners have declared it royalty-free, with that evidence recorded
in an ADR. Otherwise it must be used through a platform SDK that licenses it, or approved by a
human through the `flag-risk` skill (tag `patent`) ([ADR-0023](../adrs/0023-expired-or-royalty-free-patents-may-be-implemented.md)).

The algorithms implemented in-house under this policy, each with its evidence: TPDF dither
([ADR-0025](../adrs/0025-wav-is-built-in-house-and-integer-exports-are.md)), and the xorshift64*
noise source seeded by SplitMix64 ([ADR-0035](../adrs/0035-the-dither-noise-source-is-xorshift64-seeded-by.md)).
