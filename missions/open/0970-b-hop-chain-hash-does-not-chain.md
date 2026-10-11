---
name: 0970-b-hop-chain-hash-does-not-chain
description: HopEnvelope.chain_hash is BLAKE3(hop_secret), a per-envelope identifier, and wrap_for_hop takes no parent envelope, so no hop's hash is an input to any later hop's. verify_chain_hash compares only chain.last().chain_hash against a caller-supplied value, so RFC-0970 goal G4 — tampering with any hop's wrap invalidates the chain hash — is not enforced. A caller passing the expectation from chain.last() gets x == x.
metadata:
  node_type: mission
  type: implementation
  created: 2026-10-12T00:00:00.000Z
  v: "1.0"
  depends_on:
    - RFC-0970
    - 0970-a1-hop-crypto-and-replay-defense
---

# Mission `0970-b-hop-chain-hash-does-not-chain`

## Status

Open. Filed from a gate sweep of RFC-0970.

## The finding

RFC-0970 §Design Goals states:

> **G4: Chain hash integrity** — Tampering with any hop's wrap invalidates the
> chain hash. Test: TV8.

Measured against `crates/octo-wallet/src/capability/hop_envelope.rs`:

| Question                                      | Answer                                                                                                                                                                     |
| --------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| What is `chain_hash`?                         | `let chain_hash = hop_envelope_id`, and `hop_envelope_id = BLAKE3(hop_secret)` — a **per-envelope identifier** derived from the hop's own secret                           |
| Does `wrap_for_hop` take the parent envelope? | **No.** Its parameters are `(inner, hop_secret, wrapping_signing_key, ttl_millis_unix, wrapping_node_did, next_hop_did, node_epoch)`. There is nothing to accumulate from. |
| Is any hop's hash an input to a later hop's?  | **No.** Nothing downstream of hop _i_ reads hop _i_'s `chain_hash`.                                                                                                        |
| What does `verify_chain_hash` do?             | Compares `chain.last().chain_hash` against a caller-supplied `expected_chain_hash`. It does not walk the chain.                                                            |

So the field is named for a chain property it does not implement, and the
function named to verify it cannot verify it. This is the same shape as
`CapabilityTokenV2::chain_parent` — a chain-linked name on a per-item value.

**The trivially-passing consequence:** a caller that takes `expected` from
`chain.last()` — which is what `verify_chain_hash_matches_last_envelope` does —
is comparing `x == x`. That test passes unconditionally and would keep passing if
the comparison were deleted.

## What the existing tests actually cover

A name-based sweep first suggested G4/G5/G7 were untested. That was **wrong**, and
worth recording: `hop_envelope.rs` carries 20 tests, and TTL, audience, chain-hash
and redaction are all covered under descriptive names (`unwrap_ttl_exceeded`,
`unwrap_audience_mismatch`, `verify_chain_hash_mismatch`,
`hop_envelope_debug_redacts`). Coverage exists. What is missing is that
`chain_hash` chains.

The one genuinely uncovered vector was **TV2 (Three-Hop Chain)** — the deepest
chain exercised was two hops, and G3 names a 3-hop chain as its latency
scenario. Two hops cannot distinguish "the chain hash commits to depth" from
"the chain hash is the last envelope's id", because both hold at n=2.

## What landed alongside this finding

- `tv2_three_hop_chain_verifies` — the missing vector.
- `distinct_hops_carry_distinct_chain_hashes` — **deliberately weaker than its
  first draft claimed.** An earlier version asserted the tip hash "commits to
  depth"; measurement showed it does not, so the comment was corrected rather
  than the test left implying a property the code does not have.
- `tampering_with_a_middle_hop_is_not_detected` — a characterisation test that
  asserts the _current_ gap and is expected to turn red when this mission lands.
  Written with its direction stated explicitly, rather than as `assert!(is_ok())`,
  which would read as coverage of G4 while measuring the opposite.

## Acceptance Criteria

- [ ] `chain_hash` accumulates: it commits to the parent hop's hash, so a three-hop chain's tip differs from any of its prefixes _and_ changing any hop changes the tip.
- [ ] `wrap_for_hop` accepts the parent envelope (or a parent hash) so accumulation is possible at construction time.
- [ ] `verify_chain_hash` walks the chain rather than reading only the last element, or is renamed to what it actually does.
- [ ] `tampering_with_a_middle_hop_is_not_detected` is **inverted** and passes against the fixed implementation. Leaving it green after the fix is the failure.
- [ ] `verify_chain_hash_matches_last_envelope` is rewritten so its expectation is independent of the value it is checking.
- [ ] TV8 resolves to a test that is not `x == x`.

## What must not happen

Do not "fix" this by making `chain_hash` include the hop index. That makes depth
committed without making the chain verifiable, which would turn this finding into
a subtler one rather than a resolved one.

Do not delete `tampering_with_a_middle_hop_is_not_detected` when the fix lands.
A gap that disappears without leaving a test behind is how it comes back.

Do not satisfy TV2 by asserting that a three-element vector has three elements.
The vector is about the chain property, which is the thing that does not exist.

## Dependencies

- Mission `0970-a1-hop-crypto-and-replay-defense` — built the real X25519/Ed25519 crypto on this envelope
- Mission `0970-a-hop-envelope` — the envelope type
- RFC-0970 §Design Goals (G4), §Test Vectors (TV2, TV8)
