# RFC-0936: Pre-call Checks

## Status: Accepted

## Summary

Add pre-call checks that filter deployments before routing, matching LiteLLM's `enable_pre_call_checks` behavior. Checks include context window limits, tag filtering, and model availability.

## Motivation

LiteLLM's Router filters deployments before routing based on:
- Context window limits (max_input_tokens, max_output_tokens)
- Tag filtering (allowed tags, blocked tags)
- Model availability (health checks)

quota-router routes to all deployments in a model group without filtering. This RFC adds pre-call filtering.

## Specification

### 0. New Types Required

The following types must be added to the codebase. See RFC-0928 for the full `DeploymentConfig` definition including `litellm_params`.

**Extension model:** The new fields on `ModelInfo` and `DeploymentConfig` should be added directly to the structs in `config.rs`. These are RFC-0936-specific extensions to the RFC-0928 schema. Use `Option<>` for all new fields to maintain backward compatibility with existing configs that don't specify them. Default values: `max_input_tokens: None`, `max_output_tokens: None`, `allowed_tags: None`, `blocked_tags: None`, `last_health_check: None`, `is_healthy: true`.

```rust
// In config.rs — new fields on ModelInfo (RFC-0928)
pub struct ModelInfo {
    // ... existing fields ...
    pub max_input_tokens: Option<usize>,   // NEW: max input tokens for context window
    pub max_output_tokens: Option<usize>,  // NEW: max output tokens
    pub allowed_tags: Option<Vec<String>>, // NEW: tags that can use this deployment
    pub blocked_tags: Option<Vec<String>>, // NEW: tags that cannot use this deployment
    // supports_embeddings already exists on ModelInfo (RFC-0928)
}

// In router.rs — NEW fields added to existing Router struct (not a replacement)
// Note: model_groups() already exists as a method on Router — field name may need adjustment
pub struct Router {
    // ... existing fields from router.rs ...
    pub deployment_groups: HashMap<String, Vec<DeploymentConfig>>,  // NEW: grouped deployments (renamed to avoid conflict with existing model_groups() method)
    pub pre_call_checks: Vec<Box<dyn PreCallCheck>>,               // NEW: check pipeline
}

// New type for completion requests
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub max_tokens: Option<usize>,
    pub tags: Option<Vec<String>>,
    pub model: String,
}

pub struct Message {
    pub role: String,
    pub content: String,
}
```

### 1. Pre-call Check Trait

```rust
#[async_trait]
pub trait PreCallCheck: Send + Sync {
    async fn check(&self, deployment: &DeploymentConfig, request: &CompletionRequest) -> CheckResult;
}

pub enum CheckResult {
    Pass,
    Fail { reason: String },
}
```

**Note:** Trait is async because HealthCheck needs HTTP calls. All implementations must be async.

### 2. Context Window Check

```rust
pub struct ContextWindowCheck {
    tokenizer: tiktoken_rs::CoreBPE,  // tiktoken-rs crate
}

impl ContextWindowCheck {
    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        // Use tiktoken-rs for accurate token counting
        let total_text: String = messages.iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Fallback to character/4 approximation if tokenizer fails
        match self.tokenizer.encode_with_special_tokens(&total_text) {
            tokens if !tokens.is_empty() => tokens.len(),
            _ => {
                // Fallback: approximate 4 chars per token
                let total_chars: usize = messages.iter()
                    .map(|m| m.content.len())
                    .sum();
                total_chars / 4
            }
        }
    }
}

#[async_trait]
impl PreCallCheck for ContextWindowCheck {
    async fn check(&self, deployment: &DeploymentConfig, request: &CompletionRequest) -> CheckResult {
        let model_info = &deployment.model_info;

        // Check max_input_tokens
        if let Some(max_input) = model_info.max_input_tokens {
            let input_tokens = self.estimate_tokens(&request.messages);
            if input_tokens > max_input {
                return CheckResult::Fail {
                    reason: format!("Input tokens {} exceeds max_input_tokens {}", input_tokens, max_input),
                };
            }
        }

        // Check max_output_tokens
        if let Some(max_output) = model_info.max_output_tokens {
            let requested_output = request.max_tokens.unwrap_or(max_output);
            if requested_output > max_output {
                return CheckResult::Fail {
                    reason: format!("Requested tokens {} exceeds max_output_tokens {}", requested_output, max_output),
                };
            }
        }

        CheckResult::Pass
    }
}
```

**Token estimation:** Uses tiktoken for accurate counting. Falls back to character/4 approximation if tokenizer unavailable. Performance: cache tokenizer instances per model to avoid repeated initialization.

### 3. Tag Filter Check

```rust
pub struct TagFilterCheck;

#[async_trait]
impl PreCallCheck for TagFilterCheck {
    async fn check(&self, deployment: &DeploymentConfig, request: &CompletionRequest) -> CheckResult {
        let model_info = &deployment.model_info;

        // If request has no tags, skip tag filtering (allow through)
        let request_tags = match &request.tags {
            Some(tags) if !tags.is_empty() => tags,
            _ => return CheckResult::Pass,
        };

        // Check allowed tags: request must have at least one tag in allowed list
        if let Some(ref allowed) = model_info.allowed_tags {
            if !allowed.is_empty() && !allowed.iter().any(|t| request_tags.contains(t)) {
                return CheckResult::Fail {
                    reason: "Request tags not in allowed tags".to_string(),
                };
            }
        }

        // Check blocked tags: request must not have any tag in blocked list
        if let Some(ref blocked) = model_info.blocked_tags {
            if !blocked.is_empty() && blocked.iter().any(|t| request_tags.contains(t)) {
                return CheckResult::Fail {
                    reason: "Request tags match blocked tags".to_string(),
                };
            }
        }

        CheckResult::Pass
    }
}
```

**Note:** Requests with no tags pass through tag filtering. This matches LiteLLM's behavior where tags are optional.

### 4. Health Check

```rust
pub struct HealthCheck {
    client: reqwest::Client,
    check_interval: Duration,  // Default: 30 seconds
    health_cache: Arc<RwLock<HashMap<String, HealthState>>>,  // keyed by deployment_id
}

pub struct HealthState {
    pub last_check: i64,      // Unix timestamp
    pub is_healthy: bool,
}

impl HealthCheck {
    async fn check_health(&self, deployment_id: &str, api_base: Option<&str>) -> bool {
        // Check cache first
        {
            let cache = self.health_cache.read().unwrap_or_else(|e| e.into_inner());
            if let Some(state) = cache.get(deployment_id) {
                let elapsed = Utc::now().timestamp() - state.last_check;
                if elapsed < self.check_interval.as_secs() as i64 {
                    return state.is_healthy;
                }
            }
        }

        // Perform health check
        let api_base = match api_base {
            Some(base) if !base.is_empty() => base,
            _ => {
                self.update_cache(deployment_id, false);
                return false;
            }
        };
        let health_url = format!("{}/health", api_base.trim_end_matches('/'));
        let is_healthy = match self.client.get(&health_url).timeout(Duration::from_secs(5)).send().await {
            Ok(resp) => {
                // 2xx = healthy, 404 = no health endpoint (treat as healthy), 5xx = unhealthy
                resp.status().is_success() || resp.status() == reqwest::StatusCode::NOT_FOUND
            }
            Err(_) => false,  // Connection failure = unhealthy
        };

        self.update_cache(deployment_id, is_healthy);
        is_healthy
    }

    fn update_cache(&self, deployment_id: &str, is_healthy: bool) {
        let mut cache = self.health_cache.write().unwrap_or_else(|e| e.into_inner());
        cache.insert(deployment_id.to_string(), HealthState {
            last_check: Utc::now().timestamp(),
            is_healthy,
        });
    }
}

#[async_trait]
impl PreCallCheck for HealthCheck {
    async fn check(&self, deployment: &DeploymentConfig, request: &CompletionRequest) -> CheckResult {
        let deployment_id = deployment.deployment_id.as_deref().unwrap_or("unknown");
        let is_healthy = self.check_health(
            deployment_id,
            deployment.litellm_params.api_base.as_deref(),
        ).await;

        if is_healthy {
            CheckResult::Pass
        } else {
            CheckResult::Fail {
                reason: "Deployment unhealthy".to_string(),
            }
        }
    }
}
```

**Note:** HealthCheck maintains its own `Arc<RwLock<HashMap<String, HealthState>>>` cache, keyed by deployment_id. The Router creates one `HealthCheck` instance and shares it across all requests via `Arc`.

### 5. Integration with Router

```rust
impl Router {
    pub async fn get_available_deployment(&self, model_group: &str, request: &CompletionRequest) -> Option<usize> {
        // Note: self.providers is the existing field (HashMap<String, Vec<ProviderWithState>>)
        // deployment_groups is a NEW field added by this RFC
        let deployments = self.deployment_groups.get(model_group)?;

        // Filter by pre-call checks (async)
        let mut valid_indices = Vec::new();
        for (i, d) in deployments.iter().enumerate() {
            let mut all_pass = true;
            for check in &self.pre_call_checks {  // NEW field added by this RFC
                if let CheckResult::Fail { reason } = check.check(d, request).await {
                    debug!("Deployment {} failed pre-call check: {}", i, reason);
                    all_pass = false;
                    break;
                }
            }
            if all_pass {
                valid_indices.push(i);
            }
        }

        if valid_indices.is_empty() {
            return None;
        }

        // Apply existing routing strategy to valid deployments only
        // Reuse existing Router::route() logic but restrict to valid_indices
        self.route_to_valid(&valid_indices, model_group)
    }

    fn route_to_valid(&self, valid_indices: &[usize], model_group: &str) -> Option<usize> {
        // Map valid_indices to deployment indices and apply routing strategy
        // This filters the strategy's selection to only valid deployments
        let strategy = &self.routing_strategy;
        match strategy {
            RoutingStrategy::SimpleShuffle => {
                // Weighted random among valid deployments
                self.weighted_random(valid_indices, model_group)
            }
            RoutingStrategy::RoundRobin => {
                // Cycle through valid deployments
                self.round_robin(valid_indices, model_group)
            }
            // ... other strategies
        }
    }
}
```

### 6. Configuration

```yaml
router:
  enable_pre_call_checks: true
  pre_call_checks:
    context_window:
      enabled: true
    tag_filter:
      enabled: true
    health_check:
      enabled: true
      interval_seconds: 30
      timeout_seconds: 5
```

## Dependencies

- RFC-0927: RouterConfig extension
- RFC-0928: Deployment configuration schema
- `tiktoken-rs` crate: Add to Cargo.toml for token estimation. Note: tiktoken is OpenAI-specific (cl100k_base). For non-OpenAI models, the fallback to character/4 approximation will be used.

## Test Plan

1. Context window check filters deployments with insufficient tokens
2. Tag filter check passes when tags match
3. Tag filter check fails when tags blocked
4. Health check marks unhealthy deployments
5. Multiple checks all must pass
6. Router only routes to deployments passing all checks
7. Configuration enables/disables checks
8. Token estimation is accurate
