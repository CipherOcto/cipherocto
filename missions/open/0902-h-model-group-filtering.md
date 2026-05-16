# Mission: 0902-h — model_group Filtering at Request Time

## Status

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing and Load Balancing

## Dependencies

- Mission-0929-d: Wire DispatchInfo to Proxy Dispatch Path (blocks — DispatchInfo.model_group must flow through)

## Context

`DispatchInfo.model_group` is populated by `to_provider_map()` from config (model_info.group or litellm_params.model_group_alias). However, the router does not filter deployments by model_group at request time — it selects from all deployments matching the model name.

This mission enables transparent multi-provider routing: a request for model_group "gpt-4" can route across OpenAI, Azure, and any other deployment tagged with that group.

## Acceptance Criteria

- [ ] `Router::get_provider()` accepts optional `model_group` parameter
- [ ] When model_group is provided, only deployments with matching model_group are candidates
- [ ] When model_group is None, all deployments for the model name are candidates (backward compatible)
- [ ] model_group matching is case-insensitive (per existing config.rs behavior)
- [ ] Test: two deployments same model, different groups — verify correct group selected
- [ ] Test: model_group=None falls back to all deployments
- [ ] Clippy passes with zero warnings
- [ ] Existing tests pass

## Files to Modify

- `crates/quota-router-core/src/router.rs` — add model_group filtering to get_provider()
- `crates/quota-router-core/src/proxy.rs` — pass model_group from DispatchInfo to router

## Notes

This is the final piece that makes multi-provider routing work transparently — the caller requests a model_group and the router handles provider diversity behind the scenes.

### H1: get_provider Signature

Note: Router::get_provider() takes (&mut self, model_group, index). Model group filtering must happen BEFORE calling get_provider() — filter the model_group string first, then look up the provider.
