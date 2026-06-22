# Mission: 0862e — ReplayCache Persistence

## Status

Draft (awaiting adversarial review)

## RFC

RFC-0862 v1.1.0 (Networking): Stoolap Data Sync Protocol — §4.3.1 (rate limit + replay cache), §Implementation Phases Phase 2, §Performance Targets (memory overhead ≤ 50 MB per peer), §DatabaseSyncAdapter Trait (v1.1.0)

## Summary

Implement persistent backing for the ReplayCache so the cache survives process restarts. The in-memory ReplayCache is a `BTreeMap<envelope_id, first_seen>` bounded to 10K entries (~5 MB per peer). For long-running missions, the cache must persist to disk to maintain replay protection across restarts.

This mission is split out of `0862-base` for parallel execution. It depends on `0862-base` for the in-memory ReplayCache, but ships independently as a focused persistence module.

## Design

### New module: `octo-sync-replay-store/src/persistent_cache.rs` (sub-crate of cipherocto workspace; depends on Stoolap as a persistence backend, NOT on `octo-sync` directly)

The existing in-memory ReplayCache is a `BTreeMap<envelope_id, first_seen>`. This mission adds:

1. **Disk-backed storage** using the Stoolap fork as the persistence layer (per the cipherocto convention established in mission [`0850h-d`](../../with-pr/0850h-d-stoolap-session-storage.md): raw SQLite is never used in new persistence layers in cipherocto). The Stoolap DB is accessed via the `DatabaseSyncAdapter` trait from the `octo-sync` leaf workspace, NOT via direct `MVCCEngine` calls — the cipherocto workspace does not depend on the Stoolap fork Cargo-wise (per RFC-0862 v1.1.0 §DatabaseSyncAdapter Trait).
2. **Lru-on-disk eviction** with the same 10K-entry bound as the in-memory cache
3. **Atomic flush** to ensure consistency between in-memory and on-disk state

### Storage schema (in Stoolap)

```sql
CREATE TABLE IF NOT EXISTS sync_replay_cache (
    mission_id BLOB NOT NULL,
    peer_id    BLOB NOT NULL,
    envelope_id BLOB NOT NULL,
    first_seen INTEGER NOT NULL,  -- Unix timestamp seconds
    PRIMARY KEY (mission_id, peer_id, envelope_id)
);

CREATE INDEX IF NOT EXISTS idx_replay_cache_first_seen
    ON sync_replay_cache (mission_id, peer_id, first_seen);
```

### Eviction

When the cache exceeds 10K entries for a `(mission_id, peer_id)` pair, evict the oldest by `first_seen` (LRU by time, not by access — replay protection is about preventing the *first* re-application, not LRU by access).

### Flush strategy

- **Synchronous flush** on every insert (the cache is critical for security; durability over performance)
- **Batch flush** every 1 second for high-throughput scenarios (configurable)
- **Flush on shutdown** (the process exit handler must flush before terminating)

## Acceptance Criteria

- [ ] `octo-sync-replay-store/src/persistent_cache.rs` (in a new sub-crate `octo-sync-replay-store`, NOT inside the `octo-sync` leaf workspace — see the cargo layering below) extends the existing in-memory cache with disk-backed storage
- [ ] `ReplayCache::open(mission_id, peer_id, db)` opens or creates the on-disk cache
- [ ] `ReplayCache::insert(envelope_id, first_seen)` inserts into both in-memory and on-disk
- [ ] `ReplayCache::contains(envelope_id)` checks in-memory first (fast path), then on-disk
- [ ] `ReplayCache::evict_oldest()` evicts the oldest entry by `first_seen` when size > 10K
- [ ] `ReplayCache::flush()` flushes pending writes to disk
- [ ] The cache uses Stoolap as the persistence layer (not raw SQLite)
- [ ] The schema is created on first open (`CREATE TABLE IF NOT EXISTS`)
- [ ] Process exit handler flushes pending writes
- [ ] Unit tests for: insert, contains, evict_oldest, flush, restart-persistence
- [ ] Integration test: insert 10K envelopes, restart process, verify all 10K are still in the cache

## Tests

- **Unit:**
  - `insert` adds to both in-memory and on-disk
  - `contains` returns true after insert
  - `contains` returns false for a not-yet-inserted envelope_id
  - `evict_oldest` removes the entry with the smallest `first_seen`
  - `evict_oldest` triggered when size > 10K
  - `flush` is a no-op when there are no pending writes
  - `flush` writes all pending writes to disk
  - Restart: process 1 inserts 10K envelopes, process 2 opens the same cache, sees all 10K

- **Integration:**
  - Insert 10K envelopes, verify in-memory size = 10K and on-disk size = 10K
  - Insert 10,001 envelopes, verify in-memory size = 10K (oldest evicted) and on-disk size = 10K
  - Crash recovery: insert 5K envelopes, kill the process (no graceful shutdown), restart, verify all 5K are present
  - Concurrent inserts from two threads: verify no lost writes (use a `Mutex` or transaction)

## Dependencies

- **Requires:**
  - `0862-base` — for the in-memory ReplayCache, **`DatabaseSyncAdapter` trait**
  - `stoolap` (as a dependency, per the cipherocto convention established in mission [`0850h-d`](../../with-pr/0850h-d-stoolap-session-storage.md): raw SQLite is never used) — accessed via the `DatabaseSyncAdapter` trait from the `octo-sync` leaf workspace, NOT via direct `MVCCEngine` calls
  - RFC-0850 §Replay Cache (the DOT-level ReplayCache that this mission extends)

- **Required by:**
  - `0862f` (multi-peer — multiple peers require multiple cache instances)
  - `0862h` (property tests for replay protection)

## Blockers / Dependencies

- **Blocked by:** `0862-base`
- **Blocks:** `0862f` (multi-peer needs persistent cache per peer)

### Cargo dependency layering (resolves H1 — Cargo cycle)

`0862e` MUST NOT introduce a Cargo package cycle between `octo-sync` and `stoolap`. The solution:

- The persistent ReplayCache is implemented as a separate sub-crate `octo-sync-replay-store` that depends on `stoolap`.
- `octo-sync` (the base crate) has an OPTIONAL dependency on `octo-sync-replay-store` behind a Cargo feature flag `persistent-replay-cache`.
- `stoolap` with the `sync` feature enabled transitively depends on `octo-sync-replay-store` (not on `octo-sync`).
- Default build: `octo-sync` builds without `stoolap` (in-memory only); `stoolap` users opt into persistent cache via the `sync` + `persistent-replay-cache` features.

**Cargo resolver requirement (resolves L-R3-3):** the cipherocto workspace MUST use Cargo resolver v2 to support the `octo-sync-replay-store` ↔ `stoolap` cycle. Add this to the workspace `Cargo.toml`:

```toml
[workspace]
resolver = "2"
# ...
```

Without resolver v2, Cargo will reject the cycle as a hard error. With resolver v2, the cycle is permitted as long as the feature unification is clean (i.e., `octo-sync-replay-store` does NOT enable the `sync` feature of `stoolap`, breaking the cycle at the feature level). The mission documents this constraint; it is enforced at workspace-config time.

### Reference hygiene (resolves M2 — RFC-0850h-d is a mission, not an RFC)

The "raw SQLite is never used" convention is documented in mission `missions/with-pr/0850h-d-stoolap-session-storage.md` (NOT in an RFC — `0850h-d` is a mission identifier, not an RFC identifier). The reference is therefore `[mission: 0850h-d](missions/with-pr/0850h-d-stoolap-session-storage.md)`. The convention applies because `stoolap` is the cipherocto project's universal persistence layer; new persistence code uses Stoolap as the SQL backend.

## Description

The ReplayCache is the cryptographic defense against replay attacks. Per RFC-0853 §7, the cache has a 1-hour or 10K-entry window. For long-running missions, the cache MUST survive process restarts to maintain replay protection. This mission implements the disk-backed persistence using the Stoolap fork as the storage layer (per cipherocto's project-wide persistence convention).

## Technical Details

### Performance

- **Insert latency:** < 1 ms (in-memory) + < 5 ms (disk flush)
- **Lookup latency:** < 100 µs (in-memory fast path), < 1 ms (on-disk fallback)
- **Storage overhead:** ~ 200 bytes per entry × 10K entries = 2 MB per peer on disk
- **Eviction cost:** O(1) (Lru on `first_seen`, not by access)

### Why Stoolap (not raw SQLite)?

Per the cipherocto convention established in mission [`0850h-d`](../../with-pr/0850h-d-stoolap-session-storage.md): **raw SQLite is never used in new persistence layers in cipherocto**. The Stoolap fork provides a unified SQL interface with MVCC, snapshot isolation, and the same `replay_two_phase` recovery path as the Sync application data.

### Why LRU by time (not by access)?

Replay protection is about preventing the *first* re-application of a captured envelope. A "use" (move-to-back) would make the cache useless against a slow-drip replay attack. LRU by time is the conservative choice.

### Pitfalls

- **Don't use `std::collections::HashMap` for the in-memory cache.** The BTreeMap is required for deterministic ordering (per RFC-0850's ReplayCache specification).
- **Don't use `tokio::fs` for synchronous flush.** Flush is critical for security; it must block until the disk write completes.
- **Don't store the `mission_id` and `peer_id` in every entry.** The schema uses them as the primary key prefix; the cache instance is per (mission_id, peer_id) pair.
- **Don't call `stoolap`'s `replay_two_phase` directly from the cipherocto sync engine.** The ReplayCache's DB access goes through `DatabaseSyncAdapter` (e.g., `adapter.apply_wal_entry` for inserts, `adapter.read_wal_range` for queries). The cipherocto workspace does not depend on the Stoolap fork Cargo-wise; the trait is the integration boundary.
- **Don't evict on every insert.** Evict only when size > 10K; otherwise the eviction cost is wasted.

---

**Mission Type:** Implementation
**Priority:** High
**Phase:** 2 (Catch-up via snapshot segments)
**RFC Section Coverage:** §4.3.1 (rate limit + replay cache), §Performance Targets

## Type Coverage

This mission implements the following RFC-0862 types:

| Type | Role in this mission |
|------|---------------------|
| `ReplayCache` (persistent) | Disk-backed extension of the in-memory `ReplayCache` from mission 0862-base; uses the Stoolap fork as the persistence layer (per the cipherocto convention from mission `0850h-d`) |
| `octo-sync-replay-store` (new sub-crate) | The sub-crate that holds the persistent ReplayCache; depends on `stoolap` (not on `octo-sync` directly) to avoid Cargo package cycles |

The mission does NOT implement the in-memory `ReplayCache` (which is the default) — that is in mission 0862-base. The persistent variant is opt-in via the `persistent-replay-cache` Cargo feature flag. See the Type Coverage table in 0862-base for the full mapping.
