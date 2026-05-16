# Mission: 0933-a — Rate Limiting Wiring

## Status

Open

## RFC

RFC-0933 (Economics): Rate Limiting Integration

## Dependencies

- Mission-0932-a: Gateway Auth Wiring (provides KeyContext)

## Context

The existing `RateLimiterStore` in `key_rate_limiter.rs` implements per-key RPM/TPM rate limiting via TokenBucket. But `proxy.rs` doesn't enforce rate limits. This mission wires the existing rate limiter into the proxy.

## Acceptance Criteria

### Core Wiring

- [ ] Refactor `check_rate_limits()` into `check_rpm_limit()` and `check_tpm_limit()` (avoid double RPM consumption)
- [ ] Wire `check_rpm_limit()` into proxy pre-request path
- [ ] Wire `check_tpm_limit()` into proxy post-request path
- [ ] Add rate limit headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`
- [ ] Return 429 with `retry_after` when RPM limit exceeded
- [ ] Use `KeyError::RateLimited { retry_after }` (not RpmExceeded/TpmExceeded)

### Stoolap Persistence

- [ ] Create `rate_limit_state` table: `(key_id TEXT, bucket_type TEXT, tokens_remaining INTEGER, last_refill_ts INTEGER, PRIMARY KEY (key_id, bucket_type))`
- [ ] Flush rate limit state to stoolap every 60 seconds
- [ ] On graceful shutdown, flush immediately
- [ ] On startup, reload from stoolap and advance refill based on elapsed time
- [ ] Use wall-clock timestamp (i64 Unix seconds), NOT Instant (process-relative)

### Tests

- [ ] Request within RPM limit → 200
- [ ] Request exceeding RPM limit → 429
- [ ] Rate limit headers present in response
- [ ] Multiple keys have independent limits
- [ ] Rate limit resets after window
- [ ] Stoolap persistence survives restart

## Key Files

- `crates/quota-router-core/src/proxy.rs` — main request handler
- `crates/quota-router-core/src/key_rate_limiter.rs` — TokenBucket and RateLimiterStore
- `crates/quota-router-core/src/middleware.rs` — check_rate_limits()

## Notes

The existing `RateLimiterStore` wraps `DashMap<String, (TokenBucket, TokenBucket)>` (RPM + TPM pair per key). The `check_rate_limits()` method on `KeyMiddleware` already validates against the store. This mission is about wiring it into the proxy.
