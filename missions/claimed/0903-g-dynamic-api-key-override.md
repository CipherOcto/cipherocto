# Mission: 0903-g — Per-Request Dynamic API Key Override

## Status

Open

## RFC

RFC-0903 (Economics): Virtual API Key System

## Dependencies

- Mission-0929-d: Wire DispatchInfo to Proxy Dispatch Path (blocks — proxy must consume DispatchInfo first)

## Context

RFC-0903 defines virtual API keys for the gateway. Currently, `resolve_api_key()` in `proxy.rs` uses a 2-tier chain: config key → env var. This mission extends the chain to 5 tiers by adding per-request `dynamic_api_key` override from the request body, enabling callers to supply their own provider API keys at request time.

**Priority chain (5-tier, matches RFC-0929 Section 5 + this mission):**
1. Per-request `X-API-Key` header (this mission — highest priority)
2. `key_storage` for `deployment_id` (RFC-0903 — deployment-scoped lookup)
3. Embedded `api_key` from `DispatchInfo` (RFC-0929 — resolved at config load time)
4. `provider_key_storage` for `provider_name` (RFC-0903 — provider-scoped fallback)
5. Environment variable `{PROVIDER}_API_KEY` (lowest priority)

> **Note:** RFC-0938's `resolve_api_key()` is a **config-time** function (resolves YAML `os.environ["KEY"]` syntax). This mission's chain is **runtime** (resolves actual API calls). They are complementary layers, not conflicting.

## Acceptance Criteria

- [ ] Request body supports optional `X-API-Key` header or `api_key` field for per-request override
- [ ] `resolve_api_key()` priority chain: per-request X-API-Key → key_storage → DispatchInfo.api_key → provider_key_storage → env var (5-tier, matches RFC-0929 Section 5 + this mission's per-request tier)
- [ ] Per-request key is NOT logged (security: same as api_base)
- [ ] Per-request key MUST be passed via `X-API-Key` header only (NOT in request body — body is a credential leak vector)
- [ ] Per-request key only applies to that single request (not persisted)
- [ ] Works for both litellm-mode and any-llm-mode paths
- [ ] Clippy passes with zero warnings
- [ ] Existing tests pass

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — extract per-request key, add to priority chain
- `crates/quota-router-core/src/native_http/mod.rs` — add api_key to HttpCompletionRequest if needed

## Notes

This enables multi-tenant scenarios where different callers use different provider keys through the same gateway.
