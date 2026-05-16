# RFC-0907 (Economics): Configuration Management

## Status

Planned

## Authors

- Author: @cipherocto

## Summary

Define the configuration management system for the enhanced quota router, including YAML config files, environment variable overrides, and hot-reload support.

## Dependencies

**Requires:**

**Requires:**

- RFC-0929 (Economics): GatewayConfig Provider Dispatch Mapping (DispatchInfo, to_provider_map)

**Optional:**

- RFC-0900 (Economics): AI Quota Marketplace Protocol
- RFC-0901 (Economics): Quota Router Agent Specification
- RFC-0902: Multi-Provider Routing (router settings)
- RFC-0903: Virtual API Key System (key settings)
- RFC-0904: Real-Time Cost Tracking (pricing settings)
- RFC-0905: Observability (logging settings)
- RFC-0906: Response Caching (cache settings)
- RFC-0930: Provider Inference from Model String (model prefix → provider)
- RFC-0931: any-llm-mode Environment Variable Parity (env-var resolution)

## Why Needed

Configuration management enables:

- **Declarative setup** - Define router state in code
- **Environment flexibility** - Override via env vars
- **Hot-reload** - Update config without restart
- **LiteLLM compatibility** - Match config format

## Scope

### In Scope

- YAML configuration file
- Environment variable overrides
- Hot-reload support
- Config validation
- Default values
- Secret management

### Out of Scope

- Config UI/dashboard (future)
- Config versioning (future)
- Remote config storage (future)

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | <1s config load | Startup time |
| G2 | Hot-reload support | No downtime |
| G3 | Config validation | Fail fast |
| G4 | LiteLLM format | Compatibility |

## Specification

### Main Config Structure

```yaml
# config.yaml - Main configuration

model_list:
  - model_name: gpt-4o
    litellm_params:
      model: openai/gpt-4o
      api_base: https://api.openai.com/v1
      api_key: os.environ/OPENAI_API_KEY
      rpm: 1000

  - model_name: anthropic-claude
    litellm_params:
      model: anthropic/claude-3-opus-20240229

router_settings:
  routing_strategy: "least-busy"
  fallback:
    enabled: true
    max_retries: 3
  health_check:
    enabled: true
    interval_seconds: 30

litellm_settings:
  drop_params: true
  set_verbose: false
  cache: true

general_settings:
  master_key: os.environ/MASTER_KEY
  proxy_port: 4000
  health_check_route: /health

environment_variables:
  REDIS_HOST: "localhost"
  REDIS_PORT: "6379"
```

### Environment Variable Overrides

```bash
# Override specific values
export QUOTA_ROUTER_PROXY_PORT=8080
export QUOTA_ROUTER_MASTER_KEY=sk-secret

# Provider API keys
export OPENAI_API_KEY=sk-...
export ANTHROPIC_API_KEY=sk-...
```

### Config Loading

```rust
struct Config {
    model_list: Vec<ModelEntry>,
    router_settings: RouterSettings,
    litellm_settings: LiteLLMSettings,
    general_settings: GeneralSettings,
    environment_variables: HashMap<String, String>,
}

impl Config {
    fn load(path: &Path) -> Result<Self, ConfigError> {
        // 1. Load YAML
        let yaml = std::fs::read_to_string(path)?;

        // 2. Parse
        let mut config: Config = serde_yaml::from_str(&yaml)?;

        // 3. Apply env overrides
        config.apply_env_overrides();

        // 4. Validate
        config.validate()?;

        Ok(config)
    }
}
```

### Hot Reload

```rust
// Watch config file and reload
fn watch_config(path: &Path, callback: ConfigChanged) {
    let mut watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            if event.kind.is_modify() {
                // Reload config
                let new_config = Config::load(path)?;
                callback(new_config);
            }
        }
    });

    watcher.watch(path, Recursive::false).unwrap();
}
```

### Config Validation

```rust
fn validate(&self) -> Result<(), ConfigError> {
    // Check model list
    if self.model_list.is_empty() {
        return Err(ConfigError::NoModels);
    }

    // Check router settings
    self.router_settings.validate()?;

    // Check general settings
    if self.general_settings.proxy_port == 0 {
        return Err(ConfigError::InvalidPort);
    }

    // Check required env vars
    for model in &self.model_list {
        if model.litellm_params.api_key.starts_with("os.environ/") {
            let var = model.litellm_params.api_key.strip_prefix("os.environ/").unwrap();
            if std::env::var(var).is_err() {
                return Err(ConfigError::MissingEnvVar(var.to_string()));
            }
        }
    }

    Ok(())
}
```

### CLI Commands

```bash
# Validate config
quota-router config validate

# Show effective config
quota-router config show

# Reload config
quota-router config reload
```

### LiteLLM Compatibility

> **Critical:** Must track LiteLLM's config format exactly.

Reference LiteLLM's configuration:
- `model_list` format matches exactly
- `router_settings` maps to LiteLLM's router
- `litellm_settings` matches LiteLLM params
- Environment variable syntax: `os.environ/VAR_NAME`

### os.environ Resolution (LiteLLM Parity)

LiteLLM resolves environment variables at config load time using two syntaxes:

1. **Slash syntax:** `os.environ/VAR_NAME` — resolved via `std::env::var("VAR_NAME")`
2. **Bracket syntax:** `os.environ['VAR_NAME']` or `os.environ["VAR_NAME"]` — parsed via regex

**Resolution happens in `to_provider_map()` at config load time**, not at provider call time. This catches missing env vars early (fail fast at startup) and matches LiteLLM's behavior.

```rust
/// Resolve os.environ/VAR_NAME or os.environ['VAR_NAME'] syntax
fn resolve_env_var(value: &str) -> Result<String, ConfigError> {
    // Tier 1: os.environ/VAR_NAME (slash syntax)
    if let Some(var_name) = value.strip_prefix("os.environ/") {
        return std::env::var(var_name)
            .map_err(|_| ConfigError::MissingEnvVar(var_name.to_string()));
    }
    // Tier 2: os.environ['VAR_NAME'] or os.environ["VAR_NAME"] (bracket syntax)
    if let Some(caps) = ENV_BRACKET_RE.captures(value) {
        let var_name = &caps[1];
        return std::env::var(var_name)
            .map_err(|_| ConfigError::MissingEnvVar(var_name.to_string()));
    }
    // Tier 3: literal value
    Ok(value.to_string())
}
```

### Provider Inference from Model String

When `litellm_params.provider` is empty or omitted, infer provider from model string prefix:
- `openai/gpt-4o` → provider=`openai`
- `azure/gpt-4-turbo` → provider=`azure`
- `anthropic/claude-3-opus` → provider=`anthropic`

This is complementary to RFC-0930 (which specifies the inference logic). RFC-0907 specifies **when** inference runs (at `to_provider_map()` time during config load).

### api_base Fallback Chain

The full api_base resolution order (both modes):

1. Explicit `litellm_params.api_base` value
2. `os.environ[...]` syntax resolved from environment
3. `{PROVIDER}_API_BASE` env var (RFC-0931)
4. Provider-default from RFC-0930 registry

This applies to both litellm-mode (via `HttpCompletionRequest.api_base`) and any-llm-mode (via `py_bridge::factory::completion(api_base)`).

## Persistence

> **Critical:** Use CipherOcto/stoolap as the persistence layer.

Store in stoolap:
- Config snapshots (for rollback)
- Config history
- Effective config (computed)

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/config.rs` | os.environ resolution in to_provider_map(), provider inference fallback |
| `crates/quota-router-core/src/py_bridge/factory.rs` | {PROVIDER}_API_BASE env var fallback |
| `crates/quota-router-cli/src/config.rs` | Enhanced - YAML + validation |
| `crates/quota-router-cli/src/config_loader.rs` | New - config loading |
| `crates/quota-router-cli/src/config_watcher.rs` | New - hot reload |
| `crates/quota-router-cli/src/config_validator.rs` | New - validation |

## Future Work

- F1: Web UI for config
- F2: Config versioning/rollback
- F3: Remote config storage
- F4: Config templates

## Rationale

Configuration management is important for:

1. **Declarative setup** - Infrastructure as code
2. **Environment flexibility** - Dev/prod separation
3. **Operational efficiency** - Hot-reload
4. **LiteLLM migration** - Match config format exactly

## Version History

| Version | Date       | Changes |
| ------- | ---------- | --------|
| 1.1     | 2026-05-14 | Added os.environ resolution spec (slash + bracket syntax), provider inference from model string (complementary to RFC-0930), 4-tier api_base fallback chain, updated dependencies (RFC-0929, RFC-0930, RFC-0931), updated key files |
| 1.0     | 2026-03-12 | Initial draft |

---

**Planned Date:** 2026-03-12
**Related Use Case:** Enhanced Quota Router Gateway
**Related Research:** LiteLLM Analysis and Quota Router Comparison
