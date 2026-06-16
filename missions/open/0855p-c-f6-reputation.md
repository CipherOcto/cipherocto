# Mission: 0855p-c F6 — DomainCoordinator reputation

## Status

Open (2026-06-16) — future

## RFC

RFC-0855p-c (Networking): DomainCoordinator Role — §"Future Work" F6

## Summary

Similar to RFC-0855p-b F2 (cross-mission coordinator reputation), but per DomainCoordinator and across the domains it manages. A DC with a poor cross-domain reputation (many slashes across many domains) is deprioritized in future elections.

## Design

- `DCRootedSlashReputationStore` is a key-value store mapping `dc_pubkey` to a list of `SlashEvent` references (one per domain the DC has managed).
- On election, candidates with higher cross-domain slash count are deprioritized: `priority = stake / (1 + cross_domain_slash_count)`.
- Hard threshold: `cross_domain_slash_count >= 5` → excluded.
- The store is gossiped across the libp2p mesh under `/dot/reputation/dc/{dc_pubkey}`.
- Cross-references with RFC-0855p-b F2 (cross-mission reputation): a DC with both bad cross-domain and cross-mission reputation is severely deprioritized.

## Acceptance Criteria

- [ ] `DCRootedSlashReputationStore` type
- [ ] Gossip topic `/dot/reputation/dc/{dc_pubkey}`
- [ ] Cross-domain slash count calculation
- [ ] Election integration: deprioritization formula
- [ ] Hard threshold: `cross_domain_slash_count >= 5` → excluded
- [ ] Unit tests: count calculation, threshold, election integration
- [ ] Documentation: how cross-domain reputation is computed and used

## Mitigates

D-DC-9 (re-election of repeatedly-misbehaving DCs across domains)

## Deadline

Future
