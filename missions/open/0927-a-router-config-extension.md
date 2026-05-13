# Mission: RFC-0927 — RouterConfig Extension Implementation

## Status

Open

## RFC

RFC-0927 (Economics): RouterConfig Extension for LiteLLM Compatibility

## Dependencies

- RFC-0917: Dual-Mode Query Router (base RouterConfig)

## Acceptance Criteria

- [ ] RoutingStrategy enum with 8 strategies (SimpleShuffle, RoundRobin, LeastBusy, LatencyBased, CostBased, UsageBased, UsageBasedV2, Weighted) — serde rename_all = "kebab-case"
- [ ] RoutingStrategyArgs struct with latency_threshold_ms, allowed_fails, cooldown_time_secs, tpm_weight, rpm_weight — with impl Default — NOTE: no serde rename_all attribute
- [ ] LatencyRoutingSettings struct with impl Default
- [ ] LiteLLMParams struct with api_base/base_url aliasing, resolve_api_base() method
- [ ] LiteLLMProviderConfig, Credentials, ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier, RateLimitConfig structs
- [ ] RouterConfigExt struct with impl Default
- [ ] ConfigError enum with NotYetSpecified variant ADDED (do not create new type — ConfigError already exists in config.rs)
- [ ] All types derive(Debug, Clone, Serialize, Deserialize)
- [ ] cargo clippy -D warnings passes
- [ ] cargo test --lib passes

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | Add RFC-0927 types (RoutingStrategy, RoutingStrategyArgs, LatencyRoutingSettings, LiteLLMParams, LiteLLMProviderConfig, Credentials, ProviderType, HttpProviderType, SdkProviderType, ModelIdentifier, RateLimitConfig, RouterConfigExt); Add NotYetSpecified variant to existing ConfigError |

## Notes

This mission implements the types defined in RFC-0927. The actual mapping to RFC-0917 RouterConfig is handled by the implementation layer per RFC-0928.

**Important:** RoutingStrategyArgs has NO serde rename_all attribute — only RoutingStrategy enum has rename_all = "kebab-case".