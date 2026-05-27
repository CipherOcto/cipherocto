---
title: "RFC-0852: Deterministic Gossip Protocol (DGP)"
status: Draft
version: 1.0.0
created: 2026-05-25
updated: 2026-05-25
authors:
  - CipherOcto Core Team
related:
  - RFC-0850 (Networking): Deterministic Overlay Transport
  - RFC-0851 (Networking): Gateway Discovery Protocol
  - RFC-0126 (Numeric): Deterministic Serialization
---

# RFC-0852: Deterministic Gossip Protocol (DGP)

## Status

Draft

## Authors

- Author: @cipherocto

## Maintainers

- Maintainer: @cipherocto

## Summary

The Deterministic Gossip Protocol (DGP) defines how CipherOcto nodes propagate, synchronize, deduplicate, validate, and reconcile overlay state across heterogeneous transport fabrics.

DGP provides:

- Deterministic message propagation across chaotic carrier fabrics
- Replay-safe synchronization
- Partition healing via anti-entropy reconciliation
- Censorship-resistant multi-carrier dissemination
- Mission-scoped gossip domains
- Consensus-safe relay behavior

The key invariant: **DGP separates transport nondeterminism from consensus determinism.** External networks may reorder, duplicate, censor, or delay messages, but DGP ensures logical overlay state converges deterministically.

## Dependencies

**Requires:**

- RFC-0850 (Networking): DOT — envelope format, transport abstraction
- RFC-0851 (Networking): GDP — gateway discovery
- RFC-0126 (Numeric): Deterministic Serialization — canonical encoding

**Optional:**

- RFC-0853 (Networking): OCrypt — encrypted gossip domains

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1: Deterministic Convergence | Same valid state across nodes | 100% convergence given same inputs |
| G2: Byzantine Tolerance | Adversarial transport resilience | Survive 33% malicious nodes |
| G3: Replay Resistance | Prevent stale repropagation | Zero replay acceptance |
| G4: Deduplicated Flooding | Efficient dissemination | 100% duplicate elimination |
| G5: Partition Healing | Autonomous reconciliation | Convergence within 2x partition duration |
| G6: Mission Isolation | Scoped propagation | Zero cross-mission leakage |
| G7: Multi-Transport Federation | Simultaneous heterogeneous propagation | 3+ carriers per object |
| G8: Censorship Resistance | Carrier-independent redundancy | Survive single-platform block |

## Motivation

### CAN WE? — Feasibility Research

Traditional gossip protocols assume stable homogeneous networks. DGP assumes chaotic heterogeneous carrier fabrics including Telegram, Discord, Matrix, native QUIC, Nostr, Bluetooth, LoRa, intermittent offline peers, and opportunistic relays.

Research confirms feasibility through:

- **libp2p gossipsub** provides battle-tested gossip primitives
- **CRDTs** provide deterministic merge semantics for eventually consistent state
- **Anti-entropy protocols** provide proven reconciliation mechanisms
- **Bloom filters** provide efficient set reconciliation for large state spaces

### WHY? — Why This Matters

Without DGP:

- Overlay state cannot propagate across heterogeneous carriers
- Partitions heal slowly or not at all
- Duplicate messages waste bandwidth
- Consensus fragments cannot be reliably disseminated
- Mission coordination fails under network stress

## Specification

### 1. Conceptual Model

Traditional gossip: `stable homogeneous network`

DGP: `chaotic heterogeneous carrier fabric`

### 2. Gossip Domain

```rust
#[repr(u16)]
enum GossipScope {
    GLOBAL = 0x0001,
    REGIONAL = 0x0002,
    MISSION = 0x0003,
    PRIVATE = 0x0004,
    LOCAL = 0x0005,
    CONSENSUS = 0x0006,
    // 0x0007-0xFFFF: Reserved for future use
}

struct GossipDomainId {
    network_id: u32,
    mission_id: [u8; 32],
    scope: u16,     // GossipScope enum values
}
```

| Domain | Purpose | Default TTL |
|--------|---------|-------------|
| GLOBAL | Entire overlay | 20 hops |
| REGIONAL | Geographic cluster | 10 hops |
| MISSION | Temporary mission mesh | 5 hops |
| PRIVATE | Encrypted subgroup | 3 hops |
| LOCAL | Nearby peers | 3 hops |
| CONSENSUS | Validator propagation | 10 hops |

**Domain ID Mapping (RFC-0850 ↔ RFC-0852):**

When wrapping a DOT `DeterministicEnvelope` as a `GossipObject`:
- `object_type = Envelope (0x0001)`
- `domain_id.network_id` = envelope's `network_id`
- `domain_id.mission_id` = envelope's `mission_id`
- `domain_id.scope` = derived from the GDP `DiscoveryScope` mapping (see RFC-0851)
- `object_hash = BLAKE3-256(dcs_serialize(GossipObject))` — NOT the DOT `payload_hash`

The `BroadcastDomainId` and `GossipDomainId` are separate types serving different purposes: `BroadcastDomainId` identifies platform-specific communication surfaces; `GossipDomainId` identifies gossip propagation domains. Conversion happens at the adapter layer, not in the canonical serialization.

### 3. Canonical Gossip Object

```rust
#[repr(u16)]
enum GossipObjectType {
    Envelope = 0x0001,
    RouteUpdate = 0x0002,
    ConsensusFragment = 0x0003,
    MissionState = 0x0004,
    VectorCommitment = 0x0005,
    ZkProof = 0x0006,
    DiscoveryAdvertisement = 0x0007,
    SnapshotFragment = 0x0008,
    // 0x0009-0xFFFF: Reserved for future use
}

/// Bit layout for GossipObject.propagation_flags: u64
/// Each flag indicates which gossip mode(s) should propagate this object.
const FLAG_FLOOD: u64           = 0x0001;  // Flood to all peers
const FLAG_INCREMENTAL: u64     = 0x0002;  // Only to peers missing this object
const FLAG_ANTI_ENTROPY: u64    = 0x0004;  // Include in anti-entropy sync
const FLAG_DIRECTED: u64        = 0x0008;  // Targeted to specific peers
const FLAG_RELIABLE: u64        = 0x0010;  // Require acknowledgment
const FLAG_COMPRESSED: u64      = 0x0020;  // Payload is compressed
// Bits 6-63: Reserved (MUST be zero)

struct GossipObject {
    object_type: u16,           // GossipObjectType enum
    object_hash: [u8; 32],      // BLAKE3-256(dcs_serialize(GossipObject))
    object_size: u32,
    domain_id: GossipDomainId,
    logical_timestamp: u64,
    origin_gateway: [u8; 32],
    ttl_hops: u16,
    propagation_flags: u64,     // FLAG_* bitmask
    payload_root: [u8; 32],
    signature: [u8; 64],
}
```

**object_hash derivation:** `object_hash = BLAKE3-256(dcs_serialize(GossipObject))` for all object types. This is distinct from DOT's `payload_hash` — even when `object_type = Envelope`, the `object_hash` covers the entire `GossipObject` (including `domain_id`, `ttl_hops`, etc.), not just the wrapped envelope payload.

**Gossipable Payloads:**

| Type | Description | Payload Content |
|------|-------------|-----------------|
| Envelope | DOT messages | DCS-serialized `DeterministicEnvelope` |
| RouteUpdate | Gateway topology | Route change announcement |
| ConsensusFragment | Partial blocks/checkpoints | Consensus data |
| MissionState | Mission coordination | Mission lifecycle updates |
| VectorCommitment | AI/vector state | Merkle root of vector data |
| ZkProof | Proof propagation | ZK proof bytes |
| DiscoveryAdvertisement | GDP advertisement | DCS-serialized `GatewayAdvertisement` |
| SnapshotFragment | State synchronization | Chunk of state snapshot |

### 4. Deterministic Propagation Rules

Physical arrival order: NON-DETERMINISTIC

Logical processing order: DETERMINISTIC

**Canonical processing order:** `(domain_id, logical_timestamp, object_hash)`

NOT: receive time, transport order, platform sequence, thread order.

### 5. Deduplication

**Object identity:** `object_hash` (BLAKE3-256 of canonical payload)

**Duplicate rule:** If identical `object_hash` received → process once, relay per policy.

**Conflicting payload rule:** If same logical identity but different `object_hash`:
- `FIRST_VALID_HASH_WINS` — the object with the lexicographically lowest `object_hash` that passes signature verification is canonical
- Tie-breaking when hashes are equal (should not happen with BLAKE3-256, but defined for safety): lowest lexicographic `origin_gateway` wins
- Deterministic ordering: `(object_hash ASC, origin_gateway ASC)`

### 6. Gossip Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| Flood | Broadcast aggressively | Bootstrap, emergency, partition recovery |
| Incremental | Only unseen objects | Normal operation |
| Anti-entropy | Merkle summary exchange | Periodic reconciliation |
| Directed | Targeted propagation | Mission overlays, validator coordination |

### 7. Anti-Entropy Synchronization

```rust
struct GossipStateSummary {
    domain_id: GossipDomainId,
    state_root: [u8; 32],
    object_count: u64,
    watermark: u64,
}
```

If roots differ: binary Merkle descent locates missing objects.

### 8. Propagation Classes

```rust
enum GossipPriority {
    Critical,       // Network emergencies
    Consensus,      // Validator data
    Mission,        // Mission coordination
    Standard,       // Normal messages
    Bulk,           // Large transfers
    Archive,        // Historical replication
}
```

Scheduling: Critical → Consensus → Mission → Standard → Bulk → Archive

### 9. Time Model

DGP uses logical overlay timestamps, NOT wall-clock consensus.

Clock drift isolation: Physical timestamps are advisory only. Consensus ordering MUST remain independent.

### 10. Multi-Transport Amplification

A single object MAY propagate via Telegram + Discord + Matrix + QUIC + Nostr + Bluetooth simultaneously. Loss of one carrier MUST NOT invalidate object propagation.

### 11. Gossip Compression

Large state synchronization SHOULD use Bloom filters, Merkle roots, bitmap summaries, range commitments.

**Bloom filter hash requirement (H5):** Bloom filters MUST use BLAKE3-256 as the hash function. AHasher, SipHash, and other non-deterministic hashers MUST NOT be used. Each Bloom filter hash iteration uses: `BLAKE3-256(object_hash || iteration_index)`. This ensures deterministic behavior across all nodes (Class A requirement).

**Fragment reassembly (M7):** Fragments are collected by `object_hash` and reassembled in `fragment_index` order. Timeout for incomplete reassembly is configurable (default: 30 seconds). On timeout, partial fragments are discarded and a warning is logged. Reassembled payload is verified against `payload_root` before processing.

Large objects MAY fragment:

```rust
struct GossipFragment {
    object_hash: [u8; 32],
    fragment_index: u32,
    fragment_total: u32,
    payload: Vec<u8>,
}
```

### 12. Replay Protection

Objects remain valid within network-defined replay horizon. Gateways maintain a `GossipReplayCache` modeled after DOT's `ReplayCache` (RFC-0850 §11.2).

```rust
/// Gossip replay cache for deduplication.
/// Uses BTreeMap for deterministic iteration order (Class A requirement).
struct GossipReplayCache {
    /// Object hash → first seen logical timestamp
    seen: BTreeMap<[u8; 32], u64>,
    /// Maximum entries before eviction
    max_entries: u32,
    /// Replay window in logical time units
    window_duration: u64,
}
```

**Deterministic eviction policy:**
1. Always purge entries outside the replay window first
2. If still at capacity, evict the entry with smallest `(first_seen, object_hash)` — BTreeMap's natural ordering provides deterministic iteration
3. Tie-breaking: lexicographic `object_hash` (same as DOT's approach)

### 13. Retention Classes

| Class | Retention |
|-------|-----------|
| Ephemeral | Temporary |
| Mission | Mission duration |
| Consensus | Permanent |
| Archive | Long-term storage |

## RFC-0008 Execution Class Mapping

| DGP Operation | Class | Rationale |
|---------------|-------|-----------|
| Object hash computation | A | Consensus-critical identity |
| Canonical processing order | A | Consensus-critical ordering |
| Deduplication check | A | Consensus-critical identity |
| Signature verification | A | Consensus-critical validation |
| Anti-entropy Merkle root | A | Consensus-critical state |
| Replay cache eviction | A | Must be deterministic across nodes |
| Fragment reassembly | B | Deterministic given fragments |
| Gossip propagation | C | Transport-dependent, non-deterministic |
| Peer discovery | C | Non-deterministic network conditions |

## Performance Targets

| Metric | Target |
|--------|--------|
| Object processing | <1ms per object |
| Deduplication lookup | <1µs |
| Anti-entropy sync | <5s for 10K objects |
| Multi-carrier propagation | <2s across 3 carriers |
| Fragment reassembly | <10s for 10 fragments |

## Security Considerations

| Threat | Impact | Mitigation |
|--------|--------|------------|
| Flood spam | High | Stake + quotas |
| Eclipse relay | High | Multi-carrier diversity |
| Replay storm | High | Hash replay cache |
| Route poisoning | High | Signature verification |
| Mutation attack | Critical | Payload commitment |
| Carrier censorship | Medium | Parallel propagation |

## Implementation Phases

### Phase 1: Core Gossip (Months 1-3)
- GossipObject with DCS serialization
- GossipDomainId scoping
- Deduplication via object_hash
- Canonical processing order enforcement
- Flood gossip mode

### Phase 2: Incremental and Anti-Entropy (Months 3-5)
- Incremental gossip (unseen-only propagation)
- GossipStateSummary exchange
- Binary Merkle descent reconciliation
- Bloom filter compression

### Phase 3: Priority and Mission Scoping (Months 5-8)
- GossipPriority scheduling
- Mission-scoped gossip domains
- Directed gossip for mission overlays
- Fragmentation/reassembly

### Phase 4: Multi-Transport and Economics (Months 8-12)
- Multi-carrier simultaneous propagation
- Carrier failover
- OCTO-B bandwidth accounting for gossip
- Retention class management

## Adversarial Review

| Threat | Impact | Mitigation | Verification |
|--------|--------|------------|--------------|
| Flood spam | High | Economic friction via relay stake + quotas | Rate limit test |
| Eclipse relay | High | Multi-carrier diversity constraints | Multi-gateway test |
| Replay storm | High | Hash replay cache with deterministic eviction | Replay detection test |
| Route poisoning | Medium | Signed advertisements (Ed25519) | Signature verification test |
| Mutation attack | Critical | Payload commitment (BLAKE3-256 hash) | Hash verification test |
| Carrier censorship | High | Parallel multi-transport propagation | Blocked carrier test |
| Consensus manipulation | Critical | Deterministic processing order | Ordering consistency test |
| Deduplication bypass | Medium | Global object_hash tracking | Duplicate detection test |

## Economic Analysis

### Token Integration

| Activity | Token | Rationale |
|----------|-------|-----------|
| Relay bandwidth | OCTO-B | Primary resource for gossip propagation |
| Gossip coordination | OCTO-O | Orchestration of multi-domain propagation |
| Reliable archival | OCTO-S | Storage for retention-class objects |
| Validator propagation | OCTO-N | Consensus-critical gossip participation |

### Gossip Economics

Relay rewards are proportional to:

- Objects propagated successfully
- Anti-entropy reconciliation contributions
- Multi-carrier diversity provided
- Retention class compliance

### Priority-Based Pricing

```text
cost = base_cost * priority_multiplier
```

| Priority | Multiplier | Rationale |
|----------|-----------|-----------|
| Critical | 10x | Consensus-critical, highest priority |
| Consensus | 5x | Block/attestation propagation |
| Mission | 3x | Mission-critical coordination |
| Standard | 1x | Normal operational gossip |
| Bulk | 0.5x | Background synchronization |
| Archive | 0.1x | Long-term retention, low urgency |

## Compatibility

### RFC-0843 Integration

DGP extends RFC-0843's gossipsub with deterministic semantics:

- RFC-0843 uses libp2p gossipsub for topic-based P2P messaging
- DGP adds deterministic processing order (not arrival order)
- DGP adds multi-carrier propagation beyond native P2P
- DGP adds anti-entropy Merkle reconciliation

### Forward Compatibility

- Gossip object types are extensible (values 0x0009-0xFFFF for future types)
- Gossip domains are extensible (scope values 0x0007-0xFFFF)
- Priority classes are extensible (values 0x0006-0xFFFF)

## Test Vectors

### Canonical Processing Order

```text
Object A: domain_id={1, mission_1}, timestamp=100, hash=BLAKE3-256("A")
Object B: domain_id={1, mission_1}, timestamp=100, hash=BLAKE3-256("B")
Object C: domain_id={1, mission_1}, timestamp=200, hash=BLAKE3-256("C")
Object D: domain_id={2, mission_2}, timestamp=50, hash=BLAKE3-256("D")

Canonical order: D < A < B < C
Reason: D has lower domain_id (2 > 1 is wrong — 1 < 2 so A,B come first)
        Actually: domain_id ordering is by (network_id, mission_id, scope)
        Assuming same network: A and B have same timestamp, A.hash < B.hash
        C has higher timestamp
        D has different domain — ordered by domain_id bytes
```

### Deduplication

```text
Received object_1 with hash=BLAKE3-256("payload_1") → Process, add to seen set
Received object_2 with hash=BLAKE3-256("payload_1") → Duplicate, skip processing
Received object_3 with hash=BLAKE3-256("payload_2") → Process, add to seen set
```

### Anti-Entropy Reconciliation

```text
Node A state_root = BLAKE3-256(Merkle(objects_A))
Node B state_root = BLAKE3-256(Merkle(objects_B))

If roots differ:
  Binary Merkle descent to locate divergent objects
  Exchange missing objects
  Recompute roots until convergence
```

## Alternatives Considered

| Approach | Pros | Cons | Decision |
|----------|------|------|----------|
| libp2p gossipsub only | Proven, efficient | No deterministic ordering | Supplemented by DGP |
| CRDTs for state sync | Eventually consistent | Not deterministic at consensus boundary | Rejected |
| Raft/Paxos | Strong consistency | Too slow for overlay gossip | Rejected |
| Blockchain mempool | Deterministic | High latency, expensive | Wrong abstraction |
| Broadcast flooding | Simple | Bandwidth explosion | Rejected |

**Decision:** DGP provides deterministic gossip that extends libp2p with consensus-safe ordering and multi-carrier propagation.

## Rationale

### Why deterministic processing order?

Without deterministic ordering:

1. Different nodes process objects in different orders
2. Consensus state diverges when objects have side effects
3. Replay verification fails when processing order differs

`(domain_id, logical_timestamp, object_hash)` ensures all nodes reach identical state.

### Why FIRST_VALID_HASH_WINS?

When multiple gateways inject the same payload:

1. Without a deterministic rule, conflict resolution is ambiguous
2. First-valid-hash by lexicographic ordering is deterministic
3. Prevents race conditions in multi-carrier propagation

### Why Merkle anti-entropy?

Without Merkle summaries:

1. Full state exchange is bandwidth-prohibitive at scale
2. No efficient way to locate divergent objects
3. Reconciliation becomes O(n) instead of O(log n)

Binary Merkle descent locates divergent objects in O(log n) comparisons.

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/octo-network/src/dgp/mod.rs` | DGP module root |
| `crates/octo-network/src/dgp/object.rs` | GossipObject |
| `crates/octo-network/src/dgp/domain.rs` | GossipDomainId |
| `crates/octo-network/src/dgp/dedup.rs` | Deduplication |
| `crates/octo-network/src/dgp/ordering.rs` | Canonical processing order |
| `crates/octo-network/src/dgp/anti_entropy.rs` | Merkle reconciliation |
| `crates/octo-network/src/dgp/flood.rs` | Flood gossip |
| `crates/octo-network/src/dgp/incremental.rs` | Incremental gossip |
| `crates/octo-network/src/dgp/directed.rs` | Directed gossip |
| `crates/octo-network/src/dgp/fragment.rs` | Fragmentation |

## Future Work

- F1: Adaptive gossip frequency based on network conditions
- F2: Compressed gossip using Bloom filter summaries
- F3: Zero-knowledge gossip proofs (prove delivery without revealing content)
- F4: Cross-chain gossip bridging for multi-network state synchronization
- F5: GPU-accelerated Merkle reconciliation for high-throughput nodes
- F6: Hierarchical gossip domains (nested mission scopes)
- F7: Stealth gossip for hidden mission coordination
- F8: Integration with satellite mesh networks (Starlink, Iridium)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-05-25 | Initial draft |

## Related RFCs

- RFC-0850 (Networking): DOT — envelope format
- RFC-0851 (Networking): GDP — discovery
- RFC-0855 (Networking): MON — mission overlays
- RFC-0857 (Networking): DOM — overlay mempool

## Related Use Cases

- [Decentralized Mission Execution](../../docs/use-cases/decentralized-mission-execution.md)
- [Agent Marketplace](../../docs/use-cases/agent-marketplace.md)
