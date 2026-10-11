---
name: 0009-ci-zk-attestation-never-binds-capability-chain-state
description: ProofBundle::capability_v2 was documented as feeding chain_depth, chain_parent and issuer_did into the ZK circuit public inputs "so the verifier checks hierarchical attenuation chain state". PublicInputs has none of the three fields, no circuit source mentions any of them, and the field is None at every production construction site. A proof was documented as attesting to something it does not attest to.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0009
    - RFC-0958
    - 0009-cf-capability-chain-parent-binding-is-not-enforced
---

# Mission `0009-ci-zk-attestation-never-binds-capability-chain-state`

## Status

Open. Filed from measured evidence, not from a failing test.

## The finding

`ProofBundle::capability_v2` carried a claim that no code implemented:

> When `Some`, the ZK circuit public inputs include the bundle's `chain_depth` +
> `chain_parent` (BLAKE3 binding) + `issuer_did` so the verifier checks
> hierarchical attenuation chain state.

Measured across the workspace:

| Claimed                                          | Actual                                                                               |
| ------------------------------------------------ | ------------------------------------------------------------------------------------ |
| public inputs carry `chain_depth`/`chain_parent` | `PublicInputs` has neither field                                                     |
| public inputs carry `issuer_did`                 | `PublicInputs` has neither field                                                     |
| the circuit consumes the bundle                  | no `zk-circuit`, `zk-verifier` or `zk-vendor` source mentions any of the three names |
| a verifier checks the chain                      | no such code path exists                                                             |
| the field is populated in production             | every production construction site writes `None`                                     |

`PublicInputs` carries `ask_id`, `axes_consumed`, `cap_root_hash`,
`invocation_hash`, `holder_did`, `current_unix_time`, `output_hash` and
`provider_slot_id`. None of the three claimed names appear.

The only `Some(...)` for the field is the serde round-trip test, which sets it
and reads it back.

### Why this is worse than the other four findings in this arc

The first four false claims in this arc were claims about _convenience_ — that a
field was bound, that a check happened. This one is a **soundness claim**. It
says a zero-knowledge proof attests to hierarchical attenuation chain state. A
verifier reading this doc would conclude the chain is proven. It is not proven,
not checked, and not provable from what the circuit is given.

In a proof system, "the public inputs carry X" is the entire content of the
claim. There is no weaker reading.

### The near-miss that probably caused it

`cairo/src/lib.cairo` defines `CHAIN_DEPTH = 3`. That is the **caveat** chain
depth — macaroon caveat attenuation, asserted via `CaveatChainDepthMismatch` —
and is unrelated to `CapabilityTokenV2::chain_depth`. Grepping `chain_depth`
across the circuit returns that constant and nothing else.

A reviewer checking the claim would grep, find a hit, and conclude it held. The
grep result is real and the conclusion is wrong, because the two `chain_depth`
concepts are different things that share a name.

## Why this is filed rather than closed

The field documentation and the test's comment have been corrected. Both now
state that the field binds nothing and that the test measures wire compatibility
only. Correcting the documentation does not bind the bundle to anything.

**The risk is latent, not live.** There is no proof being relied upon today: the
field is `None` everywhere it is constructed in production, so no verifier is
currently accepting a chain-state claim. What exists is a trap for whoever turns
on the field expecting the documented behaviour — and, worse, for whoever trusts
a proof on the strength of this doc entry.

## Acceptance Criteria

- [ ] `PublicInputs` carries the chain state the claim named — `chain_depth`, `chain_parent`, `issuer_did` — or the field's doc is restated to match whatever the circuit actually consumes. **These are mutually exclusive; pick one and make it true.**
- [ ] The Cairo circuit constrains the chain values it is given, so a proof cannot be produced over one bundle's chain state and verified against another's.
- [ ] A producer that sets `capability_v2: Some(..)` reaches a verifier that actually reads it, rather than a sink that discards it.
- [ ] `chain_parent_forgery_changes_the_public_inputs` — mutation-verified: altering the parent value turns it red.
- [ ] The two `chain_depth` concepts cannot be confused again: the Cairo caveat constant and `CapabilityTokenV2::chain_depth` are distinguishable by name at every use site.

## What must not happen

Do not satisfy the first criterion by adding the three names to `PublicInputs`
without constraining them in the circuit. A public input the circuit ignores is
the same defect wearing a different hat: it would make the grep pass this time,
which is exactly what made the original claim survive.

Do not treat the serde round-trip test as evidence for any criterion here. It
measures wire compatibility and will continue to pass if the bundle is dropped on
the floor at the prover boundary. It constructs a `chain_depth: 1` child token
with an arbitrary `[0xCC; 32]` parent — a shape that reads as chain coverage and
measures nothing.

Do not close this by deleting the field. The field is the only place a V2 bundle
can ride along with a proof; removing it would discard the intended migration path
along with the false claim about it.

## Dependencies

- Mission `0009-cf-capability-chain-parent-binding-is-not-enforced` — the parent
  binding itself is not computed or verified anywhere, so this mission's chain
  public inputs would bind to an unverified value until that one lands.
- Mission `0009-cf` must land first or together; binding an unbound value into a
  proof would move the defect rather than fix it.
- RFC-0958 §Data Structures (proof bundle shape)
- RFC-0009 §Threat model A10 (the attenuation chain being claimed)
