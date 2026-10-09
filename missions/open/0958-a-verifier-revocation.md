---
name: 0958-a-verifier-revocation
description: RFC-0958 v1.7. Implement the revocation contract on the ZK verify path: mandatory identity-scoped check, corrected asymmetric freshness window with validated policy relationships, RevocationSnapshot, and three new error variants.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0958
---

# Mission `0958-a-verifier-revocation` — OPEN 2026-10-09

## Status

Open (2026-10-09). Claimable — no claimant assigned.

## Dependencies

`depends_on: RFC-0958`. No hard dependency on the RFC-0009 v1.3 missions,
though per-key revocation (`0009-c`) is the other half of the composed control
and is a known open gap until both land.

## Scope

Implement RFC-0958 v1.7. Nothing here exists in the code yet:
`capability.rs` still holds the symmetric `abs_diff` check against
`MAX_SKEW_SECS` and has no revocation step at all.

1. **`RevocationSnapshot`** — `effective_at` + `revoked_identities`. Note there
   is deliberately **no `version` field**; §Data Structures explains why.
2. **`VerifierFreshnessPolicy`** with constructor validation per
   §Configuration Validation.
3. **The freshness checks** — `ProofTooOld`, `ClockSkewExceeded` against
   `max_prover_future_skew_secs`.
4. **The revocation checks** — `RevocationStateStale`,
   `RevocationSnapshotFuture`, `IdentityRevoked`.
5. **Reordering** — the revocation lookup moves after `stwo_plugin::verify`.
6. **Signature change** — add `revocation` and `policy`; **restore**
   `accepted_casm_blake3_hashes`, which v1.6's spec block had dropped.

## The arithmetic. Get this right; it was wrong twice

**Proof lifetime is `max_age_secs + max_prover_future_skew_secs`.** Not
`max_age_secs`. The prover chooses `current_unix_time` and the circuit only
asserts it is below the capability's expiry, so it may be set to anything up
to that. Acceptance at time `T` requires
`c ∈ [T - max_age, T + future]`, and survival runs to `T = c + max_age`,
maximised by maximising `c`.

So **front-dating** buys the extra lifetime. **Back-dating** shortens a proof's
life to zero. An earlier draft of this RFC had that backwards, and the
consequence was that the "asymmetric" pair delivered no reduction at all when
its two values were equal — while appearing to.

The asymmetry only buys anything when the forward tolerance is far smaller than
the past one. That is why the constructor rejects
`max_prover_future_skew_secs * 10 > max_age_secs` instead of merely documenting
it.

**Revocation response is `max_age_secs + max_revocation_staleness_secs`.** A
revocation committed at `R` can still be invisible at
`R + max_age_secs + max_revocation_staleness_secs`. A stale snapshot makes
revocation _slow_; it does not make replay unbounded, because the proof window
still bounds replay.

Size the policy against those sums. Cutting only `max_age_secs` leaves the
response time floored by the staleness bound.

## Two live-code defects this mission must correct

**`CapabilityVerifier::at_time`'s doc comment names the wrong clock source.** It
says production callers pass "typically the slot's issuance time from the
request context". Request context is attacker-influenceable, and it is the
reference for every freshness bound. IA-13 requires a verifier-local monotonic
clock or a verifier-minted token, never request context. Correct the comment
along with the code, or the next implementer will follow it.

**`capability.rs` must not regress to a const CASM check.** The real signature
carries `accepted_casm_blake3_hashes: &[[u8; 32]]` for the R1 H13 N=2
rotation grace. The v1.6 spec block replaced it with a single
`COMPILED_CASM_BLAKE3_HASH` const, which would have quietly removed rotation
support. Keep the slice.

## Ordering: why revocation moves after STWO verify

The revocation read is the most expensive non-STARK operation in the function.
Running it ahead of STWO verification makes it reachable by an unauthenticated
caller submitting a garbage proof with a chosen `holder_did`. The "structural
checks first, they are cheap" rationale that orders steps 1-3 does not extend
to a store read.

## Known gap, do not paper over

This mission delivers the **identity** gate only. RFC-0009 A14's per-key gate
is not on this path, because §Algorithms still resolves the DID inside the
circuit. A compromised key on a sound identity passes. That needs new public
inputs and therefore a circuit change, tracked separately.

`zk-verify-compromised-key.json` is a deliberately **failing** vector that
documents the hole. Keep it failing rather than deleting it, and do not paper
over it by widening the identity check.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors (per RFC-0958 v1.7 §Test Vectors)

- `zk-verify-revoked-identity.json` → `IdentityRevoked`
- `zk-verify-stale-revocation.json` → `RevocationStateStale`
- `zk-verify-future-revocation.json` → `RevocationSnapshotFuture`, **not**
  `RevocationStateStale` and not a rendered `age 0s > 300s`
- `zk-verify-proof-too-old.json` → `ProofTooOld`
- `zk-verify-forward-dated.json` → `ClockSkewExceeded` — exists because
  front-dating is the direction that matters
- `zk-verify-backdated.json` → accepted within `max_age_secs`, `ProofTooOld`
  beyond it. Pins that the circuit's `current_unix_time <= before` does **not**
  catch back-dating
- `zk-verify-policy-rejects.json` → constructor `Err` for a policy violating the
  §Configuration Validation relationships
- `zk-verify-compromised-key.json` → **currently expected to ACCEPT.** Keep it
  as a documented failing vector

## Layer direction (per `cipherocto-design-principles`)

- `quota-router-core` (Layer C) — the verifier
- The revocation store is reached through a typed query boundary, per
  §Principle 12: storage is not a protocol. This RFC specifies the obligation,
  not the store, and the store must be swappable without changing the contract.

The lookup MUST be an indexed query, not an unbounded materialisation of the
revoked set. `revoked_identities` is the result of a lookup for one
`holder_did`. A `Vec::contains` over the whole registry has no stated
relationship to the G2 sub-100ms target.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p quota-router-core -p zk-verifier --all-targets --all-features -- -D warnings
cargo test -p quota-router-core --lib
```

## Backward compat

No shim, by design. An older verifier cannot be configured into this behaviour
because the missing arguments **are** the revocation state and the policy.

No new public input and no circuit change, so the CASM hash is unchanged and
existing proofs stay cryptographically valid — which means an older verifier
will still accept them, and that is precisely the gap A6 describes.

## Cross-references

- `rfcs/accepted/proof-systems/0958-zk-capability-subclass.md` — §Verification,
  §Revocation and freshness, §Configuration Validation, §Time Bounds, IA-12,
  IA-13, IA-14, A6, §Test Vectors
- `rfcs/accepted/process/0009-identity-evolution-v12.md` — A13, A14, AC#14; the
  per-key gate that lands in `0009-c`
- `crates/quota-router-core/src/zk_verify/capability.rs` — the file under change
- `missions/open/0009-c-verification-dispatch`

## Claimant

@unassigned

## Pull Request

#
