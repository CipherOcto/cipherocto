# Mission: 0936-a — Pre-call Checks

## Status

Open

## RFC

RFC-0936 (Economics): Pre-call Checks

## Dependencies

- Mission-0928-a: Deployment Config Schema (Archived — types available in config.rs)
- Mission-0927-a: RouterConfig Extension (archived — types merged into Mission-0928-a)

## Context

RFC-0936 specifies `PreCallCheck` trait for filtering deployments before routing. Checks include context window limits, tag filtering, and health checks. This mission implements the trait and integrates it into the router.

**New types required** (must be added to config.rs per RFC-0936 §0):
- `CompletionRequest` struct (messages, max_tokens: Option<usize>, tags, model)
- New fields on `ModelInfo`: max_input_tokens, max_output_tokens, allowed_tags, blocked_tags
- Health state (last_health_check, is_healthy) lives on `ProviderWithState`, NOT on `DeploymentConfig`. `DeploymentConfig` is the config spec; `ProviderWithState` is the runtime state.
- New field on `Router`: pre_call_checks (Vec<Box<dyn PreCallCheck>>)

**Existing types to reuse:**
- `Message` struct — already exists in `shared_types.rs` and `types.rs` (only has `role: String` and `content: String` — same as RFC-0936's definition)
- `supports_embeddings` — already exists on `ModelInfo` in `config.rs`
- `Router.providers` type is `HashMap<String, Vec<ProviderWithState>>` — groups providers with health state, not raw DeploymentConfigs.
- `deployment_groups()` — use this method name (not `model_groups()`) to match the `deployment_groups` field name on Router.

## Acceptance Criteria

### Trait

- [ ] `PreCallCheck` async trait with `check(&self, deployment, request) -> CheckResult`
- [ ] `CheckResult` enum: Pass, Fail { reason }

### Implementations

- [ ] `ContextWindowCheck` — checks max_input_tokens, max_output_tokens
- [ ] `TagFilterCheck` — checks allowed_tags, blocked_tags
- [ ] `HealthCheck` — checks deployment health via HTTP

### Token Estimation

- [ ] `estimate_tokens()` using tiktoken-rs crate
- [ ] Fallback to character/4 approximation
- [ ] Cache tokenizer instances per model

### Router Integration

- [ ] `get_available_deployment(&self)` filters by pre-call checks. Takes `&self` (immutable) — round-robin index update uses interior mutability via `RefCell<HashMap<String, usize>>` or atomic operations.
- [ ] `route_to_valid()` does not exist — implement as a new method. The actual routing method in router.rs is `Self::simple_shuffle_impl()`.
- [ ] Async check execution

### DeploymentConfig Updates

- [ ] `DeploymentConfig.model_info` is `Option<ModelInfo>`. Pre-call checks must handle `None` gracefully — skip context window check if `model_info` is not available.

### Tests

- [ ] Context window check filters deployments
- [ ] Tag filter check passes/blocks correctly
- [ ] Health check marks unhealthy deployments
- [ ] Router only routes to deployments passing all checks
- [ ] Requests with no tags pass tag filtering

## Key Files

- `crates/quota-router-core/src/pre_call_checks.rs` — new file (trait + implementations)
- `crates/quota-router-core/src/router.rs` — get_available_deployment()
- `crates/quota-router-core/src/config.rs` — DeploymentConfig (add fields)

## Notes

This is a new module. The trait should be async. The `ContextWindowCheck` needs tiktoken-rs crate for accurate token counting. The `HealthCheck` needs HTTP client for health endpoint checks.
