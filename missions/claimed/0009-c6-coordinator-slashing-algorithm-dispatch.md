---
name: 0009-c6-coordinator-slashing-algorithm-dispatch
description: RFC-0009 AC#11, octo-coordinator-types slice. Dispatch slash-proof adjudication through AlgorithmRegistry instead of hardcoding Ed25519.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-10T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - 0009-b-algorithm-registry
    - 0009-c-verification-dispatch
    - 0009-c5-vault-cache-bus-algorithm-dispatch
---

# Mission `0009-c6-coordinator-slashing-algorithm-dispatch` — CLAIMED 2026-10-10

## Status

**Claimed and implemented.** 57 lib vectors and all integration targets pass;
clippy clean at `-D warnings` with `--all-targets --all-features`.

## Scope

`crates/octo-coordinator-types/src/slashing.rs` §`verify_slash_proof`.

One production site. It matched a governance key by adjudicator id, built an
`ed25519_dalek::Signature`, and verified the proof's signature against it.

## Why the stated blocker was false

`AC11_PENDING` recorded this site as blocked on the RFC-0010 DID codec, on the
same grounds as the `octo-network` set: a key recovered by parsing a DID into
bare 32 bytes carries no algorithm.

**That is not the path here.** `GovernanceVerifyingKeys` is supplied by the
caller as `&[([u8; 32], VerifyingKey)]`, and `verify_slash_proof` has no caller
outside its own tests. No DID is parsed anywhere between the caller and the
verification.

This is the second time a pending blocker did not hold, and both times the
cause was the same: the blocker was inherited from a neighbouring site rather
than derived from this one. `octo-vault` was first.

## What changed

`GovernanceVerifyingKeys` is now `&[([u8; 32], AlgorithmId, VerifyingKey)]`, and
verification dispatches through `verify_dispatch` on the entry's own algorithm.
`ed25519_governance_keys` builds the Ed25519-only form for callers that
genuinely have nothing else — a convenience, **not** a fallback.

`adjudicator_signature` stays `[u8; 64]` on the wire, so no serialisation or
compat surface changed.

The dalek `Signature` is gone from the verification path. That is the part that
matters: building one before dispatching would assert Ed25519 before the
registry had a say, and an unregistered algorithm would surface as a malformed
signature rather than as an unknown one.

## Test Vectors

- `tv_sl_ac11_dispatch_on_the_declared_algorithm_verifies` — the happy path
  with the algorithm stated on the entry. **Control:** this alone cannot
  distinguish dispatch from a hardcoded Ed25519.
- `tv_sl_ac11_refuses_an_unregistered_algorithm` — the fail-closed half, and
  what makes the pair meaningful.
- `tv_sl_ac11_still_refuses_a_bad_signature` — dispatch did not degrade the
  check into a lookup.

**Mutation-verified, and the result is worth stating plainly:** replacing the
recorded algorithm with a hardcoded Ed25519 identifier turns exactly one vector
red — the unregistered-algorithm control — and leaves the other two green.

So on this crate, as on `octo-vault`, the happy path and the bad-signature check
are both satisfied by a verifier that ignores the algorithm entirely. Neither is
evidence that dispatch works. Only the fail-closed vector can tell the
difference, which is why it is not optional.

## Layer direction

`octo-coordinator-types` is Layer B and takes the same two edges `octo-wallet`,
`octo-reputation` and now `octo-vault` take: `octo-ident` for `AlgorithmId`, and
`octo-algo-ed25519` for the dispatch helper. Both are recorded in the manifest
with the layer justification, matching `octo-wallet`'s existing wording.

The arrow that must never point back is E → B internals. A Layer B consumer
depending on a Layer E algorithm crate is the ordinary direction.

## Blast radius

One type alias, one verification function, and the fixtures that build the key
set — nine sites across the lib tests and `canonical_slashing_blobs`. No caller
outside this crate to migrate.

## Validation

```bash
cargo fmt --all -- --check
cargo clippy -p octo-coordinator-types --all-targets --all-features -- -D warnings
cargo test -p octo-coordinator-types
```

## Backward compat

None required. Nothing is deployed.

## Cross-references

- `rfcs/accepted/process/0009-identity-evolution-v12.md` — AC#11
- `missions/claimed/0009-c5-vault-cache-bus-algorithm-dispatch` — the first
  instance of the same false blocker, and the pattern followed here
- `missions/archived/completed/0009-c3-network-verification-dispatch` — where these sites were
  found, and whose blockers still hold
- `crates/octo-cli/tests/security_regressions.rs` — `AC11_PENDING`

## Claimant

@unassigned

## Pull Request

#
