# Mission: 0902-a — Fallback Chain Wiring

## Status

Complete

Open

## RFC

RFC-0902 (Economics): Multi-Provider Routing & Load Balancing

## Dependencies

- None (standalone)

## Context

The existing `fallback.rs` implements `FallbackConfig`, `FallbackExecutor`, and `RouterError` with support for general, context window, and content policy fallbacks. But the proxy doesn't use it — provider failures don't trigger fallbacks.

## Acceptance Criteria

### Core Wiring

- [x] Wire `FallbackExecutor` into proxy request path
- [x] On provider failure, check `FallbackConfig::get_fallback_models()`
- [x] Retry with fallback model on `RouterError::ProviderUnavailable`
- [x] Retry with context_window_fallback on `RouterError::ContextWindowExceeded`
- [x] Retry with content_policy_fallback on `RouterError::ContentPolicyViolation`
- [x] Respect `max_retries` (default 3)
- [x] Apply `retry_delay_ms` with `backoff_multiplier`

### Configuration

- [x] Wire existing `FallbackConfig` fields into proxy (already exist in fallback.rs)
- [x] Add `FallbackConfig` to `GatewayConfig` or `RouterSettings` (config.rs)
- [x] Map `RouterSettings.fallbacks` (HashMap format) to `FallbackConfig.fallbacks` (Vec<FallbackEntry> format)

### Tests

- [x] Primary provider failure triggers fallback
- [x] Context window exceeded triggers context_window_fallback
- [x] Content policy violation triggers content_policy_fallback
- [x] Max retries limit respected
- [x] Backoff delay applied correctly

## Key Files

- `crates/quota-router-core/src/fallback.rs` — FallbackConfig, FallbackExecutor, RouterError
- `crates/quota-router-core/src/proxy.rs` — main request handler
- `crates/quota-router-core/src/config.rs` — RouterConfig (add fallback fields)

## Notes

The existing `FallbackExecutor` wraps `FallbackConfig` and provides `has_fallback()`, `max_retries()`, `retry_delay()`. The `get_fallback_models()` method returns fallback models based on error type. This mission is about wiring it into the proxy.

### H1: FallbackConfig Location

FallbackConfig should be added to GatewayConfig (top-level), not RouterSettings. Rationale: Fallback chains are cross-cutting (may span multiple router groups). Add field: fallbacks: Option<FallbackConfig>.

### H2: Config Mapping

Config mapping: GatewayConfig.fallbacks (Vec<FallbackEntry>) maps directly to FallbackConfig.fallbacks. No HashMap conversion needed — the YAML structure matches the Rust struct.

### H3: DispatchInfo Integration

When fallback retry triggers, the next model's DispatchInfo is looked up from the dispatch map. If the fallback model is not in the dispatch map, use the original DispatchInfo with only the model name changed.
