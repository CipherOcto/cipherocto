# Mission: 0938-a — YAML Interpolation & Universal Key

## Status

Open

## RFC

RFC-0938 (Economics): YAML Interpolation & Universal Key

## Dependencies

- Mission-0931-a: Env Var Syntax Implementation (complementary)

## Context

RFC-0938 specifies `${VAR}` YAML interpolation and `ANY_LLM_KEY` universal key support for any-llm compatibility. This mission implements both features.

## Acceptance Criteria

### YAML Interpolation

- [ ] `interpolate_yaml()` function parses `${VAR}` syntax
- [ ] `${VAR:-default}` syntax for default values
- [ ] `$$` escape produces literal `$` character
- [ ] Undefined variables resolve to empty string
- [ ] No recursive interpolation
- [ ] Interpolation happens BEFORE YAML parse

### Universal Key

- [ ] `ANY_LLM_KEY` env var as fallback for all providers
- [ ] Full precedence: config_key > os.environ["KEY"] (Mission-0931-a) > ANY_LLM_KEY > {PROVIDER}_API_KEY
- [ ] Log warning when using universal key

### Config Loading

- [ ] Update `parse_config()` to call `interpolate_yaml()` before YAML parse
- [ ] Handle YAML special characters in interpolated values

### Tests

- [ ] `${VAR}` interpolation works
- [ ] `${VAR:-default}` works (default used when var undefined)
- [ ] `$$` escape produces literal `${`
- [ ] Undefined variable without default resolves to empty string (NOT parse error)
- [ ] ANY_LLM_KEY works as fallback
- [ ] Config key > os.environ["KEY"] > ANY_LLM_KEY > provider-specific env var (4-tier precedence)

## Key Files

- `crates/quota-router-core/src/config.rs` — interpolate_yaml(), parse_config()
- `crates/quota-router-core/src/proxy.rs` — resolve_api_key() (add ANY_LLM_KEY)

## Notes

The `interpolate_yaml()` function should be a simple character-by-character parser. The `parse_config()` function should call it before `serde_yaml::from_str()`.

**SecretReader integration:** After Mission-0935 completes, `resolve_api_key()` should integrate `SecretReader::get_secret()` as an additional fallback tier (tier 4) per RFC-0935 Section 5.
