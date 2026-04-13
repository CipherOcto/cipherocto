# Mission: RFC-0903 Phase 4 — TokenBucket Rate Limiter + Team Budget Enforcement

## Status

Open

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Summary

Upgrade the current simple counter-based rate limiter to the RFC-0903 TokenBucket algorithm with DashMap storage, and implement team budget enforcement with `record_spend_with_team()`. The current `KeyRateLimiter` uses simple counters with no refill mechanism; RFC-0903 requires token bucket with continuous refill and stale bucket eviction.

## Motivation

Current implementation gaps:
1. **TokenBucket not implemented:** RFC-0903 specifies TokenBucket algorithm with per-minute refill rates; current implementation uses simple 60-second window counters
2. **DashMap not used:** Current uses `HashMap` + `RwLock`; RFC-0903 requires `DashMap` for concurrent access
3. **Stale eviction missing:** No `cleanup_stale_buckets()` for memory management
4. **Team budget enforcement:** `record_spend_with_team()` partially specified but not implemented

## Dependencies

- Mission: 0903-a-ledger-based-budget-enforcement (depends on ledger schema and SpendEvent)

## Acceptance Criteria

- [ ] **TokenBucket struct:** Implement per RFC-0903:
  ```rust
  pub struct TokenBucket {
      capacity: u64,
      tokens: u64,
      refill_rate_per_minute: u64,
      last_refill: Instant,     // Monotonic time - immune to clock adjustments
      last_access: Instant,    // For cleanup tracking
  }
  ```
  - `new(capacity: u32, refill_per_minute: u32)` — initialize to capacity
  - `try_consume(tokens: u32) -> bool` — try to consume tokens, refill first
  - `retry_after() -> u64` — calculate seconds until next token available
  - `is_stale(now_ms: u64, max_idle_ms: u64) -> bool` — check if bucket is idle
  - Uses integer arithmetic only (deterministic)
- [ ] **RateLimiterStore with DashMap:** Replace `KeyRateLimiter` with:
  ```rust
  pub struct RateLimiterStore {
      buckets: DashMap<Uuid, (TokenBucket, TokenBucket)>, // (RPM, TPM)
  }
  ```
  - `check_rate_limit(key: &ApiKey, tokens: u32) -> Result<(), KeyError>`
  - `invalidate(key_id: &Uuid)` — remove rate limiter for key
  - `cleanup_stale_buckets(max_idle_ms: u64, max_size: usize)` — evict stale buckets
- [ ] **cleanup_stale_buckets():** Periodic cleanup worker:
  - Remove buckets idle > max_idle_ms (default 10 minutes)
  - Enforce max_size cap by removing oldest entries
  - Call periodically (e.g., every 5 minutes)
- [ ] **record_spend_with_team() — existing implementation:** Ensure already-implemented in 0903-a is wired correctly with team lock ordering:
  - Lock team row BEFORE key row (deadlock prevention)
  - Verify both key_budget and team_budget not exceeded
  - Insert into spend_ledger with team_id
- [ ] **Rate Limiting Determinism disclaimer:** Document that rate limiting is NOT deterministic across router nodes and MUST NOT influence accounting logic

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/key_rate_limiter.rs` | Replace with TokenBucket + DashMap implementation |
| `crates/quota-router-core/src/storage.rs` | Verify record_spend_with_team lock ordering |
| `crates/quota-router-core/src/keys/mod.rs` | Add cleanup helper if needed |

## Complexity

Medium — algorithmic change from simple counter to token bucket with memory management

## Reference

- RFC-0903 §Rate Limiting Algorithm (lines 498-577)
- RFC-0903 §Rate Limiter Storage (lines 579-664)
- RFC-0903 §Rate Limiting Determinism (lines 1672-1680)
- RFC-0903 §Rate Limiter Memory Management (lines 2165-2186)
- RFC-0903 §Lock Ordering Invariant (lines 1700-1711)
