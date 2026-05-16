# Mission: 0930-a — Provider Registry Expansion

## Status

Open

## RFC

RFC-0930 (Economics): Provider Inference from Model String

## Dependencies

- None (standalone)

## Context

RFC-0930 specifies `get_provider_default_api_base()` with a registry of known providers and their default api_bases. The function does NOT exist yet. The py_bridge factory supports 42 providers.

## Acceptance Criteria

### Registry Expansion

- [ ] Add all 42 py_bridge providers to `get_provider_default_api_base()` (including workersai)
- [ ] Each provider has correct default api_base
- [ ] Azure returns `None` (requires explicit api_base)

### ConfigError::MissingProvider

- [ ] Add `MissingProvider` variant to `ConfigError` enum
- [ ] Return `MissingProvider` when provider not in registry
- [ ] Error message includes provider name

### Tests

- [ ] All 42 providers have correct default api_base
- [ ] Azure returns None
- [ ] Unknown provider returns MissingProvider error
- [ ] Provider inference works for all providers

## Key Files

- `crates/quota-router-core/src/config.rs` — get_provider_default_api_base(), ConfigError enum
- `crates/quota-router-core/src/py_bridge/factory.rs` — provider list

## Notes

The `get_provider_default_api_base()` function does NOT exist yet — it must be CREATED as part of this mission. The function should be added to `config.rs` per RFC-0930 file mapping. The registry should be a `HashMap<&str, &str>` or match statement with all 42 providers.

**Provider count:** factory.rs has 42 match arms (including `workersai`). All 42 must be in the registry.

**API base values:** RFC-0930 Section 3.1 specifies api_bases for 7 providers (openai, anthropic, mistral, gemini, cohere, voyage, azure). For the remaining 35 providers, return `None` (like Azure) unless the provider has a well-known default api_base. The implementer should research each provider's default api_base or return `None` if unknown.
