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

- [x] `proxy.rs` resolves DispatchInfo from dispatch_map before calling provider
- [x] `api_key` from DispatchInfo flows through `resolve_api_key()` priority chain
- [x] `api_base` from DispatchInfo flows to provider call (litellm-mode via HttpCompletionRequest.api_base)
- [ ] `model_group` from DispatchInfo passed to Router for filtered deployment selection (deferred — Router not yet wired to ProxyServer)
- [ ] `max_retries` from DispatchInfo overrides router_settings.num_retries per-deployment (deferred — retry loop not yet in proxy)
- [x] Clippy passes with zero warnings
- [x] Existing tests pass (226 tests)

> **Note:** model_group and max_retries are partially deferred. The DispatchInfo lookup by model_group works (proxy finds DispatchInfo by model name or model_group), but Router integration and retry loop are separate missions. The dispatch_map is passed as empty HashMap from CLI (GatewayConfig integration is a later mission).

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — dispatch_map in ProxyServer, DispatchInfo lookup, api_key/api_base wiring
- `crates/quota-router-cli/src/commands.rs` — pass dispatch_map to ProxyServer::new()

## Notes

This is the critical wiring gap — without it, the DispatchInfo infrastructure from 0929-a/b/c is dead code in the proxy path.

### H1: resolve_api_key Bridge

Bridge mechanism: proxy.rs's resolve_api_key(provider, config_key) is called with DispatchInfo.api_key as config_key. This passes the DispatchInfo-resolved key to the existing proxy function. No new function needed.

### H2: model_group=None Behavior

When model_group is None, use the first model_group from the deployment's model_list. If model_list is empty, return 400 Bad Request.

### H3: max_retries Override

max_retries override: DispatchInfo.max_retries overrides the default max_retries in the retry loop. Apply at the top of the retry loop, before the first attempt.
