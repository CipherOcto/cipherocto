# Mission: 0862-c15 — Phase 3 Performance Test Vector Fixture, TV-6 (RFC-0862)

## Status

**LANDED 2026-08-19**

## Summary

Author the perf-budget test vector fixture for **TV-6 only**
(`drain_throughput_1k_per_sec`) per RFC-0862 v1.3.0 §Performance
Targets. Sibling to `0862-c12` (TV-5), `0862-c13` (TV-8),
`0862-c14` (TV-7), all LANDED 2026-08-19. Last of RFC-0862 v1.3.0
Phase 3 perf-budget TVs. Separate fixture file per R17 M3 scope
discipline — each TV owns its own fixture file.

## Why TV-6 last (needs async harness)

- TV-6 substrate is async — `RaftLikeDrainCoordinator::submit_drain`
  via `RaftLikeWriterElection::acquire_writer` (both `async fn`).
- Requires `#[tokio::test]` + `RaftLikeDrainCoordinator` +
  `RaftLikeWriterElection` construction (per existing pattern in
  `cross_instance_drain_tv.rs`).
- Budget 1000 txn/s per shard = 1 ms per-op (per-iter latency
  interpretation); ci_slack_factor = 10x → 10 ms per-op p99 threshold.
- Single-shard harness: 1 cluster + 1 writer + 1 drain coordinator;
  no contention path.

## Acceptance Criteria

- [x] `tests/fixtures/phase3_tv_0862_tv6.json` exists at repo root.
- [x] JSON contains entry for `TV-6` ONLY — TV-5/7/8 stay in sibling
      fixture files (LANDED 2026-08-19).
- [x] TV-6 entry has structure:
      - `name`: "TV-6"
      - `description`: short prose of the perf invariant
      - `test_function`: `drain_throughput_1k_per_sec`
      - `budget_ms`: 1 (per-iter p99; 1000 txn/s = 1 ms/op)
      - `ci_slack_factor`: 10x (CI jitter; threshold = 10 ms)
      - `iterations`: 100 (number of drains to measure)
      - `verification_command`: exact `cargo test` invocation
- [x] TV-6 reproducible (100-iter p99 = 12µs well under 10 ms threshold).
- [x] TV-5 (c12) + TV-7 (c14) + TV-8 (c13) still pass — no regression.
- [x] All octo-sync integration tests green (262/262 incl. TV-5/6/7/8).

## Substrate landed

- `tests/fixtures/phase3_tv_0862_tv6.json` (NEW) — repo-root
  perf fixture. 1 entry: TV-6 drain throughput.
- `octo-sync/tests/phase3_tv_0862_tv6.rs` (NEW) — async gate
  test (`#[tokio::test]`) + dump test, mirrors `_tv7.rs` /
  `_tv8.rs` patterns but separate file (per R17 M3).
- `phase3_tv_0862.rs` + `_tv7.rs` + `_tv8.rs` — UNTOUCHED.

## Verification (LANDED gate)

- `cargo test -p octo-sync --test phase3_tv_0862_tv6` — 2/2 green.
  Observed: 100 submit_drain calls in 0ms total, **p99 = 12µs,
  max = 18µs** (throughput too fast to measure in wall-clock ms).
- `cargo test -p octo-sync --tests` — 262/262 green (229 + 4 + 4 + 8
  + 2 + 2 + 2 + 2 + 2 + 7 across 10 integration test binaries,
  including TV-5/7/8 regression coverage).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-sync --all-targets -- -D warnings` clean.

## Test shape (preview)

```rust
async fn tv6_drain_throughput_1k_per_sec(iterations: u32) -> Vec<u8> {
    let cluster = Cluster::new();
    let chain_id = ChainId::new("cipherocto-test").unwrap();
    let node_id = WriterNodeId([1u8; 32]);
    let election = Arc::new(RaftLikeWriterElection::new(
        node_id, cluster.clone(), chain_id.clone(),
    ));
    let coord = Arc::new(RaftLikeDrainCoordinator::new(
        cluster.clone(), chain_id.clone(), node_id,
        election.clone() as Arc<dyn WriterElection>,
    ));
    let holder = "did:octo:zHolder";
    let macaroon_id: [u8; 16] = [0xA6; 16];
    let shard_key = ShardKey::derive_canonical(holder.as_bytes());

    // Acquire leader lease (one-time setup).
    election.acquire_writer(&shard_key, 60_000).await.unwrap();

    let mut per_iter_us = Vec::with_capacity(iterations as usize);
    for i in 0..iterations {
        let t0 = Instant::now();
        let _r = coord.submit_drain(holder, &macaroon_id, 100 + i as u128)
            .await
            .unwrap_or_else(|e| panic!("submit_drain iter {i} failed: {e:?}"));
        per_iter_us.push(t0.elapsed().as_micros() as u64);
    }
    // ... pack + p99 (same pattern as TV-7/8)
}
```

## Verification (LANDED gate)

- `cargo test -p octo-sync --test phase3_tv_0862_tv6` — 2/2 green.
- `cargo test -p octo-sync --tests` — 262+/262+ green (incl. TV-5/7/8 regression).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-sync --all-targets -- -D warnings` clean.

## Key design decisions

- **Per-iter latency interpretation** (not raw throughput floor):
  budget = 1 ms per-op (1000 txn/s = 1 ms/op); per-iter p99 ceiling.
  Matches sibling TV-5/7/8 pattern; uses same `ci_slack_factor` ×
  `budget_ms` formula. Raw-throughput floor would require different
  fixture schema.
- **`ci_slack_factor = 10`** (10 ms threshold for 1 ms budget): same
  as TV-5/TV-8 (10x). CI jitter on async runtime can be higher than
  sync; 10x slack absorbs.
- **Per-TV file (R17 M3)** — separate fixture + gate test file
  from TV-5/7/8. Four sibling files now (5/6/7/8 each own file).
- **`UPDATE_PHASE3_TV=1` regen pattern** mirrors c12/c13/c14 + Phase
  1 + `goldens.rs`. Budget values are constants.

## Cross-references

- RFC-0862 v1.3.0 §Test Vectors (preview) — Phase 3 TV-6
- RFC-0862 v1.3.0 §Performance Targets — TV-6 budget 1000 txn/s per shard
- Mission `0862-c12` (TV-5) + `0862-c13` (TV-8) + `0862-c14` (TV-7) — siblings
- `octo-sync/tests/cross_instance_drain_tv.rs` — existing async drain
  harness pattern (reused for TV-6 single-shard variant)

## Out of scope (NOT this mission)

- Cross-shard drain (multi-shard throughput) — out of RFC-0862 v1.3.0
  scope; possible follow-on RFC-0862 v1.4 amendment.
- Mission 0111 (DECIMAL/DFP) — off-limits per user constraint.

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-19 | open   | Mission filed. Phase 3 TV-6 only (drain throughput ≥ 1000 txn/s per shard, per-iter latency interpretation). Async `RaftLikeDrainCoordinator::submit_drain` harness. Sibling to TV-5/7/8. Last of RFC-0862 v1.3.0 Phase 3 perf-budget TVs. |
| v1.0    | 2026-08-19 | LANDED | Fixture + async gate test (`#[tokio::test]`) + dump test landed. 1 perf-budget TV: drain throughput (1 ms p99 budget × 10x CI slack = 10 ms threshold, 100 iterations). Observed p99 = 12µs, max = 18µs across 100 calls. Single-shard harness via `RaftLikeDrainCoordinator` + `RaftLikeWriterElection` (lease acquired once upfront, 60s lease covers all iters). `DrainCoordinator` trait import required. 262/262 octo-sync integration tests + clippy + fmt green. |