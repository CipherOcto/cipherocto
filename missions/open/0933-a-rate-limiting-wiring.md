# Mission: 0933-a — Rate Limiting Wiring

## Status

Open

## RFC

RFC-0933 (Economics): Rate Limiting Integration

## Dependencies

- Mission-0932-a: Gateway Auth Wiring (provides ApiKey context)

**Note:** This mission requires stoolap for persistence (rate_limit_state table). Mission-0914-a: Stoolap Persistence (Open — provides rate_limit_state table). Use in-memory storage with periodic flush as interim solution if stoolap is not available.

## Context

The existing `RateLimiterStore` in `key_rate_limiter.rs` implements per-key RPM/TPM rate limiting via TokenBucket. But `proxy.rs` doesn't enforce rate limits. This mission wires the existing rate limiter into the proxy.

## Acceptance Criteria

### Core Wiring

- [ ] Refactor `check_rate_limits()` into `check_rpm_only()` and `check_tpm_only()`. The current unified function cannot be used at both pre-request and post-request points because calling it twice would consume 2 RPM tokens instead of 1.
- [ ] `check_rpm_only()` and `check_tpm_only()` are separate methods that each check only their respective counter. No double-counting: RPM counts requests, TPM counts tokens.
- [ ] Wire `check_rpm_only()` into proxy pre-request path
- [ ] Wire `check_tpm_only()` into proxy post-request path
- [ ] Add rate limit headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`
- [ ] Return 429 with `retry_after` when RPM limit exceeded
- [ ] 429 responses MUST include Retry-After HTTP header (in addition to JSON body retry_after field). Value is seconds until the rate limit window resets.
- [ ] Use `KeyError::RateLimited { retry_after }` (not RpmExceeded/TpmExceeded)

### Stoolap Persistence

- [ ] Create `rate_limit_state` table: `(key_id TEXT, bucket_type TEXT, tokens_remaining INTEGER, last_refill_ts INTEGER, PRIMARY KEY (key_id, bucket_type))`
- [ ] Flush rate limit state to stoolap every 60 seconds
- [ ] On graceful shutdown, flush immediately
- [ ] On startup, reload from stoolap and advance refill based on elapsed time
- [ ] Use wall-clock timestamp (i64 Unix seconds), NOT Instant (process-relative)

**Timestamps:** Use `SystemTime::now().duration_since(UNIX_EPOCH).as_secs() as i64` for all persistent timestamps. TokenBucket's `last_refill` remains `Instant` (monotonic clock for refill timing only). The `rate_limit_state` table stores i64 Unix timestamps.
- [ ] On reload, if `last_refill_ts` is in the future (clock drift), clamp to `now` and log warning
- [ ] `flush_interval_seconds` should be configurable (default 60)

**Config:** Add `flush_interval_seconds` to RouterSettings (default: 60, type: u64). Controls how often in-memory counters are flushed to stoolap.

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

**TPM flagged semantics:** When TPM limit is exceeded, the key is NOT immediately blocked. Instead, the request is rejected with 429 and the key's TPM counter is flagged. Subsequent requests within the same window are rejected immediately without re-checking.
