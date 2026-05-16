# Mission: 0930-a — Provider Registry Expansion

## Status

Open

## RFC

RFC-0930 (Economics): Provider Inference from Model String

## Dependencies

- None (standalone)

## Context

RFC-0930 specifies `get_provider_default_api_base()` with a registry of known providers and their default api_bases. The function does NOT exist yet. The py_bridge factory has 41 provider match arms (verified by counting `py_bridge/factory.rs`).

## Acceptance Criteria

### Registry Expansion

- [ ] Add all 41 py_bridge providers to `get_provider_default_api_base()` (verified count from factory.rs match arms)
- [ ] Each provider has correct default api_base
- [ ] Azure returns `None` (requires explicit api_base)

### ConfigError::MissingProvider

- [ ] Add `MissingProvider` variant to `ConfigError` enum
- [ ] Return `MissingProvider` from `to_provider_map()` when provider is empty and cannot be inferred from model_name
- [ ] Error message includes provider name

### Tests

- [ ] All 41 providers have correct default api_base (7 providers with specified defaults per RFC-0930 Section 3.1 table + 34 returning None)
- [ ] Azure returns None
- [ ] Unknown provider returns `None` from `get_provider_default_api_base()` (MissingProvider error is from `to_provider_map()`, not this function)
- [ ] Provider inference works for all providers

## Key Files

- `crates/quota-router-core/src/config.rs` — get_provider_default_api_base(), ConfigError enum
- `crates/quota-router-core/src/py_bridge/factory.rs` — provider list

## Notes

The `get_provider_default_api_base()` function does NOT exist yet — it must be CREATED as part of this mission. Per RFC-0930 "Files to Modify" section, both the function AND the registry go to `config.rs`. The registry should be a `HashMap<&str, &str>` or match statement with all 42 providers.

**Provider count:** factory.rs has 41 match arms. All 41 must be in the registry. (Count verified 2026-05-16 by counting actual match arms in `py_bridge/factory.rs`.)

**API base values:** RFC-0930 Section 3.1 specifies api_bases for 7 providers (openai, anthropic, mistral, gemini, cohere, voyage, azure). For the remaining 35 providers, return `None` (like Azure) unless the provider has a well-known default api_base. The implementer should research each provider's default api_base or return `None` if unknown.
