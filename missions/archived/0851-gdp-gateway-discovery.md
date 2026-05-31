# Mission: GDP Gateway Discovery

## Status

Open

## RFC

RFC-0851: Gateway Discovery Protocol (GDP)

## Summary

Implement gateway discovery with advertisements, capability Merkle commitments, heartbeat monitoring, and deterministic cache eviction.

## Type Coverage (C-GDP-6 fix)

| RFC-0851 Type | Implemented By |
|---------------|---------------|
| GatewayIdentity | This mission (extends RFC-0850 §3.2) |
| GatewayAdvertisement | This mission |
| GatewayCapability | This mission |
| GatewayHeartbeat | This mission (references RFC-0860 §2.2) |
| GatewayCapacity | This mission (from RFC-0850) |
| GdpError | This mission |
| DiscoveryScope | Mission 0851a |
| DiscoveryLifecycle | Mission 0851a |
| OverlayEndpoint | This mission |
| AdvertisementExpiration | This mission |
| StakeRequirement | Mission 0851b |
| DiversityScore | Mission 0851b |

## Acceptance Criteria

- [x] `GatewayIdentity` extends DOT's identity with gateway_id, public_key, network_id, gateway_class, creation_epoch
- [x] `GatewayAdvertisement` with version, gateway_id, network_id, sequence, logical_timestamp, gateway_class, capabilities_root, transport_root, route_root, trust_root, overlay_endpoints, signature
- [x] `GatewayCapability` enum (bitmask) with Edge(0x0001), Relay(0x0002), Consensus(0x0004), Archive(0x0008), Stealth(0x0010), Translation(0x0020), Storage(0x0040), OnionRelay(0x0080), AIExecution(0x0100), VectorIndex(0x0200), ZkVerification(0x0400), MissionCoordinator(0x0800)
- [x] `GatewayHeartbeat` references RFC-0860 §2.2 canonical definition (7 fields: gateway_id, sequence, active_routes, load_class, uptime_class, logical_timestamp, signature)
- [x] `GatewayCache` with BTreeMap, deterministic eviction by (trust, utility, age)
- [x] Discovery lifecycle: bootstrap → expansion → stabilization
- [x] Heartbeat timeout detection after N missed heartbeats
- [x] `GdpError` enum with all error variants
- [x] Unit tests: 59 tests covering advertisement serialization, cache eviction, heartbeat timeout, stake gating, diversity, sybil detection, discovery gossip
- [x] `cargo fmt -- --check` passes
- [x] `cargo test -p octo-network` passes (643 tests)

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
- GatewayClass enum (from RFC-0850 §3.2): Edge=0x0001, Relay=0x0002, Consensus=0x0003, Archive=0x0004, Stealth=0x0005, Translation=0x0006
- GatewayCapability bitmask (Section 5): Edge=0x0001, Relay=0x0002, Consensus=0x0004, Archive=0x0008, Stealth=0x0010, Translation=0x0020, Storage=0x0040, OnionRelay=0x0080, AIExecution=0x0100, VectorIndex=0x0200, ZkVerification=0x0400, MissionCoordinator=0x0800
- Note: GatewayClass is a `#[repr(u16)]` enum for classification. GatewayCapability is a `#[repr(u64)]` bitmask for multi-role capabilities.

## Reference

- RFC-0851: Gateway Discovery Protocol (§3, §4, §5, §12)
- `docs/07-developers/networking-implementation-guide.md` (Module Tree, Error Types)
