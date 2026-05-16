# Mission: 0931-a — Env Var Syntax Implementation

## Status

Open

## RFC

RFC-0931 (Economics): any-llm-mode Environment Variable Parity

## Dependencies

- Mission-0930-a: Provider Registry Expansion (provides get_provider_default_api_base() for Tier 4)

## Context

RFC-0931 specifies `os.environ["KEY"]` syntax for env var resolution in config. This is not yet implemented. The `resolve_api_key()` and `resolve_api_base()` methods on `LiteLLMParams` need to support this syntax.

## Acceptance Criteria

### os.environ["KEY"] Syntax

- [ ] Parse `os.environ["KEY"]` and `os.environ['KEY']` syntax
- [ ] Extract env var name from brackets
- [ ] Resolve from `std::env::var()`
- [ ] Return `None` if env var not set

### resolve_api_key()

- [ ] Tier 1: Explicit non-empty non-os.environ value
- [ ] Tier 2: `os.environ["KEY"]` syntax
- [ ] Tier 3: `{PROVIDER}_API_KEY` env var
- [ ] Return `None` if all tiers fail

### resolve_api_base()

- [ ] Tier 1: Explicit non-empty value (existing: api_base or base_url alias)
- [ ] Tier 2: `os.environ["KEY"]` syntax
- [ ] Tier 3: `{PROVIDER}_API_BASE` env var
- [ ] Tier 4: Provider-specific default from RFC-0930 registry (requires Mission-0930-a)
- [ ] Return `None` if all tiers fail

### Tests

- [ ] `os.environ["MY_KEY"]` resolves correctly
- [ ] `os.environ['MY_KEY']` resolves correctly
- [ ] Missing env var returns None
- [ ] Empty `os.environ[""]` returns None
- [ ] Provider env var fallback works

## Key Files

- `crates/quota-router-core/src/config.rs` — LiteLLMParams, resolve_api_key(), resolve_api_base()

## Notes

The `extract_os_environ_key()` helper function needs to be implemented to parse the bracket syntax.

**resolve_api_key():** Does NOT exist on `LiteLLMParams`. There is a standalone `resolve_api_key()` in `proxy.rs` that takes `(&Provider, Option<&str>)`. This mission should add `resolve_api_key()` as a method on `LiteLLMParams` with the 3-tier resolution.

**resolve_api_base():** Already exists on `LiteLLMParams` in `config.rs` but only checks `api_base` then `base_url` (returns `Option<&str>`). This mission must extend it to 4 tiers. **Breaking change:** The 4-tier implementation returns `Option<String>` (owned) because env var lookup and provider registry lookup produce owned Strings. All callers of `resolve_api_base()` will need updating.
