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
| G4: Byzantine Tolerance | Adversarial-resistant | Survive f < n/3 malicious gateways (BFT threshold) |
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

GDP uses the canonical `GatewayIdentity` struct defined in RFC-0850 Section 3.2. The `gateway_class` field uses the `GatewayClass` enum (not raw `u16`). Any field additions MUST be made in RFC-0850 and referenced here.

**C-GDP-3 fix:** RFC-0851 now references RFC-0850's `GatewayClass` enum for `gateway_class` instead of raw `u16`. The implementation at `crates/octo-network/src/dot/gateway.rs` uses `GatewayClass` consistently.

### 2. Discovery Scope

GDP defines discovery scopes for gateway visibility. These are the **canonical** discovery scopes used by all overlay protocols.

```rust
#[repr(u16)]
enum DiscoveryScope {
    Local = 0x0001,       // Same broadcast domain
    Regional = 0x0002,    // Geographic/latency region
    Mission = 0x0003,     // Temporary overlay visibility
    Global = 0x0004,      // Entire DOT mesh
    Private = 0x0005,     // Invite-only discovery
    Consensus = 0x0006,   // Validator/consensus node discovery
}
```

**C-GDP-1 fix — DiscoveryScope vs MON MissionDiscoveryScope:**

RFC-0855 (MON) defines `MissionDiscoveryScope` (a separate enum with different semantics) for mission-specific visibility. MON's scopes describe **who can discover a mission**, while GDP's scopes describe **how broadly a gateway advertises**. The mapping is:

| MON MissionDiscoveryScope | GDP DiscoveryScope | Notes |
|---------------------------|-------------------|-------|
| Public | Global | Mission discoverable by all gateways |
| InviteOnly | Private | Mission discoverable only by invited gateways |
| Stealth | Private + stealth flag | Mission existence hidden, requires discovery key |
| Federated | Regional | Mission discoverable within trusted federation |
| Ephemeral | Mission | Mission discoverable within mission lifetime |

MON uses a separate `MissionDiscoveryScope` enum (`#[repr(u16)]` starting at `0x0100`) to avoid discriminant collision.

**C-GDP-5 fix — DiscoveryScope vs RouteScopeFlag (RFC-0856):**

RFC-0856 defines `RouteScopeFlag` as a `#[repr(u64)]` bitmask. GDP's `DiscoveryScope` uses `#[repr(u16)]` sequential discriminants. The mapping between them is:

| GDP DiscoveryScope | DRS RouteScopeFlag | Type | Notes |
|-------------------|-------------------|------|-------|
| Local (0x0001) | Local (0x0010) | u16 enum → u64 bit | Different repr, different values |
| Regional (0x0002) | Regional (0x0002) | u16 enum → u64 bit | Same value, different types |
| Mission (0x0003) | Mission (0x0004) | u16 enum → u64 bit | Different values |
| Global (0x0004) | Global (0x0001) | u16 enum → u64 bit | Different values |
| Private (0x0005) | Private (0x0008) | u16 enum → u64 bit | Different values |
| Consensus (0x0006) | Consensus (0x0020) | u16 enum → u64 bit | Different values |

**Conversion function:** `route_scope_from_discovery(scope: DiscoveryScope) -> RouteScopeFlag` MUST be defined in the implementation. GDP scope is for discovery visibility; DRS scope is for route domain isolation. Both are needed but serve different purposes.

**C-GDP-2 fix — RFC-0008 Execution Class Mapping:**

| GDP Operation | Execution Class | Rationale |
|---------------|-----------------|-----------|
| Advertisement serialization | Class A | Consensus-critical identity |
| Advertisement signature verification | Class A | Consensus-critical validation |
| Discovery ordering by (network_id, gateway_id, sequence, hash) | Class A | Consensus-critical ordering |
| Cache eviction ordering | Class A | Consensus-critical state |
| Heartbeat verification | Class A | Consensus-critical liveness |
| Heartbeat failure detection | Class A | Consensus-critical liveness |
| Route scoring (integer arithmetic) | Class A | Consensus-critical scoring |
| Gateway discovery (finding gateways) | Class B | Configurable timeouts |
| Discovery lifecycle transitions | Class B | Configurable timeouts |
| Advertisement propagation | Class C | Transport-dependent |
| Platform adapter I/O | Class C | Inherently non-deterministic |

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

**Merkle Root Computation:**

All Merkle roots (`capabilities_root`, `transport_root`, `route_root`, `trust_root`) use the same algorithm:

1. Canonicalize the items (sort by the ordering rules above)
2. Compute leaf hashes: `leaf_i = BLAKE3-256(canonical_bytes(item_i))`
3. Build binary Merkle tree: `parent = BLAKE3-256(left_child || right_child)`
4. If odd number of leaves, duplicate the last leaf
5. The root is the hash of the final parent node

**Empty sets:** If a set is empty, the Merkle root is `[0x00; 32]` (all zeros).

### 5. Capability Advertisement

```rust
#[repr(u64)]
enum GatewayCapability {
    // Base capabilities (inherited from RFC-0850 GatewayRoleFlags)
    Edge = 0x0001,
    Relay = 0x0002,
    Consensus = 0x0004,
    Archive = 0x0008,
    Stealth = 0x0010,
    Translation = 0x0020,

    // GDP-specific extensions (HIGH-1 fix: aligned with RFC-0850 base)
    Storage = 0x0040,
    OnionRelay = 0x0080,
    AIExecution = 0x0100,
    VectorIndex = 0x0200,
    ZkVerification = 0x0400,
    MissionCoordinator = 0x0800,
}
```

**HIGH-1 fix:** `GatewayCapability` now uses `#[repr(u64)]` bitmask positions aligned with RFC-0850's `GatewayRoleFlags` for the base 6 (Edge through Translation). GDP-specific extensions use higher bit positions (0x0040+). This resolves the contradiction between Section 6, M-GDP-10, and RFC-0850.

### 6. Transport Advertisement

```rust
struct OverlayEndpoint {
    transport_type: u16,        // Per RFC-0850 platform types
    endpoint_hash: [u8; 32],    // BLAKE3-256 of platform endpoint ID
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

**Deterministic scoring (integer-only):** `score = trust_weight * 5 + bandwidth_class * 3 + latency_class * 2`

**Note:** All weights are integers multiplied by 10 to eliminate floating-point arithmetic. Per RFC-0008, consensus-critical scoring MUST use Class A (Protocol Deterministic) operations. Floating-point is forbidden. See RFC-0105 (DQA) for the deterministic numeric foundation.

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
    /// MUST be sorted by enum value (ascending) for deterministic Merkle computation
    capabilities: Vec<GatewayCapability>,
    /// MUST be sorted by (transport_type, endpoint_hash) for deterministic Merkle computation
    endpoints: Vec<OverlayEndpoint>,
}
```

**Determinism Requirement:** `capabilities` MUST be sorted by `GatewayCapability` enum value (ascending) and `endpoints` MUST be sorted by `(transport_type, endpoint_hash)` (lexicographic) before Merkle root computation. This ensures all nodes compute identical `capabilities_root` and `transport_root` for the same advertisement.

**Deterministic eviction:** Uses the composite eviction formula defined in Section 13 (M-GDP-2). Lower eviction_score → evicted first. Ties broken by lexicographic `gateway_id`.

### 11. Anti-Sybil Mechanisms

GDP MUST assume adversarial gateway creation. Anti-sybil integrates with RFC-0860 (PoRelay) Section 6.

**11.1 Stake-Gated Discovery**

Minimum stake required per discovery scope (per BLUEPRINT dual-stake model):

| Scope | OCTO Global Stake | OCTO-B Role Stake | Rationale |
|-------|-------------------|-------------------|-----------|
| Local | 0 | 0 | Same broadcast domain, trust assumed |
| Regional | 500 OCTO | 50 OCTO-B | Moderate barrier |
| Mission | Mission-defined | Mission-defined | Per MON governance |
| Global | 1,000 OCTO | 100 OCTO-B | Highest barrier, Sybil-resistant |
| Private | Invite-only | Inviter-determined | Trust delegated to inviter |
| Consensus | 1,000 OCTO | 200 OCTO-B | Validator-grade trust required |

The 1,000 OCTO global minimum comes from `docs/01-foundation/whitepaper/v1.0-whitepaper.md` §11.2.4 (Dual-Stake Anti-Sybil model).

**11.2 Diversity Constraints**

Gateways MUST maintain diversity across:

| Dimension | Metric | Minimum Threshold |
|-----------|--------|-------------------|
| Transport | Number of distinct platform types | ≥ 2 for Regional, ≥ 3 for Global |
| Geographic | Number of distinct regions | ≥ 2 for Global |
| Trust-source | Number of distinct trust attestors | ≥ 2 for Regional |

Non-compliant gateways are deprioritized (not rejected) in discovery ordering. Diversity score: `diversity_score = transport_diversity * 3 + geographic_diversity * 2 + trust_diversity * 1`.

**11.3 Rejection Behavior**

| Condition | Action |
|-----------|--------|
| Stake below minimum for scope | Advertisement silently dropped |
| Zero diversity for Global scope | Advertisement deprioritized (score = 0) |
| Known Sybil cluster (correlated behavior) | All cluster members deprioritized |

**11.4 Integration with RFC-0860**

GDP's `trust_root` Merkle commitment references RFC-0860 Section 2.2 `RelayScore`. Gateways with higher PoRelay trust scores receive higher discovery priority.

### 12. Gateway Heartbeat and Failure Detection

**H-GDP-1 fix:** The canonical `GatewayHeartbeat` struct is defined in RFC-0860 Section 2.2. GDP references RFC-0860's definition and specifies only the processing rules here.

**GDP Heartbeat Processing Rules:**

- Heartbeats are processed in `(gateway_id, sequence)` order
- Duplicate or out-of-order heartbeats MUST be rejected
- Gateway considered degraded after N missed heartbeats (default: 3, network-configured)
- Heartbeat interval: 30s (default)
- Failure detection: 90s (3 × 30s)
- On failure detection: gateway removed from active discovery, trust score decremented

**H-GDP-2 fix — GatewayCapacity Integration:**

RFC-0850's `GatewayCapacity` is conveyed as part of the capability Merkle tree under `capabilities_root`. Gateways include capacity information as entries in the capabilities Merkle tree. Discovering gateways can verify capacity claims against observed behavior.

**H-GDP-3 fix — GdpError Enum:**

```rust
#[derive(Clone, Debug)]
enum GdpError {
    // Advertisement errors
    InvalidAdvertisement { reason: &'static str },
    StaleSequence { got: u64, minimum: u64 },
    ReplayDetected { gateway_id: [u8; 32], sequence: u64 },
    InvalidSignature { gateway_id: [u8; 32] },

    // Capability errors
    CapabilityMismatch { required: u64, available: u64 },
    InsufficientStake { scope: u16, required: u64, available: u64 },

    // Cache errors
    CacheFull { max_entries: u32 },

    // Heartbeat errors
    HeartbeatTimeout { gateway_id: [u8; 32], missed: u32 },
    HeartbeatOutOfOrder { gateway_id: [u8; 32], got: u64, expected: u64 },

    // Discovery errors
    ScopeNotPermitted { scope: u16, stake: u64 },
    DiversityViolation { dimension: &'static str, required: u32, actual: u32 },
}
```

### 13. Discovery Gossip

**H-GDP-5 fix — DGP Integration:**

GDP advertisements are propagated as DGP `DiscoveryAdvertisement` objects (RFC-0852 Section 3). The integration is:

| GDP DiscoveryScope | DGP GossipDomainId.scope | Default TTL |
|-------------------|-------------------------|-------------|
| Local | LOCAL | 3 hops |
| Regional | REGIONAL | 10 hops |
| Mission | MISSION | 5 hops |
| Global | GLOBAL | 20 hops |
| Private | PRIVATE | 3 hops |
| Consensus | CONSENSUS | 10 hops |

GDP advertisements wrap into DGP `GossipObject` with `object_type = DiscoveryAdvertisement` and `domain_id` derived from `DiscoveryScope`.

**M-GDP-8 fix — Advertisement Expiration:**

Advertisements expire after `logical_timestamp + EXPIRY_EPOCHS` (default: 100 epochs). Expired advertisements are removed from cache during purge. This supplements sequence monotonicity with temporal bounds.

**M-GDP-6 fix — Consensus Discovery:**

Consensus gateways use `DiscoveryScope::Consensus` (0x0006) for validator discovery. This maps to DGP's CONSENSUS gossip domain. Consensus discovery has higher stake requirements (1,000 OCTO + 200 OCTO-B) and requires validator-grade trust.

**M-GDP-3 fix — Lifecycle States:**

```rust
#[repr(u16)]
enum DiscoveryLifecycle {
    Bootstrap = 0x0001,     // < 5 known gateways, flood mode
    Expansion = 0x0002,     // Growing peer graph, incremental gossip
    Stabilization = 0x0003, // Steady state, trust-weighted neighbors
    Degraded = 0x0004,      // Partition detected, anti-entropy mode
    Recovering = 0x0005,    // Healing after partition
}
```

Transition conditions:
- Bootstrap → Expansion: ≥ 5 known gateways
- Expansion → Stabilization: < 10% new gateways per epoch
- Any → Degraded: > 33% of known gateways unreachable
- Degraded → Recovering: anti-entropy reconciliation succeeds
- Recovering → Stabilization: < 5% divergence in Merkle summaries

**Gossip Modes:**

| Mode | Use | When |
|------|-----|------|
| Flood | Bootstrap | Node startup, < 5 known gateways |
| Incremental | Normal operation | Steady state, propagate new/updated advertisements |
| Anti-entropy | State healing | Every 60s (default), Merkle summary exchange |
| Directed sync | Mission overlays | On mission join, sync mission-scoped gateways |

**M-GDP-2 fix — Cache Eviction Formula:**

```text
eviction_score = trust_score * 10 + utility_score * 5 + recency_score * 2
```

Where:
- `trust_score` = RFC-0860 `RelayScore.composite` (0-1000)
- `utility_score` = number of routes using this gateway in last 100 epochs (0-1000)
- `recency_score` = 1000 - min(1000, current_epoch - last_seen)

Lower eviction_score → evicted first. Ties broken by lexicographic `gateway_id`.

**M-GDP-1 fix — OverlayEndpoint:**

`OverlayEndpoint` is defined in RFC-0851 Section 6 (not RFC-0850). It represents a platform-specific transport endpoint for gateway communication. |

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

**MEDIUM-1 fix — Stake values reconciled with Section 11.1:**

The authoritative stake requirements are defined in Section 11.1 (Anti-Sybil Mechanisms). This section references them for economic analysis. The dual-stake model requires both OCTO global stake and OCTO-B role stake:

| Scope | OCTO Global Stake | OCTO-B Role Stake | Source |
|-------|-------------------|-------------------|--------|
| Local | 0 | 0 | §11.1 |
| Regional | 500 OCTO | 50 OCTO-B | §11.1 |
| Mission | Mission-defined | Mission-defined | MON governance |
| Global | 1,000 OCTO | 100 OCTO-B | §11.1 (whitepaper S11.2.4) |
| Private | Invite-only | Inviter-determined | §11.1 |
| Consensus | 1,000 OCTO | 200 OCTO-B | §11.1 |

**Note:** All arithmetic is integer-only per RFC-0008 Class A requirements. Floating-point is forbidden for consensus-critical operations. The 1,000 OCTO global minimum comes from `docs/01-foundation/whitepaper/v1.0-whitepaper.md` §11.2.4.

**M-GDP-4 fix — Economic Integration:**

GDP economic integration follows RFC-0850 Section 13 and RFC-0860 Section 7. Gateway discovery economics are governed by:

- **Advertisement relay:** OCTO-B per advertisement relayed (per RFC-0850 bandwidth model)
- **Discovery coordination:** OCTO-O for orchestration of discovery protocols
- **Gateway uptime:** OCTO-N for stable heartbeat maintenance
- **Trust integration:** RFC-0860 PoRelay trust scores influence discovery priority
- **Fee splitting:** Discovery relay earns 10% of the DOT bandwidth fee for advertisements

**M-GDP-10 fix — GatewayCapability vs GatewayRoleFlags:**

GDP's `GatewayCapability` extends RFC-0850's `GatewayRoleFlags` bitmask. The base 6 capabilities (Edge=0x0001 through Translation=0x0020) are inherited from RFC-0850. GDP adds 6 extensions:

| Capability | Bit Position | Description |
|-----------|-------------|-------------|
| Storage | 0x0040 | Decentralized storage gateway |
| OnionRelay | 0x0080 | Onion routing relay node |
| AIExecution | 0x0100 | AI inference gateway |
| VectorIndex | 0x0200 | Vector search endpoint |
| ZkVerification | 0x0400 | ZK proof verification |
| MissionCoordinator | 0x0800 | Mission lifecycle management |

**Note:** Bitmask positions (u64) and MissionDiscoveryScope discriminants (u16) are independent namespaces. GDP extensions use 0x0040+ to avoid overlap with MissionDiscoveryScope range (0x0100-0x0105) where ZkVerification (0x0400) and MissionCoordinator (0x0800) are clearly separated.

**M-GDP-8 fix — Advertisement Expiration:**

Advertisements expire after `logical_timestamp + EXPIRY_EPOCHS` (default: 100 epochs, network-configurable). Expired advertisements are removed from cache during purge cycles. This supplements sequence monotonicity with temporal bounds and prevents stale gateway entries from accumulating.

**M-GDP-3 fix — Lifecycle States:**

```rust
#[repr(u16)]
enum DiscoveryLifecycle {
    Bootstrap = 0x0001,     // < 5 known gateways, flood mode
    Expansion = 0x0002,     // Growing peer graph, incremental gossip
    Stabilization = 0x0003, // Steady state, trust-weighted neighbors
    Degraded = 0x0004,      // Partition detected, anti-entropy mode
    Recovering = 0x0005,    // Healing after partition
}
```

Transition conditions:
- Bootstrap → Expansion: ≥ 5 known gateways
- Expansion → Stabilization: < 10% new gateways per epoch
- Any → Degraded: > 33% of known gateways unreachable
- Degraded → Recovering: anti-entropy reconciliation succeeds
- Recovering → Stabilization: < 5% divergence in Merkle summaries

**M-GDP-6 fix — Consensus Discovery:**

Consensus gateways use `DiscoveryScope::Consensus` (0x0006) for validator discovery. This maps to DGP's CONSENSUS gossip domain. Consensus discovery has higher stake requirements (1,000 OCTO + 200 OCTO-B) and requires validator-grade trust scores.

**M-GDP-2 fix — Cache Eviction Formula:**

```text
eviction_score = trust_score * 10 + utility_score * 5 + recency_score * 2
```

Where:
- `trust_score` = RFC-0860 `RelayScore.composite` (0-1000)
- `utility_score` = number of routes using this gateway in last 100 epochs (0-1000)
- `recency_score` = 1000 - min(1000, current_epoch - last_seen)

Lower eviction_score → evicted first. Ties broken by lexicographic `gateway_id`. This formula is deterministic (all integer arithmetic per RFC-0008 Class A) and produces consistent results across all nodes.

## Compatibility

### RFC-0843 Integration

GDP extends RFC-0843's peer discovery with overlay-specific features:

- RFC-0843 uses Kademlia DHT for native P2P discovery
- GDP adds platform-agnostic gateway discovery across heterogeneous transports
- Gateway advertisements can propagate via DOT carriers (RFC-0850)
- GDP discovery state can be synchronized via DGP (RFC-0852)

### Forward Compatibility

- Gateway class enum is extensible (values 0x0007-0x003F reserved for future roles, 0x0040-0xFFFF available)
- Capability flags are extensible (bitmask allows 64 capability types)
- Discovery scopes are extensible (values 0x0007-0x00FE for future scopes, 0x00FF-0xFFFF reserved)
- MissionDiscoveryScope (RFC-0855) uses 0x0100-0x0105 range to avoid collision

## Test Vectors

### Heartbeat Processing

```text
Heartbeat A: gateway_id=[0x01;32], sequence=1 → ACCEPTED (first heartbeat)
Heartbeat B: gateway_id=[0x01;32], sequence=1 → REJECTED (duplicate sequence)
Heartbeat C: gateway_id=[0x01;32], sequence=3 → REJECTED (out of order, expected 2)
Heartbeat D: gateway_id=[0x01;32], sequence=2 → ACCEPTED (next in sequence)
```

### Merkle Root Computation

```text
Empty set: root = [0x00; 32]

Single item: root = BLAKE3-256(canonical_bytes(item))

Two items: root = BLAKE3-256(leaf_0 || leaf_1)

Three items: root = BLAKE3-256(
    BLAKE3-256(leaf_0 || leaf_1),
    BLAKE3-256(leaf_2 || leaf_2)  // duplicate last leaf
)
```

### Stake Verification

```text
Gateway with 500 OCTO + 50 OCTO-B:
  - Regional scope: ACCEPTED (≥ 500 + 50)
  - Global scope: REJECTED (< 1000 + 100)

Gateway with 1000 OCTO + 100 OCTO-B:
  - Regional scope: ACCEPTED
  - Global scope: ACCEPTED
```

### Gateway Advertisement Serialization

```text
Input:
  version = 1
  gateway_id = [0x01; 32]
  network_id = 0x00000001
  sequence = 1
  logical_timestamp = 1000
  gateway_class = 0x0001 (Edge)
  capabilities_root = BLAKE3-256(capability_entries)
  transport_root = BLAKE3-256(transport_entries)
  route_root = BLAKE3-256(route_entries)
  trust_root = BLAKE3-256(trust_entries)
  overlay_endpoints = [endpoint_1]

Expected: Canonical DCS bytes with fields in declaration order
```

### Deterministic Discovery Ordering

```text
Gateway A: network_id=1, gateway_id=[0x01;32], sequence=1, hash=BLAKE3-256("A")
Gateway B: network_id=1, gateway_id=[0x01;32], sequence=1, hash=BLAKE3-256("B")
Gateway C: network_id=1, gateway_id=[0x01;32], sequence=2, hash=BLAKE3-256("C")

Canonical order: A < B < C
Reason: A and B have same (network_id, gateway_id, sequence), A.hash < B.hash
        C has higher sequence than both
```

### Cache Eviction Order

Using the composite eviction formula from Section 13 (M-GDP-2):
`eviction_score = trust_score * 10 + utility_score * 5 + recency_score * 2`
where `recency_score = 1000 - min(1000, current_epoch - last_seen)`

```text
Cache entries (current_epoch = 2000):
  X: trust=100, last_seen=1000, utility=50
     recency = 1000 - min(1000, 2000-1000) = 0
     score = 100*10 + 50*5 + 0*2 = 1250

  Y: trust=50,  last_seen=2000, utility=80
     recency = 1000 - min(1000, 2000-2000) = 1000
     score = 50*10 + 80*5 + 1000*2 = 2900

  Z: trust=100, last_seen=1500, utility=30
     recency = 1000 - min(1000, 2000-1500) = 500
     score = 100*10 + 30*5 + 500*2 = 2150

Eviction order: X → Z → Y
Reason: X has lowest eviction_score (1250) due to zero recency (last_seen=1000,
oldest entry). Z is next (2150) with moderate recency and low utility.
Y is last (2900) — lowest trust but most recent and highest utility.
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
- RFC-0855 (Networking): MON — mission overlay networks consuming GDP discovery
- RFC-0856 (Networking): DRS — route selection
- RFC-0860 (Networking): PoRelay — trust scoring

## Related Use Cases

- [Decentralized Mission Execution](../../docs/use-cases/decentralized-mission-execution.md)
- [Hybrid AI-Blockchain Runtime](../../docs/use-cases/hybrid-ai-blockchain-runtime.md)
- [Node Operations](../../docs/use-cases/node-operations.md)
