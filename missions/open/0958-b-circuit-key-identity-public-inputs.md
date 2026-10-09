---
name: 0958-b-circuit-key-identity-public-inputs
description: RFC-0958 + RFC-0009 A14. Publish holder_did, algorithm_id and key_id as ZK public inputs, constrain the circuit to the verification method bearing that key_id, and let the verifier gate on the (holder_did, key_id) pair. Closes A14's per-key revocation gate on the ZK path. Moves the committed CASM digest.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-09T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0958
    - RFC-0009
    - 0009-b-algorithm-registry
---

# Mission `0958-b-circuit-key-identity-public-inputs` — OPEN 2026-10-09

## Status

Open (2026-10-09). Claimable once `0009-b-algorithm-registry` lands.

This is the **circuit-change mission RFC-0009 A14 was deferred to**. It is the
only route to closing the per-key revocation gate on the ZK path, and it is
deliberately a separate mission from both verifier-side work, because neither
verifier mission can deliver it.

## Dependencies

- `0009-b-algorithm-registry` — supplies `AlgorithmId` and
  `VerificationMethodId`, the two values this mission publishes
- `0958-a-verifier-revocation` — supplies the identity gate this mission's
  per-key gate runs _in front of_. Not a compile-time dependency, but the two
  compose and landing them out of order leaves a gate with nothing behind it

## Scope

Publish the signing key's identity as ZK public inputs, and constrain the
circuit so the public claim is true.

### Why a verifier cannot do this

`BatchSigPublicInputs` today carries `signer_roots` and `message_root`. No
holder DID, no `algorithm_id`, no `key_id`. A verifier therefore holds **no
cryptographically bound statement of which key signed**, and cannot check that
key's revocation even in principle. Adding a `revoked_at` lookup at the
verifier does nothing, because the verifier cannot tell which key to look up.

And the check cannot be moved inside the circuit either: revocation is mutable
state, and nothing mutable is reachable from inside a proof. That is the same
reason the identity gate is verifier-side.

So the circuit must **publish** the key identity, and the verifier must
**check** it. The binding between the two is what makes the gate sound.

### The three public inputs

```
BatchSigPublicInputs {
    signer_roots: Vec<[u8; 32]>,
    message_root: [u8; 32],
    holder_did:   [u8; 32],   // the resolved DID's canonical hash
    algorithm_id: [u8; 16],   // the algorithm the signing key verifies under
    key_id:       [u8; 16],   // the VerificationMethodId of that key
}
```

All three fixed-width per RFC-0009 §Domain separation. `algorithm_id` and
`key_id` are the raw 16-byte forms the signable prefix already names, so the
circuit's claim and the signed domain are the same bytes — not a re-encoding of
them.

### The constraint that makes it sound

Publishing is worthless without constraining. The circuit MUST assert that the
key whose signature it verified is **the DID method bearing `key_id`, and that
method's `algorithm_id` is `algorithm_id`**. Without that, a prover holding a
revoked key names a sound key's `key_id` in the public inputs and the
verifier gates on the wrong one — the exact silent-wrong-answer shape A12
describes, one layer up.

**The tempting shortcut is to drop the constraint and keep the publication.**
It compiles, every existing vector passes, and the gate is decorative. If the
constraint is hard to write, the honest outcome is to land the publication and
the verifier check together, never one without the other.

### What the verifier then does

Before the identity gate, and after STWO verification:

1. `key_id` is not revoked in the holder's DID document
2. `algorithm_id` matches the registered algorithm for that key
3. only then the identity-scoped check RFC-0958 §Revocation and freshness
   already performs

Same fail-closed rule: an unreadable document or an unknown `key_id` refuses,
it does not pass.

## Acceptance Criteria

Verified by the §Test Vectors list. Each vector is a named, independently
runnable check; a vector passing is the acceptance condition met.

## Test Vectors

- `public_inputs_carry_holder_did_algorithm_id_and_key_id` — the three fields
  exist and are fixed-width
- `circuit_rejects_a_key_id_that_names_a_different_method` — **the one that
  matters most.** A proof whose public `key_id` names a sound method while the
  signature was made with another MUST be rejected. This is the vector that
  distinguishes a real gate from a decorative one, and it must fail if the
  constraint above is dropped
- `compromised_key_revoked_without_identity_revocation` — RFC-0009 A14's
  oracle, on the ZK path. It is a **currently failing** vector today; this
  mission is what makes it pass
- `revoked_identity_still_rejected` — the identity gate keeps working behind
  the new per-key gate
- `sound_key_with_sound_identity_still_verifies` — no regression on the happy
  path
- `casm_hash_moves_and_is_recorded` — the committed digest changes, and the
  new value is written into the accepted-hash list, not left to drift

## Environment note

Proving runs cost roughly 440-505s each, so **background every measurement**.
`stwo-sys` is excluded from the workspace and must be built from
`crates/zk-vendor/stwo-sys` on its pinned nightly, or clippy fails with E0554
on the `#![feature]` lines. Its prover stack is pinned at the call site via
`std::thread::Builder`, so `RUST_MIN_STACK` need not be set.

## CASM budget

The live ceiling is 3,300 words / 110 KB, asserted in
`crates/zk-circuit/tests/casm_snapshot.rs`. The chain re-derivation baseline is
3,004 words, so the headroom is roughly 296 words. **Three new public inputs
and a lookup constraint must fit inside that**, and if they do not, the answer
is an explicit ceiling amendment — not trimming the chain assertion, which is
load-bearing.

## Layer direction (per `cipherocto-design-principles`)

- `zk-circuit` (Layer A) — the constraint and the public-input layout
- `zk-verifier` (Layer B) — the per-key gate in front of the identity gate
- `octo-ident` (Layer B) — supplies the id types; not modified here

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p zk-circuit -p zk-verifier --all-targets --all-features -- -D warnings
cargo test -p zk-circuit --lib
cargo test -p zk-verifier --lib
```

Proving vectors must be backgrounded. Do not run the full workspace suite for
this mission.

## Backward compat

None, and none is needed — nothing is deployed. What _does_ change is the
committed CASM digest: every proof minted against the old digest stops
verifying once the new digest is the only accepted one. Both values stay in the
accepted-hash list through the migration grace, and **the grace window is a
deployment decision, not a constant in this mission**. Dropping the old hash
early invalidates live proofs; keeping it forever means two circuit versions
verify, which is the state a migration is supposed to pass through rather than
end in.

## Known gaps, recorded not hidden

- **The in-circuit DID resolution stays.** This mission publishes the key
  identity; it does not move DID resolution to the verifier. That is a larger
  change and is not required for A14.
- **`ZkSupport::Circuit` is still `None` for every algorithm**, including
  STARK-Schnorr (`0009-d`). Publishing the inputs does not create a constrained
  verifier for any algorithm. AC#14's mint gate stays shut until a circuit
  actually verifies, and claiming otherwise is the fail-open that mission
  `0009-d`'s review caught.
- **Two keys, two claims.** An identity holding two keys produces two proofs
  with different `key_id`s. The gate is per-proof, so revoking one key does not
  stop the other — which is A14's oracle, but it also means revocation is only
  as fast as the store read that backs it.

## Cross-references

- `rfcs/accepted/proof-systems/0958-zk-capability-subclass.md` —
  §Algorithms, §Revocation and freshness, §Adversary Analysis A6, IA-12,
  §Test Vectors
- `rfcs/accepted/process/0009-identity-evolution-v12.md` — A14, AC#13,
  AC#14, §Domain separation, §Dual-format signature algorithms and the
  ZK-capable key
- `crates/zk-circuit/src/lib.rs` — `BatchSigPublicInputs`, the struct this
  mission extends
- `crates/zk-circuit/tests/casm_snapshot.rs` — the live CASM ceiling
- `missions/claimed/0009-b-algorithm-registry` — the id types published here
- `missions/open/0958-a-verifier-revocation` — the identity gate this runs
  behind

## Claimant

@unassigned

## Pull Request

#
