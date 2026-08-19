# Mission: 0862-c14 — Phase 3 Performance Test Vector Fixture, TV-7 (RFC-0862)

## Status

**open** (2026-08-19)

## Summary

Author the perf-budget test vector fixture for **TV-7 only**
(`failover_pause_under_3s`) per RFC-0862 v1.3.0 §Performance
Targets. Sibling to `0862-c12` (TV-5) + `0862-c13` (TV-8),
both LANDED 2026-08-19. Separate fixture file per R17 M3
scope discipline — each TV owns its own fixture file.
TV-6 deferred to follow-on (requires async harness).

## Why TV-7 next (smallest viable landing unit)

- TV-7 substrate is sync — `Cluster::set_lease_duration_ms` +
  `Cluster::kill` + `Cluster::try_acquire_leader` + `Cluster::current_leader`.
  No async runtime — same pattern as TV-5 (acquire) + TV-8 (append+read).
- Budget 3s p99 = generous; lease-expiry path is sub-millisecond in current
  substrate (HashMap insert + term increment).
- Existing cluster test `failover_after_lease_expiry` (line 274) already
  exercises the core pattern — TV-7 perf-budget harness extends to N iters.
- `kill` + `revive` available (line 98/103) — failover pause can be
  measured either via lease-expiry path OR kill-switch path.

## Acceptance Criteria

- [ ] `tests/fixtures/phase3_tv_0862_tv7.json` exists at repo root.
- [ ] JSON contains entry for `TV-7` ONLY — TV-5/8 stay in sibling
      fixture files (LANDED 2026-08-19). TV-6 deferred.
- [ ] TV-7 entry has structure:
      - `name`: "TV-7"
      - `description`: short prose of the perf invariant
      - `test_function`: `failover_pause_under_3s`
      - `budget_ms`: 3000 (per RFC-0862 §Performance Targets)
      - `ci_slack_factor`: 5x (CI jitter; threshold = 15000ms)
      - `iterations`: 100 (number of trials to measure p99 over)
      - `verification_command`: exact `cargo test` invocation
- [ ] TV-7 reproducible (100-iter p99 well under 15s threshold).
- [ ] TV-5 (c12) + TV-8 (c13) still pass — no regression.
- [ ] All octo-sync integration tests green (260+ incl. TV-5/7/8).

## Substrate to land

- `tests/fixtures/phase3_tv_0862_tv7.json` (NEW) — repo-root
  perf fixture. 1 entry: TV-7 failover pause.
- `octo-sync/tests/phase3_tv_0862_tv7.rs` (NEW) — gate test
  + dump test, mirrors `phase3_tv_0862.rs` (TV-5) + `_tv8.rs`
  patterns but separate file (per R17 M3).
- `octo-sync/tests/phase3_tv_0862.rs` + `phase3_tv_0862_tv8.rs` —
  UNTOUCHED.

## Test shape (preview)

`fn tv7_failover_pause_under_3s(iterations: u32) -> Vec<u8>`:
- For i in 0..iterations:
  - `cluster = Cluster::new(); cluster.set_lease_duration_ms(0);`
  - `node_a = WriterNodeId([i;32]); node_b = WriterNodeId([i^0xFF;32]); shard = ShardKey([i;32]);`
  - `cluster.try_acquire_leader(node_a, shard, hlc_a).unwrap();` — node_a is leader
  - t0 = Instant::now()
  - `let id_b = cluster.try_acquire_leader(node_b, shard, hlc_b).unwrap();` — failover
  - assert id_b.writer_node_id == node_b + id_b.term > 1 (lease-expiry path fired)
  - per_iter_us.push(t0.elapsed().as_micros())
- p99 computation + assert under threshold

Alternative measure: kill-based failover (kill node_a, then re-acquire
from node_b). Lease-expiry version chosen for simplicity — uses only
existing `try_acquire_leader` path, mirrors the `failover_after_lease_expiry`
unit test directly.

## Verification (LANDED gate)

- `cargo test -p octo-sync --test phase3_tv_0862_tv7` — 2/2 green.
- `cargo test -p octo-sync --tests` — 260+/260+ green (incl. TV-5/8 regression).
- `cargo fmt --all -- --check` clean.
- `cargo clippy -p octo-sync --all-targets -- -D warnings` clean.

## Key design decisions

- **Per-TV file (R17 M3 discipline)** — separate fixture + gate test
  file per TV. Matches LANDED c12 + c13 pattern.
- **Lease-expiry path** (not kill-switch) — simplest substrate; uses
  only existing `try_acquire_leader` path with `lease_duration_ms = 0`.
  Mirrors existing unit test `failover_after_lease_expiry` directly.
- **`ci_slack_factor = 5`** (15 s threshold for 3 s budget): less slack
  than TV-5/8 (which use 10x) because failover pause budget is itself
  30x more generous (3s vs 100ms/3s). 5x = 15s threshold is plenty.
- **`UPDATE_PHASE3_TV=1` regen pattern** mirrors c12 + c13 + Phase 1 +
  `goldens.rs`. Budget values are constants.

## Cross-references

- RFC-0862 v1.3.0 §Test Vectors (preview) — Phase 3 TV-7
- RFC-0862 v1.3.0 §Performance Targets — TV-7 budget 3s p99
- Mission `0862-c12` (TV-5, LANDED 2026-08-19) — sibling
- Mission `0862-c13` (TV-8, LANDED 2026-08-19) — sibling

## Out of scope (NOT this mission)

- TV-6 (`drain_throughput_1k_per_sec`) — follow-on mission.
  Requires async `RaftLikeDrainCoordinator::submit_drain` harness.
- Mission 0111 (DECIMAL/DFP) — off-limits per user constraint.

## Version History

| Version | Date       | Status | Changes |
| ------- | ---------- | ------ | ------- |
| v0.1    | 2026-08-19 | open   | Mission filed. Phase 3 TV-7 only (failover pause ≤ 3s p99). Lease-expiry path. Sibling to TV-5 + TV-8. |