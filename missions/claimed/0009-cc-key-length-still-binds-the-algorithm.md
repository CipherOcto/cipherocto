---
name: 0009-cc-key-length-still-binds-the-algorithm
description: RFC-0009 AC#11 follow-on. Every dispatched verification site takes &[u8; 32], and stark-schnorr public keys are 33 bytes, so no site can accept the second algorithm. The one cross-algorithm test that appears to cover this is vacuous.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-cb-dot-verify-takes-key-bytes-not-verifying-key
---

# Mission `0009-cc-key-length-still-binds-the-algorithm` — OPEN 2026-10-10

## Status

Open 2026-10-10. Found by testing the premise `0009-cb` rested on rather than
the premise itself.

## `0009-cb` fixed the type and left the length

`0009-cb` changed 25 `dot/` verifiers from `key: &VerifyingKey` to
`key: &[u8; 32]`, on the reasoning that a caller holding a non-Ed25519 key could
not construct an `ed25519_dalek::VerifyingKey`, so the type had pre-decided the
algorithm before the dispatcher got a vote.

That reasoning was sound and the fix was incomplete. The array is still sized
for Ed25519:

- `octo-algo-stark-schnorr`'s `StarkSchnorrAlgorithm::PUBLIC_KEY_LEN` is
  **33**.
- Ed25519's is 32.

So a real stark-schnorr key cannot be passed to any of the 25 sites. The
dispatch parameter is accepted and used; the key type still decides in advance
which algorithm could possibly apply.

`&[u8; 32]` is a **workspace-wide convention, not something `0009-cb`
introduced**. `dot/envelope.rs` already used it before any of this work, and
`0009-c7`, `0009-c9` and `0009-ca` followed it. Measured: **79** key parameters
across `octo-network` (18), `octo-runtime` (3), `octo-reputation` (2),
`octo-wallet` (1) and `octo-ident` (1).

## The test that appears to cover this is vacuous

`crates/octo-wallet/tests/lifecycle_replay.rs` contains
`a_revocation_proof_replayed_as_the_other_algorithm_is_rejected`, whose doc
comment says the prefix exists so a caller cannot replay one algorithm's proof
as the other's. It is the only place in the workspace that names
`stark_schnorr_algorithm_id()` outside the algorithm crate itself, and it
therefore looks like the cross-algorithm coverage the whole arc was missing.

**It is not.** Proved by mutation: hardcoding the algorithm inside
`IdentityKey::verify_lifecycle_proof` — replacing the `algorithm_id` argument
to `verify_dispatch` with a literal `ed25519_algorithm_id()` — leaves all three
tests in that file **green**, including the cross-algorithm one.

The reason is the same shape as every vacuous guard this arc has found. The
test passes an Ed25519 key (`IdentityKey::public_key_bytes() -> [u8; 32]`) and
an Ed25519 proof, named as stark-schnorr. Both a dispatching verifier and a
hardcoded Ed25519 one refuse that, because the signature genuinely is not a
stark-schnorr signature. Rejection is guaranteed by the input, so it carries no
information about whether the algorithm argument was honoured.

The one vector in the workspace that appears to prove a second algorithm is
reachable proves nothing about it.

## What a real vector needs

Both halves, and neither exists yet:

1. **A stark-schnorr keypair and signature.** `octo-algo-stark-schnorr` has
   51 passing tests including reference vectors, so the material can be made.
2. **A verification site that accepts a 33-byte key.** Which is the work above.

With those, the vector is a round trip: sign under stark-schnorr, verify
through the dispatched site, expect `Ok`. Mutation then flips it, because a
hardcoded Ed25519 verifier cannot produce `Ok` from a 33-byte key and a
stark-schnorr signature.

That is the same shape as `dot_envelope_a_second_algorithm_reaches_the_verifier`
from `0009-cb`, except that one could only reach an _unregistered_ algorithm.
This one has to reach a **working** one.

## The change

`&[u8; 32]` becomes `&[u8]` at verification sites, matching
`verify_dispatch`'s own signature — which has taken `public_key: &[u8]` since
the registry landed.

The length check is not lost, it moves to the layer that owns it. Each
algorithm registration already does this explicitly and distinguishes it from a
bad signature:

```rust
if public_key.len() != Self::PUBLIC_KEY_LEN {
    return Err(SignatureError::BadKeyLength { alg, got, expected });
}
```

That is strictly better than a compile-time length, because it reports _which_
algorithm wanted _what_ length, and it keeps working when a third algorithm
lands with a different size.

## Scope

79 parameters across 5 crates, plus their call sites. This is a cross-cutting
convention change and should be done as one piece, not per crate — a repo with
two key-length conventions is the parallel-abstraction failure the design
principles treat as a hard no.

## Why this was not folded into `0009-cb`

`0009-cb` verified its own claim, and its claim was true as stated: it was
about the _type_. Discovering that a second layer existed required measuring
stark-schnorr's key length, which is a different question and a different
mission. Recording it rather than silently widening `0009-cb`'s scope is the
honest split — `0009-cb`'s mission file should not claim to have fixed
something it did not.

## Acceptance Criteria

- No verification site takes a fixed-size key array.
- `crates/octo-wallet/tests/lifecycle_replay.rs` gains a round-trip vector that
  goes **red** under the hardcoded-algorithm mutation.
- A cross-algorithm replay vector that is red under the same mutation.
- `octo-algo-stark-schnorr` unchanged — it is already correct.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-network -p octo-wallet -p octo-runtime -p octo-reputation --all-targets --all-features -- -D warnings
cargo test -p octo-wallet --test lifecycle_replay
cargo test -p octo-cli --test security_regressions
```

## Backward compat

None required. Nothing is deployed.

## Cross-references

- `missions/claimed/0009-cb-dot-verify-takes-key-bytes-not-verifying-key` — the
  change this extends, and whose claim was narrower than it looked
- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11

## Claimant

@unassigned

## Pull Request

#
