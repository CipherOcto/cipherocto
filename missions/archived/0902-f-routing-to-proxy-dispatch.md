# Mission: 0902-f — Connect Routing Strategies to Proxy Dispatch

## Status

Complete

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- Mission-0929-d: Wire DispatchInfo to Proxy Dispatch Path (blocks — proxy must consume DispatchInfo first)

## Context

RFC-0902 defines routing strategies (SimpleShuffle, LeastBusy, LatencyBased, etc.) and `Router::get_provider()` exists in `router.rs`. However, `proxy.rs` does not call the router for provider selection — it uses a direct provider lookup. This mission connects the two.

## Acceptance Criteria

- [ ] `proxy.rs` calls `Router::get_provider(model, model_group)` for provider selection
- [ ] Routing strategy from `router_settings.routing_strategy` is used at dispatch time
- [ ] Fallback chain executes on provider failure (retry with next provider)
- [ ] Provider state (active_requests, latency) updated after each request
- [ ] Works for both litellm-mode and any-llm-mode dispatch paths
- [ ] Clippy passes with zero warnings
- [ ] Existing tests pass

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — integrate Router into dispatch
- `crates/quota-router-core/src/router.rs` — ensure get_provider accepts model_group filter

## Notes

Depends on 0929-d because the proxy must resolve DispatchInfo before it can route through Router.

### H1: Router Signature

Router::get_provider(&mut self, model_group: &str, index: usize) -> Option<&mut ProviderWithState>. Note: takes &mut self (mutable) because it updates round_robin_index.

### H2: Provider State

Provider state update: After successful request, update ProviderWithState.last_used, latency, and health status. This happens in the proxy response handler, not in the router.
