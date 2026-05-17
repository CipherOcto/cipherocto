# Mission: 0902-b — Content Policy Fallback

## Status

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing & Load Balancing

## Context

LiteLLM has content_policy_fallbacks in router settings for fallback on content policy violations.

## Acceptance Criteria

- [ ] Add content_policy_fallbacks to RouterSettings config
- [ ] Wire content policy fallback in proxy error handling
- [ ] Try fallback models when content policy violation occurs

## Files to Modify

- `crates/quota-router-core/src/config.rs` — add content_policy_fallbacks
- `crates/quota-router-core/src/proxy.rs` — wire content policy fallback
