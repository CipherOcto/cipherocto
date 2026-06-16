# Mission: 0855p-c — DomainCoordinator reputation

## Status

Open (2026-06-16) — future

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work"

## Summary

Similar to RFC-0855p-b F(cross-mission coordinator reputation), but per DomainCoordinator and across the domains it manages. A DC with a poor cross-domain reputation (many slashes across many domains) is deprioritized in future elections.

## Design

- `DCRootedSlashReputationStore` is a key-value store mapping `dc_pubkey` to a list of `SlashEvent` references (one per domain the DC has managed).
- On election, candidates with higher cross-domain slash count are deprioritized: `priority = stake / (1 + cross_domain_slash_count)`.
- Hard threshold: `cross_domain_slash_count >= 5` → excluded.
- The store is gossiped across the libp2p mesh under `/dot/reputation/dc/{dc_pubkey}`.
- Cross-references with RFC-0855p-b F(cross-mission reputation): a DC with both bad cross-domain and cross-mission reputation is severely deprioritized.

## Acceptance Criteria

- [ ] `DCRootedSlashReputationStore` type
- [ ] Gossip topic `/dot/reputation/dc/{dc_pubkey}`
- [ ] Cross-domain slash count calculation
- [ ] Election integration: deprioritization formula
- [ ] Hard threshold: `cross_domain_slash_count >= 5` → excluded
- [ ] Unit tests: count calculation, threshold, election integration
- [ ] Documentation: how cross-domain reputation is computed and used

## Dependencies

Depends on:
- Mission 0855p-c-cross-domain-slash (which updates the reputation store)
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

## Notes

### Why separate from 0855p-b reputation?

Mission-level reputation (0855p-b) tracks a mission coordinator. DC-level reputation (0855p-c) tracks a DomainCoordinator. The two are correlated (a mission coordinator may also be a DC) but separate in scope.

### Why hard threshold at 5?

Same as 0855p-b: 5 slashes is a strong signal of repeated misbehavior. The threshold is consistent across both reputation stores.

### Type Coverage

| RFC-0855p-c Type | Implemented By |
|-----------------|----------------|
| `DCRootedSlashReputationStore` type | This mission |
| `/dot/reputation/dc/{dc_pubkey}` gossip topic | This mission |
| Cross-domain slash count formula | This mission |

### Implementation Guide

Reference: `crates/octo-network/src/reputation/slash_store.rs` (existing, from 0855p-b); `crates/octo-network/src/reputation/dc_store.rs` (new).

## Mitigates

D-DC-9 (re-election of repeatedly-misbehaving DCs across domains)

## Deadline

Future
