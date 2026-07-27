# Mission: 0855p-b — Cross-mission recorder reputation

## Status

Claimed 2026-06-16 — post-launch. Cross-mission gossip substrate under active implementation alongside 0968 Phase 4 (federation storage); per mission gating, 0968 Phase 4 PR description MUST reference this mission path.

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work"

## Summary

Each `SlashEvent` per §"Slash Reason Codes" carries a per-mission `slash_count`. For cross-mission reputation, augment the local count with a global view fetched from a `SlashReputationStoreCompat` (the RFC-0968-A1 compatibility adapter that reads from the persisted `ReputationStore`). The store is keyed by canonical recorder DID OR stable lineage identifier — NOT by `coordinator_pubkey` (which the RFC-0968-A1 amendments 28-29 replaced as authoritative identity). On election, candidates with a higher global slash count are deprioritized via the canonical RFC-0968 §10 `election_priority` adapter; coordinator signatures are source-mission authorization only, NOT authoritative for the reputation payload.

## Design

- `SlashReputationStoreCompat` is a key-value store mapping canonical recorder DID (or stable lineage identifier) to a list of `SlashEvent` references (one per mission). The legacy `coordinator_pubkey` topic and keying model are REMOVED.
- On election start, the election coordinator queries `SlashReputationStoreCompat` for each candidate's global slash count, then delegates priority computation to the canonical `election_priority(candidate_did, stake, store, layer, now_unix)` adapter defined in RFC-0968 §10. The legacy `stake / (1 + global_slash_count)` formula is **deprecated**; the legacy value is preserved as `priority_legacy` for back-compat comparisons only.
- The cross-mission `SlashReputationStoreCompat` provides raw counts to `election_priority` via a thin adapter (`global_slash_count_for(did) -> u64`) that maps to the persisted RFC-0968 `severity_emitted_total`-style aggregate. The canonical priority is the RFC-0968 `election_priority` formula: `priority = ((min(stake, MAX_ELECTION_STAKE) × score_clamped).div(MAX_ELECTION_STAKE))` per RFC-0968 §28.1 amendment 13 (Round 6 I10 stake-saturation cap + the `.div(MAX_ELECTION_STAKE)` normalization), with the additional `MIN_ELECTION_SCORE = 0.05` floor below which `election_priority` returns `None`. The shorthand `stake_saturated × score_ewma` is a verbal abridgement; implementations MUST include the `.div(MAX_ELECTION_STAKE)` step to keep the priority value in the same units as the legacy `priority_legacy` comparison.
- Candidates with `global_slash_count >= 5` are excluded from the election (hard threshold). This is preserved as a pre-filter to `election_priority`.
- The store is gossiped across the libp2p mesh under `/dot/reputation/{recorder_did}` topic (canonical recorder DID / stable lineage identifier), with the canonical RFC-0968 recorder-authoritative envelope fields: `event_id`, `recorder_did`, `recorder_signature`, `source_mission`, `source_domain`, and rotation lineage where applicable. Coordinator signatures are source-mission authorization only; they cannot replace the recorder-authoritative signature on the slash event itself.
- Privacy: slash events are referenced by hash, not included in full; the full event is fetched on demand.

## Acceptance Criteria

- [ ] `crates/octo-network/src/reputation/slash_store.rs` — `SlashReputationStoreCompat` type, keyed by canonical recorder DID or stable lineage identifier
- [ ] Gossip topic `/dot/reputation/{recorder_did}` (NOT `coordinator_pubkey`) in `crates/octo-network/src/gossip/reputation.rs`
- [ ] No authoritative store key or gossip topic uses `coordinator_pubkey`
- [ ] Slash events carry the RFC-0968 recorder-authoritative envelope: `event_id`, `recorder_did`, `recorder_signature`, `source_mission`, `source_domain`, rotation lineage where applicable
- [ ] Coordinator signatures are present only as source-mission authorization; they do NOT replace recorder authorization on the slash event itself
- [ ] Election priority consumed via RFC-0968 `election_priority` (legacy `stake / (1 + global_slash_count)` is `priority_legacy` only)
- [ ] Hard threshold: `global_slash_count >= 5` → excluded (pre-filter to `election_priority`)
- [ ] Cross-mission conformance test: legacy `priority_legacy` and canonical `election_priority` produce identical ordering over 1000 candidate set with both fully populated (differential test; tolerance = byte-identical priority ordering)
- [ ] Unit tests: priority calculation, threshold enforcement
- [ ] Integration test: gossip propagation of slash reputation
- [ ] Documentation: how slash reputation is computed and used in elections

### Implementation Guide

Reference: `crates/octo-network/src/reputation/slash_store.rs` (new); `crates/octo-network/src/gossip/reputation.rs` (new).

### Type Coverage

| RFC-0855p-b Type                                                                         | Implemented By |
| ---------------------------------------------------------------------------------------- | -------------- |
| `SlashReputationStoreCompat` type (RFC-0968-A1 adapter over persisted `ReputationStore`) | This mission   |
| `/dot/reputation/{recorder_did}` gossip topic (canonical DID / lineage)                  | This mission   |
| Priority formula: `stake / (1 + global_slash_count)` (legacy `priority_legacy`)          | This mission   |

## Dependencies

Depends on:

- **RFC-0968 (Accepted)** — authoritative reputation store, §10 election_priority adapter, §7.1 retirement gate per adapter, RFC-0968-A1 amendments 28-29 (canonical DID / recorder-authoritative signature model)
- **Mission 0968 (claimed)** — provides persisted `ReputationStore` and `SlashReputationStoreCompat` adapter; this mission reads through the adapter, NOT directly from the legacy in-memory store
- **Mission 0968-b (open, this RFC's marketplace carrier)** — owns marketplace read-side retirement gate
- RFC-0855p-b status: Accepted
- Mission 0855p-b (slash reason codes base implementation)
- The libp2p mesh (already operational)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/reputation/slash_store.rs` (new); `crates/octo-network/src/gossip/reputation.rs` (new).

## Complexity

Medium (~400 lines; store type, gossip protocol, priority formula).

## Prerequisites

- RFC-0855p-b status: Accepted
- **RFC-0968 (Accepted)** — see Dependencies

## Notes

### Why a soft penalty?

A hard disqualification is a one-strike-and-out policy that is too aggressive. A slashed coordinator may have been a victim of platform misbehavior (e.g., admin key compromise by an attacker). The soft penalty (priority = stake / (1 + count)) reduces the chance of re-election but doesn't forbid it.

### Why a hard threshold at 5?

5 slashes is a strong signal of repeated misbehavior. Beyond this, the coordinator is excluded.

### Why canonical DID / recorder-authoritative envelope?

RFC-0968-A1 amendments 28-29 closed the pubkey-keyed / coordinator-authoritative model. Slash gossip is part of the cross-mission reputation pipeline; coordinator rotation, key compromise, and Sybil-instability all break the pubkey model. The canonical recorder DID / stable lineage identifier is governance-attested (amendment 40 / 44) and survives coordinator rotation; the recorder-authoritative envelope prevents a misbehaving coordinator from forging or suppressing slash events against a recorder.

## Mitigates

D-CL-3 (re-election of repeatedly-misbehaving coordinators)

## Deadline

Post-launch
