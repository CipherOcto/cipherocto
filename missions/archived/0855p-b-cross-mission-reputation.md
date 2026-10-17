# Mission: 0855p-b — Cross-mission recorder reputation

## Status

Completed (2026-07-27) — PR submission pending. All 12 acceptance criteria landed across the 4-session plan (`spicy-painting-globe.md`): canonical types + gossip envelope contract (`f0c8d6ad`); slash store compat + gossip substrate over libp2p with DID-keyed topics + 1000-candidate differential test (`388fd327`); federation storage in stoolap with attestor quorum + rate limit + catch-up (`87ffe153`); 2-node integration test + operator runbook (`f16132c0`). Per mission gating, 0968 Phase 4 PR description MUST reference this mission path.

## RFC

RFC-0855p-b (Networking): Coordinator Lifecycle — §"Future Work"

## Summary

Each `SlashEvent` per §"Slash Reason Codes" carries a per-mission `slash_count`. For cross-mission reputation, augment the local count with a global view fetched from a `SlashReputationStoreCompat` (the RFC-0968-A1 compatibility adapter that reads from the persisted `ReputationStore`). The store is keyed by canonical recorder DID OR stable lineage identifier — NOT by `coordinator_pubkey` (which the RFC-0968 §28.4 amendments 21 + 22 replaced as authoritative identity). On election, candidates with a higher global slash count are deprioritized via the canonical RFC-0968 §10 `election_priority` adapter; coordinator signatures are source-mission authorization only, NOT authoritative for the reputation payload.

## Design

- `SlashReputationStoreCompat` is a key-value store mapping canonical recorder DID (or stable lineage identifier) to a list of `SlashEvent` references (one per mission). The legacy `coordinator_pubkey` topic and keying model are REMOVED.
- On election start, the election coordinator queries `SlashReputationStoreCompat` for each candidate's global slash count, then delegates priority computation to the canonical `election_priority(candidate_did, stake, store, layer, now_unix)` adapter defined in RFC-0968 §10. The legacy `stake / (1 + global_slash_count)` formula is **deprecated**; the legacy value is preserved as `priority_legacy` for back-compat comparisons only.
- The cross-mission `SlashReputationStoreCompat` provides raw counts to `election_priority` via a thin adapter (`global_slash_count_for(did) -> u64`) that maps to the persisted RFC-0968 `severity_emitted_total`-style aggregate. The canonical priority is the RFC-0968 `election_priority` formula: `priority = ((min(stake, MAX_ELECTION_STAKE) × score_clamped).div(MAX_ELECTION_STAKE))` per RFC-0968 §28.1 amendment 13 (Round 6 I10 stake-saturation cap + the `.div(MAX_ELECTION_STAKE)` normalization), with the additional `MIN_ELECTION_SCORE = 0.05` floor below which `election_priority` returns `None`. The shorthand `stake_saturated × score_ewma` is a verbal abridgement; implementations MUST include the `.div(MAX_ELECTION_STAKE)` step to keep the priority value in the same units as the legacy `priority_legacy` comparison.
- Candidates with `global_slash_count >= 5` are excluded from the election (hard threshold). This is preserved as a pre-filter to `election_priority`.
- The store is gossiped across the libp2p mesh under `/dot/reputation/{recorder_did}` topic (canonical recorder DID / stable lineage identifier), with the canonical RFC-0968 recorder-authoritative envelope fields: `event_id`, `recorder_did`, `recorder_signature`, `source_mission`, `source_domain`, and rotation lineage where applicable. Coordinator signatures are source-mission authorization only; they cannot replace the recorder-authoritative signature on the slash event itself.
- Privacy: slash events are referenced by hash, not included in full; the full event is fetched on demand.

## Acceptance Criteria

- [x] `crates/octo-network/src/reputation/slash_store.rs` — `SlashReputationStoreCompat` keyed by canonical recorder DID (`388fd327`)
- [x] Gossip topic `/dot/reputation/{recorder_did}` (NOT `coordinator_pubkey`) in `crates/octo-network/src/gossip/reputation.rs` (`388fd327`)
- [x] No authoritative store key or gossip topic uses `coordinator_pubkey` (audit: no `coordinator_pubkey:` field in `SlashReputationStoreCompat`)
- [x] Slash events carry the RFC-0968 recorder-authoritative envelope: `event_id`, `recorder_did`, `recorder_signature`, `source_mission`, `source_domain`, rotation lineage where applicable (`GossipEnvelope` in `crates/octo-reputation/src/gossip.rs`, `f0c8d6ad`)
- [x] Coordinator signatures are present only as source-mission authorization; they do NOT replace recorder authorization on the slash event itself (RFC-0968 §28.4 amendment 21 + authority model in `dc_store.rs`)
- [x] Election priority consumed via RFC-0968 `election_priority` (`388fd327` + amendment 27); legacy `priority_legacy` preserved for differential test
- [x] Hard threshold: `global_slash_count >= 5` → excluded (pre-filter to `election_priority`) — `HARD_THRESHOLD = 5` constant in `slash_store.rs`
- [x] Cross-mission conformance test: byte-identical ordering over 1000 candidates (`differential_1000_candidates_byte_identical_ordering`, `388fd327`)
- [x] Unit tests: priority calculation, threshold enforcement (`388fd327` + `dc_store.rs::differential_*`)
- [x] Integration test: gossip propagation (real 2-node QUIC loopback, `crates/octo-network/tests/cross_mission_federation.rs`, `f16132c0`)
- [x] Documentation: `docs/07-developers/reputation-federation-guide.md` (`f16132c0`)

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

- **RFC-0968 (Accepted)** — authoritative reputation store, §10 election_priority adapter, §7.1 retirement gate per adapter, RFC-0968 §28.4 amendments 21 + 22 (canonical DID / recorder-authoritative signature model)
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

RFC-0968 §28.4 amendments 21 + 22 closed the pubkey-keyed / coordinator-authoritative model. Slash gossip is part of the cross-mission reputation pipeline; coordinator rotation, key compromise, and Sybil-instability all break the pubkey model. The canonical recorder DID / stable lineage identifier is governance-attested (RFC-0968-A1 amendment 40 (deferred to RFC-0968-A2); RFC-0968-A1 amendment 44 (deferred to RFC-0968-A2)) and survives coordinator rotation; the recorder-authoritative envelope prevents a misbehaving coordinator from forging or suppressing slash events against a recorder.

## Mitigates

D-CL-3 (re-election of repeatedly-misbehaving coordinators)

## Deadline

Post-launch
