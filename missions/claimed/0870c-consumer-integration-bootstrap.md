# Mission: 0870c — Consumer integration + bootstrap + inbound handler

## Status

Completed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network — Phase 3: Consumer Integration + Bootstrap

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types, `QuotaRouterNode`, scoring
- 0870b (must complete first) — gossip, peer cache, HMAC signing

## Summary

Implement the `QuotaRouterNode::route()` public API, `QuotaRouterHandler` (full `NetworkReceiver` implementation), `build_with_bootstrap()` for RFC-0851p-a integration, and `ProviderHealthProbe`/`ProviderHealthReport` for local provider health tracking. This mission makes the quota router network functional end-to-end: consumers can submit requests, nodes forward them through the mesh, and responses route back.

## Design

### Files to implement

- `quota-router/src/handler.rs` — full `QuotaRouterHandler` with all 7 envelope handlers
- `quota-router/src/lib.rs` — `QuotaRouterNode::route()`, `QuotaRouterNode::receive()`, `build_with_bootstrap()`. The builder wires the handler into `NodeTransport` automatically via `register_receiver()`.

### Methods to implement

#### `QuotaRouterNode::route()` (`lib.rs`)

```rust
pub async fn route(
    &self,
    context: &RequestContext,
    payload: &[u8],
) -> Result<Vec<u8>, RouterNodeError> {
    // 1. Build local provider capacities from config
    // 2. Snapshot peer capacities from gossip cache
    // 3. Call select_destinations (3-phase algorithm)
    // 4. If empty → NoProvider error
    // 5. If Local → dispatch via primary_provider.completion()
    // 6. If Remote → build ForwardRequest, insert into PendingRequests,
    //    send via transport, await response with timeout
}
```

#### `QuotaRouterNode::receive()` (`lib.rs`)

```rust
/// Public inbound API: dispatch a payload through `NodeTransport` to all
/// registered receivers. The internal `QuotaRouterHandler` is one of
/// those receivers (registered automatically by the builder). Symmetric
/// to `route()` for outbound traffic.
pub async fn receive(
    &self,
    payload: &[u8],
    ctx: &ReceiveContext,
) -> Result<(), TransportError> {
    self.transport.dispatch(payload, ctx).await
}
```

#### Wiring

The full consumer-facing wiring is a single builder call plus symmetric outbound/inbound use:

```rust
use quota_router::QuotaRouterNode;

let node = QuotaRouterNode::builder()
    .node_id(my_node_id)
    .network_id(network_id)
    .provider(openai_config)
    .provider(anthropic_config)
    .peer(peer_b_config)
    .policy(RoutingPolicy::Balanced)
    .build()?;

// Outbound: consumer-facing request dispatch.
// node.route(ctx).await? returns provider bytes.

let recv_ctx = ReceiveContext {
    source_transport: "tcp".into(),
    mission_id: [0u8; 32],
    sender_id: None,
};
// Inbound: a transport adapter (in tests, an mpsc channel; in
// production, a `PlatformAdapter` polling loop) feeds payloads into
// `node.receive(...)`. The handler is internal — no manual wiring.
// node.receive(&wire_bytes, &recv_ctx).await?;
```

There is no step where the caller constructs or registers a handler. The builder handles that internally. If a caller wants multiple receivers (for example, an observability sink in addition to the quota router handler), they can call `node.transport.register_receiver(...)` directly after `build()` — but that is an opt-in pattern, not part of the consumer contract.

#### `QuotaRouterHandler` (`handler.rs`)

```rust
pub struct QuotaRouterHandler {
    node: Arc<QuotaRouterNode>,
    provider: Arc<dyn LocalProvider>,
    network_key: [u8; 32],
}

impl NetworkReceiver for QuotaRouterHandler {
    async fn on_receive(&self, payload: &[u8], ctx: &ReceiveContext) -> Result<(), TransportError>;
}
```

Handler methods (all use `self.node` for state access and `self.node.transport` for outbound sends):

- `handle_forward_request` — TTL check, destination selection, dispatch or forward via `self.node.transport.send_best()`
- `handle_forward_response` — complete pending request via oneshot channel
- `handle_forward_reject` — reject pending request, trigger pull-gossip on CapacityExhausted
- `handle_capacity_gossip` — verify HMAC, merge capacities, merge known_peers
- `handle_router_announce` — verify HMAC, add peer if model overlap
- `handle_capacity_request` — build gossip snapshot, reply via `self.node.transport.send_best()`
- `handle_router_withdraw` — verify HMAC, remove peer

Helper methods:
- `send_forward_response` — build ForwardResponsePayload, send via `self.node.transport.send_best()`
- `send_forward_reject` — build ForwardRejectPayload, send via `self.node.transport.send_best()`

#### `build_with_bootstrap()` (`lib.rs`)

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct QuotaRouterBootstrap {
    pub seed_list_path: Option<PathBuf>,
    pub static_peers: Vec<PeerConfig>,
    pub timeout: Duration,
    pub min_peers: usize,
}

pub async fn build_with_bootstrap(
    config: RouterNodeConfig,
    bootstrap: QuotaRouterBootstrap,
) -> Result<Self, RouterNodeError>;
```

Tries `BootstrapOrchestrator` first, falls back to static peers.

### What this mission does NOT implement

- HMAC verification on inbound messages (0870d)
- Rate limiting (0870d)
- Prometheus metrics (0870d)

## Acceptance Criteria

- [ ] `QuotaRouterNode::route()` returns `Result<Vec<u8>, RouterNodeError>`
- [ ] `route()` dispatches locally when best destination is `Destination::Local`
- [ ] `route()` forwards via `ForwardRequest` when best destination is `Destination::Remote`
- [ ] `route()` awaits `ForwardOutcome` with `forward_timeout` (30s default)
- [ ] `route()` returns `RouterNodeError::ForwardRejected` on `ForwardOutcome::Rejected`
- [ ] `route()` returns `RouterNodeError::ForwardTimeout` on timeout
- [ ] `QuotaRouterNode::receive()` is reachable as `pub async fn`
- [ ] `receive(payload, ctx)` delegates to `self.transport.dispatch(payload, ctx)` — symmetric to `route()`
- [ ] Builder auto-registers the handler as a `NetworkReceiver` (no caller-side wiring)
- [ ] `QuotaRouterHandler` implements `NetworkReceiver` with 7 discriminator dispatch arms
- [ ] `handle_forward_request` uses `DropAction` enum to avoid Mutex-held-across-await
- [ ] `handle_forward_response` completes pending request via oneshot
- [ ] `handle_forward_reject` rejects pending request and triggers pull-gossip
- [ ] `handle_capacity_gossip` verifies HMAC before merging
- [ ] `handle_router_announce` verifies HMAC before adding peer
- [ ] `handle_capacity_request` builds and sends gossip reply
- [ ] `handle_router_withdraw` verifies HMAC and removes peer
- [ ] `send_forward_response`/`send_forward_reject` use `self.node.transport`
- [ ] `build_with_bootstrap` tries `BootstrapOrchestrator`, falls back to static peers
- [ ] `QuotaRouterBootstrap` config struct exists with `seed_list_path`, `static_peers`, `timeout`, `min_peers`
- [ ] Integration test: two nodes, one forwards request to the other, response routes back
- [ ] Unit tests pass for all handler methods
- [ ] Clippy clean, `cargo fmt --check` passes

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `QuotaRouterNode::route()` | This mission |
| `QuotaRouterHandler` (full impl) | This mission |
| `QuotaRouterHandler::handle_forward_request` | This mission |
| `QuotaRouterHandler::handle_forward_response` | This mission |
| `QuotaRouterHandler::handle_forward_reject` | This mission |
| `QuotaRouterHandler::handle_capacity_gossip` | This mission |
| `QuotaRouterHandler::handle_router_announce` | This mission |
| `QuotaRouterHandler::handle_capacity_request` | This mission |
| `QuotaRouterHandler::handle_router_withdraw` | This mission |
| `QuotaRouterHandler::send_forward_response` | This mission |
| `QuotaRouterHandler::send_forward_reject` | This mission |
| `QuotaRouterNode::build_with_bootstrap()` | This mission |
| `QuotaRouterBootstrap` struct | This mission |
| `DropAction` enum | This mission |
| `serialize`/`deserialize` module helpers (bincode) | This mission |

## Complexity

High (~600-800 lines). Full inbound handler + route API + bootstrap integration + integration tests.

## Implementation Notes

- The handler holds `Arc<QuotaRouterNode>` directly (no Mutex). Concurrency safety is provided by `GossipCache` and `PeerCache` using internal `RwLock`s — readers (`route`) and writers (handler) never block each other. Outbound sends go through `self.node.transport` (the same transport `route` uses).
- `route()` inserts into `PendingRequests` before sending the `ForwardRequest`, so the response can be routed back.
- `handle_forward_request` uses `DropAction` enum: scoring is synchronous (under lock), dispatch/forward is async (lock released).
- `build_with_bootstrap` requires `octo-transport/src/bootstrap.rs` to be functional — if the stub is not fixed, this method falls back to static peers only.
- Integration test should use `MockLocalProvider` (returns canned responses) and `MockNetworkSender` (records sent payloads).
