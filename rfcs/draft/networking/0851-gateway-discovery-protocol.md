---
title: "RFC-0851: Gateway Discovery Protocol (GDP)"
status: Draft
version: 1.0.0
created: 2026-05-25
updated: 2026-05-25
authors:
  - CipherOcto Core Team
related:
  - RFC-0850 (Networking): Deterministic Overlay Transport
  - RFC-0843 (Networking): OCTO-Network Protocol
  - RFC-0009 (Process): Identity Management
  - RFC-0126 (Numeric): Deterministic Serialization
---

# RFC-0851: Gateway Discovery Protocol (GDP)

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

The Gateway Discovery Protocol (GDP) defines how CipherOcto nodes discover gateways, advertise capabilities, establish overlay topology, exchange route metadata, negotiate transport compatibility, maintain deterministic peer visibility, resist Sybil and eclipse attacks, and bootstrap decentralized mission overlays.

GDP is the overlay equivalent of:

| Internet Analogy | GDP Equivalent |
|-----------------|----------------|
| DNS | Gateway identity resolution |
| BGP | Overlay route advertisement |
| ARP | Local overlay discovery |
| DHT bootstrap | Initial peer acquisition |
| mDNS | Local opportunistic discovery |

GDP extends RFC-0843 (OCTO-Network Protocol) peer discovery with overlay-specific gateway capabilities, transport advertisements, and deterministic discovery ordering.

## Dependencies

**Requires:**

- RFC-0850 (Networking): DOT — gateway identity, broadcast domains
- RFC-0843 (Networking): OCTO-Network Protocol — base peer discovery
- RFC-0009 (Process): Identity Management — identity model
- RFC-0126 (Numeric): Deterministic Serialization — canonical encoding

**Optional:**

- RFC-0860 (Networking): Proof-of-Relay — trust scoring integration

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1: Sovereign Discovery | No centralized registry | Zero single points of failure |
| G2: Deterministic Visibility | Canonical discovery ordering | Identical output from identical input |
| G3: Platform Independence | Transport-agnostic | Works across all DOT carriers |
| G4: Byzantine Tolerance | Adversarial-resistant | Survive 33% malicious gateways |
| G5: Opportunistic Networking | Dynamic route acquisition | <5s discovery time |
| G6: Partition Recovery | Autonomous healing | Convergence after partition |
| G7: Replay Safety | Canonical advertisements | Zero replay acceptance |
| G8: Scalable Federation | Millions of gateways | Sublinear overhead |

## Motivation

### CAN WE? — Feasibility Research

Research confirms feasibility through:

- **Kademlia DHT** (RFC-0843) provides battle-tested peer discovery at scale
- **OpenClaw channel adapters** demonstrate multi-platform gateway discovery
- **BGP route advertisement** provides proven model for distributed route propagation
- **mDNS/Bonjour** demonstrates local network discovery without central authority

### WHY? — Why This Matters

Without GDP:

- Gateways cannot find each other — overlay network cannot form
- New nodes cannot bootstrap — network cannot grow
- Route information cannot propagate — deterministic routing fails
- Sybil attacks are undetectable — network integrity compromised

## Specification

### 1. Gateway Identity

Every gateway possesses a sovereign cryptographic identity (extends RFC-0850 Section 3.2):

```rust
struct GatewayIdentity {
    gateway_id: [u8; 32],       // SHA-256(public_key || network_id || creation_epoch)
    public_key: [u8; 32],       // Ed25519 public key
    network_id: u32,
    gateway_class: u16,
    creation_epoch: u64,
}
```

### 2. Discovery Scope

```rust
enum DiscoveryScope {
    Local = 0x0001,       // Same broadcast domain
    Regional = 0x0002,    // Geographic/latency region
    Mission = 0x0003,     // Temporary overlay visibility
    Global = 0x0004,      // Entire DOT mesh
    Private = 0x0005,     // Invite-only discovery
}
```

### 3. Discovery Plane vs Data Plane

| Plane | Function | Protocol |
|-------|----------|----------|
| Discovery Plane | Gateway visibility/topology | GDP |
| Data Plane | Actual envelope routing | DOT (RFC-0850) |

### 4. Gateway Advertisement (GADV)

```rust
struct GatewayAdvertisement {
    version: u16,
    gateway_id: [u8; 32],
    network_id: u32,
    sequence: u64,                  // Strictly monotonic per gateway
    logical_timestamp: u64,
    gateway_class: u16,
    capabilities_root: [u8; 32],    // Merkle root of capabilities
    transport_root: [u8; 32],       // Merkle root of transport endpoints
    route_root: [u8; 32],           // Merkle root of route vectors
    trust_root: [u8; 32],           // Trust score commitment (RFC-0860)
    overlay_endpoints: Vec<OverlayEndpoint>,
    signature: [u8; 64],
}
```

**Canonicalization:** Endpoint ordering by `(transport_type, endpoint_hash)`, capability ordering by enum value, route ordering by `(destination, next_hop)` — all lexicographic.

### 5. Capability Advertisement

```rust
enum GatewayCapability {
    Relay = 0x0001,
    Consensus = 0x0002,
    Storage = 0x0003,
    Archive = 0x0004,
    OnionRelay = 0x0005,
    Translation = 0x0006,
    AIExecution = 0x0007,
    VectorIndex = 0x0008,
    ZkVerification = 0x0009,
    MissionCoordinator = 0x000A,
}
```

### 6. Transport Advertisement

```rust
struct OverlayEndpoint {
    transport_type: u16,        // Per RFC-0850 platform types
    endpoint_hash: [u8; 32],    // SHA-256 of platform endpoint ID
    priority: u16,              // Lower = preferred
    bandwidth_class: u16,       // 0-255
    flags: u64,
}
```

### 7. Route Vector

```rust
struct RouteVector {
    destination_gateway: [u8; 32],
    next_hop: [u8; 32],
    hop_count: u16,
    trust_weight: u32,
    latency_class: u16,
    bandwidth_class: u16,
}
```

**Deterministic scoring:** `score = trust_weight * 0.5 + bandwidth_class * 0.3 + latency_class * 0.2`

### 8. Discovery Lifecycle

**Bootstrap:** Seed list → QR/blob → Local broadcast → Existing DOT domain → Trusted peers → Mission invitation

**Expansion:** Gateway A advertises peers → peer graph expands recursively (DHT-like)

**Stabilization:** Maintain preferred gateways, trust-weighted neighbors, route diversity, anti-eclipse diversity.

### 9. Discovery Ordering

All advertisements sorted by: `(network_id, gateway_id, sequence, advertisement_hash)`

Sequence MUST be strictly monotonic. Violation → rejection.

### 10. Gateway Cache

```rust
struct GatewayCacheEntry {
    advertisement_hash: [u8; 32],
    first_seen: u64,
    last_seen: u64,
    trust_score: u32,
    identity: GatewayIdentity,
    capabilities: Vec<GatewayCapability>,
    endpoints: Vec<OverlayEndpoint>,
}
```

**Deterministic eviction:** lowest trust → oldest unseen → lowest route utility.

### 11. Anti-Sybil Mechanisms

- Stake-gated discovery (minimum OCTO-B for global propagation)
- Diversity constraints: transport, geographic, organizational, trust-source

### 12. Failure Detection

Gateway considered degraded after N missed heartbeats (default: 3, network-configured).

### 13. Discovery Gossip

| Mode | Use |
|------|-----|
| Flood | Bootstrap |
| Incremental | Normal operation |
| Anti-entropy | State healing (Merkle exchange) |
| Directed sync | Mission overlays |

## Performance Targets

| Metric | Target |
|--------|--------|
| Bootstrap discovery | <5s |
| Advertisement processing | <1ms |
| Cache lookup | <1µs |
| Route propagation | <2s |
| Heartbeat interval | 30s |
| Failure detection | <90s |
| Cache capacity | 10K gateways/node |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Sybil | High | Stake + PoR diversity |
| Eclipse | High | Diversity constraints |
| Replay | High | Sequence monotonicity |
| Poisoning | High | Signed advertisements |
| Enumeration | Medium | Scoped discovery |
| Partitioning | Medium | Multi-transport federation |

## Adversarial Review

| Threat | Impact | Mitigation |
|--------|--------|------------|
| 1000 Sybil gateways | High | Stake + diversity constraints |
| Advertisement replay | High | Sequence monotonicity |
| Eclipse via selective advertisement | High | Multi-transport diversity |
| Route poisoning | High | Signed route vectors |
| Cache exhaustion | Medium | Deterministic eviction |

## Implementation Phases

### Phase 1: Core Discovery (Months 1-3)
- GatewayAdvertisement with DCS serialization
- GatewayIdentity derivation
- CapabilityAdvertisement with Merkle commitment
- GatewayCache with deterministic eviction
- Bootstrap via seed list

### Phase 2: Route Propagation (Months 3-5)
- RouteVector with deterministic scoring
- Incremental gossip propagation
- Anti-entropy reconciliation
- Integration with DOT route computation

### Phase 3: Health and Trust (Months 5-8)
- GatewayHeartbeat generation/verification
- Failure detection
- RFC-0860 trust scoring integration
- Diversity constraint enforcement

### Phase 4: Advanced Discovery (Months 8-12)
- Local broadcast (mDNS)
- Mission-scoped discovery
- Private/invite-only discovery
- Stake-gated global propagation

## Economic Analysis

### Token Integration

| Activity | Token | Rationale |
|----------|-------|-----------|
| Relay availability | OCTO-B | Bandwidth for advertisement propagation |
| Discovery coordination | OCTO-O | Orchestration of discovery protocols |
| Stable uptime | OCTO-N | Node operation and availability |
| Trusted routing | PoR boosts | Reputation-weighted discovery priority |

### Gateway Economics

Gateways earn discovery rewards for:

- Maintaining uptime (heartbeat consistency)
- Propagating advertisements reliably
- Providing accurate route metadata
- Supporting diverse transport carriers

### Stake Requirements

Global advertisement propagation requires minimum stake:

```text
min_stake = base_stake * (1 + discovery_scope_multiplier)
```

Where `discovery_scope_multiplier` scales with visibility scope (LOCAL=0, REGIONAL=0.5, MISSION=1.0, GLOBAL=2.0).

## Compatibility

### RFC-0843 Integration

GDP extends RFC-0843's peer discovery with overlay-specific features:

- RFC-0843 uses Kademlia DHT for native P2P discovery
- GDP adds platform-agnostic gateway discovery across heterogeneous transports
- Gateway advertisements can propagate via DOT carriers (RFC-0850)
- GDP discovery state can be synchronized via DGP (RFC-0852)

### Forward Compatibility

- Gateway class enum is extensible (values 0x0040-0xFFFF for future roles)
- Capability flags are extensible (bitmask allows 64 capability types)
- Discovery scopes are extensible (values 0x0006-0xFFFF for future scopes)

## Test Vectors

### Gateway Advertisement Serialization

```text
Input:
  version = 1
  gateway_id = [0x01; 32]
  network_id = 0x00000001
  sequence = 1
  logical_timestamp = 1000
  gateway_class = 0x0001 (Edge)
  capabilities_root = SHA-256(capability_entries)
  transport_root = SHA-256(transport_entries)
  route_root = SHA-256(route_entries)
  trust_root = SHA-256(trust_entries)
  overlay_endpoints = [endpoint_1]

Expected: Canonical DCS bytes with fields in declaration order
```

### Deterministic Discovery Ordering

```text
Gateway A: network_id=1, gateway_id=[0x01;32], sequence=1, hash=SHA-256("A")
Gateway B: network_id=1, gateway_id=[0x01;32], sequence=1, hash=SHA-256("B")
Gateway C: network_id=1, gateway_id=[0x01;32], sequence=2, hash=SHA-256("C")

Canonical order: A < B < C
Reason: A and B have same (network_id, gateway_id, sequence), A.hash < B.hash
        C has higher sequence than both
```

### Cache Eviction Order

```text
Cache entries:
  X: trust=100, last_seen=1000, utility=50
  Y: trust=50,  last_seen=2000, utility=80
  Z: trust=100, last_seen=1500, utility=30

Eviction order: Y → Z → X
Reason: Y has lowest trust (50), then Z has lower utility (30), then X
```

## Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| Kademlia DHT only (RFC-0843) | Proven, efficient | No platform-agnostic discovery | Supplemented by GDP |
| Centralized registry | Simple | Single point of failure, censorship | Rejected |
| Blockchain-based discovery | Decentralized | High latency, expensive | Too slow for real-time |
| mDNS-only | Zero configuration | Local only, no federation | Insufficient reach |
| DNS-based discovery | Familiar model | Centralized, requires infrastructure | Rejected |

**Decision:** GDP provides sovereign, deterministic discovery that extends RFC-0843's DHT with overlay-specific capabilities.

## Rationale

### Why separate from RFC-0843?

RFC-0843's Kademlia DHT is optimized for native P2P. GDP adds:

1. Platform-agnostic gateway discovery (not just libp2p peers)
2. Deterministic advertisement ordering for consensus safety
3. Capability and transport advertisement for heterogeneous routing
4. Mission-scoped discovery for temporary overlays
5. Stake-gated global propagation for Sybil resistance

### Why Merkle-committed capabilities?

Without Merkle commitments:

1. Gateways could advertise capabilities and silently revoke them
2. Capability verification requires trusting the gateway
3. No audit trail for capability changes

Merkle roots enable lightweight verification without trusting the gateway.

### Why deterministic cache eviction?

Non-deterministic eviction (LRU with wall-clock timestamps) would cause:

1. Different nodes evict different entries
2. Discovery state diverges across the network
3. Consensus on gateway topology becomes impossible

Deterministic eviction by (trust, utility, age) ensures convergence.

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-network/src/gdp/mod.rs` | GDP module root |
| `crates/octo-network/src/gdp/advertisement.rs` | GatewayAdvertisement |
| `crates/octo-network/src/gdp/identity.rs` | GatewayIdentity |
| `crates/octo-network/src/gdp/capabilities.rs` | CapabilityAdvertisement |
| `crates/octo-network/src/gdp/transport.rs` | TransportAdvertisement |
| `crates/octo-network/src/gdp/route.rs` | RouteVector |
| `crates/octo-network/src/gdp/cache.rs` | GatewayCache |
| `crates/octo-network/src/gdp/heartbeat.rs` | GatewayHeartbeat |
| `crates/octo-network/src/gdp/gossip.rs` | Discovery gossip |
| `crates/octo-network/src/gdp/bootstrap.rs` | Bootstrap discovery |

## Future Work

- F1: Hierarchical discovery (regional → global)
- F2: Stealth discovery (encrypted advertisements)
- F3: Partial topology disclosure
- F4: Merkleized topology snapshots for forensic auditing

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-25 | Initial draft |

## Related RFCs

- RFC-0850 (Networking): DOT — gateway identity, broadcast domains
- RFC-0843 (Networking): OCTO-Network Protocol — base peer discovery
- RFC-0852 (Networking): DGP — gossip propagation
- RFC-0856 (Networking): DRS — route selection
- RFC-0860 (Networking): PoRelay — trust scoring

## Related Use Cases

- [Decentralized Mission Execution](../../docs/use-cases/decentralized-mission-execution.md)
- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Node Operations](../../docs/use-cases/node-operations.md)
