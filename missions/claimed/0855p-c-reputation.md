# Mission: 0855p-c — DomainCoordinator reputation

## Status

Completed (2026-07-27) — PR submission pending. All 7 acceptance criteria landed: `DcRootedSlashReputationStoreCompat` in `crates/octo-network/src/reputation/dc_store.rs` keyed by canonical DC recorder DID (`fe3238d2`); gossip topic `/dot/reputation/dc/{dc_did}` NOT `dc_pubkey`; Path B `refresh_cross_domain_for` reading from persisted `ReputationStore` (`9dd61a66`); canonical `election_priority` adapter consuming RFC-0968 §10; hard threshold `cross_domain_slash_count >= 5`; 1000-candidate differential test against `priority_legacy` for byte-identical ordering.

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work"

## Summary

Similar to RFC-0855p-b F(cross-mission recorder reputation), but per DomainCoordinator and across the domains it manages. A DC with a poor cross-domain reputation (many slashes across many domains) is deprioritized in future elections. The store is keyed by canonical DC recorder DID or stable lineage identifier — NOT by `dc_pubkey` (which RFC-0968-A1 amendment 29 replaced as authoritative identity).

## Design

- `DcRootedSlashReputationStoreCompat` is a key-value store mapping canonical DC recorder DID (or stable lineage identifier) to a list of `SlashEvent` references (one per domain the DC has managed). The legacy `dc_pubkey` topic and keying model are REMOVED; this mission is a compatibility adapter over the persisted RFC-0968 `ReputationStore`.
- On election, candidates with higher cross-domain slash count are deprioritized via the canonical RFC-0968 §10 `election_priority` adapter. The legacy `priority = stake / (1 + cross_domain_slash_count)` formula is **deprecated**; the value is preserved as `priority_legacy` for back-compat comparisons only. The cross-domain `DcRootedSlashReputationStoreCompat` exposes `cross_domain_slash_count_for(did) -> u64` which feeds the RFC-0968 `election_priority` formula: `priority = ((min(stake, MAX_ELECTION_STAKE) × score_clamped).div(MAX_ELECTION_STAKE))` per RFC-0968 §28.1 amendment 13. The shorthand `stake_saturated × score_ewma` is a verbal abridgement; implementations MUST include the `.div(MAX_ELECTION_STAKE)` step.
- Hard threshold: `cross_domain_slash_count >= 5` → excluded.
- The store is gossiped across the libp2p mesh under `/dot/reputation/dc/{dc_did}` (canonical DC recorder DID / stable lineage identifier, NOT `dc_pubkey`), with the canonical RFC-0968 recorder-authoritative envelope fields: `event_id`, `recorder_did`, `recorder_signature`, `source_domain`, rotation lineage where applicable. Cross-domain slash events are converted into the RFC-0968 canonical event/aggregate form BEFORE persistence.
- Cross-references with RFC-0855p-b F(cross-mission reputation): a DC with both bad cross-domain and cross-mission reputation is severely deprioritized.
- Election consumers invoke the §10 `election_priority` adapter with the correct `layer` (the DC's per-domain layer) and a caller-supplied trusted `now_unix` per RFC-0968 Class-B determinism (RFC-0968 §16 + amendment 14).

## Acceptance Criteria

- [x] `DcRootedSlashReputationStoreCompat` keyed by canonical DC recorder DID (`crates/octo-network/src/reputation/dc_store.rs`, `fe3238d2`)
- [x] Gossip topic `/dot/reputation/dc/{dc_did}` (NOT `dc_pubkey`) per RFC-0968-A1 amendment 29 (`fe3238d2`)
- [x] No authoritative store key or gossip topic uses `dc_pubkey` (audit: see `crates/octo-network/src/reputation/dc_store.rs` — keyed by `RecorderDid` only)
- [x] Cross-domain slash events converted into RFC-0968 canonical event/aggregate form BEFORE persistence (DC layer = `ReputationLayer::Coordinator` filter in `refresh_cross_domain_for`)
- [x] Election integration: RFC-0968 §10 `election_priority` adapter with `layer = Coordinator` + caller-supplied trusted `now_unix`; legacy `priority_legacy` preserved for back-compat (`fe3238d2`)
- [x] Cross-domain slash issuance: Round 7 CRITICAL gov-2 byte-equality gate inherited from mission 0851p-a — `issue_governance_slash` enforces `SlashDestinationMismatch = 0x35` for cross-domain slashes too
- [x] Hard threshold: `cross_domain_slash_count >= 5` → excluded (shared `HARD_THRESHOLD = 5` with `slash_store.rs`, `fe3238d2`)
- [x] Unit tests: count calculation, threshold, election integration, 1000-candidate differential (`fe3238d2` + `9dd61a66`)
- [x] Documentation: how cross-domain reputation is computed and used — `crates/octo-network/src/reputation/dc_store.rs` module doc + `docs/07-developers/reputation-federation-guide.md` cross-references the DC layer

### Implementation Guide

Reference: `crates/octo-network/src/reputation/slash_store.rs` (existing, from 0855p-b); `crates/octo-network/src/reputation/dc_store.rs` (new). Implementation MUST consult both `reputation_rotations` and `reputation_events` for the replayed DC DID and reject rotation-lineage collisions per RFC-0968 §15.

### Type Coverage

| RFC-0855p-c Type                                                                                 | Implemented By |
| ------------------------------------------------------------------------------------------------ | -------------- |
| `DcRootedSlashReputationStoreCompat` type (RFC-0968-A1 adapter over persisted `ReputationStore`) | This mission   |
| `/dot/reputation/dc/{dc_did}` gossip topic (canonical DC DID / lineage)                          | This mission   |
| Cross-domain slash count formula                                                                 | This mission   |

## Dependencies

Depends on:

- **RFC-0968 (Accepted)** — authoritative reputation store, §10 election_priority adapter, §7.1 retirement gate per adapter, RFC-0968-A1 amendment 29 (canonical DC DID / recorder-authoritative signature model)
- **Mission 0968 (claimed)** — provides persisted `ReputationStore` and `DcRootedSlashReputationStoreCompat` adapter; this mission reads through the adapter, NOT directly from a legacy in-memory store
- **Mission 0968-b (open, this RFC's marketplace carrier)** — owns marketplace read-side retirement gate
- Mission 0855p-c-cross-domain-slash (which updates the reputation store; must use the same canonical DC DID / lineage model per the amendment)
- Mission 0855p-b-cross-mission-reputation (similar pattern, can share code)

## Claimant

(none — Open mission)

## Pull Request

(none — Open mission)

## Location

`crates/octo-network/src/reputation/dc_store.rs` (new).

## Complexity

Low (~200 lines; new store type, gossip topic, election integration).

## Prerequisites

- Mission 0855p-c-cross-domain-slash
- **RFC-0968 (Accepted)** — see Dependencies

## Notes

### Why separate from 0855p-b reputation?

Mission-level reputation (0855p-b) tracks a mission recorder. DC-level reputation (0855p-c) tracks a DomainCoordinator. The two are correlated (a mission recorder may also be a DC) but separate in scope.

### Why hard threshold at 5?

Same as 0855p-b: 5 slashes is a strong signal of repeated misbehavior. The threshold is consistent across both reputation stores.

### Why canonical DC DID / recorder-authoritative envelope?

RFC-0968-A1 amendment 29 closed the pubkey-keyed / coordinator-authoritative model for DC slash gossip. DC rotation, key compromise, and Sybil-instability all break the pubkey model. The canonical DC recorder DID / stable lineage identifier is governance-attested (amendment 40 / 44) and survives DC rotation; the recorder-authoritative envelope prevents a misbehaving DC from forging or suppressing slash events against another DC.

## Mitigates

D-DC-9 (re-election of repeatedly-misbehaving DCs across domains)

## Deadline

Future
