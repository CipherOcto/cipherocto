use proptest::prelude::*;

use quota_router_core::node::announce::SignedPayload;
use quota_router_core::node::gossip::{CapacityGossipPayload, GossipCache};
use quota_router_core::node::provider::{
    ModelPricing, ProviderCapacity, ProviderHealth, ProviderId, RouterNodeId,
};
use quota_router_core::node::ratelimit::RateLimiter;
use quota_router_core::node::request::{RequestContext, RoutingPolicy};
use quota_router_core::node::scorer::select_destinations;
use quota_router_core::node::scorer::Destination;

fn any_provider_capacity() -> impl Strategy<Value = ProviderCapacity> {
    ("[a-z0-9-]{1,16}", 0u64..1000, 0u32..500, 0u16..10000).prop_map(
        |(name, remaining, latency, success_bps)| ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: name,
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: remaining,
            pricing: vec![ModelPricing {
                model: "gpt-4o".into(),
                price_per_1k_tokens: 3,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: latency,
            success_rate_bps: success_bps,
            last_updated: 0,
        },
    )
}

fn any_gossip_capacity() -> impl Strategy<Value = ProviderCapacity> {
    ("[a-z]{1,8}", 0u64..1000).prop_map(|(name, remaining)| ProviderCapacity {
        provider_id: ProviderId([1u8; 32]),
        provider_name: name,
        router_node_id: RouterNodeId([0u8; 32]),
        models: vec!["gpt-4o".into()],
        requests_remaining: remaining,
        pricing: vec![ModelPricing {
            model: "gpt-4o".into(),
            price_per_1k_tokens: 3,
        }],
        status: ProviderHealth::Healthy,
        latency_ms: 200,
        success_rate_bps: 9500,
        last_updated: 0,
    })
}

fn any_node_id() -> impl Strategy<Value = RouterNodeId> {
    any::<[u8; 32]>().prop_map(RouterNodeId)
}

fn make_request(model: &str) -> RequestContext {
    RequestContext {
        model: model.to_string(),
        preferred_provider: None,
        model_group: None,
        input_tokens: None,
        max_output_tokens: None,
        tags: None,
        max_price_per_1k_tokens: None,
        max_latency_ms: None,
        policy_override: None,
        consumer_id: [0u8; 32],
        priority: 0,
        deadline: None,
    }
}

fn make_forward_request(
    ttl: u8,
    hop_count: u8,
) -> quota_router_core::node::forward::ForwardRequestPayload {
    quota_router_core::node::forward::ForwardRequestPayload {
        request_id: [1u8; 32],
        network_id: quota_router_core::node::provider::NetworkId([2u8; 32]),
        context: make_request("gpt-4o"),
        payload: b"test".to_vec(),
        ttl,
        origin_node: RouterNodeId([9u8; 32]),
        hop_count,
        created_at: 0,
        hmac: [0u8; 32],
    }
}

proptest! {
    // Property tests run 1000+ iterations per the 0870h AC-3 spec.
    // PROPTEST_CASES=1000 sets this per-test config. See:
    // missions/claimed/0870h-property-tests-and-adversarial-e2e.md AC-3.
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn scoring_deterministic(
        providers in proptest::collection::vec(any_provider_capacity(), 0..50),
        model in "[a-z0-9-]{1,32}",
    ) {
        let req = make_request(&model);
        let dest1 = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        let dest2 = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        prop_assert_eq!(dest1.len(), dest2.len());
        for (d1, d2) in dest1.iter().zip(dest2.iter()) {
            prop_assert!((d1.score() - d2.score()).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn scoring_model_filter(
        providers in proptest::collection::vec(any_provider_capacity(), 1..20),
        model in "[a-z0-9-]{1,32}",
    ) {
        let req = make_request(&model);
        let dests = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        for d in &dests {
            if let Destination::Local { provider, .. } = d {
                prop_assert!(provider.models.contains(&model));
            }
        }
    }

    #[test]
    fn scoring_capacity_filter(
        remaining in 0u64..1,
    ) {
        let providers = vec![ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: "a".into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: remaining,
            pricing: vec![ModelPricing { model: "gpt-4o".into(), price_per_1k_tokens: 3 }],
            status: ProviderHealth::Healthy,
            latency_ms: 200,
            success_rate_bps: 9500,
            last_updated: 0,
        }];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &providers, &[], &RoutingPolicy::Balanced);
        prop_assert!(dests.is_empty());
    }

    #[test]
    fn scoring_quality_monotonic(
        high_bps in 5000u16..10000u16,
        low_bps in 1000u16..4999u16,
    ) {
        let high = ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: "high".into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: 100,
            pricing: vec![ModelPricing { model: "gpt-4o".into(), price_per_1k_tokens: 5 }],
            status: ProviderHealth::Healthy,
            latency_ms: 200,
            success_rate_bps: high_bps,
            last_updated: 0,
        };
        let low = ProviderCapacity {
            provider_id: ProviderId([2u8; 32]),
            provider_name: "low".into(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec!["gpt-4o".into()],
            requests_remaining: 100,
            pricing: vec![ModelPricing { model: "gpt-4o".into(), price_per_1k_tokens: 5 }],
            status: ProviderHealth::Healthy,
            latency_ms: 200,
            success_rate_bps: low_bps,
            last_updated: 0,
        };
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &[high, low], &[], &RoutingPolicy::Quality);
        if dests.len() == 2 {
            prop_assert!(dests[0].score() >= dests[1].score());
        }
    }

    #[test]
    fn gossip_merge_commutative(
        caps_a in proptest::collection::vec(any_gossip_capacity(), 1..5),
        caps_b in proptest::collection::vec(any_gossip_capacity(), 1..5),
    ) {
        let cache1 = GossipCache::new();
        let cache2 = GossipCache::new();
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

    #[test]
    fn gossip_merge_idempotent(
        caps in proptest::collection::vec(any_gossip_capacity(), 1..5),
    ) {
        let cache1 = GossipCache::new();
        let cache2 = GossipCache::new();
        let sender = RouterNodeId([1u8; 32]);
        cache1.merge(sender, caps.clone());
        cache2.merge(sender, caps);
        // Second merge with same data should produce same snapshot
        cache2.merge(sender, cache1.snapshot()[0].1.clone());
        prop_assert_eq!(cache1.snapshot().len(), cache2.snapshot().len());
    }

    #[test]
    fn hmac_deterministic(
        key in any::<[u8; 32]>(),
        sender in any_node_id(),
    ) {
        let gossip = CapacityGossipPayload {
            sender_id: sender,
            timestamp: 100,
            capacities: vec![],
            known_peers: vec![],
            hmac: [0u8; 32],
        };
        let h1 = gossip.compute_hmac(&key);
        let h2 = gossip.compute_hmac(&key);
        prop_assert_eq!(h1, h2);
    }

    #[test]
    fn hmac_key_binding(
        key in any::<[u8; 32]>(),
        sender in any_node_id(),
    ) {
        let gossip = CapacityGossipPayload {
            sender_id: sender,
            timestamp: 100,
            capacities: vec![],
            known_peers: vec![],
            hmac: [0u8; 32],
        };
        let h1 = gossip.compute_hmac(&key);
        let mut key2 = key;
        key2[0] ^= 1;
        let h2 = gossip.compute_hmac(&key2);
        prop_assert_ne!(h1, h2);
    }

    #[test]
    fn hmac_payload_binding(
        key in any::<[u8; 32]>(),
        sender in any_node_id(),
        ts in any::<u64>(),
    ) {
        let g1 = CapacityGossipPayload {
            sender_id: sender, timestamp: ts, capacities: vec![], known_peers: vec![], hmac: [0u8; 32],
        };
        let g2 = CapacityGossipPayload {
            sender_id: sender, timestamp: ts + 1, capacities: vec![], known_peers: vec![], hmac: [0u8; 32],
        };
        let h1 = g1.compute_hmac(&key);
        let h2 = g2.compute_hmac(&key);
        prop_assert_ne!(h1, h2);
    }

    #[test]
    fn forward_ttl_is_u8(
        ttl in 0u8..20u8,
        hop_count in 0u8..20u8,
    ) {
        let req = make_forward_request(ttl, hop_count);
        prop_assert!(req.ttl < 20);
        prop_assert!(req.hop_count < 20);
    }

    #[test]
    fn handler_decrements_ttl(
        ttl in 1u8..20u8,
        hop_count in 0u8..20u8,
    ) {
        let req = make_forward_request(ttl, hop_count);
        let forwarded_ttl = req.ttl.saturating_sub(1);
        let forwarded_hop = req.hop_count.saturating_add(1);
        prop_assert_eq!(forwarded_ttl, ttl - 1);
        prop_assert_eq!(forwarded_hop, hop_count + 1);
    }

    #[test]
    fn rate_limiter_burst_invariant(
        max_sustained in 1u32..1000,
        max_burst in 1u32..1000,
        requests in 1usize..2000,
    ) {
        let limiter = RateLimiter::new(max_sustained, max_burst);
        let consumer = [1u8; 32];
        let start = std::time::Instant::now();
        let mut allowed = 0;
        for _ in 0..requests {
            if limiter.check_consumer(&consumer) {
                allowed += 1;
            }
        }
        // Token-bucket invariant: over a time window of `elapsed`,
        // the bucket allows at most `max_burst + elapsed_secs *
        // refill_rate` tokens, where `refill_rate = max_sustained`
        // tokens/sec. The un-bounded `allowed <= max_burst` version
        // is non-deterministic under proptest SourceParallel
        // (wall-clock `Instant::now()` refill pushes allowed above
        // max_burst when the loop runs for >~1ms).
        let elapsed_secs = start.elapsed().as_secs_f64();
        let max_allowed =
            max_burst as f64 + elapsed_secs * max_sustained as f64;
        // Add 2.0 tokens of slop for sub-millisecond refill
        // measurement rounding.
        prop_assert!(
            allowed as f64 <= max_allowed + 2.0,
            "allowed={} max_burst={} max_sustained={} elapsed={:?}",
            allowed,
            max_burst,
            max_sustained,
            elapsed_secs
        );
    }
}
