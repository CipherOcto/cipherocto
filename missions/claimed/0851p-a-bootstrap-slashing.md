# Mission: 0851p-a — Bootstrap node slashing

## Status

Claimed (2026-06-16) — byte-equality caller API landed; canonical slash issuance still pending chain-tx layer

## RFC

RFC-0851p-a (Networking): Network Bootstrap — §"Future Work"

## Summary

Extend slash reason codes (defined in RFC-0855p-b §B "Slash Offense Codes") with `0x000D` = `bootstrap_node_misbehavior`. Bootstrap nodes that misbehave (e.g., withhold peers, serve stale data, censor, lie about their reachability) are slashed and removed from the seed list.

**Cross-reference (Round 7 cross-mission-governance #7):** bootstrap slash finalization flows through the canonical RFC-0968 governance-issued slash event chain + quorum proof (`governance_set_hash`, `GOVERNANCE_QUORUM = 3`, amendment 24 / 53), the signed `SlashDestination` (Round 7 CRITICAL gov-2 binding), and the persisted `ReputationStore` — NOT an ad hoc 2/3 witness vote. The 2/3-witness vote is the WITNESS evidence path; the governance-issued slash event is the AUTHORITATIVE slash that persists to `ReputationStore` and triggers seed-list removal.

## Design

1. **New slash reason code:**
   - `0x000D` = `bootstrap_node_misbehavior`
   - Range allocation: `0x000A-0x000B` is transport-level (RFC-0850p-c §6), `0x000C-0xFFFF` is reserved. We claim `0x000D` for bootstrap-specific misbehavior. The reservation policy is in RFC-0855p-b §B.
2. **Misbehavior types (defined per code):**
   - `0x000D` = `bootstrap_node_misbehavior` (general; details in `slash_reason_data` field)
   - Sub-codes in `slash_reason_data`:
     - `0x000D.01` = `withholds_peers` (claims 0 reachable peers when it has > 0)
     - `0x000D.02` = `stale_data` (serves seed list older than `MAX_SEED_AGE_EPOCHS`)
     - `0x000D.03` = `censors_legit_peer` (refuses to include a specific peer that other seeds have)
     - `0x000D.04` = `false_reachability_claim` (claims a peer is reachable when it is not)
3. **Slash mechanism (Round 7 cross-mission-governance #7):**
   - Witnesses gather evidence (`2/3` majority per RFC-0855p-b is the EVIDENCE threshold; the canonical `SlashEvent` is governance-issued, NOT a direct witness vote).
   - The canonical slash is issued via `ReputationStore::slash_recorder(recorder_id, reason, destination, auth: &SuspensionAuth::Governance { proof }, governance_registry, now_unix)` (RFC-0968 §10). The `GovernanceProof` carries a fresh `GovernanceSnapshot` (within `MAX_GOVERNANCE_SNAPSHOT_AGE_SECS`), the `governance_set_hash` (amendment 24), and 3 distinct `governance_pubkey` signatures (the `GOVERNANCE_QUORUM = 3` threshold). The destination / amount / asset are signed (Round 7 CRITICAL gov-2).
   - The slash persists as a `SlashEvent` row in `reputation_events`; the bootstrap node's `peer_id` is added to a local blacklist via a consumer of the persisted `ReputationStore` state, NOT a side-channel.
   - Recovery uses RFC-0968 resume / re-entry rules (§3 grace + escalation per amendment 5 + Round 7 cross-mission-3 dual-stake counter bound to `controller_id`).
4. **Recovery:** Slashed bootstrap nodes can appeal via the canonical RFC-0968 governance path (`resume_recorder` with a fresh `ResumeProof` carrying the governance-quorum signature).

## Acceptance Criteria

- [ ] `0x000D` slash reason code in RFC-0855p-b §B (extends the table)
- [ ] `slash_reason_data: u32` field in `SlashEnvelope`
- [ ] `crates/octo-network/src/mon/bootstrap.rs::load_and_validate` — reject slashed seeds by consulting the persisted `ReputationStore` `reputation_events` table for canonical `SlashEvent` rows (NOT a side-channel local blacklist). `peer_id_to_recorder_did` is the canonical string→`RecorderDid` mapping (BLAKE3-derived, 32-byte digest zero-padded to 52 bytes under domain `cipherocto/bootstrap/peer_id_to_recorder_did/v1`). `SeedListValidation` carries `accepted` + `rejected` lists; `into_envelope` builds the filtered `SeedListEnvelope`. The pre-existing `SlashedSeedBlacklist` is documented as DEPRECATED and preserved only for serde back-compat in pre-0851p-a seed list files.
- [ ] Witness evidence voting flow in `crates/octo-network/src/mon/slash.rs` (the witness 2/3 majority is the EVIDENCE threshold, NOT the finalization)
- [ ] Canonical slash issuance via `ReputationStore::slash_recorder(recorder_id, reason, destination, auth, governance_registry, now_unix)` with a `GovernanceProof` carrying fresh snapshot, `governance_set_hash`, and `GOVERNANCE_QUORUM = 3` signatures (Round 7 cross-mission-governance #7)
- [ ] Signed `SlashDestination` (Round 7 CRITICAL gov-2 / Round 8 cross-mission-governance #2): the three `GovernanceProof` extension fields `slash_destination: SlashDestination`, `slash_amount: u64`, and `slash_asset: AssetTag` are bound to the governance signature. The on-wire `GovernanceProof.signature` covers `BLAKE3(BLAKE3_REPUTATION_SUSPENSION_DOMAIN || recorder_id || reason_hash || slash_destination_canonical_bytes || slash_amount_be || slash_asset_byte || governance_pubkey || now_unix)`. Mismatch on ANY of the three `caller_arg != signed_field` byte-equality checks returns `ReputationError::SlashDestinationMismatch` (`0x35`) BEFORE any chain-side transaction. Tests: `slash_amount` mismatch returns `0x35`; `slash_asset` mismatch returns `0x35`; `slash_destination` mismatch returns `0x35` (each independently). Cites RFC-0968 §10 + §21 + §23 Review-Round-7 vector.
- [ ] `cargo test -p oct-reputation --features stoolap --lib` integration test: bootstrap slash cannot finalize from an ad hoc 2/3 witness vote; the canonical path requires the governance-issued slash event + persistence via `ReputationStore`
- [ ] Unit tests: each misbehavior sub-code, witness vote aggregation (evidence path), governance slash finalization, SlashDestinationMismatch rejection
- [ ] Documentation: how bootstrap nodes can avoid being slashed (best practices)
- [ ] Documentation: operator guide for reviewing slash evidence

### Implementation Guide

Reference: RFC-0855p-b §B (slash reason codes); `crates/octo-network/src/mon/slash.rs` (existing slash flow).

### Type Coverage

| RFC-0851p-a Type                                                    | Implemented By |
| ------------------------------------------------------------------- | -------------- |
| `0x000D` slash reason code                                          | This mission   |
| `slash_reason_data: u32` field for sub-codes                        | This mission   |
| `crates/octo-bootstrap/src/seed_list.rs` rejection of slashed seeds | This mission   |

## Dependencies

Depends on:

- RFC-0855p-b status: Accepted (slash reason code allocation)
- Mission 0855p-b (slash reason codes base implementation)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-bootstrap/src/seed_list.rs` (add rejection); `crates/octo-network/src/mon/slash.rs` (add 0x000D).

## Complexity

Medium (~300 lines; slash flow integration, seed list blacklist, sub-code definitions).

## Prerequisites

- RFC-0855p-b status: Accepted

## Notes

### Why 0x000D?

The slash reason code range `0x000C-0xFFFF` is reserved (per RFC-0855p-b §B). `0x000D` is the first free code in that range. Transport-level codes (`0x000A-0x000B`) are taken by RFC-0850p-c §6.

### Why sub-codes?

A single `0x000D` code is too coarse. Sub-codes (`.01` withholds peers, `.02` stale data, etc.) let witnesses specify the exact offense.

## Mitigates

D-NB-3 (malicious seed list operator); D-NB-6 (Sybil via compromised bootstrap nodes).

## Deadline

Post-launch
