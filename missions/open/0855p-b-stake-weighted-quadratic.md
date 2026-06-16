# Mission: 0855p-b — Stake-weighted quadratic-cost voting

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work"

## Summary

Voting weight is `sqrt(stake) * cosigners`, where `cosigners` is the count of cross-signers on the candidate's `CoordinatorRecord` (a measure of social trust). This dampens the influence of large stakeholders (quadratic cost) while still rewarding stake and trust. The election algorithm is per governance model (e.g., DAO uses this, Centralized uses designator).

## Design

- `voting_weight = sqrt(stake) * cosigners`
- `stake`: the candidate's OCTO-O stake in the mission (per RFC-0855 §13 "Token Economics Integration")
- `cosigners`: the count of distinct `CosignEnvelope` signatures on the candidate's `CoordinatorRecord` (excluding the candidate's own self-attestation)
- The square root dampens the influence of large stakeholders: a candidate with 4× the stake has only 2× the voting weight, not 4×. This prevents plutocracy.
- The `cosigners` multiplier rewards social trust: a candidate endorsed by 10 other coordinators has 10× the weight of an unendorsed candidate, all else equal.
- For Centralized governance: voting weight is irrelevant; the designator picks. Quadratic-cost voting is only for DAO and Federated models.

## Acceptance Criteria

- [ ] `voting_weight = sqrt(stake) * cosigners` formula in `crates/octo-network/src/election/quadratic.rs`
- [ ] `cosigners` count from `CoordinatorRecord::cosignatures` field
- [ ] Used for DAO and Federated governance models
- [ ] Centralized governance: ignores voting weight
- [ ] Unit tests: weight calculation, model-specific application
- [ ] Documentation: why quadratic (anti-plutocracy), why cosigners (anti-Sybil)
- [ ] Documentation: comparison with linear voting and pure quadratic voting

## Mitigates

D-CL-4 (plutocracy in election); D-CL-5 (Sybil in election)

## Deadline

Post-launch
