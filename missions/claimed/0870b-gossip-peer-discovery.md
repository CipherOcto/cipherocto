# Mission: 0870b — Capacity gossip + peer discovery + lifecycle broadcast

## Status

Completed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network — Phase 2: Capacity Gossip + Peer Discovery

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types, `QuotaRouterNode`, `GossipCache` stub, `PeerCache` stub

## Summary

Implement the capacity gossip protocol, `GossipCache` with staleness eviction, `PeerCache` with LRU eviction, `RouterAnnounce`/`RouterWithdraw` envelope types with HMAC signing, the `SignedPayload` trait, `broadcast_gossip`/`broadcast_announce` methods on `QuotaRouterNode`, and the `CapacityRequest` pull-based gossip trigger. This mission makes the mesh aware of its peers and their provider capacities.

## Design

### Files to implement

- `octo-transport/src/quota_router/gossip.rs` — fill in `GossipCache` methods, `CapacityGossipPayload` with `known_peers` field
- `octo-transport/src/quota_router/announce.rs` — `RouterAnnouncePayload`, `RouterWithdrawPayload`, `SignedPayload` trait
- `octo-transport/src/quota_router/mod.rs` — add `broadcast_gossip`, `broadcast_announce`, `build_capacity_gossip`, `request_capacity_from`, `monotonic_now`

### Types to implement

#### Gossip (`gossip.rs`)

```rust
pub struct CapacityGossipPayload {
    pub sender_id: RouterNodeId,
    pub timestamp: u64,
    pub capacities: Vec<ProviderCapacity>,
    pub known_peers: Vec<RouterNodeId>,  // up to 32
    pub hmac: [u8; 32],
}

pub struct GossipCache {
    entries: BTreeMap<RouterNodeId, Vec<ProviderCapacity>>,
    last_updated: BTreeMap<RouterNodeId, u64>,
}

impl GossipCache {
    pub fn new() -> Self;
    pub fn merge(&mut self, sender_id: RouterNodeId, capacities: Vec<ProviderCapacity>);
    pub fn snapshot(&self) -> Vec<(RouterNodeId, Vec<ProviderCapacity>)>;
}
```

#### Announce (`announce.rs`)

```rust
pub struct RouterAnnouncePayload {
    pub node_id: RouterNodeId,
    pub network_id: NetworkId,
    pub supported_models: Vec<String>,
    pub capacities: Vec<ProviderCapacity>,
    pub timestamp: u64,
    pub hmac: [u8; 32],
}

pub struct RouterWithdrawPayload {
    pub node_id: RouterNodeId,
    pub reason: WithdrawReason,
    pub timestamp: u64,
    pub hmac: [u8; 32],
}

pub enum WithdrawReason { Graceful, Maintenance, Decommissioned }

pub trait SignedPayload {
    fn compute_hmac(&self, network_key: &[u8; 32]) -> [u8; 32];
    fn verify_hmac(&self, network_key: &[u8; 32]) -> bool;
}
```

#### Peer cache (`mod.rs` or `gossip.rs`)

```rust
pub struct PeerCache {
    direct: BTreeMap<RouterNodeId, PeerInfo>,
    discovered: BTreeMap<RouterNodeId, PeerInfo>,
    max_peers: usize,
}

pub struct PeerInfo {
    pub node_id: RouterNodeId,
    pub trust_level: PeerTrust,
    pub discovered: bool,
    pub last_seen: u64,
}

impl PeerCache {
    pub fn new() -> Self;
    pub fn add_direct(&mut self, node_id: RouterNodeId, capacities: Vec<ProviderCapacity>);
    pub fn try_add(&mut self, node_id: RouterNodeId);
    pub fn remove(&mut self, node_id: RouterNodeId);
    pub fn total(&self) -> usize;
    pub fn direct_ids(&self) -> Vec<RouterNodeId>;
}
```

### What this mission does NOT implement

- `QuotaRouterNode::route()` (0870c)
- `build_with_bootstrap()` (0870c)
- HMAC verification on inbound gossip (0870d)
- Rate limiting (0870d)

## Acceptance Criteria

- [ ] `GossipCache::merge` inserts capacities and refreshes staleness timestamp
- [ ] `GossipCache::snapshot` filters entries older than 30s (staleness threshold)
- [ ] `PeerCache::try_add` enforces max_peers (128) with LRU eviction of discovered peers
- [ ] `PeerCache::add_direct` marks peer as `PeerTrust::Verified`
- [ ] `SignedPayload` trait implemented for `RouterAnnouncePayload`, `RouterWithdrawPayload`, `CapacityGossipPayload`
- [ ] `compute_hmac` uses `blake3::keyed_hash` with zeroed HMAC field as canonical pre-image
- [ ] `verify_hmac` uses constant-time comparison via `blake3::Hash::ct_eq`
- [ ] `CapacityGossipPayload` includes `known_peers: Vec<RouterNodeId>` (up to 32)
- [ ] `QuotaRouterNode::broadcast_gossip` builds gossip, signs HMAC, broadcasts via `NodeTransport::broadcast()`
- [ ] `QuotaRouterNode::broadcast_announce` builds announce, signs HMAC, broadcasts via `NodeTransport::broadcast()`
- [ ] `QuotaRouterNode::build_capacity_gossip` includes local capacities + up to 32 direct peer IDs
- [ ] `monotonic_now()` returns monotonically increasing values (atomic counter)
- [ ] Unit tests pass for gossip merge, staleness eviction, peer cache LRU, HMAC computation
- [ ] Clippy clean, `cargo fmt --check` passes

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `CapacityGossipPayload` struct | This mission |
| `GossipCache` struct + methods | This mission |
| `PeerCache` struct + methods | This mission |
| `PeerInfo` struct | This mission |
| `RouterAnnouncePayload` struct | This mission |
| `RouterWithdrawPayload` struct | This mission |
| `WithdrawReason` enum | This mission |
| `SignedPayload` trait | This mission |
| `QuotaRouterNode::broadcast_gossip` | This mission |
| `QuotaRouterNode::broadcast_announce` | This mission |
| `QuotaRouterNode::build_capacity_gossip` | This mission |
| `QuotaRouterNode::request_capacity_from` | This mission |
| `QuotaRouterNode::monotonic_now` | This mission |
| `QuotaRouterNode::network_key` (helper, derives key from `network_id`) | This mission |
| `CapacityRequestPayload` struct | This mission |

## Complexity

Medium (~500-700 lines). Gossip protocol + peer cache + HMAC signing + tests.

## Implementation Notes

- `SignedPayload` uses `serde_json::to_vec` for canonical pre-image (HMAC field zeroed). DCS encoding is a v2 enhancement.
- `GossipCache::snapshot` uses a hardcoded `STALENESS_THRESHOLD` of 30s (3 × default gossip_interval).
- `PeerCache::try_add` only adds peers that haven't been seen before (idempotent). Real implementation should check for prior `RouterAnnounce` (identity verification per §Phase 3 rule 2) — but v1 trusts any peer ID in `known_peers` from verified gossip.
- `broadcast_gossip` and `broadcast_announce` are async methods that call `self.transport.broadcast()`. They hold the lock only for the synchronous gossip-building step, then release it before the async broadcast.
