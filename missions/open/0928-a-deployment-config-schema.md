# Mission: RFC-0928 — Deployment Configuration Schema Implementation

## Status

Open

## RFC

RFC-0928 (Economics): Deployment Configuration Schema

## Dependencies

- Mission-0927-a: RFC-0927 RouterConfig Extension Implementation
- RFC-0917: Dual-Mode Query Router (base provider model)
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility

## Acceptance Criteria

- [ ] DeploymentConfig with serde aliases (id, requests_per_minute, tokens_per_minute)
- [ ] RouterSettings with redis_host, redis_port, redis_password, stream_timeout_secs — with impl Default
- [ ] LiteLLMSettings with set_google_vertex_ai — with impl Default
- [ ] ModelInfo with model_group (alias: "group"), supports_embeddings (alias: "embeddings")
- [ ] PricingConfig with optional per-million and per-second pricing fields
- [ ] GatewayConfig with Optional<pricing>, providers/AnyLlmProviderConfig
- [ ] GatewayConfig::get_deployments() method
- [ ] parse_config() and load_config() functions
- [ ] to_provider_map() function (returns NotYetSpecified until RFC-0917 mapping complete)
- [ ] ConfigError enum with NotYetSpecified variant ADDED (do not create new type — ConfigError already exists in config.rs)
- [ ] All types derive(Debug, Clone, Serialize, Deserialize)
- [ ] YAML parsing tests for both LiteLLM and any-llm formats
- [ ] cargo clippy -D warnings passes
- [ ] cargo test --lib passes

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | Add RFC-0928 types (DeploymentConfig, RouterSettings, LiteLLMSettings, ModelInfo, PricingConfig, GatewayConfig, AnyLlmProviderConfig) and functions (parse_config, load_config, to_provider_map); ConfigError already exists — add NotYetSpecified variant only |

## Notes

This mission implements the deployment configuration schema. The to_provider_map function returns NotYetSpecified until the actual mapping to RFC-0917's providers HashMap is implemented.

**Important:** RoutingStrategyArgs has NO serde rename_all attribute — only RoutingStrategy enum has rename_all = "kebab-case". This is imported from RFC-0927 via Mission-0927-a.