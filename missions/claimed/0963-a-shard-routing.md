# Mission 0963-a: Resource Shard Routing (RFC-0963)

## Status

Closed (2026-08-11) — Claimed (2026-07-23) — LANDED.

## RFC

- RFC-0963 (Economics): Resource Shard Routing

**BLUEPRINT gate:** RFC-0963 reached Accepted on RFC-0960 v2.0 promotion date. Mission is CLAIMABLE.

## Summary

Implement shard routing for resource-scoped WAL segments. `shard_for_segment(wal_segment_id, num_shards)` returns a deterministic shard index based on BLAKE3 hash of the segment id modulo `num_shards`. `num_shards_for(n)` computes the cluster size: `clamp(ceil(sqrt(N)), 4, 256)`. `ShardMap` wraps the configuration into a routable structure.

## Depends on (RFC + upstream missions)

| Dependency                                              | Status                            | Required?                       |
| ------------------------------------------------------- | --------------------------------- | ------------------------------- |
|                                                         | RFC-0960 (Grand Design)           | Accepted                        | YES — shard routing context                     |
|                                                         | RFC-0963 (Resource Shard Routing) | Accepted                        | YES — wire format                               |
|                                                         | RFC-0962 (ExecutionEnvelope)      | Accepted                        | YES — `MultiEnvelope` cross-shard (RFC-0962 §7) |
| RFC-0862 (Stoolap Sync Layer)                           | Accepted v1.2.0                   | YES — segment hash format       |
| RFC-0959 (Ask Settlement)                               | Accepted v1.0                     | YES — asks routed per shard     |
| Mission `missions/claimed/0962-a-execution-envelope.md` | Claimed (2026-07-23)              | YES — `MultiEnvelope` available |

## Type Coverage

Per RFC-0963 §1 + §7, the following types are implemented in this mission:

| Type                                                           | Implemented By |
| -------------------------------------------------------------- | -------------- |
| `num_shards_for(n: usize) -> u32`                              | This mission   |
| `shard_for_segment(&[u8; 32], u32) -> Result<u32, ShardError>` | This mission   |
| `ShardId`                                                      | This mission   |
| `ShardMap`                                                     | This mission   |
| `ShardError::ZeroShards`                                       | This mission   |

## In Scope

1. **`crates/quota-router-sm-engine/src/shard.rs`** — new module.
2. **`num_shards_for(n)`** — sqrt formula with `clamp(4, 256)`.
3. **`shard_for_segment(...)`** — BLAKE3-based deterministic routing.
4. **`ShardMap`** — cluster config + route method.
5. **10 unit tests** — clamp, sqrt, determinism, range, distribution, error.

## Out of Scope (this mission only)

- `events_shard_{N}` catalog schema (RFC-0963 §7) — separate migration mission.
- `shard_migration_log` (RFC-0963 §7) — separate migration mission.
- `shard_registry` table (RFC-0963 §7) — separate migration mission.
- Drain + refill re-shard logic (RFC-0963 §4.1) — re-shard operation mission.
- Multi-shard `MultiEnvelope` cross-shard atomicity (RFC-0962 §7) — wired via W6 envelope already.
- 4-shard cluster integration test (plan §4 W7 exit) — needs `W2` + `W6` complete.

## Implementation Guide

**File:** `crates/quota-router-sm-engine/src/shard.rs`

**Routing algorithm:**

```rust
let hash = blake3::hash(wal_segment_id);
let n = u64::from_le_bytes(hash[..8]);
let shard = (n % num_shards) as u32;
```

**`num_shards_for(n)`:**

```rust
let sqrt = (n as f64).sqrt().ceil() as u32;
sqrt.clamp(4, 256)
```

## Acceptance Criteria

- [ ] **AC-1:** `crates/quota-router-sm-engine/src/shard.rs` exists
- [ ] **AC-2:** `cargo test -p quota-router-sm-engine --lib shard` passes (10 tests)
- [ ] **AC-3:** `num_shards_for(0..=16) == 4` (clamped to min)
- [ ] **AC-4:** `num_shards_for(100_000) == 256` (clamped to max)
- [ ] **AC-5:** `num_shards_for(100) == 10` (sqrt formula)
- [ ] **AC-6:** `shard_for_segment(s, n)` deterministic across calls
- [ ] **AC-7:** `shard_for_segment(s, 0)` returns `Err(ZeroShards)`
- [ ] **AC-8:** Distribution test: 1000 random segments cover ≥ 14/16 shards
- [ ] **AC-9:** `ShardMap::route` matches `shard_for_segment` directly

## Risks (this mission)

| Risk                                              | Mitigation                                                    |
| ------------------------------------------------- | ------------------------------------------------------------- |
| BLAKE3 hash modulo non-power-of-2 introduces bias | Acceptable: bias is small; WAL segments are hashed uniformly  |
| `num_shards = 0` panic                            | `shard_for_segment` returns `Err(ZeroShards)`                 |
| Cluster size ≠ `num_shards`                       | `ShardMap::for_cluster` derives shard count from cluster size |

## Notes

### Wire alignment

- RFC-0963 §1a: `shard_for_segment(wal_segment_id, num_shards)` formula. We use `BLAKE3(segment_id) % num_shards`.
- RFC-0963 §1b: `num_shards = clamp(ceil(sqrt(N)), 4, 256)`. Implemented verbatim.
- Cross-shard writes use `MultiEnvelope` (RFC-0962 §7). Available in W6.

### Future work

- `events_shard_{N}` per-shard event store tables.
- `shard_migration_log` for re-shard history.
- `shard_registry` cluster-config table.
- 4-shard cluster integration test (deferred to W2 + W6 completion).

---

**Submission Date:** 2026-07-23
**Last Updated:** 2026-08-11
**Version:** 1.0 (Claimed)

## Version History

| Version | Date | Change
| --- -- ----- -- -- -- -- -- ---
| v0.1 | 2026-07-23 | Filed | CLAIMABLE. RFC-0963 reached Accepted on RFC-0960 v2.0 promotion date. Mission is CLAIMABLE.
| v0.2 | 2026-08-11 | CLOSED | (Band A 2026-08-11). Substrate shipped in prior sessions (commit d3fd7e27) | verified post-landing (4ba6aef1 fix 3 pre-existing clippy warnings). 11 shard-specific tests pass (num_shards_clamped_to_min/_to_max | sqrt_formula coverage >= 14/16 shards). 79/79 sm-engine lib tests pass. Future work | (`events_shard_{N}` , `shard_migration_log` , `shard_registry` , 4-shard cluster integration test) deferred to W2+ W6 completion.
