# Mission: GDP Gateway Discovery

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP)

## Summary

Implement gateway discovery with advertisements, capability Merkle commitments, heartbeat monitoring, and deterministic cache eviction.

## Acceptance Criteria

- [ ] `GatewayIdentity` extends DOT's identity with gateway_id, public_key, network_id, gateway_class, creation_epoch
- [ ] `GatewayAdvertisement` with version, gateway_id, network_id, sequence, logical_timestamp, gateway_class, capabilities_root, transport_root, route_root, trust_root, overlay_endpoints, signature
- [ ] `GatewayCapability` enum (bitmask) with Edge(0x0001), Relay(0x0002), Consensus(0x0004), Archive(0x0008), Stealth(0x0010), Translation(0x0020), Storage(0x0040), OnionRelay(0x0080), AIExecution(0x0100), VectorIndex(0x0200), ZkVerification(0x0400), MissionCoordinator(0x0800)
- [ ] `GatewayHeartbeat` with gateway_id, sequence, active_routes, load_class, uptime_class, logical_timestamp, signature (7 fields per RFC-0860 §2.2)
- [ ] `GatewayCache` with BTreeMap, deterministic eviction by (trust, utility, age)
- [ ] Discovery lifecycle: bootstrap → expansion → stabilization
- [ ] Heartbeat timeout detection after N missed heartbeats
- [ ] `GdpError` enum with all error variants
- [ ] Unit tests: 10+ tests covering advertisement serialization, cache eviction, heartbeat timeout
- [ ] `cargo fmt -- --check` passes
- [ ] `cargo test -p octo-network` passes

## Location

`crates/octo-network/src/gdp/`

## Complexity

High

## Prerequisites

- Mission 0850: DOT Core Envelope and Native P2P

## Implementation Notes

- See `docs/07-developers/networking-implementation-guide.md` for concrete Rust code
- Gateway identity extends DOT's `GatewayIdentity` struct
- All Merkle commitments use BLAKE3-256
- Cache eviction uses BTreeMap for deterministic iteration
- GatewayClass enum values: Edge=0x0001, Relay=0x0002, Consensus=0x0003, Archive=0x0004, Stealth=0x0005, Translation=0x0006

## Reference

- RFC-0851: Gateway Discovery Protocol (§3, §4, §5, §12)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree, Error Types)
