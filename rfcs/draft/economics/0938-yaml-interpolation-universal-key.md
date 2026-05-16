# RFC-0938: YAML Interpolation & Universal Key

## Status: Draft

## Summary

Add `${VAR}` YAML interpolation and `ANY_LLM_KEY` universal key support, matching any-llm's gateway config behavior.

## Motivation

any-llm's gateway config supports:
- `${VAR_NAME}` syntax for environment variable interpolation in YAML
- `ANY_LLM_KEY` environment variable as a universal key for all providers

quota-router uses `os.environ["KEY"]` syntax (RFC-0931) but doesn't support `${VAR}` interpolation or universal keys. This RFC adds both for any-llm compatibility.

## Specification

### 1. YAML Interpolation

Parse `${VAR_NAME}` in YAML values and replace with environment variable values:

```rust
fn interpolate_yaml(value: &str) -> Result<String> {
    let mut result = String::new();
    let mut chars = value.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'$') {
            // $$ escape — consume both and push literal $
            chars.next();
            result.push('$');
        } else if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // skip '{'
            let mut var_name = String::new();
            let mut default_value = None;

            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(':') if chars.peek() == Some(&'-') => {
                        chars.next(); // skip '-'
                        let mut default = String::new();
                        loop {
                            match chars.next() {
                                Some('}') => break,
                                Some(c) => default.push(c),
                                None => return Err(ConfigError::NotYetSpecified("Unterminated ${...} interpolation".to_string())),
                            }
                        }
                        default_value = Some(default);
                        break;
                    }
                    Some(c) => var_name.push(c),
                    None => return Err(ConfigError::NotYetSpecified("Unterminated ${...} interpolation".to_string())),
                }
            }

            // Resolve: env var > default > empty string (NO error on undefined)
            let var_value = std::env::var(&var_name)
                .unwrap_or_else(|_| default_value.unwrap_or_default());
            result.push_str(&var_value);
        } else {
            result.push(c);
        }
    }

    Ok(result)
}
```

**Behavior change:** Undefined variables resolve to empty string (or default value if `${VAR:-default}` syntax used). This matches LiteLLM's behavior where undefined vars are silently ignored. Use `${VAR}` without default for required vars — validation happens later when the config is used.

### 2. Config Loading

Apply interpolation BEFORE YAML parsing:
```rust
fn parse_config(yaml: &str) -> Result<GatewayConfig> {
    // 1. Interpolate environment variables (before YAML parse)
    let interpolated = interpolate_yaml(yaml)?;

    // 2. Parse YAML
    let config: GatewayConfig = serde_yaml::from_str(&interpolated)?;

    Ok(config)
}
```

**Parsing order:** Interpolation happens before YAML parse. This means `${VAR}` is replaced with the env var value, then the result is parsed as YAML. If the env var value contains YAML special characters (e.g., `:`, `#`), they will be interpreted as YAML syntax. Users should quote interpolated values that may contain special characters.

### 3. Universal Key

Support `ANY_LLM_KEY` as fallback for all providers. **Precedence matches any-llm's `_create_provider()` behavior:**

```rust
async fn resolve_api_key(
    config_key: Option<&str>,
    provider: &str,
) -> Result<Option<String>> {
    // 1. Explicit config key (from YAML)
    if let Some(key) = config_key {
        if !key.is_empty() {
            return Ok(Some(key.to_string()));
        }
    }

    // 2. ANY_LLM_KEY (any-llm compat — checked BEFORE provider-specific)
    // Matches any-llm's _create_provider() which checks ANY_LLM_KEY first
    if let Ok(key) = std::env::var("ANY_LLM_KEY") {
        if !key.is_empty() {
            return Ok(Some(key));
        }
    }

    // 3. Provider-specific env var
    let env_key = format!("{}_API_KEY", provider.to_uppercase());
    if let Ok(key) = std::env::var(&env_key) {
        if !key.is_empty() {
            return Ok(Some(key));
        }
    }

    // 4. SecretReader (optional — only if RFC-0935 is implemented)
    // When SecretReader is configured, add as additional fallback:
    // if let Some(key) = secret_reader.get_secret(&env_key).await? {
    //     return Ok(Some(key));
    // }

    Ok(None)
}
```

**Precedence note:** ANY_LLM_KEY is checked BEFORE provider-specific env vars. This matches any-llm's behavior where `ANY_LLM_KEY` is checked in `_create_provider()` before the provider's `ENV_API_KEY_NAME`.

**SecretReader integration:** When RFC-0935 is implemented, `resolve_api_key()` should integrate `SecretReader::get_secret()` as tier 4 (after provider-specific env var, before returning None). This is optional — the function works without it using only env vars.

### 4. Configuration Example

```yaml
providers:
  openai:
    api_key: ${OPENAI_API_KEY}
    api_base: https://api.openai.com/v1
  anthropic:
    api_key: ${ANTHROPIC_API_KEY}
    api_base: https://api.anthropic.com/v1
  custom:
    api_key: ${CUSTOM_API_KEY}
    api_base: ${CUSTOM_API_BASE}
```

### 5. Syntax Compatibility

| Syntax | quota-router | LiteLLM | any-llm |
|--------|-------------|---------|---------|
| `${VAR}` | This RFC | NOT supported | Supported |
| `os.environ["KEY"]` | RFC-0931 | Supported | NOT supported |
| `os.environ/KEY` | Conditional (RFC-0935 EnvSecretManager) | NOT supported | NOT supported |
| `ANY_LLM_KEY` | This RFC | NOT supported | Supported |

### 6. Escaping

To include literal `$` in YAML values, use `$$`:
```yaml
description: "Use $$100 for dollar amounts"
# Result: "Use $100 for dollar amounts"
```

**Rule:** `$$` is replaced with `$` during interpolation. `${` without preceding `$` starts interpolation.

**Note:** Literal `${...}` cannot be produced by escaping. To include a literal `${` in output, use one of these workarounds:
```yaml
# Workaround 1: Use an env var containing the literal text
template: ${LITERAL_BRACE}  # LITERAL_BRACE="${VAR}"

# Workaround 2: Use YAML single quotes to prevent interpolation
template: '${VAR}'  # Single quotes prevent ${} interpolation

# Workaround 3: Use Unicode escape if supported by downstream parser
template: "\u0024{VAR}"  # $ as Unicode escape
```
This is a known limitation of the `$$`-based escaping approach.

**Supported syntax:** Only `${VAR:-default}` (colon-dash) is supported. `${VAR-default}` (without colon) is intentionally NOT supported. This deviates from shell semantics where `:-` substitutes if unset OR empty, while `-` substitutes only if unset.

### 7. Security

- Undefined variables resolve to empty string (no error, matches LiteLLM behavior)
- No recursive interpolation (prevent infinite loops)
- Variables can't contain other variables
- Log warning when using universal key (ANY_LLM_KEY)
- Env var values are NOT recursively interpolated
- YAML injection risk: interpolation happens before YAML parse. If an env var contains YAML structural characters (`:`, `#`, `{`, `}`, `[`, `]`, `|`, `>`), it can break or inject YAML structure. Mitigations:
  - **Recommended:** Restrict interpolation to values that are already quoted in YAML (e.g., `key: "${VAR}"`)
  - **Alternative:** Validate that interpolated values don't contain YAML metacharacters before substitution
  - **Alternative:** Use two-pass approach: parse YAML with placeholders first, then substitute in AST
  - **Accepted risk:** In containerized deployments where env vars come from trusted sources (ConfigMaps, secrets), the risk is lower. Document this as a deployment consideration.

## Dependencies

- RFC-0931: os.environ["KEY"] syntax (complementary)
- RFC-0935: Secret Manager Integration (optional — SecretReader as additional fallback tier)

## Test Plan

1. `${VAR}` interpolation replaces with env var value
2. `${VAR:-default}` uses default when var undefined
3. Undefined variable without default resolves to empty string (NOT parse error)
4. `$$` escape produces literal `$` (single dollar sign)
5. Nested `${VAR}` not supported (no recursion)
6. `ANY_LLM_KEY` works as fallback for any provider
7. Config key > ANY_LLM_KEY > provider-specific env var (matches any-llm precedence)
8. Empty env var treated as not set
9. Config with mixed syntaxes works
10. Security: no injection via variable values
