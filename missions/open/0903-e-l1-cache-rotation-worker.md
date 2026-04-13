# Mission: RFC-0903 Phase 5 — L1 Key Cache + Key Rotation Worker

## Status

Open

## RFC

RFC-0903 (Economics): Virtual API Key System — Final v29

## Summary

Implement the L1 key cache (`KeyCache`) with TTL-based invalidation and the background key rotation worker for automatic key rotation. The current `cache.rs` is a stub with TODO comments. Key rotation worker is referenced but not implemented.

## Motivation

Current implementation gaps:
1. **L1 cache stub:** `CacheInvalidation` in `cache.rs` has no implementation
2. **Key rotation worker missing:** No background worker for automatic rotation on expiry
3. **Soft budget pre-check missing:** `check_budget_soft_limit()` for UX not implemented
4. **Cache invalidation not wired:** No calls to cache.invalidate() on key mutations

## Dependencies

- Mission: 0903-b-key-management-rest-api (depends on rotate_key implementation)

## Acceptance Criteria

- [ ] **KeyCache with LRU + TTL:** Implement per RFC-0903 §L1 Cache for Fast Lookups:
  ```rust
  pub struct KeyCache {
      cache: Arc<RwLock<LruCache<Vec<u8>, CacheEntry>>>,
  }

  struct CacheEntry {
      api_key: Arc<ApiKey>,
      cached_at: Instant,
  }
  ```
  - `CACHE_SIZE: usize = 10_000`
  - `CACHE_TTL_SECS: u64 = 30`
  - `get(key_hash: &[u8]) -> Option<Arc<ApiKey>>` — with TTL check
  - `put(key_hash: Vec<u8>, api_key: ApiKey)` — wraps ApiKey in Arc
  - `invalidate(key_hash: &[u8])` — remove entry
  - `clear()` — clear all entries
  - Uses Vec<u8> for cache key (binary, not hex)
- [ ] **validate_key_with_cache():** Implement cached validation flow:
  ```rust
  pub fn validate_key_with_cache(
      db: &Database,
      cache: &KeyCache,
      key: &str,
  ) -> Result<Arc<ApiKey>, KeyError>
  ```
  - Check cache first (TTL check)
  - On miss: lookup in DB, validate, add to cache
  - Returns Arc<ApiKey> to avoid cloning
- [ ] **Cache invalidation on mutations:** Wire cache.invalidate() calls:
  - `revoke_key()` — invalidate after DB update
  - `update_key()` — invalidate after DB update
  - `rotate_key()` — invalidate old key after DB update
- [ ] **check_budget_soft_limit():** Implement soft pre-flight budget check:
  ```rust
  pub fn check_budget_soft_limit(
      db: &Database,
      key_id: &Uuid,
      estimated_max_cost: u64,
  ) -> Result<(), KeyError>
  ```
  - Non-locking check (UX improvement, not authoritative)
  - Computes current from spend_ledger
  - Returns BudgetExceeded if current + estimated > budget
  - Authoritative check happens atomically in record_spend()
- [ ] **rotation_worker:** Implement background worker:
  ```rust
  pub async fn rotation_worker(db: &Database, cache: &Cache)
  ```
  - Runs every 5 minutes
  - Finds keys where `auto_rotate = 1 AND expires_at < now`
  - Calls rotate_key() for each
  - Logs failures but continues processing

## Key Files to Modify

| File | Change |
|------|--------|
| `crates/quota-router-core/src/cache.rs` | Full KeyCache implementation (currently stub) |
| `crates/quota-router-core/src/keys/mod.rs` | Add validate_key_with_cache, check_budget_soft_limit |
| `crates/quota-router-core/src/keys/models.rs` | Add CacheEntry struct |
| `crates/quota-router-cli/src/main.rs` | Wire up rotation_worker background task |

## Complexity

Medium — cache infrastructure + async worker

## Reference

- RFC-0903 §L1 Cache for Fast Lookups (lines 790-918)
- RFC-0903 §Cache Invalidation Rules (lines 2104-2112)
- RFC-0903 §Key Rotation Worker (lines 1156-1185)
- RFC-0903 §Soft budget pre-check (lines 361-394)
- RFC-0903 §Cache Revocation Rule (lines 1682-1698)
