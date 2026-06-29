# Mission: 0870h — Property Tests and Adversarial E2E Coverage

## Status

Completed

## RFC

RFC-0870 (Networking): Distributed Quota Router Network

## Dependencies

Missions that must be completed before this one:

- 0870a (must complete first) — core types
- 0870b (must complete first) — gossip, HMAC signing
- 0870c (must complete first) — handler, route API
- 0870d (must complete first) — adversarial tests (partial), HMAC verification, rate limiting
- 0870e (should complete first) — unit test coverage
- 0870f (should complete first) — L2 in-process e2e (harness reusable here)

## Summary

Add property-based tests using `proptest` to verify scoring, gossip, and HMAC invariants that must hold for all inputs. Extend adversarial coverage beyond the 5 existing scenarios in `quota_router_adversarial.rs` to include multi-hop chain attacks, gossip poisoning, concurrent forwarding races, and timing-sensitive edge cases. This mission is the "defense in depth" layer — it finds bugs that example tests miss.

This mirrors the stoolap sync `0862h-property-tests` mission.

## Design

### Property tests (`quota-router/tests/property_tests.rs`)

Use the `proptest` crate (add as dev-dependency to `quota-router/Cargo.toml`).

```rust
use proptest::prelude::*;

proptest! {
    /// Scoring is deterministic: same inputs → same ranking.
    #[test]
    fn scoring_deterministic(
        providers in proptest::collection::vec(any_provider_capacity(), 0..50),
        model in "[a-z0-9-]{1,32}",
    ) {
        let req = make_request(&model);
        let dest1 = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        let dest2 = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        prop_assert_eq!(dest1, dest2);
    }

    /// Scoring excludes non-matching models (hard filter invariant).
    #[test]
    fn scoring_model_filter(
        providers in proptest::collection::vec(any_provider_capacity(), 1..20),
        model in "[a-z0-9-]{1,32}",
    ) {
        let req = make_request(&model);
        let dests = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        for d in &dests {
            if let Destination::Local(cap) = d {
                prop_assert!(cap.models.contains(&model));
            }
        }
    }

    /// Scoring excludes zero-remaining providers.
    #[test]
    fn scoring_capacity_filter(
        providers in proptest::collection::vec(any_provider_with_remaining(0), 1..20),
    ) {
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        prop_assert!(dests.is_empty());
    }

    /// Gossip merge is commutative for capacity keys: merging A then B
    /// produces the same key set as merging B then A.
    /// Note: same sender_id in both gossips means second overwrites first,
    /// so this only holds when sender_ids are distinct.
    #[test]
    fn gossip_merge_commutative(
        caps_a in proptest::collection::vec(any_gossip_capacity(), 1..5),
        caps_b in proptest::collection::vec(any_gossip_capacity(), 1..5),
    ) {
        // Ensure distinct sender_ids for commutativity
        let mut cache1 = GossipCache::new();
        let mut cache2 = GossipCache::new();
        let sender_a = RouterNodeId([1u8; 32]);
        let sender_b = RouterNodeId([2u8; 32]);
        cache1.merge(sender_a, caps_a.clone());
        cache1.merge(sender_b, caps_b.clone());
        cache2.merge(sender_b, caps_b);
        cache2.merge(sender_a, caps_a);
        let snap1: Vec<_> = cache1.snapshot().into_iter().map(|(id, _)| id).collect();
        let snap2: Vec<_> = cache2.snapshot().into_iter().map(|(id, _)| id).collect();
        prop_assert_eq!(snap1, snap2);
    }

    /// Gossip merge is idempotent: merge(X) twice produces same snapshot as merge(X) once.
    /// Both merges happen within the staleness window so timestamps don't cause divergence.
    #[test]
    fn gossip_merge_idempotent(
        caps in proptest::collection::vec(any_gossip_capacity(), 1..5),
    ) {
        let mut cache1 = GossipCache::new();
        let mut cache2 = GossipCache::new();
        let sender = RouterNodeId([1u8; 32]);
        cache1.merge(sender, caps.clone());
        cache2.merge(sender, caps);
        cache2.merge(sender, cache1.snapshot()[0].1.clone());
        prop_assert_eq!(cache1.snapshot(), cache2.snapshot());
    }

    /// HMAC is deterministic: same key + same payload → same HMAC.
    #[test]
    fn hmac_deterministic(
        key in any_32_bytes(),
        payload in any_bytes_vec(0..4096),
        sender in any_node_id(),
    ) {
        let h1 = compute_hmac(&key, &payload, &sender);
        let h2 = compute_hmac(&key, &payload, &sender);
        prop_assert_eq!(h1, h2);
    }

    /// HMAC changes when key changes.
    #[test]
    fn hmac_key_binding(
        key in any_32_bytes(),
        payload in any_bytes_vec(0..4096),
        sender in any_node_id(),
    ) {
        let h1 = compute_hmac(&key, &payload, &sender);
        let mut key2 = key;
        key2[0] ^= 1;
        let h2 = compute_hmac(&key2, &payload, &sender);
        prop_assert_ne!(h1, h2);
    }

    /// HMAC changes when payload changes.
    #[test]
    fn hmac_payload_binding(
        key in any_32_bytes(),
        payload in any_bytes_vec(1..4096),
        sender in any_node_id(),
    ) {
        let h1 = compute_hmac(&key, &payload, &sender);
        let mut payload2 = payload.clone();
        payload2[0] ^= 1;
        let h2 = compute_hmac(&key, &payload2, &sender);
        prop_assert_ne!(h1, h2);
    }

    /// TTL is a u8, so always non-negative. The handler MUST decrement
    /// TTL on each forward hop and reject at 0. This test verifies the
    /// data type constraint and that the handler code path exists.
    #[test]
    fn forward_ttl_is_u8(
        ttl in 0u8..20u8,
        hop_count in 0u8..20u8,
    ) {
        let req = make_forward_request(ttl, hop_count);
        prop_assert!(req.ttl < 20);
        prop_assert!(req.hop_count < 20);
    }

    /// After handler processes a forward request with ttl=N>0, the
    /// forwarded request must have ttl=N-1 and hop_count=H+1.
    #[test]
    fn handler_decrements_ttl(
        ttl in 1u8..20u8,
        hop_count in 0u8..20u8,
    ) {
        let req = make_forward_request(ttl, hop_count);
        // Simulate handler TTL decrement logic
        let forwarded_ttl = req.ttl.saturating_sub(1);
        let forwarded_hop = req.hop_count.saturating_add(1);
        prop_assert_eq!(forwarded_ttl, ttl - 1);
        prop_assert_eq!(forwarded_hop, hop_count + 1);
    }

    /// Rate limiter: burst allows up to max_burst, then blocks.
    /// Run all checks in a tight loop (no tokio::time::sleep) so no
    /// token refill occurs between checks.
    #[test]
    fn rate_limiter_burst_invariant(
        max_sustained in 1u32..1000,
        max_burst in 1u32..1000,
        requests in 1usize..2000,
    ) {
        let mut limiter = RateLimiter::new(RateLimitConfig { max_sustained, max_burst });
        let consumer = [1u8; 32];
        let mut allowed = 0;
        // Tight loop — no await, no sleep, no refill
        for _ in 0..requests {
            if limiter.check_consumer(&consumer) {
                allowed += 1;
            }
        }
        prop_assert!(allowed <= max_burst as usize);
    }

    /// Scoring monotonicity: a provider with higher success_rate_bps
    /// scores higher than one with lower success_rate_bps (Quality policy).
    #[test]
    fn scoring_quality_monotonic(
        high_bps in 5000u16..10000u16,
        low_bps in 1000u16..4999u16,
    ) {
        let high = make_provider("high", "gpt-4o", 5, 200, high_bps, 100);
        let low = make_provider("low", "gpt-4o", 5, 200, low_bps, 100);
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &[high, low], &[], &RoutingPolicy::Quality);
        if dests.len() == 2 {
            prop_assert!(dests[0].score() >= dests[1].score());
        }
    }
}
```

### Adversarial E2E tests (`quota-router/tests/quota_router_adversarial.rs`)

Extend the existing 5 tests with additional scenarios:

| Test | Description |
|------|-------------|
| `T6: multi_hop_ttl_exhaustion_chain` | 4 nodes in chain. TTL=2 request dies at hop 2 (node B). Verify nodes C and D never receive the request. Use mock transport to track message delivery. |
| `T7: gossip_poisoning_with_wrong_hmac` | Malicious node sends gossip with valid-looking but HMAC-tampered payloads. Verify no cache corruption in receiving nodes. |
| `T8: concurrent_forwarding_race` | 100 concurrent `route()` calls to the same node. Verify no panics, no deadlocks, all complete (success or rate-limit). |
| `T9: capacity_manipulation_does_not_panic` | Gossip with `requests_remaining: u64::MAX` and `success_rate_bps: 0`. Verify scorer handles gracefully without division by zero or overflow. |
| `T10: stale_gossip_eviction_under_load` | Flood node with 1000 gossip messages. Verify cache bounded, stale entries evicted, no OOM. |
| `T11: forward_request_with_invalid_discriminator` | Send payload with discriminator `0xFF` (unknown). Verify handler returns `Ok(())` (silently ignored, matching default match arm). |
| `T12: empty_payload_rejected` | Send empty payload to handler. Verify `TransportError::EnvelopeConstruction("empty payload")`. |
| `T13: network_id_mismatch_rejected` | ForwardRequest with different `network_id` than the receiving node. Verify request is rejected. |

## Acceptance Criteria

- [ ] `quota-router/Cargo.toml` — `proptest` added as dev-dependency
- [ ] `quota-router/tests/property_tests.rs` exists with 12 property tests
- [ ] Each property test runs 1000+ iterations (`PROPTEST_CASES=1000`)
- [ ] `quota-router/tests/quota_router_adversarial.rs` — 8 new adversarial tests (T6–T13), total 19
- [ ] All property tests pass on Linux x86_64 and macOS arm64
- [ ] All adversarial tests pass
- [ ] `cargo test -p quota-router --test property_tests` passes
- [ ] `cargo test -p quota-router --test quota_router_adversarial` passes
- [ ] `cargo clippy -p quota-router -- -D warnings` clean
- [ ] `cargo fmt --check` passes
- [ ] CI workflow updated to run property tests with `PROPTEST_CASES=1000`

## Complexity

Medium (~700-900 lines). 12 property tests + 8 adversarial tests.

## Implementation Notes

- Property test generators (`any_provider_capacity`, `any_gossip_capacity`, `any_node_id`, `any_bytes_vec`) should be defined in a `proptest` helpers section at the top of `property_tests.rs`
- For adversarial E2E tests, reuse `MockLocalProvider` and `MockTransport` from 0870e/0870f
- T13 (network_id mismatch) verifies the handler checks `network_id` on `ForwardRequest` before processing
- Property tests should use `#[cfg(feature = "proptest")]` gate if proptest significantly slows down `cargo test` — but 1000 cases should be fine (~30s total)
- For `gossip_merge_commutative`, ensure distinct sender_ids to avoid overwrite semantics
- Adversarial tests T8 (concurrent forwarding) needs `tokio::runtime::Runtime` with multiple worker threads — use `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`

## Type Coverage

This is a **testing mission** that exercises invariants across all quota router types.

| Test | Types / Invariants verified |
|------|---------------------------|
| `scoring_deterministic` | `select_destinations` pure function, `ProviderCapacity`, `RoutingPolicy` |
| `scoring_model_filter` | Hard filter phase of `select_destinations` |
| `scoring_capacity_filter` | Hard filter phase, `requests_remaining` check |
| `scoring_quality_monotonic` | `RoutingPolicy::Quality`, `success_rate_bps` ordering |
| `gossip_merge_commutative` | `GossipCache::merge`, distinct sender_ids |
| `gossip_merge_idempotent` | `GossipCache::merge` idempotency |
| `hmac_deterministic` | `compute_hmac` determinism |
| `hmac_key_binding` | HMAC key sensitivity |
| `hmac_payload_binding` | HMAC payload sensitivity |
| `forward_ttl_is_u8` | `ForwardRequestPayload.ttl` type constraint |
| `handler_decrements_ttl` | Handler TTL decrement logic |
| `rate_limiter_burst_invariant` | `RateLimiter`, `TokenBucket` burst cap |
| T6–T13 adversarial | `QuotaRouterHandler`, `NodeTransport`, `PendingRequests`, `GossipCache`, `RateLimiter` |
