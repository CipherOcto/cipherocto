# Mission: 0855p-b — Governance RFC (governance_id rotation)

## Status

Open (2026-06-16) — post-launch

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work"

## Summary

A new RFC-0855p-d "Governance Lifecycle" specifies: (1) `governance_id` rotation on key compromise; (2) slash semantics for governance key compromise; (3) governance key ceremony.

## Design

### 1. `governance_id` rotation on key compromise

- The 3-of-5 governance multi-sig signs a `GOVERNANCE_ROTATION` envelope: `{ new_governance_id, old_governance_id, evidence, signed_at_epoch, signatures: Vec<Sig> }`.
- `evidence` is a signed attestation of the key compromise (e.g., a slash vote `0x000E` = `governance_key_compromise`).
- After broadcast, all subsequent slash votes must include the new `governance_id` in the `payload`.
- Old `governance_id` is invalid for new slash votes but remains valid for historical slashing (immutability).

### 2. Slash semantics for governance key compromise

- The old governance key is effectively burned: any slash vote with the old `governance_id` after `GOVERNANCE_ROTATION.epoch` is rejected.
- All missions must migrate to the new key within `GOVERNANCE_MIGRATION_WINDOW = 100` epochs (~100 minutes at 1-min epochs).
- After `GOVERNANCE_MIGRATION_WINDOW`, missions that have not migrated are suspended (no new elections; existing coordinators continue).

### 3. Governance key ceremony

- **Initial key gen:** All 5 governance key holders generate their keypairs independently; they exchange public keys out-of-band; they jointly compute the 3-of-5 threshold public key (using a DKG or trusted setup).
- **Recovery key gen:** A 5-of-7 recovery multi-sig is generated alongside the 3-of-5 governance multi-sig. The recovery multi-sig can rotate the governance multi-sig (one-shot).
- **Compromise response:** 2 of 5 recovery key holders can rotate the governance key; 3 of 5 must agree to rotate (anti-collusion).
- **Documentation:** step-by-step ceremony guide with hardware security module (HSM) recommendations.

## Acceptance Criteria

- [ ] New RFC-0855p-d "Governance Lifecycle" document
- [ ] `GOVERNANCE_ROTATION` envelope type in `crates/octo-network/src/governance/rotation.rs`
- [ ] `GOVERNANCE_MIGRATION_WINDOW = 100` constant
- [ ] `0x000E` slash reason code added in RFC-0855p-b §B "Slash Offense Codes"
- [ ] Old `governance_id` rejection in slash vote validation
- [ ] 5-of-7 recovery multi-sig spec
- [ ] DKG (distributed key generation) protocol spec
- [ ] Documentation: governance key ceremony step-by-step
- [ ] Documentation: HSM setup for governance key holders


### Implementation Guide

Reference: New RFC-0855p-d (created by this mission); DKG literature.


### Type Coverage

| RFC-0855p-b Type | Implemented By |
|-----------------|----------------|
| `GOVERNANCE_ROTATION` envelope type | This mission |
| `GOVERNANCE_MIGRATION_WINDOW = 100` constant | This mission |
| `0x000E` slash reason code (governance key compromise) | This mission |
| 5-of-7 recovery multi-sig | This mission |

## Dependencies

Depends on:
- RFC-0855 §11 (Governance) status: Accepted
- A new RFC-0855p-d (Governance Lifecycle) being created (this mission creates it)
- DKG (Distributed Key Generation) library

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/governance/rotation.rs` (new); `rfcs/accepted/networking/0855p-d-governance-lifecycle.md` (new RFC).

## Complexity

High (~1200 lines; new RFC, DKG ceremony spec, recovery multi-sig, slash semantics, migration window logic).

## Prerequisites

- RFC-0855 §11 status: Accepted

## Notes

### Why 5-of-7 recovery?

5-of-7 is a higher threshold than the 3-of-5 governance multi-sig, providing defense-in-depth. The recovery multi-sig is harder to compromise (requires 5 of 7 holders colluding).

### Why 100 epoch migration window?

100 minutes is enough time for missions to update their `governance_id` references (which involves a coordinated upgrade) without being so long that slashed missions operate in limbo.

### Why a new RFC?

The governance lifecycle (rotation, compromise, ceremony) is a substantial spec in its own right. It deserves its own RFC for review and maintenance.

## Mitigates

D-CL-6 (governance key compromise → mass slash event); D-CL-7 (governance key loss → frozen slash system)

## Deadline

Post-launch
