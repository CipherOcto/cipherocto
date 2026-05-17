# Mission: 0938-a — YAML Interpolation & Universal Key

## Status

Open

## RFC

RFC-0938 (Economics): YAML Interpolation & Universal Key

## Dependencies

- Mission-0931-a: Env Var Syntax Implementation (complementary)

## Context

RFC-0938 specifies `${VAR}` YAML interpolation and `ANY_LLM_KEY` universal key support for any-llm compatibility. This mission implements both features.

**BREAKING CHANGE:** `resolve_api_key()` signature changes from `(provider: &Provider, config_key: Option<&str>) -> Option<String>` (sync) to `(config_key: Option<&str>, provider: &str) -> Result<Option<String>>` (sync, NOT async). Parameter order is reversed. Return type adds `Result` wrapper. This is a signature rewrite, not a backward-compatible extension.

**`resolve_api_key()` is synchronous (not async).** All operations are CPU-bound string manipulation and env var lookup. No async operations needed.

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
- [ ] Precedence chain (3-tier, RFC-0938): (1) Explicit config key from YAML (highest), (2) ANY_LLM_KEY env var, (3) {PROVIDER}_API_KEY env var (lowest). Note: RFC-0935's SecretReader is tier 4 when implemented.
- [ ] Log warning when using universal key

### Config Loading

- [ ] Update `parse_config()` to call `interpolate_yaml()` before YAML parse
- [ ] Handle YAML special characters in interpolated values. Validation: Reject interpolated values containing newlines, YAML flow indicators (`{`, `[`, `|`, `>`), and backslash. ALLOW quote characters (`"`, `'`) — real API keys often contain them. The validation prevents YAML structure injection, not value content.

### Tests

- [ ] `${VAR}` interpolation works
- [ ] `${VAR:-default}` works (default used when var undefined)
- [ ] `$$` escape produces literal `$` character
- [ ] Undefined variable without default resolves to empty string (NOT parse error)
- [ ] ANY_LLM_KEY works as fallback
- [ ] Combined precedence (RFC-0938): (1) config_key, (2) ANY_LLM_KEY, (3) {PROVIDER}_API_KEY. SecretReader is tier 4 when implemented.

## Key Files

- `crates/quota-router-core/src/config.rs` — interpolate_yaml(), parse_config()
- `crates/quota-router-core/src/proxy.rs` — resolve_api_key() (add ANY_LLM_KEY)

## Notes

The `interpolate_yaml()` function should be a simple character-by-character parser. The `parse_config()` function should call it before `serde_yaml::from_str()`.

**SecretReader integration:** After Mission-0935 completes, `resolve_api_key()` should integrate `SecretReader::get_secret()` as tier 5 (after provider-specific env var) per RFC-0935 Section 5.

**os.environ dependency:** `os.environ["KEY"]` syntax in YAML values is handled by Mission-0931-a (config-time resolution). This mission handles runtime resolution via env var fallback. They are complementary: Mission-0931-a resolves at config load, this mission resolves at request time.

**parse_config() modification scope:** Only the env var resolution layer changes. YAML parsing itself is unchanged. The interpolation runs on raw YAML text before parsing.

**SecretReader integration (when configured):** When RFC-0935's SecretReader is configured, call `secret_reader.get_secret(key_name)` as tier 4 (after env vars). If SecretReader returns `None`, fall through to env var check.
