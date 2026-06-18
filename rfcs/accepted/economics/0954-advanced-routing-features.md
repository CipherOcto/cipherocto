---
title: "RFC-0954: Advanced Routing Features"
status: Accepted
version: 0.1.0
created: 2026-05-18
updated: 2026-05-18
authors:
  - quota-router team
related:
  - RFC-0902 (Multi-Provider Routing and Load Balancing)
  - RFC-0929 (GatewayConfig Provider Dispatch)
  - RFC-0933 (Rate Limiting Integration)
  - RFC-0936 (Pre-Call Checks)
---

# RFC-0954: Advanced Routing Features

## Status

Accepted

## Summary

Extend RFC-0902 (Multi-Provider Routing) and RFC-0936 (Pre-Call Checks) with enhanced routing features: Model Group Alias, improved Context Window Fallbacks, and per-model Allowed Fails configuration.

**Relationship to existing RFCs:**
- RFC-0936 (Accepted) already specifies Context Window Check and Health Check — this RFC extends those with per-model configuration
- RFC-0902 (Accepted) already specifies `context_window_fallbacks` in `router_settings` — this RFC adds per-model overrides
- This RFC adds Model Group Alias as genuinely new functionality

## Dependencies

**Requires:**

- RFC-0902 (Economics): Multi-Provider Routing and Load Balancing
- RFC-0929 (Economics): GatewayConfig Provider Dispatch
- RFC-0936 (Economics): Pre-Call Checks (Context Window Check, Health Check)

**Optional:**

- RFC-0933 (Economics): Rate Limiting Integration

## Design Goals

| Goal | Target | Metric |
|------|--------|--------|
| G1 | Context-aware fallback | Auto-switch on context overflow |
| G2 | Model aliasing | Flexible model naming |
| G3 | Failure thresholds | Configurable resilience |
| G4 | Zero config | Sensible defaults |

## Motivation

litellm provides sophisticated routing features that quota-router lacks:
- Context Window Fallbacks: Auto-switch to larger context model when input exceeds limit
- Model Group Alias: Use friendly names that map to multiple models
- Allowed Fails: Configure how many failures before removing a model from rotation

These features are critical for production deployments where resilience and flexibility matter.

## Specification

### Context Window Fallbacks (Extends RFC-0936)

RFC-0936 Section 2 defines the Context Window Check. This RFC extends it with per-model fallback configuration in `router_settings` (consistent with RFC-0902 line 219):

```yaml
# config.yaml (extends RFC-0902 format)
router_settings:
  # Global fallbacks (RFC-0902 format)
  context_window_fallbacks:
    gpt-3.5-turbo: gpt-3.5-turbo-16k
    gpt-4o: gpt-4-turbo

  # Per-model fallbacks (new in this RFC)
  model_context_window_fallbacks:
    gpt-4o:
      - model: gpt-4-turbo
        max_input_tokens: 128000
      - model: claude-3-opus
        max_input_tokens: 200000
    gpt-3.5-turbo:
      - model: gpt-4o
        max_input_tokens: 128000
```

#### Implementation (Extends RFC-0936 ContextWindowCheck)

```rust
// Extends RFC-0936's ContextWindowCheck with fallback support
struct ContextWindowFallback {
    model: String,
    max_input_tokens: u32,
}

// Uses RFC-0936's tiktoken-rs based token counting
// Uses Arc<RwLock<HashMap>> for thread-safe state (per RFC-0936)
async fn route_with_context_fallback(
    request: &CompletionRequest,
    config: &RouterConfig,
    health: Arc<RwLock<HashMap<String, HealthState>>>,
) -> Result<CompletionResponse, RouterError> {
    // Delegate to RFC-0936's ContextWindowCheck for token counting
    let input_tokens = count_tokens_tiktoken(&request.messages)?;

    if input_tokens > config.max_input_tokens {
        // Try per-model fallbacks first
        if let Some(fallback_list) = config.model_context_window_fallbacks.get(&config.model) {
            for fallback in fallback_list {
                if input_tokens <= fallback.max_input_tokens {
                    // Check health (RFC-0936 HealthCheck)
                    let mut health_state = health.write().await;
                    if let Some(state) = health_state.get_mut(&fallback.model) {
                        if state.is_available() {
                            return try_completion(&fallback.model, request).await;
                        }
                    } else {
                        // No health state = never used = available
                        return try_completion(&fallback.model, request).await;
                    }
                }
            }
        }
        // Try global fallbacks (RFC-0902 format: model -> single fallback)
        if let Some(fallback_model) = config.context_window_fallbacks.get(&config.model) {
            let mut health_state = health.write().await;
            if let Some(state) = health_state.get_mut(fallback_model) {
                if state.is_available() {
                    return try_completion(fallback_model, request).await;
                }
            } else {
                return try_completion(fallback_model, request).await;
            }
        }
        return Err(RouterError::ContextWindowExceeded {
            input_tokens,
            max_tokens: config.max_input_tokens,
        });
    }

    try_completion(&config.model, request).await
}
```

### Model Group Alias

Map friendly names to multiple models for simplified routing.

```yaml
# config.yaml
model_list:
  # Group alias
  - model_name: best-model
    litellm_params:
      model_list:
        - model: openai/gpt-4o
          weight: 0.5
        - model: anthropic/claude-3-opus
          weight: 0.3
        - model: google/gemini-pro
          weight: 0.2

  # Simple alias
  - model_name: fast
    litellm_params:
      model: openai/gpt-3.5-turbo

  # Tiered alias
  - model_name: tier1
    litellm_params:
      model_list:
        - model: openai/gpt-4o
        - model: anthropic/claude-3-opus
      routing_strategy: least-busy
```

#### Implementation

```rust
struct ModelGroupAlias {
    name: String,
    models: Vec<ModelEntry>,
    routing_strategy: Option<RoutingStrategy>,
}

struct ModelEntry {
    model: String,
    weight: Option<f32>,
    max_input_tokens: Option<u32>,
}

fn resolve_model_alias(
    model_name: &str,
    config: &Config,
) -> Result<String, RouterError> {
    // Check if model_name is an alias
    if let Some(alias) = config.model_aliases.get(model_name) {
        // Select model based on routing strategy
        match &alias.routing_strategy {
            Some(strategy) => select_by_strategy(strategy, &alias.models),
            None => select_by_weight(&alias.models),
        }
    } else {
        // Use model_name directly
        Ok(model_name.to_string())
    }
}
```

### Allowed Fails (Extends RFC-0936 HealthCheck)

RFC-0936 Section 4 defines the Health Check with `HealthState`. This RFC extends it with per-model configuration:

```yaml
# config.yaml (extends RFC-0902/0936 format)
router_settings:
  # Global settings (RFC-0936 compatible)
  allowed_fails: 3
  allowed_fails_window: 60
  cooldown_time: 300

  # Per-model overrides (new in this RFC)
  model_allowed_fails:
    openai/gpt-4o:
      allowed_fails: 5
      cooldown_time: 600
    anthropic/claude-3-opus:
      allowed_fails: 2
      cooldown_time: 120
```

#### Implementation (Extends RFC-0936 HealthState)

```rust
// Extends RFC-0936's HealthState with per-model config
// Uses Arc<RwLock<HashMap>> for thread safety (per RFC-0936)
struct AllowedFailsConfig {
    allowed_fails: u32,
    allowed_fails_window: u64,  // seconds
    cooldown_time: u64,         // seconds
}

// Extends RFC-0936's HealthState
struct ExtendedHealthState {
    fail_count: u32,
    last_fail: Option<Instant>,
    last_success: Option<Instant>,
    cooldown_until: Option<Instant>,
    config: AllowedFailsConfig,
}

impl ExtendedHealthState {
    fn record_failure(&mut self) {
        self.fail_count += 1;
        self.last_fail = Some(Instant::now());

        if self.fail_count >= self.config.allowed_fails {
            self.cooldown_until = Some(
                Instant::now() + Duration::from_secs(self.config.cooldown_time)
            );
        }
    }

    fn record_success(&mut self) {
        self.last_success = Some(Instant::now());
        // Reset fail count on success
        self.fail_count = 0;
        self.cooldown_until = None;
    }

    fn cooldown_remaining(&self) -> Option<Duration> {
        self.cooldown_until.map(|until| {
            let now = Instant::now();
            if now < until { until - now } else { Duration::ZERO }
        })
    }

    fn is_available(&mut self) -> bool {
        // Check if in cooldown
        if let Some(cooldown) = self.cooldown_until {
            if Instant::now() < cooldown {
                return false;
            }
            // Cooldown expired - reset state
            self.fail_count = 0;
            self.cooldown_until = None;
        }

        // Check if fail count exceeds threshold within window
        if let Some(last_fail) = self.last_fail {
            if last_fail.elapsed().as_secs() > self.config.allowed_fails_window {
                // Outside window - reset
                self.fail_count = 0;
                return true;
            }
        }

        self.fail_count < self.config.allowed_fails
    }
}
```

### Integration with Routing

All features MUST integrate with existing routing infrastructure:

```rust
async fn route_request(
    request: &CompletionRequest,
    config: &Config,
    health: &mut HashMap<String, ModelHealth>,
) -> Result<CompletionResponse, RouterError> {
    // 1. Resolve model alias
    let model = resolve_model_alias(&request.model, config)?;

    // 2. Check context window
    let model_config = config.get_model_config(&model)?;
    let input_tokens = count_tokens(&request.messages);

    if input_tokens > model_config.max_input_tokens {
        // Try context window fallbacks
        for fallback in &model_config.context_window_fallbacks {
            if input_tokens <= fallback.max_input_tokens {
                let fallback_health = health.get_mut(&fallback.model);
                if fallback_health.map_or(true, |h| h.is_available()) {
                    return try_completion(&fallback.model, request).await;
                }
            }
        }
        return Err(RouterError::ContextWindowExceeded);
    }

    // 3. Check model health
    let model_health = health.get_mut(&model);
    if let Some(h) = model_health {
        if !h.is_available() {
            return Err(RouterError::ModelUnavailable {
                model,
                cooldown_remaining: h.cooldown_remaining(),
            });
        }
    }

    // 4. Execute request
    match try_completion(&model, request).await {
        Ok(response) => {
            // Reset fail count on success
            if let Some(h) = health.get_mut(&model) {
                h.record_success();
            }
            Ok(response)
        }
        Err(e) => {
            // Record failure
            if let Some(h) = health.get_mut(&model) {
                h.record_failure();
            }
            Err(e)
        }
    }
}
```

### Health Endpoint

Expose model health via `/health/models` endpoint (extends RFC-0905 `/health` endpoints):

```rust
#[derive(Serialize)]
struct ModelHealthResponse {
    model: String,
    is_available: bool,
    fail_count: u32,
    cooldown_remaining_ms: Option<u64>,
    last_success: Option<i64>,  // Unix timestamp
    last_failure: Option<i64>,  // Unix timestamp
}

#[derive(Serialize)]
struct HealthModelsResponse {
    status: String,  // "healthy", "degraded", "unhealthy"
    models: Vec<ModelHealthResponse>,
}
```

**Example response:**
```json
GET /health/models

{
  "status": "degraded",
  "models": [
    {
      "model": "openai/gpt-4o",
      "is_available": true,
      "fail_count": 0,
      "cooldown_remaining_ms": null,
      "last_success": 1716038400,
      "last_failure": null
    },
    {
      "model": "anthropic/claude-3-opus",
      "is_available": false,
      "fail_count": 3,
      "cooldown_remaining_ms": 245000,
      "last_success": null,
      "last_failure": 1716038700
    }
  ]
}
```

## Acceptance Criteria

- [ ] Context window fallback triggers when input exceeds limit
- [ ] Fallback models are tried in order
- [ ] Model group alias resolves to correct model
- [ ] Weighted selection works for model groups
- [ ] Allowed fails removes model after threshold
- [ ] Cooldown period prevents immediate retry
- [ ] Fail count resets outside window
- [ ] Health endpoint shows model status
- [ ] Per-model overrides work
- [ ] All existing tests pass

## Key Files

| File | Change |
|------|--------|
| `crates/quota-router-core/src/routing/context_fallback.rs` | New - context window fallbacks |
| `crates/quota-router-core/src/routing/model_alias.rs` | New - model group alias |
| `crates/quota-router-core/src/routing/health_tracker.rs` | New - allowed fails tracking |
| `crates/quota-router-core/src/routing/mod.rs` | Integrate new features |
| `crates/quota-router-core/src/config.rs` | Add routing config |
| `crates/quota-router-core/src/handlers/health.rs` | Expose model health |

## Security Considerations

- Health endpoint MUST not expose API keys
- Cooldown times MUST be bounded (prevent infinite cooldown)
- Fail counts MUST be per-process (not shared across instances)

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2026-05-18 | Initial draft |
