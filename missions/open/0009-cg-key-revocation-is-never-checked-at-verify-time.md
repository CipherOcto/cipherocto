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

### A verifier now exists. It does not close this.

`ProductionCapabilityVerifier` landed in `quota-router-core` after this mission
was filed, and it is worth being precise about what that did and did not do,
because the two are easy to confuse.

**There are three distinct revocation concepts in this workspace, not two.**

| Concept                      | Where it lives                        | Enforced in production?                                |
| ---------------------------- | ------------------------------------- | ------------------------------------------------------ |
| Capability-holder revocation | `HolderRecord.revoked_at_millis_unix` | **yes** — `lookup_active` in `verify_with_resolve`     |
| Per-key revocation (AC#14)   | `VerificationMethod.revoked_at`       | **no** — `is_revoked()` still has no production caller |
| Identity-scoped revocation   | `octo-ident` registry                 | **no** — same gap, second half of AC#14                |

The verifier enforces the **first**. That is real progress against the class of
problem, and it is mutation-verified — `a_revoked_holder_is_refused` fails if the
active check is disabled, and only that test fails.

But AC#14 names the **second and third**. Those remain exactly as they were:
`is_revoked()` still has no production caller, and nothing in any verification
path reads `VerificationMethod.revoked_at`. Checked rather than assumed — the
verifier's source contains no reference to `is_revoked`, `VerificationMethod`, or
`revoked_at` at all.

So this mission stays open in full, and the one-line status change is: _a
production capability verifier exists, and it enforces the holder gate only._
Recording it as closed would be claiming a neighbour's fix.

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

- **A production verification path must exist first.** Measured while filing
  `0009-cj`: the revocation flag has no verifier to add it to. There is no
  production implementation of the ingress `CapabilityVerifier` trait, no
  production caller of `verify_with_resolve`, and no production caller of
  `CapabilityToken::redeem`. This mission's premise — that revocation is modelled
  but unchecked — is true and narrower than it first reads: nothing is checked,
  including the signature. See §The root cause under four of the five missions in
  mission `0009-ce`.
- Mission `0009-ce-a-type-alias-hides-the-same-pin` (recorded the finding)
- RFC-0009 AC#14
- RFC-0958 §Verification
