# Mission: 0870d — Network hardening: HMAC verification, rate limiting, metrics

## Status

Completed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network — Phase 4: Network Hardening

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types
- 0870b (must complete first) — gossip, HMAC signing
- 0870c (must complete first) — handler, route API

## Summary

Add HMAC verification on all inbound messages (`PeerTrust::Verified` mode), rate limiting per consumer and per peer (token bucket), Prometheus metrics for forwarding latency/gossip bandwidth/provider health, adversarial tests (TTL exhaustion, capacity manipulation, amplification), and performance benchmarks. This mission hardens the network for production deployment.

## Design

### HMAC verification on inbound messages

Currently, `handle_capacity_gossip` and `handle_router_announce` verify HMAC. This mission extends verification to:

- `ForwardRequest` — when `PeerTrust::Verified` is configured for the sending peer
- `CapacityRequest` — optional verification
- `RouterWithdraw` — already verified (added in v1.6)

### Rate limiting

```rust
pub struct RateLimiter {
    /// Per-consumer token bucket.
    consumer_buckets: BTreeMap<[u8; 32], TokenBucket>,
    /// Per-peer token bucket.
    peer_buckets: BTreeMap<RouterNodeId, TokenBucket>,
    /// Default rate: 100 req/s sustained, 500 burst.
    default_config: RateLimitConfig,
}

pub struct RateLimitConfig {
    pub max_sustained: u32,   // tokens per second
    pub max_burst: u32,       // burst capacity
}
```

Rate limiting is applied:
- At `QuotaRouterNode::route()` — per-consumer check before dispatch
- At `QuotaRouterHandler::handle_forward_request` — per-peer check before processing

### Prometheus metrics

Uses the `prometheus` crate (already a workspace dependency per RFC-0937). All metric types (`Histogram`, `Counter`, `GaugeVec`, `Gauge`) are from `prometheus::*`.

```rust
pub struct QuotaRouterMetrics {
    /// Forwarding latency histogram (per hop count).
    pub forwarding_latency: Histogram,
    /// Gossip bandwidth counter (bytes/sec).
    pub gossip_bytes: Counter,
    /// Provider health gauge (per provider).
    pub provider_health: GaugeVec,
    /// Active forwarded requests gauge.
    pub active_forwards: Gauge,
    /// Request outcome counter (local_success, remote_success, rejected, timeout).
    pub request_outcomes: CounterVec,
}
```

### Adversarial tests

| Test | Description |
|------|-------------|
| TTL exhaustion | Forward request with TTL=1 through 5-node chain — verify it dies at hop 2 |
| Capacity manipulation | Node gossips fake capacity (1M remaining) — verify scoring still works correctly |
| Amplification | Single malicious node forwards to all peers — verify TTL + rate limiting caps fan-out |
| HMAC forgery | Send gossip with wrong HMAC — verify it's dropped |
| Peer cache overflow | Add 200 peers — verify LRU eviction keeps cache at 128 |

### Performance benchmarks

| Benchmark | Target |
|-----------|--------|
| `select_destinations` (100 providers) | < 1ms |
| `route()` local dispatch | < 5ms overhead |
| `broadcast_gossip` (8 providers) | < 2ms |
| HMAC compute + verify | < 0.1ms |

### What this mission does NOT implement

- On-chain settlement (future — RFC-0900 Phase 2)
- DHT-based routing (F3)
- Streaming response forwarding (F8)

## Acceptance Criteria

- [ ] HMAC verification on `ForwardRequest` when peer trust is `Verified`
- [ ] `RateLimiter` struct with per-consumer and per-peer token buckets
- [ ] Rate limit check in `route()` before dispatch
- [ ] Rate limit check in `handle_forward_request` before processing
- [ ] `QuotaRouterMetrics` struct with forwarding latency, gossip bytes, provider health, active forwards, request outcomes
- [ ] Metrics wired into `route()`, `broadcast_gossip`, `broadcast_announce`, handler methods
- [ ] Adversarial test: TTL exhaustion across multi-hop chain
- [ ] Adversarial test: capacity manipulation doesn't break scoring
- [ ] Adversarial test: amplification capped by TTL + rate limiting
- [ ] Adversarial test: HMAC forgery rejected
- [ ] Adversarial test: peer cache overflow triggers LRU eviction
- [ ] Performance benchmark: `select_destinations` < 1ms for 100 providers
- [ ] Performance benchmark: `route()` local dispatch < 5ms overhead
- [ ] Performance benchmark: HMAC compute + verify < 0.1ms
- [ ] All existing tests still pass
- [ ] Clippy clean, `cargo fmt --check` passes

## Type Coverage

| RFC Type | Implemented By |
|----------|---------------|
| HMAC verification on ForwardRequest | This mission |
| `RateLimiter` struct | This mission |
| `RateLimitConfig` struct | This mission |
| `TokenBucket` struct (internal to RateLimiter) | This mission |
| `QuotaRouterMetrics` struct | This mission |
| Adversarial tests (5 scenarios) | This mission |
| Performance benchmarks (4 targets) | This mission |

## Complexity

Medium (~400-600 lines). Rate limiter + metrics + adversarial tests + benchmarks.

## Implementation Notes

- Rate limiter uses `TokenBucket` algorithm (simple, well-understood). No external crate needed — implement with `AtomicU64` + timestamp.
- Prometheus metrics use the `prometheus` crate (already a dependency in the workspace per RFC-0937).
- Adversarial tests use mock providers and mock network senders — no real API calls.
- Performance benchmarks use `criterion` crate or `tokio::test` with `Instant` timing.
- HMAC verification on `ForwardRequest` is optional — controlled by `PeerTrust` config per peer. Default is `Trusted` (no verification) for v1.
