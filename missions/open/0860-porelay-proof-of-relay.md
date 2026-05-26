# Mission: PoRelay Proof-of-Relay

## Status

Open

## RFC

RFC-0860: Proof-of-Relay (PoRelay)

## Summary

Implement proof-of-relay with relay proofs (forwarding, availability, bandwidth, uptime), gateway heartbeat, trust score computation, slashing conditions, and OCTO-S token integration for proof archival.

## Acceptance Criteria

- [ ] `ForwardingProof` with envelope_hash, relay_gateway, next_hop, timestamp, signature
- [ ] `AvailabilityProof` with gateway_id, epoch_start, epoch_end, uptime_ratio, signature
- [ ] `BandwidthProof` with gateway_id, bytes_relayed, epoch, signature
- [ ] `UptimeProof` with gateway_id, consecutive_epochs, signature
- [ ] `RelayScore` with forwarding_score, availability_score, bandwidth_score, uptime_score, aggregate
- [ ] `AggregatedRelayProof` with constituent_proofs, aggregation_root
- [ ] `GatewayAdvertisementWithPoR` extending GatewayAdvertisement with relay_proofs
- [ ] `GatewayHeartbeat` with gateway_id, sequence, active_routes, load_class, uptime_class, signature
- [ ] `TrustScore` with historical_uptime, proof_of_relay, stake_weight, mission_trust, consensus_participation
- [ ] Trust score computation is RFC-0008 Class A (deterministic)
- [ ] Slashing conditions: invalid proofs, downtime threshold, malicious routing
- [ ] OCTO-B token for relay bandwidth rewards
- [ ] OCTO-N token for stable uptime rewards
- [ ] OCTO-S token for proof archival costs
- [ ] PoR boosts for trusted routing
- [ ] RFC-0008 execution class mapping table
- [ ] `PoRelayError` enum with all error variants
- [ ] Unit tests: 12+ tests covering proof generation, trust computation, slashing
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/porelay/`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P
- Mission 0851: GDP Gateway Discovery
- Mission 0853: OCrypt Overlay Cryptography
- Mission 0854: DPS Deterministic Proof Substrate

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Trust score computation MUST be Class A — all nodes derive identical scores from identical proof sets
- OCTO-S for proof archival: proofs are stored, not just verified
- Slashing triggers: invalid proof (immediate), downtime > threshold (gradual), malicious routing (immediate)
- Integration with RFC-0650 (Proof Aggregation) for recursive relay proofs

## Reference

- RFC-0860: Proof-of-Relay (§4, §5, §6, §7)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree)
- `docs/04-tokenomics/token-design.md` (OCTO-B/N/S tokens)
