# Mission: 0870a — Base: Core types, QuotaRouterNode, scoring, forwarding

## Status

Completed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network — Phase 1: Core Router Node

## Dependencies

Missions that must be completed before this one:

- RFC-0863 accepted (✅) — `NodeTransport`, `NetworkSender`, `SendContext`
- RFC-0850 accepted (✅) — envelope wire format, platform adapters
- RFC-0126 accepted (✅) — deterministic serialization

## Summary

Create the `quota_router/` module tree under `octo-transport/src/` and implement all core types, the `QuotaRouterNode` struct with its builder, the two-phase destination selection algorithm, `ForwardRequest`/`ForwardResponse`/`ForwardReject` envelope types, and the `LocalProvider` trait with `HttpLocalProvider`. This is the foundation — all subsequent 0870 missions depend on it.

## Design

### Module layout

```
octo-transport/src/quota_router/
├── mod.rs          — QuotaRouterNode, RouterNodeConfig, lifecycle, builder
├── provider.rs     — LocalProvider trait, ProviderCapacity, HttpLocalProvider
├── scorer.rs       — select_destinations algorithm, Destination enum
├── forward.rs      — ForwardRequestPayload, ForwardResponsePayload, ForwardRejectPayload, PendingRequests
├── request.rs      — RequestContext, RoutingPolicy, ForwardingConfig
├── handler.rs      — QuotaRouterHandler (stub — Phase 3 fills in)
├── gossip.rs       — CapacityGossipPayload, GossipCache (stub — Phase 2 fills in)
└── announce.rs     — RouterAnnouncePayload, RouterWithdrawPayload (stub — Phase 2 fills in)
```

### Types to implement

#### Core types (`mod.rs`)

```rust
use std::net::SocketAddr;

pub struct RouterNodeId(pub [u8; 32]);
pub struct ProviderId(pub [u8; 32]);
pub struct NetworkId(pub [u8; 32]);

pub struct QuotaRouterNode {
    pub config: RouterNodeConfig,
    pub state: RouterNodeLifecycle,
    pub transport: NodeTransport,
    pub gossip_cache: GossipCache,
    pub peer_cache: PeerCache,
    pending: PendingRequests,
    pub keypair: Keypair,
    primary_provider: Arc<dyn LocalProvider>,
}

pub struct QuotaRouterNodeBuilder { ... }
```

#### Provider types (`provider.rs`)

```rust
#[async_trait]
pub trait LocalProvider: Send + Sync {
    async fn completion(&self, model: &str, messages: &[u8], params: &ProviderCapacity) -> Result<Vec<u8>, ProviderError>;
    async fn health_check(&self) -> ProviderHealth;
    fn supported_models(&self) -> Vec<String>;
}

pub struct ProviderCapacity { ... }
pub struct HttpLocalProvider { ... }
pub enum ProviderHealth { Healthy, Degraded, Unavailable, Unknown }
```

#### Scoring algorithm (`scorer.rs`)

```rust
pub fn select_destinations(
    request: &RequestContext,
    local_providers: &[ProviderCapacity],
    peer_capabilities: &[(RouterNodeId, Vec<ProviderCapacity>)],
    policy: &RoutingPolicy,
) -> Vec<Destination>;
```

#### Forwarding types (`forward.rs`)

```rust
pub struct ForwardRequestPayload { ... }
pub struct ForwardResponsePayload { ... }
pub struct ForwardRejectPayload { ... }
pub struct PendingRequests { ... }
pub enum ForwardOutcome { Completed(Vec<u8>), Rejected(ForwardRejectReason), Timeout }
pub enum ForwardRejectReason { TtlExpired, NoProvider, ModelNotSupported, ... }
```

#### Request types (`request.rs`)

```rust
pub struct RequestContext { ... }
pub enum RoutingPolicy { Cheapest, Fastest, Quality, Balanced, LocalOnly, Custom(CustomPolicy) }
pub struct ForwardingConfig { ... }
```

### What this mission does NOT implement

- Gossip broadcast/receive (0870b)
- `GossipCache` methods: `merge`, `snapshot` (0870b — 0870a provides struct + `new()` only)
- `PeerCache` methods: `add_direct`, `try_add`, `remove`, `total`, `direct_ids` (0870b — 0870a provides struct + `new()` only)
- `RouterAnnounce`/`RouterWithdraw` (0870b)
- `SignedPayload` trait (0870b)
- `QuotaRouterNode::route()` public API (0870c)
- `QuotaRouterHandler` full implementation (0870c — 0870a provides stub only)
- `build_with_bootstrap()` (0870c)
- `monotonic_now()` implementation (0870b — 0870a provides placeholder returning 0)
- HMAC verification (0870d)
- Rate limiting (0870d)

## Acceptance Criteria

- [ ] `octo-transport/src/quota_router/mod.rs` exists with `QuotaRouterNode`, `RouterNodeConfig`, `RouterNodeLifecycle` (7 states), `QuotaRouterNodeBuilder`
- [ ] `octo-transport/src/quota_router/provider.rs` exists with `LocalProvider` trait, `ProviderCapacity`, `ProviderCapacity::from_config`, `HttpLocalProvider`, `ProviderHealth`
- [ ] `octo-transport/src/quota_router/scorer.rs` exists with `select_destinations` implementing 3-phase algorithm (hard filters → soft scoring → ranking), `Destination` enum
- [ ] `octo-transport/src/quota_router/forward.rs` exists with `ForwardRequestPayload`, `ForwardResponsePayload`, `ForwardRejectPayload`, `PendingRequests`, `ForwardOutcome`, `ForwardRejectReason`
- [ ] `octo-transport/src/quota_router/request.rs` exists with `RequestContext` (14 fields), `RoutingPolicy` (6 variants), `CustomPolicy`, `ForwardingConfig`
- [ ] `QuotaRouterNodeBuilder::build()` returns `Result<QuotaRouterNode, RouterNodeError>` (handler creation deferred to 0870c)
- [ ] `QuotaRouterNodeBuilder` has setters for all config fields: `node_id`, `network_id`, `provider`, `peer`, `policy`, `forwarding`, `gossip_interval`
- [ ] Scoring function uses `ProviderHealth::` and `RoutingPolicy::` prefixes (compiles)
- [ ] All types have `#[derive(serde::Serialize, serde::Deserialize)]` where needed for wire format
- [ ] Unit tests pass: `cargo test -p octo-transport -- quota_router`
- [ ] Clippy clean: `cargo clippy -p octo-transport -- -D warnings`
- [ ] `cargo fmt --check` passes

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| `RouterNodeId`, `ProviderId`, `NetworkId` | This mission |
| `QuotaRouterNode` struct | This mission |
| `RouterNodeConfig` struct | This mission |
| `RouterNodeLifecycle` enum (7 states) | This mission |
| `QuotaRouterNodeBuilder` | This mission |
| `LocalProvider` trait | This mission |
| `HttpLocalProvider` struct | This mission |
| `ProviderCapacity` struct | This mission |
| `ProviderCapacity::from_config` | This mission |
| `ProviderHealth` enum | This mission |
| `select_destinations` function | This mission |
| `Destination` enum | This mission |
| `ForwardRequestPayload` struct | This mission |
| `ForwardResponsePayload` struct | This mission |
| `ForwardRejectPayload` struct | This mission |
| `PendingRequests` struct | This mission |
| `ForwardOutcome` enum | This mission |
| `ForwardRejectReason` enum | This mission |
| `RequestContext` struct | This mission |
| `RoutingPolicy` enum | This mission |
| `CustomPolicy` struct | This mission |
| `ModelOverride` struct | This mission |
| `ForwardingConfig` struct | This mission |
| `ProviderConfig` struct | This mission |
| `ProviderAuth` enum | This mission |
| `PeerConfig` struct | This mission |
| `PeerTrust` enum | This mission |
| `RouterNodeError` enum | This mission |
| `ProviderError` enum | This mission |
| `LocalProviderSender` (no-op adapter) | This mission |
| `CapacityGossipPayload` | 0870b |
| `GossipCache` (struct + methods) | 0870b |
| `RouterAnnouncePayload` | 0870b |
| `RouterWithdrawPayload` | 0870b |
| `SignedPayload` trait | 0870b |
| `QuotaRouterHandler` (full impl) | 0870c |
| `QuotaRouterBootstrap` | 0870c |
| `monotonic_now()` | 0870b |

## Complexity

Medium (~800-1000 lines). Core types + scoring algorithm + forwarding types + builder + tests.

## Implementation Notes

- Follow the `GovernedTransport` pattern from RFC-0863p-a: `QuotaRouterNode` wraps `NodeTransport`, adds domain-specific logic.
- The `select_destinations` function is pure (no side effects, no I/O) — easy to unit test with mock data.
- `PendingRequests` uses `std::sync::Mutex<BTreeMap>` (not tokio Mutex) because `complete`/`reject` are called from sync context.
- `monotonic_now()` uses an `AtomicU64` counter — see RFC-0870 §Data Structures.
- `DropAction` enum (private to handler.rs) is used to avoid Mutex-held-across-await in `handle_forward_request`.
- `LocalProviderSender` is a no-op `NetworkSender` adapter that satisfies `NodeTransport`'s constructor.
