---
name: 0009-cf-capability-chain-parent-binding-is-not-enforced
description: RFC-0009 threat model A10. CapabilityTokenV2::chain_parent is documented as a BLAKE3 binding computed by compute_chain_parent and checked by verify_chain_parent. Neither function exists, nothing reads the field to make a decision, and every production mint writes the zero value. The documented defence is absent rather than merely untested.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-ce-a-type-alias-hides-the-same-pin
---

# Mission `0009-cf-capability-chain-parent-binding-is-not-enforced`

## Status

Open. Filed from measured evidence, not from a failing test.

## The finding

RFC-0009 §Threat model A10 specifies the defence plainly:

> `verify_chain_parent` checks `blake3(parent || child || depth) == chain_parent`;
> forgery requires inverting BLAKE3.

Measured across the workspace:

| Claim                                | State                               |
| ------------------------------------ | ----------------------------------- |
| `compute_chain_parent`               | does not exist                      |
| `verify_chain_parent`                | does not exist                      |
| `CapabilityTokenV2::chain_parent`    | documented as carrying that binding |
| Any code reading the field to decide | **none**                            |

Every occurrence of the field is an assignment, a `Debug` print, or a test
asserting the value round-trips.

**Every production mint writes `chain_depth: 0` and `chain_parent: [0u8; 32]`.**
The single non-zero depth in the tree is a fixture in `octo-cap-zk`'s tests. So
hierarchical capability chaining is not implemented, and the parent binding is
inert.

## Why this is filed rather than closed

The field documentation asserted a cryptographic binding that no code performs.
That has been corrected — the field now says it is reserved, names the missing
functions, and states that every mint writes the zero value. Correcting the
documentation does not create the defence.

**The risk is latent, not live.** There is no chain to forge today, precisely
because the feature is unused: the attack surface is empty for the same reason
the defence is missing. What exists now is a trap for whoever adds chained
issuance — the field looks bound, and would not be.

## Acceptance Criteria

- [ ] `compute_chain_parent(parent_cap_key, child_cap_key, child_depth)` exists and returns the BLAKE3-256 binding the field documents.
- [ ] `verify_chain_parent` recomputes that binding and rejects a mismatch.
- [ ] Every path that verifies a `CapabilityTokenV2` consults `verify_chain_parent` before accepting the token.
- [ ] A non-root token can actually be minted, so the check has a subject.
- [ ] `chain_parent_forgery_breaks_verification` — the gate A10 names — exists and is mutation-verified: forging a parent value turns it red.
- [ ] A missing parent in the catalog is refused rather than treated as a root token.

## What must not happen

A test that constructs a token with a wrong `chain_parent` and asserts only that
the value round-trips. That reads as coverage of A10 while measuring nothing,
and the property under test would not exist in the code being tested.

## Dependencies

- Mission `0009-ce-a-type-alias-hides-the-same-pin` (recorded the finding)
- RFC-0009 §Threat model A10
- RFC-0871 capability chain material
