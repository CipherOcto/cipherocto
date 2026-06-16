# Mission: 0855p-b F3 — Election by random beacon (VDF)

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work" F3

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

## Mitigates

D-CL-1 (predictable leader election); D-CL-2 (grinding attacks on election)

## Deadline

Post-launch
