# Mission: RFC-0927 — RouterConfig Extension Implementation

## Status

Archived — types implemented as part of Mission-0928-a (not as separate implementation)

## RFC

RFC-0927 (Economics): RouterConfig Extension for LiteLLM Compatibility

## Dependencies

- RFC-0917: Dual-Mode Query Router (base RouterConfig)
- **Note:** RoutingStrategy enum is defined in RFC-0917's `router.rs` — do NOT redefine here. This mission implements the supporting types only.

## Acceptance Criteria

- [x] RoutingStrategyArgs struct with latency_threshold_ms, allowed_fails, cooldown_time_secs, tpm_weight, rpm_weight — with impl Default — NOTE: no serde rename_all attribute
- [x] LatencyRoutingSettings struct with impl Default
- [x] LiteLLMParams struct with api_base/base_url aliasing, resolve_api_base() method
- [x] LiteLLMProviderConfig, Credentials, ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier, RateLimitConfig structs
- [x] RouterConfigExt struct with impl Default
- [x] ConfigError enum with NotYetSpecified variant ADDED (do not create new type — ConfigError already exists in config.rs)
- [x] RateLimitMode enum (Soft default, Hard) — added for RFC-0929 support
- [x] All types derive(Debug, Clone, Serialize, Deserialize)
- [x] cargo clippy -D warnings passes
- [x] cargo test --lib passes

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | Add RFC-0927 types (RoutingStrategyArgs, LatencyRoutingSettings, LiteLLMParams, LiteLLMProviderConfig, Credentials, ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier, RateLimitConfig, RouterConfigExt, RateLimitMode); Add NotYetSpecified variant to existing ConfigError |

## Notes

**Important:** RoutingStrategy enum is imported from `router.rs` (RFC-0917), not defined in this mission. Do NOT redefine it.

**Implementation:** These types were implemented as part of Mission-0928-a's config.rs rewrite, not as a separate 0927-a implementation. The RoutingStrategy import is:
```rust
pub use crate::router::RoutingStrategy;
```

**RateLimitMode** was added to satisfy RFC-0929's §API Change requirement.

**Note:** Several RFC-0927 types (LiteLLMProviderConfig, Credentials, ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier, RateLimitConfig, RouterConfigExt) were NOT implemented. Mission-0928-a claims to cover them but also did not implement all. Verify type existence before depending on them.

**Important:** RoutingStrategyArgs has NO serde rename_all attribute — only RoutingStrategy enum has rename_all = "kebab-case".