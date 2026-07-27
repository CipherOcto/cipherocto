# Mission: 0851p-a — Bootstrap node slashing

## Status

Completed (2026-07-27) — PR submission pending. All 10 acceptance criteria landed: `0x000D` reason code + `slash_reason_data` field (commit history pre-`77d48639`); canonical `issue_governance_slash` with Round 7 gov-2 byte-equality gate (`8fae15e5`); witness-evidence voting + `BootstrapEvidence::finalize` per-recorder isolation; chain-tx preimage contract (`77d48639`); `load_and_validate` via persisted `ReputationStore` rejecting side-channel blacklists (`91110a34`); stoolap-gated `ad_hoc_2_3_witness_votes_alone_do_not_finalize` + `canonical_issue_governance_slash_persists_extra_event` integration tests; two operator-facing docs (`d7551add`).

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

- [x] `0x000D` slash reason code in RFC-0855p-b §B (extends the table)
- [x] `slash_reason_data: u32` field in `SlashEnvelope`
- [x] `crates/octo-network/src/mon/bootstrap.rs::load_and_validate` — reject slashed seeds by consulting the persisted `ReputationStore` `reputation_events` table for canonical `SlashEvent` rows (NOT a side-channel local blacklist). `peer_id_to_recorder_did` is the canonical string→`RecorderDid` mapping (BLAKE3-derived, 32-byte digest zero-padded to 52 bytes under domain `cipherocto/bootstrap/peer_id_to_recorder_did/v1`). `SeedListValidation` carries `accepted` + `rejected` lists; `into_envelope` builds the filtered `SeedListEnvelope`. The pre-existing `SlashedSeedBlacklist` is documented as DEPRECATED and preserved only for serde back-compat in pre-0851p-a seed list files. (commit `91110a34`)
- [x] Witness evidence voting flow in `crates/octo-network/src/mon/slash.rs` (the witness 2/3 majority is the EVIDENCE threshold, NOT the finalization) — `SlashEnvelope::to_slash_vote` + `BootstrapEvidence::finalize` (`77d48639`)
- [x] Canonical slash issuance via `ReputationStore::slash_recorder` with `GovernanceProof` carrying fresh snapshot, `governance_set_hash`, and `GOVERNANCE_QUORUM = 3` signatures (Round 7 cross-mission-governance #7) — `issue_governance_slash` (`8fae15e5`)
- [x] Signed `SlashDestination` (Round 7 CRITICAL gov-2 / Round 8 cross-mission-governance #2): the three `GovernanceProof` extension fields are bound to the governance signature via `slash_signature_preimage`; byte-equality gates (`0x35`) at the API boundary; tests for amount/asset/destination/recorder_id mismatches (`8fae15e5` + `77d48639`)
- [x] `cargo test -p oct-reputation --features stoolap --lib` integration tests `ad_hoc_2_3_witness_votes_alone_do_not_finalize` + `canonical_issue_governance_slash_persists_extra_event` (stoolap-gated, this commit)
- [x] Unit tests: each misbehavior sub-code (`.01..04`), witness vote aggregation, `SlashDestinationMismatch` rejection (`8fae15e5`, `77d48639`)
- [x] Documentation: how bootstrap nodes can avoid being slashed — `docs/07-developers/bootstrap-slash-prevention-guide.md` (`d7551add`)
- [x] Documentation: operator guide for reviewing slash evidence — `docs/06-operations/bootstrap-slash-evidence-runbook.md` (`d7551add`)

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
