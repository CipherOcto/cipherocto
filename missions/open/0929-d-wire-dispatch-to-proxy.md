# Mission: 0929-d — Wire DispatchInfo to Proxy Dispatch Path

## Status

Open

## RFC

RFC-0929 (Economics): GatewayConfig Provider Dispatch Mapping

## Dependencies

- Mission-0929-a: DispatchInfo Struct and to_provider_map Implementation (complete)
- Mission-0929-b: litellm-mode api_base Gap Implementation (complete)
- Mission-0929-c: any-llm-mode factory api_base Implementation (complete)

**Note:** Missions 0929-b and 0929-c are in missions/claimed/ and missions/archived/ (not completed/). Verify their implementation is actually wired before depending on them.

## Context

Missions 0929-a/b/c implemented the data structures (DispatchInfo, to_provider_map) and the provider-level api_base forwarding. However, the proxy dispatch path in `proxy.rs` does not yet consume DispatchInfo — it still uses a flat provider lookup without routing strategy integration.

This mission completes the RFC-0929 integration chain: GatewayConfig → DispatchInfo → Router → Proxy → Provider.

## Acceptance Criteria

- [ ] `proxy.rs` resolves DispatchInfo from GatewayConfig before calling provider
- [ ] `api_key` from DispatchInfo flows through `resolve_api_key()` priority chain
- [ ] `api_base` from DispatchInfo flows to provider call (litellm-mode via HttpCompletionRequest, any-llm-mode via factory)
- [ ] `model_group` from DispatchInfo passed to Router for filtered deployment selection
- [ ] `max_retries` from DispatchInfo overrides router_settings.num_retries per-deployment
- [ ] Clippy passes with zero warnings
- [ ] Existing tests pass

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — consume DispatchInfo in dispatch path
- `crates/quota-router-core/src/config.rs` — ensure to_provider_map output is accessible to proxy

## Notes

This is the critical wiring gap — without it, the DispatchInfo infrastructure from 0929-a/b/c is dead code in the proxy path.

### H1: resolve_api_key Bridge

Bridge mechanism: proxy.rs's resolve_api_key(provider, config_key) is called with DispatchInfo.api_key as config_key. This passes the DispatchInfo-resolved key to the existing proxy function. No new function needed.

### H2: model_group=None Behavior

When model_group is None, use the first model_group from the deployment's model_list. If model_list is empty, return 400 Bad Request.

### H3: max_retries Override

max_retries override: DispatchInfo.max_retries overrides the default max_retries in the retry loop. Apply at the top of the retry loop, before the first attempt.
