# Mission: 0906-a — Response Caching

## Status

Open

## RFC

RFC-0906 (Draft): Response Caching

## Context

LiteLLM supports response caching to avoid redundant API calls. This mission adds basic semantic caching using stoolap.

## Acceptance Criteria

- [ ] Add response cache using InMemoryCache (from Mission 0914-a)
- [ ] Cache key: hash of (model, messages, temperature, max_tokens)
- [ ] TTL-based expiry
- [ ] Cache bypass header: X-Cache-Control: no-cache
- [ ] Cache hit returns cached response without calling provider

## Files to Modify

- `crates/quota-router-core/src/proxy.rs` — add caching logic
- `crates/quota-router-core/src/cache.rs` — add ResponseCache
