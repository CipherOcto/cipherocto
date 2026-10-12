---
name: 0958-a-verifier-revocation
description: RFC-0958. Implement the revocation contract on the ZK verify path: mandatory identity-scoped check, corrected asymmetric freshness window with validated policy relationships, RevocationSnapshot, and three new error variants.
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

`depends_on: RFC-0958`. No hard dependency on the RFC-0009 missions,
though per-key revocation is the other half of the composed control and is a
known open gap. **Its owner is settled in RFC-0009 A14 and is neither this
mission nor `0009-c`** — it needs a circuit change. See §Known gap, do not
paper over.

## Scope

Implement RFC-0958. Nothing here exists in the code yet:
`capability.rs` still holds the symmetric `abs_diff` check against
`MAX_SKEW_SECS` and has no revocation step at all.

1. **`RevocationSnapshot`** — `effective_at` + `revoked_identities`. Note there
   is deliberately **no `version` field**; §Data Structures explains why.
2. **`VerifierFreshnessPolicy`** — four fields, and the constructor validates
   **three** independent relationships per §Configuration Validation:
   - `max_age_secs`
   - `max_prover_future_skew_secs` — the **prover's** clock, an untrusted domain
   - `max_revocation_staleness_secs` — how old a snapshot may be
   - `max_snapshot_future_skew_secs` — the **first-party store's** clock, and
     **not** to be reused from the prover skew

   That last field is the one an implementer will skip. It exists because a
   shared tolerance silently weakens a control in the other trust domain: the
   first-party store clock is not attacker-controlled the way the prover's is,
   and sizing it by prover drift tolerance admits a snapshot from the future
   by an amount chosen to tolerate a hostile prover. §Verification step 5
   forbids exactly that substitution.

3. **The freshness checks** — `ProofTooOld`, `ClockSkewExceeded` against
   `max_prover_future_skew_secs`, and `RevocationSnapshotFuture` against
   `max_snapshot_future_skew_secs`.
4. **The revocation checks** — `RevocationStateStale`,
   `RevocationSnapshotFuture`, `IdentityRevoked`.
5. **Ordering** — the revocation lookup must happen after STWO verification,
   which requires a lazy query boundary rather than a borrowed parameter. See
   §Ordering; a `&RevocationSnapshot` parameter cannot deliver this.
6. **Signature change** — add `policy` and the lazy `revocation` query;
   **restore** `accepted_casm_blake3_hashes`, which v1.6's spec block had
   dropped.
7. **`zk-verifier`'s hard-coded `MAX_SKEW_SECS` check** must be parameterised
   or removed, or it silently overrides the policy. See §Ordering.

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

Correcting the comment is not sufficient on its own — it satisfies IA-13 on
paper while the path still takes its clock from a caller. The entry points
that must be traced and fixed are:

- `verify_capability_zk_token` and `verify_batch_capability_zk`, both of which
  take `verifier_local_unix_time` from a `CapabilityVerifier`
- `verify_one_from_json` in `quota-router-cli`, which takes a caller-supplied
  `now_unix`

Those are where a request-context clock enters. IA-13 is met when the
reference clock is verifier-local or verifier-minted at each of them, not when
the doc comment stops recommending the wrong one.

**`capability.rs` must not regress to a const CASM check.** The real signature
carries `accepted_casm_blake3_hashes: &[[u8; 32]]` for the R1 H13 N=2
rotation grace. The v1.6 spec block replaced it with a single
`COMPILED_CASM_BLAKE3_HASH` const, which would have quietly removed rotation
support. Keep the slice.

## Ordering: why revocation moves after STWO verify, and why a borrowed parameter cannot do it

The revocation read is the most expensive non-STARK operation in the function.
Running it ahead of STWO verification makes it reachable by an unauthenticated
caller submitting a garbage proof with a chosen `holder_did`. The "structural
checks first, they are cheap" rationale that orders steps 1-3 does not extend
to a store read.

**An earlier draft of this mission specified the fix as "move the lookup after
`stwo_plugin::verify`", which cannot work.** Two reasons, both verified against
the code:

1. **The real call site is not `stwo_plugin::verify`.** That symbol exists only
   in the RFC and in this mission; it is in no `crates/` file. The live path is
   `quota_router_core::zk_verify::capability::verify_capability_zk`, which
   delegates to `zk_verifier::verify_capability_zk`.
2. **A borrowed snapshot forces the read to happen at the call site.** The
   current signature is `verify_capability_zk(proof, expected_public_inputs,
accepted_casm_blake3_hashes, verifier_local_unix_time)`. Adding
   `revocation: &RevocationSnapshot` means the caller must materialise the
   snapshot _before_ the function is entered — so the store read executes
   before step 1, and no amount of reordering statements inside the body moves
   it behind STWO authentication. The caller can still force the read with a
   garbage proof.

So the ordering guarantee requires a **lazy query boundary**, not a parameter.
The shape is a caller-supplied query invoked inside the function, after the
STWO call succeeds:

```rust
pub fn verify_capability_zk(
    proof: &ProofBundle,
    expected_public_inputs: &PublicInputs,
    accepted_casm_blake3_hashes: &[[u8; 32]],
    verifier_local_unix_time: u64,
    policy: &VerifierFreshnessPolicy,
    revocation: &dyn Fn(&DID) -> Result<RevocationSnapshot, StoreError>,
) -> Result<(), ZkVerifyError>
```

The closure is not called until after `zk_verifier::verify_capability_zk`
returns `Ok`, so an unauthenticated caller cannot reach the store at all.

**There is no caller-side alternative on this path.** An earlier draft offered
one — have the _caller_ perform the lookup after its own STWO check, recording
the ordering as a convention rather than a guarantee. Dropped: the only vector
that can distinguish the two orderings,
`zk-verify-stwo-fail-with-revoked-did`, asserts that the revocation query was
**not called** on a STWO failure. With no query inside `verify_capability_zk`
there is nothing for that vector to observe, so the branch would ship the
ordering unproven while the file presented both options as acceptable. A lazy
query boundary is the only shape this mission accepts on the ZK verify path.

Either way the ordering needs a vector. See
`zk-verify-stwo-fail-with-revoked-did` below.

### A second hard-coded bound the mission did not name

`MAX_SKEW_SECS` exists in **two** places, and fixing only one silently
overrides the policy:

- `capability.rs` declares `pub const MAX_SKEW_SECS: u64 = zk_verifier::MAX_SKEW_SECS;`
  — a re-export — and checks it against `abs_diff` in
  `verify_capability_zk_structural`.
- `zk_verifier::verify_capability_zk` performs **its own** skew check and
  returns `zk_verifier::VerifyError::ClockSkewExceeded { skew, max }`, which
  `capability.rs` maps onto `ZkVerifyError::ClockSkewExceeded`.

Replacing the `capability.rs` check with policy-driven asymmetric bounds and
leaving `zk-verifier`'s hard-coded 300s in place means a deployment configuring
`max_age_secs` or `max_prover_future_skew_secs` above 300s has its proofs
rejected at 300s by a bound the policy never set. The error surfaces as
`ClockSkewExceeded` from a constant, which looks like a clock problem rather
than a configuration one. Parameterise or remove the `zk-verifier` check as
part of this mission; it is already in the §Validation crate list.

## Known gap, do not paper over

This mission delivers the **identity** gate only. RFC-0009 A14's per-key gate
is not on this path, because §Algorithms still resolves the DID inside the
circuit. A compromised key on a sound identity passes. That needs new public
inputs and therefore a circuit change, tracked separately.

**Ownership of that gate is settled in RFC-0009 A14 and it is neither this
mission's nor `0009-c`'s**, because both are verifier work. The circuit's
`BatchSigPublicInputs` carries only `signer_roots` and `message_root` — no
holder DID, no `algorithm_id`, no `key_id` — so a verifier has no bound
statement of which key signed and cannot gate on one. Closing it is a circuit
change that publishes those three as public inputs and constrains the signing
key to the method bearing that `key_id`; it moves the committed CASM digest
and is its own amendment and mission. Until then A14 is unmitigated on the ZK
path, recorded here rather than half-claimed by two missions.

`zk-verify-compromised-key.json` is a deliberately **failing** vector that
documents the hole. Keep it failing rather than deleting it, and do not paper
over it by widening the identity check.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors

RFC-0958 §Test Vectors names one of these. The rest are this mission's own
additions, and the ones that matter most — the order-independence unit test and
`zk-verify-stwo-fail-with-revoked-did` — are not in the RFC at all.

- `zk-verify-revoked-identity.json` → `IdentityRevoked`. Named by RFC-0009 A13
  as `revoked_identity_replay_rejected`, and **owned by this mission** —
  `0009-c` disclaims it, so nothing else obliges it to exist
- `zk-verify-stale-revocation.json` → `RevocationStateStale`
- `zk-verify-future-revocation.json` → `RevocationSnapshotFuture`, **not**
  `RevocationStateStale` and not a rendered `age 0s > 300s`
- `zk-verify-proof-too-old.json` → `ProofTooOld`
- `zk-verify-forward-dated.json` → `ClockSkewExceeded` — exists because
  front-dating is the direction that matters
- `zk-verify-backdated.json` → accepted within `max_age_secs`, `ProofTooOld`
  beyond it. Pins that the circuit's `current_unix_time <= before` does **not**
  catch back-dating
- `zk-verify-policy-rejects.json` → constructor `Err`. **Three distinct
  policies, three distinct vectors** — one JSON fixture encodes one policy, and
  §Configuration Validation requires all three rejections:
  - `zk-verify-policy-rejects-skew-ratio.json` —
    `max_prover_future_skew_secs * 10 > max_age_secs`
  - `zk-verify-policy-rejects-staleness-over-age.json` —
    `max_revocation_staleness_secs > max_age_secs`
  - `zk-verify-policy-rejects-zero-snapshot-skew.json` —
    `max_snapshot_future_skew_secs == 0`
- `zk-verify-stwo-fail-with-revoked-did.json` → `StwoVerifyError`, **not**
  `IdentityRevoked`, and the revocation query MUST NOT have been called. This
  is the only vector that can distinguish the two orderings, so it is the one
  that holds §Ordering honest
- `verify_is_order_independent` → not a fixture; a unit test. Two calls with
  identical inputs but differing prior-verification history return identical
  `Result`. This is what guards the `version` deletion: a monotonicity floor
  reintroduced alongside `RevocationSnapshot.version` would satisfy every
  fixture above and restore the exact R41 CRITICAL, because a check whose
  result depends on what the verifier saw before is not Class A deterministic
- `zk-verify-front-dated-at-max-age.json` → **accepted**. A proof front-dated
  by exactly `max_prover_future_skew_secs` and presented at age exactly
  `max_age_secs`. This is the composed boundary; the two bounds tested in
  isolation cannot distinguish a correct composition from a shared window
- `zk-verify-compromised-key.json` → **currently expected to ACCEPT.** Keep it
  as a documented failing vector

This list is **additive** to the existing corpus in
`crates/octo-wallet/tests/fixtures/capability-zk/`, which already holds
`zk-verify-public-input-mismatch.json`, `zk-verify-casm-drift.json`,
`zk-verify-stwo-fail.json`, `zk-verify-expired.json`, `zk-cross-impl-tv1.json`
and the mint vectors. Those are out of scope here, not superseded by this
list.

## Layer direction (per `docs/architecture/design-principles.md`)

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

**No command in the repository executes any of these acceptance vectors.** The
fixture corpus lives in `crates/octo-wallet/tests/fixtures/capability-zk/` and
**no test loads it today** — the only mention of the path anywhere under
`crates/` is a doc comment in `zk_vectors.rs`, and there is no `include_str!`
or path reference in any test. The harness does not exist yet. Every vector in
§Test Vectors is specified as a JSON fixture, so as written the acceptance
condition "a vector passing is the acceptance condition met" is unmeetable.

This mission therefore builds the harness as part of its scope:

```
crates/octo-wallet/tests/capability_zk_fixtures.rs
```

which loads `fixtures/capability-zk/*.json` and drives them through
`verify_capability_zk`. Acceptance then gates on:

```bash
cargo test -p octo-wallet --test capability_zk_fixtures
```

and the nine existing fixtures must pass through the same harness unchanged —
they are the regression baseline, not superseded by this list.

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
  per-key gate — **owner settled in RFC-0009 A14, neither this mission nor
  `0009-c`**; see §Known gap, do not paper over
- `crates/quota-router-core/src/zk_verify/capability.rs` — the file under change
- `missions/claimed/0009-c-verification-dispatch`

## Claimant

@unassigned

## Pull Request

#
