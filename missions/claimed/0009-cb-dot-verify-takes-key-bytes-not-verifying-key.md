---
name: 0009-cb-dot-verify-takes-key-bytes-not-verifying-key
description: RFC-0009 AC#11 follow-on. The 25 dot/ verify signatures take an ed25519_dalek VerifyingKey, which fixes the algorithm in the type and makes the dispatched algorithm unreachable for anything but Ed25519.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c8-dot-transport-envelope-algorithm-dispatch
---

# Mission `0009-cb-dot-verify-takes-key-bytes-not-verifying-key` — OPEN 2026-10-10

## Status

Open 2026-10-10. Found by re-reading `0009-c8`'s own output rather than its
test results.

## The defect

`0009-c8` gave 25 `dot/` verifiers this shape:

```rust
pub fn verify(
    &self,
    algorithm: &octo_ident::AlgorithmId,
    witness_pubkey: &VerifyingKey, // ed25519_dalek::VerifyingKey
) -> Result<(), DotError> {
    // ...
    octo_algo_ed25519::verify_dispatch(
        algorithm,
        witness_pubkey.as_bytes(), // converted straight back to bytes
        &self.assertion_hash,
        &self.signature,
    )
}
```

The key goes in as a type and comes out as the bytes that type was hiding. The
round trip is dead ceremony, and it is the whole problem: **a caller holding an
ML-DSA-768 key cannot call this function.** There is no way to construct the
argument. `algorithm` can therefore only ever be Ed25519, no matter how
carefully the body dispatches.

## Why the tests did not catch it

They passed, and they passed honestly. The suite has three vectors per site:

- the happy path — satisfied
- a bad signature is refused — satisfied
- an unregistered algorithm fails closed — satisfied

The third one is satisfied by _any_ code that calls `verify_dispatch`, including
a body that ignores its argument entirely, which is exactly what the mutation
proved by hardcoding Ed25519 and turning that one vector red.

So the vectors test dispatch, and dispatch is present. What they cannot test is
whether any algorithm other than Ed25519 could ever arrive — because that is a
property of the _signature_, and nothing in the suite constructs a non-Ed25519
key and tries to call through.

This is the first gap in this arc where the mutation methodology reached its
limit. Everywhere else, "would this test notice a fake dispatch" was the
question. Here the honest answer is no, and the reason is structural.

## The fix

`witness_pubkey: &VerifyingKey` becomes `witness_pubkey: &[u8; 32]`, and the
`.as_bytes()` disappears with it. `verify_dispatch` already takes
`public_key: &[u8]`, so this is the shape the dispatcher actually wants, and
the type stops pre-deciding what the algorithm parameter decides.

Consequences, all of them the point of the change:

- `dot/dc.rs` no longer builds a `VerifyingKey` from the wire `witness_id`, so
  the last pending count in the crate drops from 1 to 0 and
  `octo-network` leaves `AC11_PENDING` entirely.
- The four transport adapters stop building a `VerifyingKey` to hand to an
  envelope verifier.
- The four of them that name Ed25519 explicitly as a stated assumption keep
  doing so, but now as the _only_ honest description of what the code does,
  rather than a workaround for a signature that demanded the key type.

## Scope

25 `verify` methods across the six `dot/` files, their in-crate call sites, and
the four adapters. Mechanical in the same shape as `0009-c8`, and it must be
done to all 25 or none of them — converting only the sites that are currently
blocked would leave two incompatible conventions for the same operation, which
is the parallel-abstraction failure this project treats as a hard no.

## Test Vectors

The three existing vectors per site carry over unchanged. They are necessary and
not sufficient, and this mission should say so rather than pretend otherwise.

The vector that would actually cover this is a new one, and it should be
written first:

- `a_non_ed25519_algorithm_reaches_the_verifier` — assert that some second
  algorithm can be passed through the public `verify` and is dispatched rather
  than rejected at the type boundary. If no second algorithm is registered in
  the test registry, the assertion is that the call **compiles and reaches
  `verify_dispatch`**, not that it succeeds — a compile-level vector, which is
  what was missing.

Recording this in the mission rather than pretending three green vectors prove
the signature is right.

## Acceptance Criteria

- All six `dot/` files at a pending count of zero, and `octo-network` absent
  from `AC11_PENDING`.
- `octo-network` present in `AC11_DISPATCHED`, which puts the whole crate under
  the inline-rebuild scan for the first time.
- The compile-level vector exists and is red before the change, green after.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network -p octo-adapter-p2p -p octo-adapter-wechat -p octo-adapter-udp -p octo-adapter-lora --all-targets --all-features -- -D warnings
cargo test -p octo-network --lib
cargo test -p octo-cli --test security_regressions
```

## Backward compat

None required. Nothing is deployed. The key type change is a public API change
with in-workspace callers only.

## Cross-references

- `missions/claimed/0009-c8-dot-transport-envelope-algorithm-dispatch` — the
  change this corrects
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_PENDING`,
  `AC11_DISPATCHED`

## Claimant

@unassigned

## Pull Request

#
