# 0011-h-network-peers-identity — `peers` subcommands per RFC-0011-h Phase 1

## Status

Completed (2026-09-20) — CLI dispatch slice LANDED at `next c2fee8f3` (RFC-0011-i Phase 1 IMPLEMENTATION 3-commit chain). 5 subcommands wired: `peers list` + `peers get <gateway_id>` + `identity show` + `trust-graph render` + `governance rotation status`. Substrate-faithful per RFC-0011-i §Substrate Mapping Table: `GatewayCache::iter()` + `GatewayCache::get(&[u8; 32])` + `LocalGatewayIdentity::load` (paired companion substrate `0011-h-s-a-local-gateway-identity-state` at `next caeb84c4`) + `GatewayIdentity::new(public_key, network_id, gateway_class, creation_epoch)`. Layer B substrate already present at `crates/octo-network/src/{gdp, dot, mon}/` per RFC-0011-h closure verification — no companion substrate needed for peers/identity surface. 339/339 octo-cli tests (was 323, +13 Phase 1 vectors + 3 from Phase 1 follow-on). Layer discipline preserved (zero Layer A change).

## RFC

RFC-0011-h §Implementation Phases Phase 1 + RFC-0011-i §Subcommand Taxonomy Phase 1 rows + RFC-0011-i §Substrate Mapping Table

## Summary

Substrate-faithful CLI surface for gateway cache (peers list/get + identity show). Per RFC-0011-h §Implementation Phases Phase 1.

### Subcommands

- `peers list` (CLOSE-OUT per `next c2fee8f3`)
- `peers get <gateway_id>` (CLOSE-OUT per `next c2fee8f3`)
- `identity show` (CLOSE-OUT per `next c2fee8f3`)

## Acceptance Criteria

- [x] CLI surface for the listed subcommands per RFC-0011-h §Subcommand Taxonomy (next c2fee8f3)
- [x] Test vectors per RFC-0011-h §Test Vectors for the listed subcommands (next c2fee8f3)
- [x] `cargo clippy -p octo-cli --all-targets -- -D warnings` clean (next c2fee8f3)
- [x] `cargo test -p octo-cli --lib` green (next c2fee8f3)
- [x] Layer discipline preserved (CLI Layer C only; zero Layer A change) (next c2fee8f3)
- [x] Substrate-faithful boundary: every call crosses typed façade per [[cipherocto-design-principles]] §Stable Abstractions Principle (next c2fee8f3)

## Dependencies

- Hard sequencing: RFC-0011-h must be Accepted before this mission lands — LANDED (RFC-0011-h Accepted at `next 8696ec4b`)
- Hard sequencing: RFC-0011-i must be Accepted before this mission lands — paired companion mission YAML closed paired with Phase 1 IMPLEMENTATION `next c2fee8f3`

## Out of Scope

- Substrate additions (covered by paired `0011-h-s-a-*` companion missions; only `LocalGatewayIdentity::load` needed per F2 R1.5 fix, which landed paired with this surface)

## Notes

Stub filed 2026-09-18 per [[no-phantom-mission-pointers]]. Paired closure transition paired with RFC-0011-i Phase 1 IMPLEMENTATION COMPLETE closure card. Substrate-faithful boundary verified per [[substrate-faithfulness-verification]] R7.5 lesson: `GatewayCache` + `TrustGraph` + `GovernanceRotation` substrate PRESENT at RFC-0011-h closure verification — no companion substrate needed beyond `LocalGatewayIdentity::load` (which landed paired at `next caeb84c4`).
