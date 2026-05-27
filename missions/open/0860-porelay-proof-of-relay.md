# Mission: PoRelay Proof-of-Relay

## Status

Open

## RFC

RFC-0860: Proof-of-Relay (PoRelay)

## Summary

Implement proof-of-relay with relay proofs (forwarding, availability, bandwidth, uptime), gateway heartbeat, trust score computation, slashing conditions, and OCTO-S token integration for proof archival.

## Acceptance Criteria

- [ ] `ForwardingProof` with envelope_hash, relay_gateway_id, next_hop_gateway_id, hop_index, timestamp, signature (6 fields per RFC §4.1)
- [ ] `AvailabilityProof` with gateway_id, epoch_start, epoch_end, availability_score, signature (5 fields per RFC §4.2)
- [ ] `BandwidthProof` with gateway_id, epoch, bytes_relayed, bandwidth_class, signature (5 fields per RFC §4.3)
- [ ] `UptimeProof` with gateway_id, epoch, consecutive_epochs, signature (4 fields per RFC §4.4)
- [ ] `RelayScore` with gateway_id, epoch, forwarding_score, availability_score, bandwidth_score, uptime_score, diversity_bonus, stake_multiplier, composite (9 fields per RFC §5.2)
- [ ] `AggregatedRelayProof` with constituent_proofs, aggregation_root, relay_count (3 fields per RFC §7)
- [ ] `GatewayAdvertisementWithPoR` extending GatewayAdvertisement with relay_proofs (RFC §3.1)
- [ ] `GatewayHeartbeat` with gateway_id, sequence, active_routes, load_class, uptime_class, logical_timestamp, signature (7 fields per RFC §2.2)
- [ ] Trust score integration: RelayScore feeds into TrustScore.proof_of_relay (RFC-0856 §9.1)
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
