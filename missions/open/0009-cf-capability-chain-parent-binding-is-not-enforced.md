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

Partially delivered. The binding itself now exists; the enforcement path it was
to be wired into does not, and did not.

Filed from measured evidence, not from a failing test. Amended 2026-10-12 after
implementation exposed a scoping error in the original criteria.

## Amendment (2026-10-12): the wiring criterion was vacuous

The third acceptance criterion below reads:

> Every path that verifies a `CapabilityTokenV2` consults `verify_chain_parent`
> before accepting the token.

Measured before writing any code, that criterion had **zero paths to satisfy**.

The capability verification production performs is `verify_capability` in
`octo-wallet`'s capability module. It works on the V1 macaroon substrate and
contains no reference to `CapabilityBundleV2` or `token_v2` — measured, both
terms return zero matches. `CapabilityBundleV2` appears in exactly three
non-test places: the wallet mint handler, the capability issuer handler, and
`octo-cap-zk`'s inert proof-bundle field. **All three emit it. None consumes
it for a trust decision.**

So an implementer could tick that box by doing nothing at all, and a reviewer
checking it would find nothing to check. That is the same trap the
must-not-happen section below was written to prevent — it just arrived one level
up, in an acceptance criterion rather than in a test.

The chain parent binding is one field inside a token type that no verifier
reads. Fixing the binding does not make it enforced; it makes it _enforceable_.

## What has landed

`octo-cap-macaroon` now has a `chain_parent` module implementing
`compute_chain_parent` and `verify_chain_parent` exactly as RFC-0009 specifies,
including the one-byte depth encoding the RFC explicitly warns against
confusing with the four-byte information-string form.

The derivation carries a byte-exact vector that was cross-checked against a
BLAKE3 implementation outside this crate, so it pins the formula rather than
merely recording whatever the code emitted. Four mutations were verified to turn
it red:

| Mutation                        | Tests that caught it                          |
| ------------------------------- | --------------------------------------------- |
| 4-byte depth instead of 1-byte  | 3 (binding length, pinned digest, 1-byte pin) |
| parent/child order inverted     | 2 (binding layout, pinned digest)             |
| depth cap check removed         | 1 (`depth_past_the_cap_is_refused`)           |
| predicate made to always accept | 3 (forgery gate, commitment, zero-value)      |

The last one is the important one: it is the mutation that turns this into a
suite that passes while measuring nothing, and three tests catch it.

### Why the AC#11 guard does not flag this new code, verified rather than assumed

`verify_chain_parent` takes two `[u8; 32]` parameters and its name starts with
`verify`, so it is exactly the shape the AC#11 guards were built to catch. It is
in a crate those guards scan (`crates/octo-cap-macaroon/src` is in
`AC11_DISPATCHED`), and it is not flagged.

That was confirmed by planting a genuine violation inside the new module: a
`verify_dispatch` call taking `public_key: &[u8; 32]` in the key's own
argument position. The guard went red and named the file and the binding. So the
scan covers this file, and the silence is a real result rather than a file the
guard never read.

The substantive reason it is out of scope: the guard keys on
`verify_dispatch(`, because AC#11 is about sites that _choose_ a verification
algorithm. `verify_chain_parent` chooses none. It hashes two capability root
digests, and a BLAKE3 digest is 32 bytes for every algorithm that exists and
every algorithm that will. Widening those parameters would not enable a second
algorithm; it would only destroy the self-delimiting property that lets the
three fields concatenate without length framing.

The parameters are named `*_cap_root` rather than the RFC's `*_cap_key` because
that is what they are — `HolderRecord::cap_root_hash`, a digest, not a public
key. The RFC's `CapabilityKey` type does not exist in this workspace.

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

Reordered after implementation. The first two are done; the third was found to
be vacuous and has been replaced with the criterion that is actually blocking.

- [x] `compute_chain_parent(parent_cap_key, child_cap_key, child_depth)` exists and returns the BLAKE3-256 binding the field documents.
- [x] `verify_chain_parent` recomputes that binding and rejects a mismatch. Mutation-verified against a predicate that always accepts.
- [ ] **A V2 verification path exists.** This is the real blocker and it was missing from the original criteria. The capability verifier must gain a route by which a `CapabilityBundleV2` can be presented and decided on — or the V2 bundle must be retired as a wire form. Until one of those happens, no wiring criterion below is checkable.
- [ ] That path consults `verify_chain_parent` before accepting a token with `chain_depth > 0`, and treats depth 0 as a root rather than running the binding.
- [ ] A missing parent in the catalog is refused rather than treated as a root token. (`check_wrapped_chain`'s revocation walk is out of scope here; it needs `0009-cg`.)
- [ ] A non-root token can actually be minted, so the check has a subject.
- [x] `chain_parent_forgery_breaks_verification` — the gate A10 names — exists and is mutation-verified: forging a parent value turns it red.

## What must not happen

A test that constructs a token with a wrong `chain_parent` and asserts only that
the value round-trips. That reads as coverage of A10 while measuring nothing,
and the property under test would not exist in the code being tested.

The same trap exists one level up, and it has already been sprung once: an
acceptance criterion phrased as "every path that verifies X does Y" reads as
coverage while being satisfiable by an empty set. **Do not restore the wiring
criterion above without first establishing that the path exists.** A criterion
over an empty population is not a gate; it is a comment with a checkbox.

## Dependencies

- Mission `0009-ce-a-type-alias-hides-the-same-pin` (recorded the finding)
- RFC-0009 §Threat model A10
- RFC-0871 capability chain material
