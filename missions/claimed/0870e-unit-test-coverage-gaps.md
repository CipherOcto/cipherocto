# Mission: 0870e — Unit Test Coverage for Untested Modules

## Status

Completed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types, scoring, forwarding
- 0870b (must complete first) — gossip, HMAC signing, SignedPayload
- 0870c (must complete first) — handler, route API, build_with_bootstrap
- 0870d (must complete first) — HMAC verification, rate limiting, metrics

## Summary

Fill unit test coverage gaps in 5 source modules that currently have zero tests (handler.rs, provider.rs, forward.rs, request.rs, gossip.rs), and add missing scenario coverage to modules with partial gaps (scorer.rs, ratelimit.rs, lib.rs). This mission ensures every module compiles with a solid baseline before integration testing begins.

## Test Gap Analysis

### Modules with 0 tests (781 lines of untested production code)

| Module | Lines | What needs testing |
|--------|-------|--------------------|
| `handler.rs` | 307 | `on_receive` discriminator dispatch (0xC3–0xCB: ForwardRequest, ForwardResponse, ForwardReject, CapacityGossip, CapacityRequest, RouterAnnounce, RouterWithdraw), `handle_forward_request` (TTL check, destination selection, DropAction dispatch, TTL decrement, hop_count increment), `handle_forward_response` (response routing to pending request), `handle_forward_reject` (rejection routing, CapacityRequest trigger), `handle_capacity_gossip` (HMAC verify, capacity merge, known_peers merge), `handle_capacity_request` (build gossip + reply), `handle_router_withdraw` (HMAC verify, peer removal) |
| `provider.rs` | 219 | `ProviderCapacity::from_config` conversion, `HttpLocalProvider::new` API key extraction from `ProviderAuth` variants, `LocalProvider` trait (completion, health_check, supported_models) |
| `forward.rs` | 124 | `ForwardRequestPayload` serialize/deserialize roundtrip, `ForwardResponsePayload` roundtrip, `ForwardRejectPayload` roundtrip, `CapacityRequestPayload` roundtrip, `ForwardRejectReason` all 8 variants, `PendingRequests` insert/complete/reject/cancel/origin |
| `request.rs` | 62 | `RequestContext` construction and field defaults, `RoutingPolicy` all 6 variants, `ForwardingConfig` defaults |
| `gossip.rs` | 69 | `CapacityGossipPayload` serialize/deserialize roundtrip, `GossipCache::new`/`merge`/`snapshot` (only tested indirectly via lib.rs basic test) |

### Partial gaps in tested modules

| Module | Existing tests | Missing scenarios |
|--------|---------------|-------------------|
| `scorer.rs` | 10 tests | `RoutingPolicy::Quality`, `RoutingPolicy::Custom`, `model_group` filtering, `preferred_provider` with remote providers |
| `ratelimit.rs` | 4 tests | Token refill over time (temporal behavior), zero-config defaults |
| `lib.rs` | 7 tests | `route()` end-to-end (local dispatch + remote forwarding), `broadcast_gossip`/`broadcast_announce`, `add_peer`, `local_provider_models`, `primary_provider_id` |

## Design

### Test helpers needed

Create a `#[cfg(test)] mod test_helpers` in each module (or a shared `quota-router/src/test_helpers.rs` module):

```rust
// test_helpers.rs — shared mock infrastructure
use async_trait::async_trait;
use crate::provider::{LocalProvider, ProviderCapacity, ProviderHealth};

pub struct MockLocalProvider {
    pub model_list: Vec<String>,
    pub health: ProviderHealth,
}

#[async_trait]
impl LocalProvider for MockLocalProvider {
    async fn completion(&self, model: &str, _messages: &[u8], _params: &ProviderCapacity) -> Result<Vec<u8>, crate::provider::ProviderError> {
        Ok(format!("mock-response-{}", model).into_bytes())
    }
    async fn health_check(&self) -> ProviderHealth { self.health.clone() }
    fn supported_models(&self) -> Vec<String> { self.model_list.clone() }
}
```

### Handler tests (highest priority — 307 lines, 0 tests)

The handler is the core dispatch path. Tests must cover:

1. **Discriminator dispatch** — `on_receive` routes 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xCA, 0xCB to the correct handler method; unknown discriminators silently ignored
2. **Forward request with TTL=0** — returns `DropAction::Reject` (TtlExpired)
3. **Forward request with local provider match** — returns `DropAction::LocalDispatch`
4. **Forward request with remote peer match** — returns `DropAction::Forward` (decrements TTL, increments hop_count)
5. **Forward response routing** — payload delivered to the correct pending request entry
6. **Forward reject routing** — `CapacityExhausted` triggers `CapacityRequest` to sender
7. **Capacity gossip merge** — gossip from peer updates local cache with new capacities and known_peers
8. **Capacity gossip HMAC failure** — malformed HMAC drops the gossip silently
9. **Capacity request response** — handler builds gossip payload and sends via transport
10. **Router withdraw** — HMAC-verified withdraw removes peer from cache

### Provider tests

1. `ProviderCapacity::from_config` — correct `provider_id` derivation (blake3 hash of name|node_id)
2. `HttpLocalProvider::new` — API key extraction from `ProviderAuth::ApiKey`, `Bearer`, `OAuth`
3. `LocalProvider` trait — MockLocalProvider satisfies the trait (compile-time check)
4. `ProviderHealth` — all 4 variants (Healthy, Degraded, Unavailable, Unknown) serialize/deserialize

### Forward tests

1. `ForwardRequestPayload` roundtrip — bincode serialize → deserialize, all fields preserved
2. `ForwardResponsePayload` roundtrip
3. `ForwardRejectPayload` roundtrip
4. `CapacityRequestPayload` roundtrip
5. `PendingRequests::insert` + `complete` — insert a pending entry, complete with response, get `ForwardOutcome::Completed`
6. `PendingRequests::insert` + `reject` — reject with reason, get `ForwardOutcome::Rejected`
7. `PendingRequests::insert` + `cancel` — cancel without response (simulates send failure)
8. `PendingRequests::origin` — retrieve origin node for a given request_id
9. All 8 `ForwardRejectReason` variants serialize/deserialize (TtlExpired, NoProvider, ModelNotSupported, CapacityExhausted, ContextWindowExceeded, BudgetExceeded, AuthFailure, PayloadTooLarge)

### Request tests

1. `RequestContext` — construction with all 12 fields (model, preferred_provider, model_group, input_tokens, max_output_tokens, tags, max_price_per_1k_tokens, max_latency_ms, policy_override, consumer_id, priority, deadline)
2. `RoutingPolicy` — all 6 variants constructible, `Custom` variant holds `CustomPolicy`
3. `ForwardingConfig` — default values (max_ttl=3, max_concurrent_forwards=64, forward_timeout=30s, max_payload_bytes=1MB)

### Gossip tests

1. `CapacityGossipPayload` roundtrip — bincode serialize → deserialize
2. `GossipCache::merge(sender_id, capacities)` — merge capacities for a sender, verify cache updated with correct sender_id key
3. `GossipCache::snapshot` — snapshot returns non-stale entries, filters out entries older than STALENESS_THRESHOLD
4. `GossipCache::merge` overwrite — merge same sender_id twice with different capacities, verify latest wins
5. `GossipCache::merge` multi-sender — merge from 3 different senders, verify all appear in snapshot

### Scorer gap fills

1. `Quality` policy — prefers highest `success_rate_bps`
2. `Custom` policy — applies custom scoring function
3. `model_group` filtering — providers not in the requested model group are excluded
4. `preferred_provider` with remote providers — remote peer with matching provider is ranked first

### Ratelimit gap fills

1. Token refill — after burst exhaustion, tokens refill at `max_sustained` rate
2. Zero-config defaults — `RateLimiter::new()` uses 100/s sustained, 500 burst

### Lib gap fills

1. `route()` with local provider — dispatches locally, returns `Ok(ForwardOutcome::Completed)`
2. `route()` with no local provider — forwards to peer, returns response
3. `route()` rate limit exceeded — returns `Err(RouterNodeError::RateLimited)`
4. `route()` with `RoutingPolicy::LocalOnly` — prevents forwarding, returns local-only result
5. `add_peer` — adds peer to cache, peer_count increases
6. `local_provider_models` — returns list of models from primary provider

### RFC Test Vector coverage (RFC-0870 §Test Vectors)

These unit tests directly implement the 4 canonical test vectors from the RFC:

1. **Test Vector 1: Model ID Primary Filter** — Multi-provider scoring with Balanced policy. Covered by `scorer.rs` existing test `model_filter_excludes_non_matching` + new Quality policy test.
2. **Test Vector 2: TTL Expiration** — TTL=0 yields `TtlExpired`. Covered by `handler.rs` test #2 (Forward request with TTL=0).
3. **Test Vector 3: Budget Filter** — Price > budget filters out provider. Covered by `scorer.rs` existing test `budget_filter_excludes_expensive`.
4. **Test Vector 4: Capacity Gossip Merge** — Gossip with capacities + known_peers merges into caches. Covered by `gossip.rs` tests #2–#5 (merge, snapshot, overwrite, multi-sender).

## Acceptance Criteria

- [ ] `quota-router/src/test_helpers.rs` exists with `MockLocalProvider` and `MockTransport` (implements `NetworkSender` with in-memory channel delivery)
- [ ] `handler.rs` — 10 unit tests covering all dispatch paths (0xC3–0xCB discriminators)
- [ ] `provider.rs` — 4 unit tests covering `from_config`, `new`, health check, model list
- [ ] `forward.rs` — 9 unit tests covering roundtrips, PendingRequests operations, and cancel
- [ ] `request.rs` — 3 unit tests covering `RequestContext` (12 fields), `RoutingPolicy` (6 variants), `ForwardingConfig` (max_ttl=3 defaults)
- [ ] `gossip.rs` — 5 unit tests covering roundtrip, merge, snapshot, overwrite, multi-sender
- [ ] `scorer.rs` — 4 new tests for Quality, Custom, model_group, preferred_provider remote
- [ ] `ratelimit.rs` — 2 new tests for refill and defaults
- [ ] `lib.rs` — 6 new tests for route() (local, remote, rate-limited, LocalOnly), add_peer, local_provider_models
- [ ] Total new tests: ~43 (bringing total from 50 to ~93)
- [ ] `cargo test -p quota-router` passes all tests
- [ ] `cargo clippy -p quota-router -- -D warnings` clean
- [ ] `cargo fmt --check` passes

## Complexity

Medium (~700-900 lines). Test helpers + 43 new unit tests.

## Implementation Notes

- Use `#[cfg(test)]` modules within each source file (follow existing pattern in `scorer.rs`, `ratelimit.rs`)
- `MockTransport` should use `tokio::sync::mpsc` channels to deliver messages to target handlers — this enables L2 tests later
- `MockLocalProvider` should be configurable (health status, model list, response content)
- Handler tests need to construct a full `QuotaRouterNode` with a mock transport — use the builder with `MockLocalProvider`
- For `PendingRequests` tests, use `std::time::Instant` for timeout testing (inject clock or use real time with generous timeout)
- The `gossip.rs` `merge` test needs to set `last_updated` timestamps to test staleness eviction

## Type Coverage

This is a **testing mission** that exercises types defined by 0870a–0870d.

| Module tested | Types exercised |
|---------------|-----------------|
| `handler.rs` | `QuotaRouterHandler`, `NetworkReceiver`, `DropAction` |
| `provider.rs` | `ProviderCapacity`, `HttpLocalProvider`, `ProviderAuth`, `ProviderConfig` |
| `forward.rs` | `ForwardRequestPayload`, `ForwardResponsePayload`, `ForwardRejectPayload`, `PendingRequests`, `ForwardOutcome` |
| `request.rs` | `RequestContext`, `RoutingPolicy`, `ForwardingConfig` |
| `gossip.rs` | `CapacityGossipPayload`, `GossipCache` |
| `scorer.rs` | `select_destinations`, `Destination` |
| `ratelimit.rs` | `RateLimiter`, `TokenBucket` |
| `lib.rs` | `QuotaRouterNode`, `QuotaRouterNodeBuilder`, `PeerCache` |
