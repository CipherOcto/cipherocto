# Mission: RFC-0925 — Latency-Based Routing Extensions

## Status

COMPLETE — all acceptance criteria met (2026-05-12)

## RFC

RFC-0925 (Economics): Latency-Based Routing Extensions

## Summary

Implement latency-based routing with TTFT (time-to-first-token) tracking, cooldown mechanisms for degraded deployments, and threshold-based alerting. Extends RFC-0917 `LatencyBased` routing strategy with production-grade latency observability.

## Dependencies

- RFC-0917: Dual-Mode Query Router (LatencyTracker, LatencyBased routing)
- RFC-0924: Provider Metrics Bucket Tracking (for bucketed RPM/TPM) — can be implemented in parallel

## Scope

### Data Structures

```rust
enum DeploymentState {
    Healthy,
    Cooldown,  // TTL-based, no Degraded/Recovering state
}

struct LatencyConfig {
    lowest_latency_buffer: f32,  // Default: 0.0 (litellm default)
    max_latency_list_size: usize,  // Default: 10
    timeout_penalty_us: u64,  // Default: 1_000_000_000 (1000 seconds)
    cooldown_duration_secs: u32,  // Default: 5
    failure_threshold_percent: f32,  // Default: 0.5 (50%)
    failure_threshold_min_requests: u32,  // Default: 5
    // NOTE: ttft_weight removed - TTFT scoring uses selection mode (TTFT only for streaming),
    // not weighted blend. Best provider methods take is_streaming: bool, not ttft_weight.
}

struct CooldownTracker {
    state: DeploymentState,
    total_requests: u32,
    failed_requests: u32,
    cooldown_end_time: Option<Instant>,
    penalty_latencies: Vec<u64>,
}
```

### Methods to Implement

| Method | Signature |
|--------|-----------|
| record_success | `pub fn record_success(&mut self)` |
| record_timeout_penalty | `pub fn record_timeout_penalty(&mut self, penalty_us: u64)` — appends penalty_us to penalty_latencies, increments counters |
| record_429 | `pub fn record_429(&mut self, cooldown_duration_secs: u32, is_single_deployment: bool) -> bool` |
| record_error | `pub fn record_error(&mut self)` |
| should_enter_cooldown | `pub fn should_enter_cooldown(&self, failure_threshold_percent: f32, failure_threshold_min_requests: u32, is_single_deployment: bool) -> bool` |
| reset_minute_window | `pub fn reset_minute_window(&mut self)` |
| enter_cooldown | `pub fn enter_cooldown(&mut self, duration_secs: u32)` |
| is_cooldown_expired | `pub fn is_cooldown_expired(&self) -> bool` |
| is_available | `pub fn is_available(&self) -> bool` |

### TTFT-Aware Scoring

```rust
impl LatencyTracker {
    // Selection mode, NOT weighted blend (matches litellm lowest_latency.py:501-505)
    pub fn best_provider_with_ttft(&self, is_streaming: bool, lowest_latency_buffer: f32) -> Option<&str>
    pub fn best_provider_among(&self, available_names: std::collections::HashSet<&str>, is_streaming: bool) -> Option<&str>
}
```

**TTFT scoring:** Use TTFT only for streaming with TTFT data available, otherwise use latency. NOT a weighted blend.

### Cooldown State Machine

```
Healthy ──(429 OR failure_rate > 50% AND >=5 requests)──> Cooldown
    │
    └───(retryable error: 408, 409, 429, 500+)───────────> Cooldown
                                               │
Cooldown ──(cooldown TTL elapsed)──> Healthy
```

**Transitions:**
1. `Healthy → Cooldown`: 429 response OR failure rate > threshold (50% + 5 min requests) OR retryable error (408, 409, 429, 500+)
2. `Cooldown → Healthy`: Cooldown TTL expired (automatic, no health check)

**Note:** 401 and 404 are cooldown-eligible via failure-rate path but are NOT retryable per litellm's `_should_retry()`.

**Single-deployment exemption:** Single-deployment model groups are exempt from 429 and failure-rate cooldown (they need the traffic).

### Key Implementation Details

1. **Cooldown callback:** NOT implemented in v1. litellm calls `router_cooldown_event_callback` via `asyncio.create_task()` on every cooldown trigger. quota-router v1 has no callback — cooldown is internal-only.

2. **TTL-based cooldown:** No Degraded or Recovering states. When TTL expires, deployment automatically becomes available again.

3. **record_429 returns bool:** `true` if cooldown was entered, `false` if exempted (single-deployment model group).

4. **should_enter_cooldown checks:** state == Healthy, total_requests >= min_requests, failure_rate > threshold, NOT single-deployment.

## Acceptance Criteria

- [x] `DeploymentState { Healthy, Cooldown }` enum (no Degraded/Recovering)
- [x] `LatencyConfig` struct with all fields (lowest_latency_buffer, max_latency_list_size, timeout_penalty_us, cooldown_duration_secs, failure_threshold_percent, failure_threshold_min_requests)
- [x] `CooldownTracker` with state, total_requests, failed_requests, cooldown_end_time, penalty_latencies
- [x] `record_429(cooldown_duration_secs, is_single_deployment) -> bool` returns true if cooldown entered
- [x] `should_enter_cooldown(state==Healthy, failure_rate > threshold, NOT single-deployment)`
- [x] `is_cooldown_expired()` returns true when `Instant::now() >= cooldown_end_time`
- [x] `best_provider_with_ttft` uses TTFT only for streaming (selection mode, NOT weighted blend)
- [x] `best_provider_among` filters by available provider names
- [x] `update_latency_state(success: bool, latency_us: u64, config: &LatencyConfig, is_single_deployment: bool)` records success/error and checks cooldown expiry
- [x] Cooldown callback NOT implemented (v1 internal-only)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [x] `cargo test --lib` passes

## Files Modified

| File | Change |
|------|--------|
| `crates/quota-router-core/src/router.rs` | Add `DeploymentState`, `LatencyConfig`, `CooldownTracker`, TTFT-aware `LatencyTracker`, `update_latency_state()` |

## Test Results

- 13 new unit tests for cooldown/latency tracking (DeploymentState, LatencyConfig, CooldownTracker, LatencyTracker TTFT methods, update_latency_state)
- All 188 tests pass

## Notes

- **Retryable error codes:** 408, 409, 429, 500+ (401/404 are cooldown-eligible but NOT retryable)
- **Timeout penalty:** 1_000_000_000µs (1000 seconds) per litellm
- **Cooldown duration default:** 5 seconds (litellm DEFAULT_COOLDOWN_TIME_SECONDS)
- **Failure threshold:** 50% failure rate + 5 minimum requests (litellm DEFAULT_FAILURE_THRESHOLD_PERCENT=0.5, DEFAULT_FAILURE_THRESHOLD_MINIMUM_REQUESTS=5)
- **Bug fix (2026-05-12):** `penalty_latencies` are now cleared when cooldown expires (via `clear_penalty_latencies()` called in both `update_latency_state()` and `latency_based_with_cooldown_impl()`). Previously they persisted forever even after cooldown TTL expired.