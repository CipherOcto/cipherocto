# Mission: 0870f — L2 In-Process Multi-Node E2E Tests

## Status

Open

## RFC

RFC-0870 (Networking): Distributed Quota Router Network

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types, scoring, forwarding
- 0870b (must complete first) — gossip, HMAC signing, peer exchange
- 0870c (must complete first) — handler, route API
- 0870d (must complete first) — HMAC verification, rate limiting
- 0870e (should complete first) — unit test coverage for handler, provider, forward, gossip

## Summary

Build a test harness crate `quota-router-e2e-tests/` that wires multiple `QuotaRouterNode` instances together in a single process via in-process async channels, and implement L2 e2e tests covering the full routing lifecycle: local dispatch, multi-hop forwarding, gossip convergence, peer discovery, HMAC verification across nodes, and rate limiting across nodes.

This mirrors the stoolap sync `sync-e2e-tests/` L3 pattern but adapted for quota router's simpler architecture (no WAL, no Merkle trees — just capacity gossip + request forwarding).

## Test Layers (Quota Router)

| Layer | What | Processes | Transport | When |
|-------|------|-----------|-----------|------|
| **L1** Unit | Individual modules (handler, scorer, gossip, etc.) | single | in-memory | every commit |
| **L2** In-process | Multiple nodes, full routing lifecycle | single | tokio mpsc | every commit |
| **L3** Cross-process | Real TCP transport between processes | multi | TCP | nightly |
| **L4** Property | Proptest invariants for scoring, gossip, HMAC | single | in-memory | every PR |

**This mission implements L2.** L1 is covered by 0870e. L3 is 0870g. L4 is 0870h.

## Design

### Crate layout

```
quota-router-e2e-tests/
├── Cargo.toml
├── src/
│   └── lib.rs              # TestNode, TestCluster, InProcessTransport, helpers
└── tests/
    ├── l2_basic_routing.rs          # Local dispatch + single-hop forwarding
    ├── l2_multi_hop.rs              # TTL chain, 3-node fan-out
    ├── l2_gossip_convergence.rs     # Gossip propagation, staleness
    ├── l2_peer_discovery.rs         # Known_peers exchange via gossip
    ├── l2_hmac_across_nodes.rs      # HMAC verification on gossip/announce
    ├── l2_rate_limiting.rs          # Rate limiting across forwarded requests
    └── l2_lifecycle.rs              # Node startup, shutdown, restart
```

### Test harness: `InProcessTransport`

The key abstraction is `InProcessTransport` — an in-memory adapter implementing `NetworkSender` that delivers payloads to other nodes' `QuotaRouterHandler` instances via `tokio::sync::mpsc` channels.

**API note:** The `NetworkSender` trait has `send(&self, payload: &[u8], context: &SendContext)` (not `send_best`). `SendContext` carries `mission_id`, `priority`, `source_peer`, `origin_gateway` — but NOT a target peer ID. Target routing is handled by `NodeTransport` internally. For in-process tests, the `InProcessNetwork` (shared state) maps `(source_peer, target_peer)` by inspecting the first byte of the payload (the envelope discriminator) and using the registered peer map.

```rust
use std::sync::Arc;
use tokio::sync::mpsc;
use octo_transport::sender::{NetworkSender, SendContext, TransportError};

/// Shared routing table: maps RouterNodeId → inbox sender.
type PeerMap = Arc<tokio::sync::Mutex<BTreeMap<RouterNodeId, mpsc::Sender<Vec<u8>>>>>;

pub struct InProcessTransport {
    peers: PeerMap,
    self_id: RouterNodeId,
}

#[async_trait]
impl NetworkSender for InProcessTransport {
    async fn send(&self, payload: &[u8], ctx: &SendContext) -> Result<(), TransportError> {
        // For in-process: broadcast to all peers except self.
        // The NodeTransport layer handles target selection; the adapter
        // delivers to all connected peers and lets the handler filter.
        let senders: Vec<_> = {
            let peers = self.peers.lock().await;
            peers.iter()
                .filter(|(id, _)| **id != self.self_id)
                .map(|(_, s)| s.clone())
                .collect()
        };
        for sender in senders {
            let _ = sender.send(payload.to_vec()).await;
        }
        Ok(())
    }

    fn name(&self) -> &str { "in-process" }
    fn is_healthy(&self) -> bool { true }
}
```

**Target routing strategy:** Since `NetworkSender::send` broadcasts to all peers, each handler's `on_receive` method must check whether the message is intended for it (via discriminator + context matching). This matches how real TCP transport works — the handler already discriminates by message type. For targeted sends (e.g., `ForwardResponse` to a specific origin node), the response payload contains the `request_id` which the originating node's handler matches against its `PendingRequests`.

### Test harness: `TestNode`

```rust
pub struct TestNode {
    pub node_id: RouterNodeId,
    pub node: QuotaRouterNode,
    pub provider: Arc<MockLocalProvider>,
    /// Receiver end — the test driver pulls messages from here.
    pub inbox: mpsc::Receiver<Vec<u8>>,
}
```

### Test harness: `TestCluster`

```rust
pub struct TestCluster {
    pub nodes: Vec<TestNode>,
    pub shared_peers: Arc<Mutex<BTreeMap<RouterNodeId, mpsc::Sender<Vec<u8>>>>>,
    pub network_key: [u8; 32],
}

impl TestCluster {
    /// Create a cluster of N nodes with a star or line topology.
    pub fn new(n: usize, topology: Topology) -> Self;

    /// Start all nodes (spawn gossip loops, announce to peers).
    pub async fn start_all(&self);

    /// Wait until all nodes have converged (same gossip cache snapshot).
    pub async fn wait_converged(&self, timeout: Duration);

    /// Drive one node's inbox — deliver all pending messages to its handler.
    pub async fn drive_node(&self, idx: usize);

    /// Drive all nodes' inboxes once.
    pub async fn drive_all(&self);
}
```

### Topology enum

```rust
pub enum Topology {
    Star,       // Node 0 is the hub, nodes 1..N connect to it
    Line,       // Node 0 → Node 1 → Node 2 → ... → Node N
    FullMesh,   // Every node connects to every other
}
```

### MockLocalProvider

```rust
pub struct MockLocalProvider {
    models: Vec<String>,
    health: ProviderHealth,
    responses: Mutex<HashMap<String, Vec<u8>>>,  // model → response
    call_count: AtomicUsize,                      // track invocations
}

impl MockLocalProvider {
    pub fn new(models: Vec<String>) -> Self;
    pub fn with_health(self, health: ProviderHealth) -> Self;
    pub fn with_response(self, model: &str, response: Vec<u8>) -> Self;
    pub fn call_count(&self) -> usize;
}
```

## Concrete Test Cases

### l2_basic_routing.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T1: local_dispatch` | Node A has gpt-4o locally. Consumer routes gpt-4o request. Verify local provider called, response returned. | 1 node |
| `L2-T2: single_hop_forwarding` | Node A has no providers. Node B has gpt-4o. Node A gossips Node B's capacity. Consumer routes via A → A forwards to B → B dispatches locally → response returns via A. | 2 nodes |
| `L2-T3: policy_cheapest` | Node A has gpt-4o at $0.01/1k. Node B has gpt-4o at $0.005/1k. Consumer routes with `Cheapest` policy via A → A selects B. | 2 nodes |
| `L2-T4: policy_fastest` | Node A has gpt-4o at 200ms. Node B has gpt-4o at 50ms. Consumer routes with `Fastest` policy via A → A selects B. | 2 nodes |
| `L2-T5: model_not_supported` | Consumer routes `claude-3` request to Node A which only has gpt-4o. Verify `ForwardRejectReason::ModelNotSupported`. | 1 node |
| `L2-T6: policy_quality` | Node A has gpt-4o with 9000 bps success rate. Node B has gpt-4o with 9900 bps. Consumer routes with `Quality` policy → selects B. | 2 nodes |
| `L2-T7: policy_local_only` | Node A has no providers but Node B has gpt-4o. Consumer routes with `LocalOnly` → A returns `NoProvider` without forwarding. | 2 nodes |
| `L2-T8: forward_timeout` | Node A forwards to unreachable Node B. Verify request times out after `forward_timeout` and returns `ForwardOutcome::Timeout`. | 2 nodes (B unreachable) |
| `L2-T9: max_concurrent_forwards` | Send 65 concurrent requests to Node A which must forward all. Verify the 65th is rejected with a concurrency error (max=64). | 2 nodes |
| `L2-T10: payload_too_large` | Consumer sends payload exceeding `max_payload_bytes`. Verify rejection before forwarding. | 1 node |

### l2_multi_hop.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T11: three_node_fan_out` | Nodes A, B, C in line. A has no providers. B has no providers. C has gpt-4o. Consumer routes via A → A forwards to B → B forwards to C → C dispatches locally → response returns via B → A. | 3 nodes (line) |
| `L2-T12: ttl_chain_exhaustion` | A→B→C→D chain. Consumer routes with TTL=2 via A. A forwards to B (TTL=1). B tries to forward to C (TTL=0) → `TtlExpired` reject. Verify request dies at B. | 4 nodes (line) |
| `L2-T13: ttl_prevents_infinite_forwarding` | Same as T12 but with TTL=5. Verify request reaches D (4 hops, TTL counts down 5→4→3→2→1). D dispatches locally. | 4 nodes (line) |
| `L2-T14: star_topology_load_distribution` | Node 0 (hub) has no providers. Nodes 1, 2, 3 each have different providers. Consumer routes via 0 → 0 forwards to the best-matching peer. | 4 nodes (star) |

### l2_gossip_convergence.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T15: gossip_propagation` | Node A adds a new provider. A broadcasts gossip. B receives and updates cache. Verify B's cache reflects A's capacity. | 2 nodes |
| `L2-T16: gossip_staleness` | Node A broadcasts gossip. Wait > STALENESS_THRESHOLD. Verify entries are evicted from B's cache on next merge. | 2 nodes |
| `L2-T17: three_node_gossip_convergence` | A has gpt-4o. B has claude-3. C has gemini-pro. After gossip rounds, all 3 nodes know about all 3 providers. | 3 nodes (star) |
| `L2-T18: gossip_capacity_update` | Node A's provider goes from 100 remaining to 10. A broadcasts updated gossip. B's cache reflects new capacity. Verify scoring uses updated capacity. | 2 nodes |

### l2_peer_discovery.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T19: known_peers_in_gossip` | A knows B and C. A broadcasts gossip with `known_peers = [B, C]`. D receives gossip → D adds B and C to peer cache (if B and C announced). | 3 nodes (A, D + B, C announced) |
| `L2-T20: announce_then_discover` | B announces to A. A gossips with known_peers=[B]. C receives → C adds B to cache. C can now forward to B. | 3 nodes |
| `L2-T21: withdraw_removes_peer` | A, B, C form a triangle. B withdraws. A and C remove B from peer cache. Verify no forwarding attempts to B after withdrawal. | 3 nodes |

### l2_hmac_across_nodes.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T22: gossip_hmac_verified` | Node A sends gossip with correct HMAC to B. B accepts and merges. | 2 nodes |
| `L2-T23: gossip_hmac_rejected` | Node A sends gossip with wrong HMAC to B. B drops silently. Verify B's cache unchanged. | 2 nodes |
| `L2-T24: announce_hmac_verified` | Node A announces with correct HMAC. B accepts, adds A to peer cache. | 2 nodes |
| `L2-T25: announce_hmac_rejected` | Node A announces with wrong HMAC. B drops, A not in peer cache. | 2 nodes |
| `L2-T26: withdraw_hmac_verified` | Node A withdraws with correct HMAC. B removes A from peer cache. | 2 nodes |
| `L2-T27: withdraw_hmac_rejected` | Node A withdraws with wrong HMAC. B keeps A in peer cache. | 2 nodes |

### l2_rate_limiting.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T28: rate_limit_local_dispatch` | Consumer sends 200 requests/s to Node A (limit 100/s). Verify first 100 succeed, rest rate-limited. | 1 node |
| `L2-T29: rate_limit_forwarded_requests` | Consumer sends 200 requests/s via Node A to Node B. Verify Node B's per-peer rate limiter kicks in. | 2 nodes |

### l2_lifecycle.rs

| Test | Description | Topology |
|------|-------------|----------|
| `L2-T30: node_startup_announce` | Node A starts, broadcasts announce. Node B receives, adds A to peer cache. | 2 nodes |
| `L2-T31: node_shutdown_withdraw` | Node A shuts down gracefully, broadcasts withdraw. Node B removes A from cache. | 2 nodes |
| `L2-T32: node_restart_rejoin` | Node A starts, gossips, shuts down, restarts, re-announces. Verify B re-adds A. | 2 nodes |

## Acceptance Criteria

- [ ] `quota-router-e2e-tests/Cargo.toml` exists (leaf workspace, excludes from main workspace)
- [ ] `quota-router-e2e-tests/src/lib.rs` exists with `TestNode`, `TestCluster`, `InProcessTransport`, `MockLocalProvider`, `Topology`
- [ ] `l2_basic_routing.rs` — 10 tests (T1–T10)
- [ ] `l2_multi_hop.rs` — 4 tests (T11–T14)
- [ ] `l2_gossip_convergence.rs` — 4 tests (T15–T18)
- [ ] `l2_peer_discovery.rs` — 3 tests (T19–T21)
- [ ] `l2_hmac_across_nodes.rs` — 6 tests (T22–T27)
- [ ] `l2_rate_limiting.rs` — 2 tests (T28–T29)
- [ ] `l2_lifecycle.rs` — 3 tests (T30–T32)
- [ ] All 32 tests pass with `cargo test -p quota-router-e2e-tests`
- [ ] `cargo clippy -p quota-router-e2e-tests -- -D warnings` clean
- [ ] `cargo fmt --check` passes
- [ ] CI workflow added: `.github/workflows/quota-router-e2e.yml`

## Complexity

High (~1800-2400 lines). Test harness + 32 e2e tests.

## Implementation Notes

- Use `tokio::test` for all async tests
- `InProcessTransport` uses `tokio::sync::mpsc::unbounded_channel` for message delivery (no backpressure in tests)
- `TestCluster::drive_node` is critical — it pulls messages from a node's inbox and calls `handler.on_receive()` for each. This simulates the network without real TCP.
- For gossip convergence tests, run a tight loop of `drive_all()` + `tokio::time::sleep(Duration::from_millis(10))` until convergence or timeout
- HMAC tests need nodes to share a `network_key` — `TestCluster::new` generates one
- Rate limit tests need tight timing — use `tokio::time::pause()` for deterministic time control
- The crate should NOT be added to the main workspace `Cargo.toml` exclude list — it's a leaf workspace like `quota-router/` itself
- Follow the `sync-e2e-tests/` pattern for crate structure

## Type Coverage

This is a **testing mission** that exercises the full quota router stack.

| Component | Types exercised |
|-----------|-----------------|
| In-process transport | `NetworkSender`, `SendContext`, `TransportError` |
| Node lifecycle | `QuotaRouterNode`, `QuotaRouterNodeBuilder`, `RouterNodeLifecycle` |
| Request routing | `RequestContext`, `RoutingPolicy`, `ForwardingConfig` |
| Scoring | `select_destinations`, `Destination`, `ProviderCapacity` |
| Forwarding | `ForwardRequestPayload`, `ForwardResponsePayload`, `ForwardRejectPayload` |
| Gossip | `CapacityGossipPayload`, `GossipCache` |
| Peer management | `RouterAnnouncePayload`, `RouterWithdrawPayload`, `PeerCache` |
| HMAC | `SignedPayload`, `compute_hmac`, `verify_hmac` |
| Rate limiting | `RateLimiter`, `TokenBucket` |
| Handler dispatch | `QuotaRouterHandler`, `NetworkReceiver`, `DropAction` |
