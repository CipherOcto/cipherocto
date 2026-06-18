# Mission: RFC-0924 — Provider Metrics Bucket Tracking

## Status

COMPLETE — all acceptance criteria met (2026-05-12)

## RFC

RFC-0924 (Economics): Provider Metrics Bucket Tracking

## Summary

Implement per-minute TPM/RPM bucket tracking per provider deployment for latency-based routing decisions. Tracks requests and tokens per minute with minute-granularity histograms, enabling RPM-aware routing and capacity planning.

## Dependencies

- RFC-0917: Dual-Mode Query Router (ProviderWithState, LatencyTracker)
- Mission: RFC-0917 Phase 2 — LatencyTracker Integration (for latency-based routing)

## Scope

### Data Structures

```rust
struct ProviderMetrics {
    buckets: HashMap<String, HashMap<String, BucketStats>>,
    ttl_seconds: u32,  // default: 60 (per litellm RoutingArgs.ttl)
    bucket_timestamps: HashMap<String, HashMap<String, Instant>>,
}

struct BucketStats {
    tpm: u64,  // tokens per minute
    rpm: u64,  // requests per minute
}
```

### Methods Implemented

| Method | Signature |
|--------|-----------|
| record | `pub fn record(&mut self, deployment_id: &str, tokens: u32)` |
| evict_old_buckets | `pub fn evict_old_buckets(&mut self)` |
| evict_old_buckets_for | `pub fn evict_old_buckets_for(&mut self, deployment_id: &str)` |
| rpm_at | `pub fn rpm_at(&self, deployment_id: &str, minute: &str) -> Option<u64>` |
| tpm_at | `pub fn tpm_at(&self, deployment_id: &str, minute: &str) -> Option<u64>` |
| current_rpm | `pub fn current_rpm(&self, deployment_id: &str) -> u64` |
| current_tpm | `pub fn current_tpm(&self, deployment_id: &str) -> u64` |
| can_accept_request | `pub fn can_accept_request(&self, deployment_id: &str, rpm_limit: u64, tpm_limit: u64, input_tokens: u64) -> bool` |
| rolling_avg_rpm | `pub fn rolling_avg_rpm(&self, deployment_id: &str, minutes: u32) -> Option<f32>` |
| rolling_avg_tpm | `pub fn rolling_avg_tpm(&self, deployment_id: &str, minutes: u32) -> Option<f32>` |

### Key Implementation Details

1. **Bucket key format:** `"HH-MM"` (hour-minute in local time), NOT full date. Matches litellm's `LowestTPMLoggingHandler` pattern (`datetime.now().strftime("%H-%M")`).

2. **TTL = 60 seconds** (not 60 minutes). This prevents cross-day HH-MM bucket collisions within the TTL window.

3. **Probabilistic eviction:** `evict_old_buckets_for()` called every 10th call per deployment (not on every call). This prevents O(n) global scans. `evict_old_buckets()` is for background/periodic cleanup.

4. **can_accept_request pattern:** Increment THEN check (litellm pattern: `(current_rpm + 1) <= rpm_limit && (current_tpm + input_tokens) <= tpm_limit`)

5. **current_minute_key():** Private helper that returns `"HH-MM"` string using local time. Used by `record()` to determine bucket key. Matches litellm `datetime.now().strftime("%H-%M")`.

## Acceptance Criteria

- [x] `ProviderMetrics` struct with `buckets`, `ttl_seconds`, `bucket_timestamps`
- [x] `BucketStats { tpm: u64, rpm: u64 }`
- [x] `record()` increments rpm and tpm, tracks timestamp, probabilistic eviction every 10th call
- [x] `can_accept_request()` follows litellm's increment-THEN-check pattern
- [x] `rolling_avg_rpm()` and `rolling_avg_tpm()` filter by timestamp within window
- [x] TTL is 60 seconds (not minutes)
- [x] Bucket key format is `"HH-MM"` (local time)
- [x] `current_minute_key()` returns `"HH-MM"` format using local time (matches litellm `datetime.now().strftime("%H-%M")`)
- [x] `evict_old_buckets()` iterates all deployments for background cleanup
- [x] `evict_old_buckets_for()` called probabilistically every 10th call per deployment
- [x] Integration point: standalone struct (not in ProviderWithState), documented in code comment
- [x] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [x] `cargo test --lib` passes

## Files Modified

| File | Change |
|------|--------|
| `crates/quota-router-core/src/router.rs` | Added `ProviderMetrics` struct, `BucketStats`, and 10 methods |

## Test Results

- 4 new unit tests added for ProviderMetrics
- All 167 tests pass

## Notes

- TTL eviction: litellm uses Redis TTL (`RoutingArgs.ttl=60`), our implementation uses probabilistic in-memory eviction as design decision
- Bucket key format: litellm's `LowestLatencyLoggingHandler` uses `YYYY-MM-DD-HH-MM`, but `LowestTPMLoggingHandler` uses `HH-MM`. We follow TPM/RPM handler (HH-MM)