# RFC-0931: any-llm-mode Environment Variable Parity

## Status

Accepted

## Summary

Specify environment variable fallback resolution for api_base and api_key in `any-llm-mode` (PyO3/py_bridge path), matching LiteLLM's behavior. Currently only `litellm-mode` resolves env vars; `any-llm-mode` requires explicit configuration.

## Problem Statement

LiteLLM resolves environment variables at config load time:
- `api_key: "os.environ['API_KEY_NAME']"` → resolved from environment
- `api_base: "os.environ['PROVIDER_API_BASE']"` → resolved from environment

Current CipherOcto `any-llm-mode` implementation passes literal strings to py_bridge factory — no env-var resolution. This breaks LiteLLM drop-in compatibility where users rely on env vars.

## LiteLLM `os.environ` Syntax

**Note:** LiteLLM config uses Python syntax `os.environ['KEY_NAME']` (with brackets and quotes), not `os.environ/KEY_NAME`. The `/` separator is a common misreading.

Example LiteLLM YAML config:
```yaml
litellm_params:
  api_key: os.environ["OPENAI_API_KEY"]
  api_base: os.environ["OPENAI_API_BASE"]
```

Resolution strips the `os.environ[` prefix and `]` suffix, then looks up the remaining key name as an environment variable.

**Single-quote syntax:** Both `os.environ["KEY"]` (double-quote) and `os.environ['KEY']` (single-quote) are supported.

## Specification

### 1. Resolution Order

**api_key** — 2-tier resolution at config load time:

1. Explicit non-empty non-`os.environ` value in litellm_params (highest priority)
2. `os.environ["KEY"]` or `os.environ['KEY']` syntax — resolves from environment

**Note:** `{PROVIDER}_API_KEY` env var is resolved at runtime by RFC-0938's `resolve_api_key()`, not at config load time. This ensures correct precedence: config > os.environ > ANY_LLM_KEY > {PROVIDER}_API_KEY.

**api_base** — 4-tier resolution:

1. Explicit non-empty value in litellm_params (highest priority)
2. `os.environ["KEY"]` or `os.environ['KEY']` syntax — resolves from environment (only if explicit value is absent or empty)
3. Environment variable: `{PROVIDER}_API_BASE` (only if tiers 1-2 are absent)
4. Provider-specific default from RFC-0930 registry (lowest priority)

### 2. Environment Variable Naming

| Field | Env Var Pattern | Example |
|-------|-----------------|---------|
| api_key (openai) | `OPENAI_API_KEY` | `OPENAI_API_KEY=sk-...` |
| api_key (anthropic) | `ANTHROPIC_API_KEY` | `ANTHROPIC_API_KEY=sk-ant-...` |
| api_base (openai) | `OPENAI_API_BASE` | `OPENAI_API_BASE=https://api.openai.com/v1` |
| api_base (azure) | `AZURE_API_BASE` | `AZURE_API_BASE=https://...` |

Provider names are uppercased with underscores: `OPENAI`, `ANTHROPIC`, `MISTRAL`, `AZURE`, etc.

**Requires non-empty provider.** If provider is empty (no explicit and no inferred), env var fallback is skipped and returns `None`.

### 3. Empty String Handling

An explicitly set empty string `api_key: ""` or `api_base: ""` is treated as **absent** and triggers env var fallback. This differs from `None` (not set) in serde terms, but matches user expectation that empty config should not block env var usage.

Both `resolve_api_key()` and `resolve_api_base()` apply this rule.

### 4. `os.environ` Key Extraction

```rust
/// Extract key name from os.environ["KEY"] or os.environ['KEY'] syntax
/// Returns None if input doesn't match the pattern
fn extract_os_environ_key(s: &str) -> Option<String> {
    let s = s.trim();
    if !s.starts_with("os.environ") {
        return None;
    }
    // Strip os.environ[ prefix
    let inner = s.strip_prefix("os.environ")?;
    // Handle both quote styles: ["KEY"] or ['KEY']
    // Minimum: ["x"] = 3 chars after stripping os.environ (bracket + quote + char + quote)
    if inner.starts_with('[') && inner.len() >= 4 {
        let inner = &inner[1..inner.len() - 1];
        // Strip surrounding quotes (double or single)
        if (inner.starts_with('"') && inner.ends_with('"'))
            || (inner.starts_with('\'') && inner.ends_with('\''))
        {
            return Some(inner[1..inner.len() - 1].to_string());
        }
    }
    None
}
```

### 5. resolve_api_key Implementation

```rust
impl LiteLLMParams {
    /// Resolve api_key with env fallback
    pub fn resolve_api_key(&self) -> Option<String> {
        // 1. Explicit non-empty value
        if let Some(ref key) = self.api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("os.environ") {
                return Some(key.clone());
            }
        }
        // 2. os.environ[...] syntax
        if let Some(ref key) = self.api_key {
            if let Some(env_name) = extract_os_environ_key(key) {
                if let Ok(val) = std::env::var(&env_name) {
                    return Some(val);
                }
            }
        }
        // {PROVIDER}_API_KEY is NOT resolved here — resolved at runtime by
        // RFC-0938's resolve_api_key() which checks ANY_LLM_KEY first.
        // This ensures correct precedence: config > os.environ > ANY_LLM_KEY > {PROVIDER}_API_KEY
        None
    }
}
```

### 6. resolve_api_base Implementation

```rust
impl LiteLLMParams {
    /// Resolve api_base with 4-tier fallback:
    /// 1. Explicit non-empty value (api_base takes precedence over base_url alias)
    /// 2. os.environ[...] syntax
    /// 3. {PROVIDER}_API_BASE env var
    /// 4. Provider-specific default from RFC-0930 registry
    ///
    /// When both `api_base` and `base_url` are set, first non-empty wins.
    /// `base_url` is a LiteLLM alias for `api_base` (defined via serde alias in RFC-0927).
    ///
    /// SUPERSEDES RFC-0927's resolve_api_base() which returned Option<&str> with simple
    /// api_base.or(base_url). This 4-tier version replaces it with env var resolution.
    pub fn resolve_api_base(&self) -> Option<String> {
        // 1. Explicit non-empty value (check both api_base and base_url, take first non-empty)
        for base in [self.api_base.as_ref(), self.base_url.as_ref()].iter().flatten() {
            let trimmed = base.trim();
            if !trimmed.is_empty() && !trimmed.starts_with("os.environ") {
                return Some(trimmed.to_string());
            }
        }
        // 2. os.environ[...] syntax (check both api_base and base_url alias)
        for base in [self.api_base.as_ref(), self.base_url.as_ref()].iter().flatten() {
            if let Some(env_name) = extract_os_environ_key(base) {
                if let Ok(val) = std::env::var(&env_name) {
                    return Some(val);
                }
            }
        }
        // 3. {PROVIDER}_API_BASE env var (only if provider is non-empty)
        if !self.provider.is_empty() {
            let env_base = format!("{}_API_BASE", self.provider.to_uppercase());
            if let Ok(val) = std::env::var(&env_base) {
                return Some(val);
            }
        }
        // 4. Provider-specific default from RFC-0930 registry
        if !self.provider.is_empty() {
            return get_provider_default_api_base(&self.provider);
        }
        None
    }
}
```

### 7. Resolution at to_provider_map Time

Resolve env vars at config load time (during `to_provider_map()`), not at provider call time. This:
- Catches missing env vars early (fail fast at startup)
- Avoids repeated env var lookups per request
- Matches LiteLLM's config-load-time resolution

**Known limitation:** Env vars must be set before server startup. Env vars set after startup (e.g., in a separate shell process) are not picked up.

### 8. Feature Gate Scope

**This RFC applies to all modes.**

The `resolve_api_key()` and `resolve_api_base()` methods are available in all builds (NOT feature-gated). RFC-0930's `to_provider_map()` calls these methods in all modes.

```rust
impl LiteLLMParams {
    // ... resolution methods (available in all modes)
}
```

## Dependencies

**Requires:**
- RFC-0927: RouterConfig Extension for LiteLLM Compatibility (LiteLLMParams struct)
- RFC-0930: Provider Inference from Model String (get_provider_default_api_base for tier 4)

**Required by:**
- RFC-0930: Provider Inference from Model String (calls resolve_api_key and resolve_api_base in to_provider_map)
- RFC-0938: YAML Interpolation & Universal Key (calls resolve methods at dispatch time, adds {PROVIDER}_API_KEY and ANY_LLM_KEY resolution)

**RFC-0930 is required for full api_base resolution.**

Tier 4 of `resolve_api_base()` calls `get_provider_default_api_base()` which is defined in RFC-0930. If RFC-0930 is not implemented, tier 4 returns `None` for all providers, and the api_base resolution chain ends at tier 3 (env var).

Implement RFC-0930 before RFC-0931 to get full 4-tier api_base resolution. Without RFC-0930, the behavior is identical to a 3-tier resolution.

## Interaction with RFC-0930

RFC-0930 provides the provider-default api_base registry used in tier 4 of `resolve_api_base()`:

```
resolve_api_base() flow:
  1. Explicit api_base? → use it
  2. os.environ[...] syntax? → resolve env var
  3. {PROVIDER}_API_BASE env var? → use it
  4. Provider-default from get_provider_default_api_base(provider)? → use it
  5. None
```

The `get_provider_default_api_base()` function from RFC-0930 is called to fulfill tier 4.

## Integration with to_provider_map

RFC-0930 §5 shows `to_provider_map()` populating `DispatchInfo` with resolved values:

```rust
DispatchInfo {
    provider: provider.clone(),
    model: deployment.litellm_params.model.clone(),
    api_key: deployment.litellm_params.resolve_api_key(),  // RFC-0931 resolved
    api_base,  // RFC-0931 resolved (via 4-tier resolve_api_base())
    // ...
}
```

The resolved `api_key` and `api_base` from RFC-0931 are stored directly in `DispatchInfo`. The factory receives these resolved values — no further resolution needed at call time.

## Implementation

### Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | Add `extract_os_environ_key()` supporting both quote styles, implement `resolve_api_key()` and `resolve_api_base()` with full 4-tier logic |
| `crates/quota-router-core/src/config.rs` | (Already covered by RFC-0930) Update `to_provider_map()` to use resolved values |
| `crates/quota-router-core/src/py_bridge/factory.rs` | (no changes needed — factory receives resolved values) |

### Tests

```rust
#[test]
fn test_resolve_api_key_explicit() {
    std::env::remove_var("OPENAI_API_KEY");
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: Some("sk-explicit".to_string()),
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), Some("sk-explicit".to_string()));
}

#[test]
fn test_resolve_api_key_empty_string_returns_none() {
    // Empty string treated as absent — resolve_api_key() returns None
    // {PROVIDER}_API_KEY is resolved at runtime by RFC-0938, not here
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: Some("".to_string()),
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), None);
}

#[test]
fn test_resolve_api_key_none_returns_none() {
    // api_key: None — resolve_api_key() returns None
    // {PROVIDER}_API_KEY is resolved at runtime by RFC-0938, not here
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: None,
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), None);
}

#[test]
fn test_resolve_api_key_os_environ_double_quote() {
    std::env::set_var("MY_API_KEY", "sk-from-env-var");
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: Some(r#"os.environ["MY_API_KEY"]"#.to_string()),
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), Some("sk-from-env-var".to_string()));
    std::env::remove_var("MY_API_KEY");
}

#[test]
fn test_resolve_api_key_os_environ_single_quote() {
    std::env::set_var("MY_API_KEY", "sk-from-single-quote");
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: Some("os.environ['MY_API_KEY']".to_string()),
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), Some("sk-from-single-quote".to_string()));
    std::env::remove_var("MY_API_KEY");
}

#[test]
fn test_resolve_api_key_os_environ_empty_key_returns_none() {
    // os.environ[""] — extract_os_environ_key returns Some("") (empty key name)
    // std::env::var("") fails (empty var name is invalid on most systems)
    // resolve_api_key() returns None — {PROVIDER}_API_KEY resolved at runtime by RFC-0938
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: Some(r#"os.environ[""]"#.to_string()),
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), None);
}

#[test]
fn test_resolve_api_key_os_environ_nonexistent_var_returns_none() {
    // os.environ["NONEXISTENT_VAR_12345"] — env var doesn't exist
    // resolve_api_key() returns None — {PROVIDER}_API_KEY resolved at runtime by RFC-0938
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: Some(r#"os.environ["NONEXISTENT_VAR_12345"]"#.to_string()),
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), None);
}

#[test]
fn test_resolve_api_key_empty_provider_skips_env_var() {
    // Empty provider should NOT produce "_API_KEY" env var lookup
    let params = LiteLLMParams {
        provider: "".to_string(),  // Empty
        api_key: None,
        ..Default::default()
    };
    assert_eq!(params.resolve_api_key(), None);
}

#[test]
fn test_resolve_api_base_tier_4_provider_default() {
    // When no explicit, no env var, use provider default (tier 4)
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_key: None,
        api_base: None,
        ..Default::default()
    };
    assert_eq!(params.resolve_api_base(), Some("https://api.openai.com/v1".to_string()));
}

#[test]
fn test_resolve_api_base_empty_string_falls_back_to_env() {
    std::env::set_var("OPENAI_API_BASE", "https://custom.openai.com/v1");
    let params = LiteLLMParams {
        provider: "openai".to_string(),
        api_base: Some("".to_string()),  // Empty string treated as absent
        ..Default::default()
    };
    assert_eq!(params.resolve_api_base(), Some("https://custom.openai.com/v1".to_string()));
    std::env::remove_var("OPENAI_API_BASE");
}
```

## Acceptance Criteria

- [ ] `resolve_api_key()` treats empty string as absent (falls back to env var)
- [ ] `resolve_api_key()` skips env var fallback when provider is empty
- [ ] `resolve_api_key()` resolves `os.environ["KEY"]` (double-quote) syntax correctly
- [ ] `resolve_api_key()` resolves `os.environ['KEY']` (single-quote) syntax correctly
- [ ] `resolve_api_base()` treats empty string as absent
- [ ] `resolve_api_base()` falls back to `{PROVIDER}_API_BASE` env var when explicit value not set
- [ ] `resolve_api_base()` falls back to provider-default from RFC-0930 registry (tier 4)
- [ ] Available in all modes (NOT feature-gated)
- [ ] Existing tests still pass
- [ ] Clippy clean

## Relationship to RFC-0930

RFC-0930 (Provider Inference) and RFC-0931 (Env Var Parity) are independent but complementary:
- RFC-0930: Infers provider from model string when not explicitly set
- RFC-0931: Resolves api_key/api_base from env vars when not explicitly set

Both improve LiteLLM drop-in compatibility in `any-llm-mode`.

When both are implemented, the resolution order for api_base is:
1. Explicit value → 2. `os.environ[...]` syntax → 3. `{PROVIDER}_API_BASE` env var → 4. Provider-default from RFC-0930 registry

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 2 | 2026-05-15 | Adversarial review R1 fixes: C1 — api_key resolution clarified as 2-tier at config load time (os.environ syntax is tier 2); {PROVIDER}_API_KEY moved to runtime resolution by RFC-0938; M2 — extract_os_environ_key len check fixed from >= 2 to >= 4; tests updated to reflect 2-tier behavior |
| 1 | 2026-05-14 | Initial draft |