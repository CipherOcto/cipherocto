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

## The larger half: quorum and set-hash are hollow on every path

The checks above are not unique to retirement. `verify_governance_suspension`
runs the identical three, in `store/memory.rs` and `store/stoolap.rs`:

| Check                            | On the suspension and slash path           |
| -------------------------------- | ------------------------------------------ |
| snapshot freshness               | same — the caller sets `finalized_at_unix` |
| `governance_set_hash` comparison | same — `snapshot` vs `auth`, both supplied |
| quorum ≥ `GOVERNANCE_QUORUM`     | same — `members.len()`                     |

Only the signature check differs, and on those paths it is real. So the
correct reading is narrower and more alarming than "retirement skips
verification":

- **Signature verification** is present on suspension and slash, absent on
  retirement. Those two paths are sound; retirement is not.
- **Quorum and set-hash verification are absent everywhere.** They are
  statements about the shape of caller-supplied data, and they are not load
  bearing on any path.

That matters because RFC-0968 presents the governance-set hash as the
mechanism that **closes the single-key-compromise attack**, "binding the
proof to a governance-set hash that itself requires quorum agreement to
mutate". Neither half of that sentence is implemented: there is no registry
lookup, and there is no agreement — `members` is a vector of bare 32-byte
values with no signature attached to any of them.

The practical consequence on the suspension and slash paths is bounded: a
caller still needs a valid governance key, so a single compromised officer
key can act alone and the quorum check does not make it harder. The check
reports `GovernanceQuorumNotMet` as though it were a real threshold, so an
operator reading the error would believe a quorum had been assessed.

## Why this is an outlier, not a missing subsystem

The same crate already verifies this class of record properly, on the paths
that suspend and slash a recorder:

- `auth::verify_governance_signature` is the single verification choke point.
- `store/memory.rs` and `store/stoolap.rs` both call it on the slash path,
  passing `&auth.governance_algorithm_id`.
- The `ReputationStore` trait documents `verify_governance_suspension` as
  validating "shape, freshness, quorum, and the signature", with signature
  verification delegated to that choke point.
- A test asserts both store sources read `&auth.governance_algorithm_id,`,
  because a store that assumed Ed25519 instead of reading the record would
  verify under the wrong key.

`declare_retirement_eligible` is the one governance entry point that does not
join that choke point.

## Why it is not a one-line fix

The verifier takes the algorithm as an argument, deliberately: the algorithm
travels **on the record**, so no call site can silently fall back to Ed25519.
`SuspensionAuth` carries `governance_algorithm_id`.

**`GovernanceProof` carries no algorithm field.** So retirement cannot call
`verify_governance_signature` as things stand — not because the plumbing is
missing, but because the record does not carry the one thing the verifier
needs. Two shapes are available and both are decisions, not edits:

1. Add a `governance_algorithm_id` to `GovernanceProof`, making it
   self-describing like `SuspensionAuth`. This changes a serialized record.
2. Retire through `SuspensionAuth` instead of `GovernanceProof`, unifying on
   the already-verified type and deleting the duplicate.

That is why the mission is deferred rather than implemented. Note also that
`GovernanceProof::signature` claimed "every store implementation delegates
to" the verifier. That was false and is corrected as part of this filing: it
is true of the suspension and slash paths, and false of retirement.

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
this filing. A fourth, on `GovernanceProof::signature`, claimed the
verifier is one "every store implementation delegates to" — it is corrected
in §Why it is not a one-line fix. What remains is the verification itself.

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
3. **Whether the suspension and slash paths may keep a hollow quorum check.**
   They are sound without it. Keeping a check named `GovernanceQuorumNotMet`
   that counts unattached bytes is worse than removing it, because it reports
   an authorisation assessment that did not happen.

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

- [ ] Retirement carries the algorithm it will be verified under — either
      `GovernanceProof` gains `governance_algorithm_id`, or retirement moves to
      `SuspensionAuth`. **Unblocked by nothing; this is the first decision.**
- [ ] `declare_retirement_eligible` verifies the proof signature through
      `verify_governance_signature` on both store implementations.
- [ ] A proof signed by the wrong key is refused, mutation-checked against the
      length-only check it replaces.
- [ ] The active governance set is resolved from a trust anchor rather than
      from the proof.
- [ ] Quorum counts distinct, verified signers.
- [ ] No doc claims a check the code does not perform. **Satisfied for the
      four overstatements found, and deliberately not guarded** — a guard
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
