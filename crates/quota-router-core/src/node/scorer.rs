use super::provider::{ProviderCapacity, ProviderHealth, RouterNodeId};
use super::request::{RequestContext, RoutingPolicy};

#[derive(Clone, Debug)]
pub enum Destination {
    Local {
        score: f64,
        provider: ProviderCapacity,
    },
    Remote {
        score: f64,
        peer_id: RouterNodeId,
        provider: ProviderCapacity,
    },
}

impl Destination {
    pub fn score(&self) -> f64 {
        match self {
            Destination::Local { score, .. } => *score,
            Destination::Remote { score, .. } => *score,
        }
    }
}

/// Outcome of the destination selection algorithm. Distinguishes
/// between "no candidates matched" and "all matching candidates had
/// zero capacity" so the handler can emit the correct
/// `ForwardRejectReason` and trigger pull-gossip when appropriate.
#[derive(Clone, Debug)]
pub enum SelectionState {
    /// At least one destination passed all hard filters.
    Matched(Vec<Destination>),
    /// All candidates were filtered out because no provider has
    /// remaining capacity (model matches but `requests_remaining == 0`
    /// for every matching provider, both local and remote).
    CapacityExhausted,
    /// All candidates were filtered out for other reasons (model
    /// mismatch, budget exceeded, health unavailable, etc.).
    NoMatch,
}

pub fn select_destinations(
    request: &RequestContext,
    local_providers: &[ProviderCapacity],
    peer_capabilities: &[(RouterNodeId, Vec<ProviderCapacity>)],
    policy: &RoutingPolicy,
) -> Vec<Destination> {
    let mut candidates: Vec<Destination> = Vec::new();

    for p in local_providers {
        if filter_model(p, &request.model)
            & filter_budget(p, request)
            & filter_health(p)
            & filter_capacity(p)
            & filter_provider_preference(p, request)
            & filter_context_window(p, request)
            & filter_tags(p, request)
        {
            candidates.push(Destination::Local {
                score: score_provider(p, policy, request),
                provider: p.clone(),
            });
        }
    }

    for (peer_id, peer_providers) in peer_capabilities {
        for p in peer_providers {
            if filter_model(p, &request.model)
                & filter_budget(p, request)
                & filter_health(p)
                & filter_capacity(p)
                & filter_provider_preference(p, request)
                & filter_context_window(p, request)
                & filter_tags(p, request)
            {
                candidates.push(Destination::Remote {
                    score: score_provider(p, policy, request),
                    peer_id: *peer_id,
                    provider: p.clone(),
                });
            }
        }
    }

    candidates.sort_by(|a, b| {
        b.score()
            .partial_cmp(&a.score())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

/// Selection variant that distinguishes "no match" from "capacity
/// exhausted". Used by the handler to emit the correct
/// `ForwardRejectReason` and trigger pull-gossip on capacity exhaustion.
pub fn select_destinations_with_state(
    request: &RequestContext,
    local_providers: &[ProviderCapacity],
    peer_capabilities: &[(RouterNodeId, Vec<ProviderCapacity>)],
    policy: &RoutingPolicy,
) -> SelectionState {
    let candidates = select_destinations(request, local_providers, peer_capabilities, policy);
    if !candidates.is_empty() {
        return SelectionState::Matched(candidates);
    }

    // Candidates were empty — determine why. Check if any provider
    // (local or remote) matches the model at all but has zero capacity.
    let has_matching_with_zero_capacity = local_providers
        .iter()
        .chain(peer_capabilities.iter().flat_map(|(_, caps)| caps.iter()))
        .any(|p| filter_model(p, &request.model) && p.requests_remaining == 0);

    if has_matching_with_zero_capacity {
        SelectionState::CapacityExhausted
    } else {
        SelectionState::NoMatch
    }
}

fn filter_model(provider: &ProviderCapacity, model: &str) -> bool {
    provider.models.iter().any(|m| m == model)
}

fn filter_budget(provider: &ProviderCapacity, ctx: &RequestContext) -> bool {
    match ctx.max_price_per_1k_tokens {
        Some(max) => provider
            .pricing
            .iter()
            .filter(|p| p.model == ctx.model)
            .any(|p| p.price_per_1k_tokens <= max),
        None => true,
    }
}

fn filter_health(provider: &ProviderCapacity) -> bool {
    provider.status != ProviderHealth::Unavailable
}

fn filter_capacity(provider: &ProviderCapacity) -> bool {
    provider.requests_remaining > 0
}

fn filter_provider_preference(provider: &ProviderCapacity, ctx: &RequestContext) -> bool {
    match &ctx.preferred_provider {
        Some(pref) => provider.provider_name == *pref,
        None => true,
    }
}

/// Context window filter — always passes at mesh level.
/// `ProviderCapacity` does not carry max_input_tokens/max_output_tokens.
/// Detailed context window checks happen at dispatch time (local layer)
/// using RFC-0936's `ContextWindowCheck`.
fn filter_context_window(_provider: &ProviderCapacity, _ctx: &RequestContext) -> bool {
    true
}

/// Tag filter — always passes at mesh level.
/// Tags are not gossiped (too dynamic). Detailed tag checking happens
/// at dispatch time (local layer) per RFC-0936's `TagFilterCheck`.
fn filter_tags(_provider: &ProviderCapacity, _ctx: &RequestContext) -> bool {
    true
}

fn score_provider(
    provider: &ProviderCapacity,
    policy: &RoutingPolicy,
    request: &RequestContext,
) -> f64 {
    let health_factor = match provider.status {
        ProviderHealth::Healthy => 1.0,
        ProviderHealth::Degraded => 0.5,
        ProviderHealth::Unknown => 0.3,
        ProviderHealth::Unavailable => 0.0,
    };

    let price_score = match provider.pricing.iter().find(|p| p.model == request.model) {
        Some(p) if p.price_per_1k_tokens == 0 => 1.0,
        Some(p) => 1.0 / (1.0 + p.price_per_1k_tokens as f64),
        None => 0.5,
    };

    let latency_score = if provider.latency_ms == 0 {
        0.5
    } else {
        1.0 / (1.0 + provider.latency_ms as f64 / 100.0)
    };

    let quality_score = provider.success_rate_bps as f64 / 10000.0;

    let capacity_score = (provider.requests_remaining as f64).min(1000.0) / 1000.0;

    let latency_penalty = match request.max_latency_ms {
        Some(max) if provider.latency_ms > max => 0.3,
        _ => 1.0,
    };

    let base_score = match policy {
        RoutingPolicy::Cheapest => price_score * 0.7 + capacity_score * 0.2 + quality_score * 0.1,
        RoutingPolicy::Fastest => latency_score * 0.7 + capacity_score * 0.2 + quality_score * 0.1,
        RoutingPolicy::Quality => quality_score * 0.7 + capacity_score * 0.2 + price_score * 0.1,
        RoutingPolicy::Balanced => (price_score + latency_score + quality_score) / 3.0,
        RoutingPolicy::LocalOnly => 0.0,
        RoutingPolicy::Custom(c) => {
            let model_pref = c.model_overrides.iter().find(|o| o.model == request.model);
            match model_pref {
                Some(ov) => {
                    let preferred = ov
                        .preferred_providers
                        .iter()
                        .any(|p| p == &provider.provider_name);
                    let under_price =
                        ov.max_price == 0 || price_score >= 1.0 / (1.0 + ov.max_price as f64);
                    if preferred && under_price {
                        1.0
                    } else {
                        (price_score + latency_score + quality_score) / 3.0 * 0.5
                    }
                }
                None => (price_score + latency_score + quality_score) / 3.0,
            }
        }
    };

    health_factor * base_score * latency_penalty
}

#[cfg(test)]
mod tests {
    use super::super::provider::{ModelPricing, ProviderHealth, ProviderId, RouterNodeId};
    use super::*;

    fn make_provider(
        name: &str,
        model: &str,
        price: u64,
        latency: u32,
        success_bps: u16,
        remaining: u64,
    ) -> ProviderCapacity {
        ProviderCapacity {
            provider_id: ProviderId([1u8; 32]),
            provider_name: name.to_string(),
            router_node_id: RouterNodeId([0u8; 32]),
            models: vec![model.to_string()],
            requests_remaining: remaining,
            pricing: vec![ModelPricing {
                model: model.to_string(),
                price_per_1k_tokens: price,
            }],
            status: ProviderHealth::Healthy,
            latency_ms: latency,
            success_rate_bps: success_bps,
            last_updated: 0,
        }
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

    #[test]
    fn model_filter_excludes_non_matching() {
        let local = vec![make_provider("a", "gpt-4o", 3, 200, 9500, 100)];
        let req = make_request("claude-3-opus");
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(dests.is_empty());
    }

    #[test]
    fn budget_filter_excludes_expensive() {
        let local = vec![make_provider("a", "gpt-4o", 15, 200, 9500, 100)];
        let mut req = make_request("gpt-4o");
        req.max_price_per_1k_tokens = Some(10);
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(dests.is_empty());
    }

    #[test]
    fn health_filter_excludes_unavailable() {
        let mut p = make_provider("a", "gpt-4o", 3, 200, 9500, 100);
        p.status = ProviderHealth::Unavailable;
        let local = vec![p];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(dests.is_empty());
    }

    #[test]
    fn capacity_filter_excludes_zero_remaining() {
        let local = vec![make_provider("a", "gpt-4o", 3, 200, 9500, 0)];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(dests.is_empty());
    }

    #[test]
    fn scoring_balanced_uses_price_latency_quality() {
        let local = vec![make_provider("a", "gpt-4o", 3, 200, 9500, 100)];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert_eq!(dests.len(), 1);
        assert!(dests[0].score() > 0.0);
    }

    #[test]
    fn scoring_cheapest_prefers_lower_price() {
        let cheap = make_provider("cheap", "gpt-4o", 1, 300, 9000, 100);
        let expensive = make_provider("expensive", "gpt-4o", 10, 100, 9900, 100);
        let local = vec![cheap, expensive];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Cheapest);
        assert_eq!(dests.len(), 2);
        match &dests[0] {
            Destination::Local { provider, .. } => assert_eq!(provider.provider_name, "cheap"),
            _ => panic!("expected local"),
        }
    }

    #[test]
    fn scoring_fastest_prefers_lower_latency() {
        let fast = make_provider("fast", "gpt-4o", 10, 50, 9900, 100);
        let slow = make_provider("slow", "gpt-4o", 1, 500, 9900, 100);
        let local = vec![fast, slow];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Fastest);
        assert_eq!(dests.len(), 2);
        match &dests[0] {
            Destination::Local { provider, .. } => assert_eq!(provider.provider_name, "fast"),
            _ => panic!("expected local"),
        }
    }

    #[test]
    fn preferred_provider_filter() {
        let a = make_provider("a", "gpt-4o", 3, 200, 9500, 100);
        let b = make_provider("b", "gpt-4o", 1, 100, 9900, 100);
        let local = vec![a, b];
        let mut req = make_request("gpt-4o");
        req.preferred_provider = Some("b".to_string());
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert_eq!(dests.len(), 1);
        match &dests[0] {
            Destination::Local { provider, .. } => assert_eq!(provider.provider_name, "b"),
            _ => panic!("expected local"),
        }
    }

    #[test]
    fn latency_penalty_applied() {
        let p = make_provider("a", "gpt-4o", 3, 500, 9500, 100);
        let local = vec![p];
        let mut req = make_request("gpt-4o");
        req.max_latency_ms = Some(100);
        let dests = select_destinations(&req, &local, &[], &RoutingPolicy::Balanced);
        assert_eq!(dests.len(), 1);
        assert!(dests[0].score() < 0.5);
    }

    #[test]
    fn remote_providers_scored() {
        let peer_id = RouterNodeId([2u8; 32]);
        let remote = vec![make_provider("remote", "gpt-4o", 2, 100, 9900, 50)];
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &[], &[(peer_id, remote)], &RoutingPolicy::Balanced);
        assert_eq!(dests.len(), 1);
        match &dests[0] {
            Destination::Remote {
                peer_id: id,
                provider,
                ..
            } => {
                assert_eq!(*id, peer_id);
                assert_eq!(provider.provider_name, "remote");
            }
            _ => panic!("expected remote"),
        }
    }

    #[test]
    fn quality_policy_prefers_higher_success_rate() {
        let high = make_provider("high", "gpt-4o", 5, 200, 9900, 100);
        let low = make_provider("low", "gpt-4o", 5, 200, 5000, 100);
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &[high, low], &[], &RoutingPolicy::Quality);
        assert_eq!(dests.len(), 2);
        match &dests[0] {
            Destination::Local { provider, .. } => assert_eq!(provider.provider_name, "high"),
            _ => panic!("expected high"),
        }
    }

    #[test]
    fn custom_policy_with_model_override() {
        let mut high = make_provider("preferred", "gpt-4o", 5, 200, 9500, 100);
        high.provider_name = "preferred".into();
        let low = make_provider("other", "gpt-4o", 1, 200, 9500, 100);
        let policy = RoutingPolicy::Custom(super::super::request::CustomPolicy {
            model_overrides: vec![super::super::request::ModelOverride {
                model: "gpt-4o".into(),
                preferred_providers: vec!["preferred".into()],
                max_price: 10,
            }],
            blacklist: vec![],
            max_price_per_1k_tokens: 0,
        });
        let req = make_request("gpt-4o");
        let dests = select_destinations(&req, &[high, low], &[], &policy);
        assert_eq!(dests.len(), 2);
        match &dests[0] {
            Destination::Local { score, .. } => assert!(*score >= 0.9),
            _ => panic!("expected high score"),
        }
    }

    #[test]
    fn preferred_provider_with_remote() {
        let peer_id = RouterNodeId([2u8; 32]);
        let remote = vec![make_provider("b", "gpt-4o", 1, 100, 9900, 50)];
        let mut req = make_request("gpt-4o");
        req.preferred_provider = Some("b".to_string());
        let dests = select_destinations(&req, &[], &[(peer_id, remote)], &RoutingPolicy::Balanced);
        assert_eq!(dests.len(), 1);
        match &dests[0] {
            Destination::Remote { provider, .. } => assert_eq!(provider.provider_name, "b"),
            _ => panic!("expected remote b"),
        }
    }

    #[test]
    fn selection_state_matched() {
        let local = vec![make_provider("a", "gpt-4o", 3, 200, 9500, 100)];
        let req = make_request("gpt-4o");
        let state = select_destinations_with_state(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(matches!(state, SelectionState::Matched(_)));
    }

    #[test]
    fn selection_state_no_match_model_mismatch() {
        let local = vec![make_provider("a", "gpt-4o", 3, 200, 9500, 100)];
        let req = make_request("claude-3-opus");
        let state = select_destinations_with_state(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(matches!(state, SelectionState::NoMatch));
    }

    #[test]
    fn selection_state_no_match_budget_exceeded() {
        let local = vec![make_provider("a", "gpt-4o", 15, 200, 9500, 100)];
        let mut req = make_request("gpt-4o");
        req.max_price_per_1k_tokens = Some(10);
        let state = select_destinations_with_state(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(matches!(state, SelectionState::NoMatch));
    }

    #[test]
    fn selection_state_capacity_exhausted() {
        let local = vec![make_provider("a", "gpt-4o", 3, 200, 9500, 0)];
        let req = make_request("gpt-4o");
        let state = select_destinations_with_state(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(matches!(state, SelectionState::CapacityExhausted));
    }

    #[test]
    fn selection_state_capacity_exhausted_remote_only() {
        let peer_id = RouterNodeId([2u8; 32]);
        let remote = vec![make_provider("remote", "gpt-4o", 2, 100, 9900, 0)];
        let req = make_request("gpt-4o");
        let state = select_destinations_with_state(
            &req,
            &[],
            &[(peer_id, remote)],
            &RoutingPolicy::Balanced,
        );
        assert!(matches!(state, SelectionState::CapacityExhausted));
    }

    #[test]
    fn selection_state_no_match_health_unavailable() {
        let mut p = make_provider("a", "gpt-4o", 3, 200, 9500, 100);
        p.status = ProviderHealth::Unavailable;
        let local = vec![p];
        let req = make_request("gpt-4o");
        let state = select_destinations_with_state(&req, &local, &[], &RoutingPolicy::Balanced);
        assert!(matches!(state, SelectionState::NoMatch));
    }
}
