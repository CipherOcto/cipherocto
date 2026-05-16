# Mission: 0902-a — Fallback Chain Wiring

## Status

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing & Load Balancing

## Dependencies

- None (standalone)

## Context

The existing `fallback.rs` implements `FallbackConfig`, `FallbackExecutor`, and `RouterError` with support for general, context window, and content policy fallbacks. But the proxy doesn't use it — provider failures don't trigger fallbacks.

## Acceptance Criteria

### Core Wiring

- [ ] Wire `FallbackExecutor` into proxy request path
- [ ] On provider failure, check `FallbackConfig::get_fallback_models()`
- [ ] Retry with fallback model on `RouterError::ProviderUnavailable`
- [ ] Retry with context_window_fallback on `RouterError::ContextWindowExceeded`
- [ ] Retry with content_policy_fallback on `RouterError::ContentPolicyViolation`
- [ ] Respect `max_retries` (default 3)
- [ ] Apply `retry_delay_ms` with `backoff_multiplier`

### Configuration

- [ ] Wire existing `FallbackConfig` fields into proxy (already exist in fallback.rs)
- [ ] Add `FallbackConfig` to `GatewayConfig` or `RouterSettings` (config.rs)
- [ ] Map `RouterSettings.fallbacks` (HashMap format) to `FallbackConfig.fallbacks` (Vec<FallbackEntry> format)

### Tests

- [ ] Primary provider failure triggers fallback
- [ ] Context window exceeded triggers context_window_fallback
- [ ] Content policy violation triggers content_policy_fallback
- [ ] Max retries limit respected
- [ ] Backoff delay applied correctly

## Key Files

- `crates/quota-router-core/src/fallback.rs` — FallbackConfig, FallbackExecutor, RouterError
- `crates/quota-router-core/src/proxy.rs` — main request handler
- `crates/quota-router-core/src/config.rs` — RouterConfig (add fallback fields)

## Notes

The existing `FallbackExecutor` wraps `FallbackConfig` and provides `has_fallback()`, `max_retries()`, `retry_delay()`. The `get_fallback_models()` method returns fallback models based on error type. This mission is about wiring it into the proxy.
