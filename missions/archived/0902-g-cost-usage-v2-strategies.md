# Mission: 0902-g — Implement CostBased and UsageBasedV2 Routing Strategies

## Status

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- RFC-0904: Real-Time Cost Tracking (Accepted — pricing data needed for CostBased)
- Mission-0902-f: Connect Routing Strategies to Proxy Dispatch (blocks — strategies must be wired first)

## Context

RFC-0902 defines CostBased and UsageBasedV2 as supported routing strategies. The `RoutingStrategy` enum includes both, and `Router::get_provider()` has match arms for them, but the actual implementations are stubs (fall back to SimpleShuffle).

## Acceptance Criteria

### CostBased

- [ ] CostBased selects deployment with lowest combined cost (input + output price per million tokens)
- [ ] Pricing data sourced from RFC-0904 pricing table or `ModelInfo` config
- [ ] Falls back to SimpleShuffle when pricing data unavailable for all candidates
- [ ] Test: two deployments with different prices, verify cheapest selected

### UsageBasedV2

- [ ] UsageBasedV2 uses exponential decay weighting — recent usage counts more
- [ ] Decay formula: `weight = e^(-lambda * age_seconds)` where lambda is configurable
- [ ] Uses `ProviderWithState` rolling window metrics (not raw RPM/TPM counters)
- [ ] Falls back to SimpleShuffle when no usage history exists
- [ ] Test: verify recent requests weighted more than older ones

### Both

- [ ] Clippy passes with zero warnings
- [ ] Existing tests pass

## Files to Modify

- `crates/quota-router-core/src/router.rs` — implement CostBased and UsageBasedV2 strategies
- `crates/quota-router-core/src/pricing.rs` — provide pricing lookup for CostBased (if not already available)

## Notes

CostBased requires RFC-0904 pricing data. UsageBasedV2 requires the exponential decay computation to be added to ProviderWithState.

### H1: Pricing Data

Pricing data source: If Mission-0904-a is not yet complete, use hardcoded pricing for known models (openai/gpt-4o, anthropic/claude-3, etc.). Return zero cost for unknown models and log a warning.

### H2: Lambda Parameter

Lambda parameter: The lambda value for UsageBasedV2 strategy comes from RouterConfig.lambda: Option<f64>. If not set, default to 0.5 (equal weight between cost and latency).
