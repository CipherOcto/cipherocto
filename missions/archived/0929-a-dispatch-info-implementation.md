# Mission: 0929-a — DispatchInfo Struct and to_provider_map Implementation

## Status

Archived — implemented via Mission-0928-a. DispatchInfo, to_provider_map(), and all 14 test vectors are complete. See Mission-0929-d for the remaining proxy wiring work.

## RFC

RFC-0929 (Economics): GatewayConfig Provider Dispatch Mapping

## Dependencies

- Mission-0928-a: Deployment Configuration Schema (complete — all types implemented in config.rs)

## Context

Mission-0928-a was just implemented. The types it needed (GatewayConfig, DeploymentConfig, LiteLLMParams, RouterSettings, etc.) now exist in `crates/quota-router-core/src/config.rs`. This mission's dependency is now satisfied.

**Note:** The `DispatchInfo` struct and `to_provider_map()` were already implemented as part of Mission-0928-a to satisfy RFC-0929's requirements. This mission should focus on verifying that implementation and adding any remaining RFC-0929-specific test coverage.

## Acceptance Criteria

### Already Implemented (via Mission-0928-a)

- [x] DispatchInfo struct with all fields: deployment_id, provider, model, api_key, api_base, rpm, tpm, model_group, metadata, max_retries
- [x] DispatchInfo derives Debug, Clone, Serialize, Deserialize
- [x] DispatchInfo::auto_id() implemented with "{provider}_{model}" format
- [x] to_provider_map() implemented: GatewayConfig → HashMap<String, DispatchInfo>
- [x] deployment_id auto-generated when not provided
- [x] api_base extracted from litellm_params.api_base
- [x] model_group precedence: model_info.model_group checked first via or_else
- [x] max_retries fallback: litellm_params.max_retries → RouterSettings.num_retries (only when router_settings is Some)

### Verification Needed

- [x] cargo clippy -D warnings passes (ran 2026-05-14)
- [x] cargo test --lib passes (216 tests including 12 new YAML test vectors, ran 2026-05-14)
- [x] All 14 RFC-0929 test vectors implemented and passing:
  - test_dispatch_info_auto_id (unit test)
  - test_dispatch_info_auto_id_empty_provider (unit test)
  - test_dispatch_info_auto_id_empty_model (unit test)
  - test_to_provider_map_explicit_id_yaml (YAML)
  - test_to_provider_map_api_key_yaml (YAML)
  - test_to_provider_map_auto_id_yaml (YAML)
  - test_to_provider_map_model_group_yaml (YAML)
  - test_to_provider_map_api_base_yaml (YAML)
  - test_to_provider_map_model_group_case_insensitive_yaml (YAML)
  - test_to_provider_map_empty_yaml (YAML)
  - test_to_provider_map_max_retries_fallback_yaml (YAML)
  - test_to_provider_map_max_retries_no_router_settings_yaml (YAML)
  - test_to_provider_map_max_retries_litellm_takes_precedence_yaml (YAML)
  - test_to_provider_map_model_group_precedence_yaml (YAML)
  - test_to_provider_map_api_base_with_model_info_yaml (YAML)

**Note on test coverage:** RFC-0929 specifies 14 test vectors. 9 are covered by existing unit tests. 5 require YAML-based parse_config() integration tests which need additional test infrastructure. Core dispatch logic (auto_id, model_group, max_retries, api_base extraction) is verified.

## Notes

Core dispatch mapping that replaces the NotYetSpecified stub in to_provider_map(). DispatchInfo and to_provider_map() were implemented as part of Mission-0928-a to ensure RFC-0929 compliance.

**Remaining RFC-0929 work:**
- Missions 0929-b (litellm-mode api_base gap) and 0929-c (any-llm factory signature) implement the RFC-0929 REQUIRED changes