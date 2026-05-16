# Mission: 0936-a — Pre-call Checks

## Status

Open

## RFC

RFC-0936 (Economics): Pre-call Checks

## Dependencies

- None (standalone)

## Context

RFC-0936 specifies `PreCallCheck` trait for filtering deployments before routing. Checks include context window limits, tag filtering, and health checks. This mission implements the trait and integrates it into the router.

**New types required** (must be added to config.rs per RFC-0936 §0):
- `CompletionRequest` struct (messages, max_tokens, tags, model)
- New fields on `ModelInfo`: max_input_tokens, max_output_tokens, allowed_tags, blocked_tags
- New fields on `DeploymentConfig`: last_health_check, is_healthy
- New field on `Router`: pre_call_checks (Vec<Box<dyn PreCallCheck>>)

**Existing types to reuse:**
- `Message` struct — already exists in `shared_types.rs` and `types.rs`
- `supports_embeddings` — already exists on `ModelInfo` in `config.rs`
- `model_groups()` — already exists as a method on `Router` in `router.rs`

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

- [ ] `get_available_deployment()` filters by pre-call checks
- [ ] `route_to_valid()` applies routing strategy to valid deployments only
- [ ] Async check execution

### DeploymentConfig Updates

- [ ] Add `last_health_check` field (i64 timestamp)
- [ ] Add `is_healthy` field (bool)

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
