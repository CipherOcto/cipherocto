# RFC-0933: Rate Limiting Integration

## Status: Accepted

## Summary

Wire the existing TokenBucket rate limiter (key_rate_limiter.rs) into the proxy request path, enabling per-key and per-user rate limiting for both modes.

## Motivation

quota-router has a TokenBucket rate limiter with capacity and refill_rate_per_minute. API keys have rpm_limit and tpm_limit fields. But the proxy doesn't enforce rate limits. This RFC specifies how to integrate rate limiting into the request path.

## Specification

### 1. Rate Limit Check Middleware

```rust
// proxy.rs - integrate rate limiting
// REFACTORING REQUIRED: The existing RateLimiterStore::check_rate_limit() consumes
// both RPM (1 token) and TPM (N tokens) in a single call. Splitting into separate
// RPM and TPM methods requires modifying RateLimiterStore internals:
// - Add check_rpm_only(&ApiKey) -> Result<(), KeyError>  (consumes 1 RPM token only)
// - Add check_tpm_only(&ApiKey, tokens: u32) -> Result<(), KeyError>  (consumes TPM tokens only)
// - Keep existing check_rate_limit() for backward compatibility
// Note: tokens is u32 (not u64) to match existing RateLimiterStore API

// Integration:
// let api_key = middleware.extract_and_validate(&request)?;
// middleware.check_rpm_limit(&api_key)?;  // RPM check only (pre-request)
// let response = next.run(request).await;
// let tokens = extract_token_count(&response);
// middleware.check_tpm_limit(&api_key, tokens)?;  // TPM check only (post-request)
```

**Implementation note:** The existing `RateLimiterStore` in `key_rate_limiter.rs` already implements TokenBucket per-key rate limiting. The RFC requires splitting `check_rate_limits()` into `check_rpm_limit()` and `check_tpm_limit()` to avoid double RPM consumption.

**TPM enforcement:** TPM limits ARE enforced. If TPM exceeded after response, the request still completes (can't retroactively fail), but the key is flagged and subsequent requests will be blocked until TPM bucket refills. Rate limit headers reflect RPM state only (TPM headers are best-effort since TPM is checked post-response).

### 2. Rate Limit Headers

Include in all responses:
```
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1715800000
```

### 3. Scope Hierarchy

Rate limits apply at multiple scopes:
1. **Per-key**: rpm_limit/tpm_limit on ApiKey (existing implementation)
2. **Per-user**: aggregated across all user's keys (NOT YET IMPLEMENTED)
3. **Per-team**: aggregated across all team's users (NOT YET IMPLEMENTED)
4. **Global**: system-wide limits (NOT YET IMPLEMENTED)

Priority: per-key > per-user > per-team > global

**Current implementation:** Only per-key rate limiting exists in `RateLimiterStore`. Per-user, per-team, and global scopes require aggregation logic that is not yet specified. These are deferred to a future RFC.

### 4. Persistence

**Refactored TokenBucket** (replaces existing `Instant`-based implementation):
```rust
pub struct TokenBucket {
    capacity: u64,
    tokens: u64,
    refill_rate_per_minute: u64,
    last_refill: i64,  // Unix timestamp (seconds), replaces std::time::Instant
    last_access: i64,  // Unix timestamp (seconds)
}
```

Rate limit state stored in stoolap:
- In-memory TokenBucket for fast path (refactored to use i64 timestamps)
- Periodic flush to stoolap every 60 seconds (configurable via `flush_interval_seconds`)
- On graceful shutdown, flush immediately
- On restart, reload from stoolap to restore bucket state

**Stoolap schema:**
```sql
CREATE TABLE rate_limit_state (
    key_id TEXT NOT NULL,
    bucket_type TEXT NOT NULL,  -- 'rpm' or 'tpm'
    tokens_remaining INTEGER NOT NULL,
    last_refill_ts INTEGER NOT NULL,  -- Unix timestamp (seconds)
    PRIMARY KEY (key_id, bucket_type)
);
```

**Reload logic:** On startup, read all rows. For each bucket, calculate elapsed time since `last_refill_ts` and advance refill accordingly. Use `std::time::SystemTime` for wall-clock timestamps (not `Instant`, which is process-relative and not serializable).

**Clock drift handling:** On reload, if `last_refill_ts` is in the future (clock drift from NTP adjustment), clamp it to `now` and log a warning. This prevents inflated token grants from clock adjustments.

### 5. Error Response

```json
{
  "error": {
    "message": "Rate limit exceeded",
    "type": "rate_limit_error",
    "code": "rpm_limit_exceeded",
    "retry_after": 30
  }
}
```

HTTP 429 with `Retry-After` header.

### 6. Configuration

```yaml
rate_limiting:
  enabled: true
  storage: stoolap  # per RFC-0914, stoolap-only
  flush_interval_seconds: 60
  default_rpm: 100  # matches existing unwrap_or(100) in key_rate_limiter.rs
  default_tpm: 1000  # matches existing unwrap_or(1000) in key_rate_limiter.rs
```

## Dependencies

- RFC-0903: Virtual API Key System (ApiKey struct and KeyStorage)
- RFC-0914: stoolap-only persistence

**Note:** RFC-0913 (stoolap pub/sub) is NOT required for this RFC. This RFC specifies local in-memory rate limiting with periodic stoolap flush. Distributed rate limiting via pub/sub is a future enhancement.

## Test Plan

1. Request within RPM limit → 200
2. Request exceeding RPM limit → 429 with retry_after
3. Rate limit headers present in response
4. Multiple keys have independent limits
5. Rate limit resets after window
6. stoolap persistence survives restart
7. [REMOVED — global limits are out of scope for this RFC. Per-user, per-team, and global scopes will be specified in a future RFC.]
