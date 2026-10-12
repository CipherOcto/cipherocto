---
name: 0968-b-governance-proof-verification
description: Retirement authorisation runs through stub_verify_proof_shape, which performs no cryptographic verification. The set-hash check is self-consistency between two caller-supplied fields, the quorum check counts entries in a caller-supplied Vec without checking distinctness, and the signature check is a byte length on bytes that are never parsed. The same struct is fully verified on the suspension and slash paths through verify_governance_signature, so the retirement path is the outlier rather than an unimplemented subsystem. RFC-0968 also requires the governance set hash to be checked against GovernanceRegistry::lookup_at_snapshot, which does not exist in the tree.
metadata:
  node_type: mission
  type: specification
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0968
---

# Mission `0968-b-governance-proof-verification`

> **DEFERRED — the set-hash half needs a governance registry that does not exist.**
>
> The signature half does not need a decision: the verifier already exists and
> the suspension and slash paths already call it. What is deferred is the
> narrower question of where the active governance set comes from.

## Status

Deferred (2026-10-12) — pending a decision on whether the active governance
set is looked up from chain state or carried by the caller.

## The gap

`stub_verify_proof_shape` is the only governance check on the retirement
path. Both stores call it and nothing else, in `declare_retirement_eligible`
(`store/memory.rs` and `store/stoolap.rs`). Measured, not assumed.

| Check                                        | What it actually does                                   | Forgeable? |
| -------------------------------------------- | ------------------------------------------------------- | ---------- |
| snapshot freshness                           | caller sets `finalized_at_unix`                         | yes        |
| `governance_set_hash` equals the claim       | compares two fields the caller supplies                 | yes        |
| quorum ≥ `GOVERNANCE_QUORUM`                 | `self.members.len()` — no distinctness, no verification | yes        |
| signature ≥ `MIN_RETIREMENT_SIGNATURE_BYTES` | `proof.signature.len()`, bytes never parsed             | yes        |

`quorum_count` is `self.members.len() as u32` in `auth.rs`
§GovernanceSnapshot. The module doc claimed "≥ 3 **distinct** members". It
does not check distinctness, so `[x; 3]` satisfies it.

A caller who can reach this function can therefore obtain
`RetirementEligibility { eligible: true }` without holding a governance key:
a fresh timestamp, a set hash matching itself, three members, and 96 bytes of
anything.

## Why this is an outlier, not a missing subsystem

The same crate already verifies this struct properly, on the paths that
suspend and slash a recorder:

- `auth::verify_governance_signature` is the single verification choke point.
- `store/memory.rs` and `store/stoolap.rs` both call it on the slash path.
- The `ReputationStore` trait documents `verify_governance_suspension` as
  validating "shape, freshness, quorum, and the signature", with signature
  verification delegated to that choke point.

`declare_retirement_eligible` is the one governance entry point that does not.
This is what makes it worth filing rather than treating as a stub: the
capability to verify is present, in the same file, two methods away. Retirement
is an omission from an established pattern, not a gap waiting on new
machinery.

## What RFC-0968 additionally requires, and what is absent

`GovernanceProof` carries `governance_set_hash`, and RFC-0968 states the
implementation **MUST** verify that hash against
`GovernanceRegistry::lookup_at_snapshot`'s active-set digest, presenting that
lookup as the fix for the single-key-compromise attack.

No `GovernanceRegistry` and no `lookup_at_snapshot` exist anywhere in the
tree. The implementation compares the hash against the proof's own claim
instead, which is the check the registry lookup exists to replace. This half
genuinely needs a decision; the signature half does not.

Note that the signature field is `signature: Vec<u8>`, not the `[u8; 64]`
RFC-0968 originally specified. That widening was deliberate (mission 0009-cd,
so the record names its algorithm rather than silently deciding it) and is
**not** part of this filing.

## A shape inconsistency worth recording separately

`MIN_RETIREMENT_SIGNATURE_BYTES = 96`, documented as "3 × 32-byte sigs, the
conventional compact ed25519-packing for a 3-sig governance proof". But
`GovernanceProof` carries a **single** `signature: Vec<u8>`, not three
signers, and single-signature algorithms in this tree do not all produce 96
bytes. The threshold and the struct describe different proof shapes. Once the
signature is actually parsed this resolves one way or the other, so it is
noted here rather than fixed separately.

## Why the deferral label is not sufficient on its own

The deferral is honest and correctly cited — the docs say real signature
verification is deferred, and `store/memory.rs` says so at the call site. The
problem is narrower: **the docs also described the stub as checking more than
it does**, in three places.

- "a fresh governance snapshot with quorum ≥ `GOVERNANCE_QUORUM` and a
  **non-trivial signature set**" — the signature set is never parsed.
- "quorum (≥ 3 **distinct** members)" — distinctness is never checked.
- The set-hash check reads as verification and is self-consistency.

An error variant named `ReputationError::GovernanceQuorumNotMet`, next to a
field called `governance_set_hash`, on a struct called `GovernanceProof`, is
enough that a reader concludes a signature was checked. That conclusion is
false, and it is only visible by reading the body.

Those three claims are corrected in the module and function docs as part of
this filing. What remains is the verification itself.

## What still has to be decided

1. **Where the active governance set comes from.** RFC-0968 assumes
   `GovernanceRegistry::lookup_at_snapshot`. If that registry is chain state,
   retirement needs a lookup and a trust anchor. If the set travels with the
   proof, the single-key compromise the hash was meant to close reopens.
2. **Whether quorum means distinct verified signers.** RFC-0955 models
   `GovernanceSigner { pubkey, signature }` per signer; this implementation
   models bare `[u8; 32]` members with no signature attached. The anchor path
   has `AnchorGovernanceSigner` with both fields, so one of the two shapes is
   wrong and the anchor path is the better-founded one.

## Scope

### In Scope

- A decision on the source of the active governance set.
- Retirement delegating to `verify_governance_signature`, as the suspension
  and slash paths already do.
- A quorum definition that counts distinct, verified signers.
- Resolving `MIN_RETIREMENT_SIGNATURE_BYTES` against the actual signature
  shape once signatures are parsed.
- A mutation-checked test: a proof signed by the wrong key must be refused,
  and restoring the length-only check must turn that test red.

### Out of Scope

- `validate_evidence` and the parity and bucket rules, a separate gate.
- The Stoolap backend's unimplemented dispatch, which already errors.
- Suspension and slashing authorisation, which already verify.

## Acceptance Criteria

- [ ] `declare_retirement_eligible` verifies the proof signature through
      `verify_governance_signature` on both store implementations.
- [ ] A proof signed by the wrong key is refused, mutation-checked against the
      length-only check it replaces.
- [ ] The active governance set is resolved from a trust anchor rather than
      from the proof.
- [ ] Quorum counts distinct, verified signers.
- [ ] No doc claims a check the code does not perform. **Satisfied for the
      three overstatements found, and deliberately not guarded** — a guard
      over a fixed list of already-corrected phrases is a census, not a check.

## What must not happen

**Do not close this by renaming.** `stub_verify_proof_shape` is honestly named
today and stays that way until it verifies something. Renaming it to something
authoritative would make the gap harder to find, not closed.

**Do not treat "the fields are all populated" as authorization.** Every
current check is a statement about the shape of caller-supplied data. That
category of check cannot become authorization by being tightened; only
verification against something the caller does not control can do that.

**Do not build a second verifier.** `verify_governance_signature` is the
choke point the trait names. Retirement should join it, not grow its own.
