---
name: 0009-cg-key-revocation-is-never-checked-at-verify-time
description: RFC-0009 AC#14 requires a verifier to perform both revocation gates and fail closed on an unavailable or stale snapshot. VerificationMethod::revoked_at exists and is_revoked reads it, but no production path calls is_revoked. The enforcement AC#14 asks for does not exist, so its two named gates have nothing to assert against.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-ce-a-type-alias-hides-the-same-pin
---

# Mission `0009-cg-key-revocation-is-never-checked-at-verify-time`

## Status

Open. Filed from measured evidence, not from a failing test.

## The finding

AC#14 requires that:

> A verifier MUST perform both revocation gates — key-level
> (`VerificationMethod.revoked_at`) and identity-scoped — fail closed on an
> unavailable or stale snapshot.

Measured across the workspace:

| Claim                                   | State                                |
| --------------------------------------- | ------------------------------------ |
| `VerificationMethod::revoked_at`        | exists, initialised to `None`        |
| `VerificationMethod::is_revoked()`      | exists, reads that field             |
| Production call sites of `is_revoked()` | **none — every call site is a test** |

Every other `revoked_at` in the tree belongs to a different subsystem:
`revoked_at_unix` in the runtime session-revocation store and in the policy
registry. Neither has anything to do with key revocation.

So the revocation **flag** is modelled and readable, and the revocation **gate**
AC#14 asks a verifier to perform does not exist.

## Why this is filed rather than closed

This is not a missing test for a missing feature. The data model exists and looks
right. What is absent is the enforcement — which is the part AC#14 is about.

The two named gates, `revoked_identity_replay_rejected` and
`stale_revocation_snapshot_is_rejected`, are consequently **unwritable as
specified**: there is nothing to assert against. The two available moves are to
invent the verifier — inventing the thing under test — or to assert only that
`is_revoked()` returns true after setting `revoked_at`, which tests the data
model rather than the criterion and would read as evidence for a gate that was
never built.

## Related, deliberately not folded in

AC#14's third gate, `compromised_key_revoked_without_identity_revocation`, is
named by the RFC **precisely so the gap stays visible** — it states the check is
"not yet enforced on the ZK path" because RFC-0958 still resolves the DID
in-circuit. That is an honest declared gap, not drift, and this mission does not
claim it.

## Acceptance Criteria

- [ ] A verifier path exists that performs the key-level revocation gate before accepting a capability or holder signature.
- [ ] The identity-scoped gate exists, per AC#14's second half.
- [ ] A stale revocation snapshot is refused rather than treated as "not revoked".
- [ ] An unavailable snapshot is refused rather than treated as "not revoked".
- [ ] `revoked_identity_replay_rejected` exists and is mutation-verified: revoking the identity turns it red.
- [ ] `stale_revocation_snapshot_is_rejected` exists and is mutation-verified: serving a stale snapshot turns it red.

## What must not happen

A test asserting only that setting `revoked_at` makes `is_revoked()` return
true. That exercises the accessor and nothing else, and it would make AC#14 read
as covered.

## Dependencies

- Mission `0009-ce-a-type-alias-hides-the-same-pin` (recorded the finding)
- RFC-0009 AC#14
- RFC-0958 §Verification
