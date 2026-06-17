# Mission: 0855p-b — Election by random beacon (VDF)

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work"

## Summary

A Verifiable Delay Function (VDF) per RFC-0855p-c §"Random Beacon" (currently being spec-ed) is used to elect the next coordinator. Each candidate computes `VDF(seed_for_epoch)` over `EPOCH_DURATION_SECONDS = 60`; the candidate whose VDF output is closest to the beacon's published randomness (lowest XOR distance) wins. The VDF is verified on receipt.

## Design

1. **VDF construction:** Wesolowski-style prime-field VDF. `prove(seed, t)` returns `(y, pi)` where `y = seed^(2^t) mod N` and `pi` is a proof of correct exponentiation. Verification: `verify(pi, seed, t, y)` checks `pi` is consistent.
2. **Beacon seed:** `seed_for_epoch = hash(governance_id || epoch_number || previous_seed)`. The seed is published by the previous epoch's coordinator at the start of the new epoch.
3. **Election process:**
   - At epoch boundary, all eligible candidates compute `VDF(seed_for_epoch)` over `EPOCH_DURATION_SECONDS = 60`.
   - Each candidate broadcasts `(vdf_output, vdf_proof, candidate_pubkey)`.
   - The election coordinator verifies all proofs (parallelized) and selects the candidate with the lowest `XOR(vdf_output, beacon_randomness)`.
   - `beacon_randomness` is the hash of the epoch's slash events (one-shot beacon: hard to predict, easy to verify).
4. **Tie-break:** if two candidates have identical XOR distance (collision), the lower `candidate_pubkey` (lex order) wins.
5. **Library:** use the `class_groups` crate for the Wesolowski VDF (Rust-native; no Python deps).

## Acceptance Criteria

- [ ] `crates/octo-network/src/election/vdf.rs` — VDF wrapper around `class_groups`
- [ ] `EPOCH_DURATION_SECONDS = 60` constant
- [ ] VDF proof verification on the election coordinator
- [ ] Tie-break: lex `candidate_pubkey` ordering
- [ ] Beacon: `beacon_randomness = hash(slash_events_of_epoch)`
- [ ] Unit tests: VDF proof generation + verification, election winner selection, tie-break
- [ ] Integration test: 10 candidates, all VDF-compute, one winner
- [ ] Documentation: VDF security assumptions (setup ceremony, prime selection)
- [ ] Documentation: operator guide for VDF computation (CPU cost: ~1 core × 60s per candidate per election)


### Implementation Guide

Reference: Wesolowski VDF paper; `class_groups` crate documentation; RFC-0855p-c (Random Beacon section).


### Type Coverage

| RFC-0855p-b Type | Implemented By |
|-----------------|----------------|
| `crates/octo-network/src/election/vdf.rs` | This mission |
| `EPOCH_DURATION_SECONDS = 60` constant | This mission |
| Tie-break: lex `candidate_pubkey` ordering | This mission |

## Dependencies

Depends on:
- A VDF library (the `class_groups` crate)
- A beacon randomness source (the `SlashEvent` hash for the epoch)
- RFC-0855p-c §'Random Beacon' being spec-ed (currently a forward reference; this mission triggers its creation)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/election/vdf.rs` (new).

## Complexity

High (~700 lines; VDF wrapper, proof verification, election state machine, beacon seed derivation).

## Prerequisites

- `class_groups` crate version pinning
- VDF security review (setup ceremony, prime selection)

## Notes

### Why Wesolowski?

Wesolowski VDFs are well-studied and have a simple, fast verifier. The alternative (Pietrzak) is more complex and slower to verify.

### Why 60s?

VDF computation is CPU-intensive (1 core × 60s per candidate). 60s is the minimum that produces unpredictable randomness; shorter VDFs are too easy to grind. Longer VDFs slow down the election.

## Mitigates

D-CL-1 (predictable leader election); D-CL-2 (grinding attacks on election)

## Deadline

Post-launch
